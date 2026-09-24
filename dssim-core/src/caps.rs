//! Runtime CPU-feature detection shared by the dynamically-dispatched
//! kernels (`tolab` SIMD rows, the 3-channel SSIM map, the 5-tap blur
//! interiors). Each check is resolved once via `is_*_feature_detected!`
//! and cached in an `AtomicU8`. When the features are statically enabled
//! at build time (`-C target-feature=+avx2,+fma`,
//! `-C target-cpu=native`, or aarch64's baseline NEON) the check compiles
//! down to `true` and dispatch is free.

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

/// Whether the CPU supports NEON. Resolved once, then cached.
#[cfg(target_arch = "aarch64")]
#[inline]
pub(crate) fn has_neon() -> bool {
    // Statically enabled features need neither runtime detection nor a cache.
    #[cfg(target_feature = "neon")]
    {
        true
    }
    #[cfg(not(target_feature = "neon"))]
    {
        use std::sync::atomic::{AtomicU8, Ordering};
        // 0 = unknown, 1 = neon supported, 2 = not supported.
        static CAP: AtomicU8 = AtomicU8::new(0);
        match CAP.load(Ordering::Relaxed) {
            1 => true,
            2 => false,
            _ => {
                let yes = std::arch::is_aarch64_feature_detected!("neon");
                CAP.store(if yes { 1 } else { 2 }, Ordering::Relaxed);
                yes
            }
        }
    }
}
