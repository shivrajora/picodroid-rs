#!/usr/bin/env bash
# Phase driver: smoke 15m -> combined 60m -> quiet hold 60m -> long until 07:00.
# Marks each phase boundary with the RTT byte offset so memmon floors can be
# extracted per phase afterward. Halts all input load on the panic flag.
SCRATCH="$(cd "$(dirname "$0")" && pwd)"
NAV=/tmp/soak-nav.log; RTT=/tmp/soak-rtt.log; FLAG=/tmp/soak-PANIC

phase() { echo "$(date '+%F %H:%M:%S') ===== PHASE: $* (rtt_off=$(wc -c < "$RTT")) =====" >> "$NAV"; }
halt_if_flag() { if [ -f "$FLAG" ]; then phase "HALTED on panic flag"; exit 9; fi; }

phase "smoke: A only, 15 min"
end=$(( $(date +%s) + 900 ))
while [ "$(date +%s)" -lt "$end" ]; do halt_if_flag; sleep 30; done

phase "smoke: first verified nav cycle"
"$SCRATCH/soak-nav.sh" || true
halt_if_flag

phase "combined: A+B+C, 60 min"
end=$(( $(date +%s) + 3600 )); i=0
while [ "$(date +%s)" -lt "$end" ]; do
  halt_if_flag
  if [ "$i" -eq 25 ]; then
    "$SCRATCH/soak-nav.sh" "" 300      # mid-phase 5-min Live dwell variant
  elif [ $(( i % 9 )) -eq 8 ]; then
    "$SCRATCH/soak-nav.sh" refresh     # ~every 10 min: forced NTP+weather
  else
    "$SCRATCH/soak-nav.sh"
  fi
  i=$((i+1))
  sleep 20
done

phase "quiet hold: A only, 60 min (memmon storage steady state)"
end=$(( $(date +%s) + 3600 ))
while [ "$(date +%s)" -lt "$end" ]; do halt_if_flag; sleep 30; done

phase "long: A + hourly B/C bursts until 07:00"
end=$(date -d 'tomorrow 07:00' +%s)
[ "$(date +%H)" -lt 7 ] && end=$(date -d 'today 07:00' +%s)
h=0
while [ "$(date +%s)" -lt "$end" ]; do
  halt_if_flag
  phase "long: hourly burst $h"
  "$SCRATCH/soak-nav.sh" || true
  "$SCRATCH/soak-nav.sh" refresh || true
  "$SCRATCH/soak-nav.sh" || true
  h=$((h+1))
  q=$(( $(date +%s) + 3000 ))          # ~50 min quiet between bursts
  while [ "$(date +%s)" -lt "$q" ] && [ "$(date +%s)" -lt "$end" ]; do halt_if_flag; sleep 30; done
done

phase "long: done — matched-idle floor window (5 min, no load changes)"
sleep 300
phase "soak complete"
