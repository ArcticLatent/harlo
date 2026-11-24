#!/bin/sh
# Run on a system where Harlo was just installed (a clean container): its
# programs run and find every library, its server starts, and its window
# opens and stays open (on Xvfb). With an AppImage's path, that AppImage
# instead, whose server runs from a copy of its own.
#
#   packaging/linux/check-start.sh             the installed package
#   packaging/linux/check-start.sh <AppImage>  that AppImage
#
# Everything happens in a scratch home, offline: no relay, no update
# manifest, no session bus.
set -eu
APPIMAGE_FILE=${1:-}
SCRATCH=$(mktemp -d)
export HOME="$SCRATCH/home" XDG_RUNTIME_DIR="$SCRATCH/run" \
    DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent \
    HARLO_RELAY=http://127.0.0.1:9 HARLO_UPDATE_MANIFEST_URL=http://127.0.0.1:9
mkdir -p "$HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
fail() { echo "check-start: $*" >&2; exit 1; }

HARLO=harlo
if [ -n "$APPIMAGE_FILE" ]; then
    export APPIMAGE_EXTRACT_AND_RUN=1
    HARLO=$APPIMAGE_FILE
    "$HARLO" --version
else
    harlo --version
    harlo-server --version
    for program in /opt/harlo/bin/harlo /opt/harlo/bin/harlo-server; do
        ! ldd "$program" | grep 'not found' || fail "$program is missing libraries"
    done
    # What the window loads when it opens: X11 or Wayland, OpenGL, the keyboard.
    for lib in libxkbcommon.so.0 libxkbcommon-x11.so.0 libX11.so.6 libX11-xcb.so.1 \
        libXcursor.so.1 libXrandr.so.2 libXi.so.6 libwayland-client.so.0 \
        libwayland-cursor.so.0 libwayland-egl.so.1 libEGL.so.1 libGL.so.1; do
        ldconfig -p | grep -q "$lib " || fail "$lib isn't installed"
    done
    harlo-server service start
    harlo-server service status
fi

# The window, for 15 seconds: it should still be open when they're up.
Xvfb :99 -screen 0 1280x800x24 -nolisten tcp > "$SCRATCH/xvfb.log" 2>&1 &
XVFB=$!
sleep 2
set +e
DISPLAY=:99 timeout 15 "$HARLO" > "$SCRATCH/harlo.log" 2>&1
code=$?
set -e
kill "$XVFB" 2> /dev/null || true
cat "$SCRATCH/harlo.log"
[ "$code" = 124 ] || fail "harlo exited ($code) instead of staying open"
! grep -q 'panicked' "$SCRATCH/harlo.log" || fail "harlo panicked"
if [ -n "$APPIMAGE_FILE" ]; then
    # Its server, started from a copy outside the AppImage's mount.
    ls "$HOME"/.local/share/harlo/server/*/harlo-server > /dev/null || fail "no copy of the AppImage's server"
fi
echo "check-start: ok"
