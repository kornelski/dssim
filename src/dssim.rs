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
use blur;
use image::*;
use std;

pub use val::Dssim as Val;

pub struct DssimChan<T> {
    pub width: usize,
    pub height: usize,
    pub img: Vec<T>,
    pub mu: Vec<T>,
    pub img_sq_blur: Vec<T>,
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

struct DssimChanScale<T> {
    scales: Vec<DssimChan<T>>,
}

pub struct DssimImage<T> {
    chan: Vec<DssimChanScale<T>>,
}

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
    pub data: Vec<f32>,
}

pub fn new() -> Dssim {
    Dssim::new()
}

impl<T> DssimChan<T> {
    pub fn new(bitmap: Vec<T>, width: usize, height: usize, is_chroma: bool) -> DssimChan<T> {
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

impl DssimChan<f32> {
    fn preprocess(&mut self, tmp: &mut [f32]) {
        let width = self.width;
        let height = self.height;
        let tmp = &mut tmp[0..width*height];

        assert_eq!(self.img.len(), self.width * self.height);
        assert!(width > 1);
        assert!(height > 1);

        unsafe {
            if self.is_chroma {
                blur::blur_in_place(&mut self.img[..], tmp, width, height);
            }

            self.mu.reserve(self.width * self.height);
            self.mu.set_len(self.width * self.height);
            blur::blur(&self.img[..], tmp, &mut self.mu[..], width, height);

            self.img_sq_blur = self.img.iter().cloned().map(|i|i*i).collect();
            blur::blur_in_place(&mut self.img_sq_blur[..], tmp, width, height);
        }
    }
}

impl DssimChan<LAB> {
    fn img1_img2_blur<'a>(&self, modified_img: &'a mut Vec<LAB>, tmp: &mut [LAB]) -> &'a mut [LAB] {
    use image::unzip3::Unzip3;
        let (mut l,mut a,mut b):(Vec<f32>,Vec<f32>,Vec<f32>) = modified_img.iter().zip(self.img.iter()).map(|(img2,img1)|{
            (img2.l * img1.l,
             img2.a * img1.a,
             img2.b * img1.b)
        }).unzip3();

        let mut tmp32 = unsafe {
            std::slice::from_raw_parts_mut(tmp[..].as_mut_ptr() as *mut f32, self.width * self.height)
        };

        blur::blur_in_place(&mut l[..], tmp32, self.width, self.height);
        blur::blur_in_place(&mut a[..], tmp32, self.width, self.height);
        blur::blur_in_place(&mut b[..], tmp32, self.width, self.height);

        for (mut out, l,a,b) in Zip::new((modified_img.iter_mut(), l.iter(), a.iter(), b.iter())) {
            *out = LAB{l:*l,a:*a,b:*b};
        }

        return &mut modified_img[..];
    }

}

impl Dssim {
    pub fn new() -> Dssim {
        Dssim {
            color_weight: 0.95,
            scale_weights: DEFAULT_WEIGHTS.iter().cloned().take(4).collect(),
            subsample_chroma: false,
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

    pub fn create_image<T>(&mut self, bitmap: &[T], width: usize, height: usize) -> Option<DssimImage<f32>>
        where [T]: ToLABBitmap + Downsample<T>
    {
        let num_scales = self.scale_weights.len() + if self.subsample_chroma {1} else {0};

        let mut img = DssimImage {
            chan: (0..3).map(|_|DssimChanScale{
                scales: Vec::with_capacity(num_scales),
            }).collect(),
        };

        let mut scales: Vec<Bitmap<T>> = Vec::with_capacity(num_scales);
        for _ in 0..num_scales {
            let s = if let Some(l) = scales.last() {
                    (&l.bitmap[..]).downsample(l.width, l.height)
                } else {
                    bitmap.downsample(width, height)
                };
            if let Some(s) = s {
                scales.push(s);
            } else {
                break;
            }
        }


        let mut converted = Vec::with_capacity(num_scales);
        if self.subsample_chroma {
            converted.push(Converted::Gray(bitmap.to_luma(width, height)));
        } else {
            converted.push(Converted::LAB(bitmap.to_lab(width, height)));
        }
        converted.extend(scales.drain(..).map(|s|{
            Converted::LAB((&s.bitmap[..]).to_lab(s.width, s.height))
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
                s.preprocess(&mut tmp[..]);
            }
        }

        return Some(img);
    }

    /**
     Algorithm based on Rabah Mehdi's C++ implementation
     */
    pub fn compare(&mut self, original_image: &DssimImage<f32>, modified_image: DssimImage<f32>) -> Val {
        let width = original_image.chan[0].scales[0].width;
        let height = original_image.chan[0].scales[0].height;

        let mut tmp = Vec::with_capacity(width*height);
        unsafe {tmp.set_len(width*height)};

        let mut ssim_sum = 0.0;
        let mut weight_sum = 0.0;

        let save_channel = self.save_maps_channels > 0;
        if save_channel {
            self.ssim_maps.push(DssimMapChan{scales:Vec::with_capacity(self.save_maps_scales.into())});
        }

        for (n, weight) in self.scale_weights.iter().cloned().enumerate() {
            let save_maps = save_channel && self.save_maps_scales as usize > n;

            let original_lab = Self::lab_chan(&original_image.chan[0].scales[n], &original_image.chan[1].scales[n], &original_image.chan[2].scales[n]);
            let mut modified_lab = Self::lab_chan(&modified_image.chan[0].scales[n], &modified_image.chan[1].scales[n], &modified_image.chan[2].scales[n]);

            let mut ssim_map = Self::compare_channel(&original_lab, &mut modified_lab, &mut tmp[..]);

            let half = avgworst(&ssim_map.data[..], ssim_map.width, ssim_map.height);
            let half = avg(&half.bitmap[..], half.width, half.height);
            let half = worst(&half.bitmap[..], half.width, half.height);

            let sum = half.bitmap.iter().fold(0.,|sum,i|sum+ *i as f64);
            let score = sum / (half.bitmap.len() as f64);

            ssim_map.data = half.bitmap;
            ssim_map.width = half.width;
            ssim_map.height = half.height;

            ssim_sum += weight * score;
            weight_sum += weight;

            if save_maps {
                {let chan = &mut self.ssim_maps[0];
                while chan.scales.len() <= n {
                    chan.scales.push(SsimMap::new());
                }}
                self.ssim_maps[0].scales[n] = ssim_map;
            }
        }

        return to_dssim(ssim_sum / weight_sum).into();
    }

    fn lab_chan(l: &DssimChan<f32>, a: &DssimChan<f32>, b: &DssimChan<f32>) -> DssimChan<LAB> {
        assert_eq!(l.width, a.width);
        assert_eq!(b.width, a.width);
        DssimChan {
            img_sq_blur: Zip::new((l.img_sq_blur.iter(), a.img_sq_blur.iter(), b.img_sq_blur.iter())).map(|(l,a,b)|LAB{l:*l,a:*a,b:*b}).collect(),
            img: Zip::new((l.img.iter(), a.img.iter(), b.img.iter())).map(|(l,a,b)|LAB{l:*l,a:*a,b:*b}).collect(),
            mu: Zip::new((l.mu.iter(), a.mu.iter(), b.mu.iter())).map(|(l,a,b)|LAB{l:*l,a:*a,b:*b}).collect(),
            is_chroma: false,
            width: l.width,
            height: l.height,
        }
    }

    fn compare_channel(original: &DssimChan<LAB>, mut modified: &mut DssimChan<LAB>, tmp: &mut [LAB]) -> SsimMap {
        assert_eq!(original.width, modified.width);
        assert_eq!(original.height, modified.height);

        let width = original.width;
        let height = original.height;

        let img1_img2_blur = original.img1_img2_blur(&mut modified.img, tmp);

        let c1 = 0.01 * 0.01;
        let c2 = 0.03 * 0.03;

        let mut map_out = Vec::with_capacity(width*height);
        unsafe {map_out.set_len(width*height)};

        // FIXME: slice https://users.rust-lang.org/t/how-to-zip-two-slices-efficiently/2048
        for (img1_img2_blur, mu1, mu2, img1_sq_blur, img2_sq_blur, mut map_out)
            in Zip::new((
                img1_img2_blur.iter().cloned(),
                original.mu.iter().cloned(),
                modified.mu.iter().cloned(),
                original.img_sq_blur.iter().cloned(),
                modified.img_sq_blur.iter().cloned(),
                map_out.iter_mut(),
            )) {

            let mu1_sq = (mu1*mu1).avg();
            let mu2_sq = (mu2*mu2).avg();
            let mu1_mu2 = (mu1*mu2).avg();
            let sigma1_sq = img1_sq_blur - (mu1*mu1);
            let sigma2_sq = img2_sq_blur - (mu2*mu2);
            let sigma12 = img1_img2_blur - (mu1*mu2);

            let ssim = (2. * mu1_mu2 + c1) * (2. * sigma12 + c2)
                        /
                       ((mu1_sq + mu2_sq + c1) * (sigma1_sq + sigma2_sq + c2));

            *map_out = ssim;
            }

        return SsimMap{
                width: width,
                height: height,
            dssim: -1.,
            data: map_out,
        };
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

    pub fn data(&self) -> Option<&[f32]> {
        if self.width == 0 {return None;}
        return Some(&self.data[..]);
    }
}

/*

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

*/
