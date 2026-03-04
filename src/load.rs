use rgb::{ComponentBytes, FromSlice, Gray, GrayAlpha, RGB, RGBA};
use std::io::Cursor;
use std::path::Path;

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Png(png::DecodingError),
    Jpeg(zune_jpeg::errors::DecodeErrors),
    ColorProfile(moxcms::CmsError),
    Pnm(zenbitmaps::BitmapError),
    UnsupportedFormat,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Png(e) => write!(f, "PNG error: {e}"),
            Self::Jpeg(e) => write!(f, "JPEG error: {e}"),
            Self::ColorProfile(e) => write!(f, "Color profile error: {e}"),
            Self::Pnm(e) => write!(f, "PNM error: {e}"),
            Self::UnsupportedFormat => {
                write!(f, "Unsupported image format (expected PNG, JPEG, or PNM)")
            }
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Png(e) => Some(e),
            Self::Jpeg(e) => Some(e),
            Self::ColorProfile(e) => Some(e),
            Self::Pnm(e) => Some(e),
            Self::UnsupportedFormat => None,
        }
    }
}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<png::DecodingError> for LoadError {
    fn from(e: png::DecodingError) -> Self {
        Self::Png(e)
    }
}

impl From<zune_jpeg::errors::DecodeErrors> for LoadError {
    fn from(e: zune_jpeg::errors::DecodeErrors) -> Self {
        Self::Jpeg(e)
    }
}

impl From<moxcms::CmsError> for LoadError {
    fn from(e: moxcms::CmsError) -> Self {
        Self::ColorProfile(e)
    }
}

impl From<zenbitmaps::BitmapError> for LoadError {
    fn from(e: zenbitmaps::BitmapError) -> Self {
        Self::Pnm(e)
    }
}

pub enum PixelData {
    Rgb8(Vec<RGB<u8>>),
    Rgba8(Vec<RGBA<u8>>),
    Gray8(Vec<Gray<u8>>),
    GrayA8(Vec<GrayAlpha<u8>>),
    Rgb16(Vec<RGB<u16>>),
    Rgba16(Vec<RGBA<u16>>),
    Gray16(Vec<Gray<u16>>),
    GrayA16(Vec<GrayAlpha<u16>>),
}

fn is_graya_opaque_u8(pixels: &[GrayAlpha<u8>]) -> bool {
    pixels.iter().all(|p| p.a == 255)
}

fn is_graya_opaque_u16(pixels: &[GrayAlpha<u16>]) -> bool {
    pixels.iter().all(|p| p.a == 65535)
}

fn is_rgba_opaque_u8(pixels: &[RGBA<u8>]) -> bool {
    pixels.iter().all(|p| p.a == 255)
}

fn is_rgba_opaque_u16(pixels: &[RGBA<u16>]) -> bool {
    pixels.iter().all(|p| p.a == 65535)
}

pub fn load_path(path: &Path) -> Result<(usize, usize, PixelData), LoadError> {
    let data = std::fs::read(path)?;
    if data.starts_with(b"\x89PNG") {
        load_png(&data)
    } else if data.starts_with(&[0xFF, 0xD8]) {
        load_jpeg(&data)
    } else if data.len() >= 2
        && data[0] == b'P'
        && matches!(data[1], b'5' | b'6' | b'7' | b'f' | b'F')
    {
        load_pnm(&data)
    } else {
        Err(LoadError::UnsupportedFormat)
    }
}

fn load_png(data: &[u8]) -> Result<(usize, usize, PixelData), LoadError> {
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;

    let info = reader.info();
    let icc_profile = info.icc_profile.as_ref().map(|c| c.to_vec());
    let has_srgb_chunk = info.srgb.is_some();

    let (color_type, bit_depth) = reader.output_color_type();
    let buf_size = reader.output_buffer_size().unwrap_or(0);
    let width = info.width as usize;
    let height = info.height as usize;

    let mut buf = vec![0u8; buf_size];
    reader.next_frame(&mut buf)?;

    let is_16bit = bit_depth == png::BitDepth::Sixteen;

    // Apply ICC profile if present and no sRGB chunk
    let needs_icc = !has_srgb_chunk && icc_profile.is_some();
    let icc_data = icc_profile.as_deref();

    match (color_type, is_16bit) {
        (png::ColorType::Grayscale, false) => {
            let mut pixels: Vec<Gray<u8>> = buf[..width * height].as_gray().to_vec();
            if needs_icc {
                apply_icc_8bit(
                    pixels.as_mut_slice().as_bytes_mut(),
                    icc_data.unwrap(),
                    moxcms::Layout::Gray,
                )?;
            }
            Ok((width, height, PixelData::Gray8(pixels)))
        }
        (png::ColorType::Grayscale, true) => {
            let mut pixels = bytes_to_gray16_be(&buf, width * height);
            if needs_icc {
                apply_icc_16bit(
                    pixels.as_mut_slice().as_bytes_mut(),
                    icc_data.unwrap(),
                    moxcms::Layout::Gray,
                )?;
            }
            Ok((width, height, PixelData::Gray16(pixels)))
        }
        (png::ColorType::GrayscaleAlpha, false) => {
            let mut pixels: Vec<GrayAlpha<u8>> = buf[..width * height * 2].as_gray_alpha().to_vec();
            if is_graya_opaque_u8(&pixels) {
                // Strip alpha for fully-opaque images (matches load_image behavior)
                let mut gray: Vec<Gray<u8>> = pixels.iter().map(|p| Gray::new(p.v)).collect();
                if needs_icc {
                    apply_icc_8bit(
                        gray.as_mut_slice().as_bytes_mut(),
                        icc_data.unwrap(),
                        moxcms::Layout::Gray,
                    )?;
                }
                Ok((width, height, PixelData::Gray8(gray)))
            } else {
                if needs_icc {
                    apply_icc_8bit(
                        pixels.as_mut_slice().as_bytes_mut(),
                        icc_data.unwrap(),
                        moxcms::Layout::GrayAlpha,
                    )?;
                }
                Ok((width, height, PixelData::GrayA8(pixels)))
            }
        }
        (png::ColorType::GrayscaleAlpha, true) => {
            let mut pixels = bytes_to_graya16_be(&buf, width * height);
            if is_graya_opaque_u16(&pixels) {
                let mut gray: Vec<Gray<u16>> = pixels.iter().map(|p| Gray::new(p.v)).collect();
                if needs_icc {
                    apply_icc_16bit(
                        gray.as_mut_slice().as_bytes_mut(),
                        icc_data.unwrap(),
                        moxcms::Layout::Gray,
                    )?;
                }
                Ok((width, height, PixelData::Gray16(gray)))
            } else {
                if needs_icc {
                    apply_icc_16bit(
                        pixels.as_mut_slice().as_bytes_mut(),
                        icc_data.unwrap(),
                        moxcms::Layout::GrayAlpha,
                    )?;
                }
                Ok((width, height, PixelData::GrayA16(pixels)))
            }
        }
        (png::ColorType::Rgb, false) => {
            let mut pixels: Vec<RGB<u8>> = buf[..width * height * 3].as_rgb().to_vec();
            if needs_icc {
                apply_icc_8bit(
                    pixels.as_mut_slice().as_bytes_mut(),
                    icc_data.unwrap(),
                    moxcms::Layout::Rgb,
                )?;
            }
            Ok((width, height, PixelData::Rgb8(pixels)))
        }
        (png::ColorType::Rgb, true) => {
            let mut pixels = bytes_to_rgb16_be(&buf, width * height);
            if needs_icc {
                apply_icc_16bit(
                    pixels.as_mut_slice().as_bytes_mut(),
                    icc_data.unwrap(),
                    moxcms::Layout::Rgb,
                )?;
            }
            Ok((width, height, PixelData::Rgb16(pixels)))
        }
        (png::ColorType::Rgba, false) => {
            let mut pixels: Vec<RGBA<u8>> = buf[..width * height * 4].as_rgba().to_vec();
            if is_rgba_opaque_u8(&pixels) {
                let mut rgb: Vec<RGB<u8>> =
                    pixels.iter().map(|p| RGB::new(p.r, p.g, p.b)).collect();
                if needs_icc {
                    apply_icc_8bit(
                        rgb.as_mut_slice().as_bytes_mut(),
                        icc_data.unwrap(),
                        moxcms::Layout::Rgb,
                    )?;
                }
                Ok((width, height, PixelData::Rgb8(rgb)))
            } else {
                if needs_icc {
                    apply_icc_8bit(
                        pixels.as_mut_slice().as_bytes_mut(),
                        icc_data.unwrap(),
                        moxcms::Layout::Rgba,
                    )?;
                }
                Ok((width, height, PixelData::Rgba8(pixels)))
            }
        }
        (png::ColorType::Rgba, true) => {
            let mut pixels = bytes_to_rgba16_be(&buf, width * height);
            if is_rgba_opaque_u16(&pixels) {
                let mut rgb: Vec<RGB<u16>> =
                    pixels.iter().map(|p| RGB::new(p.r, p.g, p.b)).collect();
                if needs_icc {
                    apply_icc_16bit(
                        rgb.as_mut_slice().as_bytes_mut(),
                        icc_data.unwrap(),
                        moxcms::Layout::Rgb,
                    )?;
                }
                Ok((width, height, PixelData::Rgb16(rgb)))
            } else {
                if needs_icc {
                    apply_icc_16bit(
                        pixels.as_mut_slice().as_bytes_mut(),
                        icc_data.unwrap(),
                        moxcms::Layout::Rgba,
                    )?;
                }
                Ok((width, height, PixelData::Rgba16(pixels)))
            }
        }
        _ => Err(LoadError::UnsupportedFormat),
    }
}

fn load_jpeg(data: &[u8]) -> Result<(usize, usize, PixelData), LoadError> {
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;

    let mut decoder = zune_jpeg::JpegDecoder::new(ZCursor::new(data));
    decoder.decode_headers()?;

    let icc_profile = decoder.icc_profile();
    let (width, height) = decoder.dimensions().ok_or(LoadError::UnsupportedFormat)?;
    let input_cs = decoder
        .input_colorspace()
        .ok_or(LoadError::UnsupportedFormat)?;

    // For grayscale JPEGs, decode as Luma to match the GRAY ICC profile.
    // zune-jpeg defaults to RGB output even for 1-component images.
    if input_cs == ColorSpace::Luma {
        decoder.set_options(DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::Luma));
    }

    let mut pixels = decoder.decode()?;
    let output_cs = decoder
        .output_colorspace()
        .ok_or(LoadError::UnsupportedFormat)?;

    match output_cs {
        ColorSpace::Luma => {
            if let Some(ref icc_data) = icc_profile {
                apply_icc_8bit(&mut pixels, icc_data, moxcms::Layout::Gray)?;
            }
            let gray: Vec<Gray<u8>> = pixels.into_iter().map(Gray::new).collect();
            Ok((width, height, PixelData::Gray8(gray)))
        }
        ColorSpace::RGB => {
            if let Some(ref icc_data) = icc_profile {
                apply_icc_8bit(&mut pixels, icc_data, moxcms::Layout::Rgb)?;
            }
            let rgb: Vec<RGB<u8>> = pixels
                .chunks_exact(3)
                .map(|c| RGB::new(c[0], c[1], c[2]))
                .collect();
            Ok((width, height, PixelData::Rgb8(rgb)))
        }
        _ => Err(LoadError::UnsupportedFormat),
    }
}

fn load_pnm(data: &[u8]) -> Result<(usize, usize, PixelData), LoadError> {
    let decoded = zenbitmaps::decode(data, zenbitmaps::Unstoppable)?;
    let w = decoded.width as usize;
    let h = decoded.height as usize;
    let pixels = decoded.pixels();

    match decoded.layout {
        zenbitmaps::PixelLayout::Gray8 => Ok((w, h, PixelData::Gray8(pixels.as_gray().to_vec()))),
        zenbitmaps::PixelLayout::Rgb8 => Ok((w, h, PixelData::Rgb8(pixels.as_rgb().to_vec()))),
        zenbitmaps::PixelLayout::Rgba8 => {
            let rgba = pixels.as_rgba();
            if is_rgba_opaque_u8(rgba) {
                Ok((
                    w,
                    h,
                    PixelData::Rgb8(rgba.iter().map(|p| RGB::new(p.r, p.g, p.b)).collect()),
                ))
            } else {
                Ok((w, h, PixelData::Rgba8(rgba.to_vec())))
            }
        }
        zenbitmaps::PixelLayout::Gray16 => {
            // zenbitmaps provides native-endian u16
            let gray: Vec<Gray<u16>> = pixels
                .chunks_exact(2)
                .map(|c| Gray::new(u16::from_ne_bytes([c[0], c[1]])))
                .collect();
            Ok((w, h, PixelData::Gray16(gray)))
        }
        _ => Err(LoadError::UnsupportedFormat),
    }
}

/// ICC profile colorspace is at bytes 16-19
fn icc_is_gray(icc_data: &[u8]) -> bool {
    icc_data.len() >= 20 && &icc_data[16..20] == b"GRAY"
}

/// Create the appropriate sRGB-equivalent destination profile.
/// Gray ICC profiles need a gray destination; RGB profiles need sRGB.
fn srgb_destination(is_gray: bool) -> moxcms::ColorProfile {
    if is_gray {
        // Build a gray profile with the exact sRGB parametric TRC.
        // new_gray_with_gamma(2.2) is a poor approximation — sRGB uses a
        // piecewise curve: f(x) = (a*x+b)^g for x >= d, f(x) = e*x for x < d
        let mut profile = moxcms::ColorProfile::new_gray_with_gamma(2.2);
        profile.gray_trc = Some(moxcms::ToneReprCurve::Parametric(vec![
            2.4,
            1. / 1.055,
            0.055 / 1.055,
            1. / 12.92,
            0.04045,
        ]));
        profile
    } else {
        moxcms::ColorProfile::new_srgb()
    }
}

fn apply_icc_8bit(
    buf: &mut [u8],
    icc_data: &[u8],
    layout: moxcms::Layout,
) -> Result<(), LoadError> {
    let src = moxcms::ColorProfile::new_from_slice(icc_data)?;
    let dst = srgb_destination(icc_is_gray(icc_data));
    let transform =
        src.create_transform_8bit(layout, &dst, layout, moxcms::TransformOptions::default())?;
    let mut out = vec![0u8; buf.len()];
    transform.transform(buf, &mut out)?;
    buf.copy_from_slice(&out);
    Ok(())
}

fn apply_icc_16bit(
    buf: &mut [u8],
    icc_data: &[u8],
    layout: moxcms::Layout,
) -> Result<(), LoadError> {
    let src = moxcms::ColorProfile::new_from_slice(icc_data)?;
    let dst = srgb_destination(icc_is_gray(icc_data));
    let transform =
        src.create_transform_16bit(layout, &dst, layout, moxcms::TransformOptions::default())?;
    let pixel_count = buf.len() / 2;
    let src_u16: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_ne_bytes([c[0], c[1]]))
        .collect();
    let mut dst_u16 = vec![0u16; pixel_count];
    transform.transform(&src_u16, &mut dst_u16)?;
    for (chunk, &val) in buf.chunks_exact_mut(2).zip(dst_u16.iter()) {
        let bytes = val.to_ne_bytes();
        chunk[0] = bytes[0];
        chunk[1] = bytes[1];
    }
    Ok(())
}

// PNG stores 16-bit values in big-endian byte order.
// Convert to native-endian typed pixels.

fn bytes_to_gray16_be(buf: &[u8], count: usize) -> Vec<Gray<u16>> {
    buf[..count * 2]
        .chunks_exact(2)
        .map(|c| Gray::new(u16::from_be_bytes([c[0], c[1]])))
        .collect()
}

fn bytes_to_graya16_be(buf: &[u8], count: usize) -> Vec<GrayAlpha<u16>> {
    buf[..count * 4]
        .chunks_exact(4)
        .map(|c| {
            GrayAlpha::new(
                u16::from_be_bytes([c[0], c[1]]),
                u16::from_be_bytes([c[2], c[3]]),
            )
        })
        .collect()
}

fn bytes_to_rgb16_be(buf: &[u8], count: usize) -> Vec<RGB<u16>> {
    buf[..count * 6]
        .chunks_exact(6)
        .map(|c| {
            RGB::new(
                u16::from_be_bytes([c[0], c[1]]),
                u16::from_be_bytes([c[2], c[3]]),
                u16::from_be_bytes([c[4], c[5]]),
            )
        })
        .collect()
}

fn bytes_to_rgba16_be(buf: &[u8], count: usize) -> Vec<RGBA<u16>> {
    buf[..count * 8]
        .chunks_exact(8)
        .map(|c| {
            RGBA::new(
                u16::from_be_bytes([c[0], c[1]]),
                u16::from_be_bytes([c[2], c[3]]),
                u16::from_be_bytes([c[4], c[5]]),
                u16::from_be_bytes([c[6], c[7]]),
            )
        })
        .collect()
}
