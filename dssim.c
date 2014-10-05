/*
 * © 2011-2014 Kornel Lesiński. All rights reserved.
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

#define MAX_CHANS 3

typedef struct {
    float l, A, b, a;
} laba;

typedef struct {
    int width, height;
    float *img1, *mu1, *sigma1_sq;
    float *mu2, *sigma2_sq, *sigma12;
} dssim_info_chan;

struct dssim_info {
    dssim_info_chan chan[MAX_CHANS];
    int channels;
};

dssim_info *dssim_init(int channels)
{
    if (channels != 1 && channels != MAX_CHANS) {
        return NULL;
    }

    dssim_info *inf = calloc(1, sizeof(dssim_info));
    if (inf) {
        inf->channels = channels;
    }
    return inf;
}

void dssim_dealloc(dssim_info *inf)
{
    for (int ch = 0; ch < inf->channels; ch++) {
        free(inf->chan[ch].mu2); inf->chan[ch].mu2 = NULL;
        free(inf->chan[ch].sigma2_sq); inf->chan[ch].sigma2_sq = NULL;
        free(inf->chan[ch].sigma12); inf->chan[ch].sigma12 = NULL;
        free(inf->chan[ch].img1); inf->chan[ch].img1 = NULL;
        free(inf->chan[ch].mu1); inf->chan[ch].mu1 = NULL;
        free(inf->chan[ch].sigma1_sq); inf->chan[ch].sigma1_sq = NULL;
    }
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

inline static laba rgba_to_laba(const dssim_rgba px)
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

typedef void rowcallback(float *, const int width);

static void square_row(float *row, const int width)
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
    const int size = 4;
    const float sizef = size;

    for (int j = 0; j < height; j++) {
        float *restrict row = src + j * width;

        // accumulate sum for pixels outside line
        float sum = 0;
        sum = row[0] * sizef;
        for(int i=0; i < MIN(width,size); i++) {
            sum += row[i];
        }

        // blur with left side outside line
        for(int i=0; i < MIN(width,size); i++) {
            sum -= row[0];
            if((i + size) < width){
                sum += row[i+size];
            }

            dst[i*height + j] = sum / (sizef * 2.0f);
        }

        for(int i=size; i < width-size; i++) {
            sum -= row[i-size];
            sum += row[i+size];

            dst[i*height + j] = sum / (sizef * 2.0f);
        }

        // blur with right side outside line
        for(int i=width-size; i < width; i++) {
            if(i-size >= 0){
                sum -= row[i-size];
            }
            sum += row[width-1];

            dst[i*height + j] = sum / (sizef * 2.0f);
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
        sum = row[0] * sizef;
        for(int i=0; i < MIN(width,size); i++) {
            sum += row[i];
        }

        // blur with left side outside line
        for(int i=0; i < MIN(width,size); i++) {
            sum -= row[0];
            if ((i + size) < width) {
                sum += row[i + size];
            }

            dstrow[i] = sum / (sizef * 2.0f);
        }

        for (int i = size; i < width - size; i++) {
            sum -= row[i - size];
            sum += row[i + size];

            dstrow[i] = sum / (sizef * 2.0f);
        }

        // blur with right side outside line
        for (int i = width - size; i < width; i++) {
            if (i - size >= 0) {
                sum -= row[i - size];
            }
            sum += row[width - 1];

            dstrow[i] = sum / (sizef * 2.0f);
        }
    }
}


/*
 * Filters image with callback and blurs (lousy approximate of gaussian)
 */
static void blur(float *restrict src, float *restrict tmp, float *restrict dst,
                 const int width, const int height, rowcallback *const callback, int extrablur)
{
    regular_1d_blur(src, tmp, width, height, callback);
    regular_1d_blur(tmp, dst, width, height, NULL);
    if (extrablur) {
        transposing_1d_blur(dst, tmp, width, height);
        transposing_1d_blur(tmp, dst, width, height);
    }
    transposing_1d_blur(dst, tmp, width, height);
    if (extrablur) {
        regular_1d_blur(tmp, dst, height, width, NULL);
        regular_1d_blur(dst, tmp, height, width, NULL);
        regular_1d_blur(tmp, dst, height, width, NULL);
        regular_1d_blur(dst, tmp, height, width, NULL);
    }
    regular_1d_blur(tmp, dst, height, width, NULL);
    regular_1d_blur(dst, tmp, height, width, NULL);
    transposing_1d_blur(tmp, dst, height, width);
}

/*
 * Conversion is not reversible
 */
inline static laba convert_pixel(dssim_rgba px, int i, int j)
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

    return f1;
}

static void convert_image(dssim_rgba *row_pointers[], const double gamma, dssim_info *inf, float *restrict chans[])
{
    const int width = inf->chan[0].width;
    const int height = inf->chan[0].height;
    set_gamma(gamma);

    float *const ch0 = chans[0];
    float *const ch1 = inf->channels >= 3 ? chans[1] : NULL;
    float *const ch2 = inf->channels >= 3 ? chans[2] : NULL;

    const int halfwidth = inf->chan[1].width;
    for (int y = 0, offset = 0; y < height; y++) {
        dssim_rgba *const px1 = row_pointers[y];
        const int halfy = y * inf->chan[1].height / height;
        for (int x = 0; x < width; x++, offset++) {
            laba f1 = convert_pixel(px1[x], x, y);

            ch0[offset] = f1.l;

            if (ch1) {
                ch1[x/2 + halfy*halfwidth] += f1.A * 0.25f;
                ch2[x/2 + halfy*halfwidth] += f1.b * 0.25f;
            }
        }
    }
}

/*
 Can be called only once. Copies the image.
 */
void dssim_set_original(dssim_info *inf, dssim_rgba *row_pointers[], const int width, const int height, double gamma)
{
    float *restrict chans[inf->channels];
    for(int ch=0; ch < inf->channels; ch++) {
        inf->chan[ch].width = ch > 0 ? width/2 : width;
        inf->chan[ch].height = ch > 0 ? height/2 : height;
        inf->chan[ch].img1 = chans[ch] = calloc(inf->chan[ch].width * inf->chan[ch].height, sizeof(float));
    }

    convert_image(row_pointers, gamma, inf, chans);

    float *restrict sigma1_tmp = malloc(width * height * sizeof(float));
    float *tmp = malloc(width * height * sizeof(float));

    for (int ch = 0; ch < inf->channels; ch++) {
        const int width = inf->chan[ch].width;
        const int height = inf->chan[ch].height;

        float *img1 = inf->chan[ch].img1;
        if (ch > 0) {
            blur(img1, tmp, img1, width, height, NULL, 0);
        }

        for (int j = 0; j < width*height; j++) {
            sigma1_tmp[j] = img1[j] * img1[j];
        }

        inf->chan[ch].mu1 = malloc(width * height * sizeof(float));
        blur(img1, tmp, inf->chan[ch].mu1, width, height, NULL, ch > 0);

        inf->chan[ch].sigma1_sq = malloc(width * height * sizeof(float));
        blur(sigma1_tmp, tmp, inf->chan[ch].sigma1_sq, width, height, NULL, ch > 0);
    }

    free(tmp);
    free(sigma1_tmp);
}

/*
    Returns 1 if image has wrong size.

    Can be called multiple times.
*/
int dssim_set_modified(dssim_info *inf, dssim_rgba *row_pointers[], const int image_width, const int image_height, double gamma)
{
    const int width = inf->chan[0].width;
    const int height = inf->chan[0].height;

    if (image_width != width || image_height != height) {
        return 1;
    }

    float *restrict img2[inf->channels];
    for (int ch = 0; ch < inf->channels; ch++) {
        img2[ch] = calloc(inf->chan[ch].width * inf->chan[ch].height, sizeof(float));
    }

    convert_image(row_pointers, gamma, inf, img2);

    float *tmp = malloc(width * height * sizeof(float));
    for (int ch = 0; ch < inf->channels; ch++) {
        const int width = inf->chan[ch].width;
        const int height = inf->chan[ch].height;

        if (ch > 0) {
            blur(img2[ch], tmp, img2[ch], width, height, NULL, 0);
        }
        float *restrict img1_img2 = malloc(width * height * sizeof(float));
        float *restrict img1 = inf->chan[ch].img1;

        for (int j = 0; j < width*height; j++) {
            img1_img2[j] = img1[j] * img2[ch][j];
        }

        inf->chan[ch].sigma12 = malloc(width * height * sizeof(float));
        blur(img1_img2, tmp, inf->chan[ch].sigma12, width, height, NULL, ch > 0);

        inf->chan[ch].mu2 = img1_img2; // reuse mem
        blur(img2[ch], tmp, inf->chan[ch].mu2, width, height, NULL, ch > 0);

        inf->chan[ch].sigma2_sq = malloc(width * height * sizeof(float));
        blur(img2[ch], tmp, inf->chan[ch].sigma2_sq, width, height, square_row, ch > 0);
        free(img2[ch]);
    }
    free(tmp);

    return 0;
}

static double dssim_compare_channel(dssim_info_chan *chan, float **ssimmap);

/*
 Algorithm based on Rabah Mehdi's C++ implementation

 Returns dssim.
 Saves dissimilarity visualisation as ssimfilename (pass NULL if not needed)

 You must call dssim_set_original and dssim_set_modified first.
 */
double dssim_compare(dssim_info *inf, float **ssim_map_out)
{
    double avgssim = 0;
    for (int ch = 0; ch < inf->channels; ch++) {
        avgssim += dssim_compare_channel(&inf->chan[ch], ssim_map_out && ch == 0 ? ssim_map_out : NULL);
    }
    avgssim /= (double)inf->channels;

    return 1.0 / (avgssim) - 1.0;
}

static double dssim_compare_channel(dssim_info_chan *chan, float **ssim_map_out)
{
    const int width = chan->width;
    const int height = chan->height;

    float *restrict mu1 = chan->mu1;
    float *restrict mu2 = chan->mu2;
    float *restrict sigma1_sq = chan->sigma1_sq;
    float *restrict sigma2_sq = chan->sigma2_sq;
    float *restrict sigma12 = chan->sigma12;

    const double c1 = 0.01 * 0.01, c2 = 0.03 * 0.03;
    double avgssim = 0;

    float *const ssimmap = ssim_map_out ? chan->mu2 : NULL;

    for (int offset = 0; offset < width * height; offset++) {
        const double mu1_sq = mu1[offset]*mu1[offset];
        const double mu2_sq = mu2[offset]*mu2[offset];
        const double mu1mu2 = mu1[offset]*mu2[offset];

        const double ssim = (c1 + 2.0 * mu1mu2) * (c2 + 2.0 * (sigma12[offset] - mu1mu2))
                            /
                            ((c1 + mu1_sq + mu2_sq) * (c2 + sigma1_sq[offset] - mu1_sq + sigma2_sq[offset] - mu2_sq));

        avgssim += ssim;

        if (ssimmap) {
            ssimmap[offset] = ssim;
        }
    }

    if (ssim_map_out) {
        *ssim_map_out = ssimmap; // reuses mu2 memory
    } else {
        free(chan->mu2);
    }
    chan->mu2 = NULL;

    free(chan->sigma12); chan->sigma12 = NULL;
    free(chan->sigma2_sq); chan->sigma2_sq = NULL;

    return avgssim / ((double)width * height);
}
