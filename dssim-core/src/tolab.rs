//! Safety: the feature-gated calls are checked by caps. RGB output retains
//! upstream spare-capacity writes and set_len after all rows complete.
//! Grayscale output uses initialized slices; indexing is bounds-checked.

#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use crate::image::{GammaImage, ToRGB};
use crate::image::RGBAPLU;
use crate::image::RGBLU;
use crate::linear::{GammaComponent, GammaPixel};
use imgref::*;
#[cfg(not(feature = "threads"))]
use crate::lieon as rayon;
use rayon::prelude::*;
use std::mem::MaybeUninit;

const D65x: f32 = 0.9505;
const D65y: f32 = 1.0;
const D65z: f32 = 1.089;

pub type GBitmap = ImgVec<f32>;
pub(crate) trait ToLAB {
    fn to_lab(&self) -> (f32, f32, f32);
}

#[inline(always)]
fn fma_matrix(r: f32, rx: f32, g: f32, gx: f32, b: f32, bx: f32) -> f32 {
    b.mul_add(bx, g.mul_add(gx, r * rx))
}

const EPSILON: f32 = 216. / 24389.;
const K: f32 = 24389. / (27. * 116.); // http://www.brucelindbloom.com/LContinuity.html

impl ToLAB for RGBLU {
    #[inline(always)]
    fn to_lab(&self) -> (f32, f32, f32) {
        let fx = fma_matrix(self.r, 0.4124 / D65x, self.g, 0.3576 / D65x, self.b, 0.1805 / D65x);
        let fy = fma_matrix(self.r, 0.2126 / D65y, self.g, 0.7152 / D65y, self.b, 0.0722 / D65y);
        let fz = fma_matrix(self.r, 0.0193 / D65z, self.g, 0.1192 / D65z, self.b, 0.9505 / D65z);

        let X = if fx > EPSILON { cbrt_poly(fx) - 16. / 116. } else { K * fx };
        let Y = if fy > EPSILON { cbrt_poly(fy) - 16. / 116. } else { K * fy };
        let Z = if fz > EPSILON { cbrt_poly(fz) - 16. / 116. } else { K * fz };

        let lab = (
            (Y * 1.05f32), // 1.05 instead of 1.16 to boost color importance without pushing colors outside of 1.0 range
            (500.0 / 220.0f32).mul_add(X - Y, 86.2 / 220.0f32), /* 86 is a fudge to make the value positive */
            (200.0 / 220.0f32).mul_add(Y - Z, 107.9 / 220.0f32), /* 107 is a fudge to make the value positive */
        );
        debug_assert!(lab.0 <= 1.0 && lab.1 <= 1.0 && lab.2 <= 1.0);
        lab
    }
}

#[inline]
fn cbrt_poly(x: f32) -> f32 {
    // Polynomial approximation
    let poly = [0.2f32, 1.51, -0.5];
    let y = poly[2].mul_add(x, poly[1]).mul_add(x, poly[0]);

    // 2x Halley's Method
    let y3 = y * y * y;
    let y = y * 2.0f32.mul_add(x, y3) / 2.0f32.mul_add(y3, x);
    let y3 = y * y * y;
    let y = y * 2.0f32.mul_add(x, y3) / 2.0f32.mul_add(y3, x);
    debug_assert!(y < 1.001);
    debug_assert!(x < 216. / 24389. || y >= 16. / 116.);
    y
}

/// Convert image to L\*a\*b\* planar
///
/// It should return 1 (gray) or 3 (color) planes.
pub trait ToLABBitmap {
    fn to_lab(&self) -> Vec<GBitmap>;
}

impl ToLABBitmap for ImgVec<RGBAPLU> {
    #[inline(always)]
    fn to_lab(&self) -> Vec<GBitmap> {
        self.as_ref().to_lab()
    }
}

impl ToLABBitmap for ImgVec<RGBLU> {
    #[inline(always)]
    fn to_lab(&self) -> Vec<GBitmap> {
        self.as_ref().to_lab()
    }
}
impl ToLABBitmap for GBitmap {
    fn to_lab(&self) -> Vec<GBitmap> {
        debug_assert!(self.width() > 0);
        let mut out = vec![0.0; self.width() * self.height()];
        out.as_mut_slice().par_chunks_exact_mut(self.width()).enumerate().for_each(|(y, row)| {
            gray_to_lab_row(&self[y], row);
        });

        vec![Self::new(out, self.width(), self.height())]
    }
}

/// Grayscale rows respect the input stride, including padded ImgVec buffers.
#[inline(always)]
fn gray_to_lab_row_inline(src: &[f32], dst: &mut [f32]) {
    assert_eq!(src.len(), dst.len());
    for (&fy, out) in src.iter().zip(dst) {
        *out = if fy > EPSILON { (cbrt_poly(fy) - 16. / 116.) * 1.16 } else { (K * 1.16) * fy };
    }
}

#[inline(never)]
fn gray_to_lab_row_base(src: &[f32], dst: &mut [f32]) {
    gray_to_lab_row_inline(src, dst);
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma")]
fn gray_to_lab_row_avx2(src: &[f32], dst: &mut [f32]) {
    gray_to_lab_row_inline(src, dst);
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma,avx512f,avx512bw,avx512dq,avx512vl")]
fn gray_to_lab_row_avx512(src: &[f32], dst: &mut [f32]) {
    gray_to_lab_row_inline(src, dst);
}

#[inline]
fn gray_to_lab_row(src: &[f32], dst: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx512() {
        // SAFETY: the gate checks all target features required by this clone.
        return unsafe { gray_to_lab_row_avx512(src, dst) };
    }
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx2_fma() {
        // SAFETY: the gate checks all target features required by this clone.
        return unsafe { gray_to_lab_row_avx2(src, dst) };
    }
    gray_to_lab_row_base(src, dst);
}

/// Per-row body shared by the baseline and feature-enabled row kernels.
/// `#[inline(always)]` so the `#[target_feature]` clone re-vectorizes the
/// same body under its target features. The pixel loop must live inside
/// the tagged function — `#[target_feature]` does not propagate into
/// rayon worker callbacks.
#[inline(always)]
fn rgb_to_lab_row_inline<T>(in_row: &[T], y: usize, context: &T::Context,
    l_row: &mut [MaybeUninit<f32>], a_row: &mut [MaybeUninit<f32>], b_row: &mut [MaybeUninit<f32>])
    where T: Copy + ToRGB
{
    for x in 0..in_row.len() {
        let n = (x+11) ^ (y+11);
        let (l,a,b) = in_row[x].to_rgb(n, context).to_lab();
        l_row[x].write(l);
        a_row[x].write(a);
        b_row[x].write(b);
    }
}

#[inline(never)]
fn rgb_to_lab_row_base<T>(in_row: &[T], y: usize, context: &T::Context,
    l_row: &mut [MaybeUninit<f32>], a_row: &mut [MaybeUninit<f32>], b_row: &mut [MaybeUninit<f32>])
    where T: Copy + ToRGB
{
    rgb_to_lab_row_inline(in_row, y, context, l_row, a_row, b_row)
}

/// AVX2+FMA clone of `rgb_to_lab_row_base`; same source, vectorized wider.
/// SAFETY: call only when `caps::has_avx2_fma()` has confirmed support.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma")]
fn rgb_to_lab_row_avx2<T>(in_row: &[T], y: usize, context: &T::Context,
    l_row: &mut [MaybeUninit<f32>], a_row: &mut [MaybeUninit<f32>], b_row: &mut [MaybeUninit<f32>])
    where T: Copy + ToRGB
{
    rgb_to_lab_row_inline(in_row, y, context, l_row, a_row, b_row)
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma,avx512f,avx512bw,avx512dq,avx512vl")]
fn rgb_to_lab_row_avx512<T>(in_row: &[T], y: usize, context: &T::Context,
    l_row: &mut [MaybeUninit<f32>], a_row: &mut [MaybeUninit<f32>], b_row: &mut [MaybeUninit<f32>])
    where T: Copy + ToRGB
{
    rgb_to_lab_row_inline(in_row, y, context, l_row, a_row, b_row)
}

/// Runtime dispatch: AVX-512, then AVX2/FMA, then baseline.
/// aarch64 needs no clone — NEON is its baseline and autovectorizes.
#[inline]
fn rgb_to_lab_row<T>(in_row: &[T], y: usize, context: &T::Context,
    l_row: &mut [MaybeUninit<f32>], a_row: &mut [MaybeUninit<f32>], b_row: &mut [MaybeUninit<f32>])
    where T: Copy + ToRGB
{
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx512() {
        // SAFETY: has_avx512() confirmed AVX2/FMA and AVX-512 F/BW/DQ/VL support.
        unsafe { rgb_to_lab_row_avx512(in_row, y, context, l_row, a_row, b_row) };
        return;
    }
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx2_fma() {
        // SAFETY: has_avx2_fma() confirmed AVX2+FMA support.
        unsafe { rgb_to_lab_row_avx2(in_row, y, context, l_row, a_row, b_row) };
        return;
    }
    rgb_to_lab_row_base(in_row, y, context, l_row, a_row, b_row)
}

/// Convert each row into three planar outputs.
/// Rows are farmed to rayon; the per-row pixel loop lives in the
/// dispatched `rgb_to_lab_row_*` fns so the feature gate applies to the
/// actual math (a `#[target_feature]` here would only tag the outer fn).
fn rgb_to_lab<T>(img: ImgRef<'_, T>, context: &T::Context) -> Vec<GBitmap>
    where T: Copy + ToRGB + Sync + Send + 'static
{
    let width = img.width();
    assert!(width > 0);
    let height = img.height();
    let area = width * height;

    let mut out_l = Vec::with_capacity(area);
    let mut out_a = Vec::with_capacity(area);
    let mut out_b = Vec::with_capacity(area);

    // For output width == stride
    out_l.spare_capacity_mut().par_chunks_exact_mut(width).take(height).zip(
        out_a.spare_capacity_mut().par_chunks_exact_mut(width).take(height).zip(
            out_b.spare_capacity_mut().par_chunks_exact_mut(width).take(height))
    ).enumerate()
    .for_each(|(y, (l_row, (a_row, b_row)))| {
        let in_row = &img.rows().nth(y).unwrap()[0..width];
        rgb_to_lab_row(in_row, y, context,
            &mut l_row[0..width], &mut a_row[0..width], &mut b_row[0..width]);
    });

    // SAFETY: all rows initialized every output slot; a panic skips publication.
    unsafe { out_l.set_len(area) };
    unsafe { out_a.set_len(area) };
    unsafe { out_b.set_len(area) };

    vec![
        Img::new(out_l, width, height),
        Img::new(out_a, width, height),
        Img::new(out_b, width, height),
    ]
}

impl ToLABBitmap for ImgRef<'_, RGBAPLU> {
    #[inline]
    fn to_lab(&self) -> Vec<GBitmap> {
        rgb_to_lab(*self, &())
    }
}

impl ToLABBitmap for ImgRef<'_, RGBLU> {
    #[inline]
    fn to_lab(&self) -> Vec<GBitmap> {
        rgb_to_lab(*self, &())
    }
}

/// The private adapter uses the same LUT, alpha and Lab arithmetic as the
/// materialized path, without its full-resolution linear pixel buffer.
impl<P> ToLABBitmap for GammaImage<'_, P>
where
    P: GammaPixel<Output = RGBAPLU> + Copy + Sync + Send + 'static,
    <P::Component as GammaComponent>::Lut: Sync,
{
    fn to_lab(&self) -> Vec<GBitmap> {
        let lut = P::make_lut();
        rgb_to_lab(self.0, &lut)
    }
}

#[test]
fn cbrts1() {
    let mut totaldiff = 0.;
    let mut maxdiff: f64 = 0.;
    for i in (0..=10001).rev() {
        let x = (f64::from(i) / 10001.) as f32;
        let a = cbrt_poly(x);
        let actual = a * a * a;
        let expected = x;
        let absdiff = (f64::from(expected) - f64::from(actual)).abs();
        assert!(absdiff < 0.0002, "{expected} - {actual} = {} @ {x}", expected - actual);
        if i % 400 == 0 {
            println!("{:+0.3}", (expected - actual) * 255.);
        }
        totaldiff += absdiff;
        maxdiff = maxdiff.max(absdiff);
    }
    println!("1={totaldiff:0.6}; {maxdiff:0.8}");
    assert!(totaldiff < 0.0025, "{totaldiff}");
}

#[test]
fn cbrts2() {
    let mut totaldiff = 0.;
    let mut maxdiff: f64 = 0.;
    for i in (2000..=10001).rev() {
        let x = f64::from(i) / 10001.;
        let actual = f64::from(cbrt_poly(x as f32));
        let expected = x.cbrt();
        let absdiff = (expected - actual).abs();
        totaldiff += absdiff;
        maxdiff = maxdiff.max(absdiff);
        assert!(absdiff < 0.0000005, "{expected} - {actual} = {} @ {x}", expected - actual);
    }
    println!("2={totaldiff:0.6}; {maxdiff:0.8}");
    assert!(totaldiff < 0.0025, "{totaldiff}");
}

#[cfg(test)]
#[path = "tolab/dispatch_tests.rs"]
mod dispatch_tests;

#[cfg(test)]
#[path = "tolab/fused_tests.rs"]
mod fused_tests;
