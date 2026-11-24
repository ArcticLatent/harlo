#!/usr/bin/env bash
# Runs opencode-client's tests, written against OpenCode's server, against
# harlo-server instead: the client's sandbox starts whatever binary
# HARLO_OPENCODE_BIN names with the same `serve` arguments.
#
#   tools/engine-conformance.sh            the suites harlo-server passes
#   tools/engine-conformance.sh m7 git     these suites
#
# Each test runs its own server in scratch folders with the scripted
# model, as the client's tests always do.
set -euo pipefail
cd "$(dirname "$0")/.."

# The suites harlo-server passes, each with the tests it leaves out and why.
declare -A SKIP=(
	# The first four drive the `opencode` command line as a second client.
	# undo_with_user_edit expects the person's later line in the agent's
	# file to be lost; Harlo's undo keeps it, or reports the file, instead.
	[m0]="attach_and_detach queued_prompt_across_disconnect approval_across_disconnect question_across_disconnect undo_with_user_edit"
	[m3]=""
	[m4]=""
	[m6]=""
	[endpoint]=""
	[events]=""
	[asides]=""
	[git]=""
	[shell]=""
	[transfer]=""
	[projects]=""
)

suites=("$@")
if [ ${#suites[@]} -eq 0 ]; then
	suites=(m0 m3 m4 m6 endpoint events asides git shell transfer projects)
fi

cargo build -q -p harlo-server
export HARLO_OPENCODE_BIN="$PWD/target/debug/harlo-server"

failed=()
for suite in "${suites[@]}"; do
	args=()
	for test in ${SKIP[$suite]:-}; do
		args+=(--skip "$test")
	done
	echo "== $suite"
	if ! cargo test -q -p opencode-client --test "$suite" -- "${args[@]}"; then
		failed+=("$suite")
	fi
done

if [ ${#failed[@]} -ne 0 ]; then
	echo "failed: ${failed[*]}" >&2
	exit 1
fi
echo "all passed: ${suites[*]}"
