//! See the [dssim-core](https://lib.rs/dssim-core) crate if you'd like to use only the library part.
#![doc(html_logo_url = "https://kornel.ski/dssim/logo.png")]
#![allow(clippy::manual_range_contains)]

pub use dssim_core::*;
use imgref::*;
use load_image::*;
use std::path::Path;

fn load(attr: &Dssim, path: &Path) -> Result<DssimImage<f32>, lodepng::Error> {
    let img = load_image::load_path(path, false)?;
    Ok(match img.bitmap {
        ImageData::RGB8(ref bitmap) => attr.create_image(&Img::new(bitmap.to_rgblu(), img.width, img.height)),
        ImageData::RGB16(ref bitmap) => attr.create_image(&Img::new(bitmap.to_rgblu(), img.width, img.height)),
        ImageData::RGBA8(ref bitmap) => attr.create_image(&Img::new(bitmap.to_rgbaplu(), img.width, img.height)),
        ImageData::RGBA16(ref bitmap) => attr.create_image(&Img::new(bitmap.to_rgbaplu(), img.width, img.height)),
        ImageData::GRAY8(ref bitmap) => attr.create_image(&Img::new(bitmap.to_rgblu(), img.width, img.height)),
        ImageData::GRAY16(ref bitmap) => attr.create_image(&Img::new(bitmap.to_rgblu(), img.width, img.height)),
        ImageData::GRAYA8(ref bitmap) => attr.create_image(&Img::new(bitmap.to_rgbaplu(), img.width, img.height)),
        ImageData::GRAYA16(ref bitmap) => attr.create_image(&Img::new(bitmap.to_rgbaplu(), img.width, img.height)),
    }.expect("infallible"))
}

/// Load PNG or JPEG image from the given path. Applies color profiles and converts to sRGB.
#[inline]
pub fn load_image(attr: &Dssim, path: impl AsRef<Path>) -> Result<DssimImage<f32>, lodepng::Error> {
    load(attr, path.as_ref())
}
