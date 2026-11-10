#!/bin/sh
# Launches the picker with demo data, fails if it does not stay running, and saves a screenshot.
#
# Usage: smoke-test-macos.sh <executable> <screenshot.png> [arguments...]
set -eu

executable=$1
screenshot=$2
shift 2

"$executable" --demo "$@" &
pid=$!
sleep 5

if ! kill -0 "$pid" 2>/dev/null; then
    status=0
    wait "$pid" || status=$?
    echo "VS Recent exited during startup with status $status" >&2
    exit 1
fi

# The screenshot is for reviewing the interface. Only the launch decides the result.
mkdir -p "$(dirname "$screenshot")"
screencapture -x "$screenshot" || echo "warning: could not capture the screen" >&2

kill "$pid"
wait "$pid" 2>/dev/null || true
echo "VS Recent stayed running; screenshot saved to $screenshot"
