//! Pictures only Windows can read here: HEIF and HEIC, through the Windows Imaging
//! Component and Microsoft's HEIF and HEVC extensions (the HEVC patents are
//! Microsoft's to license). The picture comes out upright with its colour profile;
//! phone photos are usually Display P3, which looks dull without it.

use crate::decode::Failure;
use image::{DynamicImage, RgbaImage};
use std::path::Path;
use windows::core::HSTRING;
use windows::Win32::Foundation::{GENERIC_READ, WINCODEC_ERR_COMPONENTNOTFOUND};
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};

pub fn read_picture(path: &Path) -> Result<(DynamicImage, Option<Vec<u8>>), Failure> {
    let other = |e: windows::core::Error| Failure::Other(format!("Windows couldn't read this picture: {}", e.message()));
    // SAFETY: plain COM calls on interfaces this function owns; the pixel buffer is sized
    // for the stride and height passed with it. COM is started for this thread; a thread
    // that already started it differently keeps its own.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(other)?;
        // No codec for the file: the HEIF extension is missing. With HEIF but no HEVC,
        // opening works and decoding the pixels fails.
        let decoder = factory
            .CreateDecoderFromFilename(&HSTRING::from(path.as_os_str()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)
            .map_err(|e| if e.code() == WINCODEC_ERR_COMPONENTNOTFOUND { Failure::NoDecoder } else { other(e) })?;
        let frame = decoder.GetFrame(0).map_err(other)?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).map_err(other)?;
        let converter = factory.CreateFormatConverter().map_err(other)?;
        converter
            .Initialize(&frame, &GUID_WICPixelFormat32bppRGBA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)
            .map_err(other)?;
        let mut pixels = vec![0u8; w as usize * h as usize * 4];
        converter.CopyPixels(std::ptr::null(), w * 4, &mut pixels).map_err(|_| Failure::NoDecoder)?;
        let img = RgbaImage::from_raw(w, h, pixels).ok_or_else(|| Failure::Other("Windows gave a picture of the wrong size.".into()))?;
        Ok((DynamicImage::ImageRgba8(img), profile(&factory, &frame)))
    }
}

/// The first ICC profile the picture carries.
unsafe fn profile(factory: &IWICImagingFactory, frame: &IWICBitmapFrameDecode) -> Option<Vec<u8>> {
    let mut count = 0u32;
    frame.GetColorContexts(&mut [], &mut count).ok()?;
    let mut contexts: Vec<Option<IWICColorContext>> = (0..count).map(|_| factory.CreateColorContext().ok()).collect();
    if contexts.iter().any(Option::is_none) {
        return None;
    }
    frame.GetColorContexts(&mut contexts, &mut count).ok()?;
    contexts.into_iter().flatten().find_map(|c| {
        if c.GetType().ok()? != WICColorContextProfile {
            return None;
        }
        let mut len = 0u32;
        c.GetProfileBytes(&mut [], &mut len).ok()?;
        let mut bytes = vec![0u8; len as usize];
        c.GetProfileBytes(&mut bytes, &mut len).ok()?;
        Some(bytes)
    })
}
