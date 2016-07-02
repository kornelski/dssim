use super::image::RGBAPLU;
use rgb::*;

pub trait ToRGBAPLU {
    fn to_rgbaplu(&self) -> Vec<RGBAPLU>;
}

impl ToRGBAPLU for [RGBA<u8>] {
    fn to_rgbaplu(&self) -> Vec<RGBAPLU> {
        let mut gamma_lut = [0f32; 256];

        for i in 0..256 {
            let s: f64 = i as f64 / 255.0;
            gamma_lut[i] = if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            } as f32
        }

        self.iter().map(|px|{
            let a_unit = px.a as f32 / 255.0;
            RGBAPLU {
                r: gamma_lut[px.r as usize] * a_unit,
                g: gamma_lut[px.g as usize] * a_unit,
                b: gamma_lut[px.b as usize] * a_unit,
                a: a_unit,
            }
        }).collect()
    }
}

impl ToRGBAPLU for [RGB<u8>] {
    fn to_rgbaplu(&self) -> Vec<RGBAPLU> {
        let mut gamma_lut = [0f32; 256];

        for i in 0..256 {
            let s: f64 = i as f64 / 255.0;
            gamma_lut[i] = if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            } as f32
        }

        self.iter().map(|px|{
            RGBAPLU {
                r: gamma_lut[px.r as usize],
                g: gamma_lut[px.g as usize],
                b: gamma_lut[px.b as usize],
                a: 1.0,
            }
        }).collect()
    }
}
