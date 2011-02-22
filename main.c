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

int main(int argc, const char *argv[])
{
    if (argc < 3) {
        fprintf(stderr, "Usage: %s file1 file2 [output]\n", argv[0]);
        return 1;
    }

    const char *file1 = argv[1];
    const char *file2 = argv[2];
    const char *outfile = argc > 3 ? argv[3] : NULL;

    png24_image image1;
    int retval = read_image(file1, &image1);
    if (retval) {
        fprintf(stderr, "Can't read %s\n", file1);
        return retval;
    }

    png24_image image2;
    retval = read_image(file2, &image2);
    if (retval) {
        fprintf(stderr, "Can't read %s\n", file2);
        return retval;
    }

    double dssim = dssim_image(&image1, &image2, outfile);
    printf("%.4f\n", dssim);
    return 0;
}
