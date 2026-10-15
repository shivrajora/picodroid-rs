#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Fold a `cargo llvm-cov report --json --summary-only` export into per-crate numbers.

A crate is the path prefix that owns the file: crates/<x>, platforms/rp,
tools/<x>. Next to the line / region / function percentages, each crate gets
the .rs files under its src/ that the report does not name at all: never
built by the measured run, or built but holding no executable code (a list of
`mod`s, type definitions, consts). On the host the big ones are device-only
code (platforms/rp/src/hal/rp/**, hal/freertos_tcp), which no host run can
reach by design, and the opt-in diagnostics (mem_diag, sched_diag).

Usage:
  coverage-summary.py SUMMARY_JSON --markdown OUT.md [--title T] [--uncompiled OUT.txt]
  coverage-summary.py SUMMARY_JSON --history CSV --layer unit --commit SHA
  coverage-summary.py SUMMARY_JSON --check BASELINE [--tolerance PP]
  coverage-summary.py SUMMARY_JSON --write-baseline BASELINE

--check fails (exit 1) when a crate's line coverage is more than --tolerance
percentage points below its baseline, and also when it is that far above it
(the baseline has not caught up: re-run with --write-baseline, the way
check-source-guards.sh --unsafe-accept works), or when a crate is new or gone.
"""

import argparse
import csv
import datetime
import json
import os
import re
import sys

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))

# The same filter scripts/coverage.sh hands llvm-cov, applied to the
# on-disk walk so the two denominators agree.
IGNORE = re.compile(r"(/tests?/|/tests?\.rs$|_tests?\.rs$|/third_party/|/benches/|/examples/)")

ROOTS = ("crates", "tools")


def crate_of(rel):
    """crates/jvm/src/x.rs -> crates/jvm; platforms/rp/src/x.rs -> platforms/rp."""
    parts = rel.split("/")
    if len(parts) >= 2 and parts[0] in ROOTS + ("platforms",):
        return "/".join(parts[:2])
    return None


def workspace_crates():
    out = []
    for root in ROOTS + ("platforms",):
        base = os.path.join(REPO_ROOT, root)
        if not os.path.isdir(base):
            continue
        for name in sorted(os.listdir(base)):
            if os.path.isfile(os.path.join(base, name, "Cargo.toml")):
                out.append(f"{root}/{name}")
    return out


def count_lines(path):
    with open(path, "rb") as f:
        return sum(1 for _ in f)


def source_files(crate):
    """Every non-test .rs file under the crate's src/ (repo-relative)."""
    src = os.path.join(REPO_ROOT, crate, "src")
    for dirpath, _dirs, files in os.walk(src):
        for name in files:
            if name.endswith(".rs"):
                rel = os.path.relpath(os.path.join(dirpath, name), REPO_ROOT)
                if not IGNORE.search("/" + rel):
                    yield rel


def load(path):
    with open(path) as f:
        data = json.load(f)
    rows = {}
    seen = set()
    for entry in data["data"]:
        for fe in entry["files"]:
            name = os.path.abspath(fe["filename"])
            if not name.startswith(REPO_ROOT + os.sep):
                continue
            rel = os.path.relpath(name, REPO_ROOT)
            crate = crate_of(rel)
            if crate is None:
                continue
            seen.add(rel)
            r = rows.setdefault(crate, {k: [0, 0] for k in ("lines", "regions", "functions")})
            for k in r:
                r[k][0] += fe["summary"][k]["covered"]
                r[k][1] += fe["summary"][k]["count"]
    uncompiled = {}
    for crate in workspace_crates():
        rows.setdefault(crate, {k: [0, 0] for k in ("lines", "regions", "functions")})
        missing = [f for f in source_files(crate) if f not in seen]
        uncompiled[crate] = [(f, count_lines(os.path.join(REPO_ROOT, f))) for f in sorted(missing)]
    return rows, uncompiled


def pct(covered, count):
    return 100.0 * covered / count if count else 0.0


def write_markdown(rows, uncompiled, out, title):
    tot = {k: [0, 0] for k in ("lines", "regions", "functions")}
    lines = [
        f"### {title}",
        "",
        "| Crate | Lines | Line % | Region % | Function % | Not in report (files / lines) |",
        "|---|--:|--:|--:|--:|--:|",
    ]
    for crate in sorted(rows):
        r = rows[crate]
        for k in tot:
            tot[k][0] += r[k][0]
            tot[k][1] += r[k][1]
        unc = uncompiled.get(crate, [])
        unc_s = f"{len(unc)} / {sum(n for _, n in unc):,}" if unc else "—"
        if r["lines"][1] == 0:
            lines.append(f"| {crate} | 0 | — | — | — | {unc_s} |")
            continue
        lines.append(
            f"| {crate} | {r['lines'][1]:,} | {pct(*r['lines']):.1f} | "
            f"{pct(*r['regions']):.1f} | {pct(*r['functions']):.1f} | {unc_s} |"
        )
    unc_all = [x for v in uncompiled.values() for x in v]
    lines.append(
        f"| **total** | {tot['lines'][1]:,} | **{pct(*tot['lines']):.1f}** | "
        f"{pct(*tot['regions']):.1f} | {pct(*tot['functions']):.1f} | "
        f"{len(unc_all)} / {sum(n for _, n in unc_all):,} |"
    )
    lines += [
        "",
        "Lines are instrumented lines (llvm-cov), test files excluded. "
        "*Not in report* counts the source files under each crate's src/ the "
        "report never names: not built by this run (device-only code, opt-in "
        "diagnostics) or holding no executable code; uncompiled.txt lists "
        "them (docs/coverage.md).",
        "",
    ]
    with open(out, "w") as f:
        f.write("\n".join(lines))


def write_uncompiled(uncompiled, out):
    with open(out, "w") as f:
        for crate in sorted(uncompiled):
            for name, n in uncompiled[crate]:
                f.write(f"{n:6d} {name}\n")


def append_history(rows, path, layer, commit):
    utc = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    new = not os.path.exists(path)
    with open(path, "a", newline="") as f:
        w = csv.writer(f)
        if new:
            w.writerow(["utc", "commit", "layer", "crate", "lines", "covered", "line_pct"])
        for crate in sorted(rows):
            covered, count = rows[crate]["lines"]
            if count:
                w.writerow([utc, commit, layer, crate, count, covered, f"{pct(covered, count):.2f}"])


def read_baseline(path):
    base = {}
    with open(path) as f:
        for line in f:
            line = line.split("#", 1)[0].strip()
            if line:
                crate, value = line.split()
                base[crate] = float(value)
    return base


def write_baseline(rows, path):
    with open(path, "w") as f:
        f.write("# Per-crate line coverage of the unit-test layer; scripts/coverage.sh --check.\n")
        f.write("# Rewrite with scripts/coverage.sh --accept (commit trailer `coverage:`).\n")
        f.write("# crate line_pct\n")
        for crate in sorted(rows):
            if rows[crate]["lines"][1]:
                f.write(f"{crate} {pct(*rows[crate]['lines']):.2f}\n")
    print(f"wrote {os.path.relpath(path, REPO_ROOT)}")


def check(rows, path, tolerance):
    base = read_baseline(path)
    now = {c: pct(*r["lines"]) for c, r in rows.items() if r["lines"][1]}
    bad = 0
    for crate in sorted(set(base) | set(now)):
        if crate not in now:
            print(f"gone  {crate}: in the baseline, no instrumented lines now")
            bad += 1
        elif crate not in base:
            print(f"new   {crate}: {now[crate]:.2f}% not in the baseline")
            bad += 1
        elif now[crate] < base[crate] - tolerance:
            print(f"DROP  {crate}: {now[crate]:.2f}% < baseline {base[crate]:.2f}% - {tolerance}")
            bad += 1
        elif now[crate] > base[crate] + tolerance:
            print(f"RISE  {crate}: {now[crate]:.2f}% > baseline {base[crate]:.2f}% + {tolerance}; accept it")
            bad += 1
    if bad:
        print("coverage ratchet: FAIL (scripts/coverage.sh --accept to re-baseline)")
        return 1
    print(f"coverage ratchet: ok ({len(now)} crates within {tolerance} pp)")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("summary_json")
    ap.add_argument("--markdown")
    ap.add_argument("--title", default="Coverage")
    ap.add_argument("--uncompiled")
    ap.add_argument("--history")
    ap.add_argument("--layer", default="unit")
    ap.add_argument("--commit", default="")
    ap.add_argument("--check")
    ap.add_argument("--tolerance", type=float, default=0.5)
    ap.add_argument("--write-baseline")
    args = ap.parse_args()

    rows, uncompiled = load(args.summary_json)
    if args.markdown:
        write_markdown(rows, uncompiled, args.markdown, args.title)
    if args.uncompiled:
        write_uncompiled(uncompiled, args.uncompiled)
    if args.history:
        append_history(rows, args.history, args.layer, args.commit)
    if args.write_baseline:
        write_baseline(rows, args.write_baseline)
    if args.check:
        return check(rows, args.check, args.tolerance)
    return 0


if __name__ == "__main__":
    sys.exit(main())
