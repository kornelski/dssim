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

