#!/usr/bin/env bash
# Builds harlo's Linux packages on this machine, in the same Ubuntu 22.04
# environment as the release workflow (rootless Podman), then the AppImage,
# .deb, .rpm and Arch package land in dist/ with SHA256SUMS:
#
#   packaging/linux/build-in-container.sh && packaging/linux/test-install.sh dist
#
# The build image is made once (packaging/linux/Containerfile); Cargo's
# downloads and the build cache stay in volumes between runs.
set -euo pipefail
cd "$(dirname "$0")/../.."
IMAGE=harlo-build:22.04
# (Made once; REBUILD=1 makes it again, e.g. after the Containerfile changes.)
if [ -n "${REBUILD:-}" ] || ! podman image exists "$IMAGE"; then
    podman build -t "$IMAGE" -f packaging/linux/Containerfile packaging/linux
fi
podman run --rm -v "$PWD:/src:Z" -w /src \
    -v harlo-cargo-registry:/opt/cargo/registry -v harlo-target:/src/target-container \
    -e CARGO_TARGET_DIR=/src/target-container "$IMAGE" bash -c '
        set -euo pipefail
        git config --global --add safe.directory /src
        packaging/linux/bundle.sh
        export VERSION=$(cat dist/version)
        for p in deb rpm archlinux; do nfpm package -f packaging/linux/nfpm.yaml -p $p -t dist/; done
        cd dist && sha256sum *.AppImage *.deb *.rpm *.pkg.tar.zst > SHA256SUMS && cat SHA256SUMS'
