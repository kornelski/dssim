LIBOBJS = dssim.o
OBJS = $(LIBOBJS) rwpng.o main.o
BIN = $(DESTDIR)$(PREFIX)dssim
STATICLIB = $(DESTDIR)$(PREFIX)libdssim.a

CFLAGSOPT ?= -DNDEBUG -O3 -fstrict-aliasing -ffast-math -funroll-loops -fomit-frame-pointer -ffinite-math-only
CFLAGS ?= -Wall -I. $(CFLAGSOPT)
CFLAGS += -std=c99 `pkg-config libpng --cflags || pkg-config libpng16 --cflags` $(CFLAGSADD)

LDFLAGS += `pkg-config libpng --libs || pkg-config libpng16 --libs` -lm -lz $(LDFLAGSADD)

ifdef USE_COCOA
OBJS += rwpng_cocoa.m
CC=clang
CFLAGS += -DUSE_COCOA=1
LDFLAGS += -mmacosx-version-min=10.7 -framework Cocoa -framework Accelerate
endif

%.o: %.c %.h
	$(CC) $(CFLAGS) -c -o $@ $<

$(BIN): $(OBJS)
	$(CC) -o $@ $^ $(CFLAGS) $(LDFLAGS)

$(STATICLIB): $(LIBOBJS)
	$(AR) $(ARFLAGS) $@ $^

clean:
	-rm -f dssim *.o
