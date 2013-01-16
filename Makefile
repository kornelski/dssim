SOURCES = dssim.c rwpng.c main.c

CFLAGSOPT ?= -DNDEBUG -O3 -fstrict-aliasing -ffast-math -funroll-loops -fomit-frame-pointer -ffinite-math-only
CFLAGS ?= -Wall -I. $(CFLAGSOPT)
CFLAGS += -std=c99 `pkg-config libpng --cflags` $(CFLAGSADD)

LDFLAGS += `pkg-config libpng --libs` $(LDFLAGSADD)

ifdef USE_COCOA
SOURCES += rwpng_cocoa.m
CC=clang
LDFLAGS += -framework Cocoa
endif

%.o: %.c %.h
	$(CC) $(CFLAGS) -c -o $@ $<

dssim: $(SOURCES)
	$(CC) -o $@ $^ $(CFLAGS) $(LDFLAGS) -lm -lz

clean:
	-rm dssim
