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
#define COLOR_WEIGHT 1.0

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
    float l, A, b;
} dssim_lab;

typedef struct {
    unsigned char r, g, b, a;
} dssim_rgb;

typedef struct {
    int width, height;
    float *img, *mu, *img_sq_blur;
    bool is_chroma;
} dssim_chan;

struct dssim_image {
    dssim_chan chan[MAX_CHANS];
    int channels;
};

void dssim_dealloc_image(dssim_image *img)
{
    for (int ch = 0; ch < img->channels; ch++) {
        free(img->chan[ch].img);
        free(img->chan[ch].mu);
        free(img->chan[ch].img_sq_blur);
    }
    free(img);
}

static void set_gamma(double gamma_lut[static 256], const double invgamma)
{
    for (int i = 0; i < 256; i++) {
        gamma_lut[i] = pow(i / 255.0, 1.0 / invgamma);
    }
}

static const double D65x = 0.9505, D65y = 1.0, D65z = 1.089;

inline static dssim_lab rgb_to_lab(const double gamma_lut[static 256], const unsigned char pxr, const unsigned char pxg, const unsigned char pxb)
{
    const double r = gamma_lut[pxr],
                 g = gamma_lut[pxg],
                 b = gamma_lut[pxb];

    const double fx = (r * 0.4124 + g * 0.3576 + b * 0.1805) / D65x;
    const double fy = (r * 0.2126 + g * 0.7152 + b * 0.0722) / D65y;
    const double fz = (r * 0.0193 + g * 0.1192 + b * 0.9505) / D65z;

    const double epsilon = 216.0 / 24389.0;
    const double k = (24389.0 / 27.0) / 116.f; // http://www.brucelindbloom.com/LContinuity.html
    const float X = (fx > epsilon) ? powf(fx, 1.f / 3.f) - 16.f/116.f : k * fx;
    const float Y = (fy > epsilon) ? powf(fy, 1.f / 3.f) - 16.f/116.f : k * fy;
    const float Z = (fz > epsilon) ? powf(fz, 1.f / 3.f) - 16.f/116.f : k * fz;

    return (dssim_lab) {
        Y * 1.16f,
        (86.2f/ 220.0f + 500.0f/ 220.0f * (X - Y)), /* 86 is a fudge to make the value positive */
        (107.9f/ 220.0f + 200.0f/ 220.0f * (Y - Z)), /* 107 is a fudge to make the value positive */
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
inline static dssim_lab convert_pixel_rgba(const double gamma_lut[static 256], dssim_rgba px, int i, int j)
{
    dssim_lab f1 = rgb_to_lab(gamma_lut, px.r, px.g, px.b);
    assert(f1.l >= 0.f && f1.l <= 1.0f);
    assert(f1.A >= 0.f && f1.A <= 1.0f);
    assert(f1.b >= 0.f && f1.b <= 1.0f);

    // Compose image on coloured background to better judge dissimilarity with various backgrounds
    if (px.a < 255) {
        const float a = px.a / 255.f;
        f1.l *= a; // using premultiplied alpha
        f1.A *= a;
        f1.b *= a;

        int n = i ^ j;
        if (n & 4) {
            f1.l += 1.0 - a;
        }
        if (n & 8) {
            f1.A += 1.0 - a;
        }
        if (n & 16) {
            f1.b += 1.0 - a;
        }
    }

    return f1;
}

static void convert_image(dssim_image *img, dssim_row_callback cb, void *callback_user_data, const bool subsample_chroma)
{
    const int width = img->chan[0].width;
    const int height = img->chan[0].height;
    float *row_tmp[img->channels];

    if (subsample_chroma) {
        for(int ch = 1; ch < img->channels; ch++) {
            row_tmp[ch] = calloc(width, sizeof(row_tmp[0])); // for the callback all channels have the same width!
        }

        for(int y = 0; y < height; y++) {
        row_tmp[0] = &img->chan[0].img[width * y]; // Luma can be written directly (it's unscaled)

        cb(row_tmp, img->channels, y, width, callback_user_data);

        for(int ch = 1; ch < img->channels; ch++) { // Chroma is downsampled
            const int halfy = y * img->chan[ch].height / height;
            float *dstrow = &img->chan[ch].img[halfy * img->chan[ch].width];

                for(int x = 0; x < width; x++) {
                    dstrow[x/2] += row_tmp[ch][x] * 0.25f;
                }
            }
        }

        for(int ch = 1; ch < img->channels; ch++) {
            free(row_tmp[ch]);
        }
    } else {
        for(int y = 0; y < height; y++) {
            for(int ch = 0; ch < img->channels; ch++) {
                row_tmp[ch] = &img->chan[ch].img[width * y];
            }
            cb(row_tmp, img->channels, y, width, callback_user_data);
        }
    }
}

typedef struct {
    double gamma_lut[256];
    unsigned char **row_pointers;
} image_data;

static void convert_image_row_rgba(float *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
{
    image_data *im = (image_data*)user_data;
    dssim_rgba *const row = (dssim_rgba *)im->row_pointers[y];
    const double *const gamma_lut = im->gamma_lut;

    for (int x = 0; x < width; x++) {
        dssim_lab px = convert_pixel_rgba(gamma_lut, row[x], x, y);
        channels[0][x] = px.l;
        if (num_channels >= 3) {
            channels[1][x] = px.A;
            channels[2][x] = px.b;
        }
    }
}

static void convert_image_row_rgb(float *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
{
    image_data *im = (image_data*)user_data;
    dssim_rgb *const row = (dssim_rgb*)im->row_pointers[y];
    const double *const gamma_lut = im->gamma_lut;

    for (int x = 0; x < width; x++) {
        dssim_lab px = rgb_to_lab(gamma_lut, row[x].r, row[x].g, row[x].b);
        channels[0][x] = px.l;
        if (num_channels >= 3) {
            channels[1][x] = px.A;
            channels[2][x] = px.b;
        }
    }
}

static void convert_image_row_gray_init(double gamma_lut[static 256]) {
    for(int i=0; i < 256; i++) {
        gamma_lut[i] = rgb_to_lab(gamma_lut, i, i, i).l;
    }
}

static void convert_image_row_gray(float *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
{
    image_data *im = (image_data*)user_data;
    unsigned char *const row = im->row_pointers[y];
    const double *const gamma_lut = im->gamma_lut;

    for (int x = 0; x < width; x++) {
        channels[0][x] = gamma_lut[row[x]];
    }
}

static void copy_image_row(float *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
{
    unsigned char *row = ((unsigned char **)user_data)[y];
    for (int x = 0; x < width; x++) {
        channels[0][x] = *row++;
        if (num_channels == 3) {
            channels[1][x] = *row++;
            channels[2][x] = *row++;
        }
    }
}

/*
 Copies the image.
 */
dssim_image *dssim_create_image(unsigned char *row_pointers[], dssim_colortype color_type, const int width, const int height, const double gamma)
{
    dssim_row_callback *converter;
    int num_channels;

    image_data im;
    im.row_pointers = row_pointers;
    set_gamma(im.gamma_lut, gamma);

    switch(color_type) {
        case DSSIM_GRAY:
            convert_image_row_gray_init(im.gamma_lut);
            converter = convert_image_row_gray;
            num_channels = 1;
            break;
        case DSSIM_RGB:
            converter = convert_image_row_rgb;
            num_channels = 3;
            break;
        case DSSIM_RGBA:
            converter = convert_image_row_rgba;
            num_channels = 3;
            break;
        case DSSIM_RGBA_TO_GRAY:
            converter = convert_image_row_rgba;
            num_channels = 1;
            break;
        case DSSIM_LUMA:
            converter = copy_image_row;
            num_channels = 1;
            break;
        case DSSIM_LAB:
            converter = copy_image_row;
            num_channels = 3;
            break;
        default:
            return NULL;
    }

    return dssim_create_image_float_callback(num_channels, width, height, converter, (void*)&im);
}

dssim_image *dssim_create_image_float_callback(const int num_channels, const int width, const int height, dssim_row_callback cb, void *callback_user_data)
{
    if (num_channels != 1 && num_channels != MAX_CHANS) {
        return NULL;
    }

    const bool subsample_chroma = num_channels > 1;

    dssim_image *img = malloc(sizeof(img[0]));
    *img = (dssim_image){
        .channels = num_channels,
    };

    for (int ch = 0; ch < img->channels; ch++) {
        img->chan[ch].is_chroma = ch > 0;
        img->chan[ch].width = subsample_chroma && img->chan[ch].is_chroma ? width/2 : width;
        img->chan[ch].height = subsample_chroma && img->chan[ch].is_chroma ? height/2 : height;
        // subsampling in convert_image relies on zeroed bitmaps
        img->chan[ch].img = calloc(img->chan[ch].width * img->chan[ch].height, sizeof(img->chan[ch].img[0]));
    }

    convert_image(img, cb, callback_user_data, subsample_chroma);

    float *tmp = malloc(width * height * sizeof(tmp[0]));
    for (int ch = 0; ch < img->channels; ch++) {
        dssim_chan *const chan = &img->chan[ch];
        const bool extrablur = chan->is_chroma;
    const int width = chan->width;
    const int height = chan->height;

        if (extrablur) {
            blur(chan->img, tmp, chan->img, width, height, NULL, 0);
    }

    chan->mu = malloc(width * height * sizeof(chan->mu[0]));
        blur(chan->img, tmp, chan->mu, width, height, NULL, extrablur);

    chan->img_sq_blur = malloc(width * height * sizeof(chan->img_sq_blur[0]));
        blur(chan->img, tmp, chan->img_sq_blur, width, height, square_row, extrablur);
}
    free(tmp);

    return img;
}

static float *get_img1_img2_blur(const dssim_chan *restrict original, dssim_chan *restrict modified, float *restrict tmp)
{
    const int width = original->width;
    const int height = original->height;

    float *restrict img1 = original->img;
    float *restrict img2 = modified->img; modified->img = NULL; // img2 is turned in-place into blur(img1*img2)

    for (int j = 0; j < width*height; j++) {
        img2[j] *= img1[j];
    }

    blur(img2, tmp, img2, width, height, NULL, original->is_chroma);

    return img2;
}

static double dssim_compare_channel(const dssim_chan *restrict original, dssim_chan *restrict modified, float *restrict tmp, float **ssim_map_out);

/**
 Algorithm based on Rabah Mehdi's C++ implementation

 @param modified is destroyed after the comparison (but you still need to call dssim_dealloc_image)
 @param ssim_map_out Saves dissimilarity visualisation (pass NULL if not needed)
 @return DSSIM value or NaN on error.
 */
double dssim_compare(const dssim_image *restrict original_image, dssim_image *restrict modified_image, float **ssim_map_out)
{
    const int channels = MIN(original_image->channels, modified_image->channels);
    float *tmp = malloc(original_image->chan[0].width * original_image->chan[0].height * sizeof(tmp[0]));

    double ssim_sum = 0;
    double total = 0;
    for (int ch = 0; ch < channels; ch++) {
        const dssim_chan *original = &original_image->chan[ch];
        dssim_chan *modified = &modified_image->chan[ch];
        double weight = original->is_chroma ? COLOR_WEIGHT : 1.0;
        const bool use_ssim_map_out = ssim_map_out && ch == 0;
        ssim_sum += weight * dssim_compare_channel(original, modified, tmp, use_ssim_map_out ? ssim_map_out : NULL);
            total += weight;
            }
    free(tmp);

    return 1.0 / (ssim_sum / total) - 1.0;
}

static double dssim_compare_channel(const dssim_chan *restrict original, dssim_chan *restrict modified, float *restrict tmp, float **ssim_map_out)
{
    if (original->width != modified->width || original->height != modified->height) {
        return 0;
    }

    const int width = original->width;
    const int height = original->height;

    const float *restrict mu1 = original->mu;
    float *const mu2 = modified->mu;
    const float *restrict img1_sq_blur = original->img_sq_blur;
    const float *restrict img2_sq_blur = modified->img_sq_blur;
    float *restrict img1_img2_blur = get_img1_img2_blur(original, modified, tmp);

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
        free(modified->mu);
    }
    modified->mu = NULL;

    free(modified->img_sq_blur); modified->img_sq_blur = NULL;
    free(img1_img2_blur);

    return ssim_sum / (width * height);
}
