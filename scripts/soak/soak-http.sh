#!/usr/bin/env bash
# Load A: browser-realistic dashboard fetch every 2 s; 3-way concurrent
# burst every ~30 min (exercises backlog-1/RST). Halts on the panic flag.
IP=$1; OUT=/tmp/soak-http.log; FLAG=/tmp/soak-PANIC
n=0
while true; do
  [ -f "$FLAG" ] && { echo "$(date +%s) HALT panic-flag" >> "$OUT"; exit 0; }
  bytes=$(curl -s -m 8 "http://$IP:8080/" | wc -c)
  echo "$(date +%s) $bytes" >> "$OUT"
  n=$((n+1))
  if [ $((n % 900)) -eq 0 ]; then
    echo "$(date +%s) BURST3" >> "$OUT"
    for j in 1 2 3; do
      ( b=$(curl -s -m 8 "http://$IP:8080/" | wc -c); echo "$(date +%s) burst $b" >> "$OUT" ) &
    done
    wait
  fi
  sleep 2
done
