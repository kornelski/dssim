#![allow(dead_code)]

use std;
use rgb::*;
use imgref::*;

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
    type Output = LAB;
    fn sub(self, other: LAB) -> Self::Output {
        LAB {
            l: self.l - other.l,
            a: self.a - other.a,
            b: self.b - other.b,
        }
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

impl From<LAB> for f32 {
    fn from(other: LAB) -> f32 {
        other.avg()
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

impl Sum4 for f32 {
    fn sum4(a: Self, b: Self, c: Self, d: Self) -> Self {
        (a + b + c + d) * 0.25
    }
}

impl Sum4 for RGBAPLU {
    fn sum4(a: Self, b: Self, c: Self, d: Self) -> Self {
        RGBAPLU {
            r: Sum4::sum4(a.r, b.r, c.r, d.r),
            g: Sum4::sum4(a.g, b.g, c.g, d.g),
            b: Sum4::sum4(a.b, b.b, c.b, d.b),
            a: Sum4::sum4(a.a, b.a, c.a, d.a),
        }
    }
}

impl Sum4 for RGBLU {
    fn sum4(a: Self, b: Self, c: Self, d: Self) -> Self {
        RGBLU {
            r: Sum4::sum4(a.r, b.r, c.r, d.r),
            g: Sum4::sum4(a.g, b.g, c.g, d.g),
            b: Sum4::sum4(a.b, b.b, c.b, d.b),
        }
    }
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


//////////////////////////////

pub trait Downsample {
    type Output;
    fn downsample(&self) -> Option<Self::Output>;
}

impl<T> Downsample for ImgVec<T> where T: Sum4 + Copy {
    type Output = ImgVec<T>;
    fn downsample(&self) -> Option<Self::Output> {
        self.as_ref().downsample()
    }
}

impl<'a, T> Downsample for ImgRef<'a, T> where T: Sum4 + Copy {
    type Output = ImgVec<T>;
    fn downsample(&self) -> Option<Self::Output> {
        let width = self.width();
        let height = self.height();

        if width < 8 || height < 8 {
            return None;
        }

        assert_eq!(width * height, self.buf.len());

        let half_height = height / 2;
        let half_width = width / 2;

        // crop odd pixels
        let bitmap = &self.buf[0..width * half_height * 2];

        let scaled:Vec<_> = bitmap.chunks(width * 2).flat_map(|pair|{
            let (top, bot) = pair.split_at(half_width * 2);
            let bot = &bot[0..half_width * 2];

            return top.chunks(2).zip(bot.chunks(2)).map(|(a,b)| Sum4::sum4(a[0], a[1], b[0], b[1]))
        }).collect();

        assert_eq!(half_width * half_height, scaled.len());
        return Some(Img::new(scaled, half_width, half_height));
    }
}

#[allow(dead_code)]
pub fn worst(input: &[f32], width: usize, height: usize) -> ImgVec<f32> {
    let half_height = height / 2;
    let half_width = width / 2;

    if half_height < 4 || half_width < 4 {
        return Img::new(input.iter().cloned().collect(), width, height);
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
    Img::new(scaled, half_width, half_height)
}

#[allow(dead_code)]
pub fn avgworst(input: &[f32], width: usize, height: usize) -> ImgVec<f32> {
    let half_height = height / 2;
    let half_width = width / 2;

    if half_height < 4 || half_width < 4 {
        return Img::new(input.iter().cloned().collect(), width, height);
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
    Img::new(scaled, half_width, half_height)
}

#[allow(dead_code)]
pub fn avg(input: &[f32], width: usize, height: usize) -> ImgVec<f32> {
    let half_height = height / 2;
    let half_width = width / 2;

    if half_height < 4 || half_width < 4 {
        return Img::new(
            input.iter().cloned().collect(),
            width,
            height,
        );
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
    Img::new(scaled, half_width, half_height)
}
