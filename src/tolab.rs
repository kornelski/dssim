#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub extern crate unzip3;
use self::unzip3::Unzip3;

use image::RGBAPLU;
use image::RGBLU;
use image::ToRGB;
use imgref::*;

const D65x: f64 = 0.9505;
const D65y: f64 = 1.0;
const D65z: f64 = 1.089;

pub type GBitmap = ImgVec<f32>;

pub trait ToLAB {
    fn to_lab(&self) -> (f32, f32, f32);
}

impl ToLAB for RGBLU {
    fn to_lab(&self) -> (f32, f32, f32) {
        let fx = ((self.r as f64 * 0.4124 + self.g as f64 * 0.3576 + self.b as f64 * 0.1805) / D65x) as f32;
        let fy = ((self.r as f64 * 0.2126 + self.g as f64 * 0.7152 + self.b as f64 * 0.0722) / D65y) as f32;
        let fz = ((self.r as f64 * 0.0193 + self.g as f64 * 0.1192 + self.b as f64 * 0.9505) / D65z) as f32;

        let epsilon: f32 = 216. / 24389.;
        let k = ((24389. / 27.) / 116.) as f32; // http://www.brucelindbloom.com/LContinuity.html
        let X = if fx > epsilon {fx.powf(1./3.) - 16./116.} else {k * fx};
        let Y = if fy > epsilon {fy.powf(1./3.) - 16./116.} else {k * fy};
        let Z = if fz > epsilon {fz.powf(1./3.) - 16./116.} else {k * fz};

        return (
            Y * 1.16,
            (86.2/ 220.0 + 500.0/ 220.0 * (X - Y)), /* 86 is a fudge to make the value positive */
            (107.9/ 220.0 + 200.0/ 220.0 * (Y - Z)), /* 107 is a fudge to make the value positive */
        );
    }
}

pub trait ToLABBitmap {
    fn to_lab(&self) -> Vec<GBitmap>;
}

impl ToLABBitmap for ImgVec<RGBAPLU> {
    fn to_lab(&self) -> Vec<GBitmap> {
        self.as_ref().to_lab()
    }
}

impl ToLABBitmap for ImgVec<f32> {
    fn to_lab(&self) -> Vec<GBitmap> {
        vec![
            Img::new(
                self.buf.iter().cloned().map(|fy| {
                    let epsilon: f32 = 216. / 24389.;
                    // http://www.brucelindbloom.com/LContinuity.html
                    let Y = if fy > epsilon { fy.powf(1. / 3.) - 16. / 116. } else { ((24389. / 27.) / 116.) * fy };

                    return Y * 1.16;
                }).collect(),
                self.width(),
                self.height(),
            )
        ]
    }
}

impl<'a> ToLABBitmap for ImgRef<'a, RGBAPLU> {
    fn to_lab(&self) -> Vec<GBitmap> {
        let width = self.width();
        let height = self.height();
        let mut x=11; // offset so that block-based compressors don't align
        let mut y=11;

        let (l,a,b) = self.buf.iter().map(|px|{
            let n = x ^ y;
            if x >= width {
                x=0;
                y+=1;
            }
            x += 1;
            px.to_rgb(n).to_lab()
        }).unzip3();

        return vec![
            Img::new(l, width, height),
            Img::new(a, width, height),
            Img::new(b, width, height),
        ];
    }
}

impl<'a> ToLABBitmap for ImgRef<'a, RGBLU> {
    fn to_lab(&self) -> Vec<GBitmap> {
        let width = self.width();
        let height = self.height();
        let (l, a, b) = self.buf.iter().map(|px| px.to_lab()).unzip3();

        return vec![
            Img::new(l, width, height),
            Img::new(a, width, height),
            Img::new(b, width, height),
        ];
    }
}
