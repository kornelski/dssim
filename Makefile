CC = gcc
LD = gcc

SOURCES = dssim.c rwpng.c
LIBS = `pkg-config libpng --libs`

CFLAGS ?= -g3

CFLAGS += -std=c99 `pkg-config libpng --cflags`

%.o: %.c %.h
	$(CC) $(CFLAGS) -c -o $@ $<

dssim: $(SOURCES) main.c
	$(CC) -o $@ $^ $(CFLAGS) $(LIBS) -lm -lz
