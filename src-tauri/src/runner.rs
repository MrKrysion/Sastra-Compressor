//! Works through the list: images and audio side by side (one per core),
//! then videos one at a time (x264 already uses every core).
//! Each file is written to a temporary name and renamed when it is done,
//! so a cancelled or failed file never leaves half a file behind.

use crate::formats::Kind;
use crate::settings::{key, numbered, plan, Job, SaveMode, Settings};
use crate::{ffmpeg, image_job};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// A name nothing else uses, so leftovers can be recognised and removed safely.
pub const PART_EXT: &str = ".sastra-part";

/// Something worth saying about a finished file. `warn` marks something lost.
#[derive(Serialize, Clone, Debug)]
pub struct Note {
    pub text: String,
    pub warn: bool,
}

// `rename_all` names the variants ("running"), `rename_all_fields` their fields ("outSize").
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Update {
    Running { id: String, progress: f64 },
    /// `kept`: the result wasn't smaller, so the original was kept instead.
    Done { id: String, out_path: String, out_size: u64, kept: bool, note: Option<Note> },
    Failed { id: String, error: String },
    Cancelled { id: String },
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Finished {
    pub cancelled: bool,
}

pub fn run(app: AppHandle, jobs: Vec<Job>, settings: Settings, cancel: Arc<AtomicBool>, busy: Arc<AtomicBool>, journal: Option<PathBuf>) {
    // Lists this run's temporary files, so the next start can remove any that a crash left behind.
    if let Some(j) = &journal {
        let parts: Vec<String> = plan(&jobs, &settings).iter().map(|o| part_path(o).to_string_lossy().into_owned()).collect();
        let _ = std::fs::write(j, parts.join("\n"));
    }
    let emit = |u: Update| {
        let _ = app.emit("job", u);
    };
    work(jobs, &settings, &cancel, &emit);
    if let Some(j) = &journal {
        let _ = std::fs::remove_file(j);
    }
    busy.store(false, Ordering::SeqCst);
    let _ = app.emit("finished", Finished { cancelled: cancel.load(Ordering::SeqCst) });
}

/// Removes what a crashed or killed run left behind.
pub fn sweep(journal: &Path) {
    let Ok(list) = std::fs::read_to_string(journal) else { return };
    for line in list.lines().filter(|l| l.ends_with(PART_EXT)) {
        let _ = std::fs::remove_file(line);
    }
    let _ = std::fs::remove_file(journal);
}

/// The work itself, without the app around it (so it can be tested).
pub fn work(jobs: Vec<Job>, settings: &Settings, cancel: &Arc<AtomicBool>, emit: &(impl Fn(Update) + Sync)) {
    let outputs = plan(&jobs, settings);
    let all: Vec<(Job, PathBuf)> = jobs.into_iter().zip(outputs).collect();
    let (videos, light): (Vec<_>, Vec<_>) = all.into_iter().partition(|(j, _)| j.kind == Kind::Video);

    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..cores.min(light.len()) {
            scope.spawn(|| {
                // Every core is busy, so leave room for whatever else is running on the PC.
                #[cfg(windows)]
                crate::windows::lower_this_thread();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((job, out)) = light.get(i) else { break };
                    one(job, out, settings, cancel, emit);
                }
            });
        }
    });
    for (job, out) in &videos {
        one(job, out, settings, cancel, emit);
    }
}

fn one(job: &Job, output: &Path, s: &Settings, cancel: &Arc<AtomicBool>, emit: &impl Fn(Update)) {
    let id = job.id.clone();
    if cancel.load(Ordering::Relaxed) {
        emit(Update::Cancelled { id });
        return;
    }
    emit(Update::Running { id: id.clone(), progress: 0.0 });

    let input = Path::new(&job.path);
    if !input.is_file() {
        emit(Update::Failed { id, error: "The file is no longer there. It may have been moved or deleted.".into() });
        return;
    }
    // The interface asks first; this is the backstop, before any time is spent compressing.
    if s.save_mode == SaveMode::Replace && !s.delete_for_good && !can_recycle(input) {
        emit(Update::Failed { id, error: NO_BIN.into() });
        return;
    }
    if let Some(dir) = output.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            emit(Update::Failed { id, error: format!("Couldn't make the folder {}.\n{e}", dir.display()) });
            return;
        }
    }
    let part = part_path(output);

    let extension = input.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let mut plays_in_browsers = ["webp", "jpg", "jpeg", "png", "gif"].contains(&extension.to_ascii_lowercase().as_str());
    let result = match job.kind {
        Kind::Image => image_job::compress(input, &part, s.webp_quality(), s.image_max_width),
        kind => {
            let info = ffmpeg::inspect(input);
            plays_in_browsers = info.plays_in_browsers(kind, &extension);
            // ffmpeg reports often; pass on at most ten updates a second.
            let mut last = Instant::now() - Duration::from_secs(1);
            ffmpeg::run(kind, input, &part, s, &info, cancel, |p| {
                if last.elapsed() >= Duration::from_millis(100) {
                    last = Instant::now();
                    emit(Update::Running { id: id.clone(), progress: p });
                }
            })
        }
    };

    let result = result.and_then(|note| {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        finish(input, &part, output, s, plays_in_browsers).map(|saved| (saved, note))
    });
    match result {
        Ok((saved, note)) => {
            let out_size = std::fs::metadata(saved.path()).map(|m| m.len()).unwrap_or(0);
            let (kept, note) = match &saved {
                Saved::Compressed(_) => (false, note),
                Saved::KeptOriginal(_) => (true, None),
                Saved::CopiedOriginal(_) => (true, Some(Note { text: "Already small, so the original was copied".into(), warn: false })),
            };
            emit(Update::Done { id, out_path: saved.path().to_string_lossy().into_owned(), out_size, kept, note });
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            if cancel.load(Ordering::Relaxed) {
                emit(Update::Cancelled { id });
            } else {
                emit(Update::Failed { id, error: e });
            }
        }
    }
}

const NO_BIN: &str = "This drive has no Recycle Bin, so the original was kept and nothing was replaced.";

/// Where a finished job's file ended up.
enum Saved {
    Compressed(PathBuf),
    /// Nothing new was made: the original is the file to use.
    KeptOriginal(PathBuf),
    /// The original, copied to the chosen folder.
    CopiedOriginal(PathBuf),
}

impl Saved {
    fn path(&self) -> &Path {
        match self {
            Saved::Compressed(p) | Saved::KeptOriginal(p) | Saved::CopiedOriginal(p) => p,
        }
    }
}

/// Puts the finished file in place. A result that isn't smaller is only worth having
/// if browsers can't play the original; otherwise it just loses quality and space.
fn finish(input: &Path, part: &Path, output: &Path, s: &Settings, plays_in_browsers: bool) -> Result<Saved, String> {
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    if plays_in_browsers && size(part) >= size(input) {
        return keep_original(input, part, output, s);
    }
    let save = || std::fs::rename(part, output).map_err(|e| format!("Couldn't save {}.\n{e}", output.display()));
    let done = || Saved::Compressed(output.to_path_buf());
    if s.save_mode != SaveMode::Replace {
        return save().map(|()| done());
    }
    if key(input) == key(output) {
        // The same name: the original has to leave first.
        dispose(input, s)?;
        save().map_err(|e| format!("{e}\nThe original is in the Recycle Bin.")).map(|()| done())
    } else {
        // Different names: save first, so a failure here can't lose anything.
        save()?;
        dispose(input, s)
            .map_err(|e| format!("The compressed file was saved, but the original stayed where it was.\n{e}"))
            .map(|()| done())
    }
}

/// Throws the bigger result away. A chosen folder still gets a copy of the original, under
/// its own name and type, so that the folder ends up complete.
fn keep_original(input: &Path, part: &Path, output: &Path, s: &Settings) -> Result<Saved, String> {
    let _ = std::fs::remove_file(part);
    let kept = || Saved::KeptOriginal(input.to_path_buf());
    if s.save_mode != SaveMode::Folder {
        return Ok(kept());
    }
    let wanted = match input.extension() {
        Some(ext) => output.with_extension(ext),
        None => output.with_extension(""),
    };
    if key(&wanted) == key(input) {
        return Ok(kept());
    }
    // Never over someone else's file. The same file again, from an earlier run, is fine.
    let mut target = wanted.clone();
    let mut n = 1;
    while target.exists() && !same_contents(input, &target) {
        n += 1;
        target = numbered(&wanted, n);
    }
    if !target.exists() {
        std::fs::copy(input, part).map_err(|e| format!("Couldn't copy the original to {}.\n{e}", target.display()))?;
        std::fs::rename(part, &target).map_err(|e| format!("Couldn't save {}.\n{e}", target.display()))?;
    }
    Ok(Saved::CopiedOriginal(target))
}

fn same_contents(a: &Path, b: &Path) -> bool {
    use std::io::Read;
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else { return false };
    if ma.len() != mb.len() {
        return false;
    }
    let (Ok(fa), Ok(fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else { return false };
    let (mut ra, mut rb) = (std::io::BufReader::new(fa), std::io::BufReader::new(fb));
    let (mut ba, mut bb) = (vec![0u8; 65536], vec![0u8; 65536]);
    loop {
        let Ok(n) = ra.read(&mut ba) else { return false };
        if n == 0 {
            return true;
        }
        if rb.read_exact(&mut bb[..n]).is_err() || ba[..n] != bb[..n] {
            return false;
        }
    }
}

/// Sends the original to the Recycle Bin, or deletes it if the person agreed to that.
fn dispose(input: &Path, s: &Settings) -> Result<(), String> {
    if can_recycle(input) {
        return trash::delete(input).map_err(|e| format!("Couldn't move the original to the Recycle Bin, so it was kept.\n{e}"));
    }
    if !s.delete_for_good {
        return Err(NO_BIN.into());
    }
    std::fs::remove_file(input).map_err(|e| format!("Couldn't delete the original, so it was kept.\n{e}"))
}

/// Windows deletes a file for good, without a word, when the Recycle Bin can't take it.
pub fn can_recycle(path: &Path) -> bool {
    #[cfg(windows)]
    {
        crate::windows::can_recycle(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        true
    }
}

/// `school.webp` is written as `school.webp.sastra-part` until it is finished.
fn part_path(output: &Path) -> PathBuf {
    let mut name = output.file_name().unwrap_or_default().to_os_string();
    name.push(PART_EXT);
    output.with_file_name(name)
}
