#!/usr/bin/env bash
# The iPhone app's model (HarloKit) against a real Harlo server: a sandbox
# server on the scripted model, its phone access on, and harlo-probe paired
# from its link (ios/HarloKit/Sources/harlo-probe). Prints the probe's
# checks; exits non-zero if any failed.
#
#   tools/ios-e2e.sh
#
# Needs Swift (swift build in ios/HarloKit) and target/debug/harlo-server.
# Never touches the real config, state or background service (tools/sandbox.sh).
set -euo pipefail

TOOLS=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$TOOLS/.." && pwd)

(cd "$ROOT/ios/HarloKit" && swift build -q --product harlo-probe)
PROBE=$(cd "$ROOT/ios/HarloKit" && swift build --show-bin-path)/harlo-probe

eval "$("$TOOLS/sandbox.sh")"
trap '"$TOOLS/sandbox.sh" stop "$DIR"' EXIT
api() { curl -sf -u "opencode:$PASSWORD" -H 'content-type: application/json' "$@"; }

# A branch to switch to, and a session so the project is listed.
git -C "$PROJECT" branch feature
api -X POST "$URL/api/session?directory=$PROJECT" -d '{}' > /dev/null

# Phone access on this machine only (no relay), and a pairing link.
api -X PUT "$URL/harlo/phone" \
  -d '{"on":true,"relay":false,"paused":false,"port":0,"name":"Harlo sandbox","hidden":[],"theme":null}' > /dev/null
link=$(api -X POST "$URL/harlo/phone/pair" | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["link"])')

HARLO_E2E_REPO=$PROJECT "$PROBE" "$link"
