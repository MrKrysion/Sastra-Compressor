//! Reading the formats ffmpeg isn't built to read: H.264, HEVC, VC-1 (WMV 9), AAC,
//! WMA Pro, Lossless, and Voice, and AMR.
//! Their patent licences are the operating system's, so the OS decodes them
//! (`platform::open_video`, `platform::read_sound`) and ffmpeg only ever sees
//! raw NV12 pictures on its stdin and the sound as a float WAV file.
//!
//! The pictures go through a pipe with no timestamps, so `pump` repeats or drops
//! them to a steady frame rate; that keeps a variable-rate phone video in step
//! with its sound. Times count from the first picture: MP4 files with B-frames
//! start a frame or two late, and the edit list that undoes it isn't applied.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// What the OS reads instead of ffmpeg, by the names ffmpeg's log gives the codecs.
/// A codec the bundled ffmpeg can decode after all is left to ffmpeg.
pub const OS_VIDEO: &[&str] = &["h264", "hevc", "vc1", "wmv3"];
pub const OS_SOUND: &[&str] = &["aac", "eac3", "wmapro", "wmalossless", "wmavoice", "amr_nb", "amr_wb"];

/// Why the OS couldn't read a file.
#[derive(Debug)]
pub enum Failure {
    /// The OS has no decoder for it (Windows N without the Media Feature Pack,
    /// HEVC without Microsoft's extension), or no decoding at all yet (macOS, Linux).
    NoDecoder,
    /// Anything else, already in words.
    Other(String),
}

impl Failure {
    /// A sentence the person can act on. `codec` is ffmpeg's name for it.
    pub fn explain(&self, codec: &str) -> String {
        match self {
            Failure::NoDecoder if !cfg!(windows) => "This computer can't read this file's format yet.".into(),
            Failure::NoDecoder if codec == "hevc" => {
                "Windows can't read HEVC (H.265) video on this PC. Install HEVC Video Extensions from the Microsoft Store, or export the video as H.264.".into()
            }
            Failure::NoDecoder => {
                "Windows can't read this file's format on this PC. On Windows N editions, install the free Media Feature Pack from Microsoft.".into()
            }
            Failure::Other(e) => e.clone(),
        }
    }
}

/// The decoded video, as it comes out of the OS.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoInfo {
    /// Always even: NV12 halves both for its colour plane.
    pub width: u32,
    pub height: u32,
    /// Frames per second as a fraction, as the file states it.
    pub fps: (u32, u32),
    /// Degrees to turn the pictures clockwise to show them upright.
    pub rotation: u32,
    pub full_range: bool,
    pub bt709: bool,
    pub duration: Option<f64>,
}

/// Pictures in NV12, rows tightly packed, with their times in 100-nanosecond units.
pub trait Frames {
    fn info(&self) -> &VideoInfo;
    /// The next picture, or `None` at the end.
    fn next(&mut self) -> Result<Option<(i64, &[u8])>, Failure>;
}

/// Writes every picture to `out` at the file's own steady rate: a picture is repeated
/// until the next one is due, and one that arrives too soon replaces the last.
pub fn pump(frames: &mut dyn Frames, out: &mut impl Write, cancel: &AtomicBool) -> Result<(), Failure> {
    let (num, den) = frames.info().fps;
    let step = 1e7 * f64::from(den) / f64::from(num.max(1));
    let broken = |e: std::io::Error| Failure::Other(format!("ffmpeg stopped taking pictures: {e}"));
    let mut held: Vec<u8> = Vec::new();
    let (mut start, mut last) = (None, None);
    let mut n = 0u64;
    while let Some((time, picture)) = frames.next()? {
        if cancel.load(Ordering::Relaxed) {
            return Err(Failure::Other("cancelled".into()));
        }
        let time = time - *start.get_or_insert(time);
        if last.is_some() {
            while (n as f64 + 0.5) * step <= time as f64 {
                out.write_all(&held).map_err(broken)?;
                n += 1;
            }
        }
        held.clear();
        held.extend_from_slice(picture);
        last = Some(time);
    }
    if let Some(time) = last {
        // The last picture lasts one frame.
        loop {
            out.write_all(&held).map_err(broken)?;
            n += 1;
            if n as f64 * step >= time as f64 + step / 2.0 {
                break;
            }
        }
    }
    out.flush().map_err(broken)
}

/// What ffmpeg needs to read the pictures `pump` writes.
pub fn input_args(info: &VideoInfo) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-f".into(), "rawvideo".into(), "-pix_fmt".into(), "nv12".into(),
        "-video_size".into(), format!("{}x{}", info.width, info.height),
        "-framerate".into(), format!("{}/{}", info.fps.0, info.fps.1.max(1)),
        "-color_range".into(), (if info.full_range { "pc" } else { "tv" }).into(),
    ];
    if info.bt709 {
        a.extend(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709"].map(String::from));
    }
    a.extend(["-i".into(), "-".into()]);
    a
}

/// The filters that turn the pictures upright; empty when they already are.
pub fn upright(rotation: u32) -> &'static str {
    match rotation {
        90 => "transpose=clock,",
        180 => "hflip,vflip,",
        270 => "transpose=cclock,",
        _ => "",
    }
}

/// A float WAV file, written as the sound is decoded. ffmpeg is told to ignore its
/// stated length, so a sound track over 4 GB still reads in full.
pub struct Wav {
    file: BufWriter<File>,
    bytes: u64,
}

impl Wav {
    pub fn create(path: &Path, channels: u16, rate: u32, mask: Option<u32>) -> std::io::Result<Wav> {
        let mut file = BufWriter::new(File::create(path)?);
        let align = u32::from(channels) * 4;
        // Speakers in the order the decoder gives them: the usual layout for that many.
        let mask = mask.unwrap_or(match channels {
            1 => 0x4,
            2 => 0x3,
            6 => 0x60F,
            8 => 0x63F,
            _ => 0,
        });
        let mut h = Vec::with_capacity(68);
        h.extend(b"RIFF");
        h.extend(0u32.to_le_bytes());
        h.extend(b"WAVEfmt ");
        h.extend(40u32.to_le_bytes());
        h.extend(0xFFFEu16.to_le_bytes()); // WAVE_FORMAT_EXTENSIBLE
        h.extend(channels.to_le_bytes());
        h.extend(rate.to_le_bytes());
        h.extend((rate * align).to_le_bytes());
        h.extend((align as u16).to_le_bytes());
        h.extend(32u16.to_le_bytes());
        h.extend(22u16.to_le_bytes());
        h.extend(32u16.to_le_bytes());
        h.extend(mask.to_le_bytes());
        // KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
        h.extend([3, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);
        h.extend(b"data");
        h.extend(0u32.to_le_bytes());
        file.write_all(&h)?;
        Ok(Wav { file, bytes: 0 })
    }

    pub fn write(&mut self, samples: &[u8]) -> std::io::Result<()> {
        self.bytes += samples.len() as u64;
        self.file.write_all(samples)
    }

    pub fn finish(mut self) -> std::io::Result<()> {
        let data = u32::try_from(self.bytes).unwrap_or(u32::MAX);
        let riff = u32::try_from(self.bytes + 60).unwrap_or(u32::MAX);
        self.file.seek(SeekFrom::Start(4))?;
        self.file.write_all(&riff.to_le_bytes())?;
        self.file.seek(SeekFrom::Start(64))?;
        self.file.write_all(&data.to_le_bytes())?;
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        info: VideoInfo,
        times: Vec<i64>,
        at: usize,
        pic: [u8; 1],
    }

    impl Frames for Fake {
        fn info(&self) -> &VideoInfo {
            &self.info
        }
        fn next(&mut self) -> Result<Option<(i64, &[u8])>, Failure> {
            let Some(&t) = self.times.get(self.at) else { return Ok(None) };
            self.at += 1;
            self.pic = [self.at as u8];
            Ok(Some((t, &self.pic)))
        }
    }

    fn paced(fps: (u32, u32), times: &[i64]) -> Vec<u8> {
        let info = VideoInfo { width: 2, height: 2, fps, rotation: 0, full_range: false, bt709: false, duration: None };
        let mut f = Fake { info, times: times.to_vec(), at: 0, pic: [0] };
        let mut out = Vec::new();
        pump(&mut f, &mut out, &AtomicBool::new(false)).unwrap();
        out
    }

    const MS: i64 = 10_000;

    #[test]
    fn steady_video_comes_out_frame_for_frame() {
        assert_eq!(paced((25, 1), &[0, 40 * MS, 80 * MS, 120 * MS]), [1, 2, 3, 4]);
    }

    #[test]
    fn gaps_repeat_the_last_picture_and_bursts_drop_one() {
        // 25 fps: a 120 ms gap after the first picture, then two pictures 10 ms apart.
        assert_eq!(paced((25, 1), &[0, 120 * MS, 130 * MS, 160 * MS]), [1, 1, 1, 3, 4]);
    }

    #[test]
    fn a_late_first_picture_starts_the_video() {
        assert_eq!(paced((10, 1), &[200 * MS, 300 * MS]), [1, 2]);
    }

    #[test]
    fn wav_header_states_the_format_and_length() {
        let p = std::env::temp_dir().join(format!("sastra-wav-{}.wav", std::process::id()));
        let mut w = Wav::create(&p, 6, 48000, None).unwrap();
        w.write(&[0u8; 48]).unwrap();
        w.finish().unwrap();
        let b = std::fs::read(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(b.len(), 68 + 48);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 108);
        assert_eq!(u16::from_le_bytes(b[22..24].try_into().unwrap()), 6);
        assert_eq!(u32::from_le_bytes(b[40..44].try_into().unwrap()), 0x60F);
        assert_eq!(&b[60..64], b"data");
        assert_eq!(u32::from_le_bytes(b[64..68].try_into().unwrap()), 48);
    }
}
