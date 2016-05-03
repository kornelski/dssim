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

#ifdef __cplusplus
extern "C" {
#endif

typedef struct dssim_image dssim_image;
typedef struct dssim_attr dssim_attr;
typedef float dssim_px_t;

typedef struct {
    unsigned char r, g, b, a;
} dssim_rgba;

typedef enum dssim_colortype {
    DSSIM_GRAY = 1, // 1 byte per pixel, gamma applied
    DSSIM_RGB  = 2, // 3 bytes per pixel, gamma applied
    DSSIM_RGBA = 3, // 4 bytes per pixel, gamma applied
    DSSIM_LUMA = 4, // 1 byte per pixel, used as-is
    DSSIM_LAB  = 5, // 3 bytes per pixel, used as-is
    DSSIM_RGBA_TO_GRAY = 3 | 32, // 4 bytes per pixel, but only luma is used
} dssim_colortype;

typedef struct {
    int width, height;
    double dssim;
    dssim_px_t *data;
} dssim_ssim_map;

dssim_attr *dssim_create_attr(void);
void dssim_dealloc_attr(dssim_attr *);

// Magic number to use in place of gamma for a true sRGB curve
static const double dssim_srgb_gamma = -47571492;

/*
    Number of scales for multiscale (1 = regular SSIM). Optional weights array contains weight of each scale.
    Set before creating any images.
*/
void dssim_set_scales(dssim_attr *attr, const int num, const double *weights);

/*
    Maximum number scales for which bitmaps with per-pixel SSIM values are saved (0 = no saving).
    Set before comparison.
*/
void dssim_set_save_ssim_maps(dssim_attr *, unsigned int num_scales, unsigned int num_channels);

/*
    Get data of ssim map. You must free(map.data);
    Use after comparison.
 */
dssim_ssim_map dssim_pop_ssim_map(dssim_attr *, unsigned int scale_index, unsigned int channel_index);

/*
    If subsampling is enabled, color is tested at half resolution (recommended).
    Color weight controls how much of chroma channels' SSIM contributes to overall result.
 */
void dssim_set_color_handling(dssim_attr *, int subsampling, double color_weight);

/*
  Write one row (from index `y`) of `width` pixels to pre-allocated arrays in `channels`.
  if num_channels == 1 write only to channels[0][0..width-1]
  if num_channels == 3 the write luma to channel 0, and chroma to 1 and 2.
 */
typedef void dssim_row_callback(dssim_px_t *const restrict channels[], const int num_channels, const int y, const int width, void *user_data);

dssim_image *dssim_create_image(dssim_attr *, unsigned char *const *const row_pointers, dssim_colortype color_type, const int width, const int height, const double gamma);
dssim_image *dssim_create_image_float_callback(dssim_attr *, const int num_channels, const int width, const int height, dssim_row_callback cb, void *callback_user_data);
void dssim_dealloc_image(dssim_image *);

/*
Returns DSSIM between two images.
Original image can be reused. Modified image is destroyed (but still needs to be freed using dssim_dealloc_image).
 */
double dssim_compare(dssim_attr *, const dssim_image *restrict original, dssim_image *restrict modified);
#ifdef __cplusplus
}
#endif
