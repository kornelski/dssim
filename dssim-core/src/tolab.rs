#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use crate::image::RGBAPLU;
use crate::image::RGBLU;
use crate::image::ToRGB;
use imgref::*;
use rayon::prelude::*;

pub type GBitmap = ImgVec<f32>;
pub(crate) trait ToLAB {
    fn to_lab(&self) -> (f32, f32, f32);
}

impl ToLAB for RGBLU {
    #[inline]
    fn to_lab(&self) -> (f32, f32, f32) {
        let ok = oklab::linear_srgb_to_oklab(*self);
        (ok.l, 2. * (ok.a + 0.24), 2. * (ok.b + 0.32)) // Not sure why it needs 2x, but otherwise it scores poorly
    }
}

/// Convert image to L\*a\*b\* planar
///
/// It should return 1 (gray) or 3 (color) planes.
pub trait ToLABBitmap {
    fn to_lab(&self) -> Vec<GBitmap>;
}

impl ToLABBitmap for ImgVec<RGBAPLU> {
    #[inline(always)]
    fn to_lab(&self) -> Vec<GBitmap> {
        self.as_ref().to_lab()
    }
}

impl ToLABBitmap for ImgVec<RGBLU> {
    #[inline(always)]
    fn to_lab(&self) -> Vec<GBitmap> {
        self.as_ref().to_lab()
    }
}

impl ToLABBitmap for GBitmap {
    fn to_lab(&self) -> Vec<GBitmap> {
        let width = self.width();
        let height = self.height();
        let size = width * height;

        let mut out = Vec::with_capacity(size);
        unsafe { out.set_len(size) };

        // For output width == stride
        out.par_chunks_mut(width).enumerate().for_each(|(y, out_row)|{
            let start = y * self.stride();
            let in_row = &self.buf()[start..start + width];
            let out_row = &mut out_row[0..width];
            let epsilon: f32 = 216. / 24389.;
            for x in 0..width {
                let fy = in_row[x];
                // http://www.brucelindbloom.com/LContinuity.html
                let Y = if fy > epsilon { fy.powf(1. / 3.) - 16. / 116. } else { ((24389. / 27.) / 116.) * fy };
                out_row[x] = Y * 1.16;
            }
        });

        vec![Img::new(out, width, height)]
    }
}

fn rgb_to_lab<'a, T: Copy + Sync + Send + 'static, F>(img: ImgRef<'a, T>, cb: F) -> Vec<GBitmap>
    where F: Fn(T, usize) -> (f32, f32, f32) + Sync + Send + 'static
{
    let width = img.width();
    let height = img.height();
    let stride = img.stride();
    let size = width * height;

    let mut out_l = Vec::with_capacity(size);
    unsafe { out_l.set_len(size) };
    let mut out_a = Vec::with_capacity(size);
    unsafe { out_a.set_len(size) };
    let mut out_b = Vec::with_capacity(size);
    unsafe { out_b.set_len(size) };

    // For output width == stride
    out_l.par_chunks_mut(width).zip(
        out_a.par_chunks_mut(width).zip(out_b.par_chunks_mut(width))
    ).enumerate()
    .for_each(|(y, (l_row, (a_row, b_row)))| {
        let start = y * stride;
        let in_row = &img.buf()[start .. start + width];
        let l_row = &mut l_row[0..width];
        let a_row = &mut a_row[0..width];
        let b_row = &mut b_row[0..width];
        for x in 0..width {
            let n = (x+11) ^ (y+11);
            let (l,a,b) = cb(in_row[x], n);
            l_row[x] = l;
            a_row[x] = a;
            b_row[x] = b;
        }
    });

    return vec![
        Img::new(out_l, width, height),
        Img::new(out_a, width, height),
        Img::new(out_b, width, height),
    ];
}

impl<'a> ToLABBitmap for ImgRef<'a, RGBAPLU> {
    #[inline]
    fn to_lab(&self) -> Vec<GBitmap> {
        rgb_to_lab(*self, |px, n|{
            px.to_rgb(n).to_lab()
        })
    }
}

impl<'a> ToLABBitmap for ImgRef<'a, RGBLU> {
    #[inline]
    fn to_lab(&self) -> Vec<GBitmap> {
        rgb_to_lab(*self, |px, _n|{
            px.to_lab()
        })
    }
}
