#![allow(unused_mut)] // Feature-specific kernel lists also compile on non-x86.

use super::*;

pub(super) fn check_rgb<T: Copy + ToRGB>(src: &[T], context: &T::Context) {
    type Kernel<T> = unsafe fn(&[T], usize, &<T as ToRGB>::Context, &mut [MaybeUninit<f32>], &mut [MaybeUninit<f32>], &mut [MaybeUninit<f32>]);
    let mut kernels: Vec<Kernel<T>> = vec![rgb_to_lab_row_base::<T>, rgb_to_lab_row::<T>];
    #[cfg(target_arch = "x86_64")]
    {
        if crate::caps::has_avx2_fma() { kernels.push(rgb_to_lab_row_avx2::<T>); }
        if crate::caps::has_avx512() { kernels.push(rgb_to_lab_row_avx512::<T>); }
    }
    for y in [0, 7, 19] {
        let expected: Vec<_> = src.iter().enumerate().map(|(x, &px)| {
            let (l, a, b) = px.to_rgb((x+11) ^ (y+11), context).to_lab();
            [l.to_bits(), a.to_bits(), b.to_bits()]
        }).collect();
        for kernel in &kernels {
            let mut out: [Vec<_>; 3] = std::array::from_fn(|_| vec![MaybeUninit::new(f32::NAN); src.len()]);
            let [l, a, b] = &mut out;
            // SAFETY: feature clones were included only after their CPU check.
            unsafe { kernel(src, y, context, l, a, b) };
            for (i, expected) in expected.iter().enumerate() {
                for c in 0..3 {
                    // SAFETY: every slot was initialized even before the kernel.
                    assert_eq!(unsafe { out[c][i].assume_init() }.to_bits(), expected[c], "pixel {i}, channel {c}");
                }
            }
        }
    }
}

#[test]
fn rgb_rows_are_bit_exact_across_tiers() {
    for n in [0, 1, 7, 8, 15, 16, 17, 31, 33, 257] {
        let rgb: Vec<_> = (0..n).map(|i| RGBLU::new((i % 17) as f32 / 17.0, (i % 13) as f32 / 13.0, (i % 11) as f32 / 11.0)).collect();
        let rgba: Vec<_> = rgb.iter().enumerate().map(|(i, p)| {
            let a = (i % 7) as f32 / 6.0;
            RGBAPLU::new(p.r * a, p.g * a, p.b * a, a)
        }).collect();
        check_rgb(&rgb, &());
        check_rgb(&rgba, &());
    }
}

#[test]
fn gray_rows_are_bit_exact_across_tiers() {
    let mut kernels: Vec<unsafe fn(&[f32], &mut [f32])> = vec![gray_to_lab_row_base, gray_to_lab_row];
    #[cfg(target_arch = "x86_64")]
    {
        if crate::caps::has_avx2_fma() { kernels.push(gray_to_lab_row_avx2); }
        if crate::caps::has_avx512() { kernels.push(gray_to_lab_row_avx512); }
    }
    for n in [0, 1, 7, 8, 15, 16, 17, 257] {
        let src: Vec<_> = (0..n).map(|i| [0.0, EPSILON, f32::from_bits(EPSILON.to_bits()+1), 0.5, 1.0][i % 5]).collect();
        let mut expected = vec![0.0; n];
        gray_to_lab_row_base(&src, &mut expected);
        for kernel in &kernels {
            let mut out = vec![f32::NAN; n];
            // SAFETY: feature clones were included only after their CPU check.
            unsafe { kernel(&src, &mut out) };
            assert_eq!(out.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>());
        }
    }
}

#[test]
fn grayscale_lab_ignores_row_padding() {
    let packed = Img::new(vec![0.1_f32, 0.2, 0.3, 0.4], 2, 2);
    let padded = Img::new_stride(vec![0.1_f32, 0.2, 0.99, 0.3, 0.4], 2, 2, 3);
    assert_eq!(packed.to_lab()[0].buf(), padded.to_lab()[0].buf());
}
