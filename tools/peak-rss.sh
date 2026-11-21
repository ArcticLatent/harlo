#!/usr/bin/env bash
# Runs a command and prints its peak resident memory (whole process tree) in MB.
#   tools/peak-rss.sh <command> [args…]
"$@" &
pid=$!
peak=0
while kill -0 "$pid" 2>/dev/null; do
  total=0
  for p in $pid $(pgrep -P "$pid"); do
    kb=$(awk '/^VmRSS/{print $2}' "/proc/$p/status" 2>/dev/null)
    total=$((total + ${kb:-0}))
  done
  (( total > peak )) && peak=$total
  sleep 0.2
done
wait "$pid"; code=$?
echo "peak_rss_mb=$((peak / 1024)) exit=$code"
