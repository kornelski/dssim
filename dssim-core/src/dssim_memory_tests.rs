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
                moments: None,
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
            let expected = reference(&orig, &modif);
            for cached_orig in [false, true] {
                for cached_mod in [false, true] {
                    let mut orig = orig.clone();
                    let mut modif = modif.clone();
                    for (scale, cached) in [(&mut orig, cached_orig), (&mut modif, cached_mod)] {
                        if cached {
                            for c in &mut scale.chan {
                                let mut tmp = vec![MaybeUninit::uninit(); w * h];
                                c.moments = Some(CachedMoments {
                                    mu: blur::blur(c.img.as_ref(), &mut tmp).into_contiguous_buf().0,
                                    squared: blur::blur_mul(c.img.as_ref(), c.img.as_ref(), &mut tmp),
                                });
                            }
                        }
                    }
                    let fused = Dssim::compare_scale_fused(&orig, &modif);
                    assert_eq!(
                        fused.pixels().map(f32::to_bits).collect::<Vec<_>>(),
                        expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                        "compare diverged at {w}x{h} nchan={nchan} cached={cached_orig}/{cached_mod}",
                    );
                }
            }
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
            }).map(|img| DssimChan { width: w, height: h, img, moments: None }).collect(),
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


#[test]
fn construction_caching_is_opt_in_and_preserves_results() {
    let (w, h) = (33, 35);
    let pixels: Vec<_> = (0..w*h).map(|i| RGB::new((i*13) as u8, (i*31) as u8, (i*7) as u8)).collect();
    let changed: Vec<_> = pixels.iter().map(|p| RGB::new(p.r.wrapping_add(9), p.g, p.b)).collect();
    let mut d = Dssim::new();
    d.set_save_ssim_maps(8);
    let small = d.create_image_rgb(&pixels, w, h).unwrap();
    let small_changed = d.create_image_rgb(&changed, w, h).unwrap();
    assert!(small.scale.iter().chain(&small_changed.scale).flat_map(|s| &s.chan).all(|c| c.moments.is_none()));
    let options = ImageOptions::default().cache_for_reuse(true);
    let cached = d.create_image_rgb_with_options(&pixels, w, h, options).unwrap();
    let cached_changed = d.create_image_rgb_with_options(&changed, w, h, options).unwrap();
    for image in [&cached, &cached_changed] {
        assert!(image.scale.iter().flat_map(|s| &s.chan).all(|c| c.moments.is_some()));
    }
    // Opting in for one construction must not change subsequent defaults.
    let later = d.create_image_rgb(&pixels, w, h).unwrap();
    let disabled = d.create_image_rgb_with_options(&pixels, w, h, options.cache_for_reuse(false)).unwrap();
    for image in [&later, &disabled] {
        assert!(image.scale.iter().flat_map(|s| &s.chan).all(|c| c.moments.is_none()));
    }
    let fingerprint = |a: &DssimImage<f32>, b: &DssimImage<f32>| {
        let (score, maps) = d.compare(a, b);
        (f64::from(score).to_bits(), maps.iter().map(|m| (
            m.ssim.to_bits(), m.map.pixels().map(f32::to_bits).collect::<Vec<_>>()
        )).collect::<Vec<_>>())
    };
    let expected = fingerprint(&small, &small_changed);
    for a in [&cached, &small, &later, &disabled] {
        for b in [&cached_changed, &small_changed] {
            assert_eq!(fingerprint(a, b), expected);
        }
    }
}

#[test]
fn legacy_compare_uses_current_context_settings() {
    let (w, h) = (65, 67);
    let pixels: Vec<_> = (0..w*h).map(|i| RGB::new((i*13) as u8, (i*31) as u8, (i*7) as u8)).collect();
    let changed: Vec<_> = pixels.iter().enumerate().map(|(i, p)| {
        RGB::new(p.r.wrapping_add((i % 19) as u8), p.g / 2, p.b)
    }).collect();
    let mut d = Dssim::new();
    d.set_scales(&[0.2, 0.3, 0.5]);
    d.set_save_ssim_maps(3);
    let reference = d.create_image_rgb(&pixels, w, h).unwrap();
    let candidate = d.create_image_rgb(&changed, w, h).unwrap();
    let (before, maps) = d.compare(&reference, &candidate);
    assert_eq!(maps.len(), 3);

    // These setters have always affected comparisons of existing images.
    d.set_scales(&[0.75, 0.25]);
    d.set_save_ssim_maps(1);
    let (after, retained) = d.compare(&reference, &candidate);
    let expected_ssim = maps[1].ssim.mul_add(0.25, maps[0].ssim.mul_add(0.75, 0.0));
    assert_eq!(f64::from(after).to_bits(), to_dssim(expected_ssim).to_bits());
    assert_ne!(f64::from(before).to_bits(), f64::from(after).to_bits());
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].map.buf(), maps[0].map.buf());

    // Comparing through another context also uses that context's settings.
    let mut other = Dssim::new();
    other.set_scales(&[1.0]);
    let (score, no_maps) = other.compare(&reference, &candidate);
    assert_eq!(f64::from(score).to_bits(), to_dssim(maps[0].ssim).to_bits());
    assert!(no_maps.is_empty());
}
