#![allow(dead_code)]
#![allow(non_camel_case_types)]
extern crate libc;
use libc::{c_void, c_int, c_uint};

pub enum dssim_image { }
pub enum dssim_attr { }

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
    pub data: *mut f32,
}

pub type dssim_row_callback =
    extern "C" fn(channels: *const *mut f32, num_channels: c_int,
                  y: c_int, width: c_int, user_data: *mut c_void) -> ();
extern "C" {
    pub fn dssim_create_attr() -> *mut dssim_attr;
    pub fn dssim_dealloc_attr(arg1: *mut dssim_attr) -> ();
    pub fn dssim_set_scales(attr: *mut dssim_attr, num: c_int, weights: *const f64) -> ();
    pub fn dssim_set_save_ssim_maps(arg1: *mut dssim_attr,
                                    num_scales: c_uint,
                                    num_channels: c_uint) -> ();
    pub fn dssim_pop_ssim_map(arg1: *mut dssim_attr,
                              scale_index: c_uint,
                              channel_index: c_uint)
                              -> dssim_ssim_map;
    pub fn dssim_set_color_handling(arg1: *mut dssim_attr,
                                    subsampling: c_int,
                                    color_weight: f64) -> ();
    pub fn dssim_create_image(arg1: *mut dssim_attr,
                              row_pointers: *const *const u8,
                              color_type: dssim_colortype,
                              width: c_int, height: c_int,
                              gamma: f64) -> *mut dssim_image;
    pub fn dssim_create_image_float_callback(arg1: *mut dssim_attr,
                                             num_channels: c_int,
                                             width: c_int,
                                             height: c_int,
                                             cb: dssim_row_callback,
                                             callback_user_data: *mut c_void)
                                             -> *mut dssim_image;
    pub fn dssim_dealloc_image(arg1: *mut dssim_image) -> ();
    pub fn dssim_compare(arg1: *mut dssim_attr, original: *const dssim_image,
                         modified: *mut dssim_image) -> f64;
}
