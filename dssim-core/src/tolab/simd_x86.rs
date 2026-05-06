//! AVX2 + FMA SIMD `tolab` path for x86_64. Runtime-dispatched on
//! `is_x86_feature_detected!("avx2") && ...!("fma")` cached in an
//! `AtomicU8`. Build-time shortcut when those features are statically
//! enabled (e.g. `-C target-feature=+avx2,+fma` or
//! `-C target-cpu=x86-64-v3`).

use super::{GBitmap, EPSILON, K, RGBAPLU, RGBLU, D65x, D65y, D65z};
#[cfg(not(feature = "threads"))]
use crate::lieon as rayon;
use core::arch::x86_64::*;
use imgref::*;
use rayon::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering};

// 0 = unknown, 1 = avx2+fma supported, 2 = not supported.
static CAP: AtomicU8 = AtomicU8::new(0);

pub(super) fn has_avx2_fma() -> bool {
    match CAP.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            let yes = is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma");
            CAP.store(if yes { 1 } else { 2 }, Ordering::Relaxed);
            yes
        }
    }
}

/// 8-wide cube root: same polynomial seed + 2 Halley iterations as the
/// scalar `cbrt_poly`, lifted onto __m256. Result is within 1 ULP of
/// `f32::cbrt` over [0, 1]. Pure value-compute — every intrinsic here is
/// safe-when-target-feature-enabled, so no `unsafe` blocks needed.
#[inline]
#[target_feature(enable = "avx2,fma")]
fn cbrt_x8(x: __m256) -> __m256 {
    // Polynomial seed: y = -0.5·x² + 1.51·x + 0.2
    let c0 = _mm256_set1_ps(0.2);
    let c1 = _mm256_set1_ps(1.51);
    let c2 = _mm256_set1_ps(-0.5);
    let y = _mm256_fmadd_ps(c2, x, c1);
    let y = _mm256_fmadd_ps(y, x, c0);

    let two = _mm256_set1_ps(2.0);

    // Halley step: y ← y · (2x + y³) / (2y³ + x)
    let y3 = _mm256_mul_ps(_mm256_mul_ps(y, y), y);
    let num = _mm256_fmadd_ps(two, x, y3);
    let den = _mm256_fmadd_ps(two, y3, x);
    let y = _mm256_mul_ps(y, _mm256_div_ps(num, den));

    let y3 = _mm256_mul_ps(_mm256_mul_ps(y, y), y);
    let num = _mm256_fmadd_ps(two, x, y3);
    let den = _mm256_fmadd_ps(two, y3, x);
    _mm256_mul_ps(y, _mm256_div_ps(num, den))
}

/// 8-wide RGB-linear → Lab using the same matrix coefficients,
/// epsilon-clamp, and final Lab transform as scalar `RGBLU::to_lab`.
#[inline]
#[target_feature(enable = "avx2,fma")]
fn to_lab_x8(r: __m256, g: __m256, b: __m256) -> (__m256, __m256, __m256) {
    let m00 = _mm256_set1_ps(0.4124 / D65x);
    let m01 = _mm256_set1_ps(0.3576 / D65x);
    let m02 = _mm256_set1_ps(0.1805 / D65x);
    let m10 = _mm256_set1_ps(0.2126 / D65y);
    let m11 = _mm256_set1_ps(0.7152 / D65y);
    let m12 = _mm256_set1_ps(0.0722 / D65y);
    let m20 = _mm256_set1_ps(0.0193 / D65z);
    let m21 = _mm256_set1_ps(0.1192 / D65z);
    let m22 = _mm256_set1_ps(0.9505 / D65z);

    // fx = m00·r + m01·g + m02·b ; same for fy, fz.
    let fx = _mm256_fmadd_ps(m00, r, _mm256_fmadd_ps(m01, g, _mm256_mul_ps(m02, b)));
    let fy = _mm256_fmadd_ps(m10, r, _mm256_fmadd_ps(m11, g, _mm256_mul_ps(m12, b)));
    let fz = _mm256_fmadd_ps(m20, r, _mm256_fmadd_ps(m21, g, _mm256_mul_ps(m22, b)));

    let eps = _mm256_set1_ps(EPSILON);
    let bias = _mm256_set1_ps(16.0 / 116.0);
    let k = _mm256_set1_ps(K);

    // X = fx > EPSILON ? cbrt(fx) - 16/116 : K · fx
    let cbrt_x = _mm256_sub_ps(cbrt_x8(fx), bias);
    let lin_x = _mm256_mul_ps(k, fx);
    let mask_x = _mm256_cmp_ps::<_CMP_GT_OQ>(fx, eps);
    let x_v = _mm256_blendv_ps(lin_x, cbrt_x, mask_x);

    let cbrt_y = _mm256_sub_ps(cbrt_x8(fy), bias);
    let lin_y = _mm256_mul_ps(k, fy);
    let mask_y = _mm256_cmp_ps::<_CMP_GT_OQ>(fy, eps);
    let y_v = _mm256_blendv_ps(lin_y, cbrt_y, mask_y);

    let cbrt_z = _mm256_sub_ps(cbrt_x8(fz), bias);
    let lin_z = _mm256_mul_ps(k, fz);
    let mask_z = _mm256_cmp_ps::<_CMP_GT_OQ>(fz, eps);
    let z_v = _mm256_blendv_ps(lin_z, cbrt_z, mask_z);

    // L = Y · 1.05;  a = (500/220)·(X-Y) + 86.2/220;  b = (200/220)·(Y-Z) + 107.9/220.
    let one_oh_five = _mm256_set1_ps(1.05);
    let a_scale = _mm256_set1_ps(500.0 / 220.0);
    let a_bias = _mm256_set1_ps(86.2 / 220.0);
    let b_scale = _mm256_set1_ps(200.0 / 220.0);
    let b_bias = _mm256_set1_ps(107.9 / 220.0);

    let l = _mm256_mul_ps(y_v, one_oh_five);
    let a = _mm256_fmadd_ps(a_scale, _mm256_sub_ps(x_v, y_v), a_bias);
    let b_out = _mm256_fmadd_ps(b_scale, _mm256_sub_ps(y_v, z_v), b_bias);
    (l, a, b_out)
}

/// Process one row of RGBLU pixels in 8-pixel chunks; scalar tail.
/// `l_row`, `a_row`, `b_row` are uninitialized; every cell in `[..width]`
/// is written before this returns.
#[target_feature(enable = "avx2,fma")]
fn rgblu_row(
    in_row: &[RGBLU],
    l_row: &mut [std::mem::MaybeUninit<f32>],
    a_row: &mut [std::mem::MaybeUninit<f32>],
    b_row: &mut [std::mem::MaybeUninit<f32>],
    width: usize,
) {
    let chunks = width / 8;

    let mut r_arr = [0.0f32; 8];
    let mut g_arr = [0.0f32; 8];
    let mut b_arr = [0.0f32; 8];

    for c in 0..chunks {
        let base = c * 8;
        for i in 0..8 {
            let p = in_row[base + i];
            r_arr[i] = p.r;
            g_arr[i] = p.g;
            b_arr[i] = p.b;
        }
        // SAFETY: each *_arr is a fully-initialized stack [f32; 8]; the
        // load reads exactly 8 in-bounds f32. The store writes 8 f32 at
        // `*_row[base..base+8]`, which is in bounds because
        // `base + 8 ≤ chunks * 8 ≤ width` and each row slice has length
        // `width` (asserted at the call site in `rgblu_to_lab`).
        unsafe {
            let r = _mm256_loadu_ps(r_arr.as_ptr());
            let g = _mm256_loadu_ps(g_arr.as_ptr());
            let b = _mm256_loadu_ps(b_arr.as_ptr());
            let (l, a, b_out) = to_lab_x8(r, g, b);
            _mm256_storeu_ps(l_row.as_mut_ptr().add(base).cast::<f32>(), l);
            _mm256_storeu_ps(a_row.as_mut_ptr().add(base).cast::<f32>(), a);
            _mm256_storeu_ps(b_row.as_mut_ptr().add(base).cast::<f32>(), b_out);
        }
    }

    // Scalar tail
    for i in (chunks * 8)..width {
        let p = in_row[i];
        let (l, a, b_out) = super::ToLAB::to_lab(&p);
        l_row[i].write(l);
        a_row[i].write(a);
        b_row[i].write(b_out);
    }
}

/// Process one row of RGBAPLU pixels with dither in 8-pixel chunks.
/// Mirrors `to_rgb(n).to_lab()`: composite premul-alpha onto a
/// dither-checkered ~white background, then convert to Lab.
/// `n_lane[i] = (x_base + i + 11) ^ (y + 11)` per pixel; channel masks
/// test bits 16 (R), 8 (G), 32 (B) and add `(1 - a)` when set.
#[target_feature(enable = "avx2,fma")]
fn rgbaplu_row(
    in_row: &[RGBAPLU],
    l_row: &mut [std::mem::MaybeUninit<f32>],
    a_row: &mut [std::mem::MaybeUninit<f32>],
    b_row: &mut [std::mem::MaybeUninit<f32>],
    width: usize,
    y: usize,
) {
    let chunks = width / 8;

    let one = _mm256_set1_ps(1.0);
    let bit_r = _mm256_set1_epi32(16);
    let bit_g = _mm256_set1_epi32(8);
    let bit_b = _mm256_set1_epi32(32);
    let zero_i = _mm256_setzero_si256();
    let y_xor = _mm256_set1_epi32((y + 11) as i32);
    // Pixel-index increments for the 8 lanes: (i + 11) for i in 0..8.
    let lane_off = _mm256_setr_epi32(11, 12, 13, 14, 15, 16, 17, 18);

    let mut r_arr = [0.0f32; 8];
    let mut g_arr = [0.0f32; 8];
    let mut b_arr = [0.0f32; 8];
    let mut a_arr = [0.0f32; 8];

    for c in 0..chunks {
        let base = c * 8;
        for i in 0..8 {
            let p = in_row[base + i];
            r_arr[i] = p.r;
            g_arr[i] = p.g;
            b_arr[i] = p.b;
            a_arr[i] = p.a;
        }
        // SAFETY: same in-bounds argument as `rgblu_row`'s loads/stores.
        let (r, g, b, a) = unsafe {
            (
                _mm256_loadu_ps(r_arr.as_ptr()),
                _mm256_loadu_ps(g_arr.as_ptr()),
                _mm256_loadu_ps(b_arr.as_ptr()),
                _mm256_loadu_ps(a_arr.as_ptr()),
            )
        };

        // n_i = ((x_base + i + 11) as i32) ^ y_xor
        let x_base_v = _mm256_set1_epi32(base as i32);
        let n = _mm256_xor_si256(_mm256_add_epi32(x_base_v, lane_off), y_xor);

        // mask_R = (n & 16) != 0  → ymm of all-1s when set, all-0s otherwise.
        let all_ones = _mm256_set1_epi32(-1);
        let mask_r = _mm256_xor_si256(
            _mm256_cmpeq_epi32(_mm256_and_si256(n, bit_r), zero_i),
            all_ones,
        );
        let mask_g = _mm256_xor_si256(
            _mm256_cmpeq_epi32(_mm256_and_si256(n, bit_g), zero_i),
            all_ones,
        );
        let mask_b = _mm256_xor_si256(
            _mm256_cmpeq_epi32(_mm256_and_si256(n, bit_b), zero_i),
            all_ones,
        );

        let one_minus_a = _mm256_sub_ps(one, a);
        let dither_r = _mm256_and_ps(_mm256_castsi256_ps(mask_r), one_minus_a);
        let dither_g = _mm256_and_ps(_mm256_castsi256_ps(mask_g), one_minus_a);
        let dither_b = _mm256_and_ps(_mm256_castsi256_ps(mask_b), one_minus_a);

        let r = _mm256_add_ps(r, dither_r);
        let g = _mm256_add_ps(g, dither_g);
        let b = _mm256_add_ps(b, dither_b);

        let (l, a_lab, b_lab) = to_lab_x8(r, g, b);
        // SAFETY: stores write 8 f32 into in-bounds segments of *_row.
        unsafe {
            _mm256_storeu_ps(l_row.as_mut_ptr().add(base).cast::<f32>(), l);
            _mm256_storeu_ps(a_row.as_mut_ptr().add(base).cast::<f32>(), a_lab);
            _mm256_storeu_ps(b_row.as_mut_ptr().add(base).cast::<f32>(), b_lab);
        }
    }

    // Scalar tail: fall back to scalar to_rgb + to_lab for the last <8 pixels.
    for i in (chunks * 8)..width {
        let n = (i + 11) ^ (y + 11);
        let (l, a_lab, b_lab) = super::ToLAB::to_lab(&super::ToRGB::to_rgb(in_row[i], n));
        l_row[i].write(l);
        a_row[i].write(a_lab);
        b_row[i].write(b_lab);
    }
}

/// SAFETY: caller must guarantee AVX2+FMA at runtime.
#[target_feature(enable = "avx2,fma")]
pub(super) unsafe fn rgblu_to_lab(img: ImgRef<'_, RGBLU>) -> Vec<GBitmap> {
    let width = img.width();
    let height = img.height();
    assert!(width > 0);
    let area = width * height;

    let mut out_l: Vec<f32> = Vec::with_capacity(area);
    let mut out_a: Vec<f32> = Vec::with_capacity(area);
    let mut out_b: Vec<f32> = Vec::with_capacity(area);

    out_l.spare_capacity_mut().par_chunks_exact_mut(width).take(height).zip(
        out_a.spare_capacity_mut().par_chunks_exact_mut(width).take(height).zip(
            out_b.spare_capacity_mut().par_chunks_exact_mut(width).take(height))
    ).enumerate().for_each(|(y, (l_row, (a_row, b_row)))| {
        let in_row = &img.rows().nth(y).unwrap()[0..width];
        rgblu_row(in_row, &mut l_row[..width], &mut a_row[..width], &mut b_row[..width], width);
    });

    // SAFETY: each per-row call wrote every cell in [..width] of its three
    // output rows; combined that's all `area` cells of each Vec.
    unsafe {
        out_l.set_len(area);
        out_a.set_len(area);
        out_b.set_len(area);
    }

    vec![
        Img::new(out_l, width, height),
        Img::new(out_a, width, height),
        Img::new(out_b, width, height),
    ]
}

/// SAFETY: caller must guarantee AVX2+FMA at runtime.
#[target_feature(enable = "avx2,fma")]
pub(super) unsafe fn rgbaplu_to_lab(img: ImgRef<'_, RGBAPLU>) -> Vec<GBitmap> {
    let width = img.width();
    let height = img.height();
    assert!(width > 0);
    let area = width * height;

    let mut out_l: Vec<f32> = Vec::with_capacity(area);
    let mut out_a: Vec<f32> = Vec::with_capacity(area);
    let mut out_b: Vec<f32> = Vec::with_capacity(area);

    out_l.spare_capacity_mut().par_chunks_exact_mut(width).take(height).zip(
        out_a.spare_capacity_mut().par_chunks_exact_mut(width).take(height).zip(
            out_b.spare_capacity_mut().par_chunks_exact_mut(width).take(height))
    ).enumerate().for_each(|(y, (l_row, (a_row, b_row)))| {
        let in_row = &img.rows().nth(y).unwrap()[0..width];
        rgbaplu_row(in_row, &mut l_row[..width], &mut a_row[..width], &mut b_row[..width], width, y);
    });

    // SAFETY: see analogous comment in `rgblu_to_lab`.
    unsafe {
        out_l.set_len(area);
        out_a.set_len(area);
        out_b.set_len(area);
    }

    vec![
        Img::new(out_l, width, height),
        Img::new(out_a, width, height),
        Img::new(out_b, width, height),
    ]
}
