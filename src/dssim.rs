//! ```rust,ignore
//! let mut d = dssim::new();
//!
//! let img1 = d.new_image(file1.buffer.as_ref(), dssim::DSSIM_RGBA, file1.width, file1.width*4, 0.45455)?;
//! let img2 = d.new_image(file2.buffer.as_ref(), dssim::DSSIM_RGBA, file2.width, file2.width*4, 0.45455)?;
//!
//! let res = d.compare(&img1, img2);
//! assert!(res < 0.0160);
//! ```

extern crate libc;

pub use ffi::dssim_colortype::*;
pub use ffi::DSSIM_SRGB_GAMMA;

use std::os::raw::{c_int, c_uint};
mod ffi;
mod val;

pub use val::Dssim as Val;

#[allow(missing_copy_implementations)]
/// Object holding settings
pub struct Dssim {
    handle: *mut ffi::dssim_attr,
}

#[allow(missing_copy_implementations)]
/// Object holding pixels
pub struct DssimImage<'mem_src> {
    handle: *mut ffi::dssim_image,
    _mem_marker: std::marker::PhantomData<&'mem_src ffi::dssim_image>,
}

/// `DSSIM_RGBA` or `DSSIM_RGB` or `DSSIM_GRAY`
pub type ColorType = ffi::dssim_colortype;

/// Per-pixel results
pub struct SsimMap<'a> {
    pub width: usize,
    pub height: usize,
    pub dssim: f64,
    pub ssim_map: &'a [ffi::dssim_px_t],
}

pub fn new() -> Dssim {
    Dssim::new()
}

impl Dssim {
    pub fn new() -> Dssim {
        unsafe {
            Dssim{
                handle: ffi::dssim_create_attr(),
            }
        }
    }

    /// Weights of how important each resolution is
    pub fn set_scales(&mut self, scales: &[f64]) {
        unsafe {
            ffi::dssim_set_scales(self.handle, scales.len() as c_int, scales.as_ptr());
        }
    }

    /// Enable saving of per-pixel results for each channel/res
    pub fn set_save_ssim_maps(&mut self, num_scales: u8, num_channels: u8) {
        unsafe {
            ffi::dssim_set_save_ssim_maps(self.handle, num_scales as c_uint, num_channels as c_uint);
        }
    }

    /// Read result of specific res/channel
    #[must_use]
    pub fn pop_ssim_map(&mut self, scale_index: u8, channel_index: u8) -> Option<SsimMap> {
        let m = unsafe {
            ffi::dssim_pop_ssim_map(self.handle, scale_index as c_uint, channel_index as c_uint)
        };
        if m.width <= 0 || m.height <= 0 || m.data.is_null() {
            return None;
        }
        return Some(SsimMap {
            width: m.width as usize,
            height: m.height as usize,
            dssim: m.dssim,
            ssim_map: unsafe {
                std::slice::from_raw_parts(m.data, m.width as usize * m.height as usize)
            },
        });
    }

    /// Describe pixel array
    ///
    /// Stride is in bytes
    ///
    /// 0 gamma means sRGB
    pub fn new_image<'img, T>(&mut self, bitmap: &'img [T], color_type: ColorType, width: usize, stride: usize, gamma: f64) -> Result<DssimImage<'img>, String> {
        let pixel_size = std::mem::size_of::<T>();
        let min_stride = width * pixel_size;
        if stride < min_stride {
            return Err(format!("width {} * pixel {} < stride {}", width, pixel_size, stride));
        }

        let bitmap_bytes: &'img [u8] = unsafe {
            std::slice::from_raw_parts(std::mem::transmute(bitmap.as_ptr()), pixel_size*bitmap.len())
        };

        let row_pointers: Vec<*const u8> = bitmap_bytes.chunks(stride).map(|row| {
            assert!(row.len() >= stride, "row is {}, bitmap {}, width {}*{}<={}", row.len(), bitmap.len(), width, pixel_size, stride);
            row.as_ptr()
        }).collect();


        let handle = unsafe {
            ffi::dssim_create_image(self.handle, row_pointers.as_ptr(), color_type, width as c_int, row_pointers.len() as c_int, if gamma > 0. {gamma} else {DSSIM_SRGB_GAMMA})
        };

        if handle.is_null() {
            Err("Unable to create image".to_owned())
        } else {
            Ok(DssimImage::<'img> {
                handle: handle,
                _mem_marker: std::marker::PhantomData,
            })
        }
    }

    #[must_use]
    pub fn compare(&mut self, original: &DssimImage, modified: DssimImage) -> Val {
        assert!(!self.handle.is_null());
        assert!(!original.handle.is_null());
        assert!(!modified.handle.is_null());
        unsafe {
            ffi::dssim_compare(self.handle, original.handle, modified.handle)
        }.into()
    }
}

impl Drop for Dssim {
    fn drop(&mut self) {
        assert!(!self.handle.is_null());
        unsafe {
            ffi::dssim_dealloc_attr(self.handle);
        }
    }
}

impl<'a> Drop for SsimMap<'a> {
    fn drop(&mut self) {
        unsafe {
            libc::free(self.ssim_map.as_ptr() as *mut _);
        }
    }
}

impl<'a> Drop for DssimImage<'a> {
    fn drop(&mut self) {
        assert!(!self.handle.is_null());
        unsafe {
            ffi::dssim_dealloc_image(self.handle);
        }
    }
}

#[cfg(test)]
extern crate lodepng;

#[test]
fn test() {
    let mut d = new();
    d.set_save_ssim_maps(1, 1);
    let file1 = lodepng::decode32_file("test1.png").unwrap();
    let file2 = lodepng::decode32_file("test2.png").unwrap();

    let img1 = d.new_image(file1.buffer.as_ref(), DSSIM_RGBA, file1.width, file1.width*4, 0.45455).unwrap();
    let img2 = d.new_image(file2.buffer.as_ref(), DSSIM_RGBA, file2.width, file2.width*4, 0.45455).unwrap();

    let res = d.compare(&img1, img2);
    assert!((0.015899 - res).abs() < 0.0001, "res is {}", res);
    assert!(res < 0.0160);
    assert!(0.0158 < res);

    let img1b = d.new_image(file1.buffer.as_ref(), DSSIM_RGBA, file1.width, file1.width*4, 0.45455).unwrap();
    let res = d.compare(&img1, img1b);

    assert!(d.pop_ssim_map(1, 1).is_none());
    let map = d.pop_ssim_map(0, 0).unwrap();
    assert_eq!(file1.width, map.width);
    assert_eq!(file1.height, map.height);

    assert!(0.000000000000001 > res);
    assert!(res < 0.000000000000001);
    assert_eq!(res, res);
}
