#![allow(improper_ctypes)]
#![allow(dead_code)]
#![allow(non_camel_case_types)]

extern crate libc;
use ::libc::{c_int, c_uint, c_void, c_char};
use ::dssim::Dssim;
use ::dssim::DssimImage;
use ::dssim::DssimChan;

const MAX_SCALES: usize = 5;


pub type dssim_px_t = f32;

pub const DSSIM_SRGB_GAMMA:f64 = -47571492.0;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct dssim_rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub enum dssim_colortype {
    DSSIM_GRAY = 1,
    DSSIM_RGB = 2,
    DSSIM_RGBA = 3,
    DSSIM_LUMA = 4,
    DSSIM_LAB = 5,
    DSSIM_RGBA_TO_GRAY = 35,
}

pub type dssim_row_callback =
    extern "C" fn(channels: *const *mut dssim_px_t, num_channels: c_int,
                  y: c_int, width: c_int, user_data: *mut c_void) -> ();
extern "C" {
    pub fn dssim_init_image(arg1: &mut Dssim, arg2: &mut DssimImage,
                              row_pointers: *const *const u8,
                              color_type: dssim_colortype,
                              width: c_int, height: c_int,
                              gamma: f64) -> c_int;


    pub fn blur_in_place(srcdst:  *mut dssim_px_t, tmp:  *mut dssim_px_t,
                 width: c_int, height: c_int);
    pub fn blur(src:  *const dssim_px_t, tmp:  *mut dssim_px_t, dst:  *mut dssim_px_t,
                 width: c_int, height: c_int);

}
