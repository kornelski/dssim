//! Benchmarks for image loading and comparison.
//!
//! This uses a `harness = false` target rather than the built-in `test::Bencher`,
//! because `#![feature(test)]` only compiles on the nightly toolchain.

use dssim::{RGBAPLU, ToRGBAPLU};
use imgref::{Img, ImgVec};
use std::hint::black_box;
use std::time::{Duration, Instant};

fn load(path: &str) -> Result<ImgVec<RGBAPLU>, lodepng::Error> {
    let image = lodepng::decode32_file(path)?;
    Ok(Img::new(image.buffer.to_rgbaplu(), image.width, image.height))
}

/// Runs `f` repeatedly and prints the average wall time per iteration.
fn bench<T>(name: &str, mut f: impl FnMut() -> T) {
    // Warm up, and use the warmup to size the measured run.
    let warmup = Instant::now();
    let mut iters: u32 = 0;
    while warmup.elapsed() < Duration::from_millis(200) {
        black_box(f());
        iters += 1;
    }
    let iters = iters.max(1);

    let start = Instant::now();
    for _ in 0..iters {
        black_box(f());
    }
    let elapsed = start.elapsed();

    println!("{name}: {:?}/iter ({iters} iters)", elapsed / iters);
}

fn main() {
    let attr = dssim::Dssim::new();
    let img1 = load("tests/test1-sm.png").unwrap();
    let img2 = load("tests/test2-sm.png").unwrap();

    let orig = attr.create_image(&img2).unwrap();
    let modif = attr.create_image(&img1).unwrap();

    bench("compare", || attr.compare(&orig, modif.clone()));
    bench("create_image", || attr.create_image(&img1));
}
