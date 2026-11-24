#!/usr/bin/env bash
# Installs Harlo's packages on clean images of the systems they're for, with
# each one's own package manager, and starts Harlo there
# (packaging/linux/check-start.sh): its dependencies resolve, its server
# starts and its window opens. The AppImage too, on its own. The same check
# as the release workflow's, on this machine (rootless Podman):
#
#   packaging/linux/test-install.sh [folder with the packages, default dist]
#   ONLY="debian:12 appimage" packaging/linux/test-install.sh dist
set -uo pipefail
cd "$(dirname "$0")/../.."
DIR=$(realpath "${1:-dist}")
ls "$DIR"/*.deb "$DIR"/*.rpm "$DIR"/*.pkg.tar.zst "$DIR"/*.AppImage > /dev/null || { echo "No packages in $DIR"; exit 1; }
LOGS=$(mktemp -d "${TMPDIR:-/tmp}/harlo-install.XXXXXX")

APT="apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq"
declare -A INSTALL=(
    ["ubuntu:22.04"]="docker.io/library/ubuntu:22.04|$APT xvfb ./harlo_*_amd64.deb"
    ["ubuntu:24.04"]="docker.io/library/ubuntu:24.04|$APT xvfb ./harlo_*_amd64.deb"
    ["debian:12"]="docker.io/library/debian:12|$APT xvfb ./harlo_*_amd64.deb"
    ["fedora:42"]="registry.fedoraproject.org/fedora:42|dnf install -y -q xorg-x11-server-Xvfb ./harlo-*.x86_64.rpm"
    ["opensuse"]="registry.opensuse.org/opensuse/tumbleweed|zypper --non-interactive --quiet install --allow-unsigned-rpm xorg-x11-server-Xvfb ./harlo-*.x86_64.rpm"
    ["arch"]="docker.io/library/archlinux:latest|pacman -Syu --noconfirm --quiet > /dev/null && pacman -S --noconfirm xorg-server-xvfb && pacman -U --noconfirm ./harlo-*-x86_64.pkg.tar.zst"
    # The AppImage needs only what a desktop has (here: the window's libraries).
    ["appimage"]="docker.io/library/ubuntu:24.04|$APT xvfb libfontconfig1 libasound2t64 libxkbcommon-x11-0 libx11-xcb1 libxcursor1 libxrandr2 libxi6 libwayland-client0 libwayland-cursor0 libwayland-egl1 libegl1 libgl1|packaging/linux/check-start.sh ./harlo-*-x86_64.AppImage"
)

names=("${!INSTALL[@]}")
[ -n "${ONLY:-}" ] && names=($(for n in "${names[@]}"; do for o in $ONLY; do [[ $n == "$o" ]] && echo "$n"; done; done))
for name in "${names[@]}"; do
    IFS='|' read -r image install check <<< "${INSTALL[$name]}"
    check=${check:-packaging/linux/check-start.sh}
    # (A copy of the packages, so the image can't change them.)
    podman run --rm -v "$DIR:/pkgs:ro,Z" -v "$PWD/packaging:/src/packaging:ro,Z" "$image" bash -c \
        "cp -r /pkgs /work && cd /work && $install && cd /work && /src/$check" \
        > "$LOGS/${name//[:\/]/_}.log" 2>&1 &
    echo "$! $name" >> "$LOGS/jobs"
done
failed=0
while read -r pid name; do
    log="$LOGS/${name//[:\/]/_}.log"
    if wait "$pid"; then
        printf '%-14s ok   (%s)\n' "$name" "$(grep -m1 '^harlo [0-9]' "$log")"
    else
        printf '%-14s FAIL (log: %s)\n' "$name" "$log"
        tail -n 20 "$log" | sed 's/^/    /'
        failed=1
    fi
done < "$LOGS/jobs"
[ "$failed" = 0 ] && rm -rf "$LOGS"
exit "$failed"
