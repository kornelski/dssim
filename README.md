# RGBA Structural Similarity

This tool computes (dis)similarity between two (or more) PNG images using algorithm approximating human vision.

Comparison is done in L\*a\*b\* color space (D65 white point, gamma 2.2) with chroma subsampling,
using [the SSIM algorithm](https://ece.uwaterloo.ca/~z70wang/research/ssim/).

The value returned is 1/SSIM-1, where 0 means identical image, and >0 (unbounded) is amount of difference. Values are not directly comparable with other tools.

It's a rewrite of [Rabah Mehdi's C++ implementation](http://mehdi.rabah.free.fr/SSIM/):

* No C++ (C99)
* No OpenCV dependency (only `libpng` or Cocoa on OS X)
* Supports alpha channel
* Supports gamma correction

## Usage

    dssim file-original.png file-modified.png

Will output something like `0.2341` (smaller is better) followed by a filename.

You can supply multiple filenames to compare them all with the first file:

    dssim file.png modified1.png modified2.png modified3.png

You can save an image visualising the difference between the files:

    dssim -o difference.png file.png file-modified.png

The `dssim.c` file is also usable as a C library.

## Build or Download

You need libpng, zlib, pkg-config and make

    make

Will give you dssim. On OS X `make USE_COCOA=1` will compile without libpng.

You'll find [downloads on GitHub releases page](https://github.com/pornel/dssim/releases).

## Accuracy

Scores for version 0.8 measured against [TID2008][1] database:

TID2008 Category | Spearman correlation
--- | ---
Noise   | -0.702
Noise2  | -0.747
Safe    | -0.733
Hard    | -0.871
Simple  | -0.852
Exotic  | -0.444
Exotic2 | -0.635
Full    | -0.729

[1]: http://www.computervisiononline.com/dataset/tid2008-tampere-image-database-2008
