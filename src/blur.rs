
const KERNEL: [f32; 9] = [
    0.095332, 0.118095, 0.095332,
    0.118095, 0.146293, 0.118095,
    0.095332, 0.118095, 0.095332,
];

#[cfg(target_os = "macos")]
mod mac {
    use imgref::*;
    use std;
    use crate::ffi::vImage_Buffer;
    use crate::ffi::vImagePixelCount;
    use crate::ffi::vImageConvolve_PlanarF;
    use crate::ffi::vImage_Flags::kvImageEdgeExtend;
    use super::KERNEL;

    pub fn blur(src: ImgRef<'_, f32>, tmp: &mut [f32], mut dst: ImgRefMut<'_, f32>) {
        let srcbuf = vImage_Buffer {
            width: src.width() as vImagePixelCount,
            height: src.height() as vImagePixelCount,
            rowBytes: src.stride() * std::mem::size_of::<f32>(),
            data: src.buf().as_ptr(),
        };
        let mut dstbuf = vImage_Buffer {
            width: dst.width() as vImagePixelCount,
            height: dst.height() as vImagePixelCount,
            rowBytes: dst.stride() * std::mem::size_of::<f32>(),
            data: dst.buf_mut().as_mut_ptr(),
        };

        do_blur(&srcbuf, tmp, &mut dstbuf, src.width(), src.height());
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

    pub fn do_blur(srcbuf: &vImage_Buffer<*const f32>, tmp: &mut [f32], dstbuf: &mut vImage_Buffer<*mut f32>, width: usize, height: usize) {
        assert_eq!(tmp.len(), width * height);

        unsafe {
            let mut tmpwrbuf = vImage_Buffer {
                width: width as vImagePixelCount,
                height: height as vImagePixelCount,
                rowBytes: width * std::mem::size_of::<f32>(),
                data: tmp.as_mut_ptr(),
            };
            let res = vImageConvolve_PlanarF(srcbuf, &mut tmpwrbuf, std::ptr::null_mut(), 0, 0, KERNEL.as_ptr(), 3, 3, 0., kvImageEdgeExtend);
            assert_eq!(0, res);

            let tmprbuf = vImage_Buffer {
                width: width as vImagePixelCount,
                height: height as vImagePixelCount,
                rowBytes: width * std::mem::size_of::<f32>(),
                data: tmp.as_ptr(),
            };
            let res = vImageConvolve_PlanarF(&tmprbuf, dstbuf, std::ptr::null_mut(), 0, 0, KERNEL.as_ptr(), 3, 3, 0., kvImageEdgeExtend);
            assert_eq!(0, res);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod portable {
    use imgref::*;

    use std::cmp::min;
    use super::KERNEL;

    #[inline]
    fn do3f(prev: &[f32], curr: &[f32], next: &[f32], i: usize) -> f32 {
        debug_assert!(i > 0);

        let c0 = i-1;
        let c1 = i;
        let c2 = i+1;

        unsafe {
            prev.get_unchecked(c0)*KERNEL[0] + prev.get_unchecked(c1)*KERNEL[1] + prev.get_unchecked(c2)*KERNEL[2] +
            curr.get_unchecked(c0)*KERNEL[3] + curr.get_unchecked(c1)*KERNEL[4] + curr.get_unchecked(c2)*KERNEL[5] +
            next.get_unchecked(c0)*KERNEL[6] + next.get_unchecked(c1)*KERNEL[7] + next.get_unchecked(c2)*KERNEL[8]
        }
    }

    fn do3(prev: &[f32], curr: &[f32], next: &[f32], i: usize, width: usize) -> f32 {
        let c0 = if i > 0 {i-1} else {0};
        let c1 = i;
        let c2 = min(i+1, width-1);

        prev[c0]*KERNEL[0] + prev[c1]*KERNEL[1] + prev[c2]*KERNEL[2] +
        curr[c0]*KERNEL[3] + curr[c1]*KERNEL[4] + curr[c2]*KERNEL[5] +
        next[c0]*KERNEL[6] + next[c1]*KERNEL[7] + next[c2]*KERNEL[8]
    }

    pub fn blur(src: ImgRef<f32>, tmp: &mut [f32], dst: ImgRefMut<f32>) {
        {
            let tmp_dst = ImgRefMut::new(tmp, dst.width(), dst.height());
            do_blur(src, tmp_dst);
        }
        let tmp_src = ImgRef::new(tmp, src.width(), src.height());
        do_blur(tmp_src, dst);
    }

    pub fn do_blur(src: ImgRef<f32>, dst: ImgRefMut<f32>) {
        assert_eq!(src.width(), dst.width());
        assert_eq!(src.height(), dst.height());
        assert!(src.width() > 0);
        assert!(src.width() < 1<<24);
        assert!(src.height() > 0);
        assert!(src.height() < 1<<24);
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

            dstrow[0] = do3(prev, curr, next, 0, width);
            for i in 1..width-1 {
                dstrow[i] = do3f(prev, curr, next, i);
            }
            if width > 1 {
                dstrow[width-1] = do3(prev, curr, next, width-1, width);
            }
        }
    }

    pub fn blur_in_place(srcdst: ImgRefMut<f32>, tmp: &mut [f32]) {
        {
            let tmp_dst = ImgRefMut::new(tmp, srcdst.width(), srcdst.height());
            do_blur(srcdst.new_buf(&srcdst.buf()), tmp_dst);
        }
        let tmp_src = ImgRef::new(tmp, srcdst.width(), srcdst.height());
        do_blur(tmp_src, srcdst);
    }
}


#[cfg(target_os = "macos")]
pub use self::mac::*;

#[cfg(not(target_os = "macos"))]
pub use self::portable::*;

#[cfg(test)]
use imgref::*;

#[test]
fn blur_zero() {
    let src = vec![0.25];

    let mut tmp = vec![-55.; 1];
    let mut dst = vec![-99.; 1];

    let mut src2 = src.clone();

    blur(ImgRef::new(&src[..], 1,1), &mut tmp[..], ImgRefMut::new(&mut dst[..], 1, 1));
    blur_in_place(ImgRefMut::new(&mut src2[..], 1, 1), &mut tmp[..]);

    assert_eq!(src2, dst);
    assert!((0.25 - dst[0]).abs() < 0.00001);
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
    let mut tmp = vec![-55.; 5*5];
    let mut dst = vec![999.; 5*5];

    let mut src2 = src.clone();

    blur(src.as_ref(), &mut tmp[..], ImgRefMut::new(&mut dst[..], 5, 5));
    blur_in_place(src2.as_mut(), &mut tmp[..]);

    assert_eq!(src2.pixels().collect::<Vec<_>>(), dst);

    assert!((1./110. - dst[0]).abs() < 0.0001, "{:?}", dst);
    assert!((1./110. - dst[5*5-1]).abs() < 0.0001, "{:?}", dst);
    assert!((0.11354011 - dst[2*5+2]).abs() < 0.0001);
}

#[test]
fn blur_1x1() {
    let src = vec![1.];
    let mut src2 = src.clone();

    let mut tmp = vec![-999.; 1];
    let mut dst = vec![55.; 1];

    blur(ImgRef::new(&src[..], 1,1), &mut tmp[..], ImgRefMut::new(&mut dst[..], 1, 1));
    blur_in_place(ImgRefMut::new(&mut src2[..], 1,1), &mut tmp[..]);

    assert!((dst[0] - 1.).abs() < 0.00001);
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
    let mut tmp = vec![-55.; 4*4];
    let mut dst = vec![999.; 4*4];

    let mut src2 = src.clone();

    blur(ImgRef::new(&src[..], 4,4), &mut tmp[..], ImgRefMut::new(&mut dst[..], 4, 4));
    blur_in_place(ImgRefMut::new(&mut src2[..], 4,4), &mut tmp[..]);

    assert_eq!(src2, dst);

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

    assert!((1. - dst[3]).abs() < 0.0001, "{}", dst[3]);
    assert!((1. - dst[3 * 4]).abs() < 0.0001, "{}", dst[3 * 4]);
    assert!((1. - dst[4 * 4 - 1]).abs() < 0.0001, "{}", dst[4 * 4 - 1]);
    assert_eq!(exp, dst[0]);
}
