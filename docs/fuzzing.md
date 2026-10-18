# Fuzzing (`scripts/fuzz.sh`)

Coverage-guided fuzzing of every parser that takes untrusted bytes: the
PAPK container and its install path, the class section and the class
linker, the interpreter on checked bytecode, the pdb payloads, the HTTP
head and chunked body, JSON, and the TLS handshake. The design and the
reasons behind it are in
[designs/fuzzing-2026-10.md](designs/fuzzing-2026-10.md).

Audience: developers and AI agents. Every command below is copy-pasteable.

## The pieces

| Path | What |
|---|---|
| `crates/fuzz-targets/` | Each target as a plain `fn(&[u8])` that panics on a finding. A workspace member with no new dependencies, so clippy and `cargo test` see it like any other crate |
| `crates/fuzz-targets/tests/replay.rs` | Every file in `fuzz/regress/` through its target, on the pinned stable toolchain. It runs in `scripts/test.sh`, so in CI on every push |
| `fuzz/` | The cargo-fuzz crate: one three-line `fuzz_target!` shim per target. Nightly-only and excluded from the workspace |
| `fuzz/regress/<target>/` | Committed. `seed-*` are the starting inputs; `finding-*` are minimized reproducers of past bugs, each added by its fix |
| `fuzz/dict/<target>.dict` | libFuzzer dictionaries: magic numbers, section tags, header names |
| `build/fuzz/` | Untracked. `corpus/<target>/` is the living corpus, kept across runs; `crashes/`, `logs/` and `results/` hold each run's output |

## Setup

Run this once per machine. The bench machine already has both.

```bash
rustup toolchain install nightly
cargo install cargo-fuzz --locked
```

## Running

```bash
./scripts/fuzz.sh --list                          # targets, weights, sanitizers
./scripts/fuzz.sh --replay                        # fuzz/regress/ through every target (stable, seconds)
./scripts/fuzz.sh --target papk --time 300        # fuzz one target for 5 minutes
./scripts/fuzz.sh --target all --time 60          # every target for a minute each
./scripts/fuzz.sh --target papk --reproduce FILE  # rerun one input under the fuzz build
./scripts/fuzz.sh --build                         # build every target (pre-commit --full does this)
```

`--jobs N` sets the number of libFuzzer worker processes; the default is
the core count minus two. Targets that reach `unsafe` code (class-link's
in-place tables, the installer's flash mapping, the TLS session, the
interpreter) run under AddressSanitizer. The rest run without a sanitizer,
because their crates forbid `unsafe` and ASan would only halve the rate.

## When it finds something

1. The run stops that target and prints the panic.
   `build/fuzz/crashes/<run>/<target>/` holds the input, and `fuzz.sh`
   minimizes it. The minimized copy lands in `fuzz/artifacts/<target>/`,
   which is ignored.
2. Reproduce it on stable: copy the input into `fuzz/regress/<target>/`
   under a `finding-<what>` name and run `./scripts/fuzz.sh --replay`.
3. Fix the bug. Keep the `finding-*` file in the fix commit: from then on,
   every CI run replays it.

Not every finding is a parser bug. A target's property can be wrong too.
Read the assertion before changing the code under test.

`interp` runs the JVM inside a 1 MB heap budget (`crates/fuzz-targets/src/budget.rs`),
because a board's heap is a few hundred kilobytes and the host's is the
whole machine. `memory allocation of N bytes failed` from that target means
an allocation that a Java program can reach grows infallibly. On a board,
that is a reset when the heap is full. Make the allocation fallible, the
way the rest of the crate does: `try_reserve`, then
`JvmError::StackOverflow`, which the main loop turns into an
OutOfMemoryError. To see where the allocation is:

```bash
ASAN_OPTIONS=detect_leaks=0 gdb -batch -ex 'break alloc::alloc::handle_alloc_error' \
  -ex run -ex bt --args fuzz/target/x86_64-unknown-linux-gnu/release/interp FILE
```

## Nightly

The bench machine runs the whole set at 03:00, as the first stage of
`scripts/nightly.sh` (fuzz, then the sim matrix, then the HIL fleet, each
starting when the previous one exits):

```text
0 3 * * * cd /home/shiv/projects/picodroid-rs && mkdir -p build/nightly && ./scripts/nightly.sh >> build/nightly/cron.log 2>&1
```

`nightly.sh` pulls main once and runs `fuzz.sh --nightly --no-pull` under a
3500 s `timeout`; the fuzzer's own output goes to `build/fuzz/cron.log`.
`--nightly` does the following:

1. Pulls main (unless `--no-pull`).
2. Runs the stable replay.
3. Checks that `fuzz/Cargo.toml` restates the root's `[patch.crates-io]`.
4. Fuzzes every target for its share of 45 minutes. The share is set by the
   weight column of `--list`, and the corpus carries over from the night
   before.
5. Mails the result through `hil-email.py --suite fuzz`. Like the sim and
   HIL mails, it separates new failures from known ones.

The sim stage starts when this one exits, so a bigger budget delays the sim
and HIL results by the same amount. The `timeout` is only a backstop.

On Sundays the run gives 15 of its minutes to two chores:

- `cargo fuzz cmin` keeps each corpus to the inputs that still add coverage.
- A Miri pass replays the regression inputs of the targets that reach
  `unsafe` in `class-link` and the JVM heaps
  (`PICODROID_FUZZ_REPLAY=class_section,link_class,bytecode_check,papk,interp`).
  Miri checks those casts in a way ASan cannot.

GitHub's weekly `rust_nightly.yml` also builds every target and gives each
one a minute, so `fuzz/` does not rot unseen.

## Adding a target

1. Write `pub fn <name>(data: &[u8])` in a module of `crates/fuzz-targets`
   and add it to `TARGETS` in `src/lib.rs`. Assert a property, not just "no
   panic". The useful ones tie a check to what is later read without one,
   or round-trip through a writer.
2. Add the shim, `fuzz/fuzz_targets/<name>.rs`, and its `[[bin]]` in
   `fuzz/Cargo.toml`.
3. Add a row to `TARGETS` in `scripts/fuzz.sh`: name, weight, sanitizer,
   and `noleaks` if the target leaks on purpose.
4. Seed it: add inputs to `tests/seeds.rs` and regenerate with
   `cargo test -p fuzz-targets --test seeds -- --ignored write_seeds`. The
   replay test fails on a target with no inputs.
5. If the target reaches a crate that the root `Cargo.toml` patches, copy
   that patch line into `fuzz/Cargo.toml`. `fuzz.sh` refuses to run until
   the two agree.

## What is not fuzzed

The C code is not fuzzed: FreeRTOS+TCP's DNS and DHCP, and the cyw43
driver. Nor are pack-time inputs, the device pdb byte stream, or LittleFS
metadata. [designs/fuzzing-2026-10.md](designs/fuzzing-2026-10.md) §6 says
why for each.
