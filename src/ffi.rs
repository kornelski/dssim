#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

extern crate libc;
use ::libc::{size_t, ssize_t, c_ulong};

pub type dssim_px_t = f32;

extern "C" {
    pub fn vImageConvolve_PlanarF(src: *const vImage_Buffer<*const f32>,
                                  dest: *mut vImage_Buffer<*mut f32>,
                                  tempBuffer: *mut f32,
                                  srcOffsetToROI_X: vImagePixelCount,
                                  srcOffsetToROI_Y: vImagePixelCount,
                                  kernel: *const f32,
                                  kernel_height: u32,
                                  kernel_width: u32,
                                  backgroundColor: Pixel_F,
                                  flags: vImage_Flags) -> vImage_Error;
}

pub type vImagePixelCount = c_ulong;
pub type vImage_Error = ssize_t;
pub type Pixel_F = f32;

#[repr(u32)]
pub enum vImage_Flags {
    kvImageNoFlags = 0,

     /* Operate on red, green and blue channels only. Alpha is copied from source
        to destination. For Interleaved formats only. */
    kvImageLeaveAlphaUnchanged = 1,

     /* Copy edge pixels. Convolution Only. */
    kvImageCopyInPlace = 2,

    /* Use the background color for missing pixels. */
    kvImageBackgroundColorFill  = 4,

    /* Use the nearest pixel for missing pixels. */
    kvImageEdgeExtend = 8,

    /* Pass to turn off internal tiling and disable internal multithreading. Use this if
       you want to do your own tiling, or to use the Min/Max filters in place. */
    kvImageDoNotTile =   16,

    /* Use a higher quality, slower resampling filter for Geometry operations
       (shear, scale, rotate, affine transform, etc.) */
    kvImageHighQualityResampling =   32,

     /* Use only the part of the kernel that overlaps the image. For integer kernels,
        real_divisor = divisor * (sum of used kernel elements) / (sum of kernel elements).
        This should preserve image brightness at the edges. Convolution only. */
    kvImageTruncateKernel  =   64,

    /* The function will return the number of bytes required for the temp buffer.
       If this value is negative, it is an error, per standard usage. */
    kvImageGetTempBufferSize =  128,

    /* Some functions such as vImageConverter_CreateWithCGImageFormat have so many possible error conditions
       that developers may need more help than a simple error code to diagnose problems. When this
       flag is set and an error is encountered, an informative error message will be logged to the Apple
       System Logger (ASL).  The output should be visible in Console.app. */
    kvImagePrintDiagnosticsToConsole =  256,

    /* Pass this flag to prevent vImage from allocating additional storage. */
    kvImageNoAllocate =  512,

    /* Use methods that are HDR-aware, capable of providing correct results for input images with pixel values
       outside the otherwise limited (typically [-2,2]) range. This may be slower. */
    kvImageHDRContent =  1024
}


#[repr(C)]
pub struct vImage_Buffer<T> {
    pub data: T,
    pub height: vImagePixelCount,
    pub width: vImagePixelCount,
    pub rowBytes: size_t,
}


#[test]
fn blur_ffione() {
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

    unsafe {
        blur(src[..].as_ptr(), tmp[..].as_mut_ptr(), dst[..].as_mut_ptr(), 5, 5);
        blur_in_place(src2[..].as_mut_ptr(), tmp[..].as_mut_ptr(), 5, 5);
    }


    assert_eq!(1./256., src2[0]);
    assert_eq!(1./256., dst[0]);
    assert_eq!(1./256., dst[5*5-1]);
    let center = 1./16.*1./16. + 2./16.*2./16. + 1./16.*1./16. +
                 2./16.*2./16. + 4./16.*4./16. + 2./16.*2./16. +
                 1./16.*1./16. + 2./16.*2./16. + 1./16.*1./16.;
    assert_eq!(center, dst[2*5+2]);
    assert_eq!(src2, dst);
}

#[test]
fn blur_ffitwo() {
    let src = vec![
    0.,1.,1.,1.,
    1.,1.,1.,1.,
    1.,1.,1.,1.,
    1.,1.,1.,1.,
    ];
    let mut tmp = vec![-55.; 4*4];
    let mut dst = vec![999.; 4*4];

    let mut src2 = src.clone();

    unsafe {
        blur_in_place(src2[..].as_mut_ptr(), tmp[..].as_mut_ptr(), 4, 4);
        blur(src[..].as_ptr(), tmp[..].as_mut_ptr(), dst[..].as_mut_ptr(), 4, 4);
    }


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
    assert_eq!(1., dst[3*4]);
    assert_eq!(1., dst[4*4-1]);
    assert_eq!(exp, src2[0]);
    assert_eq!(exp, dst[0]);
    assert_eq!(src2, dst);
}
