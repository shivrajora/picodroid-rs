#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Trend, compare and gate the metrics in bench/parity/history.csv.

The CSV is append-only history produced by scripts/bench-backfill.py (nightly
logs) and scripts/parity-bench.sh (campaign runs). This is the read side.

  --trend    time series for one metric, with per-commit spread
  --compare  A..B diff table across every metric both revisions share
  --check    noise-calibrated regression gate (see THRESHOLDS)
  --fit      regress device wall time on cheaper signals; report R^2
  --holdout  held-out app summary (overfitting guard)

Noise floors, measured from the backfilled corpus (campaign plan S0):
  device wall_ms, same flashed image ....... ~32 ppm
  device wall_ms, across rebuilds .......... +/-5% (XIP/icache layout)
  sim wall_ms, same binary ................. 4-5% peak-to-peak
Hence: deterministic metrics gate at 0%, device wall at 2%, sim wall is a
hang tripwire only and never an accept/reject signal.
"""

import argparse
import collections
import csv
import statistics
import subprocess
import sys
import pathlib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CSV_PATH = REPO / "bench" / "parity" / "history.csv"

# Metrics that are exactly reproducible for a given binary. Any movement is a
# real change and must be named in the commit message.
DETERMINISTIC = {
    "insns", "allocs", "gcs", "bands", "fbytes",
    "flash_bytes", "ram_bytes", "text", "data", "bss",
    "classes_parsed", "classes_total", "apk_bytes",
    "oom_count", "gc_count",
}

# Thresholds depend on whether the two revisions were measured from the SAME
# flashed image. Across rebuilds the XIP/icache layout re-rolls, and the
# 2026-08-24 forensics measured that band at ~5% on aggregate wall_ms and up to
# +/-40% on individual microbenchmarks -- so per-test device timings are
# attribution only and never gate. Same-image batches are ~32 ppm and gate hard.
THRESHOLDS = {                       # class -> (warn, fail) as fractions
    "deterministic": (0.0, 0.0),
    "hil_wall_rebuild": (0.02, 0.05),
    "hil_wall_same_image": (0.007, 0.02),
    "sim_wall": (None, 0.25),
    "attribution": (None, None),     # reported, never failed
}


def load(path=CSV_PATH):
    if not path.exists():
        sys.exit(f"no {path} -- run ./scripts/bench-backfill.py first")
    with path.open(newline="") as f:
        return list(csv.DictReader(f))


def commit_order(commits):
    """Order commits by git history; fall back to input order for unknowns."""
    try:
        out = subprocess.run(["git", "-C", str(REPO), "rev-list", "--reverse", "HEAD"],
                             capture_output=True, text=True, check=True).stdout.split()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return {c: i for i, c in enumerate(commits)}
    rank = {}
    for i, full in enumerate(out):
        rank[full[:7]] = i
    return {c: rank.get(c[:7], -1) for c in commits}


def series(rows, env=None, app=None, mode=None, metric=None, board=None):
    """(commit, [values]) grouped by commit, ordered by time."""
    g = collections.OrderedDict()
    for r in rows:
        if env and r["env"] != env:
            continue
        if app and r["app"] != app:
            continue
        if mode and r["mode"] != mode:
            continue
        if metric and r["metric"] != metric:
            continue
        if board and r["board"] != board:
            continue
        g.setdefault((r["utc"][:10], r["commit"]), []).append(float(r["value"]))
    return g


def cmd_trend(rows, args):
    g = series(rows, env=args.env, app=args.app, mode=args.mode,
               metric=args.metric, board=args.board)
    if not g:
        sys.exit("no rows match")
    print(f"{args.metric}  env={args.env} app={args.app} mode={args.mode}\n")
    print(f"{'date':<12} {'commit':<9} {'n':>2} {'median':>12} {'p2p%':>7}  {'delta%':>7}")
    prev = None
    for (date, commit), vals in g.items():
        med = statistics.median(vals)
        p2p = (max(vals) - min(vals)) / med * 100 if med and len(vals) > 1 else 0.0
        delta = (med - prev) / prev * 100 if prev else 0.0
        flag = ""
        if prev and abs(delta) >= 2.0:
            flag = "  <== " + ("REGRESSION" if delta > 0 else "improvement")
        print(f"{date:<12} {commit:<9} {len(vals):>2} {med:>12,.0f} {p2p:>6.2f}% "
              f"{delta:>+6.2f}%{flag}")
        prev = med


def _pick(rows, commit, **f):
    """All metrics for one commit as {(env,board,app,mode,metric): median}."""
    out = collections.defaultdict(list)
    for r in rows:
        if not r["commit"].startswith(commit[:7]):
            continue
        if f.get("env") and r["env"] != f["env"]:
            continue
        out[(r["env"], r["board"], r["app"], r["mode"], r["metric"])].append(float(r["value"]))
    return {k: statistics.median(v) for k, v in out.items()}, \
           {k: len(v) for k, v in out.items()}


def classify(env, metric, same_image=False):
    if metric in DETERMINISTIC:
        return "deterministic"
    if metric.startswith("test_") or metric.startswith("subscore_"):
        # Per-workload device timings swing up to +/-40% on layout alone. They
        # localise a change; they never adjudicate one.
        return "attribution" if env == "hil" else "sim_wall"
    if metric in ("wall_ms", "wall_us", "score"):
        if env != "hil":
            return "sim_wall"
        return "hil_wall_same_image" if same_image else "hil_wall_rebuild"
    return None


def cmd_compare(rows, args):
    base, head = (args.compare.split("..") + [""])[:2]
    if not head:
        sys.exit("--compare needs A..B")
    a, _ = _pick(rows, base, env=args.env)
    b, nb = _pick(rows, head, env=args.env)
    shared = sorted(set(a) & set(b))
    if not shared:
        sys.exit(f"no metric is present for both {base} and {head}"
                 f" (have {len(a)} / {len(b)} rows)")

    print(f"{base[:7]} -> {head[:7]}   ({len(shared)} shared metrics)\n")
    print(f"{'env':<4} {'app':<20} {'mode':<10} {'metric':<26} "
          f"{'base':>12} {'head':>12} {'delta%':>8}  verdict")
    worst = 0.0
    fails = []
    for k in shared:
        env, board, app, mode, metric = k
        va, vb = a[k], b[k]
        if va == 0:
            continue
        d = (vb - va) / va
        cls = classify(env, metric, args.same_image)
        verdict = ""
        if cls:
            warn, fail = THRESHOLDS[cls]
            if fail is not None and abs(d) > fail:
                verdict = "FAIL"
                fails.append((k, d))
            elif warn is not None and abs(d) > warn:
                verdict = "warn"
        if not args.all and abs(d) < 0.005 and not verdict:
            continue
        worst = max(worst, abs(d))
        print(f"{env:<4} {app:<20} {mode:<10} {metric:<26} "
              f"{va:>12,.0f} {vb:>12,.0f} {d*100:>+7.2f}%  {verdict}")
    print(f"\nlargest movement: {worst*100:.2f}%   fails: {len(fails)}")
    return 1 if fails else 0


def cmd_fit(rows, args):
    """Regress device wall_ms on sim-side signals across the history.

    The nightlies never enabled parity-metrics, so insns/allocs are absent from
    the backfill; the available regressors are the sim's own wall clock, GC
    counters and heap peak. Report R^2 so the campaign knows how much to trust
    a sim-side proxy before spending device time.
    """
    app, mode = args.app, args.mode
    hil = series(rows, env="hil", app=app, mode=mode, metric="wall_ms")
    if not hil:
        sys.exit(f"no hil wall_ms for {app}/{mode}")
    hil_by_commit = {c: statistics.median(v) for (_, c), v in hil.items()}

    regressors = ["wall_ms", "gc_count", "gc_freed", "heap_peak_kb", "classes_parsed"]
    print(f"device wall_ms ~ sim signals   (app={app} mode={mode})\n")
    print(f"{'regressor':<18} {'n':>4} {'pearson r':>10} {'R^2':>8}")
    for reg in regressors:
        sim = series(rows, env="sim", app=app, mode=mode, metric=reg)
        sim_by_commit = {c: statistics.median(v) for (_, c), v in sim.items()}
        pairs = [(sim_by_commit[c], hil_by_commit[c])
                 for c in hil_by_commit if c in sim_by_commit]
        if len(pairs) < 4:
            print(f"{reg:<18} {len(pairs):>4}   (need 4+ paired commits)")
            continue
        xs = [p[0] for p in pairs]
        ys = [p[1] for p in pairs]
        if len(set(xs)) < 2:
            print(f"{reg:<18} {len(pairs):>4}   (constant)")
            continue
        r = statistics.correlation(xs, ys)
        print(f"{reg:<18} {len(pairs):>4} {r:>+10.3f} {r*r:>8.3f}")
    print("\nCAVEAT -- read before trusting any R^2 above. This corpus cannot identify\n"
          "a causal proxy: the paired-commit count is small, and over the backfilled\n"
          "window the sim got SLOWER while the device got FASTER (different commits\n"
          "hit each environment differently), so every regressor is anti-correlated\n"
          "by coincidence of what landed. A negative r here means confounding, not a\n"
          "predictive relationship. Collect paired runs with parity-metrics enabled\n"
          "(scripts/parity-bench.sh --both) before drawing a conclusion.")


def cmd_holdout(rows, args):
    base, head = (args.holdout.split("..") + [""])[:2]
    if not head:
        sys.exit("--holdout needs A..B")
    a, _ = _pick(rows, base)
    b, _ = _pick(rows, head)
    print(f"held-out apps: {base[:7]} -> {head[:7]}\n")
    print(f"{'env':<4} {'app':<20} {'mode':<10} {'metric':<16} {'delta%':>8}  verdict")
    split_of = {r["app"]: r["split"] for r in rows}
    vetoed = 0
    for k in sorted(set(a) & set(b)):
        env, board, app, mode, metric = k
        if split_of.get(app, "holdout") != "holdout":
            continue
        if metric not in ("wall_ms", "wall_us", "score", "oom_count", "passed"):
            continue
        va, vb = a[k], b[k]
        if va == 0:
            continue
        d = (vb - va) / va
        # A held-out app vetoes on getting WORSE. An improvement there is fine
        # (and common -- see the layout band in docs/perf-campaign-2026-08.md
        # S1.3, which moves held-out timings in both directions on a rebuild).
        if metric in ("oom_count",):
            verdict = "VETO" if vb > va else ""
        elif metric == "passed":
            verdict = "VETO" if vb < va else ""
        else:
            # One source of truth for every threshold: the same table --compare
            # gates on. Sim wall therefore vetoes only at the 25% hang tripwire,
            # never on its 4-5% run-to-run noise.
            cls = classify(env, metric, args.same_image)
            _, fail = THRESHOLDS.get(cls, (None, None))
            verdict = "VETO" if fail is not None and d > fail else ""
        if verdict:
            vetoed += 1
        elif abs(d) < 0.01:
            continue
        print(f"{env:<4} {app:<20} {mode:<10} {metric:<16} {d*100:>+7.2f}%  {verdict}")
    print(f"\n{vetoed} veto(es)")
    return 1 if vetoed else 0


RATCHET = REPO / "bench" / "parity" / "ratchet.toml"

# The RP2040 program region is 917,248 B and is enforced today only by the link
# failing. G1_HARD keeps a deliberate reserve below it so the gate trips in
# review rather than at 3am in someone's build.
G1_HARD = 908_000
G2_HARD = 532_480          # rp2350 data+bss, the chip's whole SRAM


def read_ratchet():
    """Minimal TOML reader -- Python 3.11 has tomllib, older does not, and the
    file is a flat two-level table by construction."""
    if not RATCHET.exists():
        return {}
    try:
        import tomllib
        with RATCHET.open("rb") as f:
            return tomllib.load(f)
    except ImportError:
        pass
    out, section = {}, None
    for line in RATCHET.read_text().splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1]
            out[section] = {}
        elif "=" in line and section:
            k, v = (x.strip() for x in line.split("=", 1))
            out[section][k] = int(v) if v.lstrip("-").isdigit() else v.strip('"')
    return out


def sizes_from_logs(run_dir):
    """Read a size-lane run directory directly, without the history CSV.

    The ratchet compares what the tree builds *right now* against the committed
    baseline. Routing that through the append-only history meant every commit
    wrote rows that were, by construction, the same numbers as the last commit's
    -- and left the tree dirty for a check that needed to persist nothing.
    """
    import importlib.util

    src = pathlib.Path(__file__).resolve().parent / "bench-backfill.py"
    spec = importlib.util.spec_from_file_location("bench_backfill", src)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)

    # Parse the logs directly rather than going through rows_for_run: that
    # derives utc and commit from the directory name, and a scratch directory
    # (pre-commit hands us an mktemp one) has no identity to derive. The
    # ratchet only needs board, metric and value.
    out = {}
    for log in sorted(pathlib.Path(run_dir).glob("*.log")):
        hdr = mod.read_header(log)
        board = hdr.get("board")
        if not board:
            continue
        for metric, value in mod.parse_log(log, "size").items():
            out[(board, metric)] = float(value)
    return out


def current_sizes(rows):
    """Newest size-lane reading per (board, metric)."""
    best = {}
    for r in rows:
        if r["env"] != "size":
            continue
        key = (r["board"], r["metric"])
        if key not in best or r["utc"] > best[key][0]:
            best[key] = (r["utc"], float(r["value"]))
    return {k: v for k, (_, v) in best.items()}


def cmd_ratchet(rows, args):
    """Gate flash/RAM against the committed baseline.

    Deterministic metrics ratchet at 0%: any growth must be a deliberate,
    named act. Shrinking is always allowed and is what --accept records.
    """
    base = read_ratchet()
    cur = sizes_from_logs(args.sizes_from) if args.sizes_from else current_sizes(rows)
    if not cur:
        sys.exit("no size measurements -- run "
                 "./scripts/parity-bench.sh --size-only --boards <b1,b2>")

    boards = sorted({b for b, _ in cur})
    print(f"{'board':<22} {'metric':<14} {'baseline':>10} {'current':>10} "
          f"{'delta':>9}  verdict")
    failed = accepted = 0
    for board in boards:
        for metric in ("flash_bytes", "ram_bytes"):
            c = cur.get((board, metric))
            if c is None:
                continue
            b = base.get(board, {}).get(metric)
            hard = G1_HARD if metric == "flash_bytes" else G2_HARD
            verdict = ""
            if metric == "flash_bytes" and "rp2040" in board and c > hard:
                verdict = "FAIL (hard ceiling)"
            elif b is None:
                verdict = "new"
            elif c > b:
                verdict = "FAIL (ratchet)"
            elif c < b:
                verdict = "improved"
            if verdict.startswith("FAIL"):
                failed += 1
            if verdict == "improved":
                accepted += 1
            d = "-" if b is None else f"{c - b:+,.0f}"
            bs = "-" if b is None else f"{b:,.0f}"
            print(f"{board:<22} {metric:<14} {bs:>10} {c:>10,.0f} {d:>9}  {verdict}")

    if args.accept:
        lines = ["# Committed size baseline for the perf campaign.",
                 "#",
                 "# Deterministic metrics ratchet at 0%: bench-report.py --ratchet fails",
                 "# on any growth. Advancing this file is the explicit act of consenting",
                 "# to spend budget, so do it in the same commit that spends it and say",
                 "# why in the message (size: trailer).",
                 "#",
                 "# Regenerate: ./scripts/parity-bench.sh --size-only --boards <list>",
                 "#             ./scripts/bench-report.py --ratchet --accept",
                 ""]
        for board in boards:
            lines.append(f"[{board}]")
            for metric in ("flash_bytes", "ram_bytes", "text", "bss"):
                c = cur.get((board, metric))
                if c is not None:
                    lines.append(f"{metric} = {c:.0f}")
            lines.append("")
        RATCHET.write_text("\n".join(lines))
        print(f"\nbaseline written to {RATCHET.relative_to(REPO)}")
        return 0

    print(f"\n{failed} failure(s), {accepted} improvement(s) not yet accepted")
    if accepted and not failed:
        print("run with --accept to lock the improvement in")
    return 1 if failed else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--csv", default=str(CSV_PATH))
    ap.add_argument("--trend", metavar="METRIC")
    ap.add_argument("--compare", metavar="A..B")
    ap.add_argument("--fit", action="store_true")
    ap.add_argument("--holdout", metavar="A..B")
    ap.add_argument("--ratchet", action="store_true",
                    help="gate flash/RAM against bench/parity/ratchet.toml")
    ap.add_argument("--sizes-from", metavar="RUN_DIR",
                    help="with --ratchet: read sizes from a size-lane run "
                         "directory instead of the history CSV, so the check "
                         "persists nothing")
    ap.add_argument("--accept", action="store_true",
                    help="with --ratchet: record current sizes as the baseline")
    ap.add_argument("--env", default=None, choices=[None, "sim", "hil"])
    ap.add_argument("--app", default="benchmark")
    ap.add_argument("--mode", default="no-shrink")
    ap.add_argument("--board", default=None)
    ap.add_argument("--all", action="store_true", help="show unchanged metrics too")
    ap.add_argument("--same-image", action="store_true",
                    help="both revisions measured from one flashed image "
                         "(enables the tight 2%% device threshold)")
    args = ap.parse_args()

    rows = load(Path(args.csv))

    if args.trend:
        args.metric = args.trend
        args.env = args.env or "hil"
        return cmd_trend(rows, args) or 0
    if args.compare:
        return cmd_compare(rows, args) or 0
    if args.fit:
        return cmd_fit(rows, args) or 0
    if args.holdout:
        return cmd_holdout(rows, args) or 0
    if args.ratchet:
        return cmd_ratchet(rows, args) or 0
    ap.print_help()
    return 0


if __name__ == "__main__":
    sys.exit(main())
