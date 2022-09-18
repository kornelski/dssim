# RGBA Structural Similarity

This tool computes (dis)similarity between two or more PNG &/or JPEG images using an algorithm approximating human vision. Comparison is done using a variant of [the SSIM algorithm](https://ece.uwaterloo.ca/~z70wang/research/ssim/).

The value returned is 1/SSIM-1, where 0 means identical image, and >0 (unbounded) is amount of difference. Values are not directly comparable with other tools. [See below](#interpreting-the-values) on interpreting the values.

## Features

* Improved algorithm
    * Compares at multiple weighed resolutions, and scaling is done in linear-light RGB. It's sensitive to distortions of various sizes and blends colors correctly to detect e.g. chroma subsampling errors.
    * Uses L\*a\*b\* color space for the SSIM algorithm. It measures brightness and color much better than metrics from average of RGB channels.
* Supports alpha channel.
* Supports images with color profiles.
* Takes advantage of multi-core CPUs.
* Can be used as a library in C, Rust, and WASM.
* No OpenCV or MATLAB needed.

## Usage

    dssim file-original.png file-modified.png

Will output something like "0.02341" (smaller is better) followed by a filename.

You can supply multiple filenames to compare them all with the first file:

    dssim file.png modified1.png modified2.png modified3.png

You can save an image visualising the difference between the files:

    dssim -o difference.png file.png file-modified.png

It's also usable [as a library](https://docs.rs/dssim).

Please be mindful about color profiles in the images. Different profiles, or lack of support for profiles in other tools, can make images appear different even when the pixels are the same.

### Interpreting the values

The amount of difference goes from 0 to infinity. It's not a percentage.

If you're comparing two different image compression codecs, then ensure you either:

* compress images to the same file size, and then use DSSIM to compare which one is closests to the original, or
* compress images to the same DSSIM value, and compare file sizes to see how much file size gain each option gives.

[More about benchmarking image compression](https://kornel.ski/faircomparison).

When you quote results, please include the DSSIM version. The scale has changed between versions.
The version is printed when you run `dssim -h`.

## Download

[Download from releases page](https://github.com/kornelski/dssim/releases). It's also available in Mac Homebrew and Ubuntu Snaps.

### Build from source

You'll need [Rust 1.63](https://rustup.rs) or later. Clone the repo and run:

    rustup update
    cargo build --release

Will give you `./target/release/dssim`.

## Accuracy

Scores for version 3.2 [measured][2] against [TID2013][1] database:

TID2013  | Spearman | Kendall
---------|----------|--------
Noise    |  -0.9392 | -0.7789
Actual   |  -0.9448 | -0.7913
Simple   |  -0.9499 | -0.8082
Exotic   |  -0.8436 | -0.6574
New      |  -0.8717 | -0.6963
Color    |  -0.8789 | -0.7032
Full     |  -0.8711 | -0.6984

[1]: http://www.ponomarenko.info/tid2013.htm
[2]: https://lib.rs/crates/tid2013stats

## License

DSSIM is dual-licensed under [AGPL](LICENSE) or [commercial](https://supso.org/projects/dssim) license.

## The algorithm improvements in DSSIM

* The comparison is done on multiple weighed scales (based on IWSSIM) to measure features of different sizes. A single-scale SSIM is biased towards differences smaller than its gaussian kernel.
* Scaling is done in linear-light RGB to model physical effects of viewing distance/lenses. Scaling in sRGB or Lab would have incorrect gamma and mask distortions caused by chroma subsampling.
* a/b channels of Lab are compared with lower spatial precision to simulate eyes' higher sensitivity to brightness than color changes.
* SSIM score is pooled using mean absolute deviation. You can get per-pixel SSIM from the API to implement custom pooling.

## Compiling for WASM

For compatibility with single-threaded WASM runtimes, disable the `threads` Cargo feature. It's enabled by default, so to disable it, disable default features:

```toml
dssim-core = { version = "3.2", default-features = false }
```
