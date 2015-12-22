
/// RGBA, but: premultiplied alpha, linear, f32
#[derive(Debug)]
pub struct RGBAPLU {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

// Linear gray
pub struct GL {
    g: f32,
}

// trait IntoRGBAPF32 {
//     fn to_rgba_pf32(&self) -> Vec<RGBA_pf32>
// }

// impl IntoRGBAPF32 for
