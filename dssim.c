/*
 * Copyright (c) 2011 porneL. All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions are met:
 *
 * 1. Redistributions of source code must retain the above copyright notice,
 * this list of conditions and the following disclaimer.
 *
 * 2. Redistributions in binary form must reproduce the above copyright
 * notice, this list of conditions and the following disclaimer in the
 * documentation and/or other materials
 * provided with the distribution.
 *
 * THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
 * WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
 * ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 * WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
 * ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
 * OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
 */
#include <stdlib.h>
#include <math.h>
#include <assert.h>
#include "dssim.h"

#ifndef MIN
#define MIN(a,b) ((a)<=(b)?(a):(b))
#endif

typedef struct {
    float l, A, b, a;
} laba;

struct dssim_info {
    int width, height;
    float *img1[3], *mu1[3], *sigma1_sq[3];
    float *mu2[3], *sigma2_sq[3], *sigma12[3];
};

dssim_info *dssim_init()
{
    return calloc(1, sizeof(dssim_info));
}

static void free3(float *channels[])
{
    free(channels[0]); channels[0] = NULL;
    free(channels[1]); channels[1] = NULL;
    free(channels[2]); channels[2] = NULL;
}

void dssim_dealloc(dssim_info *inf)
{
    free3(inf->mu2);
    free3(inf->sigma2_sq);
    free3(inf->sigma12);
    free3(inf->img1);
    free3(inf->mu1);
    free3(inf->sigma1_sq);
    free(inf);
}

static double gamma_lut[256];
static void set_gamma(const double invgamma)
{
    for (int i = 0; i < 256; i++) {
        gamma_lut[i] = pow(i / 255.0, 1.0 / invgamma);
    }
}

static const double D65x = 0.9505, D65y = 1.0, D65z = 1.089;

inline static laba rgba_to_laba(const rgba8 px)
{
    const double r = gamma_lut[px.r],
                 g = gamma_lut[px.g],
                 b = gamma_lut[px.b];
    const float  a = px.a / 255.f;

    double fx = (r * 0.4124 + g * 0.3576 + b * 0.1805) / D65x;
    double fy = (r * 0.2126 + g * 0.7152 + b * 0.0722) / D65y;
    double fz = (r * 0.0193 + g * 0.1192 + b * 0.9505) / D65z;

    const double epsilon = 216.0 / 24389.0;
    const double k = (24389.0 / 27.0) / 116.f; // http://www.brucelindbloom.com/LContinuity.html
    const float X = (fx > epsilon) ? powf(fx, 1.f / 3.f) - 16.f/116.f : k * fx;
    const float Y = (fy > epsilon) ? powf(fy, 1.f / 3.f) - 16.f/116.f : k * fy;
    const float Z = (fz > epsilon) ? powf(fz, 1.f / 3.f) - 16.f/116.f : k * fz;

    return (laba) {
        Y * 1.16f,
        (86.2f/ 220.0f + 500.0f/ 220.0f * (X - Y)), /* 86 is a fudge to make the value positive */
        (107.9f/ 220.0f + 200.0f/ 220.0f * (Y - Z)), /* 107 is a fudge to make the value positive */
        a
    };
}

#define LABA_OPC(dst, X, op, Y) dst = (X) op (Y)

typedef void rowcallback(float *, int width);

static void square_row(float *row, int width)
{
    for (int i = 0; i < width; i++) {
        row[i] = row[i] * row[i];
    }
}

/*
 * Blurs image horizontally (width 2*size+1) and writes it transposed to dst
 * (called twice gives 2d blur)
 * Callback is executed on every row before blurring
 */
static void transposing_1d_blur(float *restrict src, float *restrict dst, const int width, const int height)
{
    const int size = 3;
    const float sizef = size;

    for (int j = 0; j < height; j++) {
        float *restrict row = src + j * width;

        // accumulate sum for pixels outside line
        float sum = 0;
        LABA_OPC(sum,row[0],*,sizef);
        for(int i=0; i < MIN(width,size); i++) {
            sum += row[i];
        }

        // blur with left side outside line
        for(int i=0; i < MIN(width,size); i++) {
            sum -= row[0];
            if((i + size) < width){
                sum += row[i+size];
            }

            LABA_OPC(dst[i*height + j],sum,/,sizef*2.0f);
        }

        for(int i=size; i < width-size; i++) {
            sum -= row[i-size];
            sum += row[i+size];

            LABA_OPC(dst[i*height + j],sum,/,sizef*2.0f);
        }

        // blur with right side outside line
        for(int i=width-size; i < width; i++) {
            if(i-size >= 0){
                sum -= row[i-size];
            }
            sum += row[width-1];

            LABA_OPC(dst[i*height + j],sum,/,sizef*2.0f);
        }
    }
}

static void regular_1d_blur(float *restrict src, float *restrict dst, const int width, const int height, rowcallback *const callback)
{
    const int size = 1;
    const float sizef = size;

    for(int j=0; j < height; j++) {
        float *restrict row = src + j*width;
        float *restrict dstrow = dst + j*width;

        // preprocess line
        if (callback) callback(row,width);

        // accumulate sum for pixels outside line
        float sum = 0;
        LABA_OPC(sum, row[0], *, sizef);
        for(int i=0; i < MIN(width,size); i++) {
            sum += row[i];
        }

        // blur with left side outside line
        for(int i=0; i < MIN(width,size); i++) {
            sum -= row[0];
            if ((i + size) < width) {
                sum += row[i + size];
            }

            LABA_OPC(dstrow[i], sum, /, sizef * 2.0f);
        }

        for (int i = size; i < width - size; i++) {
            sum -= row[i - size];
            sum += row[i + size];

            LABA_OPC(dstrow[i], sum, /, sizef * 2.0f);
        }

        // blur with right side outside line
        for (int i = width - size; i < width; i++) {
            if (i - size >= 0) {
                sum -= row[i - size];
            }
            sum += row[width - 1];

            LABA_OPC(dstrow[i], sum, /, sizef * 2.0f);
        }
    }
}


/*
 * Filters image with callback and blurs (lousy approximate of gaussian)
 */
static void blur(float *restrict src, float *restrict tmp, float *restrict dst,
                 int width, int height, rowcallback *const callback)
{
    regular_1d_blur(src, tmp, width, height, callback);
    regular_1d_blur(tmp, dst, width, height, NULL);
    transposing_1d_blur(dst, tmp, width, height);
    regular_1d_blur(tmp, dst, height, width, NULL);
    regular_1d_blur(dst, tmp, height, width, NULL);
    transposing_1d_blur(tmp, dst, height, width);
}

/*
 * Conversion is not reversible
 */
inline static laba convert_pixel(rgba8 px, int i, int j)
{
    laba f1 = rgba_to_laba(px);
    assert(f1.l >= 0.f && f1.l <= 1.0f);
    assert(f1.A >= 0.f && f1.A <= 1.0f);
    assert(f1.b >= 0.f && f1.b <= 1.0f);
    assert(f1.a >= 0.f && f1.a <= 1.0f);

    // Compose image on coloured background to better judge dissimilarity with various backgrounds
    if (f1.a < 1.0) {
        f1.l *= f1.a; // using premultiplied alpha
        f1.A *= f1.a;
        f1.b *= f1.a;

        int n = i ^ j;
        if (n & 4) {
            f1.l += 1.0 - f1.a;
        }
        if (n & 8) {
            f1.A += 1.0 - f1.a;
        }
        if (n & 16) {
            f1.b += 1.0 - f1.a;
        }
    }

    // SSIM is supposed to be applied only to luma,
    // lower amplitude of chroma to lower score for chroma difference
    // (chroma is not ignored completely, because IMHO it also matters)
    f1.A *= 0.75;
    f1.b *= 0.75;

    return f1;
}

/*
 Can be called only once. Copies image1.
 */
void dssim_set_original(dssim_info *inf, png24_image *image1)
{
    int width = inf->width = image1->width;
    int height = inf->height = image1->height;
    set_gamma(image1->gamma);

    inf->img1[0] = malloc(width * height * sizeof(float));
    inf->img1[1] = malloc(width * height * sizeof(float));
    inf->img1[2] = malloc(width * height * sizeof(float));

    int offset = 0;
    for (int j = 0; j < height; j++) {
        rgba8 *px1 = (rgba8 *)image1->row_pointers[j];
        for (int i = 0; i < width; i++, offset++) {
            laba f1 = convert_pixel(px1[i], i, j);

            inf->img1[0][offset] = f1.l;
            inf->img1[1][offset] = f1.A;
            inf->img1[2][offset] = f1.b;
        }
    }

    float *restrict sigma1_tmp = malloc(width * height * sizeof(float));
    float *tmp = malloc(width * height * sizeof(float));

    for(int ch=0; ch < 3; ch++) {
        float *img1 = inf->img1[ch];
        if (ch > 0) {
            blur(img1, tmp, img1, width, height, NULL);
        }

        for (int j = 0; j < width*height; j++) {
            sigma1_tmp[j] = img1[j] * img1[j];
        }

        inf->mu1[ch] = malloc(width * height * sizeof(float));
        blur(img1, tmp, inf->mu1[ch], width, height, NULL);

        inf->sigma1_sq[ch] = malloc(width * height * sizeof(float));
        blur(sigma1_tmp, tmp, inf->sigma1_sq[ch], width, height, NULL);
    }

    free(tmp);
    free(sigma1_tmp);
}

/*
    Returns 1 if image has wrong size.

    Can be called multiple times.
*/
int dssim_set_modified(dssim_info *inf, png24_image *image2)
{
    int width = inf->width;
    int height = inf->height;

    if (image2->width != width || image2->height != height) {
        return 1;
    }

    set_gamma(image2->gamma);

    float *restrict img2[3] = {
        malloc(width * height * sizeof(float)),
        malloc(width * height * sizeof(float)),
        malloc(width * height * sizeof(float)),
    };

    int offset = 0;
    for (int j = 0; j < height; j++) {
        rgba8 *px2 = (rgba8 *)image2->row_pointers[j];
        for (int i = 0; i < width; i++, offset++) {
            laba f2 = convert_pixel(px2[i], i, j);

            img2[0][offset] = f2.l;
            img2[1][offset] = f2.A;
            img2[2][offset] = f2.b;
        }
    }

    float *tmp = malloc(width * height * sizeof(float));
    for(int ch=0; ch < 3; ch++) {
        if (ch > 0) {
            blur(img2[ch], tmp, img2[ch], width, height, NULL);
        }
        float *restrict img1_img2 = malloc(width * height * sizeof(float));
        float *restrict img1 = inf->img1[ch];

        for (int j = 0; j < width*height; j++) {
            img1_img2[j] = img1[j] * img2[ch][j];
        }

        inf->sigma12[ch] = malloc(width * height * sizeof(float));
        blur(img1_img2, tmp, inf->sigma12[ch], width, height, NULL);

        inf->mu2[ch] = img1_img2; // reuse mem
        blur(img2[ch], tmp, inf->mu2[ch], width, height, NULL);

        inf->sigma2_sq[ch] = malloc(width * height * sizeof(float));
        blur(img2[ch], tmp, inf->sigma2_sq[ch], width, height, square_row);
        free(img2[ch]);
    }
    free(tmp);

    return 0;
}

static double dssim_compare_channel(const int ch, dssim_info *inf, float *ssimmap);

/*
 Algorithm based on Rabah Mehdi's C++ implementation

 Returns dssim.
 Saves dissimilarity visualisation as ssimfilename (pass NULL if not needed)

 You must call dssim_set_original and dssim_set_modified first.
 */
double dssim_compare(dssim_info *inf, float **ssim_map_out)
{
    double avgssim_l = dssim_compare_channel(0, inf, NULL);
    double avgssim_A = dssim_compare_channel(1, inf, NULL);
    double avgssim_b = dssim_compare_channel(2, inf, NULL);

    double minavgssim = (avgssim_l + avgssim_A + avgssim_b)/3.0;

    return 1.0 / (minavgssim) - 1.0;
}

static double dssim_compare_channel(const int ch, dssim_info *inf, float *ssimmap)
{
    int width = inf->width;
    int height = inf->height;

    float *restrict mu1 = inf->mu1[ch];
    float *restrict mu2 = inf->mu2[ch];
    float *restrict sigma1_sq = inf->sigma1_sq[ch];
    float *restrict sigma2_sq = inf->sigma2_sq[ch];
    float *restrict sigma12 = inf->sigma12[ch];

    const double c1 = 0.01 * 0.01, c2 = 0.03 * 0.03;
    double avgssim = 0;

    for (int offset = 0; offset < width * height; offset++) {
        double ssim = ((2.0*(mu1[offset]*mu2[offset]) + c1)
                 * (2.0 *
                    (sigma12[offset] - (mu1[offset] * mu2[offset])) + c2))
                /
                (((mu1[offset]*mu1[offset]) + (mu2[offset]*mu2[offset]) + c1)
                     * ((sigma1_sq[offset] -
                         (mu1[offset] *
                          mu1[offset])) +
                        (sigma2_sq[offset] - (mu2[offset] * mu2[offset])) + c2));

        avgssim += ssim;

        if (ssimmap) {
            ssimmap[offset] = ssim;
        }
    }

    free(inf->mu2[ch]); inf->mu2[ch] = NULL;
    free(inf->sigma12[ch]); inf->sigma12[ch] = NULL;
    free(inf->sigma2_sq[ch]); inf->sigma2_sq[ch] = NULL;

    return avgssim / ((double)width * height);
}
