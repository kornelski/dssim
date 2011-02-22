/*
 *  dssim.h
 *  dssim
 *
 *  Created by porneL on 22.lut.11.
 *
 */

#include "rwpng.h"

double dssim_image(png24_image *image1, png24_image *image2, const char *ssimfilename);

typedef struct dssim_info dssim_info;

dssim_info *dssim_init();

void dssim_dealloc(dssim_info *inf);

void dssim_set_original(dssim_info *inf, png24_image *image1);
int dssim_set_modified(dssim_info *inf, png24_image *image2);

double dssim_compare(dssim_info *inf, const char *ssimfilename);
