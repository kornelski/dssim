
const KERNEL: [f32; 9] = [
    0.095332, 0.118095, 0.095332,
    0.118095, 0.146293, 0.118095,
    0.095332, 0.118095, 0.095332,
];

#[cfg(all(target_os = "macos", not(feature = "no-macos-vimage")))]
mod mac {
    use super::KERNEL;
    use crate::ffi::vImageConvolve_PlanarF;
    use crate::ffi::vImagePixelCount;
    use crate::ffi::vImage_Buffer;
    use crate::ffi::vImage_Flags::kvImageEdgeExtend;
    use imgref::*;
    use std::mem::MaybeUninit;

    pub fn blur(src: ImgRef<'_, f32>, tmp: &mut [MaybeUninit<f32>]) -> ImgVec<f32> {
        let width = src.width();
        let height = src.height();

        let srcbuf = vImage_Buffer {
            width: width as vImagePixelCount,
            height: height as vImagePixelCount,
            rowBytes: src.stride() * std::mem::size_of::<f32>(),
            data: src.buf().as_ptr(),
        };
        let mut dst_vec = Vec::with_capacity(width * height);
        let mut dstbuf = vImage_Buffer {
            width: width as vImagePixelCount,
            height: height as vImagePixelCount,
            rowBytes: width * std::mem::size_of::<f32>(),
            data: dst_vec.spare_capacity_mut().as_mut_ptr().cast(),
        };

        do_blur(&srcbuf, tmp, &mut dstbuf, width, height);
        unsafe {
            dst_vec.set_len(dst_vec.capacity());
        }
        ImgVec::new(dst_vec, width, height)
    }

    pub fn blur_in_place(mut srcdst: ImgRefMut<'_, f32>, tmp: &mut [MaybeUninit<f32>]) {
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

    fn do_blur(srcbuf: &vImage_Buffer<*const f32>, tmp: &mut [MaybeUninit<f32>], dstbuf: &mut vImage_Buffer<*mut f32>, width: usize, height: usize) {
        assert_eq!(tmp.len(), width * height);

        unsafe {
            let mut tmpwrbuf = vImage_Buffer {
                width: width as vImagePixelCount,
                height: height as vImagePixelCount,
                rowBytes: width * std::mem::size_of::<f32>(),
                data: tmp.as_mut_ptr().cast::<f32>(),
            };
            let res = vImageConvolve_PlanarF(srcbuf, &mut tmpwrbuf, std::ptr::null_mut(), 0, 0, KERNEL.as_ptr(), 3, 3, 0., kvImageEdgeExtend);
            assert_eq!(0, res);

            let tmprbuf = vImage_Buffer {
                width: width as vImagePixelCount,
                height: height as vImagePixelCount,
                rowBytes: width * std::mem::size_of::<f32>(),
                data: tmp.as_ptr().cast::<f32>(),
            };
            let res = vImageConvolve_PlanarF(&tmprbuf, dstbuf, std::ptr::null_mut(), 0, 0, KERNEL.as_ptr(), 3, 3, 0., kvImageEdgeExtend);
            assert_eq!(0, res);
        }
    }
}

#[cfg(not(all(target_os = "macos", not(feature = "no-macos-vimage"))))]
mod portable {
    use super::KERNEL;
    use imgref::*;
    use std::cmp::min;
    use std::mem::MaybeUninit;

    #[inline]
    unsafe fn do3f(prev: &[f32], curr: &[f32], next: &[f32], i: usize) -> f32 {
        debug_assert!(i > 0);

        let c0 = i - 1;
        let c1 = i;
        let c2 = i + 1;

        (prev.get_unchecked(c0)*KERNEL[0] + prev.get_unchecked(c1)*KERNEL[1] + prev.get_unchecked(c2)*KERNEL[2]) +
        (curr.get_unchecked(c0)*KERNEL[3] + curr.get_unchecked(c1)*KERNEL[4] + curr.get_unchecked(c2)*KERNEL[5]) +
        (next.get_unchecked(c0)*KERNEL[6] + next.get_unchecked(c1)*KERNEL[7] + next.get_unchecked(c2)*KERNEL[8])
    }

    fn do3(prev: &[f32], curr: &[f32], next: &[f32], i: usize, width: usize) -> f32 {
        let c0 = if i > 0 { i - 1 } else { 0 };
        let c1 = i;
        let c2 = min(i + 1, width - 1);

        prev[c2].mul_add(KERNEL[2], prev[c0].mul_add(KERNEL[0], prev[c1] * KERNEL[1])) +
        curr[c2].mul_add(KERNEL[5], curr[c0].mul_add(KERNEL[3], curr[c1] * KERNEL[4])) +
        next[c2].mul_add(KERNEL[8], next[c0].mul_add(KERNEL[6], next[c1] * KERNEL[7]))
    }

    pub fn blur(src: ImgRef<'_, f32>, tmp: &mut [MaybeUninit<f32>]) -> ImgVec<f32> {
        let width = src.width();
        let height = src.height();
        let tmp_dst = ImgRefMut::new(tmp, width, height);
        let tmp_src = do_blur(src, tmp_dst);
        let mut dst_vec = Vec::with_capacity(width * height);
        do_blur(tmp_src.as_ref(), ImgRefMut::new(dst_vec.spare_capacity_mut(), width, height));
        unsafe {
            dst_vec.set_len(dst_vec.capacity());
        }
        ImgVec::new(dst_vec, width, height)
    }

    fn do_blur<'d>(src: ImgRef<'_, f32>, mut dst: ImgRefMut<'d, MaybeUninit<f32>>) -> ImgRefMut<'d, f32> {
        assert_eq!(src.width(), dst.width());
        assert_eq!(src.height(), dst.height());
        assert!(src.width() > 0);
        assert!(src.width() < 1 << 24);
        assert!(src.height() > 0);
        assert!(src.height() < 1 << 24);
        debug_assert!(src.pixels().all(|p| p.is_finite()));

        let width = src.width();
        let height = src.height();
        let src_stride = src.stride();
        let dst_stride = dst.stride();
        let src = src.buf();
        let dst = dst.buf_mut();

        let mut prev = &src[0..width];
        let mut curr = prev;
        let mut next = prev;
        for y in 0..height {
            prev = curr;
            curr = next;
            let next_start = (y+1)*src_stride;
            next = if y+1 < height {&src[next_start..next_start+width]} else {curr};

            let dstrow = &mut dst[y*dst_stride..y*dst_stride+width];

            dstrow[0].write(do3(prev, curr, next, 0, width));
            for i in 1..width-1 {
                unsafe {
                    dstrow[i].write(do3f(prev, curr, next, i));
                }
            }
            if width > 1 {
                dstrow[width-1].write(do3(prev, curr, next, width-1, width));
            }
        }

        // assumes init after writing all the data
        unsafe {
            ImgRefMut::new_stride(std::slice::from_raw_parts_mut(dst.as_mut_ptr().cast(), dst.len()), width, height, dst_stride)
        }
    }

    pub fn blur_in_place(srcdst: ImgRefMut<'_, f32>, tmp: &mut [MaybeUninit<f32>]) {
        let tmp_dst = ImgRefMut::new(tmp, srcdst.width(), srcdst.height());
        let tmp_src = do_blur(srcdst.as_ref(), tmp_dst);
        do_blur(tmp_src.as_ref(), as_maybe_uninit(srcdst));
    }

    fn as_maybe_uninit(img: ImgRefMut<'_, f32>) -> ImgRefMut<'_, MaybeUninit<f32>> {
        img.map_buf(|dst| unsafe {
            std::slice::from_raw_parts_mut(dst.as_mut_ptr().cast::<MaybeUninit<f32>>(), dst.len())
        })
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

    let mut tmp = vec![-55.; 1]; tmp.clear();
    let dst = blur(ImgRef::new(&src[..], 1,1), tmp.spare_capacity_mut());
    blur_in_place(ImgRefMut::new(&mut src2[..], 1, 1), tmp.spare_capacity_mut());

    assert_eq!(&src2, dst.buf());
    assert!((0.25 - dst.buf()[0]).abs() < 0.00001);
}

#[test]
fn blur_one() {
    blur_one_compare(Img::new(vec![
        0.,0.,0.,0.,0.,
        0.,0.,0.,0.,0.,
        0.,0.,1.,0.,0.,
        0.,0.,0.,0.,0.,
        0.,0.,0.,0.,0.,
    ], 5, 5));
}

#[test]
fn blur_one_stride() {
    let nan = 1./0.;
    blur_one_compare(Img::new_stride(vec![
        0.,0.,0.,0.,0., nan, -11.,
        0.,0.,0.,0.,0., 333., nan,
        0.,0.,1.,0.,0., nan, -11.,
        0.,0.,0.,0.,0., 333., nan,
        0.,0.,0.,0.,0., nan,
    ], 5, 5, 7));
}

#[cfg(test)]
fn blur_one_compare(src: ImgVec<f32>) {
    let mut src2 = src.clone();

    let mut tmp = vec![-55.; 5*5]; tmp.clear();
    let dst = blur(src.as_ref(), tmp.spare_capacity_mut());
    blur_in_place(src2.as_mut(), tmp.spare_capacity_mut());

    assert_eq!(&src2.pixels().collect::<Vec<_>>(), dst.buf());

    assert!((1./110. - dst.buf()[0]).abs() < 0.0001, "{dst:?}");
    assert!((1./110. - dst.buf()[5*5-1]).abs() < 0.0001, "{dst:?}");
    assert!((0.11354011 - dst.buf()[2*5+2]).abs() < 0.0001);
}

#[test]
fn blur_1x1() {
    let src = vec![1.];
    let mut src2 = src.clone();

    let mut tmp = vec![-999.; 1]; tmp.clear();
    let dst = blur(ImgRef::new(&src[..], 1,1), tmp.spare_capacity_mut());
    blur_in_place(ImgRefMut::new(&mut src2[..], 1,1), tmp.spare_capacity_mut());

    assert!((dst.buf()[0] - 1.).abs() < 0.00001);
    assert!((src2[0] - 1.).abs() < 0.00001);
}

#[test]
fn blur_two() {
    let src = vec![
    0.,1.,1.,1.,
    1.,1.,1.,1.,
    1.,1.,1.,1.,
    1.,1.,1.,1.,
    ];
    let mut src2 = src.clone();

    let mut tmp = vec![-55.; 4*4]; tmp.clear();
    let dst = blur(ImgRef::new(&src[..], 4,4), tmp.spare_capacity_mut());
    blur_in_place(ImgRefMut::new(&mut src2[..], 4,4), tmp.spare_capacity_mut());

    assert_eq!(&src2, dst.buf());

    let z00 = 0.*KERNEL[0] + 0.*KERNEL[1] + 1.*KERNEL[2] +
              0.*KERNEL[3] + 0.*KERNEL[4] + 1.*KERNEL[5] +
              1.*KERNEL[6] + 1.*KERNEL[7] + 1.*KERNEL[8];
    let z01 =                                   0.*KERNEL[0] + 1.*KERNEL[1] + 1.*KERNEL[2] +
                                                0.*KERNEL[3] + 1.*KERNEL[4] + 1.*KERNEL[5] +
                                                1.*KERNEL[6] + 1.*KERNEL[7] + 1.*KERNEL[8];

    let z10 = 0.*KERNEL[0] + 0.*KERNEL[1] + 1.*KERNEL[2] +
              1.*KERNEL[3] + 1.*KERNEL[4] + 1.*KERNEL[5] +
              1.*KERNEL[6] + 1.*KERNEL[7] + 1.*KERNEL[8];
    let z11 =                                   0.*KERNEL[0] + 1.*KERNEL[1] + 1.*KERNEL[2] +
                                                1.*KERNEL[3] + 1.*KERNEL[4] + 1.*KERNEL[5] +
                                                1.*KERNEL[6] + 1.*KERNEL[7] + 1.*KERNEL[8];
    let exp = z00*KERNEL[0] + z00*KERNEL[1] + z01*KERNEL[2] +
              z00*KERNEL[3] + z00*KERNEL[4] + z01*KERNEL[5] +
              z10*KERNEL[6] + z10*KERNEL[7] + z11*KERNEL[8];

    assert!((1. - dst.buf()[3]).abs() < 0.0001, "{}", dst.buf()[3]);
    assert!((1. - dst.buf()[3 * 4]).abs() < 0.0001, "{}", dst.buf()[3 * 4]);
    assert!((1. - dst.buf()[4 * 4 - 1]).abs() < 0.0001, "{}", dst.buf()[4 * 4 - 1]);
    assert!((f64::from(exp) - f64::from(dst.buf()[0])).abs() < 0.0000001);
}
