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

#include <stdlib.h>
#include <stdbool.h>
#include <math.h>
#include <assert.h>
#include "dssim.h"

#ifdef USE_COCOA
#import <Accelerate/Accelerate.h>
#endif

#ifndef MIN
#define MIN(a,b) ((a)<=(b)?(a):(b))
#endif
#ifndef MAX
#define MAX(a,b) ((a)>=(b)?(a):(b))
#endif

#define MAX_SCALES 5

typedef struct {
    dssim_px_t l, A, b;
} dssim_lab;

typedef struct {
    unsigned char r, g, b;
} dssim_rgb;

typedef struct {
    dssim_px_t r, g, b, a; // premultiplied
} linear_rgba;

struct dssim_chan;
typedef struct dssim_chan dssim_chan;

int dssim_get_subsample_chroma(dssim_attr *attr);
double dssim_get_color_weight(dssim_attr *attr);
double dssim_get_scale_weights(dssim_attr *attr, unsigned int i);
void dssim_image_set_channels(const dssim_attr *attr, dssim_image *, int, int, int, int);
int dssim_image_get_num_channels(dssim_image *);
int dssim_get_chan_width(const dssim_chan *);
int dssim_get_chan_height(const dssim_chan *);
float *dssim_get_chan_img(dssim_chan *);
const float *dssim_get_chan_img_const(const dssim_chan *);
const float *dssim_get_chan_img_sq_blur_const(const dssim_chan *);
float *dssim_get_chan_img_sq_blur(dssim_chan *);
const float *dssim_get_chan_mu_const(const dssim_chan *);
int dssim_image_get_num_channel_scales(dssim_image *, int);
dssim_chan *dssim_image_get_channel(dssim_image *, int, int);

dssim_px_t *dssim_get_tmp(dssim_attr *attr, size_t size);

static int set_gamma(dssim_px_t gamma_lut[static 256], const double invgamma)
{
    if (invgamma == dssim_srgb_gamma) {
        for (int i = 0; i < 256; i++) {
            const double s = i / 255.0;
            if (s <= 0.04045) {
                gamma_lut[i] = s / 12.92;
            } else {
                gamma_lut[i] = pow((s + 0.055) / 1.055, 2.4);
            }
        }
        return 1;
    } else if (invgamma > 0 && invgamma < 1.0) {
        for (int i = 0; i < 256; i++) {
            gamma_lut[i] = pow(i / 255.0, 1.0 / invgamma);
        }
        return 1;
    } else {
        return 0;
    }
}

static const double D65x = 0.9505, D65y = 1.0, D65z = 1.089;

inline static linear_rgba rgb_to_linear(const dssim_px_t gamma_lut[static 256], const unsigned char pxr, const unsigned char pxg, const unsigned char pxb, const unsigned char pxa) {
    const dssim_px_t r = gamma_lut[pxr],
                     g = gamma_lut[pxg],
                     b = gamma_lut[pxb],
                     a = pxa / 255.0;

    return (linear_rgba){
        .r = r * a,
        .g = g * a,
        .b = b * a,
        .a = a,
    };
}

inline static dssim_lab rgb_to_lab(const dssim_px_t r, const dssim_px_t g, const dssim_px_t b)
{
    const double fx = (r * 0.4124 + g * 0.3576 + b * 0.1805) / D65x;
    const double fy = (r * 0.2126 + g * 0.7152 + b * 0.0722) / D65y;
    const double fz = (r * 0.0193 + g * 0.1192 + b * 0.9505) / D65z;

    const double epsilon = 216.0 / 24389.0;
    const double k = (24389.0 / 27.0) / 116.f; // http://www.brucelindbloom.com/LContinuity.html
    const dssim_px_t X = (fx > epsilon) ? powf(fx, 1.f / 3.f) - 16.f/116.f : k * fx;
    const dssim_px_t Y = (fy > epsilon) ? powf(fy, 1.f / 3.f) - 16.f/116.f : k * fy;
    const dssim_px_t Z = (fz > epsilon) ? powf(fz, 1.f / 3.f) - 16.f/116.f : k * fz;

    return (dssim_lab) {
        Y * 1.16f,
        (86.2f/ 220.0f + 500.0f/ 220.0f * (X - Y)), /* 86 is a fudge to make the value positive */
        (107.9f/ 220.0f + 200.0f/ 220.0f * (Y - Z)), /* 107 is a fudge to make the value positive */
    };
}

#ifndef USE_COCOA
/*
 * Flips x/y (like 90deg rotation)
 */
static void transpose(dssim_px_t *restrict src, dssim_px_t *restrict dst, const int width, const int height)
{
    int j = 0;
    for (; j < height-4; j+=4) {
        dssim_px_t *restrict row0 = src + (j+0) * width;
        dssim_px_t *restrict row1 = src + (j+1) * width;
        dssim_px_t *restrict row2 = src + (j+2) * width;
        dssim_px_t *restrict row3 = src + (j+3) * width;
        for(int i=0; i < width; i++) {
            dst[i*height + j+0] = row0[i];
            dst[i*height + j+1] = row1[i];
            dst[i*height + j+2] = row2[i];
            dst[i*height + j+3] = row3[i];
        }
    }

    for (; j < height; j++) {
        dssim_px_t *restrict row = src + j * width;
        for(int i=0; i < width; i++) {
            dst[i*height + j] = row[i];
        }
    }
}

static void regular_1d_blur(const dssim_px_t *src, dssim_px_t *restrict tmp1, dssim_px_t *dst, const int width, const int height)
{
    const int runs = 2;

    // tmp1 is expected to hold at least two lines
    dssim_px_t *restrict tmp2 = tmp1 + width;

    for(int j=0; j < height; j++) {
        for(int run = 0; run < runs; run++) {
            // To improve locality blur is done on tmp1->tmp2 and tmp2->tmp1 buffers,
            // except first and last run which use src->tmp and tmp->dst
            const dssim_px_t *restrict row = (run == 0   ? src + j*width : (run & 1) ? tmp1 : tmp2);
            dssim_px_t *restrict dstrow = (run == runs-1 ? dst + j*width : (run & 1) ? tmp2 : tmp1);

            int i=0;
            for(; i < MIN(4, width); i++) {
                dstrow[i] = (row[MAX(0, i-1)] + row[i] + row[MIN(width-1, i+1)]) / 3.f;
            }

            const int end = (width-1) & ~3UL;
            for(; i < end; i+=4) {
                const dssim_px_t p1 = row[i-1];
                const dssim_px_t n0 = row[i+0];
                const dssim_px_t n1 = row[i+1];
                const dssim_px_t n2 = row[i+2];
                const dssim_px_t n3 = row[i+3];
                const dssim_px_t n4 = row[i+4];

                dstrow[i+0] = (p1 + n0 + n1) / 3.f;
                dstrow[i+1] = (n0 + n1 + n2) / 3.f;
                dstrow[i+2] = (n1 + n2 + n3) / 3.f;
                dstrow[i+3] = (n2 + n3 + n4) / 3.f;
            }

            for(; i < width; i++) {
                dstrow[i] = (row[MAX(0, i-1)] + row[i] + row[MIN(width-1, i+1)]) / 3.f;
            }
        }
    }
}
#endif

/*
 * blurs (approximate of gaussian)
 */
void blur(const dssim_px_t *src, dssim_px_t *restrict tmp, dssim_px_t *restrict dst,
                 const int width, const int height)
{
    assert(src);
    assert(dst);
    assert(tmp);
    assert(width > 2);
    assert(height > 2);
#ifdef USE_COCOA
    vImage_Buffer srcbuf = {
        .width = width,
        .height = height,
        .rowBytes = width * sizeof(dssim_px_t),
        .data = (void*)src,
    };
    vImage_Buffer dstbuf = {
        .width = width,
        .height = height,
        .rowBytes = width * sizeof(dssim_px_t),
        .data = dst,
    };
    vImage_Buffer tmpbuf = {
        .width = width,
        .height = height,
        .rowBytes = width * sizeof(dssim_px_t),
        .data = tmp,
    };

    dssim_px_t kernel[9] = {
        1/16.f, 1/8.f, 1/16.f,
        1/8.f,  1/4.f, 1/8.f,
        1/16.f, 1/8.f, 1/16.f,
    };

    vImageConvolve_PlanarF(&srcbuf, &tmpbuf, NULL, 0, 0, kernel, 3, 3, 0, kvImageEdgeExtend);
    vImageConvolve_PlanarF(&tmpbuf, &dstbuf, NULL, 0, 0, kernel, 3, 3, 0, kvImageEdgeExtend);
#else
    regular_1d_blur(src, tmp, dst, width, height);
    transpose(dst, tmp, width, height);

    // After transposing buffer is rotated, so height and width are swapped
    // And reuse of buffers made tmp hold the image, and dst used as temporary until the last transpose
    regular_1d_blur(tmp, dst, tmp, height, width);
    transpose(tmp, dst, height, width);
#endif
}


void blur_in_place(dssim_px_t *restrict srcdst, dssim_px_t *restrict tmp,
                 const int width, const int height) {
    assert((intptr_r)srcdst > 1);
    assert(tmp);
    blur(srcdst, tmp, srcdst, width, height);
}

/*
 * Conversion is not reversible
 */
inline static dssim_lab convert_pixel_rgba(linear_rgba px, int i, int j)
{
    // Compose image on coloured background to better judge dissimilarity with various backgrounds
    if (px.a < 255) {
        int n = i ^ j;
        if (n & 4) {
            px.r += 1.0 - px.a; // assumes premultiplied alpha
        }
        if (n & 8) {
            px.g += 1.0 - px.a;
        }
        if (n & 16) {
            px.b += 1.0 - px.a;
        }
    }

    dssim_lab f1 = rgb_to_lab(px.r, px.g, px.b);
    assert(f1.l >= 0.f && f1.l <= 1.0f);
    assert(f1.A >= 0.f && f1.A <= 1.0f);
    assert(f1.b >= 0.f && f1.b <= 1.0f);

    return f1;
}

