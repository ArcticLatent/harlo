#!/usr/bin/env bash
source ./lib.sh

# Builds the app.
build() {
    cargo build --release
    helper "$1"
}

function helper {
    echo "$1"
}

build "$@"
