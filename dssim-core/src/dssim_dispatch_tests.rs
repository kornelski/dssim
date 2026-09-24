#![allow(unused_mut)] // Feature-specific kernel lists also compile on non-x86.

use super::*;

#[test]
fn ssim3_is_bit_exact_across_tiers() {
    let mut kernels: Vec<unsafe fn(&Ssim3Planes<'_>, usize, &mut [f32])> = vec![ssim3_range_base, ssim3_range];
    #[cfg(target_arch = "x86_64")]
    {
        if crate::caps::has_avx2_fma() { kernels.push(ssim3_range_avx2); }
        if crate::caps::has_avx512() { kernels.push(ssim3_range_avx512); }
    }
    for n in [0, 1, 7, 8, 15, 16, 17, 4096 + 13] {
        let planes: Vec<Vec<f32>> = (0..15).map(|seed| (0..n+3).map(|i| {
            let x = (i as u32).wrapping_mul(2_654_435_761).wrapping_add(seed);
            ((x ^ (x >> 16)) & 0xFFFF) as f32 / 65536.0
        }).collect()).collect();
        let s = Ssim3Planes {
            mu1: [&planes[0], &planes[1], &planes[2]], mu2: [&planes[3], &planes[4], &planes[5]],
            sq1: [&planes[6], &planes[7], &planes[8]], sq2: [&planes[9], &planes[10], &planes[11]],
            i12: [&planes[12], &planes[13], &planes[14]],
        };
        let mut expected = vec![0.0; n];
        ssim3_range_base(&s, 3, &mut expected);
        for kernel in &kernels {
            let mut out = vec![f32::NAN; n];
            // SAFETY: feature clones were included only after their CPU check.
            unsafe { kernel(&s, 3, &mut out) };
            assert_eq!(out.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>());
        }
    }
}
