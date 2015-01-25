extern crate libc;

pub use ffi::dssim_colortype::*;

use libc::c_int;
mod ffi;

#[allow(missing_copy_implementations)]
pub struct Dssim {
    handle: *mut ffi::dssim_attr,
}

#[allow(missing_copy_implementations)]
pub struct DssimImage<'mem_src> {
    handle: *mut ffi::dssim_image,
    _mem_marker: std::marker::PhantomData<&'mem_src ffi::dssim_image>,
}

pub type ColorType = ffi::dssim_colortype;

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

    pub fn set_scales(&mut self, scales: &[f64]) {
        unsafe {
            ffi::dssim_set_scales(self.handle, scales.len() as c_int, scales.as_ptr());
        }
    }

    pub fn create_image<'img, T>(&mut self, bitmap: &'img [T], color_type: ColorType, width: usize, stride: usize, gamma: f64) -> Option<DssimImage<'img>> {
        let pixel_size = std::mem::size_of::<T>();
        let min_stride = width * pixel_size;
        assert!(stride >= min_stride, "width {}, pixel {}, stride {}", width, pixel_size, stride);

        let bitmap_bytes: &'img [u8] = unsafe {
            std::slice::from_raw_parts(std::mem::transmute(bitmap.as_ptr()), pixel_size*bitmap.len())
        };

        let row_pointers: Vec<*const u8> = bitmap_bytes.chunks(stride).map(|row| {
            assert!(row.len() >= stride, "row is {}, bitmap {}, width {}*{}<={}", row.len(), bitmap.len(), width, pixel_size, stride);
            row.as_ptr()
        }).collect();


        let handle = unsafe {
            ffi::dssim_create_image(self.handle, row_pointers.as_ptr(), color_type, width as c_int, row_pointers.len() as c_int, gamma)
        };

        if handle.is_null() {
            None
        } else {
            Some(DssimImage::<'img> {
                handle: handle,
                _mem_marker: std::marker::PhantomData,
            })
        }
    }

    pub fn compare(&mut self, original: &DssimImage, modified: DssimImage) -> f64 {
        assert!(!self.handle.is_null());
        assert!(!original.handle.is_null());
        assert!(!modified.handle.is_null());
        unsafe {
            ffi::dssim_compare(self.handle, original.handle, modified.handle)
        }
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

impl<'a> Drop for DssimImage<'a> {
    fn drop(&mut self) {
        assert!(!self.handle.is_null());
        unsafe {
            ffi::dssim_dealloc_image(self.handle);
        }
    }
}
