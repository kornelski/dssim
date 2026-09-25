//! See the [dssim-core](https://lib.rs/dssim-core) crate if you'd like to use only the library part.
#![doc(html_logo_url = "https://kornel.ski/dssim/logo.png")]
#![allow(clippy::manual_range_contains)]

pub use dssim_core::*;
use imgref::Img;
use load_image::ImageData;
use std::path::Path;

fn load(attr: &Dssim, path: &Path, options: ImageOptions) -> Result<DssimImage<f32>, load_image::Error> {
    let img = load_image::load_path(path)?;
    Ok(match img.bitmap {
        ImageData::RGB8(ref bitmap) => attr.create_image_rgb_with_options(bitmap, img.width, img.height, options),
        ImageData::RGB16(ref bitmap) => attr.create_image_with_options(&Img::new(bitmap.to_rgblu(), img.width, img.height), options),
        ImageData::RGBA8(ref bitmap) => attr.create_image_rgba_with_options(bitmap, img.width, img.height, options),
        ImageData::RGBA16(ref bitmap) => attr.create_image_with_options(&Img::new(bitmap.to_rgbaplu(), img.width, img.height), options),
        ImageData::GRAY8(ref bitmap) => attr.create_image_with_options(&Img::new(bitmap.to_rgblu(), img.width, img.height), options),
        ImageData::GRAY16(ref bitmap) => attr.create_image_with_options(&Img::new(bitmap.to_rgblu(), img.width, img.height), options),
        ImageData::GRAYA8(ref bitmap) => attr.create_image_with_options(&Img::new(bitmap.to_rgbaplu(), img.width, img.height), options),
        ImageData::GRAYA16(ref bitmap) => attr.create_image_with_options(&Img::new(bitmap.to_rgbaplu(), img.width, img.height), options),
    }.expect("infallible"))
}

/// Load PNG or JPEG image from the given path. Applies color profiles and converts to `sRGB`.
#[inline]
pub fn load_image(attr: &Dssim, path: impl AsRef<Path>) -> Result<DssimImage<f32>, load_image::Error> {
    load_image_with_options(attr, path, ImageOptions::default())
}

/// Like [`load_image`], with per-image preparation options.
/// Use [`ImageOptions::cache_for_reuse`] for an image reused in many comparisons.
#[inline]
pub fn load_image_with_options(attr: &Dssim, path: impl AsRef<Path>, options: ImageOptions) -> Result<DssimImage<f32>, load_image::Error> {
    load(attr, path.as_ref(), options)
}
