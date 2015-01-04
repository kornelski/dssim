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
#include "rwpng.h"

#include <getopt.h>
extern char *optarg;
extern int optind, opterr;

/*
 Reads image into png24_image struct. Returns non-zero on error
 */
static int read_image(const char *filename, png24_image *image)
{
    FILE *fp = fopen(filename, "rb");
    if (!fp) {
        return 1;
    }

    int retval = rwpng_read_image24(fp, image, 0);

    fclose(fp);
    return retval;
}

static int write_image(const char *filename,
                        const dssim_rgba *pixels,
                        int width,
                        int height)
{
    FILE *outfile = fopen(filename, "wb");
    if (!outfile) {
        return 1;
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

    return 0;
}

static void usage(const char *argv0)
{
    fprintf(stderr,
        "Usage: %s original.png modified.png [modified.png...]\n" \
        "   or: %s -o difference.png original.png modified.png\n\n" \
        "Compares first image against subsequent images, and outputs\n" \
        "1/SSIM-1 difference for each of them in order (0 = identical).\n\n" \
        "Images must have identical size, but may have different gamma & depth.\n" \
        "\nVersion 0.8 http://pornel.net/dssim\n" \
        , argv0, argv0);
}

inline static unsigned char to_byte(float in) {
    if (in <= 0) return 0;
    if (in >= 255.f/256.f) return 255;
    return in * 256.f;
}

int main(int argc, char *const argv[])
{
    char *map_output_file = NULL;

    if (argc < 3) {
        usage(argv[0]);
        return 1;
    }

    opterr = 0;
    int c;
    while((c = getopt(argc, argv, "ho:")) != -1) {
        switch (c) {
            case 'h':
                usage(argv[0]);
                return 0;
            case 'o':
                map_output_file = optarg;
                break;
            default:
                fprintf(stderr, "Unknown option\n");
                return 1;
        }
    }

    if (optind+1 >= argc) {
        fprintf(stderr, "You must specify at least 2 files to compare\n");
        return 1;
    }

    const char *file1 = argv[optind];
    png24_image image1 = {};
    int retval = read_image(file1, &image1);
    if (retval) {
        fprintf(stderr, "Can't read %s\n", file1);
        return retval;
    }

    dssim_image *original = dssim_create_image(image1.row_pointers, DSSIM_RGBA, image1.width, image1.height, image1.gamma);
    free(image1.row_pointers);
    free(image1.rgba_data);

    for (int arg = optind+1; arg < argc; arg++) {
        const char *file2 = argv[arg];

        png24_image image2 = {};
        retval = read_image(file2, &image2);
        if (retval) {
            fprintf(stderr, "Can't read %s\n", file2);
            break;
        }

        if (image1.width != image2.width || image1.height != image2.height) {
            fprintf(stderr, "Image %s has different size than %s\n", file2, file1);
            break;
        }

        dssim_image *modified = dssim_create_image(image2.row_pointers, DSSIM_RGBA, image2.width, image2.height, image2.gamma);
        free(image2.row_pointers);
        free(image2.rgba_data);

        float *map = NULL;
        double dssim = dssim_compare(original, modified, map_output_file ? &map : NULL);
        dssim_dealloc_image(modified);

        printf("%.6f\t%s\n", dssim, file2);

        if (map) {
            dssim_rgba *out = (dssim_rgba*)map;
            for(int i=0; i < image2.width*image2.height; i++) {
                const float max = 1.0 - map[i];
                const float maxsq = max * max;
                out[i] = (dssim_rgba) {
                    .r = to_byte(max * 3.0),
                    .g = to_byte(maxsq * 3.0),
                    .b = to_byte((max-0.5) * 2.0f),
                    .a = 255,
                };
            }
            if (write_image(map_output_file, out, image2.width, image2.height)) {
                fprintf(stderr, "Can't write %s\n", map_output_file);
                free(map);
                return 1;
            }
            free(map);
        }
    }

    dssim_dealloc_image(original);
    return retval;
}
