#!/usr/bin/env bash
# Shared helpers for the picoenvmon device soak (2026-08-16).
# Every press is verified against the RTT log (new `key:`/`activity:` lines).
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RTT="${SOAK_RTT:-/tmp/soak-rtt.log}"
NAV="${SOAK_NAV:-/tmp/soak-nav.log}"
FLAG="${SOAK_FLAG:-/tmp/soak-PANIC}"
SEL_FILE=/tmp/soak-hub-selection   # 0=Live 1=History 2=Network 3=Settings
SCREENS=(Live History Network Settings)
CLASSES=(LiveActivity HistoryActivity NetworkActivity SettingsActivity)

ts() { date '+%H:%M:%S'; }
nav() { echo "$(ts) $*" >> "$NAV"; }
sel_get() { cat "$SEL_FILE" 2>/dev/null || echo 0; }
sel_set() { echo "$1" > "$SEL_FILE"; }
rtt_off() { wc -c < "$RTT"; }
rtt_since() { tail -c +"$(( $1 + 1 ))" "$RTT"; }

# await_line OFFSET REGEX [TIMEOUT_S] -> 0 if the line appears past OFFSET
await_line() {
  local off=$1 re=$2 t=${3:-3} i=0 steps
  steps=$(( t * 5 ))
  while [ "$i" -lt "$steps" ]; do
    rtt_since "$off" | grep -q -E "$re" && return 0
    sleep 0.2; i=$((i+1))
  done
  return 1
}

# press KEYCODE [EXPECT_RE [TIMEOUT]] -> verified injection.
# Checks, in order: pdb exit code, the `key: code=N` dispatch line,
# then the optional expectation (push/pop/onCreate/dialog line).
press() {
  local key=$1 expect=${2:-} t=${3:-5} off
  [ -f "$FLAG" ] && return 9
  off=$(rtt_off)
  if ! "$REPO/scripts/pdb.sh" input keyevent "$key" >/dev/null 2>&1; then
    nav "FAIL key=$key pdb-exit"; return 1
  fi
  if ! await_line "$off" "key: code=$key" 3; then
    nav "FAIL key=$key no-dispatch-log"; return 2
  fi
  if [ -n "$expect" ] && ! await_line "$off" "$expect" "$t"; then
    nav "FAIL key=$key missed expect='$expect'"; return 3
  fi
  nav "PASS key=$key${expect:+ expect='$expect'}"
  return 0
}

# resync_to_hub: press Y until neither a pop nor a dialog-dismiss shows up.
# Y at the hub is a no-op (HomeActivity.onBackPressed override), so this
# converges on the hub from any screen/dialog/edit-mode state.
resync_to_hub() {
  local i off
  for i in 1 2 3 4 5 6; do
    off=$(rtt_off)
    press 4 || true
    sleep 0.5
    if ! rtt_since "$off" | grep -q -E "activity: pop|dialog dismissed"; then
      nav "resync: at hub (after $i Y)"
      return 0
    fi
  done
  nav "resync: FAILED after 6 Y"; return 1
}

# open_screen TARGET_IDX. LVGL focus WRAPS, so navigate relatively from the
# tracked selection; the `activity: push <Class>` line is ground truth and
# corrects the model on any miss.
open_screen() {
  local target=$1 sel moves i off pushed
  sel=$(sel_get)
  moves=$(( (target - sel + 4) % 4 ))
  for ((i=0; i<moves; i++)); do press 20 || true; sleep 0.4; done
  off=$(rtt_off)
  press 23 "activity: push" 5 || { nav "open: no push for idx=$target"; resync_to_hub; return 1; }
  pushed=$(rtt_since "$off" | grep -o -E "activity: push [^ ]*" | head -1)
  for i in 0 1 2 3; do
    if echo "$pushed" | grep -q "${CLASSES[$i]}"; then
      sel_set "$i"
      if [ "$i" -eq "$target" ]; then nav "open: ${SCREENS[$i]} OK"; return 0; fi
      nav "open: MISS wanted ${SCREENS[$target]} got ${SCREENS[$i]}"
      press 4 "activity: pop" || true
      return 1
    fi
  done
  nav "open: unrecognized push '$pushed'"; resync_to_hub; return 1
}
