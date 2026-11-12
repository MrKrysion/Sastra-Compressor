//! The file types the app accepts. This is the one list: the interface asks for
//! it (the file picker, the home screen, and "Every format"), the scan uses it, and
//! `ffmpeg/build.sh` enables the matching parts of ffmpeg. What only one system can
//! read is added by `platform::FORMATS`.

use crate::platform;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Image,
    Audio,
    Video,
}

/// One format as people know it, with every extension it uses.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Format {
    pub kind: Kind,
    pub name: &'static str,
    pub ext: &'static [&'static str],
    /// Shown on the home screen (the five best known of each kind), as this label.
    pub common: Option<&'static str>,
    /// What this computer needs to read it, when it needs something.
    pub note: Option<&'static str>,
}

const fn f(kind: Kind, name: &'static str, ext: &'static [&'static str]) -> Format {
    Format { kind, name, ext, common: None, note: None }
}

const fn c(kind: Kind, name: &'static str, ext: &'static [&'static str], label: &'static str) -> Format {
    Format { kind, name, ext, common: Some(label), note: None }
}

use Kind::{Audio as A, Image as I, Video as V};

/// Read the same way on every system: by the app itself, or by its ffmpeg.
const FORMATS: &[Format] = &[
    c(I, "PNG", &["png"], "PNG"),
    c(I, "JPEG", &["jpg", "jpeg"], "JPEG"),
    c(I, "WebP", &["webp"], "WebP"),
    c(I, "Photoshop", &["psd"], "PSD"),
    f(I, "GIF", &["gif"]),
    f(I, "BMP", &["bmp"]),
    f(I, "TIFF", &["tif", "tiff"]),
    f(I, "Krita", &["kra"]),
    f(I, "OpenRaster", &["ora"]),
    f(I, "AVIF", &["avif"]),
    f(I, "JPEG XL", &["jxl"]),
    f(I, "Targa (TGA)", &["tga"]),
    c(A, "WAV", &["wav"], "WAV"),
    c(A, "MP3", &["mp3"], "MP3"),
    c(A, "FLAC", &["flac"], "FLAC"),
    c(A, "Ogg Vorbis and Opus", &["ogg", "oga", "opus"], "OGG"),
    c(A, "AAC and Apple Lossless", &["m4a", "m4b", "aac"], "M4A"),
    f(A, "AIFF", &["aif", "aiff", "aifc"]),
    f(A, "Windows Media Audio", &["wma"]),
    f(A, "WavPack", &["wv"]),
    f(A, "Monkey's Audio", &["ape"]),
    f(A, "True Audio (TTA)", &["tta"]),
    f(A, "Musepack", &["mpc"]),
    f(A, "Speex", &["spx"]),
    f(A, "AMR", &["amr", "3ga"]),
    f(A, "MPEG Layer II (MP2)", &["mp2", "mpa"]),
    f(A, "Dolby Digital and Digital Plus", &["ac3", "eac3"]),
    f(A, "DTS", &["dts"]),
    f(A, "Core Audio Format (CAF)", &["caf"]),
    f(A, "Matroska and WebM audio", &["mka", "weba"]),
    f(A, "Sun and NeXT audio (AU)", &["au", "snd"]),
    f(A, "Wave64", &["w64"]),
    c(V, "MP4", &["mp4", "m4v"], "MP4"),
    c(V, "QuickTime", &["mov"], "MOV"),
    c(V, "Matroska", &["mkv"], "MKV"),
    c(V, "WebM", &["webm"], "WebM"),
    c(V, "AVI", &["avi"], "AVI"),
    f(V, "Windows Media Video", &["wmv", "asf"]),
    f(V, "MPEG-1, MPEG-2, and DVD", &["mpg", "mpeg", "m2v", "vob"]),
    f(V, "AVCHD and MPEG-TS", &["mts", "m2ts", "ts"]),
    f(V, "3GP and 3G2", &["3gp", "3g2"]),
    f(V, "Ogg Theora", &["ogv"]),
];

/// Every format this system reads.
pub fn list() -> impl Iterator<Item = &'static Format> {
    FORMATS.iter().chain(platform::FORMATS)
}

pub fn kind_of(ext: &str) -> Option<Kind> {
    let ext = ext.to_ascii_lowercase();
    list().find(|f| f.ext.contains(&ext.as_str())).map(|f| f.kind)
}

/// What each kind becomes.
pub fn output_ext(kind: Kind) -> &'static str {
    match kind {
        Kind::Image => "webp",
        Kind::Audio => "ogg",
        Kind::Video => "webm",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Formats {
    /// Extensions by kind, for the file picker.
    pub image: Vec<&'static str>,
    pub audio: Vec<&'static str>,
    pub video: Vec<&'static str>,
    /// The formats themselves, for the home screen and "Every format".
    pub list: Vec<Format>,
    /// What this system can't read, or needs for some videos.
    pub video_note: Option<&'static str>,
}

pub fn all() -> Formats {
    let ext = |k: Kind| list().filter(|f| f.kind == k).flat_map(|f| f.ext.iter().copied()).collect();
    Formats { image: ext(Kind::Image), audio: ext(Kind::Audio), video: ext(Kind::Video), list: list().copied().collect(), video_note: platform::VIDEO_NOTE }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_extension_belongs_to_one_format() {
        let mut seen = std::collections::HashSet::new();
        for f in list() {
            for e in f.ext {
                assert!(seen.insert(*e), ".{e} is listed twice");
                assert_eq!(*e, e.to_ascii_lowercase());
            }
        }
        assert_eq!(kind_of("MKV"), Some(Kind::Video));
        assert_eq!(kind_of("txt"), None);
    }

    #[test]
    fn at_most_five_of_each_kind_are_shown_on_the_home_screen() {
        for k in [Kind::Image, Kind::Audio, Kind::Video] {
            assert!(list().filter(|f| f.kind == k && f.common.is_some()).count() <= 5);
        }
    }
}
