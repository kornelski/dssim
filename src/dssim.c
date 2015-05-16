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

#define MAX_CHANS 3
#define MAX_SCALES 5

typedef struct {
    dssim_px_t l, A, b;
} dssim_lab;

typedef struct {
    unsigned char r, g, b, a;
} dssim_rgb;

struct dssim_chan;
typedef struct dssim_chan dssim_chan;
struct dssim_chan {
    int width, height;
    dssim_px_t *img, *mu, *img_sq_blur;
    dssim_chan *next_half;
    int blur_size;
    bool is_chroma;
};

struct dssim_image {
    dssim_chan *chan[MAX_CHANS];
    int channels;
};

struct dssim_attr {
    dssim_px_t *tmp;
    size_t tmp_size;
    double color_weight;
    double scale_weights[MAX_SCALES];
    int num_scales;
    int detail_size;
    bool subsample_chroma;
    int save_maps_scales, save_maps_channels;
    dssim_ssim_map ssim_maps[MAX_SCALES][MAX_CHANS];
};

/* Scales are taken from IW-SSIM, but this is not IW-SSIM algorithm */
static const double default_weights[] = {0.0448, 0.2856, 0.3001, 0.2363, 0.1333};

dssim_attr *dssim_create_attr(void) {
    dssim_attr *attr = malloc(sizeof(attr[0]));
    *attr = (dssim_attr){
        /* Bigger number puts more emphasis on color channels. */
        .color_weight = 0.95,
        /* Smaller values are more sensitive to single-pixel differences. Increase for high-DPI images? */
        .detail_size = 1,
        .subsample_chroma = true,
    };

    /* Further scales test larger changes */
    dssim_set_scales(attr, 4, NULL);
    return attr;
}

void dssim_dealloc_attr(dssim_attr *attr) {
    for(int n = 0; n < MAX_SCALES; n++) {
        for(int ch = 0; ch < MAX_CHANS; ch++) {
            free(attr->ssim_maps[n][ch].data);
        }
    }
    free(attr->tmp);
    free(attr);
}

void dssim_set_scales(dssim_attr *attr, const int num, const double *weights) {
    attr->num_scales = MIN(MAX_SCALES, num);
    if (!weights) {
        weights = default_weights;
    }

    double sum = 0;
    for(int i=0; i < attr->num_scales; i++) {
        attr->scale_weights[i] = weights[i];
        sum += weights[i];
    }
    // Weights must add up to 1
    for(int i=0; i < attr->num_scales; i++) {
        attr->scale_weights[i] /= sum;
    }
}

void dssim_set_color_handling(dssim_attr *attr, int subsample_chroma, double color_weight) {
    attr->subsample_chroma = !!subsample_chroma;
    attr->color_weight = color_weight;
}

void dssim_set_save_ssim_maps(dssim_attr *attr, unsigned int scales, unsigned int channels) {
    attr->save_maps_scales = scales;
    attr->save_maps_channels = channels;
}

dssim_ssim_map dssim_pop_ssim_map(dssim_attr *attr, unsigned int scale_index, unsigned int channel_index) {
    if (scale_index >= MAX_SCALES || channel_index >= MAX_CHANS) {
        return (dssim_ssim_map){};
    }
    const dssim_ssim_map t = attr->ssim_maps[scale_index][channel_index];
    attr->ssim_maps[scale_index][channel_index].data = NULL;
    return t;
}

static dssim_px_t *dssim_get_tmp(dssim_attr *attr, size_t size) {
    if (attr->tmp) {
        if (size <= attr->tmp_size) {
            return attr->tmp;
        }
        free(attr->tmp);
    }
    attr->tmp = malloc(size);
    attr->tmp_size = size;
    return attr->tmp;
}

static void dealloc_chan(dssim_chan *chan) {
    free(chan->img);
    free(chan->mu);
    free(chan->img_sq_blur);
    if (chan->next_half) {
        dealloc_chan(chan->next_half);
    }
    free(chan);
}

void dssim_dealloc_image(dssim_image *img)
{
    for (int ch = 0; ch < img->channels; ch++) {
        dealloc_chan(img->chan[ch]);
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
    const dssim_px_t X = (fx > epsilon) ? powf(fx, 1.f / 3.f) - 16.f/116.f : k * fx;
    const dssim_px_t Y = (fy > epsilon) ? powf(fy, 1.f / 3.f) - 16.f/116.f : k * fy;
    const dssim_px_t Z = (fz > epsilon) ? powf(fz, 1.f / 3.f) - 16.f/116.f : k * fz;

    return (dssim_lab) {
        Y * 1.16f,
        (86.2f/ 220.0f + 500.0f/ 220.0f * (X - Y)), /* 86 is a fudge to make the value positive */
        (107.9f/ 220.0f + 200.0f/ 220.0f * (Y - Z)), /* 107 is a fudge to make the value positive */
    };
}

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

static void regular_1d_blur(const dssim_px_t *src, dssim_px_t *restrict tmp1, dssim_px_t *dst, const int width, const int height, const int runs)
{
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


/*
 * blurs (approximate of gaussian)
 */
static void blur(const dssim_px_t *restrict src, dssim_px_t *restrict tmp, dssim_px_t *restrict dst,
                 const int width, const int height, int size)
{
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
    regular_1d_blur(src, tmp, dst, width, height, size);
    transpose(dst, tmp, width, height);

    // After transposing buffer is rotated, so height and width are swapped
    // And reuse of buffers made tmp hold the image, and dst used as temporary until the last transpose
    regular_1d_blur(tmp, dst, tmp, height, width, size);
    transpose(tmp, dst, height, width);
#endif
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
        const dssim_px_t a = px.a / 255.f;
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

/* copy number of rows from a 2x larger image */
static void subsampled_copy(dssim_chan *new_chan, const int dest_y_offset, const int rows, const dssim_px_t *src_img, const int src_width)
{
    for(int y = 0; y < rows; y++) {
        for(int x = 0; x < new_chan->width; x++) {
            new_chan->img[x + (y + dest_y_offset) * new_chan->width] = 0.25 * (
                src_img[x*2 + y*2 * src_width] + src_img[x*2+1 + y*2 * src_width] +
                src_img[x*2 + (y*2+1) * src_width] + src_img[x*2+1 + (y*2+1) * src_width]
            );
        }
    }
}

static void convert_image(dssim_image *img, dssim_row_callback cb, void *callback_user_data, const bool subsample_chroma)
{
    const int width = img->chan[0]->width;
    const int height = img->chan[0]->height;
    dssim_px_t *row_tmp[img->channels];
    dssim_px_t *row_tmp2[img->channels];

    if (subsample_chroma && img->channels > 1) {
        for(int ch = 1; ch < img->channels; ch++) {
            row_tmp[ch] = calloc(width*2, sizeof(row_tmp[0])); // for the callback all channels have the same width!
            row_tmp2[ch] = row_tmp[ch] + width;
        }

        for(int y = 0; y < height; y++) {
            row_tmp[0] = &img->chan[0]->img[width * y]; // Luma can be written directly (it's unscaled)
            row_tmp2[0] = &img->chan[0]->img[width * y]; // Luma can be written directly (it's unscaled)

            cb(y&1 ? row_tmp2 : row_tmp, img->channels, y, width, callback_user_data);

            if (y & 1) {
        for(int ch = 1; ch < img->channels; ch++) { // Chroma is downsampled
                    subsampled_copy(img->chan[ch], y/2, 1, row_tmp[ch], width);
                }
            }
        }

        for(int ch = 1; ch < img->channels; ch++) {
            free(row_tmp[ch]);
        }
    } else {
        for(int y = 0; y < height; y++) {
            for(int ch = 0; ch < img->channels; ch++) {
                row_tmp[ch] = &img->chan[ch]->img[width * y];
            }
            cb(row_tmp, img->channels, y, width, callback_user_data);
        }
    }
}

typedef struct {
    double gamma_lut[256];
    const unsigned char *const *const row_pointers;
} image_data;

static void convert_image_row_rgba(dssim_px_t *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
{
    image_data *im = (image_data*)user_data;
    const dssim_rgba *const row = (dssim_rgba *)im->row_pointers[y];
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

static void convert_image_row_rgb(dssim_px_t *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
{
    image_data *im = (image_data*)user_data;
    const dssim_rgb *const row = (dssim_rgb *)im->row_pointers[y];
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

static void convert_image_row_gray(dssim_px_t *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
{
    image_data *im = (image_data*)user_data;
    const unsigned char *row = im->row_pointers[y];
    const double *const gamma_lut = im->gamma_lut;

    for (int x = 0; x < width; x++) {
        channels[0][x] = gamma_lut[row[x]];
    }
}

static void copy_image_row(dssim_px_t *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
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
dssim_image *dssim_create_image(dssim_attr *attr, unsigned char *const *const row_pointers, dssim_colortype color_type, const int width, const int height, const double gamma)
{
    dssim_row_callback *converter;
    int num_channels;

    image_data im = {
        .row_pointers = (const unsigned char *const *const )row_pointers,
    };
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

    return dssim_create_image_float_callback(attr, num_channels, width, height, converter, (void*)&im);
}

dssim_chan *create_chan(const int width, const int height, const int blur_size, const bool is_chroma) {
    assert(width > 0 && height > 0 && blur_size > 0);

    dssim_chan *const chan = malloc(sizeof(chan[0]));
    *chan = (dssim_chan){
        .width = width,
        .height = height,
        .blur_size = blur_size,
        .is_chroma = is_chroma,
        .img = malloc(width * height * sizeof(chan->img[0])),
    };
    return chan;
}

static void dssim_preprocess_channel(dssim_chan *chan, dssim_px_t *tmp, int depth);

dssim_image *dssim_create_image_float_callback(dssim_attr *attr, const int num_channels, const int width, const int height, dssim_row_callback cb, void *callback_user_data)
{
    if (num_channels != 1 && num_channels != MAX_CHANS) {
        return NULL;
    }

    const bool subsample_chroma = (width >= 8 && height >= 8) ? attr->subsample_chroma : false;

    dssim_image *img = malloc(sizeof(img[0]));
    *img = (dssim_image){
        .channels = num_channels,
    };

    for (int ch = 0; ch < img->channels; ch++) {
        const bool is_chroma = ch > 0;
        img->chan[ch] = create_chan(
            subsample_chroma && is_chroma ? width/2 : width,
            subsample_chroma && is_chroma ? height/2 : height,
            (attr->detail_size + 1),
            is_chroma);
    }

    convert_image(img, cb, callback_user_data, subsample_chroma);

    dssim_px_t *tmp = dssim_get_tmp(attr, width * height * sizeof(tmp[0]));
    for (int ch = 0; ch < img->channels; ch++) {
        dssim_preprocess_channel(img->chan[ch], tmp, attr->num_scales);
    }

    return img;
}

static void dssim_preprocess_channel(dssim_chan *chan, dssim_px_t *tmp, int num_scales)
{
    const int width = chan->width;
    const int height = chan->height;

    if (num_scales > 1 && chan->width >= 8 && chan->height >= 8) {
        dssim_chan *new_chan = create_chan(chan->width/2, chan->height/2, chan->blur_size, chan->is_chroma);
        chan->next_half = new_chan;
        subsampled_copy(new_chan, 0, new_chan->height, chan->img, chan->width);
        dssim_preprocess_channel(chan->next_half, tmp, num_scales-1);
    }

    if (chan->is_chroma) {
        blur(chan->img, tmp, chan->img, width, height, 2);
    }

    chan->mu = malloc(width * height * sizeof(chan->mu[0]));
    blur(chan->img, tmp, chan->mu, width, height, chan->blur_size);

    chan->img_sq_blur = malloc(width * height * sizeof(chan->img_sq_blur[0]));
    for(int i=0; i < width*height; i++) {
        chan->img_sq_blur[i] = chan->img[i] * chan->img[i];
    }
    blur(chan->img_sq_blur, tmp, chan->img_sq_blur, width, height, chan->blur_size);
}

static dssim_px_t *get_img1_img2_blur(const dssim_chan *restrict original, dssim_chan *restrict modified, dssim_px_t *restrict tmp)
{
    const int width = original->width;
    const int height = original->height;

    dssim_px_t *restrict img1 = original->img;
    dssim_px_t *restrict img2 = modified->img; modified->img = NULL; // img2 is turned in-place into blur(img1*img2)

    for (int j = 0; j < width*height; j++) {
        img2[j] *= img1[j];
    }

    blur(img2, tmp, img2, width, height, original->blur_size);

    return img2;
}

static double to_dssim(double ssim) {
    assert(ssim > 0);
    return 1.0 / MIN(1.0, ssim) - 1.0;
}

static double dssim_compare_channel(const dssim_chan *restrict original, dssim_chan *restrict modified, dssim_px_t *restrict tmp, dssim_ssim_map *ssim_map_out, bool save_ssim_map);

/**
 Algorithm based on Rabah Mehdi's C++ implementation

 @param modified is destroyed after the comparison (but you still need to call dssim_dealloc_image)
 @param ssim_map_out Saves dissimilarity visualisation (pass NULL if not needed)
 @return DSSIM value or NaN on error.
 */
double dssim_compare(dssim_attr *attr, const dssim_image *restrict original_image, dssim_image *restrict modified_image)
{
    const int channels = MIN(original_image->channels, modified_image->channels);
    dssim_px_t *tmp = dssim_get_tmp(attr, original_image->chan[0]->width * original_image->chan[0]->height * sizeof(tmp[0]));

    double ssim_sum = 0;
    double total = 0;
    for (int ch = 0; ch < channels; ch++) {

        const dssim_chan *original = original_image->chan[ch];
        dssim_chan *modified = modified_image->chan[ch];

        for(int n=0; n < attr->num_scales; n++) {
            const double weight = (original->is_chroma ? attr->color_weight : 1.0) * attr->scale_weights[n];

            const bool save_maps = attr->save_maps_scales > n && attr->save_maps_channels > ch;
            if (attr->ssim_maps[n][ch].data) {
                free(attr->ssim_maps[n][ch].data); // prevent a leak, since ssim_map will always be overwritten
                attr->ssim_maps[n][ch].data = NULL;
            }
            ssim_sum += weight * dssim_compare_channel(original, modified, tmp, &attr->ssim_maps[n][ch], save_maps);
            total += weight;
            original = original->next_half;
            modified = modified->next_half;
            if (!original || !modified) {
                break;
            }
        }
    }

    return to_dssim(ssim_sum / total);
}

static double dssim_compare_channel(const dssim_chan *restrict original, dssim_chan *restrict modified, dssim_px_t *restrict tmp, dssim_ssim_map *ssim_map_out, bool save_ssim_map)
{
    if (original->width != modified->width || original->height != modified->height) {
        return 0;
    }

    const int width = original->width;
    const int height = original->height;

    const dssim_px_t *restrict mu1 = original->mu;
    dssim_px_t *const mu2 = modified->mu;
    const dssim_px_t *restrict img1_sq_blur = original->img_sq_blur;
    const dssim_px_t *restrict img2_sq_blur = modified->img_sq_blur;
    dssim_px_t *restrict img1_img2_blur = get_img1_img2_blur(original, modified, tmp);

    const double c1 = 0.01 * 0.01, c2 = 0.03 * 0.03;
    double ssim_sum = 0;

    dssim_px_t *const ssimmap = save_ssim_map ? mu2 : NULL;

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

    if (!save_ssim_map) { // reuses mu2 memory
        free(modified->mu);
    }
    *ssim_map_out = (dssim_ssim_map){
        .width = width,
        .height = height,
        .dssim = to_dssim(ssim_sum / (width * height)),
        .data = ssimmap,
    };

    modified->mu = NULL;

    free(modified->img_sq_blur); modified->img_sq_blur = NULL;
    free(img1_img2_blur);

    return ssim_sum / (width * height);
}
