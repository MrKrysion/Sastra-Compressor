//! Reads the flattened copy that Photoshop keeps in a PSD or PSB ("Maximize
//! compatibility", on by default), without parsing the layers. That copy is
//! all a game needs, and it's the part every editor writes the same way.
//!
//! Layout (Adobe's file format specification): a 26-byte header; then three
//! sections that each start with their length (colour mode data, image resources,
//! layers and masks); then the image data: a compression word, then each channel
//! as a whole plane, raw or PackBits RLE with the length of every row first.
//!
//! When the document has transparency, the colours are stored as if drawn over
//! white, so they're un-matted here: `colour = (stored - (1 - alpha)) / alpha`.

use image::{DynamicImage, ImageBuffer, LumaA, Rgba};

/// The picture, and its colour profile if it has one.
pub fn read(bytes: &[u8]) -> Result<(DynamicImage, Option<Vec<u8>>), String> {
    let damaged = || "This Photoshop file looks damaged.".to_string();
    let mut r = Reader { b: bytes, at: 0 };
    if r.take(4).ok_or_else(damaged)? != b"8BPS" {
        return Err(damaged());
    }
    let big = match r.u16().ok_or_else(damaged)? {
        1 => false,
        2 => true,
        _ => return Err(damaged()),
    };
    r.take(6).ok_or_else(damaged)?;
    let channels = usize::from(r.u16().ok_or_else(damaged)?);
    let height = r.u32().ok_or_else(damaged)? as usize;
    let width = r.u32().ok_or_else(damaged)? as usize;
    let depth = r.u16().ok_or_else(damaged)?;
    let mode = r.u16().ok_or_else(damaged)?;
    let colours = match mode {
        3 => 3,
        1 => 1,
        _ => return Err("Only RGB and greyscale Photoshop files can be read. Change the image mode in Photoshop (Image › Mode) and save again.".into()),
    };
    if !matches!(depth, 8 | 16) {
        return Err("Only 8- and 16-bit Photoshop files can be read. Change it in Photoshop (Image › Mode) and save again.".into());
    }
    if width == 0 || height == 0 || channels < colours {
        return Err(damaged());
    }

    let mode_data = r.u32().ok_or_else(damaged)? as usize;
    r.take(mode_data).ok_or_else(damaged)?;
    let resources_len = r.u32().ok_or_else(damaged)? as usize;
    let profile = icc_profile(r.take(resources_len).ok_or_else(damaged)?);
    // Layers and masks: only the layer count's sign matters. Negative means the first
    // extra channel is the transparency of the flattened picture.
    let layers_len = if big { r.u64().ok_or_else(damaged)? as usize } else { r.u32().ok_or_else(damaged)? as usize };
    let layers = r.take(layers_len).ok_or_else(damaged)?;
    let transparent = channels > colours && layer_count(layers, big).is_some_and(|n| n < 0);

    let compression = r.u16().ok_or_else(damaged)?;
    let row = width * usize::from(depth / 8);
    let used = colours + usize::from(transparent);
    let planes = match compression {
        0 => (0..used).map(|i| r.rest().get(i * row * height..(i + 1) * row * height).map(<[u8]>::to_vec)).collect::<Option<Vec<_>>>(),
        1 => rle_planes(&mut r, channels, used, height, row, big),
        _ => return Err("This Photoshop file is saved with ZIP compression, which can't be read here. Save it again from Photoshop.".into()),
    }
    .ok_or_else(damaged)?;

    let sample = |p: &[u8], i: usize| -> f32 {
        if depth == 16 { f32::from(u16::from_be_bytes([p[2 * i], p[2 * i + 1]])) / 65535.0 } else { f32::from(p[i]) / 255.0 }
    };
    let pixels = width * height;
    let mut out: Vec<u16> = Vec::with_capacity(pixels * (colours + 1));
    for i in 0..pixels {
        let a = if transparent { sample(&planes[colours], i) } else { 1.0 };
        for plane in &planes[..colours] {
            let mut c = sample(plane, i);
            if transparent && a > 0.0 {
                c = ((c + a - 1.0) / a).clamp(0.0, 1.0);
            }
            out.push((c * 65535.0).round() as u16);
        }
        out.push((a * 65535.0).round() as u16);
    }
    let (w, h) = (width as u32, height as u32);
    let img = if colours == 3 {
        DynamicImage::ImageRgba16(ImageBuffer::<Rgba<u16>, _>::from_raw(w, h, out).ok_or_else(damaged)?)
    } else {
        DynamicImage::ImageLumaA16(ImageBuffer::<LumaA<u16>, _>::from_raw(w, h, out).ok_or_else(damaged)?)
    };
    Ok((img, profile))
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn rest(&self) -> &'a [u8] {
        &self.b[self.at..]
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|s| u16::from_be_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|s| u32::from_be_bytes(s.try_into().unwrap()))
    }
    fn u64(&mut self) -> Option<u64> {
        self.take(8).map(|s| u64::from_be_bytes(s.try_into().unwrap()))
    }
}

/// The layer count from the layers section, which starts with the layer info's length.
fn layer_count(section: &[u8], big: bool) -> Option<i16> {
    let skip = if big { 8 } else { 4 };
    section.get(skip..skip + 2).map(|s| i16::from_be_bytes([s[0], s[1]]))
}

/// Resource 1039 is the ICC profile. Each resource: "8BIM", id, a padded Pascal
/// name, then a length and data padded to an even size.
fn icc_profile(mut res: &[u8]) -> Option<Vec<u8>> {
    while res.len() >= 12 && &res[..4] == b"8BIM" {
        let id = u16::from_be_bytes([res[4], res[5]]);
        let name = (usize::from(res[6]) + 2) & !1;
        let at = 6 + name;
        let len = u32::from_be_bytes(res.get(at..at + 4)?.try_into().ok()?) as usize;
        let data = res.get(at + 4..at + 4 + len)?;
        if id == 1039 {
            return Some(data.to_vec());
        }
        res = res.get(at + 4 + ((len + 1) & !1)..)?;
    }
    None
}

/// PackBits planes: first every row's packed length (all channels), then the rows.
fn rle_planes(r: &mut Reader, channels: usize, used: usize, height: usize, row: usize, big: bool) -> Option<Vec<Vec<u8>>> {
    let mut lengths = Vec::with_capacity(channels * height);
    for _ in 0..channels * height {
        lengths.push(if big { r.u32()? as usize } else { usize::from(r.u16()?) });
    }
    let mut planes = Vec::with_capacity(used);
    for c in 0..used {
        let mut plane = Vec::with_capacity(row * height);
        for y in 0..height {
            unpack(r.take(lengths[c * height + y])?, &mut plane, row)?;
        }
        planes.push(plane);
    }
    Some(planes)
}

fn unpack(mut src: &[u8], out: &mut Vec<u8>, row: usize) -> Option<()> {
    let end = out.len() + row;
    while out.len() < end {
        let (&n, rest) = src.split_first()?;
        let n = n as i8;
        if n >= 0 {
            let count = n as usize + 1;
            out.extend_from_slice(rest.get(..count)?);
            src = &rest[count..];
        } else if n != -128 {
            let (&v, rest) = rest.split_first()?;
            out.extend(std::iter::repeat(v).take((1 - isize::from(n)) as usize));
            src = rest;
        } else {
            src = rest;
        }
    }
    (out.len() == end).then_some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2 × 1 RGB PSD, uncompressed, optionally with merged transparency.
    fn psd(transparent: bool, pixels: [[u8; 4]; 2]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(b"8BPS");
        b.extend(1u16.to_be_bytes());
        b.extend([0; 6]);
        b.extend((if transparent { 4u16 } else { 3 }).to_be_bytes());
        b.extend(1u32.to_be_bytes());
        b.extend(2u32.to_be_bytes());
        b.extend(8u16.to_be_bytes());
        b.extend(3u16.to_be_bytes());
        b.extend(0u32.to_be_bytes());
        // An ICC resource, with an odd-length name to check the padding.
        let mut res = Vec::new();
        res.extend(b"8BIM");
        res.extend(1039u16.to_be_bytes());
        res.extend([1, b'x']);
        res.extend(3u32.to_be_bytes());
        res.extend(b"icc\0");
        b.extend((res.len() as u32).to_be_bytes());
        b.extend(res);
        // Layers section: layer info length, then a layer count of -1 when transparent.
        let layers: Vec<u8> = [0u32.to_be_bytes().to_vec(), (if transparent { -1i16 } else { 1 }).to_be_bytes().to_vec()].concat();
        b.extend((layers.len() as u32).to_be_bytes());
        b.extend(layers);
        b.extend(0u16.to_be_bytes());
        for c in 0..if transparent { 4 } else { 3 } {
            b.extend(pixels.iter().map(|p| p[c]));
        }
        b
    }

    #[test]
    fn reads_the_flattened_picture_and_its_profile() {
        let (img, icc) = read(&psd(false, [[255, 0, 0, 0], [0, 0, 255, 0]])).unwrap();
        let px = img.to_rgba8();
        assert_eq!(px.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(px.get_pixel(1, 0).0, [0, 0, 255, 255]);
        assert_eq!(icc.as_deref(), Some(&b"icc"[..]));
    }

    #[test]
    fn transparent_colours_are_taken_off_the_white() {
        // Half-transparent pure red, stored over white as (255, 128, 128).
        let (img, _) = read(&psd(true, [[255, 128, 128, 128], [255, 255, 255, 0]])).unwrap();
        let px = img.to_rgba8();
        let [r, g, b, a] = px.get_pixel(0, 0).0;
        assert_eq!((r, a), (255, 128));
        assert!(g <= 2 && b <= 2, "{g} {b}");
        assert_eq!(px.get_pixel(1, 0).0[3], 0);
    }

    #[test]
    fn packbits_rows_unpack() {
        let mut out = Vec::new();
        unpack(&[0xFE, 7, 1, 9, 8], &mut out, 5).unwrap();
        assert_eq!(out, [7, 7, 7, 9, 8]);
    }

    #[test]
    fn other_modes_say_what_to_do() {
        let mut b = psd(false, [[0; 4]; 2]);
        b[25] = 4; // CMYK
        assert!(read(&b).unwrap_err().contains("Image › Mode"));
    }
}
