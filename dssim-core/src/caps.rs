//! Cached x86 feature gates. A globally enabled tier bypasses its cache;
//! a wider tier can still be detected at runtime (e.g. AVX-512 in a v3 build).
//! AArch64 uses its normal baseline code generation and needs no clones.

/// Whether the CPU supports AVX2 + FMA. Resolved once, then cached.
#[cfg(target_arch = "x86_64")]
#[inline]
pub(crate) fn has_avx2_fma() -> bool {
    // Statically enabled features need neither runtime detection nor a cache.
    #[cfg(all(target_feature = "avx2", target_feature = "fma"))]
    {
        true
    }
    #[cfg(not(all(target_feature = "avx2", target_feature = "fma")))]
    {
        use std::sync::atomic::{AtomicU8, Ordering};
        // 0 = unknown, 1 = avx2+fma supported, 2 = not supported.
        static CAP: AtomicU8 = AtomicU8::new(0);
        match CAP.load(Ordering::Relaxed) {
            1 => true,
            2 => false,
            _ => {
                let yes = is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma");
                CAP.store(if yes { 1 } else { 2 }, Ordering::Relaxed);
                yes
            }
        }
    }
}

/// Whether AVX2/FMA and AVX-512 F/BW/DQ/VL are available. This is the
/// exact feature set of the clones, not the complete psABI v4 feature set.
#[cfg(target_arch = "x86_64")]
#[inline]
pub(crate) fn has_avx512() -> bool {
    // Statically enabled features need neither runtime detection nor a cache.
    #[cfg(all(
        target_feature = "avx512f",
        target_feature = "avx512bw",
        target_feature = "avx512dq",
        target_feature = "avx512vl"
    ))]
    {
        true
    }
    #[cfg(not(all(
        target_feature = "avx512f",
        target_feature = "avx512bw",
        target_feature = "avx512dq",
        target_feature = "avx512vl"
    )))]
    {
        use std::sync::atomic::{AtomicU8, Ordering};
        // 0 = unknown, 1 = avx512 supported, 2 = not supported.
        static CAP: AtomicU8 = AtomicU8::new(0);
        match CAP.load(Ordering::Relaxed) {
            1 => true,
            2 => false,
            _ => {
                let yes = has_avx2_fma()
                    && is_x86_feature_detected!("avx512f")
                    && is_x86_feature_detected!("avx512bw")
                    && is_x86_feature_detected!("avx512dq")
                    && is_x86_feature_detected!("avx512vl");
                CAP.store(if yes { 1 } else { 2 }, Ordering::Relaxed);
                yes
            }
        }
    }
}
