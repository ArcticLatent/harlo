#!/bin/sh
# The release build (a stand-in for the tour: it takes a while, then says so).
echo "Building the release…"
sleep "${TOUR_BUILD_SECONDS:-12}"
echo "Finished release build: no warnings."
