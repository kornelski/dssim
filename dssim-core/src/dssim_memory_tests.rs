use super::*;

#[test]
fn fused_compare_bitexact_vs_plane_path() {
    use imgref::*;

    fn mkimg(w: usize, h: usize, seed: u32) -> ImgVec<f32> {
        let mut s = seed;
        ImgVec::new_stride(
            (0..(w + 3) * h).map(|_| {
                s ^= s << 13; s ^= s >> 17; s ^= s << 5;
                (s as f32) / (u32::MAX as f32)
            }).collect(),
            w, h, w + 3,
        )
    }

    fn mk_scale(imgs: Vec<ImgVec<f32>>) -> DssimChanScale<f32> {
        DssimChanScale {
            chan: imgs.into_iter().map(|img| DssimChan {
                width: img.width(),
                height: img.height(),
                img,
            }).collect(),
        }
    }

    /// Reference: upstream main's plane-materializing compare — blur each
    /// of the five product planes independently with `blur()`, then apply
    /// the per-pixel ssim formula.
    fn reference(orig: &DssimChanScale<f32>, modif: &DssimChanScale<f32>) -> Vec<f32> {
        let w = orig.chan[0].width;
        let h = orig.chan[0].height;
        let px = w * h;
        let planes: Vec<[Vec<f32>; 5]> = orig.chan.iter().zip(&modif.chan).map(|(c1, c2)| {
            let i1 = &c1.img;
            let i2 = &c2.img;
            let mut tmp = vec![MaybeUninit::uninit(); px];
            [
                blur::blur(i1.as_ref(), &mut tmp).buf().to_vec(),
                blur::blur(i2.as_ref(), &mut tmp).buf().to_vec(),
                blur::blur_mul(i1.as_ref(), i1.as_ref(), &mut tmp),
                blur::blur_mul(i2.as_ref(), i2.as_ref(), &mut tmp),
                blur::blur_mul(i1.as_ref(), i2.as_ref(), &mut tmp),
            ]
        }).collect();
        if orig.chan.len() == 3 {
            let s = Ssim3Planes {
                mu1: [&planes[0][0], &planes[1][0], &planes[2][0]],
                mu2: [&planes[0][1], &planes[1][1], &planes[2][1]],
                sq1: [&planes[0][2], &planes[1][2], &planes[2][2]],
                sq2: [&planes[0][3], &planes[1][3], &planes[2][3]],
                i12: [&planes[0][4], &planes[1][4], &planes[2][4]],
            };
            (0..px).map(|k| ssim3_px(&s, k)).collect()
        } else {
            let p: [&[f32]; 5] =
                [&planes[0][0], &planes[0][1], &planes[0][2], &planes[0][3], &planes[0][4]];
            (0..px).map(|k| rows::ssim1_px(&p, k)).collect()
        }
    }

    let shapes = [
        (1usize, 1usize), (2, 2), (3, 4), (4, 3), (5, 5), (7, 6), (8, 17),
        (9, 15), (1, 33), (16, 16), (17, 32), (31, 33), (4, 48), (64, 17),
        (33, 40), (255, 15), (64, 64), (17, 1),
    ];
    for &(w, h) in &shapes {
        for nchan in [1usize, 3] {
            let orig = mk_scale((0..nchan).map(|c| mkimg(w, h, 0x1111 + c as u32)).collect());
            let modif = mk_scale((0..nchan).map(|c| mkimg(w, h, 0x9999 + c as u32)).collect());
            let fused = Dssim::compare_scale_fused(&orig, &modif);
            let expected = reference(&orig, &modif);
            assert_eq!(
                fused.pixels().map(f32::to_bits).collect::<Vec<_>>(),
                expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                "fused compare diverged at {w}x{h} nchan={nchan}",
            );
        }
    }
}

/// Identical images must yield ssim == 1.0 exactly through the fused path
/// (the same invariant `poison` checks at the API level).
#[test]
fn fused_compare_identical_is_one() {
    use imgref::*;
    fn mk(w: usize, h: usize, seed: u32, nchan: usize) -> DssimChanScale<f32> {
        let mut s = seed;
        DssimChanScale {
            chan: (0..nchan).map(|_| {
                ImgVec::new(
                    (0..w * h).map(|_| {
                        s ^= s << 13; s ^= s >> 17; s ^= s << 5;
                        (s as f32) / (u32::MAX as f32)
                    }).collect::<Vec<_>>(),
                    w, h,
                )
            }).map(|img| DssimChan { width: w, height: h, img }).collect(),
        }
    }
    for &(w, h) in &[(1usize, 1usize), (5, 5), (17, 33), (64, 40)] {
        for nchan in [1usize, 3] {
            let scale = mk(w, h, 0x5EED + nchan as u32, nchan);
            let fused = Dssim::compare_scale_fused(&scale, &scale);
            assert!(
                fused.buf().iter().all(|&v| v == 1.0),
                "{w}x{h} nchan={nchan}: identical-image ssim != 1.0",
            );
        }
    }
}
