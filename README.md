# RGBA Structural Similarity

This tool computes (dis)similarity between two (or more) PNG images using algorithm approximating human vision.

Comparison is done using [the SSIM algorithm](https://ece.uwaterloo.ca/~z70wang/research/ssim/) (based on [Rabah Mehdi's implementation](http://mehdi.rabah.free.fr/SSIM/)) at multiple weighed resolutions.

The value returned is 1/SSIM-1, where 0 means identical image, and >0 (unbounded) is amount of difference. Values are not directly comparable with other tools.

## Features

* Comparison is done in in L\*a\*b\* color space (D65 white point, gamma 2.2) with chroma subsampling. Other implementations use uncorrected sRGB or grayscale.
* Supports alpha channel.
* Only needs C (C99) and `libpng` or Cocoa on OS X. No OpenCV or MATLAB needed.

## Usage

    dssim file-original.png file-modified.png

Will output something like "0.02341" (smaller is better) followed by a filename.

You can supply multiple filenames to compare them all with the first file:

    dssim file.png modified1.png modified2.png modified3.png

You can save an image visualising the difference between the files:

    dssim -o difference.png file.png file-modified.png

The `dssim.c` file is also usable as a C library.

## Build or Download

You need libpng, zlib, pkg-config and make

    make

Will give you `dssim`. On OS X `make USE_COCOA=1` will compile without libpng.

You'll find [downloads on GitHub releases page](https://github.com/pornel/dssim/releases).

## Accuracy

Scores for version 0.9 measured against [TID2008][1] database:

TID2008 Category | Spearman correlation
--- | ---
Noise   | -0.866
Noise2  | -0.882
Safe    | -0.884
Hard    | -0.903
Simple  | -0.920
Exotic  | -0.441
Exotic2 | -0.619
Full    | -0.804

[1]: http://www.computervisiononline.com/dataset/tid2008-tampere-image-database-2008
