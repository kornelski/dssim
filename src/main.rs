/*
 * © 2011-2022 Kornel Lesiński. All rights reserved.
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
#![allow(clippy::manual_range_contains)]
use getopts::Options;
#[cfg(feature = "threads")]
use rayon::prelude::*;
use std::env;
use std::path::PathBuf;

fn usage(argv0: &str) {
    eprintln!("\
       Usage: {argv0} original.png modified.png [modified.png...]\
     \n   or: {argv0} -o difference.png original.png modified.png\n\n\
       Compares first image against subsequent images, and outputs\n\
       1/SSIM-1 difference for each of them in order (0 = identical).\n\n\
       Images must have identical size, but may have different gamma & depth.\n\
       \nVersion {} https://kornel.ski/dssim\n", env!("CARGO_PKG_VERSION"));
}

fn to_byte(i: f32) -> u8 {
    if i <= 0.0 {0}
    else if i >= 255.0/256.0 {255}
    else {(i * 256.0) as u8}
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        if let Some(s) = e.source() {
            eprintln!("  {s}");
        }
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args();
    let program = args.next().unwrap_or_default();

    let mut opts = Options::new();
    opts.optopt("o", "", "set output file name", "NAME");
    opts.optflag("h", "help", "print this help menu");
    let matches = opts.parse(args)?;

    if matches.opt_present("h") {
        usage(&program);
        return Ok(());
    }

    let map_output_file_tmp = matches.opt_str("o");
    let map_output_file = map_output_file_tmp.as_ref();
    let files: Vec<PathBuf> = matches.free.iter().map(|p| p.into()).collect();

    if files.len() < 2 {
        usage(&program);
        return Err("You must specify at least 2 files to compare".into());
    }

    let mut attr = dssim::Dssim::new();

    #[cfg(feature = "threads")]
    let files_iter = files.par_iter();
    #[cfg(not(feature = "threads"))]
    let files_iter = files.iter();

    let mut files = files_iter.map(|file| -> Result<_, String> {
        let image = dssim::load_image(&attr, file).map_err(|e| format!("Can't load {}, because: {e}", file.display()))?;
        Ok((file, image))
    }).collect::<Result<Vec<_>,_>>()?;

    let (file1, original) = files.remove(0);

    for (file2, modified) in files {
        if original.width() != modified.width() || original.height() != modified.height() {
            eprintln!("Image {} has a different size ({}x{}) than {} ({}x{})\n",
                file2.display(), modified.width(), modified.height(),
                file1.display(), original.width(), original.height());
            std::process::exit(1);
        }

        if map_output_file.is_some() {
            attr.set_save_ssim_maps(8);
        }

        let (dssim, ssim_maps) = attr.compare(&original, modified);

        println!("{dssim:.8}\t{}", file2.display());

        if let Some(map_output_file) = map_output_file {
            #[cfg(feature = "threads")]
            let ssim_maps_iter = ssim_maps.par_iter();
            #[cfg(not(feature = "threads"))]
            let ssim_maps_iter = ssim_maps.iter();

            ssim_maps_iter.enumerate().try_for_each(|(n, map_meta)| {
                let avgssim = map_meta.ssim as f32;
                let out: Vec<_> = map_meta.map.pixels().map(|ssim|{
                    let max = 1_f32 - ssim;
                    let maxsq = max * max;
                    rgb::RGBA8 {
                        r: to_byte(maxsq * 16.0),
                        g: to_byte(max * 3.0),
                        b: to_byte(max / ((1_f32 - avgssim) * 4_f32)),
                        a: 255,
                    }
                }).collect();
                lodepng::encode32_file(format!("{map_output_file}-{n}.png"), &out, map_meta.map.width(), map_meta.map.height())
                    .map_err(|e| {
                        format!("Can't write {map_output_file}: {e}")
                    })
            })?;
        }
    }
    Ok(())
}

#[test]
fn image_gray() {
    let attr = dssim::Dssim::new();

    let g1 = dssim::load_image(&attr, "tests/gray1-rgba.png").unwrap();
    let g2 = dssim::load_image(&attr, "tests/gray1-pal.png").unwrap();
    let g3 = dssim::load_image(&attr, "tests/gray1-gray.png").unwrap();
    let g4 = dssim::load_image(&attr, "tests/gray1.jpg").unwrap();

    let (diff, _) = attr.compare(&g1, g2);
    assert!(diff < 0.00001);

    let (diff, _) = attr.compare(&g1, g3);
    assert!(diff < 0.00001);

    let (diff, _) = attr.compare(&g1, g4);
    assert!(diff < 0.00006);
}

#[test]
fn image_gray_profile() {
    let attr = dssim::Dssim::new();

    let gp1 = dssim::load_image(&attr, "tests/gray-profile.png").unwrap();
    let gp2 = dssim::load_image(&attr, "tests/gray-profile2.png").unwrap();
    let gp3 = dssim::load_image(&attr, "tests/gray-profile.jpg").unwrap();

    let (diff, _) = attr.compare(&gp1, gp2);
    assert!(diff < 0.0003, "{}", diff);

    let (diff, _) = attr.compare(&gp1, gp3);
    assert!(diff < 0.0003, "{}", diff);
}

#[test]
fn image_load1() {
    let attr = dssim::Dssim::new();
    let prof_jpg = dssim::load_image(&attr, "tests/profile.jpg").unwrap();
    let prof_png = dssim::load_image(&attr, "tests/profile.png").unwrap();
    let (diff, _) = attr.compare(&prof_jpg, prof_png);
    assert!(diff <= 0.002);

    let strip_jpg = dssim::load_image(&attr, "tests/profile-stripped.jpg").unwrap();
    let (diff, _) = attr.compare(&strip_jpg, prof_jpg);
    assert!(diff > 0.008, "{}", diff);

    let strip_png = dssim::load_image(&attr, "tests/profile-stripped.png").unwrap();
    let (diff, _) = attr.compare(&strip_jpg, strip_png);
    assert!(diff > 0.009, "{}", diff);
}

#[test]
fn rgblu_input() {
    use dssim::{Dssim, RGBLU};
    use imgref::{Img, ImgRef, ImgVec};

    let ctx = Dssim::new();
    let im: ImgVec<RGBLU> = Img::new(vec![rgb::RGB::new(0.,0.,0.)], 1, 1);
    let imr: ImgRef<'_, RGBLU> = im.as_ref();
    ctx.create_image(&imr);
}
