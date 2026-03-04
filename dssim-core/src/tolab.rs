#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use crate::image::ToRGB;
use crate::image::RGBAPLU;
use crate::image::RGBLU;
#[cfg(not(feature = "threads"))]
use crate::lieon as rayon;
use imgref::*;
use rayon::prelude::*;

const D65x: f32 = 0.9505;
const D65y: f32 = 1.0;
const D65z: f32 = 1.089;

pub type GBitmap = ImgVec<f32>;
pub(crate) trait ToLAB {
    fn to_lab(&self) -> (f32, f32, f32);
}

#[inline(always)]
fn fma_matrix(r: f32, rx: f32, g: f32, gx: f32, b: f32, bx: f32) -> f32 {
    r * rx + g * gx + b * bx
}

const EPSILON: f32 = 216. / 24389.;
const K: f32 = 24389. / (27. * 116.); // http://www.brucelindbloom.com/LContinuity.html

impl ToLAB for RGBLU {
    fn to_lab(&self) -> (f32, f32, f32) {
        let fx = fma_matrix(
            self.r,
            0.4124 / D65x,
            self.g,
            0.3576 / D65x,
            self.b,
            0.1805 / D65x,
        );
        let fy = fma_matrix(
            self.r,
            0.2126 / D65y,
            self.g,
            0.7152 / D65y,
            self.b,
            0.0722 / D65y,
        );
        let fz = fma_matrix(
            self.r,
            0.0193 / D65z,
            self.g,
            0.1192 / D65z,
            self.b,
            0.9505 / D65z,
        );

        let X = if fx > EPSILON {
            cbrt_poly(fx) - 16. / 116.
        } else {
            K * fx
        };
        let Y = if fy > EPSILON {
            cbrt_poly(fy) - 16. / 116.
        } else {
            K * fy
        };
        let Z = if fz > EPSILON {
            cbrt_poly(fz) - 16. / 116.
        } else {
            K * fz
        };

        let lab = (
            (Y * 1.05f32), // 1.05 instead of 1.16 to boost color importance without pushing colors outside of 1.0 range
            (500.0 / 220.0) * (X - Y) + (86.2 / 220.0), /* 86 is a fudge to make the value positive */
            (200.0 / 220.0) * (Y - Z) + (107.9 / 220.0), /* 107 is a fudge to make the value positive */
        );
        debug_assert!(lab.0 <= 1.0 && lab.1 <= 1.0 && lab.2 <= 1.0);
        lab
    }
}

#[inline]
fn cbrt_poly(x: f32) -> f32 {
    // Polynomial approximation
    let poly = [0.2, 1.51, -0.5];
    let y = (poly[2] * x + poly[1]) * x + poly[0];

    // 2x Halley's Method
    let y3 = y * y * y;
    let y = y * (y3 + 2. * x) / (2. * y3 + x);
    let y3 = y * y * y;
    let y = y * (y3 + 2. * x) / (2. * y3 + x);
    debug_assert!(y < 1.001);
    debug_assert!(x < 216. / 24389. || y >= 16. / 116.);
    y
}

// ── AVX2+FMA SIMD path ──────────────────────────────────────────────
#[cfg(all(feature = "fma", target_arch = "x86_64"))]
mod simd {
    use super::{cbrt_poly, GBitmap, ToLAB, EPSILON, K, RGBLU};
    #[cfg(not(feature = "threads"))]
    use crate::lieon::prelude::*;
    use archmage::prelude::*;
    use imgref::*;
    use magetypes::simd::f32x8;
    #[cfg(feature = "threads")]
    use rayon::prelude::*;

    #[rite]
    fn cbrt_poly_x8(t: X64V3Token, x: f32x8) -> f32x8 {
        let two = f32x8::splat(t, 2.0);
        let half_neg = f32x8::splat(t, -0.5);
        let c151 = f32x8::splat(t, 1.51);
        let c02 = f32x8::splat(t, 0.2);

        // Polynomial guess: y = (-0.5*x + 1.51)*x + 0.2
        let y = half_neg.mul_add(x, c151).mul_add(x, c02);

        // 1st Halley's: y = y*(y³+2x) / (2y³+x)
        let y3 = y * y * y;
        let y = y * two.mul_add(x, y3) / two.mul_add(y3, x);

        // 2nd Halley's
        let y3 = y * y * y;
        y * two.mul_add(x, y3) / two.mul_add(y3, x)
    }

    #[rite]
    fn to_lab_x8(t: X64V3Token, vr: f32x8, vg: f32x8, vb: f32x8) -> (f32x8, f32x8, f32x8) {
        // Matrix constants (pre-divided by D65)
        let m00 = f32x8::splat(t, 0.4124 / 0.9505);
        let m01 = f32x8::splat(t, 0.3576 / 0.9505);
        let m02 = f32x8::splat(t, 0.1805 / 0.9505);
        let m10 = f32x8::splat(t, 0.2126);
        let m11 = f32x8::splat(t, 0.7152);
        let m12 = f32x8::splat(t, 0.0722);
        let m20 = f32x8::splat(t, 0.0193 / 1.089);
        let m21 = f32x8::splat(t, 0.1192 / 1.089);
        let m22 = f32x8::splat(t, 0.9505 / 1.089);

        // RGB → XYZ/D65 (6 FMAs + 3 muls)
        let fx = vr.mul_add(m00, vg.mul_add(m01, vb * m02));
        let fy = vr.mul_add(m10, vg.mul_add(m11, vb * m12));
        let fz = vr.mul_add(m20, vg.mul_add(m21, vb * m22));

        // Conditional cbrt: if f > EPSILON { cbrt(f) - 16/116 } else { K*f }
        let veps = f32x8::splat(t, EPSILON);
        let vk = f32x8::splat(t, K);
        let v16_116 = f32x8::splat(t, 16.0 / 116.0);

        let cbrt_x = cbrt_poly_x8(t, fx) - v16_116;
        let linear_x = vk * fx;
        let big_x = f32x8::blend(fx.simd_gt(veps), cbrt_x, linear_x);

        let cbrt_y = cbrt_poly_x8(t, fy) - v16_116;
        let linear_y = vk * fy;
        let big_y = f32x8::blend(fy.simd_gt(veps), cbrt_y, linear_y);

        let cbrt_z = cbrt_poly_x8(t, fz) - v16_116;
        let linear_z = vk * fz;
        let big_z = f32x8::blend(fz.simd_gt(veps), cbrt_z, linear_z);

        // LAB scaling
        let l = big_y * f32x8::splat(t, 1.05);
        let a = (big_x - big_y) * f32x8::splat(t, 500.0 / 220.0) + f32x8::splat(t, 86.2 / 220.0);
        let b = (big_y - big_z) * f32x8::splat(t, 200.0 / 220.0) + f32x8::splat(t, 107.9 / 220.0);

        (l, a, b)
    }

    #[arcane]
    pub fn process_row_avx2(
        t: X64V3Token,
        in_row: &[RGBLU],
        l_row: &mut [f32],
        a_row: &mut [f32],
        b_row: &mut [f32],
    ) {
        let width = in_row.len();
        let mut x = 0;

        // SIMD loop: 8 pixels at a time
        while x + 8 <= width {
            // AoS→SoA gather
            let mut rs = [0f32; 8];
            let mut gs = [0f32; 8];
            let mut bs = [0f32; 8];
            for i in 0..8 {
                rs[i] = in_row[x + i].r;
                gs[i] = in_row[x + i].g;
                bs[i] = in_row[x + i].b;
            }
            let vr = f32x8::from_array(t, rs);
            let vg = f32x8::from_array(t, gs);
            let vb = f32x8::from_array(t, bs);

            let (vl, va, vb_out) = to_lab_x8(t, vr, vg, vb);

            vl.store((&mut l_row[x..x + 8]).try_into().unwrap());
            va.store((&mut a_row[x..x + 8]).try_into().unwrap());
            vb_out.store((&mut b_row[x..x + 8]).try_into().unwrap());
            x += 8;
        }

        // Scalar tail
        for x in x..width {
            let (l, a, b) = in_row[x].to_lab();
            l_row[x] = l;
            a_row[x] = a;
            b_row[x] = b;
        }
    }

    #[arcane]
    pub fn process_gray_row_avx2(t: X64V3Token, in_row: &[f32], out_row: &mut [f32]) {
        let width = in_row.len();
        let veps = f32x8::splat(t, EPSILON);
        let vk_116 = f32x8::splat(t, K * 1.16);
        let v16_116 = f32x8::splat(t, 16.0 / 116.0);
        let v116 = f32x8::splat(t, 1.16);

        let mut x = 0;
        while x + 8 <= width {
            let fy = f32x8::load(t, (&in_row[x..x + 8]).try_into().unwrap());
            let cbrt_path = (cbrt_poly_x8(t, fy) - v16_116) * v116;
            let linear_path = vk_116 * fy;
            let result = f32x8::blend(fy.simd_gt(veps), cbrt_path, linear_path);
            result.store((&mut out_row[x..x + 8]).try_into().unwrap());
            x += 8;
        }

        // Scalar tail
        for x in x..width {
            let fy = in_row[x];
            out_row[x] = if fy > EPSILON {
                (cbrt_poly(fy) - 16. / 116.) * 1.16
            } else {
                (K * 1.16) * fy
            };
        }
    }

    pub fn rgb_to_lab_simd(token: X64V3Token, img: ImgRef<'_, RGBLU>) -> Vec<GBitmap> {
        let width = img.width();
        let height = img.height();
        assert!(width > 0);
        let area = width * height;

        let mut out_l = vec![0f32; area];
        let mut out_a = vec![0f32; area];
        let mut out_b = vec![0f32; area];

        out_l
            .par_chunks_exact_mut(width)
            .zip(
                out_a
                    .par_chunks_exact_mut(width)
                    .zip(out_b.par_chunks_exact_mut(width)),
            )
            .enumerate()
            .for_each(|(y, (l_row, (a_row, b_row)))| {
                let in_row = &img.rows().nth(y).unwrap()[0..width];
                process_row_avx2(token, in_row, l_row, a_row, b_row);
            });

        vec![
            Img::new(out_l, width, height),
            Img::new(out_a, width, height),
            Img::new(out_b, width, height),
        ]
    }

    pub fn gray_to_lab_simd(token: X64V3Token, img: &GBitmap) -> Vec<GBitmap> {
        let width = img.width();
        let height = img.height();
        let area = width * height;
        let mut out = vec![0f32; area];

        out.par_chunks_exact_mut(width)
            .enumerate()
            .for_each(|(y, out_row)| {
                let in_row = &img.rows().nth(y).unwrap()[0..width];
                process_gray_row_avx2(token, in_row, out_row);
            });

        vec![Img::new(out, width, height)]
    }
}

/// Convert image to L\*a\*b\* planar
///
/// It should return 1 (gray) or 3 (color) planes.
pub trait ToLABBitmap {
    fn to_lab(&self) -> Vec<GBitmap>;
}

impl ToLABBitmap for ImgVec<RGBAPLU> {
    #[inline(always)]
    fn to_lab(&self) -> Vec<GBitmap> {
        self.as_ref().to_lab()
    }
}

impl ToLABBitmap for ImgVec<RGBLU> {
    #[inline(always)]
    fn to_lab(&self) -> Vec<GBitmap> {
        self.as_ref().to_lab()
    }
}
impl ToLABBitmap for GBitmap {
    fn to_lab(&self) -> Vec<GBitmap> {
        debug_assert!(self.width() > 0);

        #[cfg(all(feature = "fma", target_arch = "x86_64"))]
        {
            use archmage::SimdToken as _;
            if let Some(token) = archmage::X64V3Token::summon() {
                return simd::gray_to_lab_simd(token, self);
            }
        }

        let f = |fy| {
            if fy > EPSILON {
                (cbrt_poly(fy) - 16. / 116.) * 1.16
            } else {
                (K * 1.16) * fy
            }
        };

        #[cfg(feature = "threads")]
        let out = (0..self.height())
            .into_par_iter()
            .flat_map_iter(|y| self[y].iter().map(|&fy| f(fy)))
            .collect();

        #[cfg(not(feature = "threads"))]
        let out = self.pixels().map(f).collect();

        vec![Img::new(out, self.width(), self.height())]
    }
}

#[inline(never)]
fn rgb_to_lab<T: Copy + Sync + Send + 'static, F>(img: ImgRef<'_, T>, cb: F) -> Vec<GBitmap>
where
    F: Fn(T, usize) -> (f32, f32, f32) + Sync + Send + 'static,
{
    let width = img.width();
    assert!(width > 0);
    let height = img.height();
    let area = width * height;

    let mut out_l = vec![0f32; area];
    let mut out_a = vec![0f32; area];
    let mut out_b = vec![0f32; area];

    // For output width == stride
    out_l
        .par_chunks_exact_mut(width)
        .take(height)
        .zip(
            out_a
                .par_chunks_exact_mut(width)
                .take(height)
                .zip(out_b.par_chunks_exact_mut(width).take(height)),
        )
        .enumerate()
        .for_each(|(y, (l_row, (a_row, b_row)))| {
            let in_row = &img.rows().nth(y).unwrap()[0..width];
            let l_row = &mut l_row[0..width];
            let a_row = &mut a_row[0..width];
            let b_row = &mut b_row[0..width];
            for x in 0..width {
                let n = (x + 11) ^ (y + 11);
                let (l, a, b) = cb(in_row[x], n);
                l_row[x] = l;
                a_row[x] = a;
                b_row[x] = b;
            }
        });

    vec![
        Img::new(out_l, width, height),
        Img::new(out_a, width, height),
        Img::new(out_b, width, height),
    ]
}

impl ToLABBitmap for ImgRef<'_, RGBAPLU> {
    #[inline]
    fn to_lab(&self) -> Vec<GBitmap> {
        rgb_to_lab(*self, |px, n| px.to_rgb(n).to_lab())
    }
}

impl ToLABBitmap for ImgRef<'_, RGBLU> {
    #[inline]
    fn to_lab(&self) -> Vec<GBitmap> {
        #[cfg(all(feature = "fma", target_arch = "x86_64"))]
        {
            use archmage::SimdToken as _;
            if let Some(token) = archmage::X64V3Token::summon() {
                return simd::rgb_to_lab_simd(token, *self);
            }
        }
        rgb_to_lab(*self, |px, _n| px.to_lab())
    }
}

#[test]
fn cbrts1() {
    let mut totaldiff = 0.;
    let mut maxdiff: f64 = 0.;
    for i in (0..=10001).rev() {
        let x = (i as f64 / 10001.) as f32;
        let a = cbrt_poly(x);
        let actual = a * a * a;
        let expected = x;
        let absdiff = (expected as f64 - actual as f64).abs();
        assert!(
            absdiff < 0.0002,
            "{expected} - {actual} = {} @ {x}",
            expected - actual
        );
        if i % 400 == 0 {
            println!("{:+0.3}", (expected - actual) * 255.);
        }
        totaldiff += absdiff;
        maxdiff = maxdiff.max(absdiff);
    }
    println!("1={totaldiff:0.6}; {maxdiff:0.8}");
    assert!(totaldiff < 0.0025, "{totaldiff}");
}

#[test]
fn cbrts2() {
    let mut totaldiff = 0.;
    let mut maxdiff: f64 = 0.;
    for i in (2000..=10001).rev() {
        let x = i as f64 / 10001.;
        let actual = cbrt_poly(x as f32) as f64;
        let expected = x.cbrt();
        let absdiff = (expected - actual).abs();
        totaldiff += absdiff;
        maxdiff = maxdiff.max(absdiff);
        assert!(
            absdiff < 0.0000005,
            "{expected} - {actual} = {} @ {x}",
            expected - actual
        );
    }
    println!("2={totaldiff:0.6}; {maxdiff:0.8}");
    assert!(totaldiff < 0.0025, "{totaldiff}");
}
