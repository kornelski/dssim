/*
 * © 2011-2015 Kornel Lesiński. All rights reserved.
 *
 * This file is part of DSSIM.
 *
 * DSSIM is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License
 * as published by the Free Software Foundation, either version 3
 * of the License, or (at your option) any later version.
 *
 * DSSIM is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the license along with DSSIM.
 * If not, see <http://www.gnu.org/licenses/agpl.txt>.
 */

extern crate getopts;
extern crate libc;
extern crate lodepng;
extern crate lcms2;
extern crate mozjpeg;
extern crate dssim;
extern crate rgb;
extern crate file;

mod ffi;
mod val;
mod image;

use std::io::Write;
use std::env;
use std::io;
use std::io::Read;
use getopts::Options;
use dssim::RGBAPLU;
use dssim::ToRGBAPLU;
use dssim::Bitmap;
use lcms2::*;
use rgb::*;

fn usage(argv0: &str) {
    write!(io::stderr(), "\
       Usage: {} original.png modified.png [modified.png...]\
     \n   or: {} -o difference.png original.png modified.png\n\n\
       Compares first image against subsequent images, and outputs\n\
       1/SSIM-1 difference for each of them in order (0 = identical).\n\n\
       Images must have identical size, but may have different gamma & depth.\n\
       \nVersion 2.0.0 http://pornel.net/dssim\n", argv0, argv0).unwrap();
}

fn to_byte(i: f32) -> u8 {
    if i <= 0.0 {0}
    else if i >= 255.0/256.0 {255}
    else {(i * 256.0) as u8}
}

trait LcmsPixelFormat {
    fn pixel_format() -> (PixelFormat, ColorSpaceSignature);
}

impl LcmsPixelFormat for RGB8 { fn pixel_format() -> (PixelFormat, ColorSpaceSignature) { (PixelFormat::RGB_8, ColorSpaceSignature::SigRgbData) } }
impl LcmsPixelFormat for RGB16 { fn pixel_format() -> (PixelFormat, ColorSpaceSignature) { (PixelFormat::RGB_16, ColorSpaceSignature::SigRgbData) } }
impl LcmsPixelFormat for RGBA8 { fn pixel_format() -> (PixelFormat, ColorSpaceSignature) { (PixelFormat::RGBA_8, ColorSpaceSignature::SigRgbData) } }
impl LcmsPixelFormat for RGBA16 { fn pixel_format() -> (PixelFormat, ColorSpaceSignature) { (PixelFormat::RGBA_16, ColorSpaceSignature::SigRgbData) } }
impl LcmsPixelFormat for lodepng::Grey<u8> { fn pixel_format() -> (PixelFormat, ColorSpaceSignature) { (PixelFormat::GRAY_8, ColorSpaceSignature::SigGrayData) } }
impl LcmsPixelFormat for lodepng::Grey<u16> { fn pixel_format() -> (PixelFormat, ColorSpaceSignature) { (PixelFormat::GRAY_16, ColorSpaceSignature::SigGrayData) } }
impl LcmsPixelFormat for lodepng::GreyAlpha<u8> { fn pixel_format() -> (PixelFormat, ColorSpaceSignature) { (PixelFormat::GRAYA_8, ColorSpaceSignature::SigGrayData) } }
impl LcmsPixelFormat for lodepng::GreyAlpha<u16> { fn pixel_format() -> (PixelFormat, ColorSpaceSignature) { (PixelFormat::GRAYA_16, ColorSpaceSignature::SigGrayData) } }

trait ToSRGB {
    fn to_srgb(&mut self, profile: Option<Profile>) -> Vec<RGBAPLU>;
}

impl<T> ToSRGB for [T] where T: Copy + LcmsPixelFormat, [T]: ToRGBAPLU {
    fn to_srgb(&mut self, profile: Option<Profile>) -> Vec<RGBAPLU> {
        let (format, color_space) = T::pixel_format();
        if let Some(profile) = profile {
            if profile.color_space() == color_space {
                if PixelFormat::RGB_8 == format {
                    let t = Transform::new(&profile, PixelFormat::RGB_8,
                                           &Profile::new_srgb(), PixelFormat::RGB_8, Intent::RelativeColorimetric);
                    t.transform_in_place(self);
                    return self.to_rgbaplu();
                } else {
                    let t = Transform::new(&profile, format,
                                           &Profile::new_srgb(), PixelFormat::RGB_8, Intent::RelativeColorimetric);
                    let mut dest = vec![RGB8::new(0,0,0); self.len()];
                    t.transform_pixels(self, &mut dest);
                    return dest.to_rgbaplu();
                }
            }
        }
        return self.to_rgbaplu();
    }
}

fn load_png(mut state: lodepng::State, res: lodepng::Image) -> Result<Bitmap<RGBAPLU>, lodepng::Error> {

    let profile = if state.info_png().get("sRGB").is_some() {
        None
    } else if let Ok(iccp) = state.get_icc() {
        Profile::new_icc(iccp.as_ref())
    } else {
        None
    };

    match res {
        lodepng::Image::RGBA(mut image) => Ok(Bitmap::new(image.buffer.as_mut().to_srgb(profile), image.width, image.height)),
        lodepng::Image::RGB(mut image) => Ok(Bitmap::new(image.buffer.as_mut().to_srgb(profile), image.width, image.height)),
        lodepng::Image::RGB16(mut image) => Ok(Bitmap::new(image.buffer.as_mut().to_srgb(profile), image.width, image.height)),
        lodepng::Image::RGBA16(mut image) => Ok(Bitmap::new(image.buffer.as_mut().to_srgb(profile), image.width, image.height)),
        lodepng::Image::Grey(mut image) => {
            Ok(Bitmap::new(image.buffer.as_mut().to_srgb(profile), image.width, image.height))
        },
        lodepng::Image::Grey16(mut image) => {
            Ok(Bitmap::new(image.buffer.as_mut().to_srgb(profile), image.width, image.height))
        },
        lodepng::Image::GreyAlpha(mut image) => {
            Ok(Bitmap::new(image.buffer.as_mut().to_srgb(profile), image.width, image.height))
        },
        lodepng::Image::GreyAlpha16(mut image) => {
            Ok(Bitmap::new(image.buffer.as_mut().to_srgb(profile), image.width, image.height))
        },
        lodepng::Image::RawData(image) => {
            let mut png = state.info_raw_mut();
            if png.colortype() == lodepng::LCT_PALETTE {
                let pal_own = png.palette_mut().to_srgb(profile);
                let pal = &pal_own;

                return match png.bitdepth as u8 {
                    8 => Ok(Bitmap::new(image.buffer.as_ref().iter().map(|&c| pal[c as usize]).collect(), image.width, image.height)),
                    depth @ 1 | depth @ 2 | depth @ 4 => {
                        let pixels = 8/depth;
                        let mask = depth - 1;
                        return Ok(Bitmap::new(image.buffer.as_ref().iter().flat_map(|c| {
                            (0..pixels).rev().map(move |n|{
                                pal[(c >> (n*depth) & mask) as usize]
                            })
                        })
                        .take(image.width*image.height).collect(), image.width, image.height));
                    },
                    _ => Err(lodepng::Error(59)),
                };
            }
            return Err(lodepng::Error(59));
        },
    }
}

fn load_image(path: &str) -> Result<Bitmap<RGBAPLU>, lodepng::Error> {
    let data = match path {
        "-" => {
            let mut data = Vec::new();
            try!(std::io::stdin().read_to_end(&mut data));
            data
        },
        path => {
            try!(file::get(path))
        },
    };

    let mut state = lodepng::State::new();
    state.color_convert(false);
    state.remember_unknown_chunks(true);

    match state.decode(&data) {
        Ok(img) => load_png(state, img),
        _ => {
            let mut dinfo = mozjpeg::Decompress::new();
            dinfo.set_mem_src(&data[..]);
            dinfo.save_marker(mozjpeg::Marker::APP(2));
            assert!(dinfo.read_header(true));
            assert!(dinfo.start_decompress());
            let width = dinfo.output_width();
            let height = dinfo.output_height();

            let profile = if let Some(marker) = dinfo.markers().next() {
                let data = marker.data;
                if "ICC_PROFILE\0".as_bytes() == &data[0..12] {
                    let icc = &data[14..];
                    Profile::new_icc(icc)
                } else {None}
            } else {None};

            match dinfo.out_color_space() {
                mozjpeg::ColorSpace::JCS_RGB => {
                    let mut rgb:Vec<RGB8> = dinfo.read_scanlines().unwrap();
                    let rgba = rgb.to_srgb(profile);
                    assert_eq!(rgba.len(), width * height);
                    Ok(Bitmap::new(rgba, width, height))
                },
                mozjpeg::ColorSpace::JCS_GRAYSCALE => {
                    let mut g:Vec<lodepng::Grey<u8>> = dinfo.read_scanlines().unwrap();
                    Ok(Bitmap::new(g.to_srgb(profile), width, height))
                },
                _ => Err(lodepng::Error(59)),
            }
        },
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optopt("o", "", "set output file name", "NAME");
    opts.optflag("h", "help", "print this help menu");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(err) => {
            writeln!(io::stderr(), "{}", err).unwrap();
            std::process::exit(1);
        },
    };

    if matches.opt_present("h") {
        usage(&program);
        return;
    }

    let map_output_file_tmp = matches.opt_str("o");
    let map_output_file = map_output_file_tmp.as_ref();
    let mut files = matches.free;

    if files.len() < 2 {
        writeln!(io::stderr(), "You must specify at least 2 files to compare").unwrap();
        std::process::exit(1);
    }

    let file1 = files.remove(0);

    let orig_rgba = match load_image(&file1) {
        Ok(image) => image,
        Err(err) => {
            writeln!(io::stderr(), "Can't read {}: {}", file1, err).unwrap();
            std::process::exit(1);
        },
    };

    let mut attr = dssim::Dssim::new();
    let original = attr.create_image(&orig_rgba).expect("orig image creation");

    for file2 in files {

        let mod_rgba = match load_image(&file2) {
            Ok(image) => image,
            Err(err) => {
                writeln!(io::stderr(), "Can't read {}: {}", file2, err).unwrap();
                std::process::exit(1);
            },
        };

        // if width1 != width2 || height1 != height2 {
        //     writeln!(io::stderr(), "Image {} has different size than {}\n", file2, file1).unwrap();
        //     std::process::exit(1);
        // }

        let modified = attr.create_image(&mod_rgba).expect("mod image creation");

        if map_output_file.is_some() {
            attr.set_save_ssim_maps(1);
        }

        let dssim = attr.compare(&original, modified);

        println!("{:.6}\t{}", dssim, file2);

        if map_output_file.is_some() {
            let map_meta = attr.ssim_map(0).expect("should give ssimmap");
            let avgssim = map_meta.dssim as f32;
            let out: Vec<_> = map_meta.data().expect("map should have data").iter().map(|ssim|{
                let max = 1_f32 - ssim;
                let maxsq = max * max;
                return RGBA::<u8> {
                    r: to_byte(max * 3.0),
                    g: to_byte(maxsq * 6.0),
                    b: to_byte(max / ((1_f32 - avgssim) * 4_f32)),
                    a: 255,
                };
            }).collect();
            let write_res = lodepng::encode32_file(map_output_file.unwrap(), &out, map_meta.width as usize, map_meta.height as usize);
            if write_res.is_err() {
                writeln!(io::stderr(), "Can't write {}: {:?}", map_output_file.unwrap(), write_res).ok();
                std::process::exit(1);
            }
        }
    }
}

#[test]
fn image_gray() {
    let mut attr = dssim::Dssim::new();

    let g1 = attr.create_image(&load_image("tests/gray1-rgba.png").unwrap()).unwrap();
    let g2 = attr.create_image(&load_image("tests/gray1-pal.png").unwrap()).unwrap();
    let g3 = attr.create_image(&load_image("tests/gray1-gray.png").unwrap()).unwrap();
    let g4 = attr.create_image(&load_image("tests/gray1.jpg").unwrap()).unwrap();

    let diff = attr.compare(&g1, g2);
    assert!(diff < 0.00001);

    let diff = attr.compare(&g1, g3);
    assert!(diff < 0.00001);

    let diff = attr.compare(&g1, g4);
    assert!(diff < 0.00006);
}

#[test]
fn image_gray_profile() {
    let mut attr = dssim::Dssim::new();

    let gp1 = attr.create_image(&load_image("tests/gray-profile.png").unwrap()).unwrap();
    let gp2 = attr.create_image(&load_image("tests/gray-profile2.png").unwrap()).unwrap();
    let gp3 = attr.create_image(&load_image("tests/gray-profile.jpg").unwrap()).unwrap();

    let diff = attr.compare(&gp1, gp2);
    assert!(diff < 0.0003, "{}", diff);

    let diff = attr.compare(&gp1, gp3);
    assert!(diff < 0.0003, "{}", diff);
}

#[test]
fn image_load1() {

    let mut attr = dssim::Dssim::new();
    let prof_jpg = attr.create_image(&load_image("tests/profile.jpg").unwrap()).unwrap();
    let prof_png = attr.create_image(&load_image("tests/profile.png").unwrap()).unwrap();
    let diff = attr.compare(&prof_jpg, prof_png);
    assert!(diff <= 0.002);

    let strip_jpg = attr.create_image(&load_image("tests/profile-stripped.jpg").unwrap()).unwrap();
    let diff = attr.compare(&strip_jpg, prof_jpg);
    assert!(diff > 0.013);

    let strip_png = attr.create_image(&load_image("tests/profile-stripped.png").unwrap()).unwrap();
    let diff = attr.compare(&strip_jpg, strip_png);
    assert!(diff > 0.014);
}
