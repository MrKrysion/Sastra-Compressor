//! Works through the list: images and audio side by side (one per core),
//! then videos one at a time (x264 already uses every core).
//! Each file is written to a temporary name and renamed when it is done,
//! so a cancelled or failed file never leaves half a file behind.

use crate::formats::Kind;
use crate::settings::{plan, Job, SaveMode, Settings};
use crate::{ffmpeg, image_job};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

// `rename_all` names the variants ("running"), `rename_all_fields` their fields ("outSize").
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Update {
    Running { id: String, progress: f64 },
    Done { id: String, out_path: String, out_size: u64 },
    Failed { id: String, error: String },
    Cancelled { id: String },
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Finished {
    pub cancelled: bool,
}

pub fn run(app: AppHandle, jobs: Vec<Job>, settings: Settings, cancel: Arc<AtomicBool>, busy: Arc<AtomicBool>) {
    let emit = |u: Update| {
        let _ = app.emit("job", u);
    };
    work(jobs, &settings, &cancel, &emit);
    busy.store(false, Ordering::SeqCst);
    let _ = app.emit("finished", Finished { cancelled: cancel.load(Ordering::SeqCst) });
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
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some((job, out)) = light.get(i) else { break };
                one(job, out, settings, cancel, emit);
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

    let result = result.and_then(|()| {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        if s.save_mode == SaveMode::Replace {
            // The original goes to the Recycle Bin, so it can still be brought back.
            trash::delete(input).map_err(|e| format!("Couldn't move the original to the Recycle Bin, so it was kept.\n{e}"))?;
        }
        std::fs::rename(&part, output).map_err(|e| format!("Couldn't save {}.\n{e}", output.display()))
    });
    match result {
        Ok(()) => {
            let out_size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
            emit(Update::Done { id, out_path: output.to_string_lossy().into_owned(), out_size });
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

/// `school.webp` is written as `school.webp.part` until it is finished.
fn part_path(output: &Path) -> PathBuf {
    let mut name = output.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    output.with_file_name(name)
}
