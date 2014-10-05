
typedef struct dssim_info dssim_info;

typedef struct {
    unsigned char r, g, b, a;
} dssim_rgba;

dssim_info *dssim_init(int channels);

void dssim_dealloc(dssim_info *inf);

/*
  Write one row (from index `y`) of `width` pixels to pre-allocated arrays in `channels`.
  if num_channels == 1 write only to channels[0][0..width-1]
  if num_channels == 3 the write luma to channel 0, and chroma to 1 and 2.
 */
typedef void dssim_row_callback(float *const channels[], const int num_channels, const int y, const int width, void *user_data);

void dssim_set_original(dssim_info *inf, dssim_rgba *row_pointers[], const int width, const int height, double gamma);
void dssim_set_original_float_callback(dssim_info *inf, const int width, const int height, dssim_row_callback cb, void *callback_user_data);

int dssim_set_modified(dssim_info *inf, dssim_rgba *row_pointers[], const int width, const int height, double gamma);
int dssim_set_modified_float_callback(dssim_info *inf, const int width, const int height, dssim_row_callback cb, void *callback_user_data);

double dssim_compare(dssim_info *inf, float **ssimmap);
