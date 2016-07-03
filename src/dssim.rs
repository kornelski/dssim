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

use self::itertools::Zip;
use blur;
use image::*;
use std;
use std::ops;
pub use val::Dssim as Val;

trait Channable<T> {
    fn img1_img2_blur<'a>(&self, modified_img: &'a mut Vec<T>, tmp: &mut [T]) -> &'a mut [T];
}

pub struct DssimChan<T> {
    pub width: usize,
    pub height: usize,
    pub img: Vec<T>,
    pub mu: Vec<T>,
    pub img_sq_blur: Vec<T>,
    pub is_chroma: bool,
}

pub struct Dssim {
    scale_weights: Vec<f64>,
    save_maps_scales: u8,
    ssim_maps: Vec<SsimMap>,
}

struct DssimChanScale<T> {
    chan: Vec<DssimChan<T>>,
}

pub struct DssimImage<T> {
    scale: Vec<DssimChanScale<T>>,
}

// Scales are taken from IW-SSIM, but this is not IW-SSIM algorithm
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
        DssimChan {
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
        let tmp = &mut tmp[0..width * height];

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

            self.img_sq_blur = self.img.iter().cloned().map(|i| i * i).collect();
            blur::blur_in_place(&mut self.img_sq_blur[..], tmp, width, height);
        }
    }
}

impl Channable<LAB> for DssimChan<LAB> {
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
            scale_weights: DEFAULT_WEIGHTS.iter().cloned().take(4).collect(),
            save_maps_scales: 0,
            ssim_maps: Vec::new(),
        }
    }

    pub fn set_scales(&mut self, scales: &[f64]) {
        self.scale_weights = scales.to_vec();
    }

    pub fn set_save_ssim_maps(&mut self, num_scales: u8) {
        self.save_maps_scales = num_scales;

        self.ssim_maps.reserve(num_scales.into());
    }

    pub fn ssim_map(&mut self, scale_index: usize) -> Option<&SsimMap> {
        if self.ssim_maps.len() <= scale_index {
            return None;
        }

        return Some(&self.ssim_maps[scale_index]);
    }

    pub fn create_image<T>(&mut self, bitmap: &[T], width: usize, height: usize) -> Option<DssimImage<f32>>
        where [T]: ToLABBitmap + Downsample<T>
    {
        let num_scales = self.scale_weights.len();

        let mut img = DssimImage {
            scale: Vec::with_capacity(num_scales),
        };

        let mut downsampled: Vec<Bitmap<T>> = Vec::with_capacity(num_scales);
        for _ in 1..num_scales { // 1, because unscaled bitmap will be added
            let s = if let Some(l) = downsampled.last() {
                l.bitmap.downsample(l.width, l.height)
            } else {
                bitmap.downsample(width, height)
            };
            if let Some(s) = s {
                downsampled.push(s);
            } else {
                break;
            }
        }

        let all_sizes:Vec<_> = std::iter::once(BitmapRef::new(bitmap, width, height))
            .chain(downsampled.iter().map(|s| s.new_ref()))
            .collect();

        let mut tmp = Vec::with_capacity(width * height);
        unsafe { tmp.set_len(width * height) };

        for (l, a, b) in all_sizes.into_iter().map(|s| s.bitmap.to_lab(s.width, s.height)) {
            img.scale.push(DssimChanScale{
                chan: vec![
                    {let mut ch = DssimChan::new(l.bitmap, l.width, l.height, false); ch.preprocess(&mut tmp[..]); ch },
                    {let mut ch = DssimChan::new(a.bitmap, a.width, a.height, true); ch.preprocess(&mut tmp[..]); ch },
                    {let mut ch = DssimChan::new(b.bitmap, b.width, b.height, true); ch.preprocess(&mut tmp[..]); ch },
                ],
            });
        }

        return Some(img);
    }

    pub fn compare(&mut self, original_image: &DssimImage<f32>, modified_image: DssimImage<f32>) -> Val {
        let width = original_image.scale[0].chan[0].width;
        let height = original_image.scale[0].chan[0].height;

        let mut tmp = Vec::with_capacity(width * height);
        unsafe { tmp.set_len(width * height) };

        let mut ssim_sum = 0.0;
        let mut weight_sum = 0.0;

        for (n, weight) in self.scale_weights.iter().cloned().enumerate() {
            let save_maps = self.save_maps_scales as usize > n;

            let original_lab = Self::lab_chan(&original_image.scale[n]);
            let mut modified_lab = Self::lab_chan(&modified_image.scale[n]);

            let mut ssim_map = Self::compare_channel(&original_lab, &mut modified_lab, &mut tmp[..]);

            let half = avgworst(&ssim_map.data[..], ssim_map.width, ssim_map.height);
            let half = avg(&half.bitmap[..], half.width, half.height);
            let half = worst(&half.bitmap[..], half.width, half.height);

            let sum = half.bitmap.iter().fold(0., |sum, i| sum + *i as f64);
            let score = sum / (half.bitmap.len() as f64);

            ssim_map.data = half.bitmap;
            ssim_map.width = half.width;
            ssim_map.height = half.height;

            ssim_sum += weight * score;
            weight_sum += weight;

            if save_maps {
                while self.ssim_maps.len() <= n {
                    self.ssim_maps.push(SsimMap::new());
                }
                self.ssim_maps[n] = ssim_map;
            }
        }

        return to_dssim(ssim_sum / weight_sum).into();
    }

    fn lab_chan(scale: &DssimChanScale<f32>) -> DssimChan<LAB> {
        let l = &scale.chan[0];
        let a = &scale.chan[1];
        let b = &scale.chan[2];
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

    fn compare_channel<L>(original: &DssimChan<L>, mut modified: &mut DssimChan<L>, tmp: &mut [L]) -> SsimMap
        where DssimChan<L>: Channable<L>,
        L: Clone + Copy + ops::Mul<Output=L> + ops::Sub<Output=L>,
        f32: std::convert::From<L>
    {
        assert_eq!(original.width, modified.width);
        assert_eq!(original.height, modified.height);

        let width = original.width;
        let height = original.height;

        let img1_img2_blur = original.img1_img2_blur(&mut modified.img, tmp);

        let c1 = 0.01 * 0.01;
        let c2 = 0.03 * 0.03;

        let mut map_out = Vec::with_capacity(width * height);
        unsafe { map_out.set_len(width * height) };

        // FIXME: slice https://users.rust-lang.org/t/how-to-zip-two-slices-efficiently/2048
        for (img1_img2_blur, mu1, mu2, img1_sq_blur, img2_sq_blur, mut map_out) in
            Zip::new((img1_img2_blur.iter().cloned(),
                      original.mu.iter().cloned(),
                      modified.mu.iter().cloned(),
                      original.img_sq_blur.iter().cloned(),
                      modified.img_sq_blur.iter().cloned(),
                      map_out.iter_mut())) {

            let mu1mu1 = mu1 * mu1;
            let mu1mu2 = mu1 * mu2;
            let mu2mu2 = mu2 * mu2;
            let mu1_sq:f32 = mu1mu1.into();
            let mu2_sq:f32 = mu2mu2.into();
            let mu1_mu2:f32 = mu1mu2.into();
            let sigma1_sq:f32 = (img1_sq_blur - mu1mu1).into();
            let sigma2_sq:f32 = (img2_sq_blur - mu2mu2).into();
            let sigma12:f32 = (img1_img2_blur - mu1mu2).into();

            let ssim = (2. * mu1_mu2 + c1) * (2. * sigma12 + c2) /
                       ((mu1_sq + mu2_sq + c1) * (sigma1_sq + sigma2_sq + c2));

            *map_out = ssim;
        }

        return SsimMap {
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
        if self.width == 0 {
            return None;
        }
        return Some(&self.data[..]);
    }
}


#[test]
fn png_compare() {
    extern crate lodepng;

    use linear::*;

    let mut d = new();
    let file1 = lodepng::decode32_file("tests/test1-sm.png").unwrap();
    let file2 = lodepng::decode32_file("tests/test2-sm.png").unwrap();

    let buf1 = &file1.buffer.as_ref().to_rgbaplu();
    let buf2 = &file2.buffer.as_ref().to_rgbaplu();
    let img1 = d.create_image(buf1, file1.width, file1.height).unwrap();
    let img2 = d.create_image(buf2, file2.width, file2.height).unwrap();

    let res = d.compare(&img1, img2);
    assert!((0.003297 - res).abs() < 0.0001, "res is {}", res);
    assert!(res < 0.0033);
    assert!(0.0032 < res);

    let img1b = d.create_image(buf1, file1.width, file1.height).unwrap();
    let res = d.compare(&img1, img1b);

    assert!(0.000000000000001 > res);
    assert!(res < 0.000000000000001);
    assert_eq!(res, res);
}

