//! End-to-end checks of the compressing itself. The ffmpeg tests need an
//! `ffmpeg` on the PATH (any build with x264 and libopus); they are skipped
//! when there is none.

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

fn have_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok()
}

fn make(path: &Path, args: &[&str]) {
    let ok = Command::new("ffmpeg").args(["-hide_banner", "-loglevel", "error", "-y"]).args(args).arg(path).status().unwrap();
    assert!(ok.success());
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
    ffmpeg::run(Kind::Audio, &src, &out, &settings(), &Arc::new(AtomicBool::new(false)), |p| seen.push(p)).unwrap();
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
    make(&src, &["-f", "lavfi", "-i", "testsrc2=size=1921x1081:rate=24:duration=1", "-c:v", "libx264"]);
    let out = d.join("op.mp4");
    ffmpeg::run(Kind::Video, &src, &out, &settings(), &Arc::new(AtomicBool::new(false)), |_| {}).unwrap();
    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v", "-show_entries", "stream=width,height", "-of", "csv=p=0"])
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&probe.stdout).trim(), "1280,720");
}

#[test]
fn audio_file_without_sound_says_so() {
    if !have_ffmpeg() {
        return;
    }
    let d = temp("silent");
    let src = d.join("silent.mp4");
    make(&src, &["-f", "lavfi", "-i", "testsrc2=size=64x64:duration=1", "-c:v", "libx264"]);
    let err = ffmpeg::run(Kind::Audio, &src, &d.join("x.ogg"), &settings(), &Arc::new(AtomicBool::new(false)), |_| {}).unwrap_err();
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

#[test]
fn every_listed_image_format_compresses() {
    let d = temp("formats");
    for ext in crate::formats::IMAGE {
        let src = d.join(format!("a.{ext}"));
        image::RgbImage::from_fn(32, 32, |x, y| image::Rgb([(x * 7) as u8, (y * 7) as u8, 90])).save(&src).unwrap_or_else(|e| panic!("{ext}: {e}"));
        let out = d.join(format!("a-{ext}.webp"));
        image_job::compress(&src, &out, 75.0, 0).unwrap_or_else(|e| panic!("{ext}: {e}"));
        assert_eq!(image::open(&out).unwrap().to_rgb8().dimensions(), (32, 32), "{ext}");
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
    let cap = |fps| ffmpeg::args(Kind::Video, Path::new("a.mp4"), Path::new("b.mp4"), &Settings { video_max_fps: fps, ..settings() });
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
