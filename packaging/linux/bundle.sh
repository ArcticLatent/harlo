#!/usr/bin/env bash
# harlo's release bundle for Linux, Qt included (most distributions ship a
# Qt older than the 6.9 harlo needs):
#
#   QMAKE=/path/to/Qt/6.x/gcc_64/bin/qmake packaging/linux/bundle.sh
#
# makes dist/AppDir (harlo, Qt, its QML modules and plugins, the libraries
# they need), dist/harlo-<version>-x86_64.AppImage, and dist/opt/harlo, the
# tree the .deb, .rpm and Arch packages install (packaging/linux/nfpm.yaml).
# Needs: cargo, cmake, clang, rsvg-convert, curl, and FUSE or
# APPIMAGE_EXTRACT_AND_RUN=1 for the AppImage tools.
set -euo pipefail
cd "$(dirname "$0")/../.."
: "${QMAKE:?QMAKE must point at Qt 6.9 or newer}"
QTDIR=$(dirname "$(dirname "$QMAKE")")
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
OUT=dist
TOOLS=$OUT/tools
mkdir -p "$TOOLS"

# whisper.cpp (dictation) for any x86-64 CPU with AVX2 (2013 on), not just
# the build machine's.
export GGML_NATIVE=OFF GGML_AVX=ON GGML_AVX2=ON GGML_FMA=ON GGML_F16C=ON
cargo build --release --locked -p harlo-app

fetch() {
    [ -x "$TOOLS/$1" ] || { curl -fsSL -o "$TOOLS/$1" "$2"; chmod +x "$TOOLS/$1"; }
}
fetch linuxdeploy https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
fetch linuxdeploy-plugin-qt https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous/linuxdeploy-plugin-qt-x86_64.AppImage
fetch appimagetool https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage

APPDIR=$OUT/AppDir
rm -rf "$APPDIR"
install -Dm755 "${CARGO_TARGET_DIR:-target}/release/harlo" "$APPDIR/usr/bin/harlo"
install -Dm644 packaging/harlo.desktop "$APPDIR/usr/share/applications/harlo.desktop"
install -Dm644 crates/harlo-app/icons/app/harlo.svg "$APPDIR/usr/share/icons/hicolor/scalable/apps/harlo.svg"
for size in 64 128 256 512; do
    mkdir -p "$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps"
    rsvg-convert -w $size -h $size crates/harlo-app/icons/app/harlo.svg -o "$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps/harlo.png"
done
LIC=$APPDIR/usr/share/licenses/harlo
install -Dm644 LICENSE "$LIC/LICENSE"
install -Dm644 crates/harlo-app/themes/LICENSE "$LIC/themes-LICENSE"
install -Dm644 crates/harlo-app/icons/file-types/LICENSE "$LIC/file-icons-LICENSE"
install -Dm644 crates/harlo-app/icons/REMIXICON-LICENSE "$LIC/remixicon-LICENSE"
install -Dm644 crates/harlo-app/icons/PASEO-NOTICE "$LIC/paseo-NOTICE"
install -Dm644 crates/harlo-app/icons/providers/OPENCHAMBER-LICENSE "$LIC/provider-logos-LICENSE"
install -Dm644 crates/harlo-app/phone/TWEETNACL-LICENSE "$LIC/tweetnacl-LICENSE"
for f in crates/harlo-app/fonts/*LICENSE* crates/harlo-app/fonts/*OFL*; do [ -f "$f" ] && install -Dm644 "$f" "$LIC/fonts-$(basename "$f")"; done
# (Qt's own licences: LGPLv3, linked dynamically.)
[ -d "$QTDIR/../../Licenses" ] && cp -r "$QTDIR/../../Licenses" "$LIC/qt" || true

# Qt, the QML modules harlo imports, the plugins it needs (X11 and Wayland,
# the desktop's file dialogs and theme, SVG icons, sound) and every library
# they link, but the ones every desktop has (glibc, OpenGL, fonts…).
export QML_SOURCES_PATHS=crates/harlo-app/qml
export EXTRA_PLATFORM_PLUGINS="libqwayland.so;libqoffscreen.so"
export EXTRA_QT_MODULES="waylandclient;svg"
export EXTRA_QT_PLUGINS="iconengines;imageformats;platformthemes;platforminputcontexts;tls;wayland-decoration-client;wayland-graphics-integration-client;wayland-shell-integration;xcbglintegrations;multimedia"
export LD_LIBRARY_PATH="$QTDIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export PATH="$QTDIR/bin:$PATH"
"$TOOLS/linuxdeploy" --appdir "$APPDIR" --plugin qt \
    --desktop-file "$APPDIR/usr/share/applications/harlo.desktop" \
    --icon-file "$APPDIR/usr/share/icons/hicolor/256x256/apps/harlo.png"
echo "appimage" > "$APPDIR/usr/install-kind"
# The AppImage's own icon: a PNG and .DirIcon at its root (appimagetool no
# longer takes the SVG linuxdeploy links there).
ln -sf usr/share/icons/hicolor/256x256/apps/harlo.png "$APPDIR/harlo.png"
ln -sf harlo.png "$APPDIR/.DirIcon"

# The AppImage.
ARCH=x86_64 "$TOOLS/appimagetool" --no-appstream "$APPDIR" "$OUT/harlo-$VERSION-x86_64.AppImage"

# The packages' tree: /opt/harlo (bin, lib, plugins, qml, share); nfpm
# adds /usr/bin/harlo, the menu entry and the icons.
rm -rf "$OUT/opt"
mkdir -p "$OUT/opt"
cp -a "$APPDIR/usr" "$OUT/opt/harlo"
rm -f "$OUT/opt/harlo/install-kind"
echo "$VERSION" > "$OUT/version"
echo "Bundle for harlo $VERSION in $OUT"
