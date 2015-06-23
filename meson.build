project ('dssim', 'C')

dssim_lib_sources = ['src/dssim.c']

dssim_sources = ['src/main.c',
		 'src/rwpng.c']

c_args = ['-DNDEBUG',
	  '-O3',
	  '-fstrict-aliasing',
	  '-ffast-math',
	  '-funroll-loops',
	  '-fomit-frame-pointer',
	  '-ffinite-math-only',
	  '-Wall',
	  '-std=c99']

mathlib = find_library('m', required : true)
zlib = find_library('z', required : true)

libdssim = shared_library('dssim-lib',
			  dssim_lib_sources,
			  version: '1.1',
			  dependencies: [mathlib, zlib],
			  c_args: c_args,
			  install:true)

h = install_headers ('src/dssim.h')

pkgconfig_gen(libraries : libdssim,
	      subdirs : '.',
	      name : 'libdssim',
	      filebase : 'dssim',
	      version : '1.1',
	      description : 'A structural similarity library.')

png_dep = dependency('libpng')


executable = executable ('dssim',
			 dssim_sources,
			 install: true,
			 c_args: c_args,
			 dependencies: [png_dep],
			 link_with: [libdssim])
