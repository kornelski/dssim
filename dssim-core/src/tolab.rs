#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use crate::image::ToRGB;
use crate::image::RGBAPLU;
use crate::image::RGBLU;
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
        let f = |fy| {
            if fy > EPSILON { (cbrt_poly(fy) - 16. / 116.) * 1.16 } else { (K * 1.16) * fy }
        };

        #[cfg(feature = "threads")]
        let out = (0..self.height()).into_par_iter().flat_map_iter(|y| {
            self[y].iter().map(|&fy| f(fy))
        }).collect();

        #[cfg(not(feature = "threads"))]
        let out = self.pixels().map(f).collect();

        vec![Self::new(out, self.width(), self.height())]
    }
}

/// Per-pixel Lab conversion strategy for `rgb_to_lab`'s row kernels.
/// `conv` MUST be `#[inline(always)]` in every impl: the row kernels are
/// `#[target_feature]` clones, and only code inlined into them compiles
/// with FMA — a non-inlined callee (e.g. a fat closure) silently runs at
/// baseline features as libm `fmaf` calls.
trait LabConv<T: Copy>: Sync + Send {
    fn conv(&self, px: T, n: usize) -> (f32, f32, f32);
}

/// Per-row body shared by the baseline and AVX2+FMA row writers.
/// `#[inline(always)]` so the `#[target_feature]` clone re-vectorizes the
/// same body under its target features. The pixel loop must live inside
/// the tagged function — `#[target_feature]` does not propagate into
/// rayon worker callbacks.
#[inline(always)]
fn rgb_to_lab_row_inline<T, C>(in_row: &[T], y: usize, conv: &C,
    l_row: &mut [MaybeUninit<f32>], a_row: &mut [MaybeUninit<f32>], b_row: &mut [MaybeUninit<f32>])
    where T: Copy, C: LabConv<T>
{
    for x in 0..in_row.len() {
        let n = (x+11) ^ (y+11);
        let (l,a,b) = conv.conv(in_row[x], n);
        l_row[x].write(l);
        a_row[x].write(a);
        b_row[x].write(b);
    }
}

#[inline(never)]
fn rgb_to_lab_row_base<T, C>(in_row: &[T], y: usize, conv: &C,
    l_row: &mut [MaybeUninit<f32>], a_row: &mut [MaybeUninit<f32>], b_row: &mut [MaybeUninit<f32>])
    where T: Copy, C: LabConv<T>
{
    rgb_to_lab_row_inline(in_row, y, conv, l_row, a_row, b_row)
}

/// AVX2+FMA clone of `rgb_to_lab_row_base`; same source, vectorized wider.
/// SAFETY: call only when `caps::has_avx2_fma()` has confirmed support.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma")]
fn rgb_to_lab_row_avx2<T, C>(in_row: &[T], y: usize, conv: &C,
    l_row: &mut [MaybeUninit<f32>], a_row: &mut [MaybeUninit<f32>], b_row: &mut [MaybeUninit<f32>])
    where T: Copy, C: LabConv<T>
{
    rgb_to_lab_row_inline(in_row, y, conv, l_row, a_row, b_row)
}

/// Runtime dispatch: AVX2+FMA kernel when detected, baseline otherwise.
/// aarch64 needs no clone — NEON is its baseline and autovectorizes.
#[inline]
fn rgb_to_lab_row<T, C>(in_row: &[T], y: usize, conv: &C,
    l_row: &mut [MaybeUninit<f32>], a_row: &mut [MaybeUninit<f32>], b_row: &mut [MaybeUninit<f32>])
    where T: Copy, C: LabConv<T>
{
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx2_fma() {
        // SAFETY: has_avx2_fma() confirmed AVX2+FMA support.
        unsafe { rgb_to_lab_row_avx2(in_row, y, conv, l_row, a_row, b_row) };
        return;
    }
    rgb_to_lab_row_base(in_row, y, conv, l_row, a_row, b_row)
}

/// Shared row writer: calls `conv` per pixel into three planar outputs.
/// Rows are farmed to rayon; the per-row pixel loop lives in the
/// dispatched `rgb_to_lab_row_*` fns so the feature gate applies to the
/// actual math (a `#[target_feature]` here would only tag the outer fn).
fn rgb_to_lab<T, C>(img: ImgRef<'_, T>, conv: &C) -> Vec<GBitmap>
    where T: Copy + Sync + Send + 'static, C: LabConv<T>
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
        rgb_to_lab_row(in_row, y, conv,
            &mut l_row[0..width], &mut a_row[0..width], &mut b_row[0..width]);
    });

    unsafe { out_l.set_len(area) };
    unsafe { out_a.set_len(area) };
    unsafe { out_b.set_len(area) };

    vec![
        Img::new(out_l, width, height),
        Img::new(out_a, width, height),
        Img::new(out_b, width, height),
    ]
}

struct RgbapluLab;
struct RgbluLab;

impl LabConv<RGBAPLU> for RgbapluLab {
    #[inline(always)]
    fn conv(&self, px: RGBAPLU, n: usize) -> (f32, f32, f32) {
        px.to_rgb(n).to_lab()
    }
}

impl LabConv<RGBLU> for RgbluLab {
    #[inline(always)]
    fn conv(&self, px: RGBLU, _n: usize) -> (f32, f32, f32) {
        px.to_lab()
    }
}

impl ToLABBitmap for ImgRef<'_, RGBAPLU> {
    #[inline]
    fn to_lab(&self) -> Vec<GBitmap> {
        rgb_to_lab(*self, &RgbapluLab)
    }
}

impl ToLABBitmap for ImgRef<'_, RGBLU> {
    #[inline]
    fn to_lab(&self) -> Vec<GBitmap> {
        rgb_to_lab(*self, &RgbluLab)
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
