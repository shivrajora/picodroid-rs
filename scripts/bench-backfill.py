#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Recover benchmark metrics from nightly run logs into bench/parity/history.csv.

The nightly sim/HIL crons have been writing timed benchmark output to
build/{sim,hil}/logs/<RUN_ID>/ since June 2026 and nothing has ever read it.
This walks those directories (or one fresh --run-dir) and emits the schema-v2
rows that scripts/bench-report.py trends.

Schema: utc,commit,env,board,app,mode,split,metric,value

Note on coverage: the crons do not enable the `parity-metrics` or `mem-diag`
cargo features, so historical logs carry no `parity: insns=...` counters and no
`[memmon]` windows. Those metrics are parsed here for forward-looking runs
(scripts/parity-bench.sh enables parity-metrics) but will be absent from the
backfill. What historical logs DO carry: per-microbench splits, GC counts,
heap peaks, OOM detail, and lazy-load class counts.

Usage:
  ./scripts/bench-backfill.py                      # full backfill, both envs
  ./scripts/bench-backfill.py --env sim            # one env
  ./scripts/bench-backfill.py --run-dir <dir>      # one run (called by the crons)
  ./scripts/bench-backfill.py --dry-run            # print rows, write nothing
"""

import argparse
import csv
import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CSV_PATH = REPO / "bench" / "parity" / "history.csv"
HEADER = ["utc", "commit", "env", "board", "app", "mode", "split", "metric", "value"]

# Apps whose numbers may drive accept/reject. Everything else is held out and
# may only veto -- see the campaign plan's overfitting guard.
TRAIN_APPS = {"benchmark", "perfbench"}

# sim-run.sh hardcodes two board-matrix lanes; hil-tests.conf gives netexception
# a board override in its 5th field. Everything else runs on the default board.
DEFAULT_BOARD = "testbench_rp2350"
BOARD_BY_APP = {
    "picoenvmon-enviro": "pico_enviro_mon",
    "picoenvmon-enviro-w": "pico_enviro_mon_w",
    "netexception": "testbench_rp2350w",
}

RUN_ID_RE = re.compile(r"^(\d{4}-\d{2}-\d{2})_(\d{2})h(\d{2})m(\d{2})s_([0-9a-f]+)$")

# defmt/RTT framing: "[INFO ] Tag: msg (picodroid_core src/util/log.rs:28)"
DEFMT_RE = re.compile(r"^\[[A-Z ]+\]\s*(.*?)\s*\([a-z_]+ src/[^)]*\)\s*$")
# sim framing: "[Tag] msg"
SIMTAG_RE = re.compile(r"^\[([A-Za-z0-9_]+)\]\s+(.*)$")

TOTAL_RE = re.compile(r"^TOTAL:\s+(\d+)\s+(ms|us)$")
SUBTEST_RE = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*):\s+(\d+)\s+(ms|us)$")
# perfbench/graphicsbench: "int_arith: wall 120 ms (gc 0 ms / 0 cyc), peak +0 KB -> score 120"
PERFTEST_RE = re.compile(
    r"^([A-Za-z_][A-Za-z0-9_]*):\s+wall\s+(\d+)\s+ms\s+\(gc\s+(\d+)\s+ms\s*/\s*(\d+)\s+cyc\),"
    r"\s+peak\s+\+(\d+)\s+KB\s+->\s+score\s+(\d+)$"
)
SCORE_RE = re.compile(r"^SCORE\s+(\d+)$")
SUBSCORE_RE = re.compile(r"^SUBSCORE\s+(.+)$")
KV_RE = re.compile(r"([A-Za-z_][A-Za-z0-9_]*)=(-?\d+)")

PARITY_RE = re.compile(
    r"parity:\s+insns=(\d+)\s+allocs=(\d+)\s+gcs=(\d+)\s+bands=(\d+)\s+fbytes=(\d+)"
)
JVM_WALL_RE = re.compile(
    r"^\[sim\] JVM wall-clock:\s+(\d+)\s+ms,\s+gc:\s+(\d+)\s+collections,\s+(\d+)\s+freed,"
    r"\s+(\d+)\s+us,\s+lazy-load:\s+(\d+)/(\d+)\s+classes parsed"
)
HEAP_PEAK_RE = re.compile(
    r"^\[sim\] heap:\s+peak\s+(\d+)\s+KB\s+/\s+(\d+)\s+KB limit\s+\((\d+)\s+KB current\)"
)
HEAP_PHASE_RE = re.compile(
    r"^\[sim\] heap phase:\s+(\S+)\s+delta\s+([+-]?\d+)\s+B\s+\(cur\s+(\d+)\s+B,\s+peak\s+(\d+)\s+B\)"
)
OOM_RE = re.compile(
    r"^\[sim\] OOM:\s+tried\s+(\d+)\s+B.*?largest block\s+(\d+)\s+B.*?min-ever-free\s+(\d+)\s+B"
)
APK_RE = re.compile(r"^\[sim\] APK loaded from\s+\S+\s+\((\d+)\s+bytes")
MEMMON_RE = re.compile(r"^\[memmon\]\s+w=\d+\s+(.*)$")
FLASH_TIME_RE = re.compile(r"^\s*Finished in\s+([\d.]+)s")
PASSED_RE = re.compile(r"===\s+PASSED\s+===")


def normalize(line):
    """Strip sim/defmt framing. Returns (tag, body) or (None, raw)."""
    line = line.rstrip("\n")
    m = DEFMT_RE.match(line)
    if m:
        body = m.group(1)
        if ": " in body:
            tag, rest = body.split(": ", 1)
            return tag, rest
        return None, body
    m = SIMTAG_RE.match(line)
    if m:
        return m.group(1), m.group(2)
    return None, line


def parse_log(path, env):
    """Return {metric: value} for one app log."""
    out = {}
    oom_count = 0
    oom_min_free = None
    oom_min_lblk = None
    try:
        text = path.read_text(errors="replace")
    except OSError:
        return out

    for line in text.splitlines():
        # --- sim-only structured lines (matched on the raw line) ---
        m = JVM_WALL_RE.match(line)
        if m:
            out["jvm_wall_ms"] = int(m.group(1))
            out["gc_count"] = int(m.group(2))
            out["gc_freed"] = int(m.group(3))
            out["gc_us"] = int(m.group(4))
            out["classes_parsed"] = int(m.group(5))
            out["classes_total"] = int(m.group(6))
            continue
        m = HEAP_PEAK_RE.match(line)
        if m:
            out["heap_peak_kb"] = int(m.group(1))
            out["heap_limit_kb"] = int(m.group(2))
            out["heap_final_kb"] = int(m.group(3))
            continue
        m = HEAP_PHASE_RE.match(line)
        if m:
            phase = m.group(1).replace("-", "_")
            out[f"phase_{phase}_cur_b"] = int(m.group(3))
            out[f"phase_{phase}_peak_b"] = int(m.group(4))
            continue
        m = OOM_RE.match(line)
        if m:
            oom_count += 1
            lblk, minfree = int(m.group(2)), int(m.group(3))
            oom_min_lblk = lblk if oom_min_lblk is None else min(oom_min_lblk, lblk)
            oom_min_free = minfree if oom_min_free is None else min(oom_min_free, minfree)
            continue
        m = APK_RE.match(line)
        if m:
            out["apk_bytes"] = int(m.group(1))
            continue
        m = FLASH_TIME_RE.match(line)
        if m:
            out["flash_seconds_x100"] = int(round(float(m.group(1)) * 100))
            continue
        m = PARITY_RE.search(line)
        if m:
            for i, k in enumerate(("insns", "allocs", "gcs", "bands", "fbytes"), start=1):
                out[k] = int(m.group(i))
            continue
        m = MEMMON_RE.match(line)
        if m:
            # Last window wins; these are steady-state readings.
            for k, v in KV_RE.findall(m.group(1)):
                out[f"memmon_{k}"] = int(v)
            continue

        # --- app output, either framing ---
        tag, body = normalize(line)
        if tag is None:
            continue
        if PASSED_RE.search(body):
            out["passed"] = 1
            continue
        m = TOTAL_RE.match(body)
        if m:
            out["wall_ms" if m.group(2) == "ms" else "wall_us"] = int(m.group(1))
            continue
        m = SCORE_RE.match(body)
        if m:
            out["score"] = int(m.group(1))
            continue
        m = SUBSCORE_RE.match(body)
        if m:
            for k, v in KV_RE.findall(m.group(1)):
                out[f"subscore_{k}"] = int(v)
            continue
        m = PERFTEST_RE.match(body)
        if m:
            n = m.group(1)
            out[f"test_{n}_ms"] = int(m.group(2))
            out[f"test_{n}_gc_ms"] = int(m.group(3))
            out[f"test_{n}_gc_cyc"] = int(m.group(4))
            out[f"test_{n}_peak_kb"] = int(m.group(5))
            out[f"test_{n}_score"] = int(m.group(6))
            continue
        m = SUBTEST_RE.match(body)
        if m:
            unit = "ms" if m.group(3) == "ms" else "us"
            out[f"test_{m.group(1)}_{unit}"] = int(m.group(2))
            continue

    if oom_count or env == "sim":
        out["oom_count"] = oom_count
    if oom_min_free is not None:
        out["oom_min_ever_free_b"] = oom_min_free
        out["oom_min_largest_block_b"] = oom_min_lblk
    return out


def split_app_mode(stem):
    """Split '<app>.<mode>' out of a log stem. Apps may contain dots
    (e.g. 'blinky.pdb-install-stress.no-shrink')."""
    for m in ("no-shrink", "shrink"):
        suffix = "." + m
        if stem.endswith(suffix):
            return stem[: -len(suffix)], m
    return stem, "none"


def rows_for_run(run_dir, env):
    """Yield CSV rows for one <RUN_ID> directory."""
    m = RUN_ID_RE.match(run_dir.name)
    if not m:
        print(f"  skip (unparseable RUN_ID): {run_dir.name}", file=sys.stderr)
        return
    date, hh, mm, ss, commit = m.groups()
    utc = f"{date}T{hh}:{mm}:{ss}Z"

    for log in sorted(run_dir.glob("*.log")):
        if log.name.endswith(".build.log"):
            continue
        app, mode = split_app_mode(log.stem)
        metrics = parse_log(log, env)
        if not metrics:
            continue
        board = BOARD_BY_APP.get(app, DEFAULT_BOARD)
        split = "train" if app in TRAIN_APPS else "holdout"
        for metric, value in sorted(metrics.items()):
            yield [utc, commit, env, board, app, mode, split, metric, value]


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--env", choices=["sim", "hil", "both"], default="both")
    ap.add_argument("--run-dir", help="parse exactly one run directory")
    ap.add_argument("--out", default=str(CSV_PATH))
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args()

    rows = []
    if args.run_dir:
        d = Path(args.run_dir).resolve()
        env = "hil" if "/hil/" in str(d) else "sim"
        rows.extend(rows_for_run(d, env))
    else:
        envs = ["sim", "hil"] if args.env == "both" else [args.env]
        for env in envs:
            base = REPO / "build" / env / "logs"
            if not base.is_dir():
                print(f"  no {base}", file=sys.stderr)
                continue
            dirs = sorted(p for p in base.iterdir() if p.is_dir())
            if not args.quiet:
                print(f"==> {env}: {len(dirs)} run directories", file=sys.stderr)
            for d in dirs:
                rows.extend(rows_for_run(d, env))

    if args.dry_run:
        w = csv.writer(sys.stdout)
        w.writerow(HEADER)
        w.writerows(rows)
        print(f"\n{len(rows)} rows (dry run, nothing written)", file=sys.stderr)
        return 0

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)

    # Idempotent: a row is keyed by everything but its value, so re-running the
    # backfill over the same logs replaces rather than duplicates.
    existing = {}
    order = []
    if out.exists():
        with out.open(newline="") as f:
            r = csv.reader(f)
            head = next(r, None)
            if head and head != HEADER:
                print(f"  migrating {len(head)}-column CSV to schema v2", file=sys.stderr)
            for rec in r:
                if len(rec) == len(HEADER):
                    key = tuple(rec[:-1])
                elif len(rec) == 6:
                    # legacy: utc,commit,env,app,metric,value
                    u, c, e, a, k, v = rec
                    rec = [u, c, e, BOARD_BY_APP.get(a, DEFAULT_BOARD), a, "no-shrink",
                           "train" if a in TRAIN_APPS else "holdout", k, v]
                    key = tuple(rec[:-1])
                else:
                    continue
                if key not in existing:
                    order.append(key)
                existing[key] = rec

    added = 0
    for rec in rows:
        key = tuple(str(x) for x in rec[:-1])
        rec = [str(x) for x in rec]
        if key not in existing:
            order.append(key)
            added += 1
        existing[key] = rec

    with out.open("w", newline="") as f:
        w = csv.writer(f)
        w.writerow(HEADER)
        for key in order:
            w.writerow(existing[key])

    if not args.quiet:
        print(f"==> {out.relative_to(REPO)}: {len(order)} rows "
              f"({added} new this pass)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
