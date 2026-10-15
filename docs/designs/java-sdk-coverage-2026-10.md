# Java SDK line coverage (design, 2026-10)

Status: design only, not built. The third layer of [coverage](../coverage.md):
the Rust layers measure the runtime, and this one would measure the code
developers actually call, `sdk/java/**` (about 230 files, 26.7k lines).

## Problem

`sdk/java` never runs on a host JVM. It is compiled, packed into the
framework section, and interpreted by `pico-jvm` in the simulator or on a
board. JaCoCo and similar tools work by rewriting bytecode for a HotSpot
agent, so they cannot see it. The qa_* suites, langsuite and the demo apps
exercise the SDK every night, but nothing records which methods or lines
they reach. The interpreter is the only thing that sees every executed
bytecode, so it has to do the recording.

## Shape

1. **Recording: a `bytecode-coverage` feature on `pico-jvm`, for the sim
   only.**
   - The dispatch loop in `Executor::run` (`crates/jvm/src/interpreter/mod.rs`)
     already has a per-instruction feature hook at the
     `crate::parity::count_insn()` site (`parity-metrics`). The coverage hook
     sits beside it and sets one bit per executed `(method, pc)`.
   - Storage is a host-side `HashMap<method id, BitVec>` in a `std`-only
     module. The feature is refused unless the `sim` feature is on, so the
     device build can never pay for it.
   - `pc` is the frame's program counter before dispatch, so a bit covers
     the first byte of an instruction.
2. **Dump at exit.**
   - `sim_boot` writes the map at the same point it ends the scheduler.
   - It also writes from the SIGTERM path, because sim-run stops most rows
     with SIGTERM. LLVM continuous mode makes that free for the Rust layer,
     but it does nothing for a Rust `HashMap`. Either the SIGTERM handler
     flushes the map, or the map lives in an mmap'd file the way
     `%c` profiles do.
   - Output is `PICODROID_JCOV_FILE` (default
     `$CARGO_TARGET_DIR/jcov-<pid>.txt`), one line per method:
     `class method descriptor hex-bitset`.
   - Names must be the original ones, so runs are no-shrink only. sim-run
     `--coverage` already restricts itself to no-shrink.
3. **Mapping to lines: a `class-shrink jcov` subcommand.**
   - `tools/class-shrink/src/retrace.rs` already maps `(class, method, pc)`
     to a source line through the `LineNumberTable` of the unstripped host
     class trees (`Retracer::load_classes`). The new subcommand reuses that
     lookup over every set bit.
   - It also reads every *loaded-but-unexecuted* method from the same class
     trees, so the denominator is all SDK lines, not just the methods that
     ran.
   - It emits lcov `SF:sdk/java/...` records.
4. **Report: the `--java` flag on `coverage.sh`.**
   - It merges the per-process dumps, runs `class-shrink jcov`, and renders
     `build/coverage/java/` with `genhtml`, or with a small Python renderer
     if `genhtml` (lcov) should not become a dependency.
   - The per-package table uses the same markdown shape as
     `coverage-summary.py`.

## Costs and limits

- **Speed.** The hook is a hash lookup per instruction, so expect the sim to
  run 2–5× slower. A per-frame cached `&mut BitVec` resolved at frame push
  would make it one bit-set per instruction. Device builds are unaffected:
  the feature is sim-only.
- **Line granularity.** Coverage is only as fine as `LineNumberTable`. A
  line with several branches counts as covered when any instruction on it
  runs. Branch coverage would need the bitset compared against the CFG, a
  later step.
- **Kotlin.** `kotlin-shim` and the Kotlin fixture apps compile to the same
  bytecode, so the same pipeline covers them. Inline functions map to
  synthetic line numbers past the end of the file (`SMAP`), and the mapper
  has to skip those.
- **What it cannot show.** It cannot cover a code path only real hardware
  takes, such as a sensor that answers or a radio that joins. The HIL
  matrix stays the evidence for those.

## Open questions

- Should the recording also count calls per method? That turns the dump into
  a profile (`perfbench` would like one) at the cost of 4 bytes per method.
- Should it be one run per app, merged, or one long-lived sim that installs
  apps through `pdb` (`PICODROID_SIM_WAIT_FOR_INSTALL=1`)? The second saves
  boot time but gives up the per-row isolation that sim-run relies on.
- Should app code (`examples/**`) be reported too, or only `sdk/` and
  `system-apps/`? App coverage mostly measures the demos themselves.
