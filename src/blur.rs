const KERNEL: [f32; 9] = [
    1./16., 2./16., 1./16.,
    2./16., 4./16., 2./16.,
    1./16., 2./16., 1./16.,
];

#[cfg(target_os = "macos")]
mod mac {
    use std;
    use ffi::vImage_Buffer;
    use ffi::vImageConvolve_PlanarF;
    use ffi::vImage_Flags::kvImageEdgeExtend;
    use super::KERNEL;

    pub fn blur(src: &[f32], tmp: &mut [f32], dst: &mut [f32], width: usize, height: usize) {
        assert_eq!(src.len(), width * height);
        assert_eq!(dst.len(), width * height);

        let srcbuf = vImage_Buffer {
            width: width as u64,
            height: height as u64,
            rowBytes: width * std::mem::size_of::<f32>(),
            data: src.as_ptr(),
        };
        let mut dstbuf = vImage_Buffer {
            width: width as u64,
            height: height as u64,
            rowBytes: width * std::mem::size_of::<f32>(),
            data: dst.as_mut_ptr(),
        };

        do_blur(&srcbuf, tmp, &mut dstbuf, width, height);
    }

    pub fn blur_in_place(srcdst: &mut [f32], tmp: &mut [f32], width: usize, height: usize) {
        assert_eq!(srcdst.len(), width * height);

        let srcbuf = vImage_Buffer {
            width: width as u64,
            height: height as u64,
            rowBytes: width * std::mem::size_of::<f32>(),
            data: srcdst.as_ptr(),
        };
        let mut dstbuf = vImage_Buffer {
            width: width as u64,
            height: height as u64,
            rowBytes: width * std::mem::size_of::<f32>(),
            data: srcdst.as_mut_ptr(),
        };

        do_blur(&srcbuf, tmp, &mut dstbuf, width, height);
    }

    pub fn do_blur(srcbuf: &vImage_Buffer<*const f32>, tmp: &mut [f32], dstbuf: &mut vImage_Buffer<*mut f32>, width: usize, height: usize) {
        assert_eq!(tmp.len(), width * height);

        unsafe {
            let mut tmpwrbuf = vImage_Buffer {
                width: width as u64,
                height: height as u64,
                rowBytes: width * std::mem::size_of::<f32>(),
                data: tmp.as_mut_ptr(),
            };
            let res = vImageConvolve_PlanarF(srcbuf, &mut tmpwrbuf, std::ptr::null_mut(), 0, 0, KERNEL.as_ptr(), 3, 3, 0., kvImageEdgeExtend);
            assert_eq!(0, res);

            let tmprbuf = vImage_Buffer {
                width: width as u64,
                height: height as u64,
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

    pub fn blur(src: &[f32], tmp: &mut [f32], dst: &mut [f32], width: usize, height: usize) {
        do_blur(src, tmp, width, height);
        do_blur(tmp, dst, width, height);
    }

    pub fn do_blur(src: &[f32], dst: &mut [f32], width: usize, height: usize) {
        assert!(width > 0);
        assert!(width < 1<<24);
        assert!(height > 0);
        assert!(height < 1<<24);
        assert!(src.len() >= width*height);
        assert!(dst.len() >= width*height);

        let mut prev = &src[0..width];
        let mut curr = prev;
        let mut next = prev;
        for y in 0..height {
            prev = curr;
            curr = next;
            next = if y+1 < height {&src[(y+1)*width..(y+2)*width]} else {curr};

            let mut dstrow = &mut dst[y*width..y*width+width];

            dstrow[0] = do3(prev, curr, next, 0, width);
            for i in 1..width-1 {
                dstrow[i] = do3f(prev, curr, next, i);
            }
            if width > 1 {
                dstrow[width-1] = do3(prev, curr, next, width-1, width);
            }
        }
    }

    pub fn blur_in_place(srcdst: &mut [f32], tmp: &mut [f32], width: usize, height: usize) {
        do_blur(srcdst, tmp, width, height);
        do_blur(tmp, srcdst, width, height);
    }
}


#[cfg(target_os = "macos")]
pub use self::mac::*;

#[cfg(not(target_os = "macos"))]
pub use self::portable::*;

#[test]
fn blur_one() {
    let src = vec![
    0.,0.,0.,0.,0.,
    0.,0.,0.,0.,0.,
    0.,0.,1.,0.,0.,
    0.,0.,0.,0.,0.,
    0.,0.,0.,0.,0.,
    ];
    let mut tmp = vec![-55.; 5*5];
    let mut dst = vec![999.; 5*5];

    let mut src2 = src.clone();

    blur(&src[..], &mut tmp[..], &mut dst[..], 5, 5);
    blur_in_place(&mut src2[..], &mut tmp[..], 5, 5);

    assert_eq!(src2, dst);

    assert_eq!(1./256., dst[0]);
    assert_eq!(1./256., dst[5*5-1]);
    let center = 1./16.*1./16. + 2./16.*2./16. + 1./16.*1./16. +
                 2./16.*2./16. + 4./16.*4./16. + 2./16.*2./16. +
                 1./16.*1./16. + 2./16.*2./16. + 1./16.*1./16.;
    assert_eq!(center, dst[2*5+2]);

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

    blur(&src[..], &mut tmp[..], &mut dst[..], 4, 4);
    blur_in_place(&mut src2[..], &mut tmp[..], 4, 4);

    assert_eq!(src2, dst);

    let z00 = 0.*1./16. + 0.*2./16. + 1.*1./16. +
              0.*2./16. + 0.*4./16. + 1.*2./16. +
              1.*1./16. + 1.*2./16. + 1.*1./16.;
    let z01 =                                   0.*1./16. + 1.*2./16. + 1.*1./16. +
                                                0.*2./16. + 1.*4./16. + 1.*2./16. +
                                                1.*1./16. + 1.*2./16. + 1.*1./16.;

    let z10 = 0.*1./16. + 0.*2./16. + 1.*1./16. +
              1.*2./16. + 1.*4./16. + 1.*2./16. +
              1.*1./16. + 1.*2./16. + 1.*1./16.;
    let z11 =                                   0.*1./16. + 1.*2./16. + 1.*1./16. +
                                                1.*2./16. + 1.*4./16. + 1.*2./16. +
                                                1.*1./16. + 1.*2./16. + 1.*1./16.;
    let exp = z00*1./16. + z00*2./16. + z01*1./16. +
              z00*2./16. + z00*4./16. + z01*2./16. +
              z10*1./16. + z10*2./16. + z11*1./16.;

    assert_eq!(1., dst[3]);
    assert_eq!(1., dst[3 * 4]);
    assert_eq!(1., dst[4 * 4 - 1]);
    assert_eq!(exp, dst[0]);
}
