# Foquinho — build sem CMake: g++ + moc + rcc do Qt6
QT_MODULES := Qt6Quick Qt6Qml Qt6Gui Qt6Core Qt6Network Qt6DBus
CXX        ?= g++
CXXFLAGS   += -O2 -std=c++17 -fPIC -Wall -Wextra -Isrc \
              $(shell pkg-config --cflags $(QT_MODULES)) -I/usr/include/KF6/KGlobalAccel
LDLIBS     += $(shell pkg-config --libs $(QT_MODULES)) -lLayerShellQtInterface -lKF6GlobalAccel
MOC        := /usr/lib/qt6/moc
RCC        := /usr/lib/qt6/rcc
PREFIX     ?= $(HOME)/.local

QML_FILES  := $(wildcard qml/*.qml) qml/qmldir
OBJS       := build/main.o build/backend.o build/moc_backend.o build/qrc_resources.o
TARGET     := build/foco-max

all: $(TARGET)

build:
	@mkdir -p build

build/moc_backend.cpp: src/backend.h | build
	$(MOC) $< -o $@

build/qrc_resources.cpp: resources.qrc $(QML_FILES) | build
	$(RCC) --name resources $< -o $@

build/%.o: src/%.cpp src/backend.h | build
	$(CXX) $(CXXFLAGS) -c $< -o $@

build/moc_backend.o: build/moc_backend.cpp
	$(CXX) $(CXXFLAGS) -c $< -o $@

build/qrc_resources.o: build/qrc_resources.cpp
	$(CXX) $(CXXFLAGS) -c $< -o $@

$(TARGET): $(OBJS)
	$(CXX) -o $@ $^ $(LDLIBS)

install: all
	install -Dm755 $(TARGET) $(PREFIX)/bin/foco-max
	install -Dm644 data/foco-max.svg $(PREFIX)/share/icons/hicolor/scalable/apps/foco-max.svg
	sed "s|@BIN@|$(PREFIX)/bin/foco-max|g" data/foco-max.desktop.in > $(PREFIX)/share/applications/foco-max.desktop
	-update-desktop-database $(PREFIX)/share/applications 2>/dev/null

uninstall:
	rm -f $(PREFIX)/bin/foco-max $(PREFIX)/share/applications/foco-max.desktop \
	      $(PREFIX)/share/icons/hicolor/scalable/apps/foco-max.svg $(HOME)/.config/autostart/foco-max.desktop

run: all
	./$(TARGET)

clean:
	rm -rf build

.PHONY: all install uninstall run clean
