// AVX2+FMA SIMD for 3-channel SSIM comparison.
// Processes 8 pixels per iteration using 256-bit vectors.

#[cfg(all(feature = "fma", target_arch = "x86_64"))]
mod simd {
    use archmage::prelude::*;
    use magetypes::simd::f32x8;

    #[arcane]
    pub fn compare_3ch_avx2(
        t: X64V3Token,
        o_mu: [&[f32]; 3],
        m_mu: [&[f32]; 3],
        o_sq: [&[f32]; 3],
        m_sq: [&[f32]; 3],
        i12: [&[f32]; 3],
        out: &mut [f32],
    ) {
        let len = out.len();
        let vc1 = f32x8::splat(t, 0.01 * 0.01);
        let vc2 = f32x8::splat(t, 0.03 * 0.03);
        let vinv3 = f32x8::splat(t, 1.0 / 3.0);
        let vtwo = f32x8::splat(t, 2.0);

        let mut i = 0;
        while i + 8 <= len {
            // 15 loads: 6 mu + 6 sq + 3 i12
            let omu0 = f32x8::load(t, (&o_mu[0][i..i + 8]).try_into().unwrap());
            let omu1 = f32x8::load(t, (&o_mu[1][i..i + 8]).try_into().unwrap());
            let omu2 = f32x8::load(t, (&o_mu[2][i..i + 8]).try_into().unwrap());
            let mmu0 = f32x8::load(t, (&m_mu[0][i..i + 8]).try_into().unwrap());
            let mmu1 = f32x8::load(t, (&m_mu[1][i..i + 8]).try_into().unwrap());
            let mmu2 = f32x8::load(t, (&m_mu[2][i..i + 8]).try_into().unwrap());

            let osq0 = f32x8::load(t, (&o_sq[0][i..i + 8]).try_into().unwrap());
            let osq1 = f32x8::load(t, (&o_sq[1][i..i + 8]).try_into().unwrap());
            let osq2 = f32x8::load(t, (&o_sq[2][i..i + 8]).try_into().unwrap());
            let msq0 = f32x8::load(t, (&m_sq[0][i..i + 8]).try_into().unwrap());
            let msq1 = f32x8::load(t, (&m_sq[1][i..i + 8]).try_into().unwrap());
            let msq2 = f32x8::load(t, (&m_sq[2][i..i + 8]).try_into().unwrap());

            let iv0 = f32x8::load(t, (&i12[0][i..i + 8]).try_into().unwrap());
            let iv1 = f32x8::load(t, (&i12[1][i..i + 8]).try_into().unwrap());
            let iv2 = f32x8::load(t, (&i12[2][i..i + 8]).try_into().unwrap());

            // Per-channel mu products (9 muls)
            let mu1mu1_0 = omu0 * omu0;
            let mu2mu2_0 = mmu0 * mmu0;
            let mu1mu2_0 = omu0 * mmu0;

            let mu1mu1_1 = omu1 * omu1;
            let mu2mu2_1 = mmu1 * mmu1;
            let mu1mu2_1 = omu1 * mmu1;

            let mu1mu1_2 = omu2 * omu2;
            let mu2mu2_2 = mmu2 * mmu2;
            let mu1mu2_2 = omu2 * mmu2;

            // Average across channels (* inv3)
            let mu1_sq = (mu1mu1_0 + mu1mu1_1 + mu1mu1_2) * vinv3;
            let mu2_sq = (mu2mu2_0 + mu2mu2_1 + mu2mu2_2) * vinv3;
            let mu1_mu2 = (mu1mu2_0 + mu1mu2_1 + mu1mu2_2) * vinv3;

            // sigma = sq_blur - mu^2, averaged across channels
            let sigma1_sq = ((osq0 - mu1mu1_0) + (osq1 - mu1mu1_1) + (osq2 - mu1mu1_2)) * vinv3;
            let sigma2_sq = ((msq0 - mu2mu2_0) + (msq1 - mu2mu2_1) + (msq2 - mu2mu2_2)) * vinv3;
            let sigma12 = ((iv0 - mu1mu2_0) + (iv1 - mu1mu2_1) + (iv2 - mu1mu2_2)) * vinv3;

            // SSIM = (2*mu1_mu2 + c1)*(2*sigma12 + c2) / ((mu1_sq+mu2_sq+c1)*(sigma1_sq+sigma2_sq+c2))
            let num = vtwo.mul_add(mu1_mu2, vc1) * vtwo.mul_add(sigma12, vc2);
            let den = (mu1_sq + mu2_sq + vc1) * (sigma1_sq + sigma2_sq + vc2);
            let result = num / den;

            result.store((&mut out[i..i + 8]).try_into().unwrap());
            i += 8;
        }

        // Scalar tail for remaining pixels
        let c1: f32 = 0.01 * 0.01;
        let c2: f32 = 0.03 * 0.03;
        let inv3: f32 = 1.0 / 3.0;
        for i in i..len {
            let mu1_0 = o_mu[0][i];
            let mu2_0 = m_mu[0][i];
            let mu1mu1_0 = mu1_0 * mu1_0;
            let mu2mu2_0 = mu2_0 * mu2_0;
            let mu1mu2_0 = mu1_0 * mu2_0;

            let mu1_1 = o_mu[1][i];
            let mu2_1 = m_mu[1][i];
            let mu1mu1_1 = mu1_1 * mu1_1;
            let mu2mu2_1 = mu2_1 * mu2_1;
            let mu1mu2_1 = mu1_1 * mu2_1;

            let mu1_2 = o_mu[2][i];
            let mu2_2 = m_mu[2][i];
            let mu1mu1_2 = mu1_2 * mu1_2;
            let mu2mu2_2 = mu2_2 * mu2_2;
            let mu1mu2_2 = mu1_2 * mu2_2;

            let mu1_sq = (mu1mu1_0 + mu1mu1_1 + mu1mu1_2) * inv3;
            let mu2_sq = (mu2mu2_0 + mu2mu2_1 + mu2mu2_2) * inv3;
            let mu1_mu2 = (mu1mu2_0 + mu1mu2_1 + mu1mu2_2) * inv3;

            let sigma1_sq =
                ((o_sq[0][i] - mu1mu1_0) + (o_sq[1][i] - mu1mu1_1) + (o_sq[2][i] - mu1mu1_2))
                    * inv3;
            let sigma2_sq =
                ((m_sq[0][i] - mu2mu2_0) + (m_sq[1][i] - mu2mu2_1) + (m_sq[2][i] - mu2mu2_2))
                    * inv3;
            let sigma12 =
                ((i12[0][i] - mu1mu2_0) + (i12[1][i] - mu1mu2_1) + (i12[2][i] - mu1mu2_2)) * inv3;

            out[i] = (2.0 * mu1_mu2 + c1) * (2.0 * sigma12 + c2)
                / ((mu1_sq + mu2_sq + c1) * (sigma1_sq + sigma2_sq + c2));
        }
    }
}

#[cfg(all(feature = "fma", target_arch = "x86_64"))]
pub(crate) use simd::compare_3ch_avx2;
