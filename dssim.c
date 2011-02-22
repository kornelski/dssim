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
    unsigned char r, g, b, a;
} rgba8;

typedef struct {
    float r, g, b, a;
} rgbaf;

typedef struct {
    float l, A, b, a;
} laba;

struct dssim_info {
    int width, height;
    laba *img1, *mu1, *sigma1_sq;
    laba *mu2, *sigma2_sq, *sigma12;
};

dssim_info *dssim_init()
{
    return calloc(1, sizeof(dssim_info));
}

void dssim_dealloc(dssim_info *inf)
{
    if (inf->mu2) {
        free(inf->mu2);
        inf->mu2 = NULL;
    }
    if (inf->sigma2_sq) {
        free(inf->sigma2_sq);
        inf->sigma2_sq = NULL;
    }
    if (inf->sigma12) {
        free(inf->sigma12);
        inf->sigma12 = NULL;
    }
    free(inf->img1);
    free(inf->mu1);
    free(inf->sigma1_sq);
    free(inf);
}

/* Converts 0..255 pixel to internal 0..1 with premultiplied alpha */
/*
 * inline static rgbaf rgba8_to_f(const float gamma, rgba8 px)
 * {
 *  float r = powf(px.r/255.0f, 1.0f/gamma),
 *        g = powf(px.g/255.0f, 1.0f/gamma),
 *        b = powf(px.b/255.0f, 1.0f/gamma),
 *        a = px.a/255.0f;
 *
 *  return (rgbaf){r*a,g*a,b*a,a};
 * }
 */

/* Converts premultiplied alpha 0..1 to 0..255 */
inline static rgba8 rgbaf_to_8(const float gamma, rgbaf px)
{
    if (px.a < 1.0 / 256.0f) {
        return (rgba8) {0, 0, 0, 0};
    }

    float r, g, b, a;

    // 256, because numbers are in range 0..255.9999… rounded down
    r = powf(px.r / px.a, gamma) * 256.0f;
    g = powf(px.g / px.a, gamma) * 256.0f;
    b = powf(px.b / px.a, gamma) * 256.0f;
    a = px.a * 256.0f;

    return (rgba8) {
        r >= 255 ? 255 : (r <= 0 ? 0 : r),
        g >= 255 ? 255 : (g <= 0 ? 0 : g),
        b >= 255 ? 255 : (b <= 0 ? 0 : b),
        a >= 255 ? 255 : a,
    };
}

static const float D65x = 0.9505f, D65y = 1.0f, D65z = 1.089f;

inline static laba rgba_to_laba(const float gamma, const rgba8 px)
{
    float r = powf(px.r / 255.0f, 1.0f / gamma),
          g = powf(px.g / 255.0f, 1.0f / gamma),
          b = powf(px.b / 255.0f, 1.0f / gamma),
          a = px.a / 255.0f;

    float fx = (r * 0.4124f + g * 0.3576f + b * 0.1805f) / D65x;
    float fy = (r * 0.2126f + g * 0.7152f + b * 0.0722f) / D65y;
    float fz = (r * 0.0193f + g * 0.1192f + b * 0.9505f) / D65z;
    const float epsilon = 216.0 / 24389.0;
    fx =
        ((fx > epsilon) ? powf(fx,
                               (1.0f / 3.0f)) : (7.787f * fx + 16.0f / 116.0f));
    fy =
        ((fy > epsilon) ? powf(fy,
                               (1.0f / 3.0f)) : (7.787f * fy + 16.0f / 116.0f));
    fz =
        ((fz > epsilon) ? powf(fz,
                               (1.0f / 3.0f)) : (7.787f * fz + 16.0f / 116.0f));
    return (laba) {
        (116.0f * fy - 16.0f) / 100.0 * a,
        (86.2f + 500.0f * (fx - fy)) / 220.0 * a, /* 86 is a fudge to make the value positive */
        (107.9f + 200.0f * (fy - fz)) / 220.0 * a, /* 107 is a fudge to make the value positive */
        a
    };
}

/* Macros to avoid repeating every line 4 times */

#define LABA_OP(dst, X, op, Y) dst = (laba) { \
        (X).l op(Y).l, \
        (X).A op(Y).A, \
        (X).b op(Y).b, \
        (X).a op(Y).a } \

#define LABA_OPC(dst, X, op, Y) dst = (laba) { \
        (X).l op(Y), \
        (X).A op(Y), \
        (X).b op(Y), \
        (X).a op(Y) } \

#define LABA_OP1(dst, op, Y) dst = (laba) { \
        dst.l op(Y).l, \
        dst.A op(Y).A, \
        dst.b op(Y).b, \
        dst.a op(Y).a } \


typedef void rowcallback(laba *, int width);

static void square_row(laba *row, int width)
{
    for (int i = 0; i < width; i++) {
        LABA_OP(row[i], row[i], *, row[i]);
    }
}

/*
 * Blurs image horizontally (width 2*size+1) and writes it transposed to dst
 * (called twice gives 2d blur)
 * Callback is executed on every row before blurring
 */
static void transposing_1d_blur(laba *restrict src,
                                laba *restrict dst,
                                int width,
                                int height,
                                const int size,
                                rowcallback *const callback)
{
    const float sizef = size;

    for (int j = 0; j < height; j++) {
        laba *restrict row = src + j * width;

        // preprocess line
        if (callback) {
            callback(row, width);
        }

        // accumulate sum for pixels outside line
        laba sum;
        LABA_OPC(sum, row[0], *, sizef);
        for (int i = 0; i < size; i++) {
            LABA_OP1(sum, +=, row[i]);
        }

        // blur with left side outside line
        for (int i = 0; i < size; i++) {
            LABA_OP1(sum, -=, row[0]);
            if ((i + size) < width) {
                LABA_OP1(sum, +=, row[i + size]);
            }

            LABA_OPC(dst[i * height + j], sum, /, sizef * 2.0f);
        }

        for (int i = size; i < width - size; i++) {
            LABA_OP1(sum, -=, row[i - size]);
            LABA_OP1(sum, +=, row[i + size]);

            LABA_OPC(dst[i * height + j], sum, /, sizef * 2.0f);
        }

        // blur with right side outside line
        for (int i = width - size; i < width; i++) {
            if (i - size >= 0) {
                LABA_OP1(sum, -=, row[i - size]);
            }
            LABA_OP1(sum, +=, row[width - 1]);

            LABA_OPC(dst[i * height + j], sum, /, sizef * 2.0f);
        }
    }
}

/*
 * Filters image with callback and blurs (lousy approximate of gaussian)
 * it proportionally to
 */
static void blur(laba *restrict src, laba *restrict tmp, laba *restrict dst,
                 int width, int height, rowcallback *const callback)
{
    int small = 1, big = 1;
    if (MIN(height, width) > 100) {
        big++;
    }
    if (MIN(height, width) > 200) {
        big++;
    }
    if (MIN(height, width) > 500) {
        small++;
    }
    if (MIN(height, width) > 800) {
        big++;
    }

    transposing_1d_blur(src, tmp, width, height, 1, callback);
    transposing_1d_blur(tmp, dst, height, width, 1, NULL);
    transposing_1d_blur(src, tmp, width, height, small, NULL);
    transposing_1d_blur(tmp, dst, height, width, small, NULL);
    transposing_1d_blur(dst, tmp, width, height, big, NULL);
    transposing_1d_blur(tmp, dst, height, width, big, NULL);
}

static void write_image(const char *filename,
                        const rgba8 *pixels,
                        int width,
                        int height,
                        float gamma)
{
    FILE *outfile = fopen(filename, "wb");
    if (!outfile) {
        return;
    }

    png_structp png_ptr = png_create_write_struct(PNG_LIBPNG_VER_STRING,
                          NULL, NULL, NULL);
    png_infop info_ptr = png_create_info_struct(png_ptr);
    png_init_io(png_ptr, outfile);
    png_set_IHDR(png_ptr, info_ptr, width, height, 8, PNG_COLOR_TYPE_RGBA,
                 0, PNG_COMPRESSION_TYPE_DEFAULT, PNG_FILTER_TYPE_DEFAULT);
    png_set_gAMA(png_ptr, info_ptr, gamma);
    png_write_info(png_ptr, info_ptr);

    for (int i = 0; i < height; i++) {
        png_write_row(png_ptr, (png_bytep)(pixels + i * width));
    }

    png_write_end(png_ptr, info_ptr);
    png_destroy_write_struct(&png_ptr, &info_ptr);
}

/*
 * Conversion is not reversible
 */
inline static laba convert_pixel(rgba8 px, float gamma, int i, int j)
{
    laba f1 = rgba_to_laba(gamma, px);
    assert(f1.l >= 0.0f && f1.A >= 0.0f && f1.b >= 0.0f && f1.a <= 1.0);

    // Compose image on coloured background to better judge dissimilarity with various backgrounds
    int n = i ^ j;
    if (n & 4) {
        f1.l += 1.0 - f1.a; // using premultiplied alpha
    }
    if (n & 8) {
        f1.A += 1.0 - f1.a;
    }
    if (n & 16) {
        f1.b += 1.0 - f1.a;
    }

    // Since alpha is already blended with other channels,
    // lower amplitude of alpha to lower score for alpha difference
    f1.a *= 0.75;

    // SSIM is supposed to be applied only to luma,
    // lower amplitude of chroma to lower score for chroma difference
    // (chroma is not ignored completely, because IMHO it also matters)
    f1.A *= 0.75;
    f1.b *= 0.75;

    return f1;
}

/*
 Algorithm based on Rabah Mehdi's C++ implementation

 saves dissimilarity visualisation as ssimfilename (pass NULL if not needed)
 */
double dssim_image(png24_image *image1,
                   png24_image *image2,
                   const char *ssimfilename)
{
    dssim_info *inf = dssim_init();
    dssim_set_original(inf, image1);
    dssim_set_modified(inf, image2);
    return dssim_compare(inf, ssimfilename);
}

/*
 Can be called only once. Copies image1.
 */
void dssim_set_original(dssim_info *inf, png24_image *image1)
{
    int width = inf->width = image1->width;
    int height = inf->height = image1->height;
    float gamma = image1->gamma;

    laba *restrict img1 = inf->img1 = malloc(width * height * sizeof(laba));
    laba *restrict sigma1_tmp = malloc(width * height * sizeof(laba));

    int offset = 0;
    for (int j = 0; j < height; j++) {
        rgba8 *px1 = (rgba8 *)image1->row_pointers[j];
        for (int i = 0; i < width; i++, offset++) {
            laba f1 = convert_pixel(px1[i], gamma, i, j);

            img1[offset] = f1;
            LABA_OP(sigma1_tmp[offset], f1, *, f1);
        }
    }

    laba *tmp = malloc(width * height * sizeof(laba));
    inf->mu1 = malloc(width * height * sizeof(laba));
    blur(img1, tmp, inf->mu1, width, height, NULL);

    inf->sigma1_sq = malloc(width * height * sizeof(laba));
    blur(sigma1_tmp, tmp, inf->sigma1_sq, width, height, NULL);

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

    float gamma2 = image2->gamma;

    laba *restrict img1 = inf->img1;
    laba *restrict img2 = malloc(width * height * sizeof(laba));
    laba *restrict img1_img2 = malloc(width * height * sizeof(laba));

    int offset = 0;
    for (int j = 0; j < height; j++) {
        rgba8 *px2 = (rgba8 *)image2->row_pointers[j];
        for (int i = 0; i < width; i++, offset++) {
            laba f2 = convert_pixel(px2[i], gamma2, i, j);

            img2[offset] = f2;
            LABA_OP(img1_img2[offset], img1[offset], *, f2);
        }
    }

    laba *tmp = malloc(width * height * sizeof(laba));

    inf->sigma12 = malloc(width * height * sizeof(laba));
    blur(img1_img2, tmp, inf->sigma12, width, height, NULL);
    free(img1_img2);

    inf->mu2 = malloc(width * height * sizeof(laba));
    blur(img2, tmp, inf->mu2, width, height, NULL);

    inf->sigma2_sq = malloc(width * height * sizeof(laba));
    blur(img2, tmp, inf->sigma2_sq, width, height, square_row);

    free(img2);
    free(tmp);

    return 0;
}

/*
 Returns dssim.
 Saves dissimilarity visualisation as ssimfilename (pass NULL if not needed)

 You must call dssim_set_original and dssim_set_modified first.
 */
double dssim_compare(dssim_info *inf, const char *ssimfilename)
{
    int width = inf->width;
    int height = inf->height;

    laba *restrict mu1 = inf->mu1;
    laba *restrict mu2 = inf->mu2;
    laba *restrict sigma1_sq = inf->sigma1_sq;
    laba *restrict sigma2_sq = inf->sigma2_sq;
    laba *restrict sigma12 = inf->sigma12;

    rgba8 *ssimmap = (rgba8*)mu2; // result can overwrite source. it's safe because sizeof(rgb) <= sizeof(fpixel)

    const double c1 = 0.01 * 0.01, c2 = 0.03 * 0.03;
    double avgminssim = 0;

#define SSIM(r) ((2.0*(mu1[offset].r*mu2[offset].r) + c1) \
                 * (2.0 * \
                    (sigma12[offset].r - (mu1[offset].r * mu2[offset].r)) + c2)) \
/ \
(((mu1[offset].r*mu1[offset].r) + (mu2[offset].r*mu2[offset].r) + c1) \
     * ((sigma1_sq[offset].r - \
         (mu1[offset].r * \
          mu1[offset].r)) + \
        (sigma2_sq[offset].r - (mu2[offset].r * mu2[offset].r)) + c2))

    for (int offset = 0; offset < width * height; offset++) {
        laba ssim = (laba) {
            SSIM(l), SSIM(A), SSIM(b), SSIM(a)
        };

        double minssim = MIN(MIN(ssim.l, ssim.A), MIN(ssim.b, ssim.a));
        avgminssim += minssim;

        if (ssimfilename) {
            float max = 1.0 - MIN(MIN(ssim.l, ssim.A), ssim.b);
            float maxsq = max * max;
            ssimmap[offset] = rgbaf_to_8(1.0 / 2.2, (rgbaf) {
                (1.0 - ssim.a) + maxsq,
                max + maxsq,
                max * 0.5f + (1.0 - ssim.a) * 0.5f + maxsq,
                1
            });
        }
    }

    // mu2 is reused for ssimmap
    free(inf->sigma12);
    inf->sigma12 = NULL;
    free(inf->sigma2_sq);
    inf->sigma2_sq = NULL;

    avgminssim /= (double)width * height;

    if (ssimfilename) {
        write_image(ssimfilename, ssimmap, width, height, 1.0 / 2.2);
    }

    // mu2 is reused for ssimmap
    free(inf->mu2);
    inf->mu2 = NULL;


    return 1.0 / (avgminssim) - 1.0;
}
