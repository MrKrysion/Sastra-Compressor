//! Audio and video go through the bundled ffmpeg. The arguments here are
//! the ones `ffmpeg/test.sh` checks against every accepted format.

use crate::formats::Kind;
use crate::settings::Settings;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// ffmpeg.exe sits next to the app (Tauri puts it there).
pub fn exe() -> PathBuf {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(name))
}

pub fn args(kind: Kind, input: &Path, output: &Path, s: &Settings) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-nostats", "-y", "-i"].map(String::from).to_vec();
    a.push(input.to_string_lossy().into_owned());
    let more: Vec<String> = match kind {
        Kind::Audio => vec![
            "-map", "0:a:0", "-vn", "-sn", "-dn", "-map_chapters", "-1",
            "-c:a", "libopus", "-b:a", &format!("{}k", s.audio_bitrate), "-vbr", "on",
            "-f", "ogg",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        Kind::Video => {
            // Never wider than asked, never upscaled, and always even sizes (H.264 needs them).
            let w = if s.video_max_width > 0 { s.video_max_width.to_string() } else { "iw".into() };
            let vf = format!("scale='trunc(min({w},iw)/2)*2':-2,format=yuv420p");
            vec![
                "-map", "0:v:0", "-map", "0:a:0?", "-sn", "-dn", "-map_chapters", "-1",
                "-vf", &vf, "-c:v", "libx264", "-preset", "slow", "-crf", &s.crf().to_string(),
                "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart", "-f", "mp4",
            ]
            .into_iter()
            .map(String::from)
            .collect()
        }
        Kind::Image => unreachable!("images don't use ffmpeg"),
    };
    a.extend(more);
    a.push(output.to_string_lossy().into_owned());
    a.extend(["-progress", "pipe:1"].map(String::from));
    a
}

/// Runs one ffmpeg job. `progress` gets 0–1 as it goes.
pub fn run(
    kind: Kind,
    input: &Path,
    output: &Path,
    s: &Settings,
    cancel: &Arc<AtomicBool>,
    mut progress: impl FnMut(f64),
) -> Result<(), String> {
    let mut cmd = Command::new(exe());
    cmd.args(args(kind, input, output, s)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("Couldn't start ffmpeg: {e}"))?;

    // stderr: the duration (for progress) and the last lines (for errors).
    let duration = Arc::new(Mutex::new(0.0f64));
    let tail = Arc::new(Mutex::new(Vec::<String>::new()));
    let stderr = child.stderr.take().unwrap();
    let (d2, t2) = (duration.clone(), tail.clone());
    let err_thread = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Some(rest) = line.trim_start().strip_prefix("Duration: ") {
                let mut d = d2.lock().unwrap();
                if *d == 0.0 {
                    *d = parse_time(rest.split(',').next().unwrap_or("")).unwrap_or(0.0);
                }
            }
            let mut t = t2.lock().unwrap();
            t.push(line);
            if t.len() > 40 {
                t.remove(0);
            }
        }
    });

    let stdout = child.stdout.take().unwrap();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            break;
        }
        if let Some(us) = line.strip_prefix("out_time_us=") {
            let total = *duration.lock().unwrap();
            if let (Ok(us), true) = (us.trim().parse::<f64>(), total > 0.0) {
                progress((us / 1e6 / total).clamp(0.0, 1.0));
            }
        }
    }
    let status = child.wait().map_err(|e| format!("ffmpeg stopped: {e}"))?;
    let _ = err_thread.join();
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    if status.success() {
        Ok(())
    } else {
        Err(explain(kind, &tail.lock().unwrap()))
    }
}

fn parse_time(t: &str) -> Option<f64> {
    let mut parts = t.trim().split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let s: f64 = parts.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

/// Turns ffmpeg's last words into a sentence a person can act on.
fn explain(kind: Kind, tail: &[String]) -> String {
    let all = tail.join("\n");
    let has = |s: &str| all.contains(s);
    let last = tail
        .iter()
        .rev()
        .find(|l| {
            let t = l.trim();
            !t.is_empty() && !l.starts_with("  ") && t != "Conversion failed!" && !t.starts_with("[out#") && !t.starts_with("Error opening output")
        })
        .cloned()
        .unwrap_or_default();
    let msg = if has("matches no streams") || has("does not contain any stream") {
        match kind {
            Kind::Audio => "This file has no sound in it.",
            _ => "This file has no video in it.",
        }
    } else if has("Decoder") && has("not found") || has("Invalid data found") || has("could not find codec") {
        "Sastra Compressor can't read this file's format."
    } else if has("Permission denied") {
        "Couldn't save: Windows didn't allow writing to that folder."
    } else if has("No space left") {
        "Couldn't save: the drive is full."
    } else {
        "Couldn't compress this file."
    };
    if last.is_empty() {
        msg.into()
    } else {
        format!("{msg}\n{last}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ffmpeg_durations() {
        assert_eq!(parse_time("00:01:02.50"), Some(62.5));
        assert_eq!(parse_time("N/A"), None);
    }
}
