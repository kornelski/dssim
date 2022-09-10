use crate::image::RGBAPLU;
use crate::image::RGBLU;
use rgb::alt::*;
use rgb::*;

/// See `GammaPixel` & `ToRGBAPLU`
#[doc(hidden)]
pub trait GammaComponent {
    type Lut;
    fn max_value() -> usize;
    fn to_linear(&self, lut: &Self::Lut) -> f32;
    fn make_lut() -> Self::Lut;
}

/// Downsampling should be done in linear RGB color space.
///
/// Used by `ToRGBAPLU`
///
/// This trait provides gamma to linear conversion via lookup table,
/// and there's implementation for sRGB for common RGB types.
#[doc(hidden)]
pub trait GammaPixel {
    type Component: GammaComponent;
    type Output;

    fn to_linear(&self, gamma_lut: &<Self::Component as GammaComponent>::Lut) -> Self::Output;

    #[inline(always)]
    fn make_lut() -> <Self::Component as GammaComponent>::Lut {
        <Self::Component as GammaComponent>::make_lut()
    }
}

#[inline]
fn to_linear(s: f32) -> f32 {
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

/// RGBA Premultiplied Linear-light Unit scale
///
/// Convenience function `.to_rgbaplu()` to convert RGBA bitmaps to a format useful for DSSIM.
pub trait ToRGBAPLU {
    /// Convert with alpha channel preserved
    fn to_rgbaplu(&self) -> Vec<RGBAPLU>;
    /// Discard alpha channel, if any
    fn to_rgblu(&self) -> Vec<RGBLU>;
}

impl GammaComponent for u8 {
    type Lut = [f32; 256];
    fn max_value() -> usize { 255 }
    #[inline(always)]
    fn to_linear(&self, lut: &Self::Lut) -> f32 {
        lut[*self as usize]
    }

    #[inline]
    fn make_lut() -> Self::Lut {
        let mut out = [0.; 256];
        for (i, o) in out.iter_mut().enumerate() {
            *o = to_linear(i as f32 / Self::max_value() as f32);
        }
        out
    }
}

impl GammaComponent for u16 {
    type Lut = [f32; 65536];
    fn max_value() -> usize { 65535 }
    #[inline(always)]
    fn to_linear(&self, lut: &Self::Lut) -> f32 {
        lut[*self as usize]
    }

    #[inline]
    fn make_lut() -> Self::Lut {
        let mut out = [0.; 65536];
        for (i, o) in out.iter_mut().enumerate() {
            *o = to_linear(i as f32 / Self::max_value() as f32);
        }
        out
    }
}

impl<M> GammaPixel for RGBA<M> where M: Clone + Into<f32> + GammaComponent {
    type Component = M;
    type Output = RGBAPLU;
    #[inline]
    fn to_linear(&self, gamma_lut: &M::Lut) -> RGBAPLU {
        let a_unit = self.a.clone().into() / M::max_value() as f32;
        RGBAPLU {
            r: self.r.to_linear(gamma_lut) * a_unit,
            g: self.g.to_linear(gamma_lut) * a_unit,
            b: self.b.to_linear(gamma_lut) * a_unit,
            a: a_unit,
        }
    }
}

impl<M> GammaPixel for BGRA<M> where M: Clone + Into<f32> + GammaComponent {
    type Component = M;
    type Output = RGBAPLU;
    #[inline]
    fn to_linear(&self, gamma_lut: &M::Lut) -> RGBAPLU {
        let a_unit = self.a.clone().into() / M::max_value() as f32;
        RGBAPLU {
            r: self.r.to_linear(gamma_lut) * a_unit,
            g: self.g.to_linear(gamma_lut) * a_unit,
            b: self.b.to_linear(gamma_lut) * a_unit,
            a: a_unit,
        }
    }
}

impl<M> GammaPixel for RGB<M> where M: GammaComponent {
    type Component = M;
    type Output = RGBAPLU;
    #[inline]
    fn to_linear(&self, gamma_lut: &M::Lut) -> RGBAPLU {
        RGBAPLU {
            r: self.r.to_linear(gamma_lut),
            g: self.g.to_linear(gamma_lut),
            b: self.b.to_linear(gamma_lut),
            a: 1.0,
        }
    }
}

impl<M> GammaPixel for BGR<M> where M: GammaComponent {
    type Component = M;
    type Output = RGBAPLU;
    #[inline]
    fn to_linear(&self, gamma_lut: &M::Lut) -> RGBAPLU {
        RGBAPLU {
            r: self.r.to_linear(gamma_lut),
            g: self.g.to_linear(gamma_lut),
            b: self.b.to_linear(gamma_lut),
            a: 1.0,
        }
    }
}

impl<M> GammaPixel for GrayAlpha<M> where M: Copy + Clone + Into<f32> + GammaComponent {
    type Component = M;
    type Output = RGBAPLU;
    fn to_linear(&self, gamma_lut: &M::Lut) -> RGBAPLU {
        let a_unit = self.1.into() / M::max_value() as f32;
        let g = self.0.to_linear(gamma_lut);
        RGBAPLU {
            r: g * a_unit,
            g: g * a_unit,
            b: g * a_unit,
            a: a_unit,
        }
    }
}

impl<M> GammaPixel for M where M: GammaComponent {
    type Component = M;
    type Output = f32;
    #[inline(always)]
    fn to_linear(&self, gamma_lut: &M::Lut) -> f32 {
        self.to_linear(gamma_lut)
    }
}

impl<M> GammaPixel for Gray<M> where M: Copy + GammaComponent {
    type Component = M;
    type Output = RGBAPLU;
    #[inline(always)]
    fn to_linear(&self, gamma_lut: &M::Lut) -> RGBAPLU {
        let g = self.0.to_linear(gamma_lut);
        RGBAPLU {
            r: g,
            g: g,
            b: g,
            a: 1.0,
        }
    }
}

impl<P> ToRGBAPLU for [P] where P: GammaPixel<Output=RGBAPLU> {
    fn to_rgbaplu(&self) -> Vec<RGBAPLU> {
        let gamma_lut = P::make_lut();
        self.iter().map(|px| px.to_linear(&gamma_lut)).collect()
    }

    fn to_rgblu(&self) -> Vec<RGBLU> {
        let gamma_lut = P::make_lut();
        self.iter().map(|px| px.to_linear(&gamma_lut).rgb()).collect()
    }
}
