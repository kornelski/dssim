//! Five moment rows for streaming comparison. All buffers are initialized
//! f32 slices with checked indexing. The only unsafe operations are calls
//! guarded by the matching CPU-feature checks; no pointer casts or set_len.
use super::{K5_EDGE_CENTER, K5_EDGE_NEAR, K5_EDGE_FAR, K5_OUTER, K5_INNER, K5_MID};

/// Clamped vertical tap indices [-2,-1,0,+1,+2] for output row `y` and
/// the matching edge kind — mirroring `blur_v5`'s
/// row selection, checked against the independent full-plane implementation.
pub(crate) fn v5_window(y: usize, height: usize) -> ([usize; 5], V5Edge) {
    let last = height - 1;
    let taps = [
        y.saturating_sub(2),
        y.saturating_sub(1),
        y,
        (y + 1).min(last),
        (y + 2).min(last),
    ];
    let edge = if y == 0 {
        V5Edge::Top
    } else if y == last {
        V5Edge::Bottom
    } else {
        V5Edge::None
    };
    (taps, edge)
}

/// One output row of the vertical 5-tap combine. `taps` are the five
/// source rows at clamped indices [-2,-1,0,+1,+2] around the output
/// row; `edge` picks the H1·H1-derived 3-coefficient form at y=0 and
/// y=height-1, plain 5-tap everywhere else.
///
/// The `match` sits inside the loop: `edge` is loop-invariant, so
/// LLVM unswitches it — one source loop, three specialized codegen
/// paths (the interior one vectorized under AVX2+FMA).
#[inline(always)]
fn v5_combine_row_inline(taps: [&[f32]; 5], edge: V5Edge, out: &mut [f32]) {
    let [m2, m1, c, p1, p2] = taps;
    for (x, o) in out.iter_mut().enumerate() {
        *o = match edge {
            V5Edge::None => {
                (m2[x] + p2[x]) * K5_OUTER + (m1[x] + p1[x]) * K5_INNER + c[x] * K5_MID
            }
            V5Edge::Top => {
                K5_EDGE_CENTER * c[x] + K5_EDGE_NEAR * p1[x] + K5_EDGE_FAR * p2[x]
            }
            V5Edge::Bottom => {
                K5_EDGE_FAR * m2[x] + K5_EDGE_NEAR * m1[x] + K5_EDGE_CENTER * c[x]
            }
        };
    }
}

#[inline(never)]
fn v5_combine_row_base(taps: [&[f32]; 5], edge: V5Edge, out: &mut [f32]) {
    v5_combine_row_inline(taps, edge, out);
}

/// AVX2+FMA clone of `v5_combine_row_base`; same source, vectorized
/// wider. SAFETY: call only when `caps::has_avx2_fma()` has confirmed
/// support.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma")]
fn v5_combine_row_avx2(taps: [&[f32]; 5], edge: V5Edge, out: &mut [f32]) {
    v5_combine_row_inline(taps, edge, out);
}

/// AVX-512 clone of `v5_combine_row_base` (AVX-512 F/BW/DQ/VL —
/// `avx512cd` unused by these kernels).
/// SAFETY: call only when `caps::has_avx512()` has confirmed support.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma,avx512f,avx512bw,avx512dq,avx512vl")]
fn v5_combine_row_avx512(taps: [&[f32]; 5], edge: V5Edge, out: &mut [f32]) {
    v5_combine_row_inline(taps, edge, out);
}

/// Runtime dispatch, resolved once per row.
#[inline]
fn v5_combine_row(taps: [&[f32]; 5], edge: V5Edge, out: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx512() {
        // SAFETY: has_avx512() confirmed AVX2/FMA and AVX-512 F/BW/DQ/VL.
        unsafe { v5_combine_row_avx512(taps, edge, out) };
        return;
    }
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx2_fma() {
        // SAFETY: has_avx2_fma() confirmed AVX2+FMA support.
        unsafe { v5_combine_row_avx2(taps, edge, out) };
        return;
    }
    v5_combine_row_base(taps, edge, out);
}

/// Vertical 5-tap blur, bit-equivalent to two sequential clamped 1D
/// Scalar edge columns shared by all moment outputs: the same
/// clamped-tap math as `blur_h5`, applied to an arbitrary per-pixel
/// product of the two input rows. Writes j ∈ {0, 1, w-2, w-1}
/// (deduplicated for tiny widths) and leaves the interior untouched.
#[inline(always)]
fn h5_edges(
    r1: &[f32],
    r2: &[f32],
    prod: impl Fn(&[f32], &[f32], usize) -> f32,
    out: &mut [f32],
) {
    let width = out.len();
    let last = width - 1;
    let p = |i: usize| prod(r1, r2, i);
    out[0] = K5_EDGE_CENTER * p(0) + K5_EDGE_NEAR * p(1.min(last)) + K5_EDGE_FAR * p(2.min(last));
    if width >= 2 {
        out[last] = K5_EDGE_FAR * p(last.saturating_sub(2)) + K5_EDGE_NEAR * p(last - 1) + K5_EDGE_CENTER * p(last);
    }
    if width >= 3 {
        out[1] = (p(0) + p(3.min(last))) * K5_OUTER + (p(0) + p(2.min(last))) * K5_INNER + p(1) * K5_MID;
    }
    if width >= 4 {
        let i = last - 1;
        out[i] = (p(i - 2) + p((i + 2).min(last))) * K5_OUTER + (p(i - 1) + p(i + 1)) * K5_INNER + p(i) * K5_MID;
    }
}

/// One row of the fused horizontal moments pass: writes h5(i1), h5(i2),
/// h5(i1*i1), h5(i2*i2), h5(i1*i2) for a single source row pair.
/// Processes both inputs together. `rows[p]` must have `r1.len()` cells.
///
/// `#[inline(always)]` so the AVX2+FMA wrapper re-vectorizes this same
/// body under its target features — five shifted sub-slices per input
/// row give LLVM the same unit-stride stencil it vectorizes in
/// `blur5_inner_inline`.
#[inline(always)]
fn blur_h5_moments_row_inline(
    r1: &[f32],
    r2: &[f32],
    rows: [&mut [f32]; 5],
) {
    let width = r1.len();
    debug_assert!(width >= 1);
    let inner = width.saturating_sub(4);
    let mut rows = rows;

    // Edges: same clamped-tap math as blur_h5, one per product.
    h5_edges(r1, r2, |a, _b, i| a[i], rows[0]);
    h5_edges(r1, r2, |_a, b, i| b[i], rows[1]);
    h5_edges(r1, r2, |a, _b, i| a[i] * a[i], rows[2]);
    h5_edges(r1, r2, |_a, b, i| b[i] * b[i], rows[3]);
    h5_edges(r1, r2, |a, b, i| a[i] * b[i], rows[4]);

    if inner > 0 {
        // Interior x ∈ [2, w-2): tap k of output x+k... i.e. output
        // index k+2 reads input offsets k..k+4 — five aligned
        // sub-slices like `blur_h5` builds for `blur5_inner`.
        let a = [
            &r1[..inner],
            &r1[1..=inner],
            &r1[2..2 + inner],
            &r1[3..3 + inner],
            &r1[4..4 + inner],
        ];
        let b = [
            &r2[..inner],
            &r2[1..=inner],
            &r2[2..2 + inner],
            &r2[3..3 + inner],
            &r2[4..4 + inner],
        ];
        let [d0, d1, d2, d3, d4] = rows.each_mut().map(|r| &mut r[2..2 + inner]);
        for k in 0..inner {
            let (x0, x1, x2, x3, x4) = (a[0][k], a[1][k], a[2][k], a[3][k], a[4][k]);
            let (y0, y1, y2, y3, y4) = (b[0][k], b[1][k], b[2][k], b[3][k], b[4][k]);
            d0[k] = (x0 + x4) * K5_OUTER + (x1 + x3) * K5_INNER + x2 * K5_MID;
            d1[k] = (y0 + y4) * K5_OUTER + (y1 + y3) * K5_INNER + y2 * K5_MID;
            let (p0, p1, p2, p3, p4) = (x0 * x0, x1 * x1, x2 * x2, x3 * x3, x4 * x4);
            d2[k] = (p0 + p4) * K5_OUTER + (p1 + p3) * K5_INNER + p2 * K5_MID;
            let (q0, q1, q2, q3, q4) = (y0 * y0, y1 * y1, y2 * y2, y3 * y3, y4 * y4);
            d3[k] = (q0 + q4) * K5_OUTER + (q1 + q3) * K5_INNER + q2 * K5_MID;
            let (c0, c1, c2, c3, c4) = (x0 * y0, x1 * y1, x2 * y2, x3 * y3, x4 * y4);
            d4[k] = (c0 + c4) * K5_OUTER + (c1 + c3) * K5_INNER + c2 * K5_MID;
        }
    }
}

#[inline(never)]
fn blur_h5_moments_row_base(r1: &[f32], r2: &[f32], rows: [&mut [f32]; 5]) {
    blur_h5_moments_row_inline(r1, r2, rows);
}

/// AVX2+FMA clone of `blur_h5_moments_row_base`; same source, vectorized
/// wider. SAFETY: call only when `caps::has_avx2_fma()` has confirmed
/// support.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma")]
fn blur_h5_moments_row_avx2(r1: &[f32], r2: &[f32], rows: [&mut [f32]; 5]) {
    blur_h5_moments_row_inline(r1, r2, rows);
}

/// AVX-512 clone of `blur_h5_moments_row_base` (AVX-512 F/BW/DQ/VL
/// — `avx512cd` unused by these kernels).
/// SAFETY: call only when `caps::has_avx512()` has confirmed support.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma,avx512f,avx512bw,avx512dq,avx512vl")]
fn blur_h5_moments_row_avx512(r1: &[f32], r2: &[f32], rows: [&mut [f32]; 5]) {
    blur_h5_moments_row_inline(r1, r2, rows);
}

/// Row-level dispatch for the fused horizontal moments pass.
/// `rows[p]` gets the horizontal blur of `r1`, `r2`, `r1*r1`,
/// `r2*r2`, `r1*r2` respectively.
pub fn blur_moments_row(r1: &[f32], r2: &[f32], rows: [&mut [f32]; 5]) {
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx512() {
        // SAFETY: has_avx512() confirmed AVX2/FMA and AVX-512 F/BW/DQ/VL.
        unsafe { blur_h5_moments_row_avx512(r1, r2, rows) };
        return;
    }
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx2_fma() {
        // SAFETY: has_avx2_fma() confirmed AVX2+FMA support.
        unsafe { blur_h5_moments_row_avx2(r1, r2, rows) };
        return;
    }
    blur_h5_moments_row_base(r1, r2, rows);
}

/// Border kind for `blur_moments_v5_row`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum V5Edge {
    /// Output row 0: 3-coefficient edge form over the last three taps.
    Top,
    /// Output row height-1: mirrored 3-coefficient edge form.
    Bottom,
    /// Interior or near-edge row: plain clamped 5-tap.
    None,
}

/// Vertical 5-tap combine producing one output row for all five moment
/// planes at once. `taps[p]` = the five H-filtered rows for product `p`
/// at clamped indices [-2,-1,0,+1,+2] around the output row — i.e. the
/// same row selection `v5_window` produces for `blur_v5`.
pub fn blur_moments_v5_row(
    taps: [[&[f32]; 5]; 5],
    edge: V5Edge,
    mut out: [&mut [f32]; 5],
) {
    for (p, o) in out.iter_mut().enumerate() {
        v5_combine_row(taps[p], edge, o);
    }
}


#[cfg(test)]
#[path = "moments_tests.rs"]
mod tests;
