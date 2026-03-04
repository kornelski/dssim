#[cfg(all(target_os = "macos", not(feature = "no-macos-vimage")))]
const KERNEL: [f32; 9] = [
    0.095332, 0.118095, 0.095332, 0.118095, 0.146293, 0.118095, 0.095332, 0.118095, 0.095332,
];

/// Allocate an f32 buffer without zeroing memory.
///
/// All blur functions fully write every element of their `tmp` and `dst` buffers
/// before reading, so uninitialized contents are never observed.
#[inline]
#[allow(clippy::uninit_vec)]
pub(crate) fn uninit_f32_vec(len: usize) -> Vec<f32> {
    let mut v = Vec::with_capacity(len);
    // SAFETY: all blur functions (blur_h*, blur_v*) write every element of
    // their output buffer before any element is read. The caller must uphold
    // this contract.
    unsafe {
        v.set_len(len);
    }
    v
}

#[cfg(all(target_os = "macos", not(feature = "no-macos-vimage")))]
mod mac {
    use super::KERNEL;
    use crate::ffi::vImage_Buffer;
    use crate::ffi::vImage_Flags::kvImageEdgeExtend;
    use crate::ffi::vImageConvolve_PlanarF;
    use crate::ffi::vImagePixelCount;
    use imgref::*;

    pub fn blur(src: ImgRef<'_, f32>, tmp: &mut [f32]) -> ImgVec<f32> {
        let width = src.width();
        let height = src.height();

        let srcbuf = vImage_Buffer {
            width: width as vImagePixelCount,
            height: height as vImagePixelCount,
            rowBytes: src.stride() * std::mem::size_of::<f32>(),
            data: src.buf().as_ptr(),
        };
        let mut dst_vec = vec![0f32; width * height];
        let mut dstbuf = vImage_Buffer {
            width: width as vImagePixelCount,
            height: height as vImagePixelCount,
            rowBytes: width * std::mem::size_of::<f32>(),
            data: dst_vec.as_mut_ptr(),
        };

        do_blur(&srcbuf, tmp, &mut dstbuf, width, height);
        ImgVec::new(dst_vec, width, height)
    }

    pub fn blur_in_place(mut srcdst: ImgRefMut<'_, f32>, tmp: &mut [f32]) {
        let srcbuf = vImage_Buffer {
            width: srcdst.width() as vImagePixelCount,
            height: srcdst.height() as vImagePixelCount,
            rowBytes: srcdst.stride() * std::mem::size_of::<f32>(),
            data: srcdst.buf().as_ptr(),
        };
        let mut dstbuf = vImage_Buffer {
            width: srcdst.width() as vImagePixelCount,
            height: srcdst.height() as vImagePixelCount,
            rowBytes: srcdst.stride() * std::mem::size_of::<f32>(),
            data: srcdst.buf_mut().as_mut_ptr(),
        };

        do_blur(&srcbuf, tmp, &mut dstbuf, srcdst.width(), srcdst.height());
    }

    /// Blur the element-wise product of two images. On macOS, falls back to
    /// multiply then blur since vImage has no fused variant.
    pub fn blur_mul(src1: ImgRef<'_, f32>, src2: ImgRef<'_, f32>, tmp: &mut [f32]) -> Vec<f32> {
        let width = src1.width();
        let height = src1.height();
        let mut product: Vec<f32> = src1
            .pixels()
            .zip(src2.pixels())
            .map(|(a, b)| a * b)
            .collect();
        blur_in_place(ImgRefMut::new(&mut product, width, height), tmp);
        product
    }

    fn do_blur(
        srcbuf: &vImage_Buffer<*const f32>,
        tmp: &mut [f32],
        dstbuf: &mut vImage_Buffer<*mut f32>,
        width: usize,
        height: usize,
    ) {
        assert_eq!(tmp.len(), width * height);

        unsafe {
            let mut tmpwrbuf = vImage_Buffer {
                width: width as vImagePixelCount,
                height: height as vImagePixelCount,
                rowBytes: width * std::mem::size_of::<f32>(),
                data: tmp.as_mut_ptr(),
            };
            let res = vImageConvolve_PlanarF(
                srcbuf,
                &mut tmpwrbuf,
                std::ptr::null_mut(),
                0,
                0,
                KERNEL.as_ptr(),
                3,
                3,
                0.,
                kvImageEdgeExtend,
            );
            assert_eq!(0, res);

            let tmprbuf = vImage_Buffer {
                width: width as vImagePixelCount,
                height: height as vImagePixelCount,
                rowBytes: width * std::mem::size_of::<f32>(),
                data: tmp.as_ptr(),
            };
            let res = vImageConvolve_PlanarF(
                &tmprbuf,
                dstbuf,
                std::ptr::null_mut(),
                0,
                0,
                KERNEL.as_ptr(),
                3,
                3,
                0.,
                kvImageEdgeExtend,
            );
            assert_eq!(0, res);
        }
    }
}

#[cfg(not(all(target_os = "macos", not(feature = "no-macos-vimage"))))]
mod portable {
    use super::uninit_f32_vec;
    use imgref::*;

    // 1D kernel from separable decomposition of the 3×3 kernel.
    // Symmetric: K1D = [K_SIDE, K_CENTER, K_SIDE].
    const K_SIDE: f32 = 0.308_758_86;
    const K_CENTER: f32 = 0.382_482_8;

    // Fused double-blur 5-tap kernel: convolving K1D with itself.
    // K5 = [K5_OUTER, K5_INNER, K5_MID, K5_INNER, K5_OUTER]
    // This makes H→V→H→V equivalent to a single H5→V5 pass, halving memory traffic.
    const K5_OUTER: f32 = K_SIDE * K_SIDE;
    const K5_INNER: f32 = 2.0 * K_SIDE * K_CENTER;
    const K5_MID: f32 = 2.0 * K_SIDE * K_SIDE + K_CENTER * K_CENTER;

    /// Horizontal 5-tap blur. Equivalent to two sequential 3-tap horizontal blurs.
    #[inline(never)]
    fn blur_h5(src: &[f32], dst: &mut [f32], width: usize, height: usize, src_stride: usize) {
        let last = width - 1;
        for y in 0..height {
            let row = &src[y * src_stride..][..width];
            let out = &mut dst[y * width..][..width];

            // Left edge pixels (0, 1): clamp negative indices to 0
            for i in 0..2.min(width) {
                let m2 = row[0]; // i-2 and i-1 clamp to 0
                let m1 = if i >= 1 { row[i - 1] } else { row[0] };
                let p1 = if i < last { row[i + 1] } else { row[last] };
                let p2 = if i + 2 <= last { row[i + 2] } else { row[last] };
                out[i] = (m2 + p2) * K5_OUTER + (m1 + p1) * K5_INNER + row[i] * K5_MID;
            }

            // Inner pixels: 2 <= i <= width-3
            for i in 2..width.saturating_sub(2) {
                out[i] = (row[i - 2] + row[i + 2]) * K5_OUTER
                    + (row[i - 1] + row[i + 1]) * K5_INNER
                    + row[i] * K5_MID;
            }

            // Right edge pixels: clamp beyond-end indices to last
            for i in width.saturating_sub(2).max(2)..width {
                let p1 = if i < last { row[i + 1] } else { row[last] };
                let p2 = if i + 2 <= last { row[i + 2] } else { row[last] };
                out[i] =
                    (row[i - 2] + p2) * K5_OUTER + (row[i - 1] + p1) * K5_INNER + row[i] * K5_MID;
            }
        }
    }

    /// Vertical 5-tap blur. Equivalent to two sequential 3-tap vertical blurs.
    #[inline(never)]
    fn blur_v5(src: &[f32], dst: &mut [f32], width: usize, height: usize, dst_stride: usize) {
        let last_y = height - 1;

        for y in 0..height {
            let ym2 = y.saturating_sub(2);
            let ym1 = y.saturating_sub(1);
            let yp1 = (y + 1).min(last_y);
            let yp2 = (y + 2).min(last_y);

            let rm2 = &src[ym2 * width..][..width];
            let rm1 = &src[ym1 * width..][..width];
            let rc = &src[y * width..][..width];
            let rp1 = &src[yp1 * width..][..width];
            let rp2 = &src[yp2 * width..][..width];

            let out = &mut dst[y * dst_stride..][..width];

            for x in 0..width {
                out[x] =
                    (rm2[x] + rp2[x]) * K5_OUTER + (rm1[x] + rp1[x]) * K5_INNER + rc[x] * K5_MID;
            }
        }
    }

    /// Horizontal 5-tap blur with fused element-wise multiply.
    /// Computes blur(src1 * src2) in a single H5 pass.
    #[allow(clippy::too_many_arguments)]
    #[inline(never)]
    fn blur_h5_mul(
        src1: &[f32],
        src2: &[f32],
        dst: &mut [f32],
        width: usize,
        height: usize,
        stride1: usize,
        stride2: usize,
    ) {
        let last = width - 1;
        for y in 0..height {
            let r1 = &src1[y * stride1..][..width];
            let r2 = &src2[y * stride2..][..width];
            let out = &mut dst[y * width..][..width];

            // General clamped access for edge pixels
            let clamp = |i: isize| i.max(0).min(last as isize) as usize;
            let prod = |i: isize| r1[clamp(i)] * r2[clamp(i)];

            // Left edge pixels
            for i in 0..2.min(width) {
                let ii = i as isize;
                out[i] = (prod(ii - 2) + prod(ii + 2)) * K5_OUTER
                    + (prod(ii - 1) + prod(ii + 1)) * K5_INNER
                    + (r1[i] * r2[i]) * K5_MID;
            }

            // Inner pixels
            for i in 2..width.saturating_sub(2) {
                let pm2 = r1[i - 2] * r2[i - 2];
                let pm1 = r1[i - 1] * r2[i - 1];
                let pc = r1[i] * r2[i];
                let pp1 = r1[i + 1] * r2[i + 1];
                let pp2 = r1[i + 2] * r2[i + 2];
                out[i] = (pm2 + pp2) * K5_OUTER + (pm1 + pp1) * K5_INNER + pc * K5_MID;
            }

            // Right edge pixels
            for i in width.saturating_sub(2).max(2)..width {
                let ii = i as isize;
                out[i] = (prod(ii - 2) + prod(ii + 2)) * K5_OUTER
                    + (prod(ii - 1) + prod(ii + 1)) * K5_INNER
                    + (r1[i] * r2[i]) * K5_MID;
            }
        }
    }

    // ── AVX2+FMA SIMD path ──────────────────────────────────────────────
    #[cfg(all(feature = "fma", target_arch = "x86_64"))]
    mod simd {
        use super::{K5_INNER, K5_MID, K5_OUTER};
        use archmage::prelude::*;
        use magetypes::simd::f32x8;

        #[arcane]
        pub fn blur_avx2(
            t: X64V3Token,
            buf: &[f32],
            tmp: &mut [f32],
            dst: &mut [f32],
            w: usize,
            h: usize,
            stride: usize,
        ) {
            blur_h5_simd(t, buf, tmp, w, h, stride);
            blur_v5_simd(t, tmp, dst, w, h, w);
        }

        #[arcane]
        pub fn blur_in_place_avx2(
            t: X64V3Token,
            buf: &mut [f32],
            tmp: &mut [f32],
            w: usize,
            h: usize,
            stride: usize,
        ) {
            blur_h5_simd(t, buf, tmp, w, h, stride);
            blur_v5_simd(t, tmp, buf, w, h, stride);
        }

        #[allow(clippy::too_many_arguments)]
        #[arcane]
        pub fn blur_mul_avx2(
            t: X64V3Token,
            src1: &[f32],
            src2: &[f32],
            tmp: &mut [f32],
            dst: &mut [f32],
            w: usize,
            h: usize,
            stride1: usize,
            stride2: usize,
        ) {
            blur_h5_mul_simd(t, src1, src2, tmp, w, h, stride1, stride2);
            blur_v5_simd(t, tmp, dst, w, h, w);
        }

        #[rite]
        fn blur_h5_simd(
            t: X64V3Token,
            src: &[f32],
            dst: &mut [f32],
            width: usize,
            height: usize,
            src_stride: usize,
        ) {
            let vk_outer = f32x8::splat(t, K5_OUTER);
            let vk_inner = f32x8::splat(t, K5_INNER);
            let vk_mid = f32x8::splat(t, K5_MID);
            let last = width - 1;

            for y in 0..height {
                let row = &src[y * src_stride..][..width];
                let out = &mut dst[y * width..][..width];

                // Left edge: scalar (2 pixels)
                for i in 0..2.min(width) {
                    let ii = i as isize;
                    let get = |j: isize| row[j.max(0).min(last as isize) as usize];
                    out[i] = (get(ii - 2) + get(ii + 2)) * K5_OUTER
                        + (get(ii - 1) + get(ii + 1)) * K5_INNER
                        + row[i] * K5_MID;
                }

                // SIMD loop: needs row[i-2..i+10] valid
                let mut i = 2;
                while i + 10 <= width {
                    let far_left = f32x8::load(t, (&row[i - 2..i + 6]).try_into().unwrap());
                    let left = f32x8::load(t, (&row[i - 1..i + 7]).try_into().unwrap());
                    let center = f32x8::load(t, (&row[i..i + 8]).try_into().unwrap());
                    let right = f32x8::load(t, (&row[i + 1..i + 9]).try_into().unwrap());
                    let far_right = f32x8::load(t, (&row[i + 2..i + 10]).try_into().unwrap());
                    let outer_sum = far_left + far_right;
                    let inner_sum = left + right;
                    let result =
                        outer_sum.mul_add(vk_outer, inner_sum.mul_add(vk_inner, center * vk_mid));
                    result.store((&mut out[i..i + 8]).try_into().unwrap());
                    i += 8;
                }

                // Scalar tail + right edges
                while i < width {
                    let ii = i as isize;
                    let get = |j: isize| row[j.max(0).min(last as isize) as usize];
                    out[i] = (get(ii - 2) + get(ii + 2)) * K5_OUTER
                        + (get(ii - 1) + get(ii + 1)) * K5_INNER
                        + row[i] * K5_MID;
                    i += 1;
                }
            }
        }

        #[rite]
        fn blur_v5_simd(
            t: X64V3Token,
            src: &[f32],
            dst: &mut [f32],
            width: usize,
            height: usize,
            dst_stride: usize,
        ) {
            let vk_outer = f32x8::splat(t, K5_OUTER);
            let vk_inner = f32x8::splat(t, K5_INNER);
            let vk_mid = f32x8::splat(t, K5_MID);
            let last_y = height - 1;

            for y in 0..height {
                let ym2 = y.saturating_sub(2);
                let ym1 = y.saturating_sub(1);
                let yp1 = (y + 1).min(last_y);
                let yp2 = (y + 2).min(last_y);

                let rm2 = &src[ym2 * width..][..width];
                let rm1 = &src[ym1 * width..][..width];
                let rc = &src[y * width..][..width];
                let rp1 = &src[yp1 * width..][..width];
                let rp2 = &src[yp2 * width..][..width];

                let out = &mut dst[y * dst_stride..][..width];

                let mut x = 0;
                while x + 8 <= width {
                    let vm2 = f32x8::load(t, (&rm2[x..x + 8]).try_into().unwrap());
                    let vm1 = f32x8::load(t, (&rm1[x..x + 8]).try_into().unwrap());
                    let vc = f32x8::load(t, (&rc[x..x + 8]).try_into().unwrap());
                    let vp1 = f32x8::load(t, (&rp1[x..x + 8]).try_into().unwrap());
                    let vp2 = f32x8::load(t, (&rp2[x..x + 8]).try_into().unwrap());
                    let outer = vm2 + vp2;
                    let inner = vm1 + vp1;
                    let result = outer.mul_add(vk_outer, inner.mul_add(vk_inner, vc * vk_mid));
                    result.store((&mut out[x..x + 8]).try_into().unwrap());
                    x += 8;
                }

                // Scalar tail
                while x < width {
                    out[x] = (rm2[x] + rp2[x]) * K5_OUTER
                        + (rm1[x] + rp1[x]) * K5_INNER
                        + rc[x] * K5_MID;
                    x += 1;
                }
            }
        }

        #[allow(clippy::too_many_arguments)]
        #[rite]
        fn blur_h5_mul_simd(
            t: X64V3Token,
            src1: &[f32],
            src2: &[f32],
            dst: &mut [f32],
            width: usize,
            height: usize,
            stride1: usize,
            stride2: usize,
        ) {
            let vk_outer = f32x8::splat(t, K5_OUTER);
            let vk_inner = f32x8::splat(t, K5_INNER);
            let vk_mid = f32x8::splat(t, K5_MID);
            let last = width - 1;

            for y in 0..height {
                let r1 = &src1[y * stride1..][..width];
                let r2 = &src2[y * stride2..][..width];
                let out = &mut dst[y * width..][..width];

                // Left edge: scalar (2 pixels)
                for i in 0..2.min(width) {
                    let ii = i as isize;
                    let cl = |j: isize| j.max(0).min(last as isize) as usize;
                    let p = |j: isize| r1[cl(j)] * r2[cl(j)];
                    out[i] = (p(ii - 2) + p(ii + 2)) * K5_OUTER
                        + (p(ii - 1) + p(ii + 1)) * K5_INNER
                        + (r1[i] * r2[i]) * K5_MID;
                }

                // SIMD loop: needs [i-2..i+10] valid from both sources
                let mut i = 2;
                while i + 10 <= width {
                    let l1m2 = f32x8::load(t, (&r1[i - 2..i + 6]).try_into().unwrap());
                    let l2m2 = f32x8::load(t, (&r2[i - 2..i + 6]).try_into().unwrap());
                    let l1m1 = f32x8::load(t, (&r1[i - 1..i + 7]).try_into().unwrap());
                    let l2m1 = f32x8::load(t, (&r2[i - 1..i + 7]).try_into().unwrap());
                    let l1c = f32x8::load(t, (&r1[i..i + 8]).try_into().unwrap());
                    let l2c = f32x8::load(t, (&r2[i..i + 8]).try_into().unwrap());
                    let l1p1 = f32x8::load(t, (&r1[i + 1..i + 9]).try_into().unwrap());
                    let l2p1 = f32x8::load(t, (&r2[i + 1..i + 9]).try_into().unwrap());
                    let l1p2 = f32x8::load(t, (&r1[i + 2..i + 10]).try_into().unwrap());
                    let l2p2 = f32x8::load(t, (&r2[i + 2..i + 10]).try_into().unwrap());

                    let p_m2 = l1m2 * l2m2;
                    let p_m1 = l1m1 * l2m1;
                    let p_c = l1c * l2c;
                    let p_p1 = l1p1 * l2p1;
                    let p_p2 = l1p2 * l2p2;

                    let outer = p_m2 + p_p2;
                    let inner = p_m1 + p_p1;
                    let result = outer.mul_add(vk_outer, inner.mul_add(vk_inner, p_c * vk_mid));
                    result.store((&mut out[i..i + 8]).try_into().unwrap());
                    i += 8;
                }

                // Scalar tail + right edges
                while i < width {
                    let ii = i as isize;
                    let cl = |j: isize| j.max(0).min(last as isize) as usize;
                    let p = |j: isize| r1[cl(j)] * r2[cl(j)];
                    out[i] = (p(ii - 2) + p(ii + 2)) * K5_OUTER
                        + (p(ii - 1) + p(ii + 1)) * K5_INNER
                        + (r1[i] * r2[i]) * K5_MID;
                    i += 1;
                }
            }
        }
    }

    pub fn blur(src: ImgRef<'_, f32>, tmp: &mut [f32]) -> ImgVec<f32> {
        let width = src.width();
        let height = src.height();
        assert!(width > 0 && width < 1 << 24);
        assert!(height > 0 && height < 1 << 24);
        debug_assert!(src.pixels().all(|p| p.is_finite()));

        let pixels = width * height;
        assert!(tmp.len() >= pixels);
        let tmp = &mut tmp[..pixels];
        let mut dst = uninit_f32_vec(pixels);

        #[cfg(all(feature = "fma", target_arch = "x86_64"))]
        {
            use archmage::SimdToken as _;
            if let Some(token) = archmage::X64V3Token::summon() {
                simd::blur_avx2(token, src.buf(), tmp, &mut dst, width, height, src.stride());
                return ImgVec::new(dst, width, height);
            }
        }

        blur_h5(src.buf(), tmp, width, height, src.stride());
        blur_v5(tmp, &mut dst, width, height, width);

        ImgVec::new(dst, width, height)
    }

    pub fn blur_in_place(mut srcdst: ImgRefMut<'_, f32>, tmp: &mut [f32]) {
        let width = srcdst.width();
        let height = srcdst.height();
        let stride = srcdst.stride();
        let pixels = width * height;

        assert!(tmp.len() >= pixels);
        let tmp = &mut tmp[..pixels];
        let buf = srcdst.buf_mut();

        #[cfg(all(feature = "fma", target_arch = "x86_64"))]
        {
            use archmage::SimdToken as _;
            if let Some(token) = archmage::X64V3Token::summon() {
                simd::blur_in_place_avx2(token, buf, tmp, width, height, stride);
                return;
            }
        }

        blur_h5(buf, tmp, width, height, stride);
        blur_v5(tmp, buf, width, height, stride);
    }

    /// Blur the element-wise product of two images: blur(src1 * src2).
    /// Fuses the multiply into the horizontal pass, then does a single vertical pass.
    pub fn blur_mul(src1: ImgRef<'_, f32>, src2: ImgRef<'_, f32>, tmp: &mut [f32]) -> Vec<f32> {
        let width = src1.width();
        let height = src1.height();
        debug_assert_eq!(width, src2.width());
        debug_assert_eq!(height, src2.height());
        assert!(width > 0 && width < 1 << 24);
        assert!(height > 0 && height < 1 << 24);

        let pixels = width * height;
        assert!(tmp.len() >= pixels);
        let tmp = &mut tmp[..pixels];
        let mut dst = uninit_f32_vec(pixels);

        #[cfg(all(feature = "fma", target_arch = "x86_64"))]
        {
            use archmage::SimdToken as _;
            if let Some(token) = archmage::X64V3Token::summon() {
                simd::blur_mul_avx2(
                    token,
                    src1.buf(),
                    src2.buf(),
                    tmp,
                    &mut dst,
                    width,
                    height,
                    src1.stride(),
                    src2.stride(),
                );
                return dst;
            }
        }

        blur_h5_mul(
            src1.buf(),
            src2.buf(),
            tmp,
            width,
            height,
            src1.stride(),
            src2.stride(),
        );
        blur_v5(tmp, &mut dst, width, height, width);

        dst
    }
}

#[cfg(all(target_os = "macos", not(feature = "no-macos-vimage")))]
pub use self::mac::*;

#[cfg(not(all(target_os = "macos", not(feature = "no-macos-vimage"))))]
pub use self::portable::*;

#[cfg(test)]
use imgref::*;

#[test]
fn blur_zero() {
    let src = vec![0.25];
    let mut src2 = src.clone();

    let mut tmp = vec![0.; 1];
    let dst = blur(ImgRef::new(&src[..], 1, 1), &mut tmp);
    blur_in_place(ImgRefMut::new(&mut src2[..], 1, 1), &mut tmp);

    assert_eq!(&src2, dst.buf());
    assert!((0.25 - dst.buf()[0]).abs() < 0.00001);
}

#[test]
fn blur_one() {
    blur_one_compare(Img::new(
        vec![
            0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 0., 0., 0., 0.,
            0., 0., 0.,
        ],
        5,
        5,
    ));
}

#[test]
fn blur_one_stride() {
    let nan = 1. / 0.;
    blur_one_compare(Img::new_stride(
        vec![
            0., 0., 0., 0., 0., nan, -11., 0., 0., 0., 0., 0., 333., nan, 0., 0., 1., 0., 0., nan,
            -11., 0., 0., 0., 0., 0., 333., nan, 0., 0., 0., 0., 0., nan,
        ],
        5,
        5,
        7,
    ));
}

#[cfg(test)]
fn blur_one_compare(src: ImgVec<f32>) {
    let mut src2 = src.clone();

    let mut tmp = vec![0.; 5 * 5];
    let dst = blur(src.as_ref(), &mut tmp);
    blur_in_place(src2.as_mut(), &mut tmp);

    assert_eq!(&src2.pixels().collect::<Vec<_>>(), dst.buf());

    assert!((1. / 110. - dst.buf()[0]).abs() < 0.0001, "{dst:?}");
    assert!((1. / 110. - dst.buf()[5 * 5 - 1]).abs() < 0.0001, "{dst:?}");
    assert!((0.11354011 - dst.buf()[2 * 5 + 2]).abs() < 0.0001);
}

#[test]
fn blur_1x1() {
    let src = vec![1.];
    let mut src2 = src.clone();

    let mut tmp = vec![0.; 1];
    let dst = blur(ImgRef::new(&src[..], 1, 1), &mut tmp);
    blur_in_place(ImgRefMut::new(&mut src2[..], 1, 1), &mut tmp);

    assert!((dst.buf()[0] - 1.).abs() < 0.00001);
    assert!((src2[0] - 1.).abs() < 0.00001);
}

#[test]
fn blur_two() {
    let src = vec![
        0., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1.,
    ];
    let mut src2 = src.clone();

    let mut tmp = vec![0.; 4 * 4];
    let dst = blur(ImgRef::new(&src[..], 4, 4), &mut tmp);
    blur_in_place(ImgRefMut::new(&mut src2[..], 4, 4), &mut tmp);

    assert_eq!(&src2, dst.buf());

    // All-1 corners should remain 1.0 (kernel is normalized)
    assert!((1. - dst.buf()[3]).abs() < 0.0001, "{}", dst.buf()[3]);
    assert!(
        (1. - dst.buf()[3 * 4]).abs() < 0.0001,
        "{}",
        dst.buf()[3 * 4]
    );
    assert!(
        (1. - dst.buf()[4 * 4 - 1]).abs() < 0.0001,
        "{}",
        dst.buf()[4 * 4 - 1]
    );

    // Reference 5-tap computation for corner [0][0]
    let k_side: f32 = 0.308_758_86;
    let k_center: f32 = 0.382_482_8;
    let k5o = k_side * k_side;
    let k5i = 2.0 * k_side * k_center;
    let k5m = 2.0 * k_side * k_side + k_center * k_center;
    let cl = |i: isize, max: usize| i.max(0).min(max as isize) as usize;

    // H5 pass on 4×4
    let mut h = [0.0f32; 16];
    for y in 0..4 {
        for x in 0..4usize {
            let xi = x as isize;
            h[y * 4 + x] = (src[y * 4 + cl(xi - 2, 3)] + src[y * 4 + cl(xi + 2, 3)]) * k5o
                + (src[y * 4 + cl(xi - 1, 3)] + src[y * 4 + cl(xi + 1, 3)]) * k5i
                + src[y * 4 + x] * k5m;
        }
    }
    // V5 pass
    let mut exp_all = [0.0f32; 16];
    for y in 0..4usize {
        for x in 0..4 {
            let yi = y as isize;
            exp_all[y * 4 + x] = (h[cl(yi - 2, 3) * 4 + x] + h[cl(yi + 2, 3) * 4 + x]) * k5o
                + (h[cl(yi - 1, 3) * 4 + x] + h[cl(yi + 1, 3) * 4 + x]) * k5i
                + h[y * 4 + x] * k5m;
        }
    }
    let exp = exp_all[0];
    assert!(
        (f64::from(exp) - f64::from(dst.buf()[0])).abs() < 0.0001,
        "expected {exp}, got {}",
        dst.buf()[0]
    );
}
