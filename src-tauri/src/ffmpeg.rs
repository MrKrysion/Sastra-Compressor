//! Audio and video go through the bundled ffmpeg. The arguments here are
//! the ones `ffmpeg/test.sh` checks against every accepted format.

use crate::formats::Kind;
use crate::runner::Note;
use crate::settings::Settings;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// ffmpeg.exe sits next to the app (Tauri puts it there).
pub fn exe() -> PathBuf {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Never asks for a higher sound bitrate than the source has: that makes the file
/// bigger without making it sound any better.
fn sound_bitrate(wanted: u32, source: Option<u32>) -> u32 {
    const LOWEST: u32 = 32;
    source.map_or(wanted, |kbps| wanted.min(kbps.max(LOWEST)))
}

/// `source_kbps` is the bitrate of the input's sound, if known.
pub fn args(kind: Kind, input: &Path, output: &Path, s: &Settings, source_kbps: Option<u32>) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-nostats", "-y", "-i"].map(String::from).to_vec();
    a.push(input.to_string_lossy().into_owned());
    let more: Vec<String> = match kind {
        Kind::Audio => vec![
            "-map", "0:a:0", "-vn", "-sn", "-dn", "-map_chapters", "-1",
            "-c:a", "libopus", "-b:a", &format!("{}k", sound_bitrate(s.audio_bitrate, source_kbps)), "-vbr", "on",
            "-f", "ogg",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        Kind::Video => {
            // Never wider than asked, never upscaled, and always even sizes (H.264 needs them).
            let w = if s.video_max_width > 0 { s.video_max_width.to_string() } else { "iw".into() };
            let vf = format!("scale='trunc(min({w},iw)/2)*2':-2,format=yuv420p");
            let sound = format!("{}k", sound_bitrate(128, source_kbps));
            let mut v: Vec<String> = [
                "-map", "0:v:0", "-map", "0:a:0?", "-sn", "-dn", "-map_chapters", "-1",
                "-vf", &vf, "-c:v", "libx264", "-preset", "slow", "-crf", &s.crf().to_string(),
                "-c:a", "aac", "-b:a", &sound, "-movflags", "+faststart", "-f", "mp4",
            ]
            .map(String::from)
            .to_vec();
            // A ceiling: a slower video keeps its own frame rate.
            if s.video_max_fps > 0 {
                v.extend(["-fpsmax".to_string(), s.video_max_fps.to_string()]);
            }
            v
        }
        Kind::Image => unreachable!("images don't use ffmpeg"),
    };
    a.extend(more);
    a.push(output.to_string_lossy().into_owned());
    a.extend(["-progress", "pipe:1"].map(String::from));
    a
}

/// What ffmpeg's log says about the input.
#[derive(Default, Debug)]
pub struct InputInfo {
    alpha: bool,
    hdr: bool,
    /// The log has moved on from describing the input.
    read: bool,
    video_codec: Option<String>,
    /// 8-bit 4:2:0, the only kind of picture every browser can show.
    plain_picture: bool,
    audio_codec: Option<String>,
    audio_kbps: Option<u32>,
}

impl InputInfo {
    fn read_line(&mut self, line: &str) {
        if self.read {
            return;
        }
        let t = line.trim_start();
        if t.starts_with("Stream mapping:") {
            self.read = true;
        } else if t.contains(": Video:") {
            const WITH_ALPHA: [&str; 7] = [" yuva", "rgba", "bgra", "argb", "abgr", "gbrap", " ya"];
            self.alpha |= WITH_ALPHA.iter().any(|f| t.contains(f));
            self.hdr |= t.contains("smpte2084") || t.contains("arib-std-b67");
            // Only the first of each: the streams that get used.
            if self.video_codec.is_none() {
                self.video_codec = codec_after(t, ": Video: ");
                self.plain_picture = (t.contains("yuv420p") || t.contains("yuvj420p")) && !t.contains("yuv420p1");
            }
        } else if t.contains(": Audio: ") {
            if self.audio_codec.is_none() {
                self.audio_codec = codec_after(t, ": Audio: ");
                self.audio_kbps = t.split(", ").find_map(|part| part.split_once(" kb/s")?.0.trim().parse().ok());
            }
        } else if let Some(v) = t.strip_prefix("alpha_mode") {
            self.alpha |= v.trim_start_matches([' ', ':']).starts_with('1');
        }
    }

    /// Whether browsers can play the original as it is. If so, there's no reason to keep
    /// a compressed copy that came out bigger; if not, the copy is worth it even so.
    pub fn plays_in_browsers(&self, kind: Kind, extension: &str) -> bool {
        let ext = extension.to_ascii_lowercase();
        let sound = |allowed: &[&str]| self.audio_codec.as_deref().is_some_and(|c| allowed.contains(&c));
        match kind {
            Kind::Video => {
                ["mp4", "m4v", "webm"].contains(&ext.as_str())
                    && self.plain_picture
                    && self.video_codec.as_deref().is_some_and(|c| ["h264", "vp8", "vp9", "av1"].contains(&c))
                    && (self.audio_codec.is_none() || sound(&["aac", "mp3", "opus", "vorbis", "flac"]))
            }
            Kind::Audio => {
                ["ogg", "oga", "opus", "mp3", "m4a", "aac", "wav", "flac"].contains(&ext.as_str())
                    && sound(&["vorbis", "opus", "mp3", "aac", "flac", "pcm_s16le", "pcm_s24le", "pcm_f32le", "pcm_u8"])
            }
            Kind::Image => false,
        }
    }

    pub fn audio_kbps(&self) -> Option<u32> {
        self.audio_kbps
    }

    /// What a video loses that nobody would otherwise notice until it's in the game.
    fn note(&self) -> Option<Note> {
        let text = match (self.alpha, self.hdr) {
            (true, true) => "Transparency removed (MP4 can't keep it), and HDR colours may look washed out",
            (true, false) => "Transparency removed: MP4 can't keep it",
            (false, true) => "HDR video: the colours may look washed out",
            (false, false) => return None,
        };
        Some(Note { text: text.into(), warn: true })
    }
}

fn codec_after(line: &str, label: &str) -> Option<String> {
    let rest = line.split_once(label)?.1;
    let name = rest.split([' ', ',']).next()?;
    (!name.is_empty()).then(|| name.to_string())
}

fn hidden(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // x264 takes every core, so let anything else on the PC go first.
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Reads what's in a file. With no output named, ffmpeg describes the input and stops.
pub fn inspect(input: &Path) -> InputInfo {
    let mut cmd = Command::new(exe());
    cmd.args(["-hide_banner", "-nostdin", "-i"]).arg(input).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    hidden(&mut cmd);
    let mut info = InputInfo::default();
    if let Ok(done) = cmd.output() {
        String::from_utf8_lossy(&done.stderr).lines().for_each(|l| info.read_line(l));
    }
    info
}

/// Runs one ffmpeg job. `progress` gets 0–1 as it goes.
pub fn run(
    kind: Kind,
    input: &Path,
    output: &Path,
    s: &Settings,
    info: &InputInfo,
    cancel: &Arc<AtomicBool>,
    mut progress: impl FnMut(f64),
) -> Result<Option<Note>, String> {
    let mut cmd = Command::new(exe());
    cmd.args(args(kind, input, output, s, info.audio_kbps())).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    hidden(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("Couldn't start ffmpeg: {e}"))?;
    #[cfg(windows)]
    crate::windows::tie_to_app(&child);

    // stderr: the duration (for progress) and the last lines (for errors).
    let duration = Arc::new(Mutex::new(0.0f64));
    let tail = Arc::new(Mutex::new(Vec::<String>::new()));
    let stderr = child.stderr.take().unwrap();
    let stdout = child.stdout.take().unwrap();
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

    // Watches for Cancel by itself, so stopping doesn't wait for ffmpeg's next progress line.
    let stop = cancel.clone();
    let waiter = std::thread::spawn(move || loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
        if stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            return child.wait();
        }
        std::thread::sleep(Duration::from_millis(40));
    });

    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(us) = line.strip_prefix("out_time_us=") {
            let total = *duration.lock().unwrap();
            if let (Ok(us), true) = (us.trim().parse::<f64>(), total > 0.0) {
                progress((us / 1e6 / total).clamp(0.0, 1.0));
            }
        }
    }
    let status = waiter
        .join()
        .map_err(|_| "ffmpeg stopped unexpectedly.".to_string())?
        .map_err(|e| format!("ffmpeg stopped: {e}"))?;
    let _ = err_thread.join();
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    if status.success() {
        Ok(if kind == Kind::Video { info.note() } else { None })
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
    // FFmpeg 8: "[aist#0:1/dts @ 0x…] Decoding requested, but no decoder found for: dts"
    let sound_unreadable = tail.iter().any(|l| l.contains("no decoder found") && l.contains("aist#"));
    let msg = if has("matches no streams") || has("does not contain any stream") {
        match kind {
            Kind::Audio => "This file has no sound in it.",
            _ => "This file has no video in it.",
        }
    } else if sound_unreadable && kind == Kind::Video {
        "Sastra Compressor can't read the format of this video's sound."
    } else if has("no decoder found") || has("Decoder") && has("not found") || has("Invalid data found") || has("could not find codec") {
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

    #[test]
    fn explains_a_missing_decoder() {
        let sound = ["[aist#0:1/dts @ 000001] Decoding requested, but no decoder found for: dts".to_string()];
        assert!(explain(Kind::Video, &sound).starts_with("Sastra Compressor can't read the format of this video's sound."));
        let picture = ["[vist#0:0/qtrle @ 000001] Decoding requested, but no decoder found for: qtrle".to_string()];
        assert!(explain(Kind::Video, &picture).starts_with("Sastra Compressor can't read this file's format."));
    }

    #[test]
    fn notices_what_a_video_will_lose() {
        let read = |lines: &[&str]| {
            let mut i = InputInfo::default();
            lines.iter().for_each(|l| i.read_line(l));
            (i.alpha, i.hdr)
        };
        assert_eq!(read(&["  Stream #0:0: Video: prores (4444), yuva444p12le(bt709), 640x360"]), (true, false));
        assert_eq!(read(&["  Stream #0:0: Video: vp9 (Profile 0), yuv420p(tv), 640x360", "      alpha_mode      : 1"]), (true, false));
        assert_eq!(read(&["  Stream #0:0: Video: hevc (Main 10), yuv420p10le(tv, bt2020nc/bt2020/smpte2084), 640x360"]), (false, true));
        assert_eq!(read(&["  Stream #0:0: Video: h264 (High), yuv420p(tv, bt709), 640x360"]), (false, false));
        // The output is described after the mapping, and doesn't count.
        assert_eq!(read(&["Stream mapping:", "  Stream #0:0: Video: png, rgba, 640x360"]), (false, false));
    }

    fn info(lines: &[&str]) -> InputInfo {
        let mut i = InputInfo::default();
        lines.iter().for_each(|l| i.read_line(l));
        i
    }

    #[test]
    fn reads_the_first_streams_codecs_and_sound_bitrate() {
        let i = info(&[
            "  Stream #0:0(und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(tv, bt709), 640x360, 744 kb/s, 24 fps",
            "  Stream #0:1(und): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz, mono, fltp, 64 kb/s (default)",
            "  Stream #0:2(und): Audio: ac3, 48000 Hz, 5.1, fltp, 384 kb/s",
        ]);
        assert_eq!(i.video_codec.as_deref(), Some("h264"));
        assert_eq!(i.audio_codec.as_deref(), Some("aac"));
        assert_eq!(i.audio_kbps(), Some(64));
    }

    #[test]
    fn knows_what_browsers_can_play() {
        let web = info(&["Stream #0:0: Video: h264 (High), yuv420p(tv), 640x360", "Stream #0:1: Audio: aac (LC), 48000 Hz, stereo, fltp, 128 kb/s"]);
        assert!(web.plays_in_browsers(Kind::Video, "MP4"));
        assert!(!web.plays_in_browsers(Kind::Video, "avi"), "wrong container");
        assert!(!info(&["Stream #0:0: Video: wmv2, yuv420p, 640x360"]).plays_in_browsers(Kind::Video, "mp4"), "wrong codec");
        assert!(!info(&["Stream #0:0: Video: hevc (Main), yuv420p(tv), 640x360"]).plays_in_browsers(Kind::Video, "mp4"), "HEVC isn't universal");
        assert!(!info(&["Stream #0:0: Video: h264 (High 10), yuv420p10le(tv), 640x360"]).plays_in_browsers(Kind::Video, "mp4"), "10-bit");
        assert!(!info(&["Stream #0:0: Video: h264, yuv420p, 640x360", "Stream #0:1: Audio: dts, 48000 Hz, stereo, fltp, 1411 kb/s"]).plays_in_browsers(Kind::Video, "mp4"), "sound");
        assert!(info(&["Stream #0:0: Video: vp9 (Profile 0), yuv420p(tv), 640x360"]).plays_in_browsers(Kind::Video, "webm"), "no sound at all is fine");
        assert!(info(&["Stream #0:0: Audio: mp3 (mp3float), 44100 Hz, stereo, fltp, 64 kb/s"]).plays_in_browsers(Kind::Audio, "mp3"));
        assert!(!info(&["Stream #0:0: Audio: adpcm_ms, 48000 Hz, mono, s16, 128 kb/s"]).plays_in_browsers(Kind::Audio, "wav"));
        assert!(!info(&["Stream #0:0: Audio: pcm_s16be, 44100 Hz, stereo, s16, 1411 kb/s"]).plays_in_browsers(Kind::Audio, "aiff"));
    }

    #[test]
    fn never_raises_the_sound_bitrate() {
        assert_eq!(sound_bitrate(128, Some(64)), 64);
        assert_eq!(sound_bitrate(128, Some(320)), 128);
        assert_eq!(sound_bitrate(128, None), 128);
        assert_eq!(sound_bitrate(96, Some(8)), 32, "not so low that it falls apart");
    }
}
