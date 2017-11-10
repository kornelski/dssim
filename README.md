# RGBA Structural Similarity

This tool computes (dis)similarity between two or more PNG images using an algorithm approximating human vision.

Comparison is done using [the SSIM algorithm](https://ece.uwaterloo.ca/~z70wang/research/ssim/) at multiple weighed resolutions.

The value returned is 1/SSIM-1, where 0 means identical image, and >0 (unbounded) is amount of difference. Values are not directly comparable with other tools. [See below](#interpreting-the-values) on interpreting the values.

## Features

* Comparison is done in in L\*a\*b\* color space (D65 white point, sRGB gamma) with chroma subsampling. Other implementations use "RGB" or grayscale without gamma correction.
* Supports alpha channel.
* No OpenCV or MATLAB needed.
   - DSSIM [version 1.x](https://github.com/pornel/dssim/tree/dssim1-c) uses C (C99) and `libpng` or Cocoa on macOS.
   - DSSIM version 2.x is easy to build with [Rust](https://www.rust-lang.org/).

## Usage

    dssim file-original.png file-modified.png

Will output something like "0.02341" (smaller is better) followed by a filename.

You can supply multiple filenames to compare them all with the first file:

    dssim file.png modified1.png modified2.png modified3.png

You can save an image visualising the difference between the files:

    dssim -o difference.png file.png file-modified.png

The `dssim.c` file is also usable as a C library.

### Interpreting the values

The amount of difference goes from 0 to infinity. It's not a percentage.

If you're comparing two different image compression codecs, then ensure you either:

* compress images to the same file size, and then use DSSIM to compare which one is closests to the original, or
* compress images to the same DSSIM value, and compare file sizes to see how much file size gain each option gives.

[More about benchmarking image compression](https://kornel.ski/faircomparison).

When you quote results, please include DSSIM version, since the scale has changed between versions.
The version is printed when you run `dssim -h`.

## Build or Download

You need Rust

    cargo build --release

Will give you `./target/release/dssim`.

## Accuracy

Scores for version 2.0 measured against [TID2013][1] database:

TID2013 Category | Spearman correlation
--- | ---
Noise  | -0.930
Actual | -0.937
Simple | -0.945
Exotic | -0.842
New    | -0.771
Color  | -0.779
Full   | -0.851

[1]: http://www.ponomarenko.info/tid2013.htm

## License

DSSIM is dual-licensed under [AGPL](LICENSE) or [commercial](https://supportedsource.org/projects/dssim) license.
