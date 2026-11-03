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
        save_mode: SaveMode::Beside,
        output_folder: None,
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
    let u = Update::Done { id: "a".into(), out_path: "b".into(), out_size: 3 };
    assert_eq!(serde_json::to_string(&u).unwrap(), r#"{"state":"done","id":"a","outPath":"b","outSize":3}"#);
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
    assert!(!d.join("cg.webp.part").exists());
}
