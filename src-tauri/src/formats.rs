//! The file types the app accepts. This is the one list: the interface asks
//! for it, and `ffmpeg/build.sh` enables the matching parts of ffmpeg.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Image,
    Audio,
    Video,
}

pub const IMAGE: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif"];
pub const AUDIO: &[&str] = &["wav", "flac", "mp3", "ogg", "oga", "opus", "m4a", "aac", "aif", "aiff", "w64"];
pub const VIDEO: &[&str] = &["mp4", "m4v", "mov", "mkv", "webm", "avi"];

pub fn kind_of(ext: &str) -> Option<Kind> {
    let ext = ext.to_ascii_lowercase();
    let ext = ext.as_str();
    if IMAGE.contains(&ext) {
        Some(Kind::Image)
    } else if AUDIO.contains(&ext) {
        Some(Kind::Audio)
    } else if VIDEO.contains(&ext) {
        Some(Kind::Video)
    } else {
        None
    }
}

/// What each kind becomes.
pub fn output_ext(kind: Kind) -> &'static str {
    match kind {
        Kind::Image => "webp",
        Kind::Audio => "ogg",
        Kind::Video => "mp4",
    }
}

#[derive(Serialize)]
pub struct Formats {
    pub image: &'static [&'static str],
    pub audio: &'static [&'static str],
    pub video: &'static [&'static str],
}

pub fn all() -> Formats {
    Formats { image: IMAGE, audio: AUDIO, video: VIDEO }
}
