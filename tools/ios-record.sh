#!/usr/bin/env bash
# Records the iPhone app's tour (ios/fixtures/recording.jsonl): the app's
# UI tests and screenshots replay it (`-replay <scene>`). A sandbox server
# on the scripted model works in the tour's project (ios/fixtures/tour),
# with its two models ("Tour Coder" and "Tour Helper") answering from its
# scripts.json; harlo-probe pairs and drives it as the app would.
#
#   tools/ios-record.sh [recording.jsonl]
#
# Needs Swift (swift build in ios/HarloKit) and target/debug/harlo-server.
# Never touches the real config, state or background service (tools/sandbox.sh).
set -euo pipefail

TOOLS=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$TOOLS/.." && pwd)
OUT=$(realpath -m "${1:-$ROOT/ios/fixtures/recording.jsonl}")
TOUR=$ROOT/ios/fixtures/tour

(cd "$ROOT/ios/HarloKit" && swift build -q --product harlo-probe)
PROBE=$(cd "$ROOT/ios/HarloKit" && swift build --show-bin-path)/harlo-probe

eval "$("$TOOLS/sandbox.sh")"
trap '"$TOOLS/sandbox.sh" stop "$DIR"' EXIT
api() { curl -sf -u "opencode:$PASSWORD" -H 'content-type: application/json' "$@"; }

# The tour's project, as a repository with a second branch.
repo=$DIR/harlo
cp -r "$TOUR/project" "$repo"
git -C "$repo" init -q -b main
git -C "$repo" add .
git -C "$repo" -c user.name=Tour -c user.email=tour@example.com commit -qm "The sidebar and its projects"
git -C "$repo" branch relay-limits

# The tour's models, answering from its scripts (beside the model's log).
cp "$TOUR/scripts.json" "$DIR/scripts.json"
python3 -I - "$DIR/config/opencode/opencode.json" <<'PY'
import json, sys
path = sys.argv[1]
config = json.load(open(path))
fake = config["providers"].pop("fake")
fake["name"] = "Tour"
fake["models"] = {
    "tour-coder": {"name": "Tour Coder", "limit": {"context": 128000, "output": 8192}},
    "tour-helper": {"name": "Tour Helper", "limit": {"context": 128000, "output": 8192}},
}
config["providers"]["tour"] = fake
config["model"] = "tour/tour-coder"
json.dump(config, open(path, "w"), indent=2)
PY
"$TOOLS/sandbox.sh" restart "$DIR" > /dev/null

# The tour's first session, a short question on the other model, started
# here: phones list the projects that have sessions.
id=$(api -X POST "$URL/api/session?directory=$repo" -d '{"model":{"providerID":"tour","id":"tour-helper"}}' \
  | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["data"]["id"])')
api -X POST "$URL/api/session/$id/prompt" -d '{"text":"Why does the relay drop idle phones?"}' > /dev/null

# Phone access on this machine only (no relay), and a pairing link.
api -X PUT "$URL/harlo/phone" \
  -d '{"on":true,"relay":false,"paused":false,"port":0,"name":"Studio","hidden":[],"theme":null}' > /dev/null
link=$(api -X POST "$URL/harlo/phone/pair" | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["link"])')

"$PROBE" record "$link" "$OUT"
echo "recorded $(grep -c '"scene"' "$OUT") scenes in $OUT"
