use crate::Dssim;
use crate::ToRGBAPLU;
use imgref::ImgVec;
use rgb::RGBA8;

pub type DssimImage = crate::DssimImage<f32>;

/// Create new context for comparisons
#[no_mangle] pub extern fn dssim_new() -> *mut Dssim {
    let d = Box::new(crate::new());
    Box::into_raw(d)
}

/// Free the context
#[no_mangle] pub unsafe extern fn dssim_free(d: *mut Dssim) {
    if d.is_null() {
        return;
    }
    let _ = Box::from_raw(d);
}

/// Take sRGB RGBA pixels and preprocess them into image format that can be compared.
///
/// Pixels are copied. Returns NULL on error.
///
/// Call `dssim_free_image` to free memory when the image is no longer needed.
#[no_mangle] pub unsafe extern fn dssim_create_image_rgba(dssim: &mut Dssim, pixels: *const u8, width: u32, height: u32) -> *mut DssimImage {
    let width = width as usize;
    let height = height as usize;
    let pixels = std::slice::from_raw_parts(pixels as *const RGBA8, width * height);
    let pixels = pixels.to_rgbaplu();
    let img = ImgVec::new(pixels, width, height);
    match dssim.create_image(&img) {
        Some(img) => Box::into_raw(Box::new(img)),
        None => std::ptr::null_mut(),
    }
}

/// Free image data
#[no_mangle] pub unsafe extern fn dssim_free_image(img: *mut DssimImage) {
    if img.is_null() {
        return;
    }
    let _ = Box::from_raw(img);
}

/// Compare these two images.
///
/// `img1` can be reused for multiple comparisons.
///
/// Don't forget to free images and DSSIM context when done.
#[no_mangle] pub unsafe extern fn dssim_compare(dssim: &mut Dssim, img1: *const DssimImage, img2: *const DssimImage) -> f64 {
    let img1 = img1.as_ref().unwrap();
    let img2 = img2.as_ref().unwrap();
    let (val, _) = dssim.compare(img1, img2);
    val.into()
}

