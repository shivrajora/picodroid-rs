#!/usr/bin/env bash
# Send a control command to a running sim-remote's keypad/touch FIFO.
#
# scripts/sim-remote.sh exports a FIFO at /tmp/picodroid-sim-remote-<N>-ctrl
# that the sim reads button/touch commands from (one command per line). The
# display number <N> is picked dynamically from :99..:150, so the path is not
# fixed. This wrapper auto-discovers the live FIFO and forwards the command,
# turning:
#
#   echo 'tap Y' > /tmp/picodroid-sim-remote-99-ctrl
#
# into:
#
#   ./scripts/sim-ctrl.sh tap Y
#
# Usage:
#   ./scripts/sim-ctrl.sh tap A            # press-and-release the 1st button
#   ./scripts/sim-ctrl.sh down B           # press-and-hold
#   ./scripts/sim-ctrl.sh up B             # release
#   ./scripts/sim-ctrl.sh press ENTER      # same as tap
#   ./scripts/sim-ctrl.sh touch down 120 80
#   ./scripts/sim-ctrl.sh touch up
#   ./scripts/sim-ctrl.sh -d 100 tap Y     # target display :100 explicitly
#
# Verbs:   down | up | press | tap        (press/tap embed a 40ms press->release)
# Buttons: A | B | X | Y | PREV | NEXT | ENTER | ESC | <gpio-pin>
# Touch:   touch down|move <x> <y> | touch up
# Memory:  memstats — one [memmon] snapshot (+histogram if enabled); needs a
#          sim built with --mem-diag (docs/memory-diagnostics.md)
#
# The command is forwarded verbatim; the sim validates it and reports unknown
# tokens on its own log ("[sim] control channel: unknown ...").
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

FIFO_GLOB="/tmp/picodroid-sim-remote-*-ctrl"
DISPLAY_NUM=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [-d N] <command...>

Send one control command to a running sim-remote's control FIFO.

Options:
  -d, --display <N>   Target display :N's FIFO (/tmp/picodroid-sim-remote-N-ctrl).
                      Default: auto-discover the single running sim-remote.
  -h, --help          Show this help message.

Commands (forwarded verbatim to the sim):
  down|up|press|tap <A|B|X|Y|PREV|NEXT|ENTER|ESC|pin>
  touch down|move <x> <y>
  touch up
  memstats     ([memmon] snapshot; sim must be built with --mem-diag)

Examples:
  $(basename "$0") tap A
  $(basename "$0") down B
  $(basename "$0") up B
  $(basename "$0") touch down 120 80
  $(basename "$0") -d 100 tap Y

Start a sim-remote first (buttons board), e.g.:
  ./scripts/sim-remote.sh --board pico_enviro_mon --app picoenvmon
EOF
}

# ── Parse leading options; everything after is the command ──────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    -d|--display)
      [[ $# -ge 2 ]] || { echo "sim-ctrl: $1 needs a display number" >&2; exit 2; }
      DISPLAY_NUM="$2"
      shift 2
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "sim-ctrl: unknown option '$1'" >&2
      usage >&2
      exit 2
      ;;
    *)
      break
      ;;
  esac
done

if [[ $# -eq 0 ]]; then
  echo "sim-ctrl: no command given" >&2
  usage >&2
  exit 2
fi

# ── Resolve the target FIFO ─────────────────────────────────────────────────
FIFO=""
if [[ -n "$DISPLAY_NUM" ]]; then
  FIFO="/tmp/picodroid-sim-remote-${DISPLAY_NUM}-ctrl"
  if [[ ! -p "$FIFO" ]]; then
    echo "sim-ctrl: no control FIFO for display :$DISPLAY_NUM ($FIFO)" >&2
    echo "  Is sim-remote running on that display? See its startup banner." >&2
    exit 1
  fi
elif [[ -n "${PICODROID_SIM_CTRL_FIFO:-}" && -p "${PICODROID_SIM_CTRL_FIFO}" ]]; then
  FIFO="$PICODROID_SIM_CTRL_FIFO"
else
  # Auto-discover: exactly one live FIFO is the common case.
  matches=()
  for f in $FIFO_GLOB; do
    [[ -p "$f" ]] && matches+=("$f")
  done
  case ${#matches[@]} in
    0)
      echo "sim-ctrl: no running sim-remote found (no $FIFO_GLOB)" >&2
      echo "  Start one first, e.g. ./scripts/sim-remote.sh --app picoenvmon" >&2
      exit 1
      ;;
    1)
      FIFO="${matches[0]}"
      ;;
    *)
      echo "sim-ctrl: multiple sim-remote instances running; pick one with -d N:" >&2
      for f in "${matches[@]}"; do
        # /tmp/picodroid-sim-remote-<N>-ctrl -> N
        n="${f#/tmp/picodroid-sim-remote-}"
        n="${n%-ctrl}"
        echo "    -d $n   ($f)" >&2
      done
      exit 1
      ;;
  esac
fi

# ── Forward the command as a single line ────────────────────────────────────
# The sim opens the FIFO read+write, so a reader is always present while it
# runs and the write returns immediately. If the sim crashed without cleanup
# the FIFO can linger with no reader and a plain write would block forever, so
# guard with a short timeout when available.
CMD="$*"
if command -v timeout >/dev/null 2>&1; then
  if ! timeout 2 bash -c 'printf "%s\n" "$1" > "$2"' _ "$CMD" "$FIFO"; then
    echo "sim-ctrl: FIFO '$FIFO' exists but no sim is reading it (stale?)" >&2
    exit 1
  fi
else
  printf '%s\n' "$CMD" > "$FIFO"
fi
