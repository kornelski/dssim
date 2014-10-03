
typedef struct dssim_info dssim_info;

typedef struct {
    unsigned char r, g, b, a;
} dssim_rgba;

dssim_info *dssim_init();

void dssim_dealloc(dssim_info *inf);

void dssim_set_original(dssim_info *inf, dssim_rgba *row_pointers[], const int width, const int height, double gamma);
int dssim_set_modified(dssim_info *inf, dssim_rgba *row_pointers[], const int width, const int height, double gamma);

double dssim_compare(dssim_info *inf, float **ssimmap);
