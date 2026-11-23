#!/usr/bin/env bash
# Harlo verify: the whole app, feature by feature. Runs in a memory-capped
# scope (a runaway test can't take the computer down), then verify.py.
#
#   tools/verify/verify.sh all | changed | item <id>… | check <check>… | list | report [run]
#
# See verify.py for the modes and checks, items.toml for the features.
set -u
cd "$(dirname "$0")/../.."
case "${1:-}" in
list | report | help | -h | --help) exec python3 -I tools/verify/verify.py "$@" ;;
esac
if [ -z "${VERIFY_SCOPED:-}" ]; then
    exec systemd-run --user --scope -q -p MemoryMax=16G -p MemorySwapMax=0 \
        env VERIFY_SCOPED=1 "$0" "$@"
fi
exec python3 -I tools/verify/verify.py "$@"
