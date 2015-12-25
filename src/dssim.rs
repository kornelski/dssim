#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
/*
 * © 2011-2015 Kornel Lesiński. All rights reserved.
 *
 * This file is part of DSSIM.
 *
 * DSSIM is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License
 * as published by the Free Software Foundation, either version 3
 * of the License, or (at your option) any later version.
 *
 * DSSIM is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the license along with DSSIM.
 * If not, see <http://www.gnu.org/licenses/agpl.txt>.
 */

extern crate libc;
extern crate itertools;
extern crate lodepng;

use self::itertools::Zip;
use ffi;
use std;
use image::*;
use std::marker;

use ::self::libc::{c_int, c_uint, size_t};
pub type Px = f32;

pub use val::Dssim as Val;

pub struct DssimChan {
    pub width: usize,
    pub height: usize,
    pub img: Vec<Px>,
    pub mu: Vec<Px>,
    pub img_sq_blur: Vec<Px>,
    pub is_chroma: bool,
}

struct DssimMapChan {
    scales: Vec<SsimMap>,
}

pub struct Dssim {
    color_weight: f64,
    scale_weights: Vec<f64>,
    subsample_chroma: bool,
    save_maps_scales: u8,
    save_maps_channels: u8,
    ssim_maps: Vec<DssimMapChan>,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct rgba {
    r:u8,g:u8,b:u8,a:u8,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct rgb {
    r:u8,g:u8,b:u8,
}

struct DssimChanScale {
    scales: Vec<DssimChan>,
}

pub struct DssimImage {
    chan: Vec<DssimChanScale>,
}

struct Bitmap<T> {
    bitmap: Vec<T>,
    width: usize,
    height: usize,
}
type GBitmap = Bitmap<f32>;
type RGBAPLUBitmap = Bitmap<RGBAPLU>;

enum Converted {
    Gray(GBitmap),
    LAB((GBitmap, GBitmap, GBitmap)),
}

const D65x: f64 = 0.9505;
const D65y: f64 = 1.0;
const D65z: f64 = 1.089;

// #[allow(non_camel_case_types)]
// pub enum Gamma {
//     sRGB,
//     Pow(f64),
// }

/* Scales are taken from IW-SSIM, but this is not IW-SSIM algorithm */
const DEFAULT_WEIGHTS: [f64; 5] = [0.0448, 0.2856, 0.3001, 0.2363, 0.1333];

pub struct SsimMap {
    pub width: usize,
    pub height: usize,
    pub dssim: f64,
    pub data: Vec<Px>,
}

pub fn new() -> Dssim {
    Dssim::new()
}

impl DssimChan {
    pub fn new(bitmap: Vec<f32>, width: usize, height: usize, is_chroma: bool) -> DssimChan {
        DssimChan{
            img: bitmap,
            mu: Vec::with_capacity(width * height),
            img_sq_blur: Vec::new(),
            width: width,
            height: height,
            is_chroma: is_chroma,
        }
    }
}

impl Dssim {
    pub fn new() -> Dssim {
        Dssim {
            color_weight: 0.95,
            scale_weights: DEFAULT_WEIGHTS.iter().cloned().take(4).collect(),
            subsample_chroma: true,
            save_maps_scales: 0,
            save_maps_channels: 0,
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

    fn downsample(bitmap: &[RGBAPLU], width: usize, height: usize) -> Option<RGBAPLUBitmap> {
        if width < 8 || height < 8 {
            return None;
        }

        assert_eq!(width * height, bitmap.len());

        let half_height = height/2;
        let half_width = width/2;

        // crop odd pixels
        let bitmap = &bitmap[0..width * half_height * 2];

        let scaled:Vec<_> = bitmap.chunks(width * 2).flat_map(|pair|{
            let (top, bot) = pair.split_at(half_width * 2);
            let bot = &bot[0..half_width * 2];

            return top.chunks(2).zip(bot.chunks(2)).map(|(a,b)| RGBAPLU {
                r: (a[0].r + a[1].r + b[0].r + b[1].r) * 0.25,
                g: (a[0].g + a[1].g + b[0].g + b[1].g) * 0.25,
                b: (a[0].b + a[1].b + b[0].b + b[1].b) * 0.25,
                a: (a[0].a + a[1].a + b[0].a + b[1].a) * 0.25,
            })
        }).collect();

        assert_eq!(half_width * half_height, scaled.len());
        return Some(RGBAPLUBitmap{bitmap:scaled, width:half_width, height:half_height});
    }

    fn unzip3<A, B, C, I, FromA, FromB, FromC>(iter: I) -> (FromA, FromB, FromC) where
        FromA: Default + Extend<A>,
        FromB: Default + Extend<B>,
        FromC: Default + Extend<C>,
        I: Sized + Iterator<Item=(A, B, C)>,
    {
        struct SizeHint<A>(usize, Option<usize>, marker::PhantomData<A>);
        impl<A> Iterator for SizeHint<A> {
            type Item = A;

            fn next(&mut self) -> Option<A> { None }
            fn size_hint(&self) -> (usize, Option<usize>) {
                (self.0, self.1)
            }
        }

        let (lo, hi) = iter.size_hint();
        let mut ts: FromA = Default::default();
        let mut us: FromB = Default::default();
        let mut vs: FromC = Default::default();

        ts.extend(SizeHint(lo, hi, marker::PhantomData));
        us.extend(SizeHint(lo, hi, marker::PhantomData));
        vs.extend(SizeHint(lo, hi, marker::PhantomData));

        for (t, u, v) in iter {
            ts.extend(Some(t));
            us.extend(Some(u));
            vs.extend(Some(v));
        }

        (ts, us, vs)
    }


    fn to_rgb(rgba: &[RGBAPLU], width: usize, _: usize) -> Vec<RGBLU> {
        let mut x=11; // offset so that block-based compressors don't align
        let mut y=11;
        let rgb:Vec<_> = rgba.iter().map(|px|{
            let n = x ^ y;
            if x >= width {
                x=0;
                y+=1;
            }
            x += 1;
            let mut r = px.r;
            let mut g = px.g;
            let mut b = px.b;
            let a = px.a;
            if a < 255.0 {
                if (n & 16) != 0 {
                    r += 1.0 - a;
                }
                if (n & 8) != 0 {
                    g += 1.0 - a; // assumes premultiplied alpha
                }
                if (n & 32) != 0 {
                    b += 1.0 - a;
                }
            }

            RGBLU {
                r: r,
                g: g,
                b: b,
            }
        }).collect();

        return rgb;
    }

    fn to_luma(rgb: &[RGBLU], width: usize, height: usize) -> GBitmap {
        GBitmap{
            bitmap: rgb.iter().map(|px| {
                let fy = ((px.r as f64 * 0.2126 + px.g as f64 * 0.7152 + px.b as f64 * 0.0722) / D65y) as f32;

                let epsilon: f32 = 216.0 / 24389.0;
                // http://www.brucelindbloom.com/LContinuity.html
                let Y = if fy > epsilon {fy.powf(1.0f32 / 3.0f32) - 16.0f32/116.0f32} else {((24389.0 / 27.0) / 116.0) * fy};

                return Y * 1.16;
            }).collect(),
            width: width,
            height: height,
        }
    }

    fn to_lab(rgb: &[RGBLU], width: usize, height: usize) -> (GBitmap, GBitmap, GBitmap) {
        let (l,a,b) = Self::unzip3(rgb.iter().map(|px| {

            let fx = ((px.r as f64 * 0.4124 + px.g as f64 * 0.3576 + px.b as f64 * 0.1805) / D65x) as f32;
            let fy = ((px.r as f64 * 0.2126 + px.g as f64 * 0.7152 + px.b as f64 * 0.0722) / D65y) as f32;
            let fz = ((px.r as f64 * 0.0193 + px.g as f64 * 0.1192 + px.b as f64 * 0.9505) / D65z) as f32;

            let epsilon: f32 = 216.0 / 24389.0;
            let k = ((24389.0 / 27.0) / 116.0) as f32; // http://www.brucelindbloom.com/LContinuity.html
            let X = if fx > epsilon {fx.powf(1.0f32 / 3.0f32) - 16.0f32/116.0f32} else {k * fx};
            let Y = if fy > epsilon {fy.powf(1.0f32 / 3.0f32) - 16.0f32/116.0f32} else {k * fy};
            let Z = if fz > epsilon {fz.powf(1.0f32 / 3.0f32) - 16.0f32/116.0f32} else {k * fz};

            return (
                Y * 1.16,
                (86.2/ 220.0 + 500.0/ 220.0 * (X - Y)), /* 86 is a fudge to make the value positive */
                (107.9/ 220.0 + 200.0/ 220.0 * (Y - Z)), /* 107 is a fudge to make the value positive */
            );
        }));

        return (
            GBitmap{bitmap:l, width:width, height:height},
            GBitmap{bitmap:a, width:width, height:height},
            GBitmap{bitmap:b, width:width, height:height},
        );
    }

    pub fn create_image(&mut self, bitmap: &[RGBAPLU], width: usize, height: usize) -> Option<DssimImage> {
        assert!(bitmap.len() >= width * height);

        let num_scales = self.scale_weights.len() + if self.subsample_chroma {1} else {0};

        let mut img = DssimImage {
            chan: (0..3).map(|_|DssimChanScale{
                scales: Vec::with_capacity(num_scales),
            }).collect(),
        };

        let mut scales: Vec<RGBAPLUBitmap> = Vec::with_capacity(num_scales);
        for _ in 0..num_scales {
            let s = {
                let (b, w, h) = if let Some(l) = scales.last() {
                    (&l.bitmap[..], l.width, l.height)
                } else {
                    (bitmap, width, height)
                };
                Self::downsample(b, w, h)
            };
            if let Some(s) = s {
                scales.push(s);
            } else {
                break;
            }
        }


        let mut converted = Vec::with_capacity(num_scales);
        if self.subsample_chroma {
            converted.push(Converted::Gray(Self::to_luma(&Self::to_rgb(bitmap, width, height), width, height)));
        } else {
            converted.push(Converted::LAB(Self::to_lab(&Self::to_rgb(bitmap, width, height), width, height)));
        }
        converted.extend(scales.drain(..).map(|s|{
            Converted::LAB(Self::to_lab(&Self::to_rgb(&s.bitmap[..], s.width, s.height), s.width, s.height))
        }));

        for c in converted.drain(..) {
            match c {
                Converted::Gray(l) => {
                    img.chan[0].scales.push(DssimChan::new(l.bitmap, l.width, l.height, false));
                }
                Converted::LAB((l,a,b)) => {
                    img.chan[0].scales.push(DssimChan::new(l.bitmap, l.width, l.height, false));
                    img.chan[1].scales.push(DssimChan::new(a.bitmap, a.width, a.height, true));
                    img.chan[2].scales.push(DssimChan::new(b.bitmap, b.width, b.height, true));
                }
            }
        }

        let mut tmp = Vec::with_capacity(width*height);
        unsafe {tmp.set_len(width*height)};

        for mut ch in img.chan.iter_mut() {
            for mut s in ch.scales.iter_mut() {
                Self::preprocess_channel(s, &mut tmp[..]);
            }
        }

        return Some(img);
    }

    fn preprocess_channel(chan: &mut DssimChan, tmp: &mut [Px]) {
        let width = chan.width;
        let height = chan.height;
        let tmp = &mut tmp[0..width*height];

        assert_eq!(chan.img.len(), chan.width * chan.height);
        assert!(width > 1);
        assert!(height > 1);

        unsafe {
            if chan.is_chroma {
                ffi::blur_in_place(chan.img[..].as_mut_ptr(), tmp.as_mut_ptr(), width as c_int, height as c_int);
            }

            chan.mu.reserve(chan.width * chan.height);
            chan.mu.set_len(chan.width * chan.height);
            ffi::blur(chan.img[..].as_ptr(), tmp.as_mut_ptr(), chan.mu[..].as_mut_ptr(), width as c_int, height as c_int);

            chan.img_sq_blur = chan.img.iter().cloned().map(|i|i*i).collect();
            ffi::blur_in_place(chan.img_sq_blur[..].as_mut_ptr(), tmp.as_mut_ptr(), width as c_int, height as c_int);
        }
    }

    /**
     Algorithm based on Rabah Mehdi's C++ implementation
     */
    pub fn compare(&mut self, original_image: &DssimImage, mut modified_image: DssimImage) -> Val {
        let width = original_image.chan[0].scales[0].width;
        let height = original_image.chan[0].scales[0].height;

        let mut tmp = Vec::with_capacity(width*height);
        unsafe {tmp.set_len(width*height)};

        let mut ssim_sum = 0.0;
        let mut weight_sum = 0.0;
        for ((ch, original), mut modified) in original_image.chan.iter().enumerate().zip(modified_image.chan.drain(..)) {

            let save_channel = self.save_maps_channels as usize > ch;
            while save_channel && self.ssim_maps.len() <= ch {
                self.ssim_maps.push(DssimMapChan{scales:Vec::with_capacity(self.save_maps_scales.into())});
            }

            for ((n, scale_weight), original, modified) in Zip::new((self.scale_weights.iter().cloned().enumerate(), original.scales.iter(), modified.scales.drain(..))) {

                let weight = if original.is_chroma {self.color_weight} else {1.0} * scale_weight;

                let save_maps = save_channel && self.save_maps_scales as usize > n;

                let (score, ssim_map) = Self::compare_channel(original, modified, &mut tmp[..], save_maps).unwrap();
                ssim_sum += weight * score;
                weight_sum += weight;

                if let Some(ssim_map) = ssim_map {
                    {let chan = &mut self.ssim_maps[ch];
                    while chan.scales.len() <= n {
                        chan.scales.push(SsimMap::new());
                    }}
                    self.ssim_maps[ch].scales[n] = ssim_map;
                }
            }
        }

        return to_dssim(ssim_sum / weight_sum).into();
    }

    fn compare_channel(original: &DssimChan, mut modified: DssimChan, tmp: &mut [f32], save_ssim_map: bool) -> Option<(f64, Option<SsimMap>)> {
        if original.width != modified.width || original.height != modified.height {
            return None;
        }

        let width = original.width;
        let height = original.height;

        let img1_img2_blur = get_img1_img2_blur(original, &mut modified.img, tmp);

        let c1 = 0.01 * 0.01;
        let c2 = 0.03 * 0.03;
        let mut ssim_sum: f64 = 0.0;

        // FIXME: slice https://users.rust-lang.org/t/how-to-zip-two-slices-efficiently/2048
        for (img1_img2_blur, mu1, mut mu2_in_map_out, img1_sq_blur, img2_sq_blur)
            in Zip::new((
                img1_img2_blur.iter().cloned(),
                original.mu.iter().cloned(),
                modified.mu.iter_mut(),
                original.img_sq_blur.iter().cloned(),
                modified.img_sq_blur.iter().cloned(),
            )) {
            let mu1: f64 = mu1.into();
            let mu2: f64 = (*mu2_in_map_out).into();
            let img1_sq_blur: f64 = img1_sq_blur.into();
            let img2_sq_blur: f64 = img2_sq_blur.into();
            let img1_img2_blur: f64 = img1_img2_blur.into();

            let mu1_sq = mu1*mu1;
            let mu2_sq = mu2*mu2;
            let mu1_mu2 = mu1*mu2;
            let sigma1_sq = img1_sq_blur - mu1_sq;
            let sigma2_sq = img2_sq_blur - mu2_sq;
            let sigma12 = img1_img2_blur - mu1_mu2;

            let ssim = (2.0 * mu1_mu2 + c1) * (2.0 * sigma12 + c2)
                          /
                          ((mu1_sq + mu2_sq + c1) * (sigma1_sq + sigma2_sq + c2));

            ssim_sum += ssim;

            if save_ssim_map {
                *mu2_in_map_out = ssim as Px;
            }
        }

        let ssim_avg = ssim_sum / (width * height) as f64;

        let ssim_map = if save_ssim_map {
            let ssimmap = modified.mu;
            Some(SsimMap{
                width: width,
                height: height,
                dssim: to_dssim(ssim_avg),
                data: ssimmap,
            })
        } else {
            None
        };

        return Some((ssim_avg, ssim_map));
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
            data: Vec::new(),
            dssim: 0.,
        }
    }

    pub fn data(&self) -> Option<&[Px]> {
        if self.width == 0 {return None;}
        return Some(&self.data[..]);
    }
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
pub extern "C" fn dssim_get_chan_mu_const(ch: &DssimChan) -> *const Px {
    ch.mu[..].as_ptr()
}

#[no_mangle]
pub extern "C" fn dssim_get_chan_img_const(ch: &DssimChan) -> *const Px {
    ch.img[..].as_ptr()
}

#[no_mangle]
pub extern "C" fn dssim_get_chan_img(ch: &mut DssimChan) -> *mut Px {
    ch.img[..].as_mut_ptr()
}

fn get_img1_img2_blur<'a>(original: &DssimChan, modified_img: &'a mut Vec<Px>, tmp: &mut [Px]) -> &'a mut [Px]
{
    for (mut img2, img1) in modified_img.iter_mut().zip(original.img.iter()) {
        *img2 *= *img1;
    }

    unsafe {
        ffi::blur_in_place(modified_img[..].as_mut_ptr(), tmp.as_mut_ptr(), original.width as c_int, original.height as c_int);
    }

    return &mut modified_img[..];
}

#[no_mangle]
pub extern "C" fn dssim_get_chan_img_sq_blur_const(ch: &DssimChan) -> *const Px {
    ch.img_sq_blur[..].as_ptr()
}

#[no_mangle]
pub extern "C" fn dssim_get_chan_img_sq_blur(ch: &mut DssimChan) -> *mut Px {
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

#[cfg(test)]
extern crate lodepng;

#[test]
fn png_compare() {
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
