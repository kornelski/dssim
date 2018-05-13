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
    unsigned char r, g, b;
} dssim_rgb;

typedef struct {
    dssim_px_t r, g, b, a; // premultiplied
} linear_rgba;

struct dssim_chan;
typedef struct dssim_chan dssim_chan;
struct dssim_chan {
    int width, height;
    dssim_px_t *img, *mu, *img_sq_blur;
    bool is_chroma;
};

typedef struct dssim_image_chan {
    dssim_chan scales[MAX_SCALES];
    int num_scales;
} dssim_image_chan;

struct dssim_image {
    dssim_image_chan chan[MAX_CHANS];
    int num_channels;
};

struct dssim_ssim_map_chan {
    dssim_ssim_map scales[MAX_SCALES];
};

struct dssim_attr {
    dssim_px_t *tmp;
    size_t tmp_size;
    double color_weight;
    double scale_weights[MAX_SCALES];
    int num_scales;
    bool subsample_chroma;
    int save_maps_scales, save_maps_channels;
    struct dssim_ssim_map_chan ssim_maps[MAX_CHANS];
};

/* Scales are taken from IW-SSIM, but this is not IW-SSIM algorithm */
static const double default_weights[] = {0.0448, 0.2856, 0.3001, 0.2363, 0.1333};

dssim_attr *dssim_create_attr(void) {
    dssim_attr *attr = malloc(sizeof(attr[0]));
    *attr = (dssim_attr){
        /* Bigger number puts more emphasis on color channels. */
        .color_weight = 0.95,
        .subsample_chroma = true,
    };

    /* Further scales test larger changes */
    dssim_set_scales(attr, 4, NULL);
    return attr;
}

void dssim_dealloc_attr(dssim_attr *attr) {
    for(int ch = 0; ch < MAX_CHANS; ch++) {
        for(int n = 0; n < MAX_SCALES; n++) {
            free(attr->ssim_maps[ch].scales[n].data);
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

    for(int i=0; i < attr->num_scales; i++) {
        attr->scale_weights[i] = weights[i];
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
    const dssim_ssim_map t = attr->ssim_maps[channel_index].scales[scale_index];
    attr->ssim_maps[channel_index].scales[scale_index].data = NULL;
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
}

void dssim_dealloc_image(dssim_image *img)
{
    for (int ch = 0; ch < img->num_channels; ch++) {
        for (int s = 0; s < img->chan[ch].num_scales; s++) {
            dealloc_chan(&img->chan[ch].scales[s]);
        }
    }
    free(img);
}

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
    assert(src);
    assert(tmp1);
    assert(dst);
    assert(width > 4);
    assert(height > 4);

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
static void blur(const dssim_px_t *restrict src, dssim_px_t *restrict tmp, dssim_px_t *dst,
                 const int width, const int height)
{
    assert(src);
    assert(dst);
    assert(tmp);
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

static void convert_image_subsampled(dssim_image *img, dssim_row_callback cb, void *callback_user_data)
{
    dssim_chan *chan = &img->chan[0].scales[0];
    const int width = chan->width;
    const int height = chan->height;
    dssim_px_t *row_tmp0[img->num_channels];
    dssim_px_t *row_tmp1[img->num_channels];

    for(int ch = 1; ch < img->num_channels; ch++) {
        row_tmp0[ch] = calloc(width*2, sizeof(row_tmp0[0])); // for the callback all channels have the same width!
        row_tmp1[ch] = row_tmp0[ch] + width;
    }

    for(int y = 0; y < height; y += 2) {
        row_tmp0[0] = &chan->img[width * y]; // Luma can be written directly (it's unscaled)
        row_tmp1[0] = &chan->img[width * MIN(height-1, y+1)];

        cb(row_tmp0, img->num_channels, y, width, callback_user_data);
        cb(row_tmp1, img->num_channels, MIN(height-1, y+1), width, callback_user_data);

        if (y < height-1) {
            for(int ch = 1; ch < img->num_channels; ch++) { // Chroma is downsampled
                subsampled_copy(&img->chan[ch].scales[0], y/2, 1, row_tmp0[ch], width);
            }
        }
    }

    for(int ch = 1; ch < img->num_channels; ch++) {
        free(row_tmp0[ch]);
    }
}

static void convert_image_simple(dssim_image *img, dssim_row_callback cb, void *callback_user_data)
{
    dssim_chan *chan = &img->chan[0].scales[0];
    const int width = chan->width;
    const int height = chan->height;
    dssim_px_t *row_tmp[img->num_channels];

    for(int y = 0; y < height; y++) {
        for(int ch = 0; ch < img->num_channels; ch++) {
            row_tmp[ch] = &img->chan[ch].scales[0].img[width * y];
        }
        cb(row_tmp, img->num_channels, y, width, callback_user_data);
    }
}

typedef struct {
    dssim_px_t gamma_lut[256];
    const unsigned char *const *const row_pointers;
} image_data;

static void convert_image_row_rgba(dssim_px_t *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
{
    image_data *im = (image_data*)user_data;
    const dssim_rgba *const row = (dssim_rgba *)im->row_pointers[y];
    const dssim_px_t *const gamma_lut = im->gamma_lut;

    for (int x = 0; x < width; x++) {
        const linear_rgba rgba = rgb_to_linear(gamma_lut, row[x].r, row[x].g, row[x].b, row[x].a);
        const dssim_lab px = convert_pixel_rgba(rgba, x, y);
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
    const dssim_px_t *const gamma_lut = im->gamma_lut;

    for (int x = 0; x < width; x++) {
        dssim_lab px = rgb_to_lab(gamma_lut[row[x].r], gamma_lut[row[x].g], gamma_lut[row[x].b]);
        channels[0][x] = px.l;
        if (num_channels >= 3) {
            channels[1][x] = px.A;
            channels[2][x] = px.b;
        }
    }
}

static void convert_image_row_gray_init(dssim_px_t gamma_lut[static 256]) {
    for(int i=0; i < 256; i++) {
        gamma_lut[i] = rgb_to_lab(gamma_lut[i], gamma_lut[i], gamma_lut[i]).l;
    }
}

static void convert_image_row_gray(dssim_px_t *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
{
    image_data *im = (image_data*)user_data;
    const unsigned char *row = im->row_pointers[y];
    const dssim_px_t *const luma_lut = im->gamma_lut; // init converts it

    for (int x = 0; x < width; x++) {
        channels[0][x] = luma_lut[row[x]];
    }
}

static void convert_u8_to_float(dssim_px_t *const restrict channels[], const int num_channels, const int y, const int width, void *user_data)
{
    unsigned char *row = ((unsigned char **)user_data)[y];
    for (int x = 0; x < width; x++) {
        channels[0][x] = (*row++) / 255.f;
        if (num_channels == 3) {
            channels[1][x] = (*row++) / 255.f;
            channels[2][x] = (*row++) / 255.f;
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

    if (!set_gamma(im.gamma_lut, gamma)) {
        return NULL;
    }

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
            converter = convert_u8_to_float;
            num_channels = 1;
            break;
        case DSSIM_LAB:
            converter = convert_u8_to_float;
            num_channels = 3;
            break;
        default:
            return NULL;
    }

    return dssim_create_image_float_callback(attr, num_channels, width, height, converter, (void*)&im);
}

static void dssim_preprocess_channel(dssim_chan *chan, dssim_px_t *tmp);

dssim_image *dssim_create_image_float_callback(dssim_attr *attr, const int num_channels, const int width, const int height, dssim_row_callback cb, void *callback_user_data)
{
    if (num_channels != 1 && num_channels != MAX_CHANS) {
        return NULL;
    }

    const bool subsample_chroma = (width >= 8 && height >= 8) ? attr->subsample_chroma : false;

    dssim_image *img = malloc(sizeof(img[0]));
    *img = (dssim_image){
        .num_channels = num_channels,
    };

    for (int ch = 0; ch < img->num_channels; ch++) {
        const bool is_chroma = ch > 0;
        int chan_width = subsample_chroma && is_chroma ? width/2 : width;
        int chan_height = subsample_chroma && is_chroma ? height/2 : height;
        int s = 0;
        for(; s < attr->num_scales && chan_width >= 8 && chan_height >= 8; s++, chan_width /= 2, chan_height /= 2) {
            img->chan[ch].scales[s] = (dssim_chan){
                .width = chan_width,
                .height = chan_height,
                .is_chroma = is_chroma,
                .img = malloc(chan_width * chan_height * sizeof(img->chan[ch].scales[s].img[0])),
            };
        }
        img->chan[ch].num_scales = s;
    }

    for (int ch = 0; ch < img->num_channels; ch++) {
        for (int s = 0; s < img->chan[ch].num_scales; s++) {
            assert(img->chan[ch].scales[s].img);
        }
    }

    if (subsample_chroma && img->num_channels > 1) {
        convert_image_subsampled(img, cb, callback_user_data);
    } else {
        convert_image_simple(img, cb, callback_user_data);
    }


    for (int ch = 0; ch < img->num_channels; ch++) {
        for (int s = 0; s < img->chan[ch].num_scales; s++) {
            assert(img->chan[ch].scales[s].img);
        }
    }

    dssim_px_t *tmp = dssim_get_tmp(attr, width * height * sizeof(tmp[0]));
    for (int ch = 0; ch < img->num_channels; ch++) {
        const dssim_chan *prev_chan = &img->chan[ch].scales[0];
        for (int s = 1; s < img->chan[ch].num_scales; s++) {
            dssim_chan *new_chan = &img->chan[ch].scales[s];
            subsampled_copy(new_chan, 0, new_chan->height, prev_chan->img, prev_chan->width);
            prev_chan = new_chan;
        }
        for (int s = 0; s < img->chan[ch].num_scales; s++) {
            dssim_preprocess_channel(&img->chan[ch].scales[s], tmp);
        }
    }

    return img;
}

static void dssim_preprocess_channel(dssim_chan *chan, dssim_px_t *tmp)
{
    assert(chan);
    assert(tmp);
    assert(chan->img);
    assert(!chan->mu);
    assert(!chan->img_sq_blur);
    const int width = chan->width;
    const int height = chan->height;

    if (chan->is_chroma) {
        blur(chan->img, tmp, chan->img, width, height);
    }

    chan->mu = malloc(width * height * sizeof(chan->mu[0]));
    blur(chan->img, tmp, chan->mu, width, height);

    chan->img_sq_blur = malloc(width * height * sizeof(chan->img_sq_blur[0]));
    for(int i=0; i < width*height; i++) {
        chan->img_sq_blur[i] = chan->img[i] * chan->img[i];
    }
    blur(chan->img_sq_blur, tmp, chan->img_sq_blur, width, height);
}

static dssim_px_t *get_img1_img2_blur(const dssim_chan *restrict original, dssim_chan *restrict modified, dssim_px_t *restrict tmp)
{
    const int width = original->width;
    const int height = original->height;

    dssim_px_t *restrict img1 = original->img;
    dssim_px_t *restrict img2 = modified->img; modified->img = NULL; // img2 is turned in-place into blur(img1*img2)

    assert(img1);
    assert(img2);

    for (int j = 0; j < width*height; j++) {
        img2[j] *= img1[j];
    }

    blur(img2, tmp, img2, width, height);

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
    assert(attr);
    assert(original_image);
    assert(modified_image);

    const int channels = MIN(original_image->num_channels, modified_image->num_channels);
    assert(channels > 0);

    dssim_px_t *tmp = dssim_get_tmp(attr, original_image->chan[0].scales[0].width * original_image->chan[0].scales[0].height * sizeof(tmp[0]));
    assert(tmp);

    double ssim_sum = 0;
    double weight_sum = 0;
    for (int ch = 0; ch < channels; ch++) {

        const dssim_image_chan *original_scales = &original_image->chan[ch];
        dssim_image_chan *modified_scales = &modified_image->chan[ch];

        int num_scales = MIN(original_scales->num_scales, modified_scales->num_scales);

        for(int n=0; n < num_scales; n++) {
            const dssim_chan *original = &original_scales->scales[n];
            dssim_chan *modified = &modified_scales->scales[n];

            const double weight = (original->is_chroma ? attr->color_weight : 1.0) * attr->scale_weights[n];

            const bool save_maps = attr->save_maps_scales > n && attr->save_maps_channels > ch;
            if (attr->ssim_maps[ch].scales[n].data) {
                free(attr->ssim_maps[ch].scales[n].data); // prevent a leak, since ssim_map will always be overwritten
                attr->ssim_maps[ch].scales[n].data = NULL;
            }
            assert(original);
            assert(modified);
            ssim_sum += weight * dssim_compare_channel(original, modified, tmp, &attr->ssim_maps[ch].scales[n], save_maps);
            weight_sum += weight;
        }
    }

    return to_dssim(ssim_sum / weight_sum);
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

    assert(mu1);
    assert(mu2);
    assert(img1_sq_blur);
    assert(img2_sq_blur);

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
