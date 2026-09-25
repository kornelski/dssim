#![allow(unused_mut)] // Feature-specific kernel lists also compile on non-x86.

use super::*;

#[test]
fn blur_rows_are_bit_exact_across_tiers() {
    type Plain = unsafe fn([&[f32]; 5], &mut [MaybeUninit<f32>]);
    type Product = unsafe fn([&[f32]; 5], [&[f32]; 5], &mut [MaybeUninit<f32>]);
    let mut kernels: Vec<(Plain, Product)> = vec![(blur5_inner_base, blur5_mul_inner_base), (blur5_inner, blur5_mul_inner)];
    #[cfg(target_arch = "x86_64")]
    {
        if crate::caps::has_avx2_fma() { kernels.push((blur5_inner_avx2, blur5_mul_inner_avx2)); }
        if crate::caps::has_avx512() { kernels.push((blur5_inner_avx512, blur5_mul_inner_avx512)); }
    }
    for n in [0, 1, 7, 8, 15, 16, 17, 257] {
        let data: Vec<_> = (0..n+4).map(|i| (i % 19) as f32 / 19.0).collect();
        let rows = std::array::from_fn(|i| &data[i..i+n]);
        for (plain, product) in &kernels {
            let mut out = vec![MaybeUninit::new(f32::NAN); n];
            let mut expected = out.clone();
            blur5_inner_base(rows, &mut expected);
            // SAFETY: feature clones were included only after their CPU check.
            unsafe { plain(rows, &mut out) };
            for (a, b) in out.iter().zip(&expected) {
                // SAFETY: every slot was initialized even before the kernel.
                assert_eq!(unsafe { a.assume_init() }.to_bits(), unsafe { b.assume_init() }.to_bits());
            }
            blur5_mul_inner_base(rows, rows, &mut expected);
            // SAFETY: feature clones were included only after their CPU check.
            unsafe { product(rows, rows, &mut out) };
            for (a, b) in out.iter().zip(&expected) {
                // SAFETY: every slot was initialized even before the kernel.
                assert_eq!(unsafe { a.assume_init() }.to_bits(), unsafe { b.assume_init() }.to_bits());
            }
        }
    }
}
