//! Safety: new unsafe calls only enter CPU-feature-checked clones.
//! Downsample uses initialized output slices and checked indexing.

#![allow(dead_code)]

use crate::linear::{GammaComponent, GammaPixel};
use imgref::*;
use rgb::*;

/// RGBA, but: premultiplied alpha, linear (using sRGB primaries, but not its gamma curve), f32 unit scale 0..1
pub type RGBAPLU = RGBA<f32>;
/// RGB, but: linear (using sRGB primaries, but not its gamma curve), f32 unit scale 0..1
pub type RGBLU = RGB<f32>;

/// L\*a\*b\*b, but using float units (values are 100× smaller than in usual integer representation)
#[derive(Debug, Copy, Clone)]
pub struct LAB {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

impl std::ops::Mul<Self> for LAB {
    type Output = Self;

    fn mul(self, other: Self) -> Self::Output {
        Self {
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
    type Output = Self;

    fn mul(self, other: f32) -> Self::Output {
        Self {
            l: self.l * other,
            a: self.a * other,
            b: self.b * other,
        }
    }
}

impl std::ops::Add<Self> for LAB {
    type Output = Self;

    fn add(self, other: Self::Output) -> Self::Output {
        Self {
            l: self.l + other.l,
            a: self.a + other.a,
            b: self.b + other.b,
        }
    }
}

impl std::ops::Add<f32> for LAB {
    type Output = Self;

    fn add(self, other: f32) -> Self::Output {
        Self {
            l: self.l + other,
            a: self.a + other,
            b: self.b + other,
        }
    }
}

impl std::ops::Sub<Self> for LAB {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self {
            l: self.l - other.l,
            a: self.a - other.a,
            b: self.b - other.b,
        }
    }
}

impl LAB {
    pub(crate) fn avg(self) -> f32 {
        (self.l + self.a + self.b) * (1. / 3.)
    }
}

impl From<LAB> for f64 {
    fn from(other: LAB) -> Self {
        (Self::from(other.l) + Self::from(other.a) + Self::from(other.b)) * (1. / 3.)
    }
}

impl From<LAB> for f32 {
    fn from(other: LAB) -> Self {
        other.avg()
    }
}

impl std::ops::Div<Self> for LAB {
    type Output = Self;

    fn div(self, other: Self::Output) -> Self::Output {
        Self {
            l: self.l / other.l,
            a: self.a / other.a,
            b: self.b / other.b,
        }
    }
}

/// Component-wise averaging of pixel values used by `Downsample` to support arbitrary pixel types
///
/// Used to naively resample 4 high-res pixels into one low-res pixel
#[doc(hidden)]
pub trait Average4 {
    fn average4(a: Self, b: Self, c: Self, d: Self) -> Self;
}

impl Average4 for f32 {
    #[inline(always)]
    fn average4(a: Self, b: Self, c: Self, d: Self) -> Self {
        (a + b + c + d) * 0.25
    }
}

impl Average4 for RGBAPLU {
    #[inline(always)]
    fn average4(a: Self, b: Self, c: Self, d: Self) -> Self {
        RGBAPLU {
            r: Average4::average4(a.r, b.r, c.r, d.r),
            g: Average4::average4(a.g, b.g, c.g, d.g),
            b: Average4::average4(a.b, b.b, c.b, d.b),
            a: Average4::average4(a.a, b.a, c.a, d.a),
        }
    }
}

impl Average4 for RGBLU {
    #[inline(always)]
    fn average4(a: Self, b: Self, c: Self, d: Self) -> Self {
        RGBLU {
            r: Average4::average4(a.r, b.r, c.r, d.r),
            g: Average4::average4(a.g, b.g, c.g, d.g),
            b: Average4::average4(a.b, b.b, c.b, d.b),
        }
    }
}

pub(crate) trait ToRGB {
    /// A shared gamma lookup table, or `()` for already-linear pixels.
    type Context: Sync;
    fn to_rgb(self, n: usize, context: &Self::Context) -> RGBLU;
}

impl ToRGB for RGBLU {
    type Context = ();
    #[inline(always)]
    fn to_rgb(self, _n: usize, _: &()) -> RGBLU { self }
}

impl ToRGB for RGBAPLU {
    type Context = ();
    #[inline(always)]
    fn to_rgb(self, n: usize, _: &()) -> RGBLU {
        // Bit tests only read bits <32; u32 keeps vectorized compares in
        // 32-bit lanes instead of usize-wide ones.
        let n = n as u32;
        let mut r = self.r;
        let mut g = self.g;
        let mut b = self.b;
        let a = self.a;
        let dither = if a < 255.0 { 1.0 - a } else { 0.0 }; // assumes premultiplied alpha
        if (n & 16) != 0 {
            r += dither;
        }
        if (n & 8) != 0 {
            g += dither;
        }
        if (n & 32) != 0 {
            b += dither;
        }

        RGBLU { r, g, b }
    }
}

impl<P> ToRGB for P
where
    P: GammaPixel<Output = RGBAPLU>,
    <P::Component as GammaComponent>::Lut: Sync,
{
    type Context = <P::Component as GammaComponent>::Lut;

    #[inline(always)]
    fn to_rgb(self, n: usize, lut: &Self::Context) -> RGBLU {
        self.to_linear(lut).to_rgb(n, &())
    }
}

/// You can customize how images are downsampled
///
/// Multi-scale DSSIM needs to scale images down. This is it. It's supposed to return the same type of image, but half the size.
///
/// There is a default implementation that just averages 4 neighboring pixels.
#[doc(hidden)]
pub trait Downsample {
    type Output;
    fn downsample(&self) -> Option<Self::Output>;
}

#[inline(always)]
fn downsample_row_pair_inline<T: Average4 + Copy>(top: &[T], bot: &[T], out: &mut [T]) {
    for (i, out) in out.iter_mut().enumerate() {
        *out = Average4::average4(top[2*i], top[2*i+1], bot[2*i], bot[2*i+1]);
    }
}

#[cfg_attr(target_arch = "x86_64", inline(never))]
#[cfg_attr(not(target_arch = "x86_64"), inline(always))]
fn downsample_row_pair_base<T: Average4 + Copy>(top: &[T], bot: &[T], out: &mut [T]) {
    downsample_row_pair_inline(top, bot, out);
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma")]
fn downsample_row_pair_avx2<T: Average4 + Copy>(top: &[T], bot: &[T], out: &mut [T]) {
    downsample_row_pair_inline(top, bot, out);
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2,fma,avx512f,avx512bw,avx512dq,avx512vl")]
fn downsample_row_pair_avx512<T: Average4 + Copy>(top: &[T], bot: &[T], out: &mut [T]) {
    downsample_row_pair_inline(top, bot, out);
}

#[inline]
fn downsample_row_pair<T: Average4 + Copy>(top: &[T], bot: &[T], out: &mut [T]) {
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx512() {
        // SAFETY: the gate checks all target features required by this clone.
        return unsafe { downsample_row_pair_avx512(top, bot, out) };
    }
    #[cfg(target_arch = "x86_64")]
    if crate::caps::has_avx2_fma() {
        // SAFETY: the gate checks all target features required by this clone.
        return unsafe { downsample_row_pair_avx2(top, bot, out) };
    }
    downsample_row_pair_base(top, bot, out);
}

impl<T> Downsample for ImgVec<T> where T: Average4 + Copy + Sync + Send {
    type Output = Self;

    fn downsample(&self) -> Option<Self::Output> {
        self.as_ref().downsample()
    }
}

impl<T> Downsample for ImgRef<'_, T> where T: Average4 + Copy + Sync + Send {
    type Output = ImgVec<T>;

    fn downsample(&self) -> Option<Self::Output> {
        let stride = self.stride();
        let width = self.width();
        let height = self.height();

        if width < 8 || height < 8 {
            return None;
        }

        let half_height = height / 2;
        let half_width = width / 2;

        // Copy initializes valid storage for any T; the row kernels overwrite it.
        let mut scaled = vec![self.buf()[0]; half_width * half_height];
        for (pair, out) in self.buf().chunks(stride * 2).take(half_height)
            .zip(scaled.chunks_exact_mut(half_width))
        {
            let (top, bot) = pair.split_at(stride);
            downsample_row_pair(&top[..half_width * 2], &bot[..half_width * 2], out);
        }

        Some(Img::new(scaled, half_width, half_height))
    }
}

/// Internal view of gamma-encoded pixels. Keeping the adapter private avoids
/// adding trait implementations to callers' integer-pixel image types.
pub(crate) struct GammaImage<'a, P>(pub(crate) ImgRef<'a, P>);

impl<P> Downsample for GammaImage<'_, P>
where
    P: GammaPixel<Output = RGBAPLU> + Copy,
{
    type Output = ImgVec<RGBAPLU>;

    fn downsample(&self) -> Option<Self::Output> {
        let img = self.0;
        let stride = img.stride();
        let width = img.width();
        let height = img.height();

        if width < 8 || height < 8 {
            return None;
        }

        let half_height = height / 2;
        let half_width = width / 2;
        let lut = P::make_lut();

        // Linearize before averaging, just as the materialized input path does.
        let mut scaled = Vec::with_capacity(half_width * half_height);
        scaled.extend(img.buf().chunks(stride * 2).take(half_height).flat_map(|pair| {
            let lut = &lut;
            let (top, bot) = pair.split_at(stride);
            let top = &top[0..half_width * 2];
            let bot = &bot[0..half_width * 2];

            top.as_chunks::<2>().0.iter()
                .zip(bot.chunks_exact(2))
                .map(move |(a, b)| Average4::average4(
                    a[0].to_linear(lut), a[1].to_linear(lut),
                    b[0].to_linear(lut), b[1].to_linear(lut),
                ))
        }));

        assert_eq!(half_width * half_height, scaled.len());
        Some(Img::new(scaled, half_width, half_height))
    }
}

#[allow(dead_code)]
pub(crate) fn worst(input: ImgRef<'_, f32>) -> ImgVec<f32> {
    let stride = input.stride();
    let half_height = input.height() / 2;
    let half_width = input.width() / 2;

    if half_height < 4 || half_width < 4 {
        return input.new_buf(input.buf().to_vec());
    }

    let mut scaled = Vec::with_capacity(half_width * half_height);
    scaled.extend(input.buf().chunks(stride * 2).take(half_height).flat_map(|pair| {
        let (top, bot) = pair.split_at(stride);
        let top = &top[0..half_width * 2];
        let bot = &bot[0..half_width * 2];

        top.as_chunks::<2>().0.iter().zip(bot.chunks_exact(2)).map(|(a,b)| {
            a[0].min(a[1]).min(b[0].min(b[1]))
        })
    }));

    assert_eq!(half_width * half_height, scaled.len());
    Img::new(scaled, half_width, half_height)
}

#[allow(dead_code)]
pub(crate) fn avgworst(input: ImgRef<'_, f32>) -> ImgVec<f32> {
    let stride = input.stride();
    let half_height = input.height() / 2;
    let half_width = input.width() / 2;

    if half_height < 4 || half_width < 4 {
        return input.new_buf(input.buf().to_vec());
    }

    let mut scaled = Vec::with_capacity(half_width * half_height);
    scaled.extend(input.buf().chunks(stride * 2).take(half_height).flat_map(|pair| {
        let (top, bot) = pair.split_at(stride);
        let top = &top[0..half_width * 2];
        let bot = &bot[0..half_width * 2];

        top.as_chunks::<2>().0.iter()
            .zip(bot.chunks_exact(2))
            .map(|(a, b)| (a[0] + a[1] + b[0] + b[1]).mul_add(0.25, a[0].min(a[1]).min(b[0].min(b[1]))) * 0.5)
    }));

    assert_eq!(half_width * half_height, scaled.len());
    Img::new(scaled, half_width, half_height)
}

#[allow(dead_code)]
pub(crate) fn avg(input: ImgRef<'_, f32>) -> ImgVec<f32> {
    let stride = input.stride();
    let half_height = input.height() / 2;
    let half_width = input.width() / 2;

    if half_height < 4 || half_width < 4 {
        return input.new_buf(input.buf().to_vec());
    }

    let mut scaled = Vec::with_capacity(half_width * half_height);
    scaled.extend(input.buf().chunks(stride * 2).take(half_height).flat_map(|pair| {
        let (top, bot) = pair.split_at(stride);
        let top = &top[0..half_width * 2];
        let bot = &bot[0..half_width * 2];

        top.as_chunks::<2>().0.iter().zip(bot.chunks_exact(2)).map(|(a,b)| {
            (a[0] + a[1] + b[0] + b[1]) * 0.25
        })
    }));

    assert_eq!(half_width * half_height, scaled.len());
    Img::new(scaled, half_width, half_height)
}

#[cfg(test)]
#[path = "image/dispatch_tests.rs"]
mod dispatch_tests;
