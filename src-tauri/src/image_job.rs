//! Images are compressed in-process (no ffmpeg): decode, turn upright,
//! shrink if asked, and encode as lossy WebP. Animated GIF, PNG, and WebP
//! become animated WebP, which every current browser plays.

use crate::runner::Note;
use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, DynamicImage, Frame, GenericImageView, ImageBuffer, ImageDecoder, ImageFormat, ImageReader, Limits, Rgba};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Mutex;

const GIB: u64 = 1024 * 1024 * 1024;
/// A WebP side can't be longer than this.
const MAX_SIDE: u32 = 16383;

pub fn compress(input: &Path, output: &Path, quality: f32, max_width: u32) -> Result<Option<Note>, String> {
    if is_animated_format(input)? {
        // Every frame is held in memory, twice over while encoding, so these go one at a time.
        static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let Some(animation) = read_animation(input)? else { return compress_still(input, output, quality, max_width) };
        encode_animation(animation, output, quality, max_width)?;
        return Ok(Some(Note { text: "Stays animated".into(), warn: false }));
    }
    compress_still(input, output, quality, max_width)
}

/// Only these formats can hold an animation; everything else skips the extra reading.
fn is_animated_format(input: &Path) -> Result<bool, String> {
    let format = ImageReader::open(input)
        .map_err(|e| format!("Couldn't open the file: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("Couldn't read the file: {e}"))?
        .format();
    Ok(match format {
        Some(ImageFormat::Gif) => true,
        Some(ImageFormat::Png) => PngDecoder::new(BufReader::new(File::open(input).map_err(|e| format!("Couldn't open the file: {e}"))?)).map_err(read_error)?.is_apng().map_err(read_error)?,
        Some(ImageFormat::WebP) => WebPDecoder::new(BufReader::new(File::open(input).map_err(|e| format!("Couldn't open the file: {e}"))?)).map_err(read_error)?.has_animation(),
        _ => false,
    })
}

fn compress_still(input: &Path, output: &Path, quality: f32, max_width: u32) -> Result<Option<Note>, String> {
    let mut reader = ImageReader::open(input)
        .map_err(|e| format!("Couldn't open the file: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("Couldn't read the file: {e}"))?;
    // A 16.383 px square takes about 1 GB; the library's 512 MB default stops well short of that.
    let mut limits = Limits::default();
    limits.max_alloc = Some(4 * GIB);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(read_error)?;
    let profile = decoder.icc_profile().ok().flatten().filter(|p| worth_keeping(p));
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(read_error)?;
    // Photos from phones are often stored sideways with a note to turn them.
    img.apply_orientation(orientation);
    let img = shrink(img, max_width);

    let (w, h) = img.dimensions();
    check_side(w, h)?;

    let keep_alpha = is_transparent(&img);
    let rgb;
    let rgba;
    let encoder = if keep_alpha {
        rgba = img.to_rgba8();
        webp::Encoder::from_rgba(rgba.as_raw(), w, h)
    } else {
        rgb = img.to_rgb8();
        webp::Encoder::from_rgb(rgb.as_raw(), w, h)
    };

    let mut config = config(quality)?;
    config.method = 6; // slowest and smallest; images are quick anyway
    let data = encoder
        .encode_advanced(&config)
        .map_err(|e| format!("Couldn't encode the WebP: {e:?}"))?;
    let saved = match profile {
        Some(p) => std::fs::write(output, with_profile(&data, &p, w, h, keep_alpha)),
        None => std::fs::write(output, &*data),
    };
    saved.map_err(|e| format!("Couldn't save the file: {e}"))?;
    Ok(None)
}

fn config(quality: f32) -> Result<webp::WebPConfig, String> {
    let mut config = webp::WebPConfig::new().map_err(|_| "Couldn't set up the WebP encoder.".to_string())?;
    config.quality = quality;
    config.use_sharp_yuv = 1; // keeps thin coloured lines (outlines, text) crisp
    config.alpha_quality = 100;
    Ok(config)
}

fn check_side(w: u32, h: u32) -> Result<(), String> {
    if w > MAX_SIDE || h > MAX_SIDE {
        return Err(format!("WebP can't be larger than 16.383 px on a side; this image is {w} × {h} px."));
    }
    Ok(())
}

fn is_transparent(img: &DynamicImage) -> bool {
    img.color().has_alpha() && img.to_rgba8().pixels().any(|p| p.0[3] < 255)
}

/// Never wider than asked, never enlarged. Colours are weighted by their opacity while
/// resizing; otherwise the hidden colour under transparent pixels (often black) bleeds
/// into the visible edge as a dark fringe.
fn shrink(img: DynamicImage, max_width: u32) -> DynamicImage {
    if max_width == 0 || img.width() <= max_width {
        return img;
    }
    let h = ((img.height() as f64) * (max_width as f64) / (img.width() as f64)).round().max(1.0) as u32;
    if !is_transparent(&img) {
        return img.resize_exact(max_width, h, FilterType::Lanczos3);
    }
    let mut weighted = img.to_rgba16();
    for p in weighted.pixels_mut() {
        let a = p.0[3] as u32;
        for c in &mut p.0[..3] {
            *c = (*c as u32 * a / 65535) as u16;
        }
    }
    let mut small: ImageBuffer<Rgba<u16>, Vec<u16>> = image::imageops::resize(&weighted, max_width, h, FilterType::Lanczos3);
    for p in small.pixels_mut() {
        let a = p.0[3] as u32;
        for c in &mut p.0[..3] {
            *c = if a == 0 { 0 } else { (*c as u32 * 65535 / a).min(65535) as u16 };
        }
    }
    DynamicImage::ImageRgba16(small)
}

/* ---------- animations ---------- */

struct Animation {
    frames: Vec<Frame>,
    /// How many times it plays; 0 is forever.
    loops: i32,
}

/// The frames of an animated image, or None if it's a still (a single frame counts as one).
fn read_animation(input: &Path) -> Result<Option<Animation>, String> {
    let open = || File::open(input).map(BufReader::new).map_err(|e| format!("Couldn't open the file: {e}"));
    let format = ImageReader::open(input)
        .map_err(|e| format!("Couldn't open the file: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("Couldn't read the file: {e}"))?
        .format();
    let (frames, count) = match format {
        Some(ImageFormat::Gif) => {
            let d = GifDecoder::new(open()?).map_err(read_error)?;
            let count = d.loop_count();
            (d.into_frames(), count)
        }
        Some(ImageFormat::Png) => {
            let d = PngDecoder::new(open()?).map_err(read_error)?;
            if !d.is_apng().map_err(read_error)? {
                return Ok(None);
            }
            let d = d.apng().map_err(read_error)?;
            let count = d.loop_count();
            (d.into_frames(), count)
        }
        Some(ImageFormat::WebP) => {
            let d = WebPDecoder::new(open()?).map_err(read_error)?;
            if !d.has_animation() {
                return Ok(None);
            }
            let count = d.loop_count();
            (d.into_frames(), count)
        }
        _ => return Ok(None),
    };
    let mut all = Vec::new();
    let mut held = 0u64;
    for frame in frames {
        let frame = frame.map_err(read_error)?;
        held += frame.buffer().as_raw().len() as u64;
        // Every frame is held whole while encoding.
        if held > 3 * GIB {
            return Err("This animation is too long or too large to compress here, so it was left as it is.".into());
        }
        all.push(frame);
    }
    if all.len() < 2 {
        return Ok(None);
    }
    let loops = match count {
        image::metadata::LoopCount::Infinite => 0,
        image::metadata::LoopCount::Finite(n) => i32::try_from(n.get()).unwrap_or(0),
    };
    Ok(Some(Animation { frames: all, loops }))
}

fn encode_animation(a: Animation, output: &Path, quality: f32, max_width: u32) -> Result<(), String> {
    let mut times = Vec::with_capacity(a.frames.len());
    let mut at = 0i32;
    let frames: Vec<_> = a
        .frames
        .into_iter()
        .map(|f| {
            times.push(at);
            at = at.saturating_add(duration_ms(&f));
            shrink(DynamicImage::ImageRgba8(f.into_buffer()), max_width).to_rgba8()
        })
        .collect();
    let (w, h) = frames[0].dimensions();
    check_side(w, h)?;

    let mut config = config(quality)?;
    config.method = 4; // method 6 costs far too much when repeated for every frame
    let mut encoder = webp::AnimEncoder::new(w, h, &config);
    encoder.set_loop_count(a.loops);
    for (frame, time) in frames.iter().zip(&times) {
        encoder.add_frame(webp::AnimFrame::from_rgba(frame.as_raw(), w, h, *time));
    }
    let data = encoder.try_encode().map_err(|e| format!("Couldn't encode the WebP: {e:?}"))?;
    std::fs::write(output, &*data).map_err(|e| format!("Couldn't save the file: {e}"))
}

/// Browsers show frames marked 10 ms or less at 100 ms, and many GIFs rely on it.
fn duration_ms(f: &Frame) -> i32 {
    let (n, d) = f.delay().numer_denom_ms();
    let ms = n / d.max(1);
    if ms <= 10 { 100 } else { i32::try_from(ms).unwrap_or(i32::MAX) }
}

/* ---------- colour profiles ---------- */

/// Colours in a wide-gamut image (Display P3, Adobe RGB) shift if its profile is lost.
/// sRGB is what browsers assume anyway, so its profile would be wasted bytes.
fn worth_keeping(p: &[u8]) -> bool {
    let rgb = p.get(16..20) == Some(b"RGB ".as_slice());
    let names_srgb = p.windows(4).any(|w| w == b"sRGB") || p.windows(8).any(|w| w == b"s\0R\0G\0B\0" || w == b"\0s\0R\0G\0B");
    rgb && !names_srgb
}

/// Adds a colour profile to a finished WebP: an ICCP chunk directly after a VP8X header.
fn with_profile(webp: &[u8], profile: &[u8], w: u32, h: u32, alpha: bool) -> Vec<u8> {
    const ICC_FLAG: u8 = 0x20;
    const ALPHA_FLAG: u8 = 0x10;
    let body = &webp[12..];
    let mut out = Vec::with_capacity(webp.len() + profile.len() + 40);
    out.extend_from_slice(b"RIFF\0\0\0\0WEBP");
    let rest = if body.starts_with(b"VP8X") {
        out.extend_from_slice(&body[..18]);
        out[12 + 8] |= ICC_FLAG;
        &body[18..]
    } else {
        out.extend_from_slice(b"VP8X");
        out.extend_from_slice(&10u32.to_le_bytes());
        out.extend_from_slice(&[ICC_FLAG | if alpha { ALPHA_FLAG } else { 0 }, 0, 0, 0]);
        out.extend_from_slice(&(w - 1).to_le_bytes()[..3]);
        out.extend_from_slice(&(h - 1).to_le_bytes()[..3]);
        body
    };
    out.extend_from_slice(b"ICCP");
    out.extend_from_slice(&(profile.len() as u32).to_le_bytes());
    out.extend_from_slice(profile);
    if profile.len() % 2 == 1 {
        out.push(0);
    }
    out.extend_from_slice(rest);
    let size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    out
}

fn read_error(e: image::ImageError) -> String {
    match e {
        image::ImageError::Unsupported(_) => "Sastra Compressor can't read this kind of image.".into(),
        image::ImageError::Limits(_) => "This image is too large to open.".into(),
        other => format!("This image looks damaged: {other}"),
    }
}
