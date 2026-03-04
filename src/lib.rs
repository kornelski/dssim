//! See the [dssim-core](https://lib.rs/dssim-core) crate if you'd like to use only the library part.
#![doc(html_logo_url = "https://kornel.ski/dssim/logo.png")]
#![allow(clippy::manual_range_contains)]

pub use dssim_core::*;
use imgref::Img;
use std::path::Path;

pub mod load;

use load::PixelData;

fn load_impl(attr: &Dssim, path: &Path) -> Result<DssimImage<f32>, load::LoadError> {
    let (width, height, pixels) = load::load_path(path)?;
    Ok(match pixels {
        PixelData::Rgb8(ref bm) => attr.create_image(&Img::new(bm.to_rgblu(), width, height)),
        PixelData::Rgb16(ref bm) => attr.create_image(&Img::new(bm.to_rgblu(), width, height)),
        PixelData::Rgba8(ref bm) => attr.create_image(&Img::new(bm.to_rgbaplu(), width, height)),
        PixelData::Rgba16(ref bm) => attr.create_image(&Img::new(bm.to_rgbaplu(), width, height)),
        PixelData::Gray8(ref bm) => attr.create_image(&Img::new(bm.to_rgblu(), width, height)),
        PixelData::Gray16(ref bm) => attr.create_image(&Img::new(bm.to_rgblu(), width, height)),
        PixelData::GrayA8(ref bm) => attr.create_image(&Img::new(bm.to_rgbaplu(), width, height)),
        PixelData::GrayA16(ref bm) => attr.create_image(&Img::new(bm.to_rgbaplu(), width, height)),
    }
    .expect("infallible"))
}

/// Load PNG or JPEG image from the given path. Applies color profiles and converts to `sRGB`.
#[inline]
pub fn load_image(
    attr: &Dssim,
    path: impl AsRef<Path>,
) -> Result<DssimImage<f32>, load::LoadError> {
    load_impl(attr, path.as_ref())
}
