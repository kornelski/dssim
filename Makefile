DESTDIR=bin/
SRC=src/

LIBOBJS = $(SRC)dssim.o
OBJS = $(LIBOBJS) $(SRC)rwpng.o $(SRC)main.o
COCOASRC =
BIN = $(DESTDIR)$(PREFIX)dssim
STATICLIB = $(DESTDIR)$(PREFIX)libdssim.a

CFLAGSOPT ?= -DNDEBUG -O3 -fstrict-aliasing -ffast-math -funroll-loops -fomit-frame-pointer -ffinite-math-only
CFLAGS ?= -Wall -I. $(CFLAGSOPT)
CFLAGS += -std=c99 `pkg-config libpng --cflags || pkg-config libpng16 --cflags` $(CFLAGSADD)



ifdef USE_LIBJPEG
CFLAGS += -DUSE_LIBJPEG `pkg-config libjpeg --cflags 2>/dev/null`
LDFLAGS += `pkg-config libjpeg --libs 2>/dev/null || echo -ljpeg`
endif
 
LDFLAGS += `pkg-config libpng --libs || pkg-config libpng16 --libs` -lm -lz $(LDFLAGSADD)

ifdef USE_COCOA
COCOASRC = $(SRC)rwpng_cocoa.m
CC=clang
CFLAGS += -mmacosx-version-min=10.7 -DUSE_COCOA=1
LDFLAGS += -mmacosx-version-min=10.7 -framework Cocoa -framework Accelerate
endif

all: $(BIN)

$(SRC)%.o: $(SRC)%.c $(SRC)%.h
	$(CC) $(CFLAGS) -c -o $@ $<

$(BIN): $(OBJS) $(COCOASRC)
	-mkdir -p $(DESTDIR)
	$(CC) -o $@ $^ $(CFLAGS) $(LDFLAGS)

$(STATICLIB): $(LIBOBJS)
	$(AR) $(ARFLAGS) $@ $^

clean:
	-rm -f $(BIN) $(OBJS)

.PHONY: all clean
