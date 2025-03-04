#![feature(test)]

extern crate test;
use dssim::{ToRGBAPLU, RGBAPLU};
use imgref::{Img, ImgVec};
use test::Bencher;

fn load(path: &str) -> Result<ImgVec<RGBAPLU>, lodepng::Error> {
    let image = lodepng::decode32_file(path)?;
    Ok(Img::new(image.buffer.to_rgbaplu(), image.width, image.height))
}

#[bench]
fn compare(bench: &mut Bencher) {
    let attr = dssim::Dssim::new();
    let other = &load("tests/test1-sm.png").unwrap();
    let orig = attr.create_image(&load("tests/test2-sm.png").unwrap()).unwrap();
    let modif = attr.create_image(other).unwrap();

    bench.iter(|| {
        attr.compare(&orig, modif.clone())
    });
}

#[bench]
fn create_image(bench: &mut Bencher) {
    let attr = dssim::Dssim::new();
    let img = &load("tests/test1-sm.png").unwrap();

    bench.iter(|| {
        attr.create_image(img)
    });
}
