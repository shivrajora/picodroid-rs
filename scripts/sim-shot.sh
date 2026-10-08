#!/usr/bin/env bash
# Screenshot the simulator: run an app under its own X server, feed it control-channel
# lines, and save what the window shows.
#
#   ./scripts/sim-shot.sh <out.png> <settle-seconds> <sim.sh args...> [-- <ctrl line>...]
#
#   ./scripts/sim-shot.sh shots/clock.png 8 --app picoclock --board testbench_rp2350
#   ./scripts/sim-shot.sh shots/menu.png 8 --app menudemo --board testbench_rp2350 \
#       -- "input tap 297 217" "input tap 160 62"
#   PICODROID_BOOT=launcher ./scripts/sim-shot.sh shots/l.png 12 --app helloworld \
#       --board pico_display2_w --system-apps -- "input keyevent 23"
#
# Everything before `--` goes to sim.sh as given (`--app`, `--apk`, `--board`,
# `--system-apps`, …); exported environment (PICODROID_BOOT, PICODROID_SIM_FS, …) reaches it
# too. After <settle-seconds> each control line is written to the simulator's control
# channel (the verbs `pdb input` takes: `input tap X Y`, `input swipe …`, `input keyevent
# [--longpress|--down|--up] CODE`, `input back`, `apps install <papk>`, …) two seconds
# apart, then the window is captured and the simulator stopped. The simulator's log is
# saved next to the image as <out>.log, which is where the `[layout]`, `window`, `[res]`
# and app lines are read. Needs Xvfb, xdotool and scrot. One capture per invocation: a
# before/after pair is two runs.
set -u
if [ $# -lt 3 ]; then
  sed -n 2,20p "$0"
  exit 1
fi
OUT=$1; SETTLE=$2; shift 2
SIM_ARGS=()
while [ $# -gt 0 ] && [ "$1" != "--" ]; do
  SIM_ARGS+=("$1"); shift
done
[ $# -gt 0 ] && shift # the "--"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LOG="${OUT%.png}.log"
mkdir -p "$(dirname "$OUT")"
# A display number nobody else is likely to hold; one Xvfb per run.
DISP=":$((200 + RANDOM % 500))"
FIFO="$(mktemp -u "${TMPDIR:-/tmp}/sim-shot-XXXXXX.fifo")"
mkfifo "$FIFO"
Xvfb "$DISP" -screen 0 1024x768x24 >/dev/null 2>&1 &
XPID=$!
sleep 1
PICODROID_SIM_CTRL_FIFO="$FIFO" DISPLAY="$DISP" "$SCRIPT_DIR/sim.sh" "${SIM_ARGS[@]}" > "$LOG" 2>&1 &
SIMPID=$!
# Hold the FIFO open for writing so the simulator's reader never sees EOF.
exec 3>"$FIFO"
sleep "$SETTLE"
for line in "$@"; do
  echo "$line" >&3
  sleep 2
done
WID="$(DISPLAY="$DISP" xdotool search --name picodroid | head -1)"
rc=0
if [ -n "$WID" ]; then
  DISPLAY="$DISP" scrot --window "$WID" --overwrite "$OUT" && echo "shot: $OUT (log: $LOG)"
else
  echo "no picodroid window; log tail:" >&2
  tail -5 "$LOG" >&2
  rc=1
fi
exec 3>&-
kill "$SIMPID" 2>/dev/null; sleep 0.5; pkill -P "$SIMPID" 2>/dev/null
kill "$XPID" 2>/dev/null
rm -f "$FIFO"
exit $rc
