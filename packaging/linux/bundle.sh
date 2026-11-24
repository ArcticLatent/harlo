#!/usr/bin/env bash
# Harlo's release bundle for Linux:
#
#   packaging/linux/bundle.sh
#
# makes dist/AppDir (harlo and harlo-server, the menu entry, the icons and
# the licences), dist/harlo-<version>-x86_64.AppImage, and dist/opt/harlo,
# the tree the .deb, .rpm and Arch packages install
# (packaging/linux/nfpm.yaml). Nothing else is bundled: the window takes
# only libraries every desktop has.
#
# Needs: cargo, cmake, clang, rsvg-convert, curl, and FUSE or
# APPIMAGE_EXTRACT_AND_RUN=1 for appimagetool.
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
OUT=dist
TOOLS=$OUT/tools
TARGET=${CARGO_TARGET_DIR:-target}
mkdir -p "$TOOLS"

# whisper.cpp (dictation) for any x86-64 CPU with AVX2 (2013 on), not just
# the build machine's.
export GGML_NATIVE=OFF GGML_AVX=ON GGML_AVX2=ON GGML_FMA=ON GGML_F16C=ON
cargo build --release --locked -p harlo -p harlo-server

fetch() {
    [ -x "$TOOLS/$1" ] || { curl -fsSL -o "$TOOLS/$1" "$2"; chmod +x "$TOOLS/$1"; }
}
fetch appimagetool https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage

APPDIR=$OUT/AppDir
rm -rf "$APPDIR"
# harlo starts the harlo-server beside it.
install -Dm755 "$TARGET/release/harlo" "$APPDIR/usr/bin/harlo"
install -Dm755 "$TARGET/release/harlo-server" "$APPDIR/usr/bin/harlo-server"
install -Dm644 packaging/harlo.desktop "$APPDIR/usr/share/applications/harlo.desktop"
install -Dm644 packaging/harlo.svg "$APPDIR/usr/share/icons/hicolor/scalable/apps/harlo.svg"
for size in 64 128 256 512; do
    mkdir -p "$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps"
    rsvg-convert -w $size -h $size packaging/harlo.svg -o "$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps/harlo.png"
done
LIC=$APPDIR/usr/share/licenses/harlo
install -Dm644 LICENSE "$LIC/LICENSE"
install -Dm644 THIRD-PARTY-NOTICES.md "$LIC/THIRD-PARTY-NOTICES.md"
install -Dm644 crates/harlo/ui/fonts/Geist-OFL.txt "$LIC/geist-OFL.txt"
install -Dm644 crates/harlo/ui/fonts/JetBrainsMono-OFL.txt "$LIC/jetbrains-mono-OFL.txt"
install -Dm644 crates/harlo/ui/fonts/FileIcons-LICENSE.txt "$LIC/file-icons-LICENSE.txt"
install -Dm644 crates/harlo/sounds/LICENSE.txt "$LIC/sounds-LICENSE.txt"
install -Dm644 crates/harlo-core/themes/TINTED-LICENSE "$LIC/themes-LICENSE"
for f in crates/harlo/ui/images/providers/*-LICENSE; do
    name=$(basename "$f" -LICENSE | tr '[:upper:]' '[:lower:]')
    install -Dm644 "$f" "$LIC/provider-logos-$name-LICENSE"
done

# The AppImage: started through AppRun, with its own icon at the root.
cat > "$APPDIR/AppRun" <<'RUN'
#!/bin/sh
HERE=$(dirname "$(readlink -f "$0")")
exec "$HERE/usr/bin/harlo" "$@"
RUN
chmod 755 "$APPDIR/AppRun"
ln -sf usr/share/applications/harlo.desktop "$APPDIR/harlo.desktop"
cp "$APPDIR/usr/share/icons/hicolor/256x256/apps/harlo.png" "$APPDIR/harlo.png"
ln -sf harlo.png "$APPDIR/.DirIcon"
# (How an AppImage is installed is read from $APPIMAGE, not a marker.)
ARCH=x86_64 "$TOOLS/appimagetool" --no-appstream "$APPDIR" "$OUT/harlo-$VERSION-x86_64.AppImage"

# The packages' tree: /opt/harlo (bin, share); nfpm adds /usr/bin/harlo,
# the menu entry, the icons and how it was installed.
rm -rf "$OUT/opt"
mkdir -p "$OUT/opt"
cp -a "$APPDIR/usr" "$OUT/opt/harlo"
echo "$VERSION" > "$OUT/version"
echo "Bundle for harlo $VERSION in $OUT"
