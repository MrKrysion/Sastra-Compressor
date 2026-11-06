//! Works through the list: images and audio side by side (one per core),
//! then videos one at a time (x264 already uses every core).
//! Each file is written to a temporary name and renamed when it is done,
//! so a cancelled or failed file never leaves half a file behind.

use crate::formats::Kind;
use crate::settings::{key, plan, Job, SaveMode, Settings};
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
    /// `kept`: replacing, but the result wasn't smaller, so the original stayed.
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

    let result = match job.kind {
        Kind::Image => image_job::compress(input, &part, s.webp_quality(), s.image_max_width),
        kind => {
            // ffmpeg reports often; pass on at most ten updates a second.
            let mut last = Instant::now() - Duration::from_secs(1);
            ffmpeg::run(kind, input, &part, s, cancel, |p| {
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
        finish(input, &part, output, s).map(|kept| (kept, note))
    });
    match result {
        Ok((kept, note)) => {
            let saved = if kept { input } else { output };
            let out_size = std::fs::metadata(saved).map(|m| m.len()).unwrap_or(0);
            emit(Update::Done { id, out_path: saved.to_string_lossy().into_owned(), out_size, kept, note });
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

/// Puts the finished file in place. Returns true if the original stayed instead.
fn finish(input: &Path, part: &Path, output: &Path, s: &Settings) -> Result<bool, String> {
    let save = || std::fs::rename(part, output).map_err(|e| format!("Couldn't save {}.\n{e}", output.display()));
    if s.save_mode != SaveMode::Replace {
        return save().map(|()| false);
    }
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    // Replacing a file with a bigger one would only lose quality and space.
    if size(part) >= size(input) {
        let _ = std::fs::remove_file(part);
        return Ok(true);
    }
    if key(input) == key(output) {
        // The same name: the original has to leave first.
        dispose(input, s)?;
        save().map_err(|e| format!("{e}\nThe original is in the Recycle Bin.")).map(|()| false)
    } else {
        // Different names: save first, so a failure here can't lose anything.
        save()?;
        dispose(input, s)
            .map_err(|e| format!("The compressed file was saved, but the original stayed where it was.\n{e}"))
            .map(|()| false)
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
