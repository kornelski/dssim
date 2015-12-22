#![allow(dead_code)]
#![allow(non_camel_case_types)]

extern crate libc;
use ::libc::{c_int, c_uint, c_void, c_char};

pub type dssim_px_t = f32;

extern "C" {
    pub fn blur_in_place(srcdst:  *mut dssim_px_t, tmp:  *mut dssim_px_t,
                 width: c_int, height: c_int);
    pub fn blur(src:  *const dssim_px_t, tmp:  *mut dssim_px_t, dst:  *mut dssim_px_t,
                 width: c_int, height: c_int);

}
