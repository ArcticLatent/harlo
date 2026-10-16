#!/usr/bin/env bash
# Installs harlo's packages on clean images of the systems they're for, with
# each one's own package manager, and starts harlo there (offscreen): its
# dependencies resolve and nothing's missing. The same check as the release
# workflow's, on this machine (rootless Podman):
#
#   packaging/linux/test-install.sh [folder with the packages, default dist]
#   ONLY="debian:12 fedora:42" packaging/linux/test-install.sh dist
set -uo pipefail
cd "$(dirname "$0")/../.."
DIR=$(realpath "${1:-dist}")
ls "$DIR"/*.deb "$DIR"/*.rpm "$DIR"/*.pkg.tar.zst > /dev/null || { echo "No packages in $DIR"; exit 1; }
LOGS=$(mktemp -d /tmp/harlo-install.XXXXXX)

declare -A INSTALL=(
    ["docker.io/library/ubuntu:22.04"]="apt-get update -qq && apt-get install -y -qq ./harlo_*_amd64.deb"
    ["docker.io/library/ubuntu:24.04"]="apt-get update -qq && apt-get install -y -qq ./harlo_*_amd64.deb"
    ["docker.io/library/debian:12"]="apt-get update -qq && apt-get install -y -qq ./harlo_*_amd64.deb"
    ["registry.fedoraproject.org/fedora:42"]="dnf install -y -q ./harlo-*.x86_64.rpm"
    ["registry.opensuse.org/opensuse/tumbleweed"]="zypper --non-interactive --quiet install --allow-unsigned-rpm ./harlo-*.x86_64.rpm"
    ["docker.io/library/archlinux:latest"]="pacman -Syu --noconfirm --quiet > /dev/null && pacman -U --noconfirm ./harlo-*-x86_64.pkg.tar.zst"
)
START='harlo --version && export QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 HOME=$(mktemp -d) XDG_RUNTIME_DIR=$(mktemp -d) && (timeout 30 harlo --check-start > /tmp/start.log 2>&1; true) && grep -q "check-start: ok" /tmp/start.log && ! grep -E "module .* is not installed|cannot open shared object|Cannot load library" /tmp/start.log || { cat /tmp/start.log; exit 1; }'

images=("${!INSTALL[@]}")
[ -n "${ONLY:-}" ] && images=($(for i in "${images[@]}"; do for o in $ONLY; do [[ $i == *"$o"* ]] && echo "$i"; done; done))
for image in "${images[@]}"; do
    name=$(basename "$image" | tr ':/' '__')
    # (A copy of the packages, so the image can't change them.)
    podman run --rm -v "$DIR:/pkgs:ro,Z" "$image" bash -c "cp -r /pkgs /work && cd /work && ${INSTALL[$image]} && $START" > "$LOGS/$name.log" 2>&1 &
    echo "$! $name" >> "$LOGS/jobs"
done
failed=0
while read -r pid name; do
    if wait "$pid"; then
        printf '%-28s ok   (%s)\n' "$name" "$(grep -m1 '^harlo [0-9]' "$LOGS/$name.log")"
    else
        printf '%-28s FAIL (log: %s)\n' "$name" "$LOGS/$name.log"
        tail -n 15 "$LOGS/$name.log" | sed 's/^/    /'
        failed=1
    fi
done < "$LOGS/jobs"
[ "$failed" = 0 ] && rm -rf "$LOGS"
exit "$failed"
