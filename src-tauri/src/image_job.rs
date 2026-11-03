//! Images are compressed in-process (no ffmpeg): decode, turn upright,
//! shrink if asked, and encode as lossy WebP.

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader};
use std::path::Path;

pub fn compress(input: &Path, output: &Path, quality: f32, max_width: u32) -> Result<(), String> {
    let reader = ImageReader::open(input)
        .map_err(|e| format!("Couldn't open the file: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("Couldn't read the file: {e}"))?;
    let mut decoder = reader.into_decoder().map_err(read_error)?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(read_error)?;
    // Photos from phones are often stored sideways with a note to turn them.
    img.apply_orientation(orientation);

    if max_width > 0 && img.width() > max_width {
        let h = ((img.height() as f64) * (max_width as f64) / (img.width() as f64)).round().max(1.0) as u32;
        img = img.resize_exact(max_width, h, FilterType::Lanczos3);
    }

    let (w, h) = img.dimensions();
    if w > 16383 || h > 16383 {
        return Err(format!("WebP can't be larger than 16.383 px on a side; this image is {w} × {h} px."));
    }

    let keep_alpha = img.color().has_alpha() && img.to_rgba8().pixels().any(|p| p.0[3] < 255);
    let rgb;
    let rgba;
    let encoder = if keep_alpha {
        rgba = img.to_rgba8();
        webp::Encoder::from_rgba(rgba.as_raw(), w, h)
    } else {
        rgb = img.to_rgb8();
        webp::Encoder::from_rgb(rgb.as_raw(), w, h)
    };

    let mut config = webp::WebPConfig::new().map_err(|_| "Couldn't set up the WebP encoder.".to_string())?;
    config.quality = quality;
    config.method = 6; // slowest and smallest; images are quick anyway
    config.use_sharp_yuv = 1; // keeps thin coloured lines (outlines, text) crisp
    config.alpha_quality = 100;
    let data = encoder
        .encode_advanced(&config)
        .map_err(|e| format!("Couldn't encode the WebP: {e:?}"))?;
    std::fs::write(output, &*data).map_err(|e| format!("Couldn't save the file: {e}"))
}

fn read_error(e: image::ImageError) -> String {
    match e {
        image::ImageError::Unsupported(_) => "Sastra Compressor can't read this kind of image.".into(),
        other => format!("This image looks damaged: {other}"),
    }
}
