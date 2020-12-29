# RGBA Structural Similarity

This tool computes (dis)similarity between two or more PNG images using an algorithm approximating human vision.

Comparison is done using [the SSIM algorithm](https://ece.uwaterloo.ca/~z70wang/research/ssim/) at multiple weighed resolutions.

The value returned is 1/SSIM-1, where 0 means identical image, and >0 (unbounded) is amount of difference. Values are not directly comparable with other tools. [See below](#interpreting-the-values) on interpreting the values.

## Features

* Comparison is done in L\*a\*b\* color space (D65 white point, sRGB gamma). Other implementations use "RGB" or grayscale without gamma correction.
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

It's also usable [as a library](https://docs.rs/dssim).

Please be careful about color profiles in the images. Different profiles, or lack of support for profiles, can make images appear different even when the pixels are the same.

### Interpreting the values

The amount of difference goes from 0 to infinity. It's not a percentage.

If you're comparing two different image compression codecs, then ensure you either:

* compress images to the same file size, and then use DSSIM to compare which one is closests to the original, or
* compress images to the same DSSIM value, and compare file sizes to see how much file size gain each option gives.

[More about benchmarking image compression](https://kornel.ski/faircomparison).

When you quote results, please include DSSIM version, since the scale has changed between versions.
The version is printed when you run `dssim -h`.

## Build or Download

You need Rust 1.48 or later.

    cargo build --release

Will give you `./target/release/dssim`.

## Accuracy

Scores for version 2.11 [measured][2] against [TID2013][1] database:

TID2013  | Spearman | Kendall
---------|----------|--------
Noise    |  -0.9309 | -0.7615
Actual   |  -0.9380 | -0.7748
Simple   |  -0.9459 | -0.7927
Exotic   |  -0.8478 | -0.6549
New      |  -0.8127 | -0.6335
Color    |  -0.8259 | -0.6501
Full     |  -0.8650 | -0.6839

[1]: http://www.ponomarenko.info/tid2013.htm
[2]: https://lib.rs/crates/tid2013stats

## License

DSSIM is dual-licensed under [AGPL](LICENSE) or [commercial](https://supso.org/projects/dssim) license.

## The algorithm improvements in DSSIM

* The comparison is done on multiple weighed scales (based on IWSSIM) to measure features of different sizes. A single-scale SSIM is biased towards differences smaller than its gaussian kernel.
* Scaling is done in linear-light RGB to model physical effects of viewing distance/lenses. Scaling in sRGB or Lab would have incorrect gamma and mask distortions caused by chroma subsampling.
* ab channels of Lab are compared with lower spatial precision to simulate eyes' higher sensitivity to brightness than color changes.
* The lightness component of SSIM is ignored when comparing color channels.
* SSIM score is pooled using a combination of local maximums and global averages. You can get per-pixel SSIM from the API to implement custom pooling.

