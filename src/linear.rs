use super::image::RGBAPLU;
use rgb::*;

fn make_lut() -> [f32; 256] {
    let mut gamma_lut = [0f32; 256];

    for i in 0..gamma_lut.len() {
        gamma_lut[i] = to_linear(i as f32 / 255.0);
    }

    gamma_lut
}

fn make_lut16() -> [f32; 1<<16] {
    let mut gamma_lut = [0f32; 1<<16];

    for i in 0..gamma_lut.len() {
        gamma_lut[i] = to_linear(i as f32 / 65535.0);
    }

    gamma_lut
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

impl ToGLU for [u8] {
    fn to_glu(&self) -> Vec<f32> {
        let gamma_lut = make_lut();
        self.iter().cloned().map(|px|gamma_lut[px as usize]).collect()
    }
}

impl ToRGBAPLU for [RGBA<u8>] {
    fn to_rgbaplu(&self) -> Vec<RGBAPLU> {
        let gamma_lut = make_lut();
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
        let gamma_lut = make_lut();

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

impl ToRGBAPLU for [RGBA<u16>] {
    fn to_rgbaplu(&self) -> Vec<RGBAPLU> {
        let gamma_lut = make_lut16();
        self.iter().map(|px|{
            let a_unit = px.a as f32 / 65535.0;
            RGBAPLU {
                r: gamma_lut[px.r as usize] * a_unit,
                g: gamma_lut[px.g as usize] * a_unit,
                b: gamma_lut[px.b as usize] * a_unit,
                a: a_unit,
            }
        }).collect()
    }
}

impl ToRGBAPLU for [RGB<u16>] {
    fn to_rgbaplu(&self) -> Vec<RGBAPLU> {
        let gamma_lut = make_lut16();

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
