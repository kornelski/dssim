//! NEON SIMD `tolab` path for aarch64. Runtime-dispatched on
//! `is_aarch64_feature_detected!("neon")` cached in an `AtomicU8`.
//! Build-time shortcut when NEON is statically enabled (which is the
//! case for virtually every aarch64 target spec — NEON is the AArch64
//! ABI baseline).

use super::{GBitmap, EPSILON, K, RGBAPLU, RGBLU, D65x, D65y, D65z};
#[cfg(not(feature = "threads"))]
use crate::lieon as rayon;
use core::arch::aarch64::*;
use imgref::*;
use rayon::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering};

// 0 = unknown, 1 = neon supported, 2 = not supported.
static CAP: AtomicU8 = AtomicU8::new(0);

pub(super) fn has_neon() -> bool {
    // Build-time shortcut: virtually every aarch64 target spec already
    // enables NEON (it's the AArch64 ABI baseline). When `cfg!` reports
    // it's on, we skip the atomic load on every dispatch — the function
    // folds to `true` at compile time. The `is_aarch64_feature_detected!`
    // path remains for the embedded profiles that ship without NEON
    // (e.g. `aarch64-unknown-none-softfloat`).
    if cfg!(target_feature = "neon") {
        return true;
    }
    match CAP.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            let yes = std::arch::is_aarch64_feature_detected!("neon");
            CAP.store(if yes { 1 } else { 2 }, Ordering::Relaxed);
            yes
        }
    }
}

/// Test helper: run `cbrt_x4` on 4 scalar inputs and return 4 scalar outputs.
/// SAFETY: caller must guarantee NEON at runtime.
#[cfg(test)]
#[target_feature(enable = "neon")]
pub(super) unsafe fn cbrt_x4_test(input: [f32; 4]) -> [f32; 4] {
    // SAFETY: `input` is a fully-initialized stack [f32; 4]; the load reads
    // exactly 4 in-bounds f32s.
    let v = unsafe { vld1q_f32(input.as_ptr()) };
    let r = cbrt_x4(v);
    let mut out = [0.0f32; 4];
    // SAFETY: `out` is a stack [f32; 4]; the store writes 4 in-bounds f32s.
    unsafe { vst1q_f32(out.as_mut_ptr(), r) };
    out
}

/// 4-wide cube root. Same polynomial seed + 2 Halley iterations as the
/// scalar `cbrt_poly`; result within ~1 ULP of `f32::cbrt` on [0, 1].
/// Note: NEON `vfmaq_f32(a, b, c)` computes `a + b·c` (addend first).
/// Pure value-compute — every intrinsic here is safe-when-target-feature.
#[inline]
#[target_feature(enable = "neon")]
fn cbrt_x4(x: float32x4_t) -> float32x4_t {
    // y = -0.5·x² + 1.51·x + 0.2 = ((-0.5)·x + 1.51)·x + 0.2
    let c0 = vdupq_n_f32(0.2);
    let c1 = vdupq_n_f32(1.51);
    let c2 = vdupq_n_f32(-0.5);
    let y = vfmaq_f32(c1, c2, x); // 1.51 + (-0.5)·x
    let y = vfmaq_f32(c0, y, x); // 0.2 + y·x

    let two = vdupq_n_f32(2.0);

    // Halley: y ← y · (2x + y³) / (2y³ + x)
    let y3 = vmulq_f32(vmulq_f32(y, y), y);
    let num = vfmaq_f32(y3, two, x); // y3 + 2·x
    let den = vfmaq_f32(x, two, y3); // x + 2·y3
    let y = vmulq_f32(y, vdivq_f32(num, den));

    let y3 = vmulq_f32(vmulq_f32(y, y), y);
    let num = vfmaq_f32(y3, two, x);
    let den = vfmaq_f32(x, two, y3);
    vmulq_f32(y, vdivq_f32(num, den))
}

/// 4-wide RGB-linear → Lab using the same coefficients and conditional
/// epsilon-clamp as scalar `RGBLU::to_lab`.
#[inline]
#[target_feature(enable = "neon")]
fn to_lab_x4(r: float32x4_t, g: float32x4_t, b: float32x4_t)
    -> (float32x4_t, float32x4_t, float32x4_t)
{
    let m00 = vdupq_n_f32(0.4124 / D65x);
    let m01 = vdupq_n_f32(0.3576 / D65x);
    let m02 = vdupq_n_f32(0.1805 / D65x);
    let m10 = vdupq_n_f32(0.2126 / D65y);
    let m11 = vdupq_n_f32(0.7152 / D65y);
    let m12 = vdupq_n_f32(0.0722 / D65y);
    let m20 = vdupq_n_f32(0.0193 / D65z);
    let m21 = vdupq_n_f32(0.1192 / D65z);
    let m22 = vdupq_n_f32(0.9505 / D65z);

    // f = m00·r + m01·g + m02·b  (3 FMA chains)
    let fx = vfmaq_f32(vfmaq_f32(vmulq_f32(m02, b), m01, g), m00, r);
    let fy = vfmaq_f32(vfmaq_f32(vmulq_f32(m12, b), m11, g), m10, r);
    let fz = vfmaq_f32(vfmaq_f32(vmulq_f32(m22, b), m21, g), m20, r);

    let eps = vdupq_n_f32(EPSILON);
    let bias = vdupq_n_f32(16.0 / 116.0);
    let k = vdupq_n_f32(K);

    // Conditional: f > EPSILON ? cbrt(f) - bias : K·f
    let cbrt_x = vsubq_f32(cbrt_x4(fx), bias);
    let lin_x = vmulq_f32(k, fx);
    let mask_x = vcgtq_f32(fx, eps);
    let x_v = vbslq_f32(mask_x, cbrt_x, lin_x);

    let cbrt_y = vsubq_f32(cbrt_x4(fy), bias);
    let lin_y = vmulq_f32(k, fy);
    let mask_y = vcgtq_f32(fy, eps);
    let y_v = vbslq_f32(mask_y, cbrt_y, lin_y);

    let cbrt_z = vsubq_f32(cbrt_x4(fz), bias);
    let lin_z = vmulq_f32(k, fz);
    let mask_z = vcgtq_f32(fz, eps);
    let z_v = vbslq_f32(mask_z, cbrt_z, lin_z);

    // L = Y · 1.05;  a = (500/220)·(X-Y) + 86.2/220;  b = (200/220)·(Y-Z) + 107.9/220
    let one_oh_five = vdupq_n_f32(1.05);
    let a_scale = vdupq_n_f32(500.0 / 220.0);
    let a_bias = vdupq_n_f32(86.2 / 220.0);
    let b_scale = vdupq_n_f32(200.0 / 220.0);
    let b_bias = vdupq_n_f32(107.9 / 220.0);

    let l = vmulq_f32(y_v, one_oh_five);
    let a = vfmaq_f32(a_bias, a_scale, vsubq_f32(x_v, y_v));
    let b_out = vfmaq_f32(b_bias, b_scale, vsubq_f32(y_v, z_v));
    (l, a, b_out)
}

/// One row of RGBLU pixels in 4-pixel chunks; scalar tail.
/// Uses `vld3q_f32` to deinterleave AOS [r,g,b,r,g,b,…] directly.
#[target_feature(enable = "neon")]
fn rgblu_row(
    in_row: &[RGBLU],
    l_row: &mut [std::mem::MaybeUninit<f32>],
    a_row: &mut [std::mem::MaybeUninit<f32>],
    b_row: &mut [std::mem::MaybeUninit<f32>],
    width: usize,
) {
    let chunks = width / 4;
    let base_ptr = in_row.as_ptr() as *const f32;

    for c in 0..chunks {
        let base = c * 4;
        // SAFETY: 4 RGBLUs starting at `base_ptr + 3*base` are 12 contiguous
        // f32s (Rgb<f32> is `#[repr(C)]` so the layout is r,g,b,…); the
        // store writes 4 in-bounds f32s into each of l_row/a_row/b_row.
        unsafe {
            let rgb = vld3q_f32(base_ptr.add(3 * base));
            let (l, a, b_out) = to_lab_x4(rgb.0, rgb.1, rgb.2);
            vst1q_f32(l_row.as_mut_ptr().add(base).cast::<f32>(), l);
            vst1q_f32(a_row.as_mut_ptr().add(base).cast::<f32>(), a);
            vst1q_f32(b_row.as_mut_ptr().add(base).cast::<f32>(), b_out);
        }
    }

    // Scalar tail
    for i in (chunks * 4)..width {
        let p = in_row[i];
        let (l, a, b_out) = super::ToLAB::to_lab(&p);
        l_row[i].write(l);
        a_row[i].write(a);
        b_row[i].write(b_out);
    }
}

/// One row of RGBAPLU pixels in 4-pixel chunks with dither.
/// Uses `vld4q_f32` to deinterleave AOS [r,g,b,a,…] directly.
#[target_feature(enable = "neon")]
fn rgbaplu_row(
    in_row: &[RGBAPLU],
    l_row: &mut [std::mem::MaybeUninit<f32>],
    a_row: &mut [std::mem::MaybeUninit<f32>],
    b_row: &mut [std::mem::MaybeUninit<f32>],
    width: usize,
    y: usize,
) {
    let chunks = width / 4;
    let base_ptr = in_row.as_ptr() as *const f32;

    let one = vdupq_n_f32(1.0);
    let bit_r = vdupq_n_s32(16);
    let bit_g = vdupq_n_s32(8);
    let bit_b = vdupq_n_s32(32);
    let zero_i = vdupq_n_s32(0);
    let y_xor = vdupq_n_s32((y + 11) as i32);
    // (i + 11) for i in 0..4
    let lane_off_arr: [i32; 4] = [11, 12, 13, 14];
    // SAFETY: `lane_off_arr` is a fully-initialized stack [i32; 4].
    let lane_off = unsafe { vld1q_s32(lane_off_arr.as_ptr()) };

    for c in 0..chunks {
        let base = c * 4;
        // SAFETY: 4 RGBAPLUs starting at `base_ptr + 4*base` are 16
        // contiguous f32s; Rgba<f32> is `#[repr(C)]`.
        let rgba = unsafe { vld4q_f32(base_ptr.add(4 * base)) };
        let r = rgba.0;
        let g = rgba.1;
        let b = rgba.2;
        let a = rgba.3;

        // n_lane = ((base + i + 11) as i32) ^ y_xor
        let x_base_v = vdupq_n_s32(base as i32);
        let n = veorq_s32(vaddq_s32(x_base_v, lane_off), y_xor);

        // mask_R = (n & 16) != 0  → all-1s when set, all-0s otherwise.
        let mask_r = vmvnq_u32(vceqq_s32(vandq_s32(n, bit_r), zero_i));
        let mask_g = vmvnq_u32(vceqq_s32(vandq_s32(n, bit_g), zero_i));
        let mask_b_m = vmvnq_u32(vceqq_s32(vandq_s32(n, bit_b), zero_i));

        let one_minus_a = vsubq_f32(one, a);
        let zero_f = vdupq_n_f32(0.0);
        let dither_r = vbslq_f32(mask_r, one_minus_a, zero_f);
        let dither_g = vbslq_f32(mask_g, one_minus_a, zero_f);
        let dither_b = vbslq_f32(mask_b_m, one_minus_a, zero_f);

        let r = vaddq_f32(r, dither_r);
        let g = vaddq_f32(g, dither_g);
        let b = vaddq_f32(b, dither_b);

        let (l, a_lab, b_lab) = to_lab_x4(r, g, b);
        // SAFETY: stores write 4 f32 into in-bounds segments of *_row.
        unsafe {
            vst1q_f32(l_row.as_mut_ptr().add(base).cast::<f32>(), l);
            vst1q_f32(a_row.as_mut_ptr().add(base).cast::<f32>(), a_lab);
            vst1q_f32(b_row.as_mut_ptr().add(base).cast::<f32>(), b_lab);
        }
    }

    for i in (chunks * 4)..width {
        let n = (i + 11) ^ (y + 11);
        let (l, a_lab, b_lab) = super::ToLAB::to_lab(&super::ToRGB::to_rgb(in_row[i], n));
        l_row[i].write(l);
        a_row[i].write(a_lab);
        b_row[i].write(b_lab);
    }
}

/// SAFETY: caller must guarantee NEON at runtime (via `has_neon()`).
#[target_feature(enable = "neon")]
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

/// SAFETY: caller must guarantee NEON at runtime (via `has_neon()`).
#[target_feature(enable = "neon")]
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
