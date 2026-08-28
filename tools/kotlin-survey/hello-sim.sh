#!/usr/bin/env bash
# hello-kotlin-on-sim: pack the zero-stdlib Kotlin hello app by hand and run
# it on the host simulator with zero JVM changes.
#
# Usage:
#   tools/kotlin-survey/hello-sim.sh              # out/hellokt.papk
#   tools/kotlin-survey/hello-sim.sh --stripped   # out/hellokt-stripped.papk (ASM-stripped class)
#
# Prerequisite: ./gradlew :sdk:compileJava (the Kotlin compile classpath).
set -euo pipefail

SURVEY_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SURVEY_DIR/../.." && pwd)"

TASK="helloPapk"
PAPK="hellokt.papk"
LOG="hello-sim.log"
if [[ "${1:-}" == "--stripped" ]]; then
  TASK="helloPapkStripped"
  PAPK="hellokt-stripped.papk"
  LOG="hello-sim-stripped.log"
elif [[ $# -gt 0 ]]; then
  echo "Usage: $(basename "$0") [--stripped]" >&2
  exit 2
fi

cd "$REPO_ROOT"
./gradlew -p tools/kotlin-survey "$TASK" --console=plain
echo "==> papk-info tools/kotlin-survey/out/$PAPK"
cargo run --quiet -p papk-info -- "tools/kotlin-survey/out/$PAPK"

# The app is `term`-shaped (helloworld's category in hil-tests.conf): the sim
# falls out of main once onCreate returns, so the alarm is only a guard.
# stdout goes to a file and is grepped after exit; piping loses buffered lines
# on an alarm kill, and </dev/null keeps the control channel off our stdin.
echo "==> sim --apk tools/kotlin-survey/out/$PAPK"
set +e
PICODROID_SIM_HEADLESS=1 perl -e 'alarm 60; exec @ARGV' \
  ./scripts/sim.sh --apk "tools/kotlin-survey/out/$PAPK" \
  > "tools/kotlin-survey/out/$LOG" 2>&1 < /dev/null
rc=$?
set -e
echo "sim exit=$rc (log: tools/kotlin-survey/out/$LOG)"

TOKEN='[HelloKt] hi from kotlin 42'
if [[ $rc -eq 0 ]] && grep -qF "$TOKEN" "tools/kotlin-survey/out/$LOG"; then
  grep -F "$TOKEN" "tools/kotlin-survey/out/$LOG"
  echo "hello-kotlin-on-sim ($PAPK): PASS"
else
  echo "hello-kotlin-on-sim ($PAPK): FAIL — last 40 log lines:" >&2
  tail -40 "tools/kotlin-survey/out/$LOG" >&2
  exit 1
fi
