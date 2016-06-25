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
extern crate dssim;

mod ffi;
mod val;

use std::io::Write;
use std::env;
use std::io;
use getopts::Options;
use dssim::RGBAPLU;

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

fn to_rgbaplu(bitmap: &[lodepng::RGBA<u8>]) -> Vec<RGBAPLU> {
    let mut gamma_lut = [0f32; 256];

    for i in 0..256 {
        let s: f64 = i as f64 / 255.0;
        gamma_lut[i] = if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        } as f32
    }

    bitmap.iter().map(|px|{
        let a_unit = px.a as f32 / 255.0;
        RGBAPLU {
            r: gamma_lut[px.r as usize] * a_unit,
            g: gamma_lut[px.g as usize] * a_unit,
            b: gamma_lut[px.b as usize] * a_unit,
            a: a_unit,
        }
    }).collect()
}

fn load_image(path: &str) -> Result<(Vec<RGBAPLU>, usize, usize), lodepng::Error> {
    let image = try!(lodepng::decode32_file(path));

    let orig_rgba = to_rgbaplu(image.buffer.as_ref());
    Ok((orig_rgba, image.width, image.height))
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optopt("o", "", "set output file name", "NAME");
    opts.optflag("h", "help", "print this help menu");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => {m}
        Err(err) => {
            writeln!(io::stderr(), "{}", err).unwrap();
            std::process::exit(1);
        }
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

    let (orig_rgba, width1, height1) = match load_image(&file1) {
        Ok((orig_rgba, width, height)) => {(orig_rgba, width, height)}
        Err(err) => {
            writeln!(io::stderr(), "Can't read {}: {}", file1, err).unwrap();
            std::process::exit(1);
        }
    };

    let mut attr = dssim::Dssim::new();
    let original = attr.create_image(&orig_rgba, width1, height1).expect("orig image creation");

    for file2 in files {

        let (mod_rgba, width2, height2) = match load_image(&file2) {
            Ok((mod_rgba, width2, height2)) => {(mod_rgba, width2, height2)}
            Err(err) => {
                writeln!(io::stderr(), "Can't read {}: {}", file2, err).unwrap();
                std::process::exit(1);
            }
        };

        if width1 != width2 || height1 != height2 {
            writeln!(io::stderr(), "Image {} has different size than {}\n", file2, file1).unwrap();
            std::process::exit(1);
        }

        let modified = attr.create_image(&mod_rgba, width2, height2).expect("mod image creation");

        if map_output_file.is_some() {
            attr.set_save_ssim_maps(1, 1);
        }

        let dssim = attr.compare(&original, modified);

        println!("{:.6}\t{}", dssim, file2);

        if map_output_file.is_some() {
            let map_meta = attr.ssim_map(0, 0).expect("should give ssimmap");
            let avgssim = map_meta.dssim as f32;
            let out: Vec<_> = map_meta.data().expect("map should have data").iter().map(|ssim|{
                let max = 1_f32 - ssim;
                let maxsq = max * max;
                return lodepng::RGBA::<u8> {
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
