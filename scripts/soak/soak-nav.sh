#!/usr/bin/env bash
# One nav-churn cycle over all four screens (the documented worst fragmenter).
# Usage: soak-nav.sh [refresh] [dwell_secs_on_live]
source "$(dirname "$0")/soak-lib.sh"
REFRESH=${1:-}
DWELL=${2:-5}
[ -f "$FLAG" ] && exit 9
nav "=== cycle start (refresh=${REFRESH:-no} dwell=${DWELL}s) ==="

# Live: open, dwell, back. NEVER press X here — the focusable Logger Switch
# would toggle sensor logging off (must stay ON for the whole soak).
if open_screen 0; then sleep "$DWELL"; press 4 "activity: pop" || resync_to_hub; fi
sleep 1

# History: open, X pops a row AlertDialog, Y dismisses it (verified via the
# new dialog-dismiss log), second Y exits the screen.
if open_screen 1; then
  sleep 1; press 23 || true; sleep 1
  press 4 "dialog dismissed" 3 || true
  sleep 0.5; press 4 "activity: pop" || resync_to_hub
fi
sleep 1

# Network: open, optional forced NTP+weather refresh (X clicks the only
# focusable widget), back. Refresh windows cause EXPECTED http timeouts.
if open_screen 2; then
  if [ -n "$REFRESH" ]; then nav "refresh: forcing NTP+weather"; press 23 || true; sleep 2; fi
  press 4 "activity: pop" || resync_to_hub
fi
sleep 1

# Settings: open, poke X once (may enter NumberPicker edit mode or click a
# widget), then Y (exits edit mode if entered) and Y (pop). resync covers
# whichever state X actually produced.
if open_screen 3; then
  sleep 1; press 23 || true; sleep 0.5
  press 4 || true; sleep 0.5
  press 4 "activity: pop" 3 || resync_to_hub
fi
nav "=== cycle end ==="
