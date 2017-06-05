DESTDIR=bin/
SRC=src/

LIBOBJS = $(SRC)dssim.o
OBJS = $(LIBOBJS) $(SRC)rwpng.o $(SRC)main.o
COCOASRC =
BIN = $(DESTDIR)$(PREFIX)dssim
STATICLIB = $(DESTDIR)$(PREFIX)libdssim.a
SHARED_LIB_FILE = libdssim.so
SHARED_LIB = $(DESTDIR)$(PREFIX)$(SHARED_LIB_FILE)

INSTALL_LIB_DIR = /usr/lib/
INSTALL_INCLUDE_DIR = /usr/include/dssim/

CFLAGSOPT ?= -DNDEBUG -O3 -fstrict-aliasing -ffast-math -funroll-loops -fomit-frame-pointer -ffinite-math-only -fpic
CFLAGS ?= -Wall -I. $(CFLAGSOPT)
CFLAGS += -std=c99 `pkg-config libpng --cflags || pkg-config libpng16 --cflags` $(CFLAGSADD)



ifdef USE_LIBJPEG
LDFLAGS += -ljpeg
CFLAGS += -DUSE_LIBJPEG
endif
 
LDFLAGS += `pkg-config libpng --libs || pkg-config libpng16 --libs` -lm -lz $(LDFLAGSADD)

ifdef USE_COCOA
COCOASRC = $(SRC)rwpng_cocoa.m
CC=clang
CFLAGS += -mmacosx-version-min=10.7 -DUSE_COCOA=1
LDFLAGS += -mmacosx-version-min=10.7 -framework Cocoa -framework Accelerate
endif

all: $(BIN)

shared: $(SHARED_LIB)

install: shared
	-cp $(SHARED_LIB) $(INSTALL_LIB_DIR)
	-mkdir -p $(INSTALL_INCLUDE_DIR)
	-cp $(SRC)dssim_helpers.h $(INSTALL_INCLUDE_DIR)
	ldconfig

uninstall:
	-ls $(INSTALL_LIB_DIR)$(SHARED_LIB_FILE) && rm $(INSTALL_LIB_DIR)$(SHARED_LIB_FILE)
	-ls $(INSTALL_INCLUDE_DIR) && rm -rf $(INSTALL_INCLUDE_DIR)

$(SRC)%.o: $(SRC)%.c $(SRC)%.h
	$(CC) $(CFLAGS) -c -o $@ $<

$(BIN): $(OBJS) $(COCOASRC)
	-mkdir -p $(DESTDIR)
	$(CC) -o $@ $^ $(CFLAGS) $(LDFLAGS)

$(STATICLIB): $(LIBOBJS)
	$(AR) $(ARFLAGS) $@ $^

$(SHARED_LIB): $(OBJS)
	$(CC) $(CFLAGS) $(LDFLAGS) -shared -o $@ $^

clean:
	-rm -f $(BIN) $(OBJS) $(SHARED_LIB)

.PHONY: all clean
