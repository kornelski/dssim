#![allow(unused_mut)] // Feature-specific kernel lists also compile on non-x86.

use super::*;

#[test]
fn downsample_is_bit_exact_across_tiers() {
    let mut kernels: Vec<unsafe fn(&[RGBAPLU], &[RGBAPLU], &mut [RGBAPLU])> = vec![downsample_row_pair_base, downsample_row_pair];
    #[cfg(target_arch = "x86_64")]
    {
        if crate::caps::has_avx2_fma() { kernels.push(downsample_row_pair_avx2); }
        if crate::caps::has_avx512() { kernels.push(downsample_row_pair_avx512); }
    }
    let bits = |p: &RGBAPLU| [p.r.to_bits(), p.g.to_bits(), p.b.to_bits(), p.a.to_bits()];
    for n in [0, 1, 7, 8, 15, 16, 17, 257] {
        let top: Vec<_> = (0..2*n).map(|i| RGBAPLU::new(i as f32 / 997.0, (i % 13) as f32 / 13.0, 0.25, 1.0)).collect();
        let bot: Vec<_> = top.iter().rev().copied().collect();
        let mut expected = vec![RGBAPLU::new(0.0, 0.0, 0.0, 0.0); n];
        downsample_row_pair_base(&top, &bot, &mut expected);
        for kernel in &kernels {
            let mut out = vec![RGBAPLU::new(f32::NAN, f32::NAN, f32::NAN, f32::NAN); n];
            // SAFETY: feature clones were included only after their CPU check.
            unsafe { kernel(&top, &bot, &mut out) };
            assert_eq!(out.iter().map(bits).collect::<Vec<_>>(), expected.iter().map(bits).collect::<Vec<_>>());
        }
    }
}
