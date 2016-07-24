use super::image::RGBAPLU;
use rgb::*;
extern crate lodepng;

pub trait GammaComponent {
    fn max_value() -> usize;
    fn make_lut() -> Vec<f32> {
        (0..Self::max_value() + 1)
            .map(|i| to_linear(i as f32 / Self::max_value() as f32))
            .collect()
    }
    fn to_linear(&self, lut: &[f32]) -> f32;
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
        lut[*self as usize]
    }
}

impl<M: GammaComponent> ToGLU for [M] {
    fn to_glu(&self) -> Vec<f32> {
        let gamma_lut = M::make_lut();
        self.iter().map(|px| px.to_linear(&gamma_lut)).collect()
    }
}

impl<M> ToRGBAPLU for [RGBA<M>] where M: Clone + Into<f32> + GammaComponent {
    fn to_rgbaplu(&self) -> Vec<RGBAPLU> {
        let gamma_lut = M::make_lut();
        self.iter().map(|px|{
            let a_unit = px.a.clone().into() / M::max_value() as f32;
            RGBAPLU {
                r: px.r.to_linear(&gamma_lut) * a_unit,
                g: px.g.to_linear(&gamma_lut) * a_unit,
                b: px.b.to_linear(&gamma_lut) * a_unit,
                a: a_unit,
            }
        }).collect()
    }
}

impl<M> ToRGBAPLU for [RGB<M>] where M: Into<f32> + GammaComponent {
    fn to_rgbaplu(&self) -> Vec<RGBAPLU> {
        let gamma_lut = M::make_lut();
        self.iter().map(|px|{
            RGBAPLU {
                r: px.r.to_linear(&gamma_lut),
                g: px.g.to_linear(&gamma_lut),
                b: px.b.to_linear(&gamma_lut),
                a: 1.0,
            }
        }).collect()
    }
}

impl<M> ToRGBAPLU for [lodepng::GreyAlpha<M>] where M: Clone + Into<f32> + GammaComponent {
    fn to_rgbaplu(&self) -> Vec<RGBAPLU> {
        let gamma_lut = M::make_lut();
        self.iter().map(|px|{
            let a_unit = px.1.clone().into() / M::max_value() as f32;
            let g = px.0.to_linear(&gamma_lut);
            RGBAPLU {
                r: g * a_unit,
                g: g * a_unit,
                b: g * a_unit,
                a: a_unit,
            }
        }).collect()
    }
}

impl<M: GammaComponent> ToRGBAPLU for [lodepng::Grey<M>] where M: Into<f32> {
    fn to_rgbaplu(&self) -> Vec<RGBAPLU> {
        let gamma_lut = M::make_lut();
        self.iter().map(|px|{
            let g = px.0.to_linear(&gamma_lut);
            RGBAPLU {
                r: g,
                g: g,
                b: g,
                a: 1.0,
            }
        }).collect()
    }
}
