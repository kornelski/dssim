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
        "\nVersion 0.9 http://pornel.net/dssim\n" \
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

    dssim_attr *attr = dssim_create_attr();
    dssim_image *original = dssim_create_image(attr, image1.row_pointers, DSSIM_RGBA, image1.width, image1.height, image1.gamma);
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

        dssim_image *modified = dssim_create_image(attr, image2.row_pointers, DSSIM_RGBA, image2.width, image2.height, image2.gamma);
        free(image2.row_pointers);
        free(image2.rgba_data);

        if (map_output_file) {
            dssim_set_save_ssim_maps(attr, 1, 1);
        }

        double dssim = dssim_compare(attr, original, modified);
        dssim_dealloc_image(modified);

        printf("%.6f\t%s\n", dssim, file2);

        if (map_output_file) {
            dssim_ssim_map map_meta = dssim_pop_ssim_map(attr, 0, 0);
            float *map = map_meta.data;
            dssim_rgba *out = (dssim_rgba*)map;
            for(int i=0; i < map_meta.width*map_meta.height; i++) {
                const float max = 1.0 - map[i];
                const float maxsq = max * max;
                out[i] = (dssim_rgba) {
                    .r = to_byte(max * 3.0),
                    .g = to_byte(maxsq * 6.0),
                    .b = to_byte(max / ((1.0 - map_meta.ssim) * 4.0)),
                    .a = 255,
                };
            }
            if (write_image(map_output_file, out, map_meta.width, map_meta.height)) {
                fprintf(stderr, "Can't write %s\n", map_output_file);
                free(map);
                return 1;
            }
            free(map);
        }
    }

    dssim_dealloc_image(original);
    dssim_dealloc_attr(attr);

    return retval;
}
