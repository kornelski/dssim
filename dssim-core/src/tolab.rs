#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use crate::image::ToRGB;
use crate::image::RGBAPLU;
use crate::image::RGBLU;
use imgref::*;
#[cfg(not(feature = "threads"))]
use crate::lieon as rayon;
use rayon::prelude::*;

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

/// Cube root initial estimate via the standard bit-manipulation trick
/// (~5-bit accuracy). Cheap integer-only seed for Halley's refinement.
/// `B1 = 709_958_130` is the well-known fast-cbrt constant.
#[inline]
fn cbrt_initial(x: f32) -> f32 {
    const B1: u32 = 709_958_130;
    let ui = x.to_bits();
    let hx = (ui & 0x7FFF_FFFF) / 3 + B1;
    let ui_out = (ui & 0x8000_0000) | hx;
    f32::from_bits(ui_out)
}

/// Fast cube root: bit-trick seed + 2 Halley iterations.
/// Each Halley step roughly triples correct bits (5 → 15 → 45), so the
/// result is bounded by f32 precision (~24 bits), well inside the
/// existing tolerance tests.
#[inline]
fn cbrt_poly(x: f32) -> f32 {
    if x == 0.0 {
        return 0.0;
    }
    let t = cbrt_initial(x);
    // Halley step: t ← t · (2x + t³) / (2t³ + x).
    // Division-first form `t *= num / den` keeps the FMA shape and avoids
    // catastrophic underflow in `t * num` for very small x.
    let r = t * t * t;
    let t = t * x.mul_add(2.0, r) / r.mul_add(2.0, x);
    let r = t * t * t;
    let t = t * x.mul_add(2.0, r) / r.mul_add(2.0, x);
    debug_assert!(t < 1.001);
    debug_assert!(x < 216. / 24389. || t >= 16. / 116.);
    t
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

#[inline(never)]
fn rgb_to_lab<T: Copy + Sync + Send + 'static, F>(img: ImgRef<'_, T>, cb: F) -> Vec<GBitmap>
    where F: Fn(T, usize) -> (f32, f32, f32) + Sync + Send + 'static
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
        let l_row = &mut l_row[0..width];
        let a_row = &mut a_row[0..width];
        let b_row = &mut b_row[0..width];
        for x in 0..width {
            let n = (x+11) ^ (y+11);
            let (l,a,b) = cb(in_row[x], n);
            l_row[x].write(l);
            a_row[x].write(a);
            b_row[x].write(b);
        }
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

impl ToLABBitmap for ImgRef<'_, RGBAPLU> {
    #[inline]
    fn to_lab(&self) -> Vec<GBitmap> {
        #[cfg(target_arch = "x86_64")]
        if simd_x86::has_avx2_fma() {
            // SAFETY: capability gate above guarantees AVX2+FMA at runtime.
            return unsafe { simd_x86::rgbaplu_to_lab(*self) };
        }
        #[cfg(target_arch = "aarch64")]
        if simd_neon::has_neon() {
            // SAFETY: capability gate above guarantees NEON at runtime.
            return unsafe { simd_neon::rgbaplu_to_lab(*self) };
        }
        rgb_to_lab(*self, |px, n| px.to_rgb(n).to_lab())
    }
}

impl ToLABBitmap for ImgRef<'_, RGBLU> {
    #[inline]
    fn to_lab(&self) -> Vec<GBitmap> {
        #[cfg(target_arch = "x86_64")]
        if simd_x86::has_avx2_fma() {
            // SAFETY: capability gate above guarantees AVX2+FMA at runtime.
            return unsafe { simd_x86::rgblu_to_lab(*self) };
        }
        #[cfg(target_arch = "aarch64")]
        if simd_neon::has_neon() {
            // SAFETY: capability gate above guarantees NEON at runtime.
            return unsafe { simd_neon::rgblu_to_lab(*self) };
        }
        rgb_to_lab(*self, |px, _n| px.to_lab())
    }
}

// SIMD `tolab` paths live in submodules to keep this file focused on the
// scalar implementation and dispatch. Each submodule is runtime-dispatched
// via its own capability-check (`has_avx2_fma()` / `has_neon()`).
#[cfg(target_arch = "x86_64")]
mod simd_x86;

#[cfg(target_arch = "aarch64")]
mod simd_neon;

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

/// Reference scalar cube root using the *previous* polynomial-seed initial
/// estimate (`y = 0.2 + 1.51·x − 0.5·x²`) followed by 2 Halley iterations.
/// Kept inline for the brute-force comparison test below — both this and the
/// current `cbrt_poly` (bit-trick seed) should converge to within 1 ULP of
/// `f32::cbrt` over [0, 1] after 2 Halley steps, so any divergence between
/// them larger than ~3×ULP indicates a regression in either path.
#[cfg(test)]
fn cbrt_poly_old(x: f32) -> f32 {
    if x == 0.0 { return 0.0; }
    let poly = [0.2f32, 1.51, -0.5];
    let y = poly[2].mul_add(x, poly[1]).mul_add(x, poly[0]);
    let y3 = y * y * y;
    let y = y * 2.0f32.mul_add(x, y3) / 2.0f32.mul_add(y3, x);
    let y3 = y * y * y;
    y * 2.0f32.mul_add(x, y3) / 2.0f32.mul_add(y3, x)
}

/// Brute-force comparison over the *use range* `[EPSILON, 1]` (cbrt is masked
/// out below EPSILON in `to_lab`, so no path consumes the seed there).
/// 100 001 dense samples; asserts both scalar variants stay within 5×10⁻⁵
/// absolute error of the IEEE `f32::cbrt`, and pairwise within the same
/// bound. Catches any regression in either seed or Halley step.
#[test]
fn cbrt_old_vs_new_brute() {
    let lo = EPSILON as f64;
    let span = 1.0 - lo;
    let n = 100_001u32;
    let mut max_new_err: f64 = 0.0;
    let mut max_old_err: f64 = 0.0;
    let mut max_pair_diff: f64 = 0.0;
    for i in 0..=n {
        let x = (lo + span * f64::from(i) / f64::from(n)) as f32;
        let new = cbrt_poly(x);
        let old = cbrt_poly_old(x);
        let truth = f64::from(x).cbrt();
        let new_err = (f64::from(new) - truth).abs();
        let old_err = (f64::from(old) - truth).abs();
        let diff = (f64::from(new) - f64::from(old)).abs();
        max_new_err = max_new_err.max(new_err);
        max_old_err = max_old_err.max(old_err);
        max_pair_diff = max_pair_diff.max(diff);
        assert!(new_err < 5e-5, "new cbrt off: x={x} new={new} truth={truth} err={new_err}");
        assert!(old_err < 5e-5, "old cbrt off: x={x} old={old} truth={truth} err={old_err}");
        assert!(diff   < 5e-5, "old vs new diverge: x={x} new={new} old={old} diff={diff}");
    }
    println!("cbrt brute force [EPSILON,1]: n={n}, max_new_err={max_new_err:.3e}, max_old_err={max_old_err:.3e}, max_pair_diff={max_pair_diff:.3e}");
}

/// Below-EPSILON sanity check: confirms scalar `cbrt_poly` returns the
/// linear-tail-friendly value (zero at exactly zero, monotonic for positive
/// small) without panicking. Output is always discarded by the
/// `f > EPSILON ? cbrt(f) - bias : K·f` mask in `to_lab`, so we don't pin
/// numeric accuracy below the threshold — only finiteness.
#[test]
fn cbrt_below_epsilon_sane() {
    assert_eq!(cbrt_poly(0.0), 0.0);
    for &x in &[1e-12_f32, 1e-9, 1e-6, 1e-4, 1e-3, EPSILON / 2.0] {
        let y = cbrt_poly(x);
        assert!(y.is_finite(), "cbrt({x}) = {y} not finite");
        assert!(y >= 0.0, "cbrt({x}) = {y} negative");
    }
}

/// AVX2+FMA SIMD cbrt vs scalar cbrt over the use range `[EPSILON, 1]`.
/// Skipped when the CPU lacks AVX2/FMA. Lifts 100 008 inputs through
/// `cbrt_x8` 8-at-a-time and checks per-lane outputs against scalar
/// `cbrt_poly` (bit-trick seed). The SIMD path uses the polynomial seed,
/// so the cross-seed gap inside the use range is what's locked down.
#[cfg(target_arch = "x86_64")]
#[test]
fn cbrt_simd_x8_matches_scalar() {
    if !simd_x86::has_avx2_fma() {
        eprintln!("skipping: AVX2+FMA not detected");
        return;
    }
    let lo = EPSILON as f64;
    let span = 1.0 - lo;
    let n = 100_008usize; // multiple of 8
    let mut max_diff: f64 = 0.0;
    let mut buf = [0.0f32; 8];
    let mut i = 0;
    while i + 8 <= n {
        for k in 0..8 {
            buf[k] = (lo + span * (i + k) as f64 / n as f64) as f32;
        }
        // SAFETY: AVX2+FMA confirmed by has_avx2_fma() above.
        let out = unsafe { simd_x86::cbrt_x8_test(buf) };
        for k in 0..8 {
            let scalar = cbrt_poly(buf[k]);
            let diff = (f64::from(out[k]) - f64::from(scalar)).abs();
            max_diff = max_diff.max(diff);
            assert!(diff < 5e-5,
                "SIMD/scalar cbrt diverge: x={} simd={} scalar={} diff={diff}",
                buf[k], out[k], scalar);
        }
        i += 8;
    }
    println!("cbrt_x8 vs scalar [EPSILON,1]: n={i}, max_diff={max_diff:.3e}");
}

/// NEON SIMD cbrt vs scalar cbrt over the use range `[EPSILON, 1]`. NEON is
/// mandatory on aarch64 so no runtime gate.
#[cfg(target_arch = "aarch64")]
#[test]
fn cbrt_simd_x4_matches_scalar() {
    let lo = EPSILON as f64;
    let span = 1.0 - lo;
    let n = 100_004usize; // multiple of 4
    let mut max_diff: f64 = 0.0;
    let mut buf = [0.0f32; 4];
    let mut i = 0;
    while i + 4 <= n {
        for k in 0..4 {
            buf[k] = (lo + span * (i + k) as f64 / n as f64) as f32;
        }
        // SAFETY: NEON is a baseline aarch64 feature.
        let out = unsafe { simd_neon::cbrt_x4_test(buf) };
        for k in 0..4 {
            let scalar = cbrt_poly(buf[k]);
            let diff = (f64::from(out[k]) - f64::from(scalar)).abs();
            max_diff = max_diff.max(diff);
            assert!(diff < 5e-5,
                "SIMD/scalar cbrt diverge: x={} simd={} scalar={} diff={diff}",
                buf[k], out[k], scalar);
        }
        i += 4;
    }
    println!("cbrt_x4 vs scalar [EPSILON,1]: n={i}, max_diff={max_diff:.3e}");
}
