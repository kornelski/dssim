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

static void usage(const char *argv0)
{
    fprintf(stderr, "Usage: %s original.png modified.png [modified.png...]\n\n" \
            "Compares first image against subsequent images,\n" \
            "outputs SSIM difference for each of them in order.\n" \
            "Images must have identical size. May have different gamma & depth.\n" \
            "\nVersion 0.2 http://pornel.net/dssim\n" \
            , argv0);
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

        double dssim = dssim_compare(dinf, NULL);
        printf("%.6f\t%s\n", dssim, file2);
    }

    dssim_dealloc(dinf);
    return retval;
}
