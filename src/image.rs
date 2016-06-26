#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub extern crate unzip3;
pub extern crate rgb;
use self::unzip3::Unzip3;
use std;
use rgb::*;

/// RGBA, but: premultiplied alpha, linear, f32 unit
pub type RGBAPLU = RGBA<f32>;
pub type RGBLU = RGB<f32>;

#[derive(Debug, Copy, Clone)]
pub struct LAB {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

impl std::ops::Mul<LAB> for LAB {
    type Output = LAB;
    fn mul(self, other: LAB) -> Self::Output {
        LAB {
            l: self.l * other.l,
            a: self.a * other.a,
            b: self.b * other.b,
        }
    }
}

impl std::ops::Mul<LAB> for f32 {
    type Output = LAB;
    fn mul(self, other: LAB) -> Self::Output {
        LAB {
            l: self * other.l,
            a: self * other.a,
            b: self * other.b,
        }
    }
}

impl std::ops::Mul<f32> for LAB {
    type Output = LAB;
    fn mul(self, other: f32) -> Self::Output {
        LAB {
            l: self.l * other,
            a: self.a * other,
            b: self.b * other,
        }
    }
}

impl std::ops::Add<LAB> for LAB {
    type Output = LAB;
    fn add(self, other: Self::Output) -> Self::Output {
        LAB {
            l: self.l + other.l,
            a: self.a + other.a,
            b: self.b + other.b,
        }
    }
}

impl std::ops::Add<f32> for LAB {
    type Output = LAB;
    fn add(self, other: f32) -> Self::Output {
        LAB {
            l: self.l + other,
            a: self.a + other,
            b: self.b + other,
        }
    }
}

impl std::ops::Sub<LAB> for LAB {
    type Output = f32;
    fn sub(self, other: LAB) -> Self::Output {
        let l = self.l - other.l;
        let a = self.a - other.a;
        let b = self.b - other.b;
        (l+a+b)/3.0
    }
}
impl LAB {
    pub fn avg(&self) -> f32 {
        (self.l + self.a + self.b) / 3.0
    }
}

impl From<LAB> for f64 {
    fn from(other: LAB) -> f64 {
        other.avg() as f64
    }
}

impl std::ops::Div<LAB> for LAB {
    type Output = LAB;
    fn div(self, other: Self::Output) -> Self::Output {
        LAB {
            l: self.l / other.l,
            a: self.a / other.a,
            b: self.b / other.b,
        }
    }
}

pub trait Sum4 {
    fn sum4(a: Self, b: Self, c: Self, d: Self) -> Self;
}

impl Sum4 for RGBAPLU {
    fn sum4(a: Self, b: Self, c: Self, d: Self) -> Self {
        RGBAPLU {
            r: (a.r + b.r + c.r + d.r) * 0.25,
            g: (a.g + b.g + c.g + d.g) * 0.25,
            b: (a.b + b.b + c.b + d.b) * 0.25,
            a: (a.a + b.a + c.a + d.a) * 0.25,
        }
    }
}

impl Sum4 for RGBLU {
    fn sum4(a: Self, b: Self, c: Self, d: Self) -> Self {
        RGBLU {
            r: (a.r + b.r + c.r + d.r) * 0.25,
            g: (a.g + b.g + c.g + d.g) * 0.25,
            b: (a.b + b.b + c.b + d.b) * 0.25,
        }
    }
}

const D65x: f64 = 0.9505;
const D65y: f64 = 1.0;
const D65z: f64 = 1.089;

pub struct Bitmap<T> {
    pub bitmap: Vec<T>,
    pub width: usize,
    pub height: usize,
}
pub type GBitmap = Bitmap<f32>;

pub enum Converted {
    Gray(GBitmap),
    LAB((GBitmap, GBitmap, GBitmap)),
}

pub trait ToLAB {
    fn to_luma(&self) -> f32;
    fn to_lab(&self) -> (f32, f32, f32);
}

pub trait ToRGB {
    fn to_rgb(self, n: usize) -> RGBLU;
}

impl ToRGB for RGBAPLU {
    fn to_rgb(self, n: usize) -> RGBLU {
        let mut r = self.r;
        let mut g = self.g;
        let mut b = self.b;
        let a = self.a;
        if a < 255.0 {
            if (n & 16) != 0 {
                r += 1.0 - a;
            }
            if (n & 8) != 0 {
                g += 1.0 - a; // assumes premultiplied alpha
            }
            if (n & 32) != 0 {
                b += 1.0 - a;
            }
        }

        RGBLU {
            r: r,
            g: g,
            b: b,
        }
    }
}

impl ToLAB for RGBLU {
    fn to_luma(&self) -> f32 {
        let fy = ((self.r as f64 * 0.2126 + self.g as f64 * 0.7152 + self.b as f64 * 0.0722) / D65y) as f32;

        let epsilon: f32 = 216. / 24389.;
        // http://www.brucelindbloom.com/LContinuity.html
        let Y = if fy > epsilon {fy.powf(1. / 3.) - 16./116.} else {((24389. / 27.) / 116.) * fy};

        return Y * 1.16;
    }

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
    fn to_luma(&self, width: usize, height: usize) -> GBitmap;
    fn to_lab(&self, width: usize, height: usize) -> (GBitmap, GBitmap, GBitmap);
}

impl ToLABBitmap for [RGBAPLU] {
    fn to_luma(&self, width: usize, height: usize) -> GBitmap {
        let mut x=11; // offset so that block-based compressors don't align
        let mut y=11;
        GBitmap{
            bitmap: self.iter().map(|px|{
                let n = x ^ y;
                if x >= width {
                    x=0;
                    y+=1;
                }
                x += 1;
                px.to_rgb(n).to_luma()
            }).collect(),
            width: width,
            height: height,
        }
    }

    fn to_lab(&self, width: usize, height: usize) -> (GBitmap, GBitmap, GBitmap) {
        let mut x=11; // offset so that block-based compressors don't align
        let mut y=11;

        let (l,a,b) = self.iter().map(|px|{
            let n = x ^ y;
            if x >= width {
                x=0;
                y+=1;
            }
            x += 1;
            px.to_rgb(n).to_lab()
        }).unzip3();

        return (
            GBitmap{bitmap:l, width:width, height:height},
            GBitmap{bitmap:a, width:width, height:height},
            GBitmap{bitmap:b, width:width, height:height},
        );
    }
}

impl ToLABBitmap for [RGBLU] {
    fn to_luma(&self, width: usize, height: usize) -> GBitmap {
        GBitmap{
            bitmap: self.iter().map(|px| px.to_luma()).collect(),
            width: width,
            height: height,
        }
    }

    fn to_lab(&self, width: usize, height: usize) -> (GBitmap, GBitmap, GBitmap) {
        let (l,a,b) = self.iter().map(|px| px.to_lab()).unzip3();

        return (
            GBitmap{bitmap:l, width:width, height:height},
            GBitmap{bitmap:a, width:width, height:height},
            GBitmap{bitmap:b, width:width, height:height},
        );
    }
}


//////////////////////////////

pub trait Downsample<T> {
    fn downsample(&self, width: usize, height: usize) -> Option<Bitmap<T>>;
}

impl<T> Downsample<T> for [T] where T: Sum4 + Copy {
    fn downsample(&self, width: usize, height: usize) -> Option<Bitmap<T>> {
        if width < 8 || height < 8 {
            return None;
        }

        assert_eq!(width * height, self.len());

        let half_height = height/2;
        let half_width = width/2;

        // crop odd pixels
        let bitmap = &self[0..width * half_height * 2];

        let scaled:Vec<_> = bitmap.chunks(width * 2).flat_map(|pair|{
            let (top, bot) = pair.split_at(half_width * 2);
            let bot = &bot[0..half_width * 2];

            return top.chunks(2).zip(bot.chunks(2)).map(|(a,b)| Sum4::sum4(a[0], a[1], b[0], b[1]))
        }).collect();

        assert_eq!(half_width * half_height, scaled.len());
        return Some(Bitmap{bitmap:scaled, width:half_width, height:half_height});
    }
}

pub fn worst(input: &[f32], width: usize, height: usize) -> Bitmap<f32> {
    let half_height = height/2;
    let half_width = width/2;

    if half_height < 4 || half_width < 4 {
        return Bitmap{bitmap:input.iter().cloned().collect(), width:width, height:height};
    }

    // crop odd pixels
    let bitmap = &input[0..width * half_height * 2];

    let scaled:Vec<_> = bitmap.chunks(width * 2).flat_map(|pair|{
        let (top, bot) = pair.split_at(half_width * 2);
        let bot = &bot[0..half_width * 2];

        return top.chunks(2).zip(bot.chunks(2)).map(|(a,b)| {
            a[0].min(a[1]).min(b[0].min(b[1]))
        });
    }).collect();

    assert_eq!(half_width * half_height, scaled.len());
    return Bitmap{bitmap:scaled, width:half_width, height:half_height};
}

pub fn avgworst(input: &[f32], width: usize, height: usize) -> Bitmap<f32> {
    let half_height = height/2;
    let half_width = width/2;

    if half_height < 4 || half_width < 4 {
        return Bitmap{bitmap:input.iter().cloned().collect(), width:width, height:height};
    }

    // crop odd pixels
    let bitmap = &input[0..width * half_height * 2];

    let scaled:Vec<_> = bitmap.chunks(width * 2).flat_map(|pair|{
        let (top, bot) = pair.split_at(half_width * 2);
        let bot = &bot[0..half_width * 2];

        return top.chunks(2).zip(bot.chunks(2)).map(|(a,b)| {
            (a[0].min(a[1]).min(b[0].min(b[1])) + ((a[0] + a[1] + b[0] + b[1]) * 0.25))*0.5
        });
    }).collect();

    assert_eq!(half_width * half_height, scaled.len());
    return Bitmap{bitmap:scaled, width:half_width, height:half_height};
}

pub fn avg(input: &[f32], width: usize, height: usize) -> Bitmap<f32> {
    let half_height = height/2;
    let half_width = width/2;

    if half_height < 4 || half_width < 4 {
        return Bitmap{bitmap:input.iter().cloned().collect(), width:width, height:height};
    }

    // crop odd pixels
    let bitmap = &input[0..width * half_height * 2];

    let scaled:Vec<_> = bitmap.chunks(width * 2).flat_map(|pair|{
        let (top, bot) = pair.split_at(half_width * 2);
        let bot = &bot[0..half_width * 2];

        return top.chunks(2).zip(bot.chunks(2)).map(|(a,b)| {
            (a[0] + a[1] + b[0] + b[1]) * 0.25
        });
    }).collect();

    assert_eq!(half_width * half_height, scaled.len());
    return Bitmap{bitmap:scaled, width:half_width, height:half_height};
}
