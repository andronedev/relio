CROSS   ?= arm-nickel-linux-gnueabihf-
CXX     := $(CROSS)g++
MOC     := moc
STRIP   := $(CROSS)strip
PKG     := $(CROSS)pkg-config
QT      := Qt5Core Qt5Gui Qt5Widgets Qt5Network

CXXFLAGS := -O2 -std=gnu++11 -fPIC -fvisibility=hidden -Wall -Wextra -Werror \
            -march=armv7-a -mtune=cortex-a8 -mfpu=neon -mfloat-abi=hard -mthumb \
            $(shell $(PKG) --cflags $(QT))
LDFLAGS  := -shared -Wl,--no-undefined -Wl,--as-needed -pthread
LDLIBS   := $(shell $(PKG) --libs $(QT)) -ldl

SOURCES := $(wildcard src/*.cc)
MOCS    := build/image_plugin.moc.cc
OBJECTS := $(SOURCES:src/%.cc=build/%.o) $(MOCS:.cc=.o)

build/librelio.so: $(OBJECTS)
	$(CXX) $(LDFLAGS) -o $@ $^ $(LDLIBS)
	$(STRIP) --strip-unneeded $@

build/%.o: src/%.cc $(wildcard src/*.h) build/strings_table.inc | build
	$(CXX) $(CXXFLAGS) -Ibuild -c -o $@ $<

build/%.moc.cc: src/%.h src/%.json | build
	$(MOC) $< -o $@

build/%.moc.o: build/%.moc.cc
	$(CXX) $(CXXFLAGS) -Isrc -c -o $@ $<

build:
	mkdir -p build

clean:
	rm -rf build

.PHONY: clean
