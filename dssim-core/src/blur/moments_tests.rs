#![allow(unused_mut)]
use super::*;

#[test]
fn moment_rows_match_all_supported_tiers() {
    type H = unsafe fn(&[f32], &[f32], [&mut [f32]; 5]);
    type V = unsafe fn([&[f32]; 5], V5Edge, &mut [f32]);
    let mut hs: Vec<H> = vec![blur_h5_moments_row_base, blur_moments_row];
    let mut vs: Vec<V> = vec![v5_combine_row_base, v5_combine_row];
    #[cfg(target_arch = "x86_64")]
    {
        if crate::caps::has_avx2_fma() { hs.push(blur_h5_moments_row_avx2); vs.push(v5_combine_row_avx2); }
        if crate::caps::has_avx512() { hs.push(blur_h5_moments_row_avx512); vs.push(v5_combine_row_avx512); }
    }
    for n in [1,2,3,4,5,7,8,15,16,17,33,257] {
        let a: Vec<_> = (0..n).map(|i| (i % 13) as f32 / 13.0).collect();
        let b: Vec<_> = (0..n).map(|i| (i % 17) as f32 / 17.0).collect();
        let mut expected=vec![0.0;5*n];
        let mut rows=expected.chunks_mut(n);
        blur_h5_moments_row_base(&a,&b,std::array::from_fn(|_| rows.next().unwrap()));
        for kernel in &hs {
            let mut actual=vec![f32::NAN;5*n];let mut rows=actual.chunks_mut(n);
            // SAFETY: only feature-checked kernels are in hs.
            unsafe { kernel(&a,&b,std::array::from_fn(|_| rows.next().unwrap())) };
            assert_eq!(actual.iter().map(|x|x.to_bits()).collect::<Vec<_>>(),expected.iter().map(|x|x.to_bits()).collect::<Vec<_>>());
        }
        for edge in [V5Edge::Top,V5Edge::Bottom,V5Edge::None] {
            let taps=[&a[..],&b,&a,&b,&a];let mut expected=vec![0.0;n];
            v5_combine_row_base(taps,edge,&mut expected);
            for kernel in &vs {
                let mut actual=vec![f32::NAN;n];
                // SAFETY: only feature-checked kernels are in vs.
                unsafe {kernel(taps,edge,&mut actual)};
                assert_eq!(actual.iter().map(|x|x.to_bits()).collect::<Vec<_>>(),expected.iter().map(|x|x.to_bits()).collect::<Vec<_>>());
            }
        }
    }
}
