#RGBA Structural Similarity

This tool computes (dis)similarity between two (or more) PNG images using algorithm approximating human vision.

Comparison is done in L\*a\*b\* color space (D65 white point, gamma 2.2) with chroma subsampling, using the SSIM algorithm.

The value returned is equivalent to 1/SSIM-1, where 0 means identical image, and >0 (unbounded) is amount of difference. Values are not directly comparable with other tools.

It's a rewrite of [Rabah Mehdi's C++ implementation](http://mehdi.rabah.free.fr/SSIM/):

* No C++ (C99)
* No OpenCV dependency (only `libpng`)
* Supports alpha channel
* Supports gamma correction

##Usage

    dssim file.png file-modified.png

Will output something like `0.2341` (smaller is better) followed by a filename.

You can supply multiple filenames to compare them all with the first file:

    dssim file.png modified1.png modified2.png modified3.png

You can save an image visualising the difference between files to a file:

    dssim -o difference.png file.png file-modified.png

The `dssim.c` file is also usable as a C library.

##Build or Download

You need libpng, zlib, pkg-config and make

    make

Will give you dssim.

You'll find [downloads on GitHub releases page](https://github.com/pornel/dssim/releases).

