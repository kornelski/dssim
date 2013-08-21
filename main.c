/*
 Copyright (c) 2011 porneL. All rights reserved.

 Redistribution and use in source and binary forms, with or without modification, are
 permitted provided that the following conditions are met:

 1. Redistributions of source code must retain the above copyright notice, this list of
    conditions and the following disclaimer.

 2. Redistributions in binary form must reproduce the above copyright notice, this list
    of conditions and the following disclaimer in the documentation and/or other materials
    provided with the distribution.

 THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
 WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
 ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
 ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
 OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
*/

#include <stdlib.h>
#include "dssim.h"

/*
 Reads image into png24_image struct. Returns non-zero on error
 */
static int read_image(const char *filename, png24_image *image)
{
    FILE *fp = fopen(filename, "rb");
    if (!fp) {
        return 1;
    }

    int retval = rwpng_read_image24(fp, image);

    fclose(fp);
    return retval;
}

static void write_image(const char *filename,
                        const rgba8 *pixels,
                        int width,
                        int height)
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
    png_write_info(png_ptr, info_ptr);

    for (int i = 0; i < height; i++) {
        png_write_row(png_ptr, (png_bytep)(pixels + i * width));
    }

    png_write_end(png_ptr, info_ptr);
    png_destroy_write_struct(&png_ptr, &info_ptr);
}

static void usage(const char *argv0)
{
    fprintf(stderr, "Usage: %s original.png modified.png [modified.png...]\n\n" \
            "Compares first image against subsequent images,\n" \
            "outputs SSIM difference for each of them in order.\n" \
            "Images must have identical size. May have different gamma & depth.\n" \
            "\nVersion 0.2 http://pornel.net/dssim\n" \
            , argv0);
}

inline static unsigned char to_byte(float in) {
    if (in <= 0) return 0;
    if (in >= 255.f/256.f) return 255;
    return in * 256.f;
}

int main(int argc, const char *argv[])
{
    if (argc < 3) {
        usage(argv[0]);
        exit(1);
    }

    const char *file1 = argv[1];
    png24_image image1 = {};
    int retval = read_image(file1, &image1);
    if (retval) {
        fprintf(stderr, "Can't read %s\n", file1);
        return retval;
    }

    dssim_info *dinf = dssim_init();
    dssim_set_original(dinf, &image1);
    free(image1.row_pointers);
    free(image1.rgba_data);

    for (int arg = 2; arg < argc; arg++) {
        const char *file2 = argv[arg];

        png24_image image2 = {};
        retval = read_image(file2, &image2);
        if (retval) {
            fprintf(stderr, "Can't read %s\n", file2);
            break;
        }

        retval = dssim_set_modified(dinf, &image2);
        free(image2.row_pointers);
        free(image2.rgba_data);

        if (retval) {
            fprintf(stderr, "Image %s has different size than %s\n", file2, file1);
            break;
        }

        float *map = NULL;
        double dssim = dssim_compare(dinf, NULL);
        if (map) {
            rgba8 *out = (rgba8*)map;
            for(int i=0; i < image2.width*image2.height; i++) {
                const float max = 1.0 - map[i];
                const float maxsq = max * max;
                out[i] = (rgba8) {
                    .r = to_byte(max * 3.0),
                    .g = to_byte(maxsq * 3.0),
                    .b = to_byte((max-0.5) * 2.0f),
                    .a = 255,
                };
            }
            write_image("/tmp/dssim-map.png", out, image2.width, image2.height);
        }
        printf("%.6f\t%s\n", dssim, file2);
    }

    dssim_dealloc(dinf);
    return retval;
}
