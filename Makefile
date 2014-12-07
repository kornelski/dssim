SOURCES = dssim.o rwpng.o main.o

CFLAGSOPT ?= -DNDEBUG -O3 -fstrict-aliasing -ffast-math -funroll-loops -fomit-frame-pointer -ffinite-math-only
CFLAGS ?= -Wall -I. $(CFLAGSOPT)
CFLAGS += -std=c99 `pkg-config libpng --cflags || pkg-config libpng16 --cflags` $(CFLAGSADD)

LDFLAGS += `pkg-config libpng --libs || pkg-config libpng16 --libs` $(LDFLAGSADD)

ifdef USE_COCOA
SOURCES += rwpng_cocoa.m
CC=clang
CFLAGS += -DUSE_COCOA=1
LDFLAGS += -mmacosx-version-min=10.7 -framework Cocoa
endif

%.o: %.c %.h
	$(CC) $(CFLAGS) -c -o $@ $<

dssim: $(SOURCES)
	$(CC) -o $@ $^ $(CFLAGS) $(LDFLAGS) -lm -lz

clean:
	-rm -f dssim *.o
