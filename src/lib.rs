//! See the [dssim-core](https://lib.rs/dssim-core) crate if you'd like to use only the library part.
#![doc(html_logo_url = "https://kornel.ski/dssim/logo.png")]

use std::path::Path;
use load_image::*;
pub use dssim_core::*;
use imgref::*;

fn load(path: &Path) -> Result<ImgVec<RGBAPLU>, lodepng::Error> {
    let img = load_image::load_image(path, false)?;
    match img.bitmap {
        ImageData::RGB8(ref bitmap) => Ok(Img::new(bitmap.to_rgbaplu(), img.width, img.height)),
        ImageData::RGB16(ref bitmap) => Ok(Img::new(bitmap.to_rgbaplu(), img.width, img.height)),
        ImageData::RGBA8(ref bitmap) => Ok(Img::new(bitmap.to_rgbaplu(), img.width, img.height)),
        ImageData::RGBA16(ref bitmap) => Ok(Img::new(bitmap.to_rgbaplu(), img.width, img.height)),
        ImageData::GRAY8(ref bitmap) => Ok(Img::new(bitmap.to_rgbaplu(), img.width, img.height)),
        ImageData::GRAY16(ref bitmap) => Ok(Img::new(bitmap.to_rgbaplu(), img.width, img.height)),
        ImageData::GRAYA8(ref bitmap) => Ok(Img::new(bitmap.to_rgbaplu(), img.width, img.height)),
        ImageData::GRAYA16(ref bitmap) => Ok(Img::new(bitmap.to_rgbaplu(), img.width, img.height)),
    }
}

/// Load PNG or JPEG image from the given path. Applies color profiles and converts to sRGB.
#[inline]
pub fn load_image(attr: &Dssim, path: impl AsRef<Path>) -> Result<DssimImage<f32>, lodepng::Error> {
    let bitmap = load(path.as_ref())?;
    Ok(attr.create_image(&bitmap).expect("current implementation is infallible"))
}
