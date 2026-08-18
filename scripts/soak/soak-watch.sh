#!/usr/bin/env bash
# Alarm watcher: scans new RTT bytes for alarming lines, sets the panic flag
# on crash/corruption signatures, watches probe liveness and the dashboard
# uptime footer (reboot detector). Runs until killed.
RTT=/tmp/soak-rtt.log; AL=/tmp/soak-alerts.log; FLAG=/tmp/soak-PANIC
IP=$1
off=0; lastup=-1; t=0
ALARM='panicked at|HardFault|Firmware exited|holds poison|integrity violation|OOM:|Thread\.start.*failed|native miss|LEAK\?|bind failed'
FATAL='panicked at|HardFault|Firmware exited|holds poison|integrity violation'
while true; do
  sz=$(wc -c < "$RTT" 2>/dev/null || echo 0)
  if [ "$sz" -gt "$off" ]; then
    chunk=$(tail -c +$((off+1)) "$RTT")
    echo "$chunk" | grep -E "$ALARM" | sed "s/^/$(date '+%H:%M:%S') /" >> "$AL"
    if echo "$chunk" | grep -q -E "$FATAL"; then
      date > "$FLAG"; echo "$(date '+%H:%M:%S') PANIC FLAG SET" >> "$AL"
    fi
    off=$sz
  fi
  pgrep -f 'probe-r[s]' >/dev/null || echo "$(date '+%H:%M:%S') WARN probe-rs attach not running" >> "$AL"
  t=$((t+1))
  if [ $((t % 6)) -eq 0 ]; then   # every ~60 s
    up=$(curl -s -m 5 "http://$IP:8080/" | grep -o -E '[0-9]+h [0-9]+m' | head -1)
    if [ -n "$up" ]; then
      mins=$(( $(echo "$up" | sed -E 's/([0-9]+)h ([0-9]+)m/\1*60+\2/') ))
      if [ "$lastup" -ge 0 ] && [ "$mins" -lt "$lastup" ]; then
        echo "$(date '+%H:%M:%S') REBOOT DETECTED uptime ${lastup}m -> ${mins}m" >> "$AL"
        date > "$FLAG"
      fi
      lastup=$mins
    fi
  fi
  sleep 10
done
