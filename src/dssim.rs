extern crate libc;

use ffi;
use std;
pub use ffi::dssim_ssim_map;
pub use ffi::dssim_colortype::*;
pub use ffi::dssim_rgba as rgba;
pub use ffi::DSSIM_SRGB_GAMMA as SRGB_GAMMA;

use ::self::libc::{c_int, c_uint, size_t};

pub use val::Dssim as Val;

pub struct DssimChan {
    pub width: usize,
    pub height: usize,
    pub img: Vec<ffi::dssim_px_t>,
    pub mu: Vec<ffi::dssim_px_t>,
    pub img_sq_blur: Vec<ffi::dssim_px_t>,
    pub is_chroma: bool,
}

struct DssimMapChan {
    scales: Vec<SsimMap>,
}

pub struct Dssim {
    tmp: Vec<ffi::dssim_px_t>,
    color_weight: f64,
    scale_weights: Vec<f64>,
    subsample_chroma: bool,
    save_maps_scales: u8,
    save_maps_channels: u8,
    ssim_maps: Vec<DssimMapChan>,
}

pub struct DssimChanScale {
    scales: Vec<DssimChan>,
}

pub struct DssimImage<'mem_src> {
    chan: Vec<DssimChanScale>,
    _mem_marker: std::marker::PhantomData<&'mem_src u8>,
}

/* Scales are taken from IW-SSIM, but this is not IW-SSIM algorithm */
const DEFAULT_WEIGHTS: [f64; 5] = [0.0448, 0.2856, 0.3001, 0.2363, 0.1333];

pub type ColorType = ffi::dssim_colortype;

pub type SsimMap = ffi::dssim_ssim_map;

pub fn new() -> Dssim {
    Dssim::new()
}

impl Dssim {
    pub fn new() -> Dssim {
        Dssim {
            color_weight: 0.95,
            scale_weights: DEFAULT_WEIGHTS.iter().cloned().take(4).collect(),
            subsample_chroma: true,
            save_maps_scales: 0,
            save_maps_channels: 0,
            tmp: Vec::new(),
            ssim_maps: Vec::new(),
        }
    }

    pub fn set_scales(&mut self, scales: &[f64]) {
        self.scale_weights = scales.to_vec();
    }

    pub fn set_save_ssim_maps(&mut self, num_scales: u8, num_channels: u8) {
        self.save_maps_scales = num_scales;
        self.save_maps_channels = num_channels;

        self.ssim_maps.reserve(num_channels.into());
    }

    pub fn set_color_handling(&mut self, subsample_chroma: bool, color_weight: f64) {
        self.subsample_chroma = subsample_chroma;
        self.color_weight = color_weight;
    }

    pub fn ssim_map(&mut self, scale_index: usize, channel_index: usize) -> Option<&SsimMap> {
        if self.ssim_maps.len() <= channel_index {
            return None;
        }

        let chan = &self.ssim_maps[channel_index];
        if chan.scales.len() <= scale_index {
            return None;
        }

        return Some(&chan.scales[scale_index]);
    }

    pub fn create_image<'img, T>(&mut self, bitmap: &'img [T], color_type: ColorType, width: usize, stride: usize, gamma: f64) -> Option<DssimImage<'img>> {
        let pixel_size = std::mem::size_of::<T>();
        let min_stride = width * pixel_size;
        assert!(stride >= min_stride, "width {} * pixel {}, stride {} >= {}?", width, pixel_size, min_stride, stride);

        let bitmap_bytes: &'img [u8] = unsafe {
            std::slice::from_raw_parts(std::mem::transmute(bitmap.as_ptr()), pixel_size*bitmap.len())
        };

        let row_pointers: Vec<*const u8> = bitmap_bytes.chunks(stride).map(|row| {
            assert!(row.len() >= stride, "row is {}, bitmap {}, width {}*{}<={}", row.len(), bitmap.len(), width, pixel_size, stride);
            row.as_ptr()
        }).collect();


        let mut img = DssimImage::<'img> {
            chan: Vec::with_capacity(3),
            _mem_marker: std::marker::PhantomData,
        };

        unsafe {
            if 0 != ffi::dssim_init_image(self, &mut img, row_pointers.as_ptr(), color_type, width as c_int, row_pointers.len() as c_int, gamma) {
                Some(img)
            } else {
                None
            }
        }
    }

    /**
     Algorithm based on Rabah Mehdi's C++ implementation

     @param modified is destroyed after the comparison (but you still need to call dssim_dealloc_image)
     @param ssim_map_out Saves dissimilarity visualisation (pass NULL if not needed)
     @return DSSIM value or NaN on error.
     */
    pub fn compare(&mut self, original_image: &DssimImage, mut modified_image: DssimImage) -> Val {

        let channels = std::cmp::min(original_image.chan.len(), modified_image.chan.len());

        let tmp = dssim_get_tmp(self, (original_image.chan[0].scales[0].width as usize * original_image.chan[0].scales[0].height as usize * std::mem::size_of::<ffi::dssim_px_t>()));

        let mut ssim_sum = 0.0;
        let mut weight_sum = 0.0;
        for ch in 0 .. channels as usize {
            let w = self.scale_weights.clone();
            for (n, scale_weight) in w.iter().cloned().enumerate() {
                let original = &original_image.chan[ch].scales[n];
                let mut modified = &mut modified_image.chan[ch].scales[n];

                let weight = if original.is_chroma {self.color_weight} else {1.0} * scale_weight;

                let save_maps = self.save_maps_scales as usize > n && self.save_maps_channels as usize > ch;
                let score = weight * unsafe{ffi::dssim_compare_channel(original, modified, tmp, dssim_create_ssim_map(self, ch, n), if save_maps {1} else {0})};
                ssim_sum += score;
                weight_sum += weight;
                println!("chan {} wei {} {}x{} = {}", ch, n, original.width, original.height, score);
            }
        }

        return to_dssim(ssim_sum / weight_sum).into();
    }
}

fn to_dssim(ssim: f64) -> f64 {
    debug_assert!(ssim > 0.0);
    return 1.0 / ssim.min(1.0) - 1.0;
}

impl SsimMap {
    pub fn new() -> SsimMap {
        SsimMap {
            width: 0,
            height: 0,
            data: std::ptr::null_mut(),
            ssim: 0.,
        }
    }

    pub fn data(&self) -> Option<&[ffi::dssim_px_t]> {
        if self.data.is_null() {return None;}
        unsafe {
            Some(std::slice::from_raw_parts(self.data, self.width as usize * self.height as usize))
        }
    }
}

#[no_mangle]
pub extern "C" fn dssim_get_tmp(attr: &mut Dssim, size: size_t) -> *mut ffi::dssim_px_t {
    attr.tmp.reserve((size as usize + 3) / 4);
    (&mut attr.tmp[..]).as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn dssim_get_subsample_chroma(attr: &Dssim) -> c_int {
    attr.subsample_chroma as c_int
}

#[no_mangle]
pub extern "C" fn dssim_get_save_maps_scales(attr: &Dssim) -> c_int {
    attr.save_maps_scales.into()
}

#[no_mangle]
pub extern "C" fn dssim_get_save_maps_channels(attr: &Dssim) -> c_int {
    attr.save_maps_channels.into()
}

#[no_mangle]
pub extern "C" fn dssim_image_get_num_channels(img: &DssimImage) -> c_int {
    img.chan.len() as c_int
}

#[no_mangle]
pub extern "C" fn dssim_image_get_num_channel_scales(img: &DssimImage, ch: c_uint) -> c_int {
    img.chan[ch as usize].scales.len() as c_int
}

#[no_mangle]
pub extern "C" fn dssim_get_chan_width(ch: &DssimChan) -> c_int {
    ch.width as c_int
}
#[no_mangle]
pub extern "C" fn dssim_get_chan_height(ch: &DssimChan) -> c_int {
    ch.height as c_int
}
#[no_mangle]
pub extern "C" fn dssim_get_chan_mu_const(ch: &DssimChan) -> *const ffi::dssim_px_t {
    ch.mu[..].as_ptr()
}

#[no_mangle]
pub extern "C" fn dssim_get_chan_img_const(ch: &DssimChan) -> *const ffi::dssim_px_t {
    ch.img[..].as_ptr()
}

#[no_mangle]
pub extern "C" fn dssim_get_chan_img(ch: &mut DssimChan) -> *mut ffi::dssim_px_t {
    ch.img[..].as_mut_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn get_img1_img2_blur(original: &DssimChan, modified: &mut DssimChan, tmp: *mut ffi::dssim_px_t) -> *const ffi::dssim_px_t
{
    for (mut img2, img1) in modified.img.iter_mut().zip(original.img.iter()) {
        *img2 *= *img1;
    }

    ffi::blur_in_place(modified.img[..].as_mut_ptr(), tmp, modified.width as c_int, modified.height as c_int);

    return modified.img[..].as_ptr();
}

#[no_mangle]
pub extern "C" fn dssim_get_chan_img_sq_blur_const(ch: &DssimChan) -> *const ffi::dssim_px_t {
    ch.img_sq_blur[..].as_ptr()
}

#[no_mangle]
pub extern "C" fn dssim_get_chan_img_sq_blur(ch: &mut DssimChan) -> *mut ffi::dssim_px_t {
    ch.img_sq_blur[..].as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn dssim_image_get_channel<'a>(img: &'a mut DssimImage, ch: c_uint, s: c_uint) -> &'a mut DssimChan {
    &mut img.chan[ch as usize].scales[s as usize]
}

#[no_mangle]
pub extern "C" fn dssim_get_scale_weights(attr: &Dssim, i: c_uint) -> f64 {
    attr.scale_weights[i as usize]
}

#[no_mangle]
pub extern "C" fn dssim_get_color_weight(attr: &Dssim) -> f64 {
    attr.color_weight
}

#[no_mangle]
pub unsafe extern "C" fn dssim_preprocess_channel(chan: &mut DssimChan, tmp: *mut ffi::dssim_px_t) {
    let width = chan.width as c_int;
    let height = chan.height as c_int;

    if chan.is_chroma {
        ffi::blur_in_place(chan.img[..].as_mut_ptr(), tmp, width, height);
    }

    ffi::blur(chan.img[..].as_ptr(), tmp, chan.mu[..].as_mut_ptr(), width, height);

    chan.img_sq_blur = chan.img.iter().cloned().map(|i|i*i).collect();
    ffi::blur_in_place(chan.img_sq_blur[..].as_mut_ptr(), tmp, width, height);
}


#[no_mangle]
pub extern "C" fn dssim_image_set_channels(attr: &Dssim, img: &mut DssimImage, width: c_int, height: c_int, num_channels: c_int, subsample_chroma: c_int) {
    let subsample_chroma = subsample_chroma != 0;

    for ch in 0..num_channels {
        let is_chroma = ch > 0;
        let width = if is_chroma && subsample_chroma {width/2} else {width};
        let height = if is_chroma && subsample_chroma {height/2} else {height};
        img.chan.push(create_chan(width, height, is_chroma, attr.scale_weights.len()));
    }
}

fn create_chan(width: c_int, height: c_int, is_chroma: bool, num_scales: usize) -> DssimChanScale {
    let mut width = width as usize;
    let mut height = height as usize;

    let mut scales = Vec::with_capacity(num_scales);
    for _ in 0..num_scales {
        scales.push(DssimChan{
            width: width,
            height: height,
            is_chroma: is_chroma,
            img: vec![0.0; width * height],
            mu: vec![0.0; width * height],
            img_sq_blur: Vec::new(), // keep empty
        });
        width /= 2;
        height /= 2;
        if width < 8 || height < 8 {
            break;
        }
    }

    DssimChanScale {
        scales: scales,
    }
}


fn dssim_create_ssim_map(attr: &mut Dssim, channel_index: usize, scale_index: usize) -> &mut SsimMap {
    while attr.ssim_maps.len() <= channel_index {
        let mut chan = DssimMapChan{scales:Vec::new()};
        chan.scales.reserve(attr.save_maps_scales.into());
        attr.ssim_maps.push(chan);
    }
    let chan = &mut attr.ssim_maps[channel_index];
    while chan.scales.len() <= scale_index {
        chan.scales.push(SsimMap::new());
    }

    (&mut chan.scales[scale_index])
}

#[cfg(test)]
extern crate lodepng;

#[test]
fn test() {
    let mut d = new();
    let file1 = lodepng::decode32_file("test1.png").unwrap();
    let file2 = lodepng::decode32_file("test2.png").unwrap();

    let img1 = d.create_image(file1.buffer.as_ref(), DSSIM_RGBA, file1.width, file1.width*4, 0.45455).unwrap();
    let img2 = d.create_image(file2.buffer.as_ref(), DSSIM_RGBA, file2.width, file2.width*4, 0.45455).unwrap();

    let res = d.compare(&img1, img2);
    assert!((0.015899 - res).abs() < 0.0001, "res is {}", res);
    assert!(res < 0.0160);
    assert!(0.0158 < res);

    let img1b = d.create_image(file1.buffer.as_ref(), DSSIM_RGBA, file1.width, file1.width*4, 0.45455).unwrap();
    let res = d.compare(&img1, img1b);

    assert!(0.000000000000001 > res);
    assert!(res < 0.000000000000001);
    assert_eq!(res, res);
}
