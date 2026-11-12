//! End-to-end checks of the compressing itself. The ffmpeg tests make their samples
//! with a full ffmpeg and ffprobe (libx264, libx265, libvpx, libopus, the AAC and
//! E-AC-3 encoders), and are skipped without one. They use the first on the PATH
//! outside `target`, where `cargo test` puts the app's own; `SASTRA_TEST_FFMPEG`
//! names its folder instead.

use crate::formats::Kind;
use crate::runner::{work, Update};
use crate::settings::{Job, Level, SaveMode, Settings};
use std::sync::Mutex;
use crate::{ffmpeg, image_job};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sastra-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn settings() -> Settings {
    Settings {
        image_quality: Level::Balanced,
        image_max_width: 0,
        audio_bitrate: 96,
        video_quality: Level::Balanced,
        video_max_width: 1280,
        video_max_fps: 0,
        save_mode: SaveMode::Beside,
        output_folder: None,
        keep_existing: false,
        delete_for_good: false,
    }
}

#[test]
fn image_becomes_a_smaller_webp_and_shrinks() {
    let d = temp("img");
    let src = d.join("bg.png");
    let img = image::RgbImage::from_fn(2400, 1350, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]));
    img.save(&src).unwrap();
    let out = d.join("bg.webp");
    image_job::compress(&src, &out, 75.0, 1920).unwrap();
    let back = image::open(&out).unwrap();
    assert_eq!((back.width(), back.height()), (1920, 1080));
    assert!(std::fs::metadata(&out).unwrap().len() < std::fs::metadata(&src).unwrap().len());
}

#[test]
fn transparent_sprite_keeps_its_transparency() {
    let d = temp("alpha");
    let src = d.join("sprite.png");
    let img = image::RgbaImage::from_fn(300, 600, |x, _| image::Rgba([255, 0, 110, if x < 150 { 0 } else { 255 }]));
    img.save(&src).unwrap();
    let out = d.join("sprite.webp");
    image_job::compress(&src, &out, 75.0, 0).unwrap();
    let back = image::open(&out).unwrap().to_rgba8();
    assert_eq!(back.get_pixel(10, 10).0[3], 0);
    assert_eq!(back.get_pixel(290, 10).0[3], 255);
}

#[test]
fn not_an_image_is_a_clear_error() {
    let d = temp("bad");
    let src = d.join("broken.png");
    std::fs::write(&src, b"this is not a picture").unwrap();
    assert!(image_job::compress(&src, &d.join("x.webp"), 75.0, 0).is_err());
}

/// The full ffmpeg or ffprobe the samples are made and checked with.
pub fn tool(name: &str) -> Command {
    let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let dir = std::env::var_os("SASTRA_TEST_FFMPEG").map(PathBuf::from).or_else(|| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path).find(|d| d.join(&exe).is_file() && !d.components().any(|c| c.as_os_str() == "target"))
    });
    Command::new(dir.unwrap_or_default().join(exe))
}

pub fn have_ffmpeg() -> bool {
    tool("ffmpeg").args(["-hide_banner", "-filters"]).output().is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("testsrc2"))
}

pub fn make(path: &Path, args: &[&str]) {
    let ok = tool("ffmpeg").args(["-hide_banner", "-loglevel", "error", "-y"]).args(args).arg(path).status().unwrap();
    assert!(ok.success(), "making {}", path.display());
}

/// What ffprobe says, one line per stream (or format), as CSV.
fn probe(path: &Path, entries: &str) -> String {
    let out = tool("ffprobe").args(["-v", "error", "-show_entries", entries, "-of", "csv=p=0"]).arg(path).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().map(str::trim).collect::<Vec<_>>().join("\n")
}

#[test]
fn audio_becomes_opus_with_progress() {
    if !have_ffmpeg() {
        return;
    }
    let d = temp("audio");
    let src = d.join("theme.wav");
    make(&src, &["-f", "lavfi", "-i", "sine=frequency=440:duration=5"]);
    let out = d.join("theme.ogg");
    let mut seen = vec![];
    ffmpeg::compress(Kind::Audio, &src, &out, &settings(), &ffmpeg::inspect(&src), &Arc::new(AtomicBool::new(false)), |p| seen.push(p)).unwrap();
    assert!(std::fs::metadata(&out).unwrap().len() > 1000);
    assert!(seen.iter().any(|p| *p > 0.5), "progress was reported: {seen:?}");
}

#[test]
fn odd_sized_video_is_shrunk_and_made_even() {
    if !have_ffmpeg() {
        return;
    }
    let d = temp("video");
    let src = d.join("op.mkv");
    make(&src, &["-f", "lavfi", "-i", "testsrc2=size=1921x1081:rate=24:duration=1", "-c:v", "ffv1"]);
    let out = d.join("op.webm");
    ffmpeg::compress(Kind::Video, &src, &out, &settings(), &ffmpeg::inspect(&src), &Arc::new(AtomicBool::new(false)), |_| {}).unwrap();
    assert_eq!(probe(&out, "stream=codec_name,width,height"), "vp9,1280,720");
}

/// On Windows the app's ffmpeg can't decode H.264 or AAC, so this goes through Media
/// Foundation; elsewhere a full ffmpeg reads it directly. Either way: VP9 and Opus.
#[test]
fn h264_with_aac_becomes_vp9_and_opus_in_step() {
    if !have_ffmpeg() {
        return;
    }
    let d = temp("h264");
    let src = d.join("scene.mp4");
    let hd = ["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709"];
    make(&src, &[&["-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30:duration=3", "-f", "lavfi", "-i", "sine=duration=3", "-c:v", "libx264", "-c:a", "aac", "-shortest"][..], &hd].concat());
    let out = d.join("scene.webm");
    let mut seen = vec![];
    let note = ffmpeg::compress(Kind::Video, &src, &out, &settings(), &ffmpeg::inspect(&src), &Arc::new(AtomicBool::new(false)), |p| seen.push(p)).unwrap();
    assert!(note.is_none(), "{note:?}");
    assert_eq!(probe(&out, "stream=codec_name"), "vp9\nopus");
    // The source's colours carry over, also when its pictures come through the pipe.
    assert_eq!(probe(&out, "stream=color_range,color_space,color_transfer,color_primaries").lines().next(), Some("tv,bt709,bt709,bt709"));
    // 90 pictures at 30 a second, and sound just as long.
    let times = |stream: &str| -> Vec<f64> {
        let o = tool("ffprobe").args(["-v", "error", "-select_streams", stream, "-show_entries", "packet=pts_time", "-of", "csv=p=0"]).arg(&out).output().unwrap();
        String::from_utf8_lossy(&o.stdout).lines().filter_map(|l| l.trim().parse().ok()).collect()
    };
    let (video, sound) = (times("v"), times("a"));
    assert_eq!(video.len(), 90);
    let end = sound.iter().cloned().fold(0.0, f64::max);
    assert!((end - 3.0).abs() < 0.1, "sound ends at {end}");
    assert!(seen.iter().any(|p| *p > 0.5), "progress was reported: {seen:?}");
}

#[test]
fn a_phone_video_turned_on_its_side_comes_out_upright() {
    if !have_ffmpeg() {
        return;
    }
    let d = temp("rotated");
    let flat = d.join("flat.mp4");
    make(&flat, &["-f", "lavfi", "-i", "testsrc2=size=640x360:rate=24:duration=1", "-c:v", "libx264"]);
    let src = d.join("phone.mp4");
    make(&src, &["-display_rotation", "90", "-i", flat.to_str().unwrap(), "-c", "copy"]);
    let out = d.join("phone.webm");
    ffmpeg::compress(Kind::Video, &src, &out, &settings(), &ffmpeg::inspect(&src), &Arc::new(AtomicBool::new(false)), |_| {}).unwrap();
    assert_eq!(probe(&out, "stream=width,height"), "360,640");
}

#[test]
fn audio_file_without_sound_says_so() {
    if !have_ffmpeg() {
        return;
    }
    let d = temp("silent");
    let src = d.join("silent.mp4");
    make(&src, &["-f", "lavfi", "-i", "testsrc2=size=64x64:duration=1", "-c:v", "libx264"]);
    let err = ffmpeg::compress(Kind::Audio, &src, &d.join("x.ogg"), &settings(), &ffmpeg::inspect(&src), &Arc::new(AtomicBool::new(false)), |_| {}).unwrap_err();
    assert!(err.starts_with("This file has no sound in it."), "{err}");
}

#[test]
fn updates_reach_the_interface_in_its_own_words() {
    // The interface reads outPath and outSize; snake_case here showed "NaN KB".
    let u = Update::Done { id: "a".into(), out_path: "b".into(), out_size: 3, kept: false, note: None };
    assert_eq!(serde_json::to_string(&u).unwrap(), r#"{"state":"done","id":"a","outPath":"b","outSize":3,"kept":false,"note":null}"#);
}

#[test]
fn replacing_keeps_the_original_when_the_result_is_not_smaller() {
    let d = temp("bigger");
    let seed = d.join("seed.png");
    image::RgbImage::from_fn(256, 256, |x, y| image::Rgb([(x * 7 % 251) as u8, (y * 13 % 241) as u8, ((x ^ y) % 256) as u8])).save(&seed).unwrap();
    // A heavily compressed WebP, which a sharper setting can only make bigger.
    let webp = d.join("small.webp");
    image_job::compress(&seed, &webp, 20.0, 0).unwrap();
    let before = std::fs::read(&webp).unwrap();
    let jobs = vec![Job { id: "1".into(), path: webp.to_string_lossy().into(), rel: "small.webp".into(), kind: Kind::Image }];
    let s = Settings { save_mode: SaveMode::Replace, image_quality: Level::Sharper, ..settings() };
    let seen = Mutex::new(vec![]);
    work(jobs, &s, &Arc::new(AtomicBool::new(false)), &|u| seen.lock().unwrap().push(u));
    let seen = seen.into_inner().unwrap();
    assert!(matches!(seen.last(), Some(Update::Done { kept: true, .. })), "{seen:?}");
    assert_eq!(std::fs::read(&webp).unwrap(), before, "the original is untouched");
    assert!(!d.join("small.webp.sastra-part").exists());
}

#[test]
fn a_sprite_keeps_a_clean_edge_when_shrunk() {
    let d = temp("fringe");
    let src = d.join("sprite.png");
    // White on transparent black, the way paint programs commonly export it.
    image::RgbaImage::from_fn(800, 80, |x, _| if x < 400 { image::Rgba([0, 0, 0, 0]) } else { image::Rgba([255, 255, 255, 255]) }).save(&src).unwrap();
    let out = d.join("sprite.webp");
    image_job::compress(&src, &out, 100.0, 400).unwrap();
    let back = image::open(&out).unwrap().to_rgba8();
    for x in 190..210 {
        let p = back.get_pixel(x, 20).0;
        if p[3] > 8 {
            assert!(p[0] > 220, "visible edge pixel went dark at x={x}: {p:?}");
        }
    }
}

#[test]
fn animations_stay_animated() {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{AnimationDecoder, Delay, Frame};
    let d = temp("anim");
    let src = d.join("spin.gif");
    {
        let mut e = GifEncoder::new(std::fs::File::create(&src).unwrap());
        e.set_repeat(Repeat::Infinite).unwrap();
        for i in 0..4u32 {
            let img = image::RgbaImage::from_fn(64, 64, |x, _| image::Rgba([(x * 4) as u8, (i * 60) as u8, 0, 255]));
            e.encode_frame(Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(200, 1))).unwrap();
        }
    }
    let out = d.join("spin.webp");
    let note = image_job::compress(&src, &out, 75.0, 32).unwrap();
    assert_eq!(note.unwrap().text, "Stays animated");
    let reader = std::io::BufReader::new(std::fs::File::open(&out).unwrap());
    let frames: Vec<_> = image::codecs::webp::WebPDecoder::new(reader).unwrap().into_frames().collect_frames().unwrap();
    assert_eq!(frames[0].buffer().width(), 32);
    let total: u32 = frames.iter().map(|f| { let (n, d) = f.delay().numer_denom_ms(); n / d }).sum();
    assert_eq!(total, 800, "four frames of 200 ms, the last one included ({} frames)", frames.len());
}

/// A heavily compressed WebP, which a sharper setting can only make bigger.
fn small_webp(d: &Path) -> PathBuf {
    let seed = d.join("seed.png");
    image::RgbImage::from_fn(256, 256, |x, y| image::Rgb([(x * 7 % 251) as u8, (y * 13 % 241) as u8, ((x ^ y) % 256) as u8])).save(&seed).unwrap();
    let webp = d.join("small.webp");
    image_job::compress(&seed, &webp, 20.0, 0).unwrap();
    webp
}

fn run_one(path: &Path, s: &Settings) -> Vec<Update> {
    let jobs = vec![Job { id: "1".into(), path: path.to_string_lossy().into(), rel: path.file_name().unwrap().to_string_lossy().into(), kind: Kind::Image }];
    let seen = Mutex::new(vec![]);
    work(jobs, s, &Arc::new(AtomicBool::new(false)), &|u| seen.lock().unwrap().push(u));
    seen.into_inner().unwrap()
}

#[test]
fn alongside_saves_nothing_when_the_result_is_not_smaller() {
    let d = temp("kept-beside");
    let webp = small_webp(&d);
    let seen = run_one(&webp, &Settings { image_quality: Level::Sharper, ..settings() });
    assert!(matches!(seen.last(), Some(Update::Done { kept: true, .. })), "{seen:?}");
    assert!(!d.join("small_compressed.webp").exists());
}

#[test]
fn a_chosen_folder_gets_the_original_when_the_result_is_not_smaller() {
    let d = temp("kept-folder");
    let webp = small_webp(&d);
    let out = d.join("out");
    let s = Settings { image_quality: Level::Sharper, save_mode: SaveMode::Folder, output_folder: Some(out.to_string_lossy().into()), ..settings() };
    let seen = run_one(&webp, &s);
    match seen.last() {
        Some(Update::Done { kept: true, note: Some(n), .. }) => assert!(n.text.contains("copied")),
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read(out.join("small.webp")).unwrap(), std::fs::read(&webp).unwrap());
    // Running it again doesn't pile up copies...
    run_one(&webp, &s);
    assert!(!out.join("small_2.webp").exists());
    // ...and someone else's file of that name is left alone.
    std::fs::write(out.join("small.webp"), b"mine").unwrap();
    run_one(&webp, &s);
    assert_eq!(std::fs::read(out.join("small.webp")).unwrap(), b"mine");
    assert_eq!(std::fs::read(out.join("small_2.webp")).unwrap(), std::fs::read(&webp).unwrap());
}

#[test]
fn a_format_browsers_cannot_show_is_converted_even_if_bigger() {
    let d = temp("bmp");
    let bmp = d.join("tiny.bmp");
    image::RgbImage::from_fn(2, 2, |x, y| image::Rgb([(x * 90) as u8, (y * 90) as u8, 9])).save(&bmp).unwrap();
    let seen = run_one(&bmp, &settings());
    assert!(matches!(seen.last(), Some(Update::Done { kept: false, .. })), "{seen:?}");
    assert!(d.join("tiny_compressed.webp").exists());
}

#[test]
fn low_bitrate_sound_stays_low() {
    if !have_ffmpeg() {
        return;
    }
    let d = temp("lowsound");
    let src = d.join("low.mp4");
    make(&src, &["-f", "lavfi", "-i", "testsrc2=size=160x90:duration=2", "-f", "lavfi", "-i", "sine=duration=2", "-c:v", "libx264", "-c:a", "aac", "-b:a", "48k", "-shortest"]);
    let info = ffmpeg::inspect(&src);
    assert!(info.audio_kbps().is_some_and(|k| k <= 60), "{info:?}");
    let a = ffmpeg::args(Kind::Video, &src, &ffmpeg::Sources::default(), &d.join("o.webm"), &settings(), &info);
    let at = a.iter().position(|x| x == "-b:a").unwrap();
    assert!(a[at + 1].trim_end_matches('k').parse::<u32>().unwrap() <= 60, "{a:?}");
}

#[test]
fn every_image_format_the_image_library_writes_compresses() {
    let d = temp("formats");
    for ext in ["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif", "tga"] {
        let src = d.join(format!("a.{ext}"));
        image::RgbImage::from_fn(32, 32, |x, y| image::Rgb([(x * 7) as u8, (y * 7) as u8, 90])).save(&src).unwrap_or_else(|e| panic!("{ext}: {e}"));
        let out = d.join(format!("a-{ext}.webp"));
        image_job::compress(&src, &out, 75.0, 0).unwrap_or_else(|e| panic!("{ext}: {e}"));
        assert_eq!(image::open(&out).unwrap().to_rgb8().dimensions(), (32, 32), "{ext}");
    }
}

#[test]
fn krita_and_openraster_files_give_their_flattened_picture() {
    use std::io::Write;
    let d = temp("kra");
    let png = d.join("merged.png");
    image::RgbaImage::from_fn(40, 20, |x, _| image::Rgba([255, 0, 110, if x < 20 { 0 } else { 255 }])).save(&png).unwrap();
    for ext in ["kra", "ora"] {
        let src = d.join(format!("art.{ext}"));
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&src).unwrap());
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("mimetype", stored).unwrap();
        zip.write_all(b"image/openraster").unwrap();
        zip.start_file("mergedimage.png", stored).unwrap();
        zip.write_all(&std::fs::read(&png).unwrap()).unwrap();
        zip.finish().unwrap();
        let out = d.join(format!("art-{ext}.webp"));
        image_job::compress(&src, &out, 75.0, 0).unwrap();
        let back = image::open(&out).unwrap().to_rgba8();
        assert_eq!(back.dimensions(), (40, 20), "{ext}");
        assert_eq!((back.get_pixel(5, 5).0[3], back.get_pixel(35, 5).0[3]), (0, 255), "{ext} keeps its transparency");
    }
}

/// JPEG XL is read by jxl-oxide, AVIF by the app's ffmpeg; the samples come from a full ffmpeg.
#[test]
fn jpeg_xl_and_avif_pictures_become_webp() {
    if !have_ffmpeg() {
        return;
    }
    let d = temp("jxl");
    for (name, codec) in [("pic.jxl", "libjxl"), ("pic.avif", "libaom-av1")] {
        let src = d.join(name);
        let made = tool("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=96x64", "-frames:v", "1", "-c:v", codec])
            .arg(&src)
            .status()
            .is_ok_and(|s| s.success());
        if !made {
            continue; // this ffmpeg has no such encoder
        }
        let out = d.join(format!("{name}.webp"));
        image_job::compress(&src, &out, 75.0, 0).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(image::open(&out).unwrap().to_rgb8().dimensions(), (96, 64), "{name}");
    }
}

#[test]
fn a_still_gif_stays_a_still() {
    let d = temp("still");
    let src = d.join("one.gif");
    image::RgbImage::from_pixel(16, 16, image::Rgb([200, 0, 0])).save(&src).unwrap();
    assert!(image_job::compress(&src, &d.join("one.webp"), 75.0, 0).unwrap().is_none());
}

#[test]
fn wide_gamut_colours_keep_their_profile() {
    use image::{ImageDecoder, ImageEncoder};
    let d = temp("icc");
    let src = d.join("p3.png");
    // A stand-in profile: only the colour space field matters here.
    let mut profile = vec![0u8; 128];
    profile[16..20].copy_from_slice(b"RGB ");
    let mut enc = image::codecs::png::PngEncoder::new(std::fs::File::create(&src).unwrap());
    enc.set_icc_profile(profile.clone()).unwrap();
    enc.write_image(&[120u8; 8 * 8 * 3], 8, 8, image::ExtendedColorType::Rgb8).unwrap();
    let out = d.join("p3.webp");
    image_job::compress(&src, &out, 75.0, 0).unwrap();
    let reader = std::io::BufReader::new(std::fs::File::open(&out).unwrap());
    let mut back = image::codecs::webp::WebPDecoder::new(reader).unwrap();
    assert_eq!(back.icc_profile().unwrap(), Some(profile));
    assert_eq!(image::open(&out).unwrap().to_rgb8().dimensions(), (8, 8));
}

#[test]
fn the_frame_rate_cap_is_only_asked_for_when_chosen() {
    let info = ffmpeg::InputInfo::default();
    let cap = |fps| ffmpeg::args(Kind::Video, Path::new("a.mp4"), &ffmpeg::Sources::default(), Path::new("b.webm"), &Settings { video_max_fps: fps, ..settings() }, &info);
    assert!(cap(30).windows(2).any(|w| w[0] == "-fpsmax" && w[1] == "30"));
    assert!(!cap(0).iter().any(|a| a == "-fpsmax"));
}

#[test]
fn leftovers_from_a_crash_are_swept_but_nothing_else() {
    let d = temp("sweep");
    let (part, mine) = (d.join("a.webp.sastra-part"), d.join("mine.txt"));
    std::fs::write(&part, b"x").unwrap();
    std::fs::write(&mine, b"x").unwrap();
    let journal = d.join("running.txt");
    std::fs::write(&journal, format!("{}\n{}", part.display(), mine.display())).unwrap();
    crate::runner::sweep(&journal);
    assert!(!part.exists());
    assert!(mine.exists(), "only our own temporary files are ever deleted");
    assert!(!journal.exists());
}

#[test]
fn replacing_puts_the_original_in_the_bin_and_the_webp_in_its_place() {
    let d = temp("replace");
    let src = d.join("cg.png");
    image::RgbImage::from_fn(64, 64, |x, y| image::Rgb([x as u8, y as u8, 0])).save(&src).unwrap();
    let jobs = vec![Job { id: "1".into(), path: src.to_string_lossy().into(), rel: "cg.png".into(), kind: Kind::Image }];
    let s = Settings { save_mode: SaveMode::Replace, ..settings() };
    let seen = Mutex::new(vec![]);
    work(jobs, &s, &Arc::new(AtomicBool::new(false)), &|u| seen.lock().unwrap().push(u));
    let seen = seen.into_inner().unwrap();
    assert!(matches!(seen.last(), Some(Update::Done { .. })), "{seen:?}");
    assert!(!src.exists(), "the original should be in the bin");
    assert!(d.join("cg.webp").exists());
    assert!(!d.join("cg.webp.sastra-part").exists());
}
