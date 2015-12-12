#![allow(improper_ctypes)]
#![allow(dead_code)]
#![allow(non_camel_case_types)]

extern crate libc;
use ::libc::{c_int, c_uint, c_void, c_char};
use ::dssim::Dssim;
use ::dssim::DssimImage;

const MAX_SCALES: usize = 5;

#[repr(C)]
pub struct dssim_chan {
    pub width: c_int,
    pub height: c_int,
    pub img: *mut c_void,// px
    pub mu: *mut dssim_px_t,
    pub img_sq_blur: *mut dssim_px_t,
    pub is_chroma: c_char,
}

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

#[repr(C)]
#[derive(Copy, Clone)]
pub struct dssim_ssim_map {
    pub width: c_int,
    pub height: c_int,
    pub ssim: f64,
    pub data: *mut dssim_px_t,
}

pub type dssim_row_callback =
    extern "C" fn(channels: *const *mut dssim_px_t, num_channels: c_int,
                  y: c_int, width: c_int, user_data: *mut c_void) -> ();
extern "C" {
    pub fn dssim_set_scales(attr: &mut Dssim, num: c_int, weights: *const f64) -> ();
    pub fn dssim_set_save_ssim_maps(arg1: &mut Dssim,
                                    num_scales: c_uint,
                                    num_channels: c_uint) -> ();
    pub fn dssim_pop_ssim_map(arg1: &mut Dssim,
                              scale_index: c_uint,
                              channel_index: c_uint)
                              -> dssim_ssim_map;
    pub fn dssim_set_color_handling(arg1: &mut Dssim,
                                    subsampling: c_int,
                                    color_weight: f64) -> ();
    pub fn dssim_init_image(arg1: &mut Dssim, arg2: &mut DssimImage,
                              row_pointers: *const *const u8,
                              color_type: dssim_colortype,
                              width: c_int, height: c_int,
                              gamma: f64) -> c_int;

    pub fn dssim_dealloc_image(arg1: &mut DssimImage) -> ();
    pub fn dssim_compare_channel(orig: &dssim_chan, modif: &mut dssim_chan, tmp: *mut dssim_px_t,
      ssim_map_out: *mut dssim_ssim_map, save_ssim_map: c_char) -> f64;
}
