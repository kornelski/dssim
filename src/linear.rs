use super::image::RGBAPLU;
use rgb::*;
extern crate lodepng;

pub trait GammaComponent {
    fn max_value() -> usize;
    fn to_linear(&self, lut: &[f32]) -> f32;
}

pub trait GammaPixel {
    type Component: GammaComponent;
    type Output;

    fn to_linear(&self, gamma_lut: &[f32]) -> Self::Output;

    fn make_lut() -> Vec<f32> {
        (0..Self::Component::max_value() + 1)
            .map(|i| to_linear(i as f32 / Self::Component::max_value() as f32))
            .collect()
    }
}

fn to_linear(s: f32) -> f32 {
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

pub trait ToRGBAPLU {
    fn to_rgbaplu(&self) -> Vec<RGBAPLU>;
}

pub trait ToGLU {
    fn to_glu(&self) -> Vec<f32>;
}

impl GammaComponent for u8 {
    fn max_value() -> usize { 255 }
    fn to_linear(&self, lut: &[f32]) -> f32 {
        lut[*self as usize]
    }
}

impl GammaComponent for u16 {
    fn max_value() -> usize { 65535 }
    fn to_linear(&self, lut: &[f32]) -> f32 {
        lut[u16::from_be(*self) as usize] // Hacky! Lodepng assumes big-endian u16
    }
}

impl<M> ToGLU for [M] where M: GammaPixel<Output=f32> {
    fn to_glu(&self) -> Vec<f32> {
        let gamma_lut = M::make_lut();
        self.iter().map(|px| px.to_linear(&gamma_lut)).collect()
    }
}

impl<M> GammaPixel for RGBA<M> where M: Clone + Into<f32> + GammaComponent {
    type Component = M;
    type Output = RGBAPLU;
    fn to_linear(&self, gamma_lut: &[f32]) -> RGBAPLU {
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
    fn to_linear(&self, gamma_lut: &[f32]) -> RGBAPLU {
        RGBAPLU {
            r: self.r.to_linear(gamma_lut),
            g: self.g.to_linear(gamma_lut),
            b: self.b.to_linear(gamma_lut),
            a: 1.0,
        }
    }
}

impl<M> GammaPixel for lodepng::GreyAlpha<M> where M: Clone + Into<f32> + GammaComponent {
    type Component = M;
    type Output = RGBAPLU;
    fn to_linear(&self, gamma_lut: &[f32]) -> RGBAPLU {
        let a_unit = self.1.clone().into() / M::max_value() as f32;
        let g = self.0.to_linear(gamma_lut);
        RGBAPLU {
            r: g * a_unit,
            g: g * a_unit,
            b: g * a_unit,
            a: a_unit,
        }
    }
}

impl<M> GammaPixel for lodepng::Grey<M> where M: GammaComponent {
    type Component = M;
    type Output = RGBAPLU;
    fn to_linear(&self, gamma_lut: &[f32]) -> RGBAPLU {
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
}
