#!/usr/bin/env bash
# Runs the iPhone app's UI tests on GitHub's macOS runners (the iOS
# workflow with uitests on, no TestFlight upload) against the branch as
# pushed, waits for them, and downloads each run's pictures, accessibility
# audits and failures into a folder.
#
#   tools/ios-ui-tests.sh [quick|full] [branch]   quick (one phone) by default;
#                                                 the current branch by default
#
# Needs gh, signed in. The tests replay ios/fixtures/recording.jsonl
# (tools/ios-record.sh), so they use no server.
set -euo pipefail

set_name=${1:-quick}
branch=${2:-$(git rev-parse --abbrev-ref HEAD)}
[[ $set_name == quick || $set_name == full ]] || { echo "quick or full, not $set_name" >&2; exit 2; }
out=${IOS_UI_OUT:-$(git rev-parse --show-toplevel)/target/ios-ui-tests}

before=$(gh run list --workflow ios.yml --branch "$branch" --limit 1 --json databaseId --jq '.[0].databaseId // 0')
gh workflow run ios.yml --ref "$branch" -f uitests=true -f uitests_set="$set_name" -f testflight=false -f screenshots=false
# The new run appears a moment later.
run=""
for _ in $(seq 1 60); do
  run=$(gh run list --workflow ios.yml --branch "$branch" --limit 1 --json databaseId --jq '.[0].databaseId // 0')
  [[ $run != "$before" && $run != 0 ]] && break
  sleep 2
done
[[ -n $run && $run != "$before" ]] || { echo "the run didn't start" >&2; exit 1; }
echo "run $run: $(gh run view "$run" --json url --jq .url)"

status=0
gh run watch "$run" --exit-status --interval 30 > /dev/null || status=$?
rm -rf "$out"
mkdir -p "$out"
gh run download "$run" --dir "$out" --pattern 'ui-test*' || true
echo "artifacts in $out"
gh run view "$run" --json jobs --jq '.jobs[] | "\(.conclusion // .status)\t\(.name)"'
exit "$status"
