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

#if 1
#import <Accelerate/Accelerate.h>
#endif

#ifndef MIN
#define MIN(a,b) ((a)<=(b)?(a):(b))
#endif
#ifndef MAX
#define MAX(a,b) ((a)>=(b)?(a):(b))
#endif




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
void blur(const dssim_px_t *src, dssim_px_t *restrict tmp, dssim_px_t *restrict dst,
                 const int width, const int height)
{
    assert(src);
    assert(dst);
    assert(tmp);
    assert(width > 2);
    assert(height > 2);
    assert(src != tmp);
    assert(dst != tmp);

#if 1
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
        1.f/16.f, 2.f/16.f, 1.f/16.f,
        2.f/16.f, 4.f/16.f, 2.f/16.f,
        1.f/16.f, 2.f/16.f, 1.f/16.f,
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


void blur_in_place(dssim_px_t *restrict srcdst, dssim_px_t *restrict tmp,
                 const int width, const int height) {
    assert(tmp);
    assert(tmp != srcdst);
    blur(srcdst, tmp, srcdst, width, height);
}

