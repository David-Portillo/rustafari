#!/usr/bin/env bash
#
# Exercises the running app the way a person does, for verifying UI changes
# that no test, lint or screenshot can catch.
#
#   ./scripts/ui-probe.sh window                  window id and screen rect
#   ./scripts/ui-probe.sh shot out.png            screenshot, this app only
#   ./scripts/ui-probe.sh click X Y               real click at screen coords
#   ./scripts/ui-probe.sh drag X1 Y1 X2 Y2        real press-move-release
#   ./scripts/ui-probe.sh raise                   bring the app to the front
#
# Coordinates are screen points. `window` prints the app's origin, so a point
# inside the window is origin + offset; read offsets off a `shot`, remembering
# that a Retina capture has twice the pixels of the points you click in.
#
# macOS only. Needs two permissions for whichever terminal runs it, both under
# System Settings -> Privacy & Security:
#   - Screen & System Audio Recording  (for `shot`)
#   - Accessibility                    (to post input events)
#
# `shot` captures by window id rather than by screen region. That is
# deliberate: a region capture picks up whatever else happens to be on screen,
# including windows that are none of this project's business.
set -euo pipefail

cd "$(dirname "$0")/.."

APP=${RUSTAFARI_APP_NAME:-rustafari}
SRC=scripts/ui-probe.swift
BIN=target/ui-probe

if [[ "$(uname)" != "Darwin" ]]; then
	echo "ui-probe: macOS only (it posts CGEvents)" >&2
	exit 1
fi

# Rebuild only when the source is newer; swiftc takes a second or so.
if [[ ! -x "$BIN" || "$SRC" -nt "$BIN" ]]; then
	mkdir -p target
	swiftc -O "$SRC" -o "$BIN"
fi

command=${1:-}
shift || true

case "$command" in
window)
	"$BIN" window "$APP"
	;;
raise)
	osascript -e "tell application \"$APP\" to activate" >/dev/null 2>&1 || true
	sleep 1
	;;
shot)
	out=${1:?usage: shot <out.png>}
	read -r id _ _ _ _ <<<"$("$BIN" window "$APP")"
	screencapture -x -o -l"$id" "$out"
	echo "$out"
	;;
click | drag)
	"$BIN" "$command" "$@"
	;;
*)
	sed -n '3,20p' "$0" | sed 's/^# \{0,1\}//'
	exit 1
	;;
esac
