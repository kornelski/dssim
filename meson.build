project ('dssim', 'C',
  version : '1.3.2.1',
  meson_version : '>= 0.48.0',
  default_options : [ 'warning_level=1'])

dssim_lib_sources = ['src/dssim.c']
dssim_sources = ['src/main.c',
         'src/rwpng.c']

c_args = ['-DNDEBUG',
      '-fstrict-aliasing',
      '-ffast-math',
      '-funroll-loops',
      '-fomit-frame-pointer',
      '-ffinite-math-only',
      '-Wall',
      '-std=c99']

cc = meson.get_compiler('c')
mathlib = cc.find_library('m')
zlib = cc.find_library('z')

jpeg_dep = dependency('libjpeg', required: get_option('jpeg'))
if jpeg_dep.found()
  add_project_arguments('-DUSE_LIBJPEG', language: 'c')
endif

libdssim = shared_library('dssim-lib',
              dssim_lib_sources,
              version: '1.1',
              dependencies: [mathlib, zlib],
              c_args: c_args,
              install:true)

dssim_dep = declare_dependency(link_with : libdssim,
  include_directories : include_directories('src/'))

h = install_headers ('src/dssim.h')

pkg = import('pkgconfig')
pkg.generate(libraries : libdssim,
             subdirs : '.',
             name : 'libdssim',
             filebase : 'dssim',
             version : meson.project_version(),
             description : 'A structural similarity library.')

png_dep = dependency('libpng', version: '>=1.4')
executable = executable ('dssim',
             dssim_sources,
             install: true,
             c_args: c_args,
             dependencies: [jpeg_dep, png_dep],
             link_with: [libdssim])
