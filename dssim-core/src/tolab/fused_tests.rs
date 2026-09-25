use super::*;
use crate::image::Downsample;
use crate::linear::ToRGBAPLU;
use rgb::alt::*;
use rgb::{RGB, RGBA};

fn bits(planes: &[GBitmap]) -> Vec<u32> {
    planes.iter().flat_map(|p| p.pixels().map(f32::to_bits)).collect()
}

fn check<P>(pixels: Vec<P>, width: usize, height: usize)
where
    P: GammaPixel<Output = RGBAPLU> + Copy + Send + Sync + 'static,
    <P::Component as GammaComponent>::Lut: Sync,
{
    let linear = Img::new(pixels.to_rgbaplu(), width, height);
    let expected = bits(&linear.to_lab());
    let packed = Img::new(pixels.clone(), width, height);
    assert_eq!(expected, bits(&GammaImage(packed.as_ref()).to_lab()));
    super::dispatch_tests::check_rgb(&pixels[..width], &P::make_lut());

    let mut padded = Vec::new();
    for row in pixels.chunks_exact(width) {
        padded.extend_from_slice(row);
        padded.extend([pixels[0]; 3]);
    }
    let view = Img::new_stride(padded, width, height, width + 3);
    assert_eq!(expected, bits(&GammaImage(view.as_ref()).to_lab()));
    if let Some(reference) = linear.downsample() {
        for down in [GammaImage(packed.as_ref()).downsample().unwrap(), GammaImage(view.as_ref()).downsample().unwrap()] {
            assert_eq!(down.width(), reference.width());
            assert_eq!(down.height(), reference.height());
            for (a,b) in down.pixels().zip(reference.pixels()) {
                assert_eq!([a.r.to_bits(), a.g.to_bits(), a.b.to_bits(), a.a.to_bits()],
                           [b.r.to_bits(), b.g.to_bits(), b.b.to_bits(), b.a.to_bits()]);
            }
        }
    }

    let mut attr = crate::Dssim::new();
    attr.set_save_ssim_maps(8);
    let fused = attr.create_image(&GammaImage(packed.as_ref())).unwrap();
    let reference = attr.create_image(&linear).unwrap();
    let different = Img::new(vec![RGBLU::new(0.19, 0.43, 0.71); width * height], width, height);
    let different = attr.create_image(&different).unwrap();
    let (a, am) = attr.compare(&fused, &different);
    let (b, bm) = attr.compare(&reference, &different);
    assert_eq!(f64::from(a).to_bits(), f64::from(b).to_bits());
    assert_eq!(am.len(), bm.len());
    for (a,b) in am.iter().zip(&bm) {
        assert_eq!(bits(std::slice::from_ref(&a.map)), bits(std::slice::from_ref(&b.map)));
    }
}

#[test]
fn fused_integer_inputs_match_materialized_bits() {
    for (w,h) in [(1,1), (7,9), (9,8), (17,33), (66,34)] {
        let rgba: Vec<_> = (0..w*h).map(|i| {
            let v = (i as u32).wrapping_mul(2_654_435_761).rotate_left(13);
            RGBA::new(v as u8, (v >> 8) as u8, (v >> 16) as u8, [0,255,(v >> 24) as u8][i % 3])
        }).collect();
        check(rgba.clone(), w, h);
        check(rgba.iter().map(|p| RGB::new(p.r,p.g,p.b)).collect(), w, h);
        check(rgba.iter().map(|p| BGRA { b:p.b,g:p.g,r:p.r,a:p.a }).collect(), w, h);
        check(rgba.iter().map(|p| BGR { b:p.b,g:p.g,r:p.r }).collect(), w, h);
        check(rgba.iter().map(|p| Gray::new(p.r)).collect(), w, h);
        check(rgba.iter().map(|p| GrayAlpha::new(p.r,p.a)).collect(), w, h);
        let rgba: Vec<_> = rgba.iter().map(|p| RGBA::new(u16::from(p.r)*257,u16::from(p.g)*257,u16::from(p.b)*257,u16::from(p.a)*257)).collect();
        check(rgba.clone(), w, h);
        check(rgba.iter().map(|p| RGB::new(p.r,p.g,p.b)).collect(), w, h);
        check(rgba.iter().map(|p| BGRA { b:p.b,g:p.g,r:p.r,a:p.a }).collect(), w, h);
        check(rgba.iter().map(|p| BGR { b:p.b,g:p.g,r:p.r }).collect(), w, h);
        check(rgba.iter().map(|p| Gray::new(p.r)).collect(), w, h);
        check(rgba.iter().map(|p| GrayAlpha::new(p.r,p.a)).collect(), w, h);
    }
}
