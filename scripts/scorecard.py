#!/usr/bin/env python3
"""docs/scorecard.md from the repo's measured sources. Never edit that file.

    ./scripts/scorecard.py --write   # regenerate docs/scorecard.md
    ./scripts/scorecard.py --check   # fail when the committed file is stale,
                                     # a parity row is inconsistent, or the
                                     # parity floor is broken
    ./scripts/scorecard.py           # print the page to stdout

Sources, all of them things the repo already measures or gates:
    sdk/android-parity.tsv            the android.* surface and its status
    sdk/api-contract.tsv              the java/** surface pico-jvm serves
    sdk/java/picodroid/**             the SDK classes that exist
    bench/parity/ratchet.toml         flash and RAM per gated board
    bench/parity/history.csv          the size trend behind it
    scripts/coverage-baseline.conf    line coverage per crate
    scripts/unsafe-baseline.conf      unsafe Rust per crate
    scripts/panic-baseline.conf       panic sites per firmware crate
    crates/*/src/lib.rs               which crates deny the panic lints
    scripts/hil-tests.conf            the test matrix, rows per category
    fuzz/fuzz_targets, fuzz/regress   fuzz targets and replayed findings
    build/{sim,hil,fuzz}/results      last night's verdicts (this machine
                                      only: that section is outside --check)

The nightly section sits between `<!-- nightly:begin -->` and
`<!-- nightly:end -->`; --check compares everything but it, so a fresh
checkout (CI) and the bench machine agree on the rest.
"""

import glob
import os
import pathlib
import re
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
OUT = REPO / "docs" / "scorecard.md"
PARITY = REPO / "sdk" / "android-parity.tsv"
CONTRACT = REPO / "sdk" / "api-contract.tsv"
RATCHET = REPO / "bench" / "parity" / "ratchet.toml"
HISTORY = REPO / "bench" / "parity" / "history.csv"
COVERAGE = REPO / "scripts" / "coverage-baseline.conf"
UNSAFE = REPO / "scripts" / "unsafe-baseline.conf"
PANICS = REPO / "scripts" / "panic-baseline.conf"
HIL_CONF = REPO / "scripts" / "hil-tests.conf"
NIGHTLY_BEGIN = "<!-- nightly:begin -->"
NIGHTLY_END = "<!-- nightly:end -->"
STATUSES = ("live", "partial", "missing", "out-of-scope", "addition")


# ----------------------------------------------------------------- readers

def read_conf_table(path, ncols):
    """`# comment` lines skipped; rows of `name v1 v2 ...`."""
    rows = []
    for line in path.read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) == ncols:
            rows.append(parts)
    return rows


def read_parity():
    rows, floor, problems = [], None, []
    for n, line in enumerate(PARITY.read_text().splitlines(), 1):
        if line.startswith("# parity_floor:"):
            floor = float(line.split(":", 1)[1])
            continue
        if not line.strip() or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) == 3:
            parts.append("")  # the note is optional
        if len(parts) != 4:
            problems.append("android-parity.tsv:%d: expected 3 or 4 tab-separated columns" % n)
            continue
        android, pico, status, note = parts
        if status not in STATUSES:
            problems.append("android-parity.tsv:%d: unknown status %r" % (n, status))
            continue
        src = REPO / "sdk" / "java" / (pico + ".java") if pico != "-" else None
        if status in ("live", "partial") and src is not None and not src.exists():
            problems.append("android-parity.tsv:%d: %s is %s but sdk/java/%s.java does not exist"
                            % (n, android, status, pico))
        if status == "addition" and (src is None or not src.exists()):
            problems.append("android-parity.tsv:%d: %s is an addition but names no existing SDK source" % (n, android))
        if status in ("missing", "out-of-scope") and src is not None:
            problems.append("android-parity.tsv:%d: %s is %s but names a source file (%s)"
                            % (n, android, status, pico))
        rows.append(dict(android=android, pico=pico, status=status, note=note))
    if floor is None:
        problems.append("android-parity.tsv: no `# parity_floor:` line")
    return rows, floor, problems


def sdk_classes():
    root = REPO / "sdk" / "java"
    return sorted(str(p.relative_to(root))[:-5] for p in (root / "picodroid").rglob("*.java"))


def read_contract():
    """Classes and members per java/** package."""
    pkgs = {}
    for line in CONTRACT.read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        parts = line.split("\t")
        if parts[0].startswith("@"):
            continue  # @extends / @implements / @hint / @nameonly directives
        pkg = "/".join(parts[0].split("/")[:2])
        d = pkgs.setdefault(pkg, {"classes": set(), "members": 0})
        d["classes"].add(parts[0])
        if len(parts) == 3:
            d["members"] += 1
    return pkgs


def read_ratchet():
    import tomllib
    with RATCHET.open("rb") as f:
        return tomllib.load(f)


def ratchet_accepted():
    """When the size baseline was last advanced (its last commit), from git."""
    try:
        out = subprocess.run(["git", "log", "-1", "--format=%h %ad", "--date=short", "--",
                              str(RATCHET.relative_to(REPO))], cwd=REPO, check=True,
                             capture_output=True, text=True).stdout.split()
        return out[0], out[1]
    except Exception:
        return "?", "?"


def deny_crates():
    """Firmware crates whose lib.rs denies clippy::unwrap_used."""
    denies = set()
    for lib in glob.glob(str(REPO / "crates" / "*" / "src" / "lib.rs")) + \
            glob.glob(str(REPO / "platforms" / "*" / "src" / "lib.rs")) + \
            glob.glob(str(REPO / "platforms" / "*" / "src" / "main.rs")):
        text = pathlib.Path(lib).read_text()
        if "clippy::unwrap_used" in text and "#![deny(" in text:
            denies.add(str(pathlib.Path(lib).relative_to(REPO).parent.parent))
    return denies


def hil_rows():
    per = {}
    for line in HIL_CONF.read_text().splitlines():
        if not line.strip() or line.startswith("#") or "|" not in line:
            continue
        cat = line.split("|")[1].strip()
        per[cat] = per.get(cat, 0) + 1
    return per


def fuzz_counts():
    targets = len(glob.glob(str(REPO / "fuzz" / "fuzz_targets" / "*.rs")))
    regress = sum(1 for p in (REPO / "fuzz" / "regress").rglob("*") if p.is_file())
    findings = sum(1 for p in (REPO / "fuzz" / "regress").rglob("finding-*") if p.is_file())
    return targets, regress, findings


RESULT_NAME = re.compile(r"^(\d{4})-?(\d{2})-?(\d{2})[_-](\d{2})h?(\d{2})m?\d{2}s?[_-](\w+)\.txt$")


def latest_results(dir_path):
    """The newest result file by its date stamp; two naming schemes exist
    (2026-10-20_03h00m01s_<commit>.txt and the older 20260407-040002-<commit>.txt)."""
    parsed = []
    for f in glob.glob(str(dir_path / "*.txt")):
        m = RESULT_NAME.match(os.path.basename(f))
        if m:
            parsed.append((m.group(1) + m.group(2) + m.group(3) + m.group(4) + m.group(5), f, m))
    if not parsed:
        return None
    _, f, m = max(parsed)
    f = pathlib.Path(f)
    counts = {}
    for line in f.read_text().splitlines():
        verdict = line.split(" ", 1)[0]
        if verdict in ("PASS", "FAIL", "ERROR", "SKIP"):
            counts[verdict] = counts.get(verdict, 0) + 1
    return dict(date="%s-%s-%s" % (m.group(1), m.group(2), m.group(3)), time="%s:%s" % (m.group(4), m.group(5)),
                commit=m.group(6), counts=counts, file=str(f.relative_to(REPO)))


def head_commit():
    try:
        return subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=REPO, check=True,
                              capture_output=True, text=True).stdout.strip()
    except Exception:
        return "?"


# ---------------------------------------------------------------- rendering

def table(headers, rows, align=None):
    align = align or ["---"] * len(headers)
    out = ["| " + " | ".join(headers) + " |", "|" + "|".join(align) + "|"]
    for r in rows:
        out.append("| " + " | ".join(str(c) for c in r) + " |")
    return "\n".join(out)


def pct(n, d):
    return "%.1f %%" % (100.0 * n / d) if d else "n/a"


def kb(n):
    return "{:,} B".format(int(n))


def parity_summary(rows):
    per = {}
    for r in rows:
        pkg = r["android"].rsplit(".", 1)[0]
        d = per.setdefault(pkg, dict.fromkeys(STATUSES, 0))
        d[r["status"]] += 1
    return per


def parity_pct(d):
    counted = d["live"] + d["partial"] + d["missing"]
    return (100.0 * (d["live"] + d["partial"] / 2.0) / counted) if counted else None


def render(check_problems):
    parity_rows, floor, problems = read_parity()
    check_problems.extend(problems)
    per_pkg = parity_summary(parity_rows)
    total = dict.fromkeys(STATUSES, 0)
    for d in per_pkg.values():
        for k in STATUSES:
            total[k] += d[k]
    overall = parity_pct(total)
    if overall is not None and floor is not None and overall < floor:
        check_problems.append("Android parity %.1f %% is below the floor %.1f %% in sdk/android-parity.tsv"
                              % (overall, floor))
    listed = {r["pico"] for r in parity_rows}
    unlisted = [c for c in sdk_classes() if c not in listed]

    contract = read_contract()
    ratchet = read_ratchet()
    accepted = ratchet_accepted()
    coverage = read_conf_table(COVERAGE, 2)
    unsafe = read_conf_table(UNSAFE, 6)
    panics = read_conf_table(PANICS, 7)
    denies = deny_crates()
    hil = hil_rows()
    fuzz_targets, regress_inputs, findings = fuzz_counts()

    firmware_crates = [r[0] for r in panics]
    panic_total = sum(int(x) for r in panics for x in r[1:])
    unsafe_total = sum(int(r[1]) for r in unsafe)
    undocumented = sum(int(r[5]) for r in unsafe)
    cov = {r[0]: float(r[1]) for r in coverage}
    core_cov = cov.get("crates/picodroid-core")
    jvm_cov = cov.get("crates/jvm")

    L = []
    L.append("# Picodroid scorecard")
    L.append("")
    L.append("Generated by `scripts/scorecard.py --write` from the sources named in its header; "
             "`--check` runs in pre-commit and CI. **Do not edit by hand:** change the source and "
             "regenerate. Numbers here supersede estimates elsewhere; the nightly section is this "
             "machine's and is outside `--check`.")
    L.append("")
    L.append("## Summary")
    L.append("")
    L.append(table(
        ["Area", "Key numbers"],
        [
            ["[Android API parity](#android-api-parity)",
             "%s of the listed `android.*` surface (%d live, %d partial, %d missing, %d out of scope, %d picodroid-only additions; floor %.1f %%); %d SDK classes without a row"
             % (pct(total["live"] + total["partial"] / 2.0, total["live"] + total["partial"] + total["missing"]),
                total["live"], total["partial"], total["missing"], total["out-of-scope"], total["addition"], floor or 0.0, len(unlisted))],
            ["[Served `java/**` surface](#served-java-surface)",
             "%d classes, %d members across %d packages (`sdk/api-contract.tsv`)"
             % (sum(len(d["classes"]) for d in contract.values()), sum(d["members"] for d in contract.values()), len(contract))],
            ["[Firmware size](#firmware-size)",
             "; ".join("%s %s flash / %s RAM" % (b, kb(v["flash_bytes"]), kb(v["ram_bytes"])) for b, v in ratchet.items())],
            ["[Reliability](#reliability)",
             "never-panic denies on %d of %d firmware crates; %d panic sites and %d `unsafe` blocks (%d undocumented) left; "
             "coverage core %s / jvm %s"
             % (len(denies & set(firmware_crates)), len(firmware_crates), panic_total, unsafe_total, undocumented,
                ("%.1f %%" % core_cov) if core_cov is not None else "n/a",
                ("%.1f %%" % jvm_cov) if jvm_cov is not None else "n/a")],
            ["[Test matrix](#test-matrix)",
             "%d `hil-tests.conf` rows (%s); %d fuzz targets, %d regress inputs (%d fixed findings)"
             % (sum(hil.values()), ", ".join("%d %s" % (n, c) for c, n in sorted(hil.items())), fuzz_targets, regress_inputs, findings)],
        ]))
    L.append("")

    # Android parity
    L.append("## Android API parity")
    L.append("")
    L.append("Source: `sdk/android-parity.tsv` (one row per `android.*` class, status `live` / `partial` / "
             "`missing` / `out-of-scope`, and `addition` for a picodroid-only class with no Android counterpart). "
             "Parity is (live + partial / 2) over the rows in scope; out-of-scope rows and additions are not counted. "
             "The detail per class is the website's [compatibility matrix]"
             "(../website/src/content/docs/reference/compatibility-matrix.md). "
             "A `missing` row is a candidate for `docs/gaps.md`.")
    L.append("")
    rows = []
    for pkg in sorted(per_pkg):
        d = per_pkg[pkg]
        p = parity_pct(d)
        rows.append([pkg, d["live"], d["partial"], d["missing"], d["out-of-scope"], d["addition"], ("%.0f %%" % p) if p is not None else "n/a"])
    rows.append(["**all**", total["live"], total["partial"], total["missing"], total["out-of-scope"], total["addition"],
                 "**%.1f %%**" % overall if overall is not None else "n/a"])
    L.append(table(["Package", "Live", "Partial", "Missing", "Out of scope", "Additions", "Parity"], rows,
                   ["---", "---:", "---:", "---:", "---:", "---:", "---:"]))
    L.append("")
    missing = [r for r in parity_rows if r["status"] == "missing"]
    L.append("**Missing, in scope (%d):** " % len(missing) +
             ", ".join("`%s`" % r["android"] for r in missing) + ".")
    L.append("")
    if unlisted:
        L.append("**SDK classes with no row yet (%d):** " % len(unlisted) +
                 ", ".join("`%s`" % c for c in unlisted) + ". Add a row for each so the number above means the whole SDK.")
        L.append("")

    # java/** contract
    L.append("## Served java surface")
    L.append("")
    L.append("Source: `sdk/api-contract.tsv`, generated from pico-jvm's own tables by "
             "`api_contract_is_current` (`scripts/gen-api-contract.sh`). What an app may reference "
             "from `java/**` and `javax/**`; Gradle's `verifyApiContract` rejects the rest at build time.")
    L.append("")
    L.append(table(["Package", "Classes", "Members"],
                   [[p, len(d["classes"]), d["members"]] for p, d in sorted(contract.items())],
                   ["---", "---:", "---:"]))
    L.append("")

    # size
    L.append("## Firmware size")
    L.append("")
    L.append("Source: `bench/parity/ratchet.toml`, the committed baseline the nightly `size-ratchet` lane "
             "and `pre-commit --full` gate at 0 %% growth (release, `helloworld`, CI's pinned toolchain). "
             "Baseline last advanced in `%s` (%s); the per-commit history is `bench/parity/history.csv` "
             "(`scripts/bench-report.py --trend`)." % accepted)
    L.append("")
    rows = []
    for board, v in ratchet.items():
        rows.append([board, kb(v["flash_bytes"]), kb(v["ram_bytes"]), kb(v["text"]), kb(v["bss"])])
    L.append(table(["Board", "Flash", "RAM", "text", "bss"], rows,
                   ["---", "---:", "---:", "---:", "---:"]))
    L.append("")

    # reliability
    L.append("## Reliability")
    L.append("")
    L.append("Sources: `scripts/panic-baseline.conf` (`check-source-guards.sh --panics`, non-test firmware "
             "code), `scripts/unsafe-baseline.conf` (`--unsafe`), `scripts/coverage-baseline.conf` "
             "(`coverage.sh --check`, unit layer), and the `#![deny(clippy::unwrap_used, ...)]` attribute "
             "in each crate's `lib.rs`. Every ratchet only goes down (or, for coverage, up).")
    L.append("")
    unsafe_by = {r[0]: r for r in unsafe}
    rows = []
    for r in panics:
        crate = r[0]
        u = unsafe_by.get(crate)
        rows.append([crate,
                     "yes" if crate in denies else "",
                     sum(int(x) for x in r[1:]),
                     "%s/%s/%s" % (r[1], r[2], r[3]),
                     u[1] if u else "?",
                     u[5] if u else "?",
                     ("%.1f %%" % cov[crate]) if crate in cov else "n/a"])
    L.append(table(["Crate", "Denies panics", "Panic sites", "unwrap/expect/panic!", "`unsafe {`", "Undocumented", "Line coverage"],
                   rows, ["---", "---", "---:", "---:", "---:", "---:", "---:"]))
    L.append("")
    host = [r for r in coverage if r[0] not in {p[0] for p in panics}]
    if host:
        L.append("Host-side crates (coverage only, not ratcheted for panics): " +
                 ", ".join("%s %.1f %%" % (r[0], float(r[1])) for r in host) + ".")
        L.append("")

    # matrix
    L.append("## Test matrix")
    L.append("")
    L.append("Sources: `scripts/hil-tests.conf` (the rows `sim-run.sh` and `hil-run.sh` execute nightly, "
             "`check-hil-conf.sh` guards against drift), `fuzz/fuzz_targets` and `fuzz/regress` "
             "(`scripts/fuzz.sh`; `test.sh` replays every regress input on stable).")
    L.append("")
    L.append(table(["Category", "Rows"], [[c, n] for c, n in sorted(hil.items())] + [["**total**", "**%d**" % sum(hil.values())]],
                   ["---", "---:"]))
    L.append("")
    L.append("Fuzz targets: %d. Regress inputs replayed by `test.sh`: %d, of which %d are minimised findings "
             "(`finding-*`) from fixed bugs." % (fuzz_targets, regress_inputs, findings))
    L.append("")

    # nightly (machine-local)
    L.append("## Last nightly")
    L.append("")
    L.append(NIGHTLY_BEGIN)
    L.append("")
    L.append("This machine's newest result files under `build/sim/results/`, `build/hil/results/<slot>/` "
             "and `build/fuzz/results/`. Not compared by `--check`; a fresh checkout has none.")
    L.append("")
    rows = []
    sim = latest_results(REPO / "build" / "sim" / "results")
    if sim:
        rows.append(["sim", sim["date"] + " " + sim["time"], "`%s`" % sim["commit"], fmt_counts(sim["counts"])])
    for slot in sorted(glob.glob(str(REPO / "build" / "hil" / "results" / "*"))):
        if not os.path.isdir(slot):
            continue
        r = latest_results(pathlib.Path(slot))
        if r:
            rows.append(["hil `%s`" % os.path.basename(slot), r["date"] + " " + r["time"], "`%s`" % r["commit"], fmt_counts(r["counts"])])
    fz = latest_results(REPO / "build" / "fuzz" / "results")
    if fz:
        rows.append(["fuzz", fz["date"] + " " + fz["time"], "`%s`" % fz["commit"], fmt_counts(fz["counts"])])
    if rows:
        L.append(table(["Run", "Started", "Commit", "Verdicts"], rows))
    else:
        L.append("No result files on this machine.")
    L.append("")
    L.append(NIGHTLY_END)
    L.append("")
    L.append("---")
    L.append("")
    L.append("Generated at `%s`." % head_commit())
    return "\n".join(L) + "\n"


def fmt_counts(counts):
    return ", ".join("%d %s" % (counts[k], k) for k in ("PASS", "FAIL", "ERROR", "SKIP") if k in counts) or "empty"


def strip_nightly(text):
    a, b = text.find(NIGHTLY_BEGIN), text.find(NIGHTLY_END)
    if a < 0 or b < 0:
        return text
    text = text[:a] + text[b + len(NIGHTLY_END):]
    # Both lines come from git; a shallow CI checkout answers them differently.
    text = re.sub(r"Baseline last advanced in `\w+` \([0-9?-]+\)", "", text)
    return re.sub(r"Generated at `[0-9a-f?]+`\.", "", text)


def main(argv):
    problems = []
    page = render(problems)
    if "--write" in argv:
        OUT.write_text(page)
        print("wrote %s" % OUT.relative_to(REPO))
    elif "--check" in argv:
        if not OUT.exists():
            problems.append("docs/scorecard.md is missing: ./scripts/scorecard.py --write")
        elif strip_nightly(OUT.read_text()) != strip_nightly(page):
            problems.append("docs/scorecard.md is stale: ./scripts/scorecard.py --write (and commit it)")
    else:
        sys.stdout.write(page)
    if problems:
        print("")
        for p in problems:
            print("         " + p)
        print("")
        print("ERROR: the scorecard's sources disagree with the page or with each other (see above).")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
