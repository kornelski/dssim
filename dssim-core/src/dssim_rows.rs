//! Cached-pair comparison and streaming missing moments through initialized rows.
//! New unsafe calls enter feature-checked single-channel kernels below.
use super::*;

impl Dssim {
    /// Fused moments→SSIM for one scale, row-blocked: the horizontal
    /// moments pass writes a 5-slot ring buffer, vertical combine produces
    /// the five blurred moments for a single output row, and the SSIM
    /// kernel consumes them directly — the ~15 transient planes of the
    /// plane-at-a-time pipeline never materialize.
    ///
    /// Ring layout: `hring[slot][c][p]` — slot is `src_row % 5`, `c` the
    /// channel, `p` the product index [mu1, mu2, sq1, sq2, i12].
    /// NEED1/NEED2 select uncached inputs at compile time; cached moments
    /// are read directly from their image, outside the pixel arithmetic.
    fn ssim_rows<const NEED1: bool, const NEED2: bool>(
        original: &DssimChanScale<f32>,
        modified: &DssimChanScale<f32>,
        y0: usize,
        y1: usize,
        out: &mut [f32],
    ) {
        let nchan = original.chan.len();
        let width = original.chan[0].width;
        let height = original.chan[0].height;
        debug_assert!(original.chan.iter().chain(&modified.chan)
            .all(|c| c.width == width && c.height == height));
        debug_assert_eq!(out.len(), width * (y1 - y0));
        let mut hring = vec![0.0_f32; 5 * 5 * nchan * width];
        let mut vrow = vec![0.0_f32; 5 * nchan * width];
        // src row held by each ring slot (usize::MAX = never written) —
        // Checks ring scheduling in debug builds; all storage is initialized.
        let mut slot_src = [usize::MAX; 5];

        let mut hnext = y0.saturating_sub(2);
        for (i, y) in (y0..y1).enumerate() {
            // Produce the H-filtered rows this output row's V taps need.
            let need = (y + 2).min(height - 1);
            while hnext <= need {
                let slot_seg = &mut hring[(hnext % 5) * 5 * nchan * width..][..5 * nchan * width];
                for (chan_seg, (chan1, chan2)) in slot_seg
                    .chunks_mut(5 * width)
                    .zip(original.chan.iter().zip(&modified.chan))
                {
                    let r1 = &chan1.img[hnext];
                    let r2 = &chan2.img[hnext];
                    let mut rows = chan_seg.chunks_mut(width);
                    blur::moments::blur_moments_row::<NEED1, NEED2>(r1, r2, core::array::from_fn(|_| rows.next().unwrap()));
                }
                slot_src[hnext % 5] = hnext;
                hnext += 1;
            }

            let (tap_src, edge) = blur::moments::v5_window(y, height);
            // `hring` slice for product `p` of channel `c` from source row `src`.
            let hrow = |src: usize, c: usize, p: usize| -> &[f32] {
                &hring[((src % 5) * 5 * nchan + c * 5 + p) * width..][..width]
            };
            for c in 0..nchan {
                let taps: [[&[f32]; 5]; 5] = core::array::from_fn(|p| {
                    tap_src.map(|src| {
                        debug_assert_eq!(slot_src[src % 5], src);
                        hrow(src, c, p)
                    })
                });
                let mut outs = vrow[c * 5 * width..][..5 * width].chunks_mut(width);
                blur::moments::blur_moments_v5_row::<NEED1, NEED2>(taps, edge, core::array::from_fn(|_| outs.next().unwrap()));
            }

            let v = |c: usize, p: usize| -> &[f32] {
                if p < 4 && ![NEED1, NEED2][p % 2] {
                    let chan = if p % 2 == 0 { &original.chan[c] } else { &modified.chan[c] };
                    let moments = chan.moments.as_ref().unwrap();
                    let plane = if p < 2 { &moments.mu } else { &moments.squared };
                    return &plane[y * width..][..width];
                }
                &vrow[(c * 5 + p) * width..][..width]
            };
            let out_row = &mut out[i * width..][..width];
            if nchan == 3 {
                let inputs = Ssim3Planes {
                    mu1: [v(0, 0), v(1, 0), v(2, 0)],
                    mu2: [v(0, 1), v(1, 1), v(2, 1)],
                    sq1: [v(0, 2), v(1, 2), v(2, 2)],
                    sq2: [v(0, 3), v(1, 3), v(2, 3)],
                    i12: [v(0, 4), v(1, 4), v(2, 4)],
                };
                ssim3_range(&inputs, 0, out_row);
            } else {
                ssim1_range([v(0, 0), v(0, 1), v(0, 2), v(0, 3), v(0, 4)], out_row);
            }
        }
    }

    /// Preserve #197's full-plane cross blur when both images retain moments.
    /// It avoids the row-ring overhead for repeated comparisons of cached pairs.
    fn compare_scale_cached(original: &DssimChanScale<f32>, modified: &DssimChanScale<f32>) -> ImgVec<f32> {
        let width = original.chan[0].width;
        let height = original.chan[0].height;
        let pixels = width * height;
        let nchan = original.chan.len();
        let cross: Vec<Vec<f32>> = (0..nchan).into_par_iter().map(|c| {
            let mut tmp = Vec::<f32>::with_capacity(pixels);
            blur::blur_mul(original.chan[c].img.as_ref(), modified.chan[c].img.as_ref(),
                &mut tmp.spare_capacity_mut()[..pixels])
        }).collect();
        let moments = |c: usize, p: usize| -> &[f32] {
            if p == 4 { return &cross[c]; }
            let chan = if p % 2 == 0 { &original.chan[c] } else { &modified.chan[c] };
            let cache = chan.moments.as_ref().unwrap();
            if p < 2 { &cache.mu } else { &cache.squared }
        };
        let mut out = vec![0.0; pixels];
        if nchan == 3 {
            let inputs = Ssim3Planes {
                mu1: std::array::from_fn(|c| moments(c, 0)),
                mu2: std::array::from_fn(|c| moments(c, 1)),
                sq1: std::array::from_fn(|c| moments(c, 2)),
                sq2: std::array::from_fn(|c| moments(c, 3)),
                i12: std::array::from_fn(|c| moments(c, 4)),
            };
            out.as_mut_slice().par_chunks_mut(4096).enumerate().for_each(|(i, chunk)| {
                ssim3_range(&inputs, i * 4096, chunk);
            });
        } else {
            out.as_mut_slice().par_chunks_mut(4096).enumerate().for_each(|(i, chunk)| {
                let start = i * 4096;
                ssim1_range(std::array::from_fn(|p| &moments(0, p)[start..][..chunk.len()]), chunk);
            });
        }
        ImgVec::new(out, width, height)
    }

    /// Reuse the full-plane path for cached pairs; otherwise stream only
    /// missing moments in `FUSED_ROWS`-row blocks.
    #[inline(never)]
    pub(super) fn compare_scale_fused(
        original: &DssimChanScale<f32>,
        modified: &DssimChanScale<f32>,
    ) -> ImgVec<f32> {
        let nchan = original.chan.len();
        assert!(nchan == 1 || nchan == 3);
        let width = original.chan[0].width;
        let height = original.chan[0].height;
        let pixels = width * height;
        assert_eq!(modified.chan.len(), nchan);
        assert!(width > 0 && height > 0);
        assert!(original.chan.iter().chain(&modified.chan)
            .all(|c| c.width == width && c.height == height));
        let needed = [original.chan[0].moments.is_none(), modified.chan[0].moments.is_none()];
        debug_assert!(original.chan.iter().all(|c| c.moments.is_none() == needed[0]));
        debug_assert!(modified.chan.iter().all(|c| c.moments.is_none() == needed[1]));
        if needed == [false, false] {
            return Self::compare_scale_cached(original, modified);
        }
        let mut map_out = vec![0.0; pixels];

        /// Rows per parallel task. Each block recomputes four boundary H
        /// rows, so larger blocks do less duplicate work; 16 keeps ~64
        /// tasks at scale 0 — enough for work stealing.
        const FUSED_ROWS: usize = 16;
        map_out.as_mut_slice().par_chunks_mut(width * FUSED_ROWS).enumerate().for_each(|(bi, block)| {
            let y0 = bi * FUSED_ROWS;
            let y1 = (y0 + block.len() / width).min(height);
            // Select once per block; the pixel loops have no cache-policy branches.
            match needed {
                [true, true] => Self::ssim_rows::<true, true>(original, modified, y0, y1, block),
                [true, false] => Self::ssim_rows::<true, false>(original, modified, y0, y1, block),
                [false, true] => Self::ssim_rows::<false, true>(original, modified, y0, y1, block),
                [false, false] => unreachable!("cached pair handled above"),
            }
        });

        ImgVec::new(map_out, width, height)
    }
}

/// Per-pixel single-channel SSIM value — the arithmetic previously inlined
/// in `compare_scale`'s map closure. `p` is [mu1, mu2, sq1, sq2, i12].
#[inline(always)]
pub(super) fn ssim1_px(p: &[&[f32]; 5], i: usize) -> f32 {
    let c1: f32 = 0.01 * 0.01;
    let c2: f32 = 0.03 * 0.03;

    let mu1 = p[0][i];
    let mu2 = p[1][i];
    let mu1mu1 = mu1 * mu1;
    let mu1mu2 = mu1 * mu2;
    let mu2mu2 = mu2 * mu2;
    let sigma1_sq = p[2][i] - mu1mu1;
    let sigma2_sq = p[3][i] - mu2mu2;
    let sigma12 = p[4][i] - mu1mu2;

    2.0f32.mul_add(mu1mu2, c1) * 2.0f32.mul_add(sigma12, c2)
        / ((mu1mu1 + mu2mu2 + c1) * (sigma1_sq + sigma2_sq + c2))
}

/// `ssim1_px` over `out[k] = px(k)`. Same inline/vectorize pattern as
/// `ssim3_range_inline`.
#[inline(always)]
fn ssim1_range_inline(p: [&[f32]; 5], out: &mut [f32]) {
    for (k, d) in out.iter_mut().enumerate() {
        *d = ssim1_px(&p, k);
    }
}

#[inline(never)]
fn ssim1_range_base(p: [&[f32]; 5], out: &mut [f32]) {
    ssim1_range_inline(p, out);
}

/// AVX2+FMA clone of `ssim1_range_base`; same source, vectorized wider.
/// SAFETY: call only when `caps::has_avx2_fma()` has confirmed support.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma")]
fn ssim1_range_avx2(p: [&[f32]; 5], out: &mut [f32]) {
    ssim1_range_inline(p, out);
}

/// AVX-512 clone of `ssim1_range_base` (AVX-512 F/BW/DQ/VL —
/// `avx512cd` unused by these kernels).
/// SAFETY: call only when `caps::has_avx512()` has confirmed support.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma,avx512f,avx512bw,avx512dq,avx512vl")]
fn ssim1_range_avx512(p: [&[f32]; 5], out: &mut [f32]) {
    ssim1_range_inline(p, out);
}

/// Runtime dispatch: best AVX-512/AVX2 kernel when detected, baseline
/// otherwise.
#[inline]
fn ssim1_range(p: [&[f32]; 5], out: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx512() {
        // SAFETY: has_avx512() confirmed AVX2/FMA and AVX-512 F/BW/DQ/VL.
        unsafe { ssim1_range_avx512(p, out) };
        return;
    }
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx2_fma() {
        // SAFETY: has_avx2_fma() confirmed AVX2+FMA support.
        unsafe { ssim1_range_avx2(p, out) };
        return;
    }
    ssim1_range_base(p, out);
}


#[test]
#[allow(unused_mut)]
fn ssim1_tiers_are_bit_exact() {
    type Kernel = unsafe fn([&[f32]; 5], &mut [f32]);
    let mut kernels: Vec<Kernel> = vec![ssim1_range_base, ssim1_range];
    #[cfg(target_arch = "x86_64")]
    {
        if crate::caps::has_avx2_fma() { kernels.push(ssim1_range_avx2); }
        if crate::caps::has_avx512() { kernels.push(ssim1_range_avx512); }
    }
    for n in [0,1,7,8,15,16,17,33,257] {
        let planes: [Vec<f32>;5] = std::array::from_fn(|p| (0..n).map(|i| ((i*13+p*7)%97) as f32/97.0).collect());
        let inputs = planes.each_ref().map(|p| p.as_slice());
        let mut expected=vec![0.0;n];ssim1_range_base(inputs,&mut expected);
        for kernel in &kernels {
            let mut actual=vec![f32::NAN;n];
            // SAFETY: only feature-checked kernels were added above.
            unsafe {kernel(inputs,&mut actual)};
            assert_eq!(actual.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),expected.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
        }
    }
}
