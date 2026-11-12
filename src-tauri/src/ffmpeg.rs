//! Audio and video go through the bundled ffmpeg: VP9 and Opus in WebM, and Opus
//! in Ogg. The arguments here are the ones `ffmpeg/test.sh` checks against every
//! accepted format.
//!
//! ffmpeg has no decoders for the patented formats (H.264, HEVC, AAC, E-AC-3). When
//! a file needs one, the operating system decodes it (`crate::decode`): the pictures
//! come in on ffmpeg's stdin and the sound as a WAV file. A build of ffmpeg that can
//! decode them itself, such as the system's own on Linux, reads them directly.

use crate::decode::{self, Failure, Frames, VideoInfo};
use crate::formats::Kind;
use crate::platform;
use crate::runner::Note;
use crate::settings::Settings;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, OnceLock};
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

/// Whether the bundled ffmpeg has a decoder by this name.
fn decodes(codec: &str) -> bool {
    static LIST: OnceLock<String> = OnceLock::new();
    let list = LIST.get_or_init(|| {
        let mut cmd = Command::new(exe());
        cmd.args(["-hide_banner", "-decoders"]).stdin(Stdio::null()).stderr(Stdio::null());
        platform::background(&mut cmd);
        cmd.output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
    });
    list.lines().any(|l| l.split_whitespace().nth(1) == Some(codec))
}

/// Whether the operating system has to decode this codec instead of ffmpeg.
fn needs_os(codec: Option<&str>, os: &[&str]) -> bool {
    codec.is_some_and(|c| os.contains(&c) && !decodes(c))
}

/// libopus takes only these channel layouts; anything else, such as 5.1 (side), is
/// converted to the nearest one instead of failing.
const OPUS_LAYOUTS: &str = "aformat=channel_layouts=mono|stereo|3.0|quad|5.0|5.1|6.1|7.1";

/// Never asks for a higher sound bitrate than the source has: that makes the file
/// bigger without making it sound any better. `wanted` is for stereo; surround gets
/// it for each pair of speakers.
fn sound_bitrate(wanted: u32, channels: Option<u32>, source: Option<u32>) -> u32 {
    const LOWEST: u32 = 32;
    let wanted = wanted * channels.map_or(1, |c| (c / 2).max(1));
    source.map_or(wanted, |kbps| wanted.min(kbps.max(LOWEST)))
}

/// Where ffmpeg takes the pictures and the sound from, when not all from the file.
#[derive(Default)]
pub struct Sources<'a> {
    /// Pictures the operating system pipes in.
    pub frames: Option<&'a VideoInfo>,
    /// Sound the operating system decoded to this WAV.
    pub wav: Option<&'a Path>,
    /// A video's sound is left out: nothing here can read it.
    pub mute: bool,
}

pub fn args(kind: Kind, input: &Path, src: &Sources, output: &Path, s: &Settings, info: &InputInfo) -> Vec<String> {
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let mut a: Vec<String> = ["-hide_banner", "-nostats", "-y"].map(String::from).to_vec();
    match src.frames {
        Some(f) => a.extend(decode::input_args(f)),
        None => a.extend(["-nostdin".into(), "-i".into(), path(input)]),
    }
    // The sound's input: the file itself (0), or a second input (1).
    let mut sound = 0;
    if let Some(wav) = src.wav {
        // The WAV may be over 4 GB, past what its header can state.
        a.extend(["-ignore_length".into(), "1".into(), "-i".into(), path(wav)]);
        sound = 1;
    } else if src.frames.is_some() && !src.mute {
        a.extend(["-i".into(), path(input)]);
        sound = 1;
    }
    let opus = |wanted: u32| {
        let kbps = sound_bitrate(wanted, info.audio_channels, info.audio_kbps);
        ["-af", OPUS_LAYOUTS, "-c:a", "libopus", "-b:a", &format!("{kbps}k"), "-vbr", "on"].map(String::from)
    };
    match kind {
        Kind::Audio => {
            a.extend(["-map", &format!("{sound}:a:0"), "-vn", "-sn", "-dn", "-map_chapters", "-1"].map(String::from));
            a.extend(opus(s.audio_bitrate));
            a.extend(["-f", "ogg"].map(String::from));
        }
        Kind::Video => {
            a.extend(["-map", "0:v:0"].map(String::from));
            if !src.mute {
                a.extend(["-map".into(), format!("{sound}:a:0?")]);
            }
            // Upright first, so the width limit applies to the picture as it's shown.
            // Never wider than asked, never upscaled, and always even sizes.
            let w = if s.video_max_width > 0 { s.video_max_width.to_string() } else { "iw".into() };
            let turn = src.frames.map_or("", |f| decode::upright(f.rotation));
            let vf = format!("{turn}scale='trunc(min({w},iw)/2)*2':-2,format=yuv420p");
            a.extend(["-sn", "-dn", "-map_chapters", "-1", "-vf", &vf].map(String::from));
            // -b:v 0 makes the CRF a pure quality target; speed 2 of libvpx's "good" quality.
            a.extend(["-c:v", "libvpx-vp9", "-crf", &s.crf().to_string(), "-b:v", "0", "-deadline", "good", "-cpu-used", "2", "-row-mt", "1"].map(String::from));
            a.extend(opus(96));
            // A ceiling: a slower video keeps its own frame rate.
            if s.video_max_fps > 0 {
                a.extend(["-fpsmax".to_string(), s.video_max_fps.to_string()]);
            }
            // The index at the front, so a browser can start playing and seek before it has the whole file.
            a.extend(["-cues_to_front", "1", "-f", "webm"].map(String::from));
        }
        Kind::Image => unreachable!("images don't use ffmpeg"),
    }
    a.push(path(output));
    a.extend(["-progress", "pipe:1"].map(String::from));
    a
}

/// A temporary WAV for decoded sound, removed when dropped.
struct TempWav(PathBuf);

const SOUND_PREFIX: &str = "sastra-sound-";

impl TempWav {
    fn new() -> TempWav {
        static N: AtomicUsize = AtomicUsize::new(0);
        let name = format!("{SOUND_PREFIX}{}-{}.wav", std::process::id(), N.fetch_add(1, Ordering::Relaxed));
        TempWav(std::env::temp_dir().join(name))
    }
}

impl Drop for TempWav {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Removes decoded sound that a crashed run left in the temporary folder.
pub fn sweep_sound() {
    let Ok(dir) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for e in dir.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with(SOUND_PREFIX) && name.ends_with(".wav") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Compresses one audio or video file. `progress` gets 0–1 as it goes.
pub fn compress(
    kind: Kind,
    input: &Path,
    output: &Path,
    s: &Settings,
    info: &InputInfo,
    cancel: &Arc<AtomicBool>,
    progress: impl FnMut(f64),
) -> Result<Option<Note>, String> {
    let mut notes: Vec<String> = Vec::new();
    let video_codec = info.video_codec.as_deref();
    let frames = if kind == Kind::Video && needs_os(video_codec, decode::OS_VIDEO) {
        Some(platform::open_video(input).map_err(|f| f.explain(video_codec.unwrap_or_default()))?)
    } else {
        None
    };
    let wav = TempWav::new();
    let (mut use_wav, mut mute) = (false, false);
    let audio_codec = info.audio_codec.as_deref();
    if needs_os(audio_codec, decode::OS_SOUND) {
        match platform::read_sound(input, &wav.0, cancel) {
            Ok(had) => (use_wav, mute) = (had, !had),
            Err(Failure::Other(e)) if e == "cancelled" => return Err(e),
            // One unreadable sound track shouldn't cost the whole video.
            Err(f) if kind == Kind::Video => {
                mute = true;
                notes.push(match f {
                    Failure::NoDecoder => "Sound left out: this computer can't read its format".into(),
                    Failure::Other(e) => format!("Sound left out: {e}"),
                });
            }
            Err(f) => return Err(f.explain(audio_codec.unwrap_or_default())),
        }
    }
    let picture = frames.as_ref().map(|f| f.info().clone());
    let src = Sources { frames: picture.as_ref(), wav: use_wav.then_some(wav.0.as_path()), mute };
    let known = picture.as_ref().and_then(|p| p.duration);
    run(kind, &args(kind, input, &src, output, s, info), frames, known, cancel, progress)?;
    if kind == Kind::Video {
        notes.extend(info.note());
    }
    Ok((!notes.is_empty()).then(|| Note { text: notes.join(". "), warn: true }))
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
    audio_channels: Option<u32>,
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
                let parts: Vec<&str> = t.split(", ").collect();
                self.audio_kbps = parts.iter().find_map(|part| part.split_once(" kb/s")?.0.trim().parse().ok());
                // The layout follows the sample rate: "48000 Hz, 5.1(side), fltp".
                self.audio_channels = parts.iter().position(|p| p.ends_with(" Hz")).and_then(|i| parts.get(i + 1)).and_then(|l| channels(l));
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

    #[cfg(test)]
    pub fn audio_kbps(&self) -> Option<u32> {
        self.audio_kbps
    }

    /// What a video loses that nobody would otherwise notice until it's in the game.
    fn note(&self) -> Option<String> {
        let text = match (self.alpha, self.hdr) {
            (true, true) => "Transparency removed, and HDR colours may look washed out",
            (true, false) => "Transparency removed",
            (false, true) => "HDR video: the colours may look washed out",
            (false, false) => return None,
        };
        Some(text.into())
    }
}

/// How many channels a layout in ffmpeg's log has: "stereo", "5.1(side)", "3 channels".
fn channels(layout: &str) -> Option<u32> {
    let name = layout.split('(').next()?.trim();
    Some(match name {
        "mono" => 1,
        "stereo" | "downmix" => 2,
        "2.1" | "3.0" => 3,
        "quad" | "3.1" | "4.0" => 4,
        "4.1" | "5.0" => 5,
        "5.1" | "6.0" | "hexagonal" => 6,
        "6.1" | "7.0" => 7,
        "7.1" | "octagonal" => 8,
        n => n.strip_suffix(" channels")?.parse().ok()?,
    })
}

fn codec_after(line: &str, label: &str) -> Option<String> {
    let rest = line.split_once(label)?.1;
    let name = rest.split([' ', ',']).next()?;
    (!name.is_empty()).then(|| name.to_string())
}

/// Reads what's in a file. With no output named, ffmpeg describes the input and stops.
pub fn inspect(input: &Path) -> InputInfo {
    let mut cmd = Command::new(exe());
    cmd.args(["-hide_banner", "-nostdin", "-i"]).arg(input).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    platform::background(&mut cmd);
    let mut info = InputInfo::default();
    if let Ok(done) = cmd.output() {
        String::from_utf8_lossy(&done.stderr).lines().for_each(|l| info.read_line(l));
    }
    // Without its decoder, ffmpeg can't tell an H.264 video's pixel format; the OS can.
    if info.video_codec.as_deref() == Some("h264") && !info.plain_picture && !decodes("h264") {
        info.plain_picture = platform::plain_h264(input) == Some(true);
    }
    info
}

/// Feeds the piped pictures to ffmpeg, passing on its progress between pictures:
/// the pictures are written on the thread that decodes them, which is the one
/// that reports progress.
struct Feed<'a, F: FnMut(f64)> {
    stdin: ChildStdin,
    times: &'a Receiver<f64>,
    tick: &'a mut F,
}

impl<F: FnMut(f64)> Write for Feed<'_, F> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.times.try_iter().for_each(&mut *self.tick);
        self.stdin.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stdin.flush()
    }
}

/// Runs one ffmpeg job, piping `frames` into it when there are any. `known` is the
/// duration when the input can't tell ffmpeg (raw pictures have none).
fn run(
    kind: Kind,
    args: &[String],
    frames: Option<Box<dyn Frames>>,
    known: Option<f64>,
    cancel: &Arc<AtomicBool>,
    mut progress: impl FnMut(f64),
) -> Result<(), String> {
    let mut cmd = Command::new(exe());
    let stdin = if frames.is_some() { Stdio::piped() } else { Stdio::null() };
    cmd.args(args).stdin(stdin).stdout(Stdio::piped()).stderr(Stdio::piped());
    platform::background(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("Couldn't start ffmpeg: {e}"))?;
    platform::tie_to_app(&child);

    // stderr: the duration (for progress) and the last lines (for errors).
    let duration = Arc::new(Mutex::new(known.unwrap_or(0.0)));
    let tail = Arc::new(Mutex::new(Vec::<String>::new()));
    let stderr = child.stderr.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let pipe = child.stdin.take();
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
    // stdout: how far ffmpeg has got, in seconds.
    let (send, times) = channel();
    let out_thread = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(us) = line.strip_prefix("out_time_us=").and_then(|us| us.trim().parse::<f64>().ok()) {
                let _ = send.send(us / 1e6);
            }
        }
    });

    // Watches for Cancel (or a decoding failure) by itself, so stopping doesn't wait for ffmpeg.
    let abort = Arc::new(AtomicBool::new(false));
    let (stop, stop2) = (cancel.clone(), abort.clone());
    let waiter = std::thread::spawn(move || loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
        if stop.load(Ordering::Relaxed) || stop2.load(Ordering::Relaxed) {
            let _ = child.kill();
            return child.wait();
        }
        std::thread::sleep(Duration::from_millis(40));
    });

    let mut tick = |seconds: f64| {
        let total = *duration.lock().unwrap();
        if total > 0.0 {
            progress((seconds / total).clamp(0.0, 1.0));
        }
    };
    let mut decoding = Ok(());
    if let (Some(mut frames), Some(stdin)) = (frames, pipe) {
        let mut feed = Feed { stdin, times: &times, tick: &mut tick };
        decoding = decode::pump(frames.as_mut(), &mut feed, cancel);
        // Dropping the feed closes ffmpeg's stdin: the end of the video.
        drop(feed);
        if decoding.is_err() {
            abort.store(true, Ordering::Relaxed);
        }
    }
    times.iter().for_each(&mut tick);
    let status = waiter
        .join()
        .map_err(|_| "ffmpeg stopped unexpectedly.".to_string())?
        .map_err(|e| format!("ffmpeg stopped: {e}"))?;
    let _ = (err_thread.join(), out_thread.join());
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    // ffmpeg's own error explains a broken pipe better than the pipe does.
    match decoding {
        Err(Failure::Other(e)) if !e.starts_with("ffmpeg stopped") => return Err(e),
        Err(Failure::NoDecoder) => return Err(Failure::NoDecoder.explain("")),
        _ => {}
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
    // FFmpeg 8: "[aist#0:1/dts @ 0x…] Decoding requested, but no decoder found for: dts"
    let sound_unreadable = tail.iter().any(|l| l.contains("no decoder found") && l.contains("aist#"));
    let msg = if has("matches no streams") || has("does not contain any stream") {
        match kind {
            Kind::Audio => "This file has no sound in it.",
            _ => "This file has no video in it.",
        }
    } else if has("no decoder found for: prores") || has("no decoder found for: dnxhd") {
        "Sastra Compressor can't read ProRes or DNxHD video. Export it as H.264 instead."
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
        let prores = ["[vist#0:0/prores @ 000001] Decoding requested, but no decoder found for: prores".to_string()];
        assert!(explain(Kind::Video, &prores).contains("Export it as H.264"));
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
        assert_eq!(sound_bitrate(128, None, Some(64)), 64);
        assert_eq!(sound_bitrate(128, None, Some(320)), 128);
        assert_eq!(sound_bitrate(128, None, None), 128);
        assert_eq!(sound_bitrate(96, None, Some(8)), 32, "not so low that it falls apart");
    }

    #[test]
    fn surround_gets_the_bitrate_for_each_pair_of_speakers() {
        assert_eq!(sound_bitrate(96, Some(1), None), 96);
        assert_eq!(sound_bitrate(96, Some(6), Some(640)), 288);
        assert_eq!(sound_bitrate(96, Some(8), Some(200)), 200, "still never above the source");
        let i = info(&["  Stream #0:1: Audio: ac3, 48000 Hz, 5.1(side), fltp, 384 kb/s"]);
        assert_eq!((i.audio_channels, i.audio_kbps), (Some(6), Some(384)));
        assert_eq!(info(&["Stream #0:0: Audio: pcm_s16le, 44100 Hz, 3 channels, s16, 2116 kb/s"]).audio_channels, Some(3));
    }
}
