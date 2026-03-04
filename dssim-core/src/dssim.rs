#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
/*
 * © 2011-2017 Kornel Lesiński. All rights reserved.
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

use crate::blur;
use crate::image::*;
#[cfg(not(feature = "threads"))]
use crate::lieon as rayon;
use crate::linear::ToRGBAPLU;
pub use crate::tolab::ToLABBitmap;
pub use crate::val::Dssim as Val;
use imgref::*;
use rayon::prelude::*;
use rgb::{RGB, RGBA};
use std::borrow::Borrow;
use std::ops;
use std::ops::Deref;
use std::sync::Arc;

trait Channable<T, I> {
    fn img1_img2_blur(&self, modified: &Self, tmp: &mut [I]) -> Vec<T>;
}

#[derive(Clone)]
struct DssimChan<T> {
    pub width: usize,
    pub height: usize,
    pub img: Option<ImgVec<T>>,
    pub mu: Vec<T>,
    pub img_sq_blur: Vec<T>,
    pub is_chroma: bool,
}

/// Configuration for the comparison
#[derive(Clone, Debug)]
pub struct Dssim {
    scale_weights: Vec<f64>,
    save_maps_scales: u8,
}

#[derive(Clone)]
struct DssimChanScale<T> {
    chan: Vec<DssimChan<T>>,
}

/// Abstract wrapper for images. See [`Dssim::create_image()`]
#[derive(Clone)]
pub struct DssimImage<T> {
    scale: Vec<DssimChanScale<T>>,
}

impl<T> DssimImage<T> {
    #[inline]
    #[must_use]
    pub fn width(&self) -> usize {
        self.scale[0].chan[0].width
    }

    #[inline]
    #[must_use]
    pub fn height(&self) -> usize {
        self.scale[0].chan[0].height
    }
}

// Weighed scales are inspired by the IW-SSIM, but details of the algorithm and weights are different
const DEFAULT_WEIGHTS: [f64; 5] = [0.028, 0.197, 0.322, 0.298, 0.155];

/// Detailed comparison result
#[derive(Clone)]
pub struct SsimMap {
    /// SSIM scores
    pub map: ImgVec<f32>,
    /// Average SSIM (not DSSIM)
    pub ssim: f64,
}

/// Create new context for a comparison
#[must_use]
pub fn new() -> Dssim {
    Dssim::new()
}

impl DssimChan<f32> {
    pub fn new(bitmap: ImgVec<f32>, is_chroma: bool) -> Self {
        debug_assert!(
            bitmap
                .pixels()
                .all(|i| i.is_finite() && i >= 0.0 && i <= 1.0)
        );

        Self {
            width: bitmap.width(),
            height: bitmap.height(),
            mu: Vec::new(),
            img: Some(bitmap),
            img_sq_blur: Vec::new(),
            is_chroma,
        }
    }
}

impl DssimChan<f32> {
    fn preprocess(&mut self, tmp: &mut [f32]) {
        let width = self.width;
        let height = self.height;
        assert!(width > 0);
        assert!(height > 0);

        let img = self.img.as_mut().unwrap();
        debug_assert_eq!(width * height, img.pixels().count());
        debug_assert!(img.pixels().all(f32::is_finite));

        if self.is_chroma {
            blur::blur_in_place(img.as_mut(), tmp);
        }
        let (mu, _, _) = blur::blur(img.as_ref(), tmp).into_contiguous_buf();
        self.mu = mu;

        self.img_sq_blur = blur::blur_mul(img.as_ref(), img.as_ref(), tmp);
    }
}

impl Channable<f32, f32> for DssimChan<f32> {
    fn img1_img2_blur(&self, modified: &Self, tmp32: &mut [f32]) -> Vec<f32> {
        let src = self.img.as_ref().unwrap();
        let modified_img = modified.img.as_ref().unwrap();
        blur::blur_mul(src.as_ref(), modified_img.as_ref(), tmp32)
    }
}

impl Dssim {
    /// Create new context for comparisons
    #[must_use]
    pub fn new() -> Self {
        Self {
            scale_weights: DEFAULT_WEIGHTS[..].to_owned(),
            save_maps_scales: 0,
        }
    }

    /// Set how many scales will be used, and weights of each scale
    pub fn set_scales(&mut self, scales: &[f64]) {
        self.scale_weights = scales.to_vec();
    }

    /// Set how many scales will be kept for saving
    pub fn set_save_ssim_maps(&mut self, num_scales: u8) {
        self.save_maps_scales = num_scales;
    }

    /// Create image from an array of RGBA pixels (sRGB, non-premultiplied, alpha last).
    ///
    /// If you have a slice of `u8`, then see `rgb` crate's `as_rgba()`.
    #[must_use]
    pub fn create_image_rgba(
        &self,
        bitmap: &[RGBA<u8>],
        width: usize,
        height: usize,
    ) -> Option<DssimImage<f32>> {
        if width * height < bitmap.len() {
            return None;
        }
        let img = ImgVec::new(bitmap.to_rgbaplu(), width, height);
        self.create_image(&img)
    }

    /// Create image from an array of packed RGB pixels (sRGB).
    ///
    /// If you have a slice of `u8`, then see `rgb` crate's `as_rgb()`.
    #[must_use]
    pub fn create_image_rgb(
        &self,
        bitmap: &[RGB<u8>],
        width: usize,
        height: usize,
    ) -> Option<DssimImage<f32>> {
        if width * height < bitmap.len() {
            return None;
        }
        let img = ImgVec::new(bitmap.to_rgblu(), width, height);
        self.create_image(&img)
    }

    /// The input image is defined using the `imgref` crate, and the pixel type can be:
    ///
    /// * `ImgVec<RGBAPLU>` — RGBA premultiplied alpha, linear, float scaled to 0..1
    /// * `ImgVec<RGBLU>` — RGBA linear, float scaled to 0..1
    /// * `ImgVec<f32>` — linear light grayscale, float scaled to 0..1
    ///
    /// And there's [`ToRGBAPLU::to_rgbaplu()`][crate::ToRGBAPLU::to_rgbaplu()] trait to convert the input pixels from
    /// `[RGBA<u8>]`, `[RGBA<u16>]`, `[RGB<u8>]`, or `RGB<u16>`. See `lib.rs` for example how it's done.
    ///
    /// You can implement `ToLABBitmap` and `Downsample` traits on your own image type.
    pub fn create_image<InBitmap, OutBitmap>(&self, src_img: &InBitmap) -> Option<DssimImage<f32>>
    where
        InBitmap: ToLABBitmap + Send + Sync + Downsample<Output = OutBitmap>,
        OutBitmap: ToLABBitmap + Send + Sync + Downsample<Output = OutBitmap>,
    {
        let num_scales = self.scale_weights.len();
        let mut scale = Vec::with_capacity(num_scales);
        Self::make_scales_recursive(num_scales, MaybeArc::Borrowed(src_img), &mut scale);
        scale.reverse(); // depth-first made smallest scales first

        Some(DssimImage { scale })
    }

    #[inline(never)]
    fn make_scales_recursive<InBitmap, OutBitmap>(
        scales_left: usize,
        image: MaybeArc<'_, InBitmap>,
        scales: &mut Vec<DssimChanScale<f32>>,
    ) where
        InBitmap: ToLABBitmap + Send + Sync + Downsample<Output = OutBitmap>,
        OutBitmap: ToLABBitmap + Send + Sync + Downsample<Output = OutBitmap>,
    {
        // Run to_lab and next downsampling in parallel
        let (chan, _) = rayon::join(
            {
                let image = image.clone();
                move || {
                    let lab = image.to_lab();
                    drop(image); // Free larger RGB image ASAP
                    DssimChanScale {
                        chan: lab
                            .into_par_iter()
                            .with_max_len(1)
                            .enumerate()
                            .map(|(n, l)| {
                                let w = l.width();
                                let h = l.height();
                                let mut ch = DssimChan::new(l, n > 0);

                                let pixels = w * h;
                                let mut tmp = blur::uninit_f32_vec(pixels);
                                ch.preprocess(&mut tmp);
                                ch
                            })
                            .collect(),
                    }
                }
            },
            {
                let scales = &mut *scales;
                move || {
                    if scales_left > 0 {
                        let down = image.downsample();
                        drop(image);
                        if let Some(downsampled) = down {
                            Self::make_scales_recursive(
                                scales_left - 1,
                                MaybeArc::Owned(Arc::new(downsampled)),
                                scales,
                            );
                        }
                    }
                }
            },
        );
        scales.push(chan);
    }

    /// Compare original with another image. See `create_image`
    ///
    /// The `SsimMap`s are returned only if you've enabled them first.
    ///
    /// `Val` is a fancy wrapper for `f64`
    pub fn compare<M: Borrow<DssimImage<f32>>>(
        &self,
        original_image: &DssimImage<f32>,
        modified_image: M,
    ) -> (Val, Vec<SsimMap>) {
        self.compare_inner(original_image, modified_image.borrow())
    }

    #[inline(never)]
    fn compare_inner(
        &self,
        original_image: &DssimImage<f32>,
        modified_image: &DssimImage<f32>,
    ) -> (Val, Vec<SsimMap>) {
        let scaled_images_iter = modified_image.scale.iter().zip(original_image.scale.iter());
        let combined_iter = self
            .scale_weights
            .iter()
            .copied()
            .zip(scaled_images_iter)
            .enumerate();

        let res: Vec<_> = combined_iter
            .par_bridge()
            .map(
                |(n, (weight, (modified_image_scale, original_image_scale)))| {
                    let scale_width = original_image_scale.chan[0].width;
                    let scale_height = original_image_scale.chan[0].height;
                    let ssim_map = match original_image_scale.chan.len() {
                        3 => {
                            let pixels = scale_width * scale_height;
                            let img1_img2_blur: Vec<Vec<f32>> = (0..3usize)
                                .into_par_iter()
                                .map(|c| {
                                    let mut tmp = blur::uninit_f32_vec(pixels);
                                    original_image_scale.chan[c]
                                        .img1_img2_blur(&modified_image_scale.chan[c], &mut tmp)
                                })
                                .collect();
                            Self::compare_scale_3ch(
                                original_image_scale,
                                modified_image_scale,
                                &img1_img2_blur,
                            )
                        }
                        1 => {
                            let mut tmp = blur::uninit_f32_vec(scale_width * scale_height);
                            let img1_img2_blur = original_image_scale.chan[0]
                                .img1_img2_blur(&modified_image_scale.chan[0], &mut tmp);
                            Self::compare_scale(
                                &original_image_scale.chan[0],
                                &modified_image_scale.chan[0],
                                &img1_img2_blur,
                            )
                        }
                        _ => panic!(),
                    };

                    let sum = ssim_map.pixels().fold(0., |sum, i| sum + f64::from(i));
                    let len = (ssim_map.width() * ssim_map.height()) as f64;
                    let avg = (sum / len).max(0.0).powf((0.5_f64).powf(n as f64));
                    let score = 1.0
                        - (ssim_map
                            .pixels()
                            .fold(0., |sum, i| sum + (avg - f64::from(i)).abs())
                            / len);

                    let map = if self.save_maps_scales as usize > n {
                        Some(SsimMap {
                            map: ssim_map,
                            ssim: score,
                        })
                    } else {
                        None
                    };
                    (score, weight, map)
                },
            )
            .collect();

        let mut ssim_sum = 0.0;
        let mut weight_sum = 0.0;
        let mut ssim_maps = Vec::new();
        for (score, weight, map) in res {
            ssim_sum += score * weight;
            weight_sum += weight;
            if let Some(m) = map {
                ssim_maps.push(m);
            }
        }

        (to_dssim(ssim_sum / weight_sum).into(), ssim_maps)
    }

    /// Specialized 3-channel comparison with manually unrolled channel loop.
    /// Eliminates zip iterator overhead and branch mispredictions from
    /// iterating over exactly 3 channels.
    #[inline(never)]
    fn compare_scale_3ch(
        original: &DssimChanScale<f32>,
        modified: &DssimChanScale<f32>,
        img1_img2_blur: &[Vec<f32>],
    ) -> ImgVec<f32> {
        let width = original.chan[0].width;
        let height = original.chan[0].height;
        let pixels = width * height;

        // Extract all slice references up front to avoid repeated Vec/struct indexing
        let (o0, o1, o2) = (&original.chan[0], &original.chan[1], &original.chan[2]);
        let (m0, m1, m2) = (&modified.chan[0], &modified.chan[1], &modified.chan[2]);

        let o0_mu = &o0.mu[..pixels];
        let o1_mu = &o1.mu[..pixels];
        let o2_mu = &o2.mu[..pixels];
        let m0_mu = &m0.mu[..pixels];
        let m1_mu = &m1.mu[..pixels];
        let m2_mu = &m2.mu[..pixels];
        let o0_sq = &o0.img_sq_blur[..pixels];
        let o1_sq = &o1.img_sq_blur[..pixels];
        let o2_sq = &o2.img_sq_blur[..pixels];
        let m0_sq = &m0.img_sq_blur[..pixels];
        let m1_sq = &m1.img_sq_blur[..pixels];
        let m2_sq = &m2.img_sq_blur[..pixels];
        let i12_0 = &img1_img2_blur[0][..pixels];
        let i12_1 = &img1_img2_blur[1][..pixels];
        let i12_2 = &img1_img2_blur[2][..pixels];

        #[cfg(all(feature = "fma", target_arch = "x86_64"))]
        {
            use archmage::SimdToken as _;
            if let Some(token) = archmage::X64V3Token::summon() {
                let mut map_out = blur::uninit_f32_vec(pixels);
                map_out
                    .par_chunks_mut(1024)
                    .enumerate()
                    .for_each(|(ci, chunk)| {
                        let off = ci * 1024;
                        let len = chunk.len();
                        crate::ssim_simd::compare_3ch_avx2(
                            token,
                            [
                                &o0_mu[off..off + len],
                                &o1_mu[off..off + len],
                                &o2_mu[off..off + len],
                            ],
                            [
                                &m0_mu[off..off + len],
                                &m1_mu[off..off + len],
                                &m2_mu[off..off + len],
                            ],
                            [
                                &o0_sq[off..off + len],
                                &o1_sq[off..off + len],
                                &o2_sq[off..off + len],
                            ],
                            [
                                &m0_sq[off..off + len],
                                &m1_sq[off..off + len],
                                &m2_sq[off..off + len],
                            ],
                            [
                                &i12_0[off..off + len],
                                &i12_1[off..off + len],
                                &i12_2[off..off + len],
                            ],
                            chunk,
                        );
                    });
                return ImgVec::new(map_out, width, height);
            }
        }

        let c1: f32 = 0.01 * 0.01;
        let c2: f32 = 0.03 * 0.03;
        let inv3: f32 = 1.0 / 3.0;

        let map_out: Vec<f32> = (0..pixels)
            .into_par_iter()
            .with_min_len(1 << 10)
            .map(|i| {
                // Channel 0 (L)
                let mu1_0 = o0_mu[i];
                let mu2_0 = m0_mu[i];
                let mu1mu1_0 = mu1_0 * mu1_0;
                let mu2mu2_0 = mu2_0 * mu2_0;
                let mu1mu2_0 = mu1_0 * mu2_0;

                // Channel 1 (a)
                let mu1_1 = o1_mu[i];
                let mu2_1 = m1_mu[i];
                let mu1mu1_1 = mu1_1 * mu1_1;
                let mu2mu2_1 = mu2_1 * mu2_1;
                let mu1mu2_1 = mu1_1 * mu2_1;

                // Channel 2 (b)
                let mu1_2 = o2_mu[i];
                let mu2_2 = m2_mu[i];
                let mu1mu1_2 = mu1_2 * mu1_2;
                let mu2mu2_2 = mu2_2 * mu2_2;
                let mu1mu2_2 = mu1_2 * mu2_2;

                let mu1_sq = (mu1mu1_0 + mu1mu1_1 + mu1mu1_2) * inv3;
                let mu2_sq = (mu2mu2_0 + mu2mu2_1 + mu2mu2_2) * inv3;
                let mu1_mu2 = (mu1mu2_0 + mu1mu2_1 + mu1mu2_2) * inv3;

                let sigma1_sq =
                    ((o0_sq[i] - mu1mu1_0) + (o1_sq[i] - mu1mu1_1) + (o2_sq[i] - mu1mu1_2)) * inv3;
                let sigma2_sq =
                    ((m0_sq[i] - mu2mu2_0) + (m1_sq[i] - mu2mu2_1) + (m2_sq[i] - mu2mu2_2)) * inv3;
                let sigma12 =
                    ((i12_0[i] - mu1mu2_0) + (i12_1[i] - mu1mu2_1) + (i12_2[i] - mu1mu2_2)) * inv3;

                (2.0 * mu1_mu2 + c1) * (2.0 * sigma12 + c2)
                    / ((mu1_sq + mu2_sq + c1) * (sigma1_sq + sigma2_sq + c2))
            })
            .collect();

        ImgVec::new(map_out, width, height)
    }

    #[inline(never)]
    fn compare_scale<L>(
        original: &DssimChan<L>,
        modified: &DssimChan<L>,
        img1_img2_blur: &[L],
    ) -> ImgVec<f32>
    where
        L: Send + Sync + Clone + Copy + ops::Mul<Output = L> + ops::Sub<Output = L> + 'static,
        f32: From<L>,
    {
        assert_eq!(original.width, modified.width);
        assert_eq!(original.height, modified.height);

        let width = original.width;
        let height = original.height;

        let c1 = 0.01 * 0.01;
        let c2 = 0.03 * 0.03;

        debug_assert_eq!(original.mu.len(), modified.mu.len());
        debug_assert_eq!(original.img_sq_blur.len(), modified.img_sq_blur.len());
        debug_assert_eq!(img1_img2_blur.len(), original.mu.len());
        debug_assert_eq!(img1_img2_blur.len(), original.img_sq_blur.len());

        let mu_iter = original
            .mu
            .as_slice()
            .par_iter()
            .with_min_len(1 << 10)
            .cloned()
            .zip_eq(
                modified
                    .mu
                    .as_slice()
                    .par_iter()
                    .with_min_len(1 << 10)
                    .cloned(),
            );
        let sq_iter = original
            .img_sq_blur
            .as_slice()
            .par_iter()
            .with_min_len(1 << 10)
            .cloned()
            .zip_eq(
                modified
                    .img_sq_blur
                    .as_slice()
                    .par_iter()
                    .with_min_len(1 << 10)
                    .cloned(),
            );
        let map_out = img1_img2_blur
            .par_iter()
            .with_min_len(1 << 10)
            .cloned()
            .zip_eq(mu_iter)
            .zip_eq(sq_iter)
            .map(
                |((img1_img2_blur, (mu1, mu2)), (img1_sq_blur, img2_sq_blur))| {
                    let mu1mu1 = mu1 * mu1;
                    let mu1mu2 = mu1 * mu2;
                    let mu2mu2 = mu2 * mu2;
                    let mu1_sq: f32 = mu1mu1.into();
                    let mu2_sq: f32 = mu2mu2.into();
                    let mu1_mu2: f32 = mu1mu2.into();
                    let sigma1_sq: f32 = (img1_sq_blur - mu1mu1).into();
                    let sigma2_sq: f32 = (img2_sq_blur - mu2mu2).into();
                    let sigma12: f32 = (img1_img2_blur - mu1mu2).into();

                    (2.0 * mu1_mu2 + c1) * (2.0 * sigma12 + c2)
                        / ((mu1_sq + mu2_sq + c1) * (sigma1_sq + sigma2_sq + c2))
                },
            )
            .collect();

        ImgVec::new(map_out, width, height)
    }
}

fn to_dssim(ssim: f64) -> f64 {
    1.0 / ssim.max(f64::EPSILON) - 1.0
}

#[test]
fn png_compare() {
    use crate::linear::*;
    use imgref::*;

    let d = new();
    let file1 = lodepng::decode32_file("../tests/test1-sm.png").unwrap();
    let file2 = lodepng::decode32_file("../tests/test2-sm.png").unwrap();

    let buf1 = &file1.buffer.to_rgbaplu()[..];
    let buf2 = &file2.buffer.to_rgbaplu()[..];
    let img1 = d
        .create_image(&Img::new(buf1, file1.width, file1.height))
        .unwrap();
    let img2 = d
        .create_image(&Img::new(buf2, file2.width, file2.height))
        .unwrap();

    let (res, _) = d.compare(&img1, img2);
    assert!((0.001 - res).abs() < 0.0005, "res is {res}");

    let img1b = d
        .create_image(&Img::new(buf1, file1.width, file1.height))
        .unwrap();
    let (res, _) = d.compare(&img1, img1b);

    assert!(0.000000000000001 > res);
    assert!(res < 0.000000000000001);
    assert_eq!(res, res);

    let sub_img1 = d
        .create_image(&Img::new(buf1, file1.width, file1.height).sub_image(2, 3, 44, 33))
        .unwrap();
    let sub_img2 = d
        .create_image(&Img::new(buf2, file2.width, file2.height).sub_image(17, 9, 44, 33))
        .unwrap();
    // Test passing second image directly
    let (res, _) = d.compare(&sub_img1, sub_img2);
    assert!(res > 0.1);

    let sub_img1 = d
        .create_image(&Img::new(buf1, file1.width, file1.height).sub_image(22, 8, 61, 40))
        .unwrap();
    let sub_img2 = d
        .create_image(&Img::new(buf2, file2.width, file2.height).sub_image(22, 8, 61, 40))
        .unwrap();
    // Test passing second image as reference
    let (res, _) = d.compare(&sub_img1, sub_img2);
    assert!(res < 0.01);
}

enum MaybeArc<'a, T> {
    Owned(Arc<T>),
    Borrowed(&'a T),
}

impl<T> Clone for MaybeArc<'_, T> {
    fn clone(&self) -> Self {
        match self {
            Self::Owned(t) => Self::Owned(t.clone()),
            Self::Borrowed(t) => Self::Borrowed(t),
        }
    }
}

impl<T> Deref for MaybeArc<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(t) => t,
            Self::Borrowed(t) => t,
        }
    }
}

#[test]
fn poison() {
    let a = RGBAPLU::new(1., 1., 1., 1.);
    let b = RGBAPLU::new(0., 0., 0., 0.);
    let n = 1. / 0.;
    let n = RGBAPLU::new(n, n, n, n);
    let buf = vec![b, a, a, b, n, n, a, b, b, a, n, n, b, a, a, b, n];
    let img = ImgVec::new_stride(buf, 4, 3, 6);
    assert!(img.pixels().all(|p| p.r.is_finite() && p.a.is_finite()));
    assert!(
        img.as_ref()
            .pixels()
            .all(|p| p.g.is_finite() && p.b.is_finite())
    );

    let d = new();
    let sub_img1 = d.create_image(&img.as_ref()).unwrap();
    let sub_img2 = d.create_image(&img.as_ref()).unwrap();
    let (res, _) = d.compare(&sub_img1, sub_img2);
    assert!(res < 0.000001);
}
