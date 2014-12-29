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
#include <stdbool.h>
#include <math.h>
#include <assert.h>
#include "dssim.h"

/** Bigger number puts more emphasis on color channels. */
#define COLOR_WEIGHT 4

/** Smaller values are more sensitive to single-pixel differences. Increase for high-DPI images. */
#define DETAIL_SIZE 3

#ifndef MIN
#define MIN(a,b) ((a)<=(b)?(a):(b))
#endif
#ifndef MAX
#define MAX(a,b) ((a)>=(b)?(a):(b))
#endif

#define MAX_CHANS 3

typedef struct {
    float l, A, b, a;
} laba;

typedef struct {
    int width, height;
    float *img, *mu, *img_sq_blur;
} dssim_info_chan;

typedef struct {
    dssim_info_chan chan[MAX_CHANS];
} dssim_image;

struct dssim_info {
    dssim_image img[2];
    float *img1_img2_blur[MAX_CHANS];
    int channels;
    dssim_row_callback *convert_image_row;
    bool subsample_channels;
};

static void convert_image_row(float *const restrict channels[], const int num_channels, const int y, const int width, void *user_data);
static void copy_image_row(float *const restrict channels[], const int num_channels, const int y, const int width, void *user_data);

dssim_info *dssim_init(int channels)
{
    if (channels != 1 && channels != MAX_CHANS) {
        return NULL;
    }

    dssim_info *inf = malloc(sizeof(dssim_info));
    if (inf) *inf = (dssim_info){
        .channels = channels,
        .subsample_channels = channels > 1,
        .convert_image_row = convert_image_row,
    };
    return inf;
}

void dssim_dealloc(dssim_info *inf)
{
    for(int i=0; i < 2; i++) {
        for (int ch = 0; ch < inf->channels; ch++) {
            free(inf->img[i].chan[ch].img); inf->img[i].chan[ch].img = NULL;
            free(inf->img[i].chan[ch].mu); inf->img[i].chan[ch].mu = NULL;
            free(inf->img[i].chan[ch].img_sq_blur); inf->img[i].chan[ch].img_sq_blur = NULL;
        }
    }
    for (int ch = 0; ch < inf->channels; ch++) {
        free(inf->img1_img2_blur[ch]); inf->img1_img2_blur[ch] = NULL;
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

typedef void rowcallback(const float *restrict src, float *restrict dst, const int width);

static void square_row(const float *restrict src, float *restrict dst, const int width)
{
    for (int i = 0; i < width; i++) {
        dst[i] = src[i] * src[i];
    }
}

/*
 * Blurs image horizontally (width 2*size+1) and writes it transposed to dst
 * (called twice gives 2d blur)
 */
static void transposing_1d_blur(float *restrict src, float *restrict dst, const int width, const int height)
{
    const int size = DETAIL_SIZE-1;
    const double invdivisor = 1.0 / (size * 2 + 1);

    for (int j = 0; j < height; j++) {
        float *restrict row = src + j * width;

        // accumulate sum for pixels outside the image
        double sum = row[0] * size;

        // preload sum for the right side of the blur
        for(int i=0; i < size; i++) {
            sum += row[MIN(width-1, i)];
        }

        // blur with left side outside line
        for(int i=0; i < size; i++) {
            sum += row[MIN(width-1, i+size)];

            dst[i*height + j] = sum * invdivisor;

            sum -= row[MAX(0, i-size)];
        }

        for(int i=size; i < width-size; i++) {
            sum += row[i+size];

            dst[i*height + j] = sum * invdivisor;

            sum -= row[i-size];
        }

        // blur with right side outside line
        for(int i=width-size; i < width; i++) {
            sum += row[MIN(width-1, i+size)];

            dst[i*height + j] = sum * invdivisor;

            sum -= row[MAX(0, i-size)];
        }
    }
}

static void regular_1d_blur(const float *src, float *restrict tmp1, float *dst, const int width, const int height, const int runs, rowcallback *const callback)
{
    // tmp1 is expected to hold at least two lines
    float *restrict tmp2 = tmp1 + width;

    for(int j=0; j < height; j++) {
        for(int run = 0; run < runs; run++) {
            // To improve locality blur is done on tmp1->tmp2 and tmp2->tmp1 buffers,
            // except first and last run which use src->tmp and tmp->dst
            const float *restrict row = (run == 0   ? src + j*width : (run & 1) ? tmp1 : tmp2);
            float *restrict dstrow = (run == runs-1 ? dst + j*width : (run & 1) ? tmp2 : tmp1);

            if (!run && callback) {
                callback(row, tmp2, width); // on the first run tmp2 is not used
                row = tmp2;
            }

            const int size = DETAIL_SIZE;
            const double invdivisor = 1.0 / (size * 2 + 1);

            // accumulate sum for pixels outside the image
            double sum = row[0] * size;

            // preload sum for the right side of the blur
            for(int i=0; i < size; i++) {
                sum += row[MIN(width-1, i)];
            }

            // blur with left side outside line
            for(int i=0; i < size; i++) {
                sum += row[MIN(width-1, i+size)];

                dstrow[i] = sum * invdivisor;

                sum -= row[MAX(0, i-size)];
            }

            for(int i=size; i < width-size; i++) {
                sum += row[i+size];

                dstrow[i] = sum * invdivisor;

                sum -= row[i-size];
            }

            // blur with right side outside line
            for(int i=width-size; i < width; i++) {
                sum += row[MIN(width-1, i+size)];

                dstrow[i] = sum * invdivisor;

                sum -= row[MAX(0, i-size)];
            }
        }
    }
}


/*
 * Filters image with callback and blurs (lousy approximate of gaussian)
 */
static void blur(const float *restrict src, float *restrict tmp, float *restrict dst,
                 const int width, const int height, rowcallback *const callback, int extrablur)
{
    regular_1d_blur(src, tmp, dst, width, height, 2, callback);
    if (extrablur) {
        regular_1d_blur(dst, tmp, dst, height, width, 4, NULL);
    }
    transposing_1d_blur(dst, tmp, width, height);

    // After transposing buffer is rotated, so height and width are swapped
    // And reuse of buffers made tmp hold the image, and dst used as temporary until the last transpose
    regular_1d_blur(tmp, dst, tmp, height, width, 2, NULL);
    if (extrablur) {
        regular_1d_blur(tmp, dst, tmp, height, width, 4, NULL);
    }
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

static void convert_image_subsampled(dssim_image *img, dssim_row_callback cb, void *callback_user_data, const int channels)
{
    const int width = img->chan[0].width;
    const int height = img->chan[0].height;
    float *row_tmp[channels];

    for(int ch = 1; ch < channels; ch++) {
        row_tmp[ch] = calloc(width, sizeof(row_tmp[0])); // for the callback all channels have the same width!
    }

    for(int y = 0; y < height; y++) {
    row_tmp[0] = &img->chan[0].img[width * y]; // Luma can be written directly (it's unscaled)

    cb(row_tmp, channels, y, width, callback_user_data);

    for(int ch = 1; ch < channels; ch++) { // Chroma is downsampled
        const int halfy = y * img->chan[ch].height / height;
        float *dstrow = &img->chan[ch].img[halfy * img->chan[ch].width];

            for(int x = 0; x < width; x++) {
                dstrow[x/2] += row_tmp[ch][x] * 0.25f;
            }
        }
    }

    for(int ch = 1; ch < channels; ch++) {
        free(row_tmp[ch]);
    }
}

static void convert_image(dssim_image *img, dssim_row_callback cb, void *callback_user_data, const int channels)
{
    const int width = img->chan[0].width;
    const int height = img->chan[0].height;
    float *row_tmp[channels];

    for(int y = 0; y < height; y++) {
        for(int ch = 0; ch < channels; ch++) {
            row_tmp[ch] = &img->chan[ch].img[width * y];
        }
        cb(row_tmp, channels, y, width, callback_user_data);
    }
}

static void dssim_preprocess_image_channel(dssim_image *img, float *restrict tmp, const int channels, bool extrablur);

static void convert_image_row(float *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
{
    dssim_rgba *const row = ((dssim_rgba **)user_data)[y];

    for (int x = 0; x < width; x++) {
        laba px = convert_pixel(row[x], x, y);
        channels[0][x] = px.l;
        if (num_channels >= 3) {
            channels[1][x] = px.A;
            channels[2][x] = px.b;
        }
    }
}

static void copy_image_row(float *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
{
    dssim_rgba *const row = ((dssim_rgba **)user_data)[y];

    for (int x = 0; x < width; x++) {
        channels[0][x] = gamma_lut[row[x].r];
        channels[1][x] = gamma_lut[row[x].g];
        channels[2][x] = gamma_lut[row[x].b];
    }
}

/*
 Can be called only once. Copies the image.
 */
void dssim_set_original(dssim_info *inf, dssim_rgba *row_pointers[], const int width, const int height, double gamma)
{
    set_gamma(gamma);
    dssim_set_original_float_callback(inf, width, height, inf->convert_image_row, (void*)row_pointers);
}

void dssim_set_original_float_callback(dssim_info *inf, const int width, const int height, dssim_row_callback cb, void *callback_user_data)
{
    for(int ch = 0; ch < inf->channels; ch++) {
        inf->img[0].chan[ch].width = inf->subsample_channels && ch > 0 ? width/2 : width;
        inf->img[0].chan[ch].height = inf->subsample_channels && ch > 0 ? height/2 : height;
        inf->img[0].chan[ch].img = calloc(inf->img[0].chan[ch].width * inf->img[0].chan[ch].height, sizeof(inf->img[0].chan[ch].img[0]));
    }

    if (inf->subsample_channels) {
        convert_image_subsampled(&inf->img[0], cb, callback_user_data, inf->channels);
    } else {
        convert_image(&inf->img[0], cb, callback_user_data, inf->channels);
    }

    float *tmp = malloc(width * height * sizeof(tmp[0]));
    for (int ch = 0; ch < inf->channels; ch++) {
        dssim_preprocess_image_channel(&inf->img[0], tmp, ch, ch > 0 && inf->subsample_channels);
    }
    free(tmp);
}

static void dssim_preprocess_image_channel(dssim_image *img, float *restrict tmp, const int ch, bool extrablur)
{
    const int width = img->chan[ch].width;
    const int height = img->chan[ch].height;

    if (extrablur) {
        blur(img->chan[ch].img, tmp, img->chan[ch].img, width, height, NULL, 0);
    }

    img->chan[ch].mu = malloc(width * height * sizeof(img->chan[ch].mu[0]));
    blur(img->chan[ch].img, tmp, img->chan[ch].mu, width, height, NULL, extrablur);

    img->chan[ch].img_sq_blur = malloc(width * height * sizeof(img->chan[ch].img_sq_blur[0]));
    blur(img->chan[ch].img, tmp, img->chan[ch].img_sq_blur, width, height, square_row, extrablur);
}

/*
    Returns 1 if image has wrong size.

    Can be called multiple times.
*/
int dssim_set_modified(dssim_info *inf, dssim_rgba *row_pointers[], const int image_width, const int image_height, double gamma)
{
    set_gamma(gamma);
    return dssim_set_modified_float_callback(inf, image_width, image_height, inf->convert_image_row, (void*)row_pointers);
}

int dssim_set_modified_float_callback(dssim_info *inf, const int image_width, const int image_height, dssim_row_callback cb, void *callback_user_data)
{
    const int width = inf->img[0].chan[0].width;
    const int height = inf->img[0].chan[0].height;

    if (image_width != width || image_height != height) {
        return 1;
    }

    for (int ch = 0; ch < inf->channels; ch++) {
        inf->img[1].chan[ch].width = inf->img[0].chan[ch].width;
        inf->img[1].chan[ch].height = inf->img[0].chan[ch].height;
        inf->img[1].chan[ch].img = calloc(inf->img[1].chan[ch].width * inf->img[1].chan[ch].height, sizeof(inf->img[1].chan[ch].img[0]));
    }

    if (inf->subsample_channels) {
        convert_image_subsampled(&inf->img[1], cb, callback_user_data, inf->channels);
    } else {
        convert_image(&inf->img[1], cb, callback_user_data, inf->channels);
    }

    float *tmp = malloc(width * height * sizeof(tmp[0]));
    for (int ch = 0; ch < inf->channels; ch++) {
        dssim_preprocess_image_channel(&inf->img[1], tmp, ch, ch > 0 && inf->subsample_channels);
    }
    free(tmp);

    return 0;
}

static void preprocess_combined_images(dssim_info *inf) {
    for (int ch = 0; ch < inf->channels; ch++) {
        const int width = inf->img[0].chan[ch].width;
        const int height = inf->img[0].chan[ch].height;

        float *restrict img1_img2 = malloc(width * height * sizeof(img1_img2[0]));
        float *restrict img1 = inf->img[0].chan[ch].img;
        float *restrict img2 = inf->img[1].chan[ch].img;

        for (int j = 0; j < width*height; j++) {
            img1_img2[j] = img1[j] * img2[j];
        }

        float *restrict tmp = inf->img[1].chan[ch].img;
        inf->img[1].chan[ch].img = NULL;

        blur(img1_img2, tmp, img1_img2, width, height, NULL, ch > 0 && inf->subsample_channels);
        inf->img1_img2_blur[ch] = img1_img2;

        free(tmp);
    }
}

static double dssim_compare_channel(dssim_info *inf, int ch, float **ssimmap);

/*
 Algorithm based on Rabah Mehdi's C++ implementation

 Returns dssim.
 Saves dissimilarity visualisation as ssimfilename (pass NULL if not needed)

 You must call dssim_set_original and dssim_set_modified first.
 */
double dssim_compare(dssim_info *inf, float **ssim_map_out)
{
    preprocess_combined_images(inf);

    double avgssim = 0;
    int area = 0;
    for (int ch = 0; ch < inf->channels; ch++) {
        const double weight = ch && inf->subsample_channels ? COLOR_WEIGHT : 1;
        avgssim += weight * dssim_compare_channel(inf, ch, ssim_map_out && ch == 0 ? ssim_map_out : NULL);
        area += weight * inf->img[0].chan[ch].width * inf->img[0].chan[ch].height;

        free(inf->img1_img2_blur[ch]); inf->img1_img2_blur[ch] = NULL;
    }
    avgssim /= (double)area;

    return 1.0 / (avgssim) - 1.0;
}

static double dssim_compare_channel(dssim_info *inf, int ch, float **ssim_map_out)
{
    const int width = inf->img[0].chan[ch].width;
    const int height = inf->img[0].chan[ch].height;

    const float *restrict mu1 = inf->img[0].chan[ch].mu;
    float *const mu2 = inf->img[1].chan[ch].mu;
    const float *restrict img1_sq_blur = inf->img[0].chan[ch].img_sq_blur;
    const float *restrict img2_sq_blur = inf->img[1].chan[ch].img_sq_blur;
    const float *restrict img1_img2_blur = inf->img1_img2_blur[ch];

    const double c1 = 0.01 * 0.01, c2 = 0.03 * 0.03;
    double ssim_sum = 0;

    float *const ssimmap = ssim_map_out ? mu2 : NULL;

    for (int offset = 0; offset < width * height; offset++) {
        const double mu1_sq = mu1[offset]*mu1[offset];
        const double mu2_sq = mu2[offset]*mu2[offset];
        const double mu1_mu2 = mu1[offset]*mu2[offset];
        const double sigma1_sq = img1_sq_blur[offset] - mu1_sq;
        const double sigma2_sq = img2_sq_blur[offset] - mu2_sq;
        const double sigma12 = img1_img2_blur[offset] - mu1_mu2;

        const double ssim = (2.0 * mu1_mu2 + c1) * (2.0 * sigma12 + c2)
                            /
                            ((mu1_sq + mu2_sq + c1) * (sigma1_sq + sigma2_sq + c2));

        ssim_sum += ssim;

        if (ssimmap) {
            ssimmap[offset] = ssim;
        }
    }

    if (ssim_map_out) {
        *ssim_map_out = ssimmap; // reuses mu2 memory
    } else {
        free(inf->img[1].chan[ch].mu);
    }
    inf->img[1].chan[ch].mu = NULL;

    free(inf->img[1].chan[ch].img_sq_blur); inf->img[1].chan[ch].img_sq_blur = NULL;

    return ssim_sum;
}
