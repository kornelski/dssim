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
#include <string.h>


#include "dssim.h"
#include "rwpng.h"
#include "png.h"

#include <getopt.h>
extern char *optarg;
extern int optind, opterr;


#ifdef USE_LIBJPEG
#include <jpeglib.h>
static int read_image_jpeg(FILE *file, png24_image *image)
{
    int retval;
    unsigned int width,height,row_stride;
    unsigned int bpp;
    unsigned int x,y,i;
    struct jpeg_decompress_struct cinfo;
    struct jpeg_error_mgr jerr;
    cinfo.err = jpeg_std_error(&jerr);
    jpeg_create_decompress(&cinfo);

    // decompress
    jpeg_stdio_src(&cinfo,file);
    retval=jpeg_read_header(&cinfo,TRUE);
    if(retval != 1) {
        return 1;
    }
    cinfo.out_color_space = JCS_RGB;
    jpeg_start_decompress(&cinfo);

    width=cinfo.output_width;
    height=cinfo.output_height;
    bpp=cinfo.output_components;
    row_stride=width*bpp;

    // grayscale images not handled currently
    if (bpp == 1) {
        fprintf(stderr, "Error: grayscale JPEG images not handled currently.");
        return 1;
    }

    if (bpp != 3) {
        fprintf(stderr, "Error: Unsupported number of channels (%i) given. ", bpp);
        return 1;
    }

    // allocate buffer size (always use RGBA)
    unsigned char* buffer = calloc(width*height*bpp,1);

    while(cinfo.output_scanline < height) {
        unsigned char *buffer_array[1];
        buffer_array[0] = buffer + cinfo.output_scanline*row_stride;
        jpeg_read_scanlines(&cinfo,buffer_array,1);
    }

    //convert to RGBA
    image->rgba_data = calloc(width*height*4,1);
    image->row_pointers = calloc(height,sizeof(unsigned char*));

    for(y=0;y<height;y++)
    {
        for(x=0;x<width;x++)
        {
            for(i=0;i<bpp;i++)
            {
                image->rgba_data[(y*width*4)+(x*4)+i] = buffer[(y*width*bpp)+(x*bpp)+i];
            }
            // default alpha 255
            image->rgba_data[(y*width*4) + (x*4) + 3] = 0xFF;
        }
        image->row_pointers[y] = &image->rgba_data[y*width*4];
    }
    image->width=width;
    image->height=height;
    jpeg_destroy_decompress(&cinfo);
    free(buffer);
    return 0;
}
#endif // #ifdef USE_LIBJPEG

static int is_png(FILE *fh) {
#if defined(USE_COCOA) || !defined(USE_LIBJPEG)
    return 1;
#else
    int c = fgetc(fh);
    ungetc(c, fh);
    return c == 89;
#endif
}

static int read_image(const char *filename, png24_image *image)
{
    int retval=1;
    bool using_stdin = false;
    FILE *fh;

    if (0 == strcmp("-", filename)) {
        using_stdin = true;
        fh = stdin;
    } else {
        fh = fopen(filename, "rb");
        if (!fh) {
            return 1;
        }
    }

    // the png number is not really precise but I guess the situation where this would falsely pass is almost equal to 0
    if (is_png(fh)) {
        /*
         Reads image into png24_image struct. Returns non-zero on error
         */
        retval = rwpng_read_image24(fh, image, 0);
    }
#ifdef USE_LIBJPEG
    else {
        retval=read_image_jpeg(fh,image);
    }
#endif
    if(!using_stdin) {
        fclose(fh);
    }
    return retval;
}

/*
    Reads JPG image into png24_image struct
    ( this is used for compatiblity purposes )
*/

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
        "\nVersion 1.3.3 https://kornel.ski/dssim\n" \
        , argv0, argv0);
}

inline static unsigned char to_byte(float in) {
    if (in <= 0) return 0;
    if (in >= 255.f/256.f) return 255;
    return in * 256.f;
}

double get_gamma(const png24_image *image) {
    // Assume unlabelled are sRGB too
    if (RWPNG_NONE == image->output_color || RWPNG_SRGB == image->output_color) {
        return dssim_srgb_gamma;
    }
    const double gamma = image->gamma;
    if (gamma > 0 && gamma < 1.0) {
        // If the gamma chunk states gamma closest to sRGB that PNG can express, then assume sRGB too
        if (RWPNG_GAMA_ONLY == image->output_color && gamma > 0.4545499 && gamma < 0.4545501) {
            return dssim_srgb_gamma;
        }
        return gamma;
    }

    fprintf(stderr, "Warning: invalid/unsupported gamma ignored: %f\n", gamma);
    return 0.45455;
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
        fprintf(stderr, "Can't read %s (%d)\n", file1, retval);
        return retval;
    }

    dssim_attr *attr = dssim_create_attr();

    dssim_image *original = dssim_create_image(attr, image1.row_pointers, DSSIM_RGBA, image1.width, image1.height, get_gamma(&image1));
    free(image1.row_pointers);
    free(image1.rgba_data);


    for (int arg = optind+1; arg < argc; arg++) {
        const char *file2 = argv[arg];

        png24_image image2 = {};
        retval = read_image(file2, &image2);
        if (retval) {
            fprintf(stderr, "Can't read %s (%d)\n", file2, retval);
            break;
        }

        if (image1.width != image2.width || image1.height != image2.height) {
            fprintf(stderr, "Image %s has different size than %s\n", file2, file1);
            retval = 4;
            break;
        }

        dssim_image *modified = dssim_create_image(attr, image2.row_pointers, DSSIM_RGBA, image2.width, image2.height, get_gamma(&image2));
        free(image2.row_pointers);
        free(image2.rgba_data);

        if (!modified) {
            fprintf(stderr, "Unable to process image %s\n", file2);
            retval = 4;
            break;
        }

        if (map_output_file) {
            dssim_set_save_ssim_maps(attr, 1, 1);
        }

        double dssim = dssim_compare(attr, original, modified);
        dssim_dealloc_image(modified);

        printf("%.8f\t%s\n", dssim, file2);


        if (map_output_file) {
            dssim_ssim_map map_meta = dssim_pop_ssim_map(attr, 0, 0);
            dssim_px_t *map = map_meta.data;
            dssim_rgba *out = (dssim_rgba*)map;
            for(int i=0; i < map_meta.width*map_meta.height; i++) {
                const dssim_px_t max = 1.0 - map[i];
                const dssim_px_t maxsq = max * max;
                out[i] = (dssim_rgba) {
                    .r = to_byte(max * 3.0),
                    .g = to_byte(maxsq * 6.0),
                    .b = to_byte(max / ((1.0 - map_meta.dssim) * 4.0)),
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
