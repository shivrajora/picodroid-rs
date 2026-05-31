#!/usr/bin/env bash
# Run a picodroid app in host simulator mode with remote browser-based UI.
#
# Spins up Xvfb + x11vnc + noVNC on this machine, then runs scripts/sim.sh
# against the virtual X display.  A developer SSH'd in (e.g. from VSCode
# Remote-SSH on Windows) can interact with the sim's UI from a web browser
# on their local machine — VSCode auto-forwards the noVNC port.
#
# All other flags are forwarded to scripts/sim.sh.
#
# Usage:
#   ./scripts/sim-remote.sh                          # default app
#   ./scripts/sim-remote.sh --app displaydemo
#   ./scripts/sim-remote.sh --app gesturedemo --release
#
# Open the printed http://localhost:6080/vnc.html?... URL in your browser.
#
# Env overrides (defaults shown):
#   PICODROID_VNC_PORT=5901                # localhost-only VNC port
#   PICODROID_WEB_PORT=6080                # noVNC HTTP port (forwarded)
#   PICODROID_VNC_GEOMETRY=800x600x24
#   PICODROID_NOVNC_DIR=/usr/share/novnc
#
# One-time install on the server:
#   sudo apt-get install -y xvfb x11vnc novnc websockify xdotool
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

VNC_PORT="${PICODROID_VNC_PORT:-5901}"
WEB_PORT="${PICODROID_WEB_PORT:-6080}"
GEOMETRY="${PICODROID_VNC_GEOMETRY:-800x600x24}"
NOVNC_DIR="${PICODROID_NOVNC_DIR:-/usr/share/novnc}"

# ── Pre-flight: required tools and noVNC web root ───────────────────────────
missing=()
for tool in Xvfb x11vnc websockify; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    missing+=("$tool")
  fi
done
if [[ ! -d "$NOVNC_DIR" ]]; then
  missing+=("noVNC web root ($NOVNC_DIR)")
fi
if [[ ${#missing[@]} -gt 0 ]]; then
  echo "sim-remote: missing dependencies: ${missing[*]}" >&2
  echo "Install with:" >&2
  echo "  sudo apt-get install -y xvfb x11vnc novnc websockify xdotool" >&2
  exit 1
fi

# ── Pick a free X display (:99..:150) ───────────────────────────────────────
DISPLAY_NUM=""
for n in $(seq 99 150); do
  if [[ ! -e "/tmp/.X${n}-lock" ]]; then
    DISPLAY_NUM=$n
    break
  fi
done
if [[ -z "$DISPLAY_NUM" ]]; then
  echo "sim-remote: no free X display number in :99..:150" >&2
  exit 1
fi

# ── Pick free TCP ports, incrementing from the defaults ─────────────────────
port_in_use() {
  local p="$1"
  ss -ltn 2>/dev/null | awk -v p=":$p\$" '$4 ~ p { found=1 } END { exit !found }'
}
while port_in_use "$VNC_PORT"; do VNC_PORT=$((VNC_PORT + 1)); done
while port_in_use "$WEB_PORT"; do WEB_PORT=$((WEB_PORT + 1)); done

# ── Trap cleanup before starting any background process ─────────────────────
LOG_PREFIX="/tmp/picodroid-sim-remote-${DISPLAY_NUM}"
XVFB_PID=""
X11VNC_PID=""
WS_PID=""
SIM_PID=""
FOCUS_PID=""

# Recursively kill a PID and all its descendants (children, grandchildren).
# Needed because sim.sh spawns `cargo run`, which spawns the picodroid
# binary; a plain `kill $SIM_PID` only takes out sim.sh and orphans the
# grandchildren onto init.
kill_tree() {
  local pid="$1" sig="${2:-TERM}" child
  for child in $(pgrep -P "$pid" 2>/dev/null || true); do
    kill_tree "$child" "$sig"
  done
  kill -"$sig" "$pid" 2>/dev/null || true
}

cleanup() {
  trap - INT TERM EXIT
  if [[ -n "$FOCUS_PID" ]];   then kill "$FOCUS_PID"   2>/dev/null || true; fi
  if [[ -n "$SIM_PID" ]]; then
    kill_tree "$SIM_PID" TERM
    sleep 0.3
    kill_tree "$SIM_PID" KILL
  fi
  if [[ -n "$WS_PID" ]];      then kill "$WS_PID"      2>/dev/null || true; fi
  if [[ -n "$X11VNC_PID" ]];  then kill "$X11VNC_PID"  2>/dev/null || true; fi
  if [[ -n "$XVFB_PID" ]];    then kill "$XVFB_PID"    2>/dev/null || true; fi
  rm -f "/tmp/.X${DISPLAY_NUM}-lock" "/tmp/.X11-unix/X${DISPLAY_NUM}" 2>/dev/null || true
}
trap cleanup INT TERM EXIT

# ── Start Xvfb ──────────────────────────────────────────────────────────────
echo "sim-remote: starting Xvfb on :$DISPLAY_NUM ($GEOMETRY)"
Xvfb ":$DISPLAY_NUM" -screen 0 "$GEOMETRY" -nolisten tcp \
  >"${LOG_PREFIX}-xvfb.log" 2>&1 &
XVFB_PID=$!

# Wait up to ~5s for the lock file to appear (signals Xvfb is serving).
for _ in $(seq 1 50); do
  [[ -e "/tmp/.X${DISPLAY_NUM}-lock" ]] && break
  sleep 0.1
done
if ! kill -0 "$XVFB_PID" 2>/dev/null; then
  echo "sim-remote: Xvfb failed to start. See ${LOG_PREFIX}-xvfb.log" >&2
  exit 1
fi
if [[ ! -e "/tmp/.X${DISPLAY_NUM}-lock" ]]; then
  echo "sim-remote: Xvfb lock file did not appear within 5s." >&2
  exit 1
fi

# ── Start x11vnc bound to localhost ─────────────────────────────────────────
echo "sim-remote: starting x11vnc on localhost:$VNC_PORT"
x11vnc -display ":$DISPLAY_NUM" -localhost -rfbport "$VNC_PORT" \
  -nopw -forever -shared -quiet \
  >"${LOG_PREFIX}-x11vnc.log" 2>&1 &
X11VNC_PID=$!
sleep 0.3
if ! kill -0 "$X11VNC_PID" 2>/dev/null; then
  echo "sim-remote: x11vnc failed to start. See ${LOG_PREFIX}-x11vnc.log" >&2
  exit 1
fi

# ── Start websockify + noVNC ────────────────────────────────────────────────
echo "sim-remote: starting websockify+noVNC on port $WEB_PORT"
websockify --web="$NOVNC_DIR" "$WEB_PORT" "localhost:$VNC_PORT" \
  >"${LOG_PREFIX}-novnc.log" 2>&1 &
WS_PID=$!
sleep 0.3
if ! kill -0 "$WS_PID" 2>/dev/null; then
  echo "sim-remote: websockify failed to start. See ${LOG_PREFIX}-novnc.log" >&2
  exit 1
fi

# ── Banner ──────────────────────────────────────────────────────────────────
URL="http://localhost:${WEB_PORT}/vnc.html?host=localhost&port=${WEB_PORT}&autoconnect=true&resize=remote"
cat <<EOF

============================================================
 picodroid sim-remote ready
   Open in your local browser (VSCode Remote-SSH forwards ${WEB_PORT}):
     ${URL}
   Keyboard: click the sim image once so the browser forwards keys.
   Display: :${DISPLAY_NUM}    VNC: localhost:${VNC_PORT}
   Logs:    ${LOG_PREFIX}-{xvfb,x11vnc,novnc}.log
   Press Ctrl-C in this terminal to stop.
============================================================

EOF

# ── Hand off to sim.sh on the virtual display ───────────────────────────────
# Run sim.sh as a backgrounded subprocess and `wait` so we keep our PID
# captured for the cleanup trap. If the wrapper is killed externally (or
# sim.sh hangs), the EXIT trap calls kill_tree on $SIM_PID and brings down
# cargo + the picodroid binary too — without this they would orphan onto
# init.
DISPLAY=":$DISPLAY_NUM" "$SCRIPT_DIR/sim.sh" "$@" &
SIM_PID=$!

# ── Keep X input focus on the sim window so browser keystrokes land ─────────
# Xvfb has no window manager, so nothing gives the minifb window X input focus.
# X routes keyboard events — including those x11vnc injects from the browser —
# to the *focused* window; by default that's PointerRoot (the window under the
# pointer), so VNC keys are silently dropped whenever the cursor isn't over the
# sim. Poll for the window and pin focus to it, re-asserting periodically so it
# survives VNC reconnects. Mouse input is coordinate-based and works regardless.
# Requires xdotool — without it, keyboard still works only while the VNC cursor
# hovers the sim image.
if command -v xdotool >/dev/null 2>&1; then
  (
    while kill -0 "$SIM_PID" 2>/dev/null; do
      wid=$(DISPLAY=":$DISPLAY_NUM" xdotool search --name picodroid 2>/dev/null | head -1)
      [[ -n "$wid" ]] && DISPLAY=":$DISPLAY_NUM" xdotool windowfocus "$wid" 2>/dev/null || true
      sleep 1
    done
  ) &
  FOCUS_PID=$!
else
  echo "sim-remote: xdotool not found — install it so VNC keyboard input works" >&2
  echo "  reliably (sudo apt-get install -y xdotool). Without it, keep the VNC" >&2
  echo "  cursor over the sim image while typing." >&2
fi

SIM_EXIT=0
wait "$SIM_PID" || SIM_EXIT=$?
exit "$SIM_EXIT"
