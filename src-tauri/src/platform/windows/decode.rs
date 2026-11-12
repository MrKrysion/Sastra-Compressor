//! Decoding through Media Foundation's Source Reader, whose H.264, HEVC, AAC, and
//! E-AC-3 decoders Microsoft licenses (see `crate::decode`).
//!
//! Media Foundation's DLLs are loaded when first needed, not linked: Windows N
//! editions lack them until the Media Feature Pack is installed, and a linked DLL
//! that's missing would stop the whole app from starting.
//!
//! Decoding stays on the CPU. Hardware decoding needs a Direct3D device and a copy
//! back from the graphics card, and gains little: VP9 encoding is many times slower.

use crate::decode::{Failure, Frames, VideoInfo, Wav};
use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use windows::core::{Interface, GUID, HRESULT, HSTRING, PCWSTR};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};

type Startup = unsafe extern "system" fn(u32, u32) -> HRESULT;
type CreateAttributes = unsafe extern "system" fn(*mut *mut c_void, u32) -> HRESULT;
type CreateMediaType = unsafe extern "system" fn(*mut *mut c_void) -> HRESULT;
type CreateReader = unsafe extern "system" fn(PCWSTR, *mut c_void, *mut *mut c_void) -> HRESULT;
type Found = unsafe extern "system" fn() -> isize;

/// The four functions used; everything else is reached through their COM interfaces.
struct Api {
    create_attributes: CreateAttributes,
    create_media_type: CreateMediaType,
    create_reader: CreateReader,
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| {
        // SAFETY: system DLLs by name, from System32 only; each pointer is cast to the
        // documented signature of the function it names.
        unsafe {
            let load = |name: &str| {
                let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
                let module = LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
                (!module.is_null()).then_some(module)
            };
            let (plat, readwrite) = (load("mfplat.dll")?, load("mfreadwrite.dll")?);
            let startup = std::mem::transmute::<Found, Startup>(GetProcAddress(plat, c"MFStartup".as_ptr().cast())?);
            // Once for the app's whole life; it's never shut down, as the app ends first.
            startup(MF_VERSION, MFSTARTUP_LITE).ok().ok()?;
            Some(Api {
                create_attributes: std::mem::transmute::<Found, CreateAttributes>(GetProcAddress(plat, c"MFCreateAttributes".as_ptr().cast())?),
                create_media_type: std::mem::transmute::<Found, CreateMediaType>(GetProcAddress(plat, c"MFCreateMediaType".as_ptr().cast())?),
                create_reader: std::mem::transmute::<Found, CreateReader>(GetProcAddress(readwrite, c"MFCreateSourceReaderFromURL".as_ptr().cast())?),
            })
        }
    })
    .as_ref()
}

fn other(what: &str, e: windows::core::Error) -> Failure {
    Failure::Other(format!("Windows couldn't {what}: {}", e.message()))
}

fn media_type(api: &Api, major: &GUID, subtype: &GUID) -> Result<IMFMediaType, Failure> {
    let mut raw = std::ptr::null_mut();
    // SAFETY: on success the function hands over one reference, which from_raw takes.
    unsafe {
        (api.create_media_type)(&mut raw).ok().map_err(|e| other("prepare the decoder", e))?;
        let t = IMFMediaType::from_raw(raw);
        t.SetGUID(&MF_MT_MAJOR_TYPE, major).map_err(|e| other("prepare the decoder", e))?;
        t.SetGUID(&MF_MT_SUBTYPE, subtype).map_err(|e| other("prepare the decoder", e))?;
        Ok(t)
    }
}

/// A reader with only the first stream of one kind selected, decoding to `subtype`.
/// `Ok(None)`: the file has no such stream.
fn open(path: &Path, stream: MF_SOURCE_READER_CONSTANTS, major: &GUID, subtype: &GUID) -> Result<Option<(&'static Api, IMFSourceReader)>, Failure> {
    let api = api().ok_or(Failure::NoDecoder)?;
    // SAFETY: plain COM calls on interfaces this function owns. COM is started for
    // this thread; a thread that already started it differently keeps its own.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut raw = std::ptr::null_mut();
        (api.create_attributes)(&mut raw, 1).ok().map_err(|e| other("prepare the decoder", e))?;
        let attributes = IMFAttributes::from_raw(raw);
        // Converts what the decoder gives (10-bit, interlaced) into plain NV12.
        attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1).map_err(|e| other("prepare the decoder", e))?;
        let url = HSTRING::from(path.as_os_str());
        let mut raw = std::ptr::null_mut();
        (api.create_reader)(PCWSTR(url.as_ptr()), attributes.as_raw(), &mut raw)
            .ok()
            .map_err(|e| other("open this file", e))?;
        let reader = IMFSourceReader::from_raw(raw);
        let _ = reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false);
        match reader.SetStreamSelection(stream.0 as u32, true) {
            Err(e) if e.code() == MF_E_INVALIDSTREAMNUMBER => return Ok(None),
            r => r.map_err(|e| other("open this file", e))?,
        }
        // No decoder for the stream: Windows N without the Media Feature Pack, or HEVC
        // without its extension. Both show up here, when asked for decoded output.
        reader
            .SetCurrentMediaType(stream.0 as u32, None, &media_type(api, major, subtype)?)
            .map_err(|_| Failure::NoDecoder)?;
        Ok(Some((api, reader)))
    }
}

fn duration(reader: &IMFSourceReader) -> Option<f64> {
    // SAFETY: MF_PD_DURATION is a VT_UI8 in 100-nanosecond units.
    unsafe {
        let v = reader.GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION).ok()?;
        let d = v.Anonymous.Anonymous.Anonymous.uhVal;
        (d > 0).then(|| d as f64 / 1e7)
    }
}

/// Two 32-bit numbers packed in one attribute, as frame sizes and rates are.
fn pair(t: &IMFMediaType, key: &GUID) -> Option<(u32, u32)> {
    // SAFETY: a plain attribute read.
    let v = unsafe { t.GetUINT64(key) }.ok()?;
    Some(((v >> 32) as u32, v as u32))
}

/// Where the picture sits in each decoded buffer: H.264 pads 1080 to 1088 rows,
/// for one, and says which part to show.
#[derive(Clone, Copy)]
struct Layout {
    width: u32,
    rows: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

fn layout(t: &IMFMediaType) -> Result<Layout, Failure> {
    let (width, rows) = pair(t, &MF_MT_FRAME_SIZE).ok_or_else(|| Failure::Other("Windows didn't say how big the video is.".into()))?;
    let mut area = MFVideoArea::default();
    // SAFETY: the blob is an MFVideoArea, and the buffer passed is exactly that size.
    let shown = unsafe {
        let bytes = std::slice::from_raw_parts_mut(std::ptr::from_mut(&mut area).cast::<u8>(), size_of::<MFVideoArea>());
        t.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, bytes, None).is_ok()
    };
    let (x, y, w, h) = if shown && area.Area.cx > 0 && area.Area.cy > 0 {
        (area.OffsetX.value.max(0) as u32, area.OffsetY.value.max(0) as u32, area.Area.cx as u32, area.Area.cy as u32)
    } else {
        (0, 0, width, rows)
    };
    // NV12's colour plane is half size both ways, so everything stays even.
    let (x, y) = (x & !1, y & !1);
    let (w, h) = (w.min(width.saturating_sub(x)) & !1, h.min(rows.saturating_sub(y)) & !1);
    if w == 0 || h == 0 {
        return Err(Failure::Other("Windows gave the video no size.".into()));
    }
    Ok(Layout { width, rows, x, y, w, h })
}

struct MfFrames {
    reader: IMFSourceReader,
    info: VideoInfo,
    layout: Layout,
    raw: Vec<u8>,
    picture: Vec<u8>,
}

const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
const AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;

pub fn open_video(path: &Path) -> Result<Box<dyn Frames>, Failure> {
    let (_, reader) = open(path, MF_SOURCE_READER_FIRST_VIDEO_STREAM, &MFMediaType_Video, &MFVideoFormat_NV12)?
        .ok_or_else(|| Failure::Other("This file has no video in it.".into()))?;
    // SAFETY: plain attribute reads on types the reader owns.
    unsafe {
        let native = reader.GetNativeMediaType(VIDEO, 0).map_err(|e| other("read this video", e))?;
        let current = reader.GetCurrentMediaType(VIDEO).map_err(|e| other("read this video", e))?;
        let layout = layout(&current)?;
        let fps = pair(&native, &MF_MT_FRAME_RATE).filter(|(n, d)| *n > 0 && *d > 0).unwrap_or((30, 1));
        // Checked against ffmpeg's own rotation: the value is the clockwise turn to make.
        let rotation = native.GetUINT32(&MF_MT_VIDEO_ROTATION).unwrap_or(0) % 360;
        let info = VideoInfo {
            width: layout.w,
            height: layout.h,
            fps,
            rotation,
            full_range: native.GetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE).is_ok_and(|r| r == MFNominalRange_0_255.0 as u32),
            bt709: native.GetUINT32(&MF_MT_YUV_MATRIX).is_ok_and(|m| m == MFVideoTransferMatrix_BT709.0 as u32),
            duration: duration(&reader),
        };
        Ok(Box::new(MfFrames { reader, info, layout, raw: Vec::new(), picture: Vec::new() }))
    }
}

impl Frames for MfFrames {
    fn info(&self) -> &VideoInfo {
        &self.info
    }

    fn next(&mut self) -> Result<Option<(i64, &[u8])>, Failure> {
        loop {
            let (mut flags, mut time, mut sample) = (0u32, 0i64, None);
            // SAFETY: synchronous ReadSample with every out-pointer valid for the call.
            unsafe { self.reader.ReadSample(VIDEO, 0, None, Some(&mut flags), Some(&mut time), Some(&mut sample)) }
                .map_err(|e| other("read this video", e))?;
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                return Err(Failure::Other("Windows couldn't read this video: the file may be damaged.".into()));
            }
            if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                // SAFETY: a plain read of the reader's own type.
                let t = unsafe { self.reader.GetCurrentMediaType(VIDEO) }.map_err(|e| other("read this video", e))?;
                let l = layout(&t)?;
                if (l.w, l.h) != (self.info.width, self.info.height) {
                    return Err(Failure::Other("The picture changes size partway through, which can't be compressed.".into()));
                }
                self.layout = l;
            }
            if let Some(sample) = sample {
                self.copy(&sample)?;
                return Ok(Some((time, &self.picture)));
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(None);
            }
        }
    }
}

impl MfFrames {
    /// Copies the shown part of a decoded NV12 picture into `picture`, tightly packed.
    fn copy(&mut self, sample: &IMFSample) -> Result<(), Failure> {
        let fail = |e| other("read this video", e);
        let l = self.layout;
        let full = (l.width * l.rows * 3 / 2) as usize;
        // SAFETY: the buffer is locked while read, and only within the length it reports.
        unsafe {
            let buffer = sample.ConvertToContiguousBuffer().map_err(fail)?;
            if let Ok(b2) = buffer.cast::<IMF2DBuffer>() {
                // Copies without the row padding a decoder may add, so rows are `width` apart.
                self.raw.resize(b2.GetContiguousLength().map_err(fail)? as usize, 0);
                b2.ContiguousCopyTo(&mut self.raw).map_err(fail)?;
            } else {
                let (mut p, mut len) = (std::ptr::null_mut(), 0u32);
                buffer.Lock(&mut p, None, Some(&mut len)).map_err(fail)?;
                self.raw.clear();
                self.raw.extend_from_slice(std::slice::from_raw_parts(p, len as usize));
                let _ = buffer.Unlock();
            }
        }
        if self.raw.len() < full {
            return Err(Failure::Other("Windows gave a picture smaller than the video's size.".into()));
        }
        let (w, h, stride) = (l.w as usize, l.h as usize, l.width as usize);
        let (x, y) = (l.x as usize, l.y as usize);
        let chroma = stride * l.rows as usize;
        self.picture.clear();
        for r in 0..h {
            let at = (y + r) * stride + x;
            self.picture.extend_from_slice(&self.raw[at..at + w]);
        }
        for r in 0..h / 2 {
            let at = chroma + (y / 2 + r) * stride + x;
            self.picture.extend_from_slice(&self.raw[at..at + w]);
        }
        Ok(())
    }
}

/// Decodes the first sound track into a float WAV. `Ok(false)`: there's no sound.
pub fn read_sound(path: &Path, wav: &Path, cancel: &AtomicBool) -> Result<bool, Failure> {
    let Some((_, reader)) = open(path, MF_SOURCE_READER_FIRST_AUDIO_STREAM, &MFMediaType_Audio, &MFAudioFormat_Float)? else {
        return Ok(false);
    };
    let fail = |e| other("read this sound", e);
    let saving = |e: std::io::Error| Failure::Other(format!("Couldn't write the sound to a temporary file.\n{e}"));
    // SAFETY: plain COM calls; each buffer is locked while read, within its stated length.
    unsafe {
        let t = reader.GetCurrentMediaType(AUDIO).map_err(fail)?;
        let channels = t.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).map_err(fail)? as u16;
        let rate = t.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).map_err(fail)?;
        let mut out = Wav::create(wav, channels, rate, t.GetUINT32(&MF_MT_AUDIO_CHANNEL_MASK).ok()).map_err(saving)?;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(Failure::Other("cancelled".into()));
            }
            let (mut flags, mut sample) = (0u32, None);
            reader.ReadSample(AUDIO, 0, None, Some(&mut flags), None, Some(&mut sample)).map_err(fail)?;
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                return Err(Failure::Other("Windows couldn't read this sound: the file may be damaged.".into()));
            }
            if let Some(sample) = sample {
                let buffer = sample.ConvertToContiguousBuffer().map_err(fail)?;
                let (mut p, mut len) = (std::ptr::null_mut(), 0u32);
                buffer.Lock(&mut p, None, Some(&mut len)).map_err(fail)?;
                let written = out.write(std::slice::from_raw_parts(p, len as usize));
                let _ = buffer.Unlock();
                written.map_err(saving)?;
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
        }
        out.finish().map_err(saving)?;
    }
    Ok(true)
}

/// Whether an H.264 video is 8-bit 4:2:0, the kind every browser plays, judged by its
/// profile. `None` when Windows can't tell.
pub fn plain_h264(path: &Path) -> Option<bool> {
    let (_, reader) = open(path, MF_SOURCE_READER_FIRST_VIDEO_STREAM, &MFMediaType_Video, &MFVideoFormat_NV12).ok()??;
    // SAFETY: plain attribute reads.
    let profile = unsafe { reader.GetNativeMediaType(VIDEO, 0).ok()?.GetUINT32(&MF_MT_MPEG2_PROFILE).ok()? };
    // Baseline, Main, Extended, and High; High 10, 4:2:2, and 4:4:4 aren't.
    Some([66, 77, 88, 100].contains(&profile))
}

/// Needs a full ffmpeg and ffprobe (libx264, libx265, AAC and E-AC-3 encoders) to make
/// the samples and compare against; skipped without them. `cargo test` puts the app's
/// own ffmpeg first on the PATH, so point `SASTRA_TEST_FFMPEG` at the full one's folder.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::pump;
    use std::path::PathBuf;

    #[allow(non_snake_case)]
    fn Command(tool: &str) -> std::process::Command {
        let dir = std::env::var_os("SASTRA_TEST_FFMPEG").map(PathBuf::from).unwrap_or_default();
        std::process::Command::new(dir.join(tool))
    }

    fn ffmpeg() -> bool {
        Command("ffmpeg").args(["-hide_banner", "-filters"]).output().is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("testsrc2"))
    }

    fn sample(name: &str, args: &[&str]) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sastra-mf-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(name);
        let ok = Command("ffmpeg").args(["-hide_banner", "-loglevel", "error", "-y"]).args(args).arg(&p).status().unwrap();
        assert!(ok.success(), "making {name}");
        p
    }

    /// The first picture as system ffmpeg decodes it, in NV12.
    fn reference(p: &Path, filters: &str) -> Vec<u8> {
        let out = Command("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-i"])
            .arg(p)
            .args(["-frames:v", "1", "-vf", &format!("{filters}format=nv12"), "-f", "rawvideo", "-"])
            .output()
            .unwrap();
        out.stdout
    }

    fn mean_difference(a: &[u8], b: &[u8]) -> f64 {
        assert_eq!(a.len(), b.len(), "picture sizes differ");
        a.iter().zip(b).map(|(x, y)| f64::from(x.abs_diff(*y))).sum::<f64>() / a.len() as f64
    }

    #[test]
    fn h264_1080p_comes_out_cropped_and_matching() {
        if !ffmpeg() {
            return;
        }
        let p = sample("a.mp4", &["-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=30:duration=2", "-c:v", "libx264", "-pix_fmt", "yuv420p"]);
        let mut v = open_video(&p).unwrap();
        let info = v.info().clone();
        assert_eq!((info.width, info.height, info.fps, info.rotation), (1920, 1080, (30, 1), 0));
        assert!((info.duration.unwrap() - 2.0).abs() < 0.1, "{:?}", info.duration);
        let (_, first) = v.next().unwrap().unwrap();
        let diff = mean_difference(first, &reference(&p, ""));
        assert!(diff < 1.5, "differs from ffmpeg's decode by {diff}");
        assert_eq!(plain_h264(&p), Some(true));
    }

    #[test]
    fn every_frame_arrives_once_at_a_steady_rate() {
        if !ffmpeg() {
            return;
        }
        let p = sample("b.mp4", &["-f", "lavfi", "-i", "testsrc2=size=320x180:rate=24:duration=2", "-c:v", "libx264"]);
        let mut v = open_video(&p).unwrap();
        let mut out = Vec::new();
        pump(v.as_mut(), &mut out, &AtomicBool::new(false)).unwrap();
        assert_eq!(out.len(), 48 * 320 * 180 * 3 / 2);
    }

    #[test]
    fn high_10_is_read_but_not_plain() {
        if !ffmpeg() {
            return;
        }
        let p = sample("c.mp4", &["-f", "lavfi", "-i", "testsrc2=size=320x180:rate=24:duration=1", "-c:v", "libx264", "-pix_fmt", "yuv420p10le"]);
        assert_eq!(plain_h264(&p), Some(false));
        match open_video(&p) {
            // Windows' H.264 decoder doesn't do 10-bit everywhere; it must say so, not fail oddly.
            Err(Failure::NoDecoder) => {}
            r => assert_eq!(r.map(|v| v.info().width).ok(), Some(320)),
        }
    }

    #[test]
    fn rotation_matches_what_ffmpeg_would_do() {
        if !ffmpeg() {
            return;
        }
        let plain = sample("d.mp4", &["-f", "lavfi", "-i", "testsrc2=size=320x180:rate=24:duration=1", "-c:v", "libx264"]);
        let p = sample("d-rotated.mp4", &["-display_rotation", "90", "-i", plain.to_str().unwrap(), "-c", "copy"]);
        let mut v = open_video(&p).unwrap();
        let info = v.info().clone();
        let (_, first) = v.next().unwrap().unwrap();
        let first = first.to_vec();
        // Turn Media Foundation's picture upright with the app's filters, and compare
        // with ffmpeg's own automatic rotation.
        let raw = std::env::temp_dir().join(format!("sastra-mf-{}/rot.nv12", std::process::id()));
        std::fs::write(&raw, &first).unwrap();
        let upright = Command("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "nv12", "-video_size", &format!("{}x{}", info.width, info.height), "-i"])
            .arg(&raw)
            .args(["-vf", &format!("{}format=nv12", crate::decode::upright(info.rotation)), "-f", "rawvideo", "-"])
            .output()
            .unwrap()
            .stdout;
        let diff = mean_difference(&upright, &reference(&p, ""));
        assert!(diff < 1.5, "rotation {} differs by {diff}", info.rotation);
    }

    #[test]
    fn surround_aac_becomes_a_float_wav() {
        if !ffmpeg() {
            return;
        }
        let p = sample("e.m4a", &["-f", "lavfi", "-i", "sine=frequency=440:duration=3,aformat=channel_layouts=5.1", "-c:a", "aac"]);
        let wav = p.with_extension("wav");
        assert!(read_sound(&p, &wav, &AtomicBool::new(false)).unwrap());
        let probe = Command("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_name,channels,channel_layout:format=duration", "-of", "csv=p=0"])
            .arg(&wav)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&probe.stdout);
        assert!(text.contains("pcm_f32le,6,5.1"), "{text}");
        let seconds: f64 = text.lines().last().unwrap().trim().parse().unwrap();
        assert!((seconds - 3.0).abs() < 0.1, "{seconds}");
    }

    #[test]
    fn a_video_without_sound_says_so() {
        if !ffmpeg() {
            return;
        }
        let p = sample("f.mp4", &["-f", "lavfi", "-i", "testsrc2=size=64x64:duration=1", "-c:v", "libx264"]);
        assert!(!read_sound(&p, &p.with_extension("wav"), &AtomicBool::new(false)).unwrap());
    }

    /// HEVC and E-AC-3 need decoders not every PC has; either they work or the app says what's missing.
    #[test]
    fn hevc_and_eac3_work_or_say_what_is_missing() {
        if !ffmpeg() {
            return;
        }
        let p = sample("g.mp4", &[
            "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=25:duration=1", "-f", "lavfi", "-i", "sine=duration=1",
            "-c:v", "libx265", "-x265-params", "log-level=error", "-tag:v", "hvc1", "-c:a", "eac3", "-shortest",
        ]);
        match open_video(&p) {
            Ok(mut v) => {
                let (_, first) = v.next().unwrap().unwrap();
                let diff = mean_difference(first, &reference(&p, ""));
                assert!(diff < 1.5, "HEVC differs by {diff}");
                println!("HEVC: read by Windows");
            }
            Err(Failure::NoDecoder) => println!("HEVC: no decoder on this PC"),
            Err(Failure::Other(e)) => panic!("{e}"),
        }
        match read_sound(&p, &p.with_extension("wav"), &AtomicBool::new(false)) {
            Ok(had) => {
                assert!(had);
                println!("E-AC-3: read by Windows");
            }
            Err(Failure::NoDecoder) => println!("E-AC-3: no decoder on this PC"),
            Err(Failure::Other(e)) => panic!("{e}"),
        }
    }
}
