# crates/fuzz-targets

The harness bodies behind `fuzz/fuzz_targets/*.rs` (libFuzzer, nightly Rust) and `tests/replay.rs`, which replays `fuzz/regress/**` on stable through `./scripts/test.sh`. Design and the list of what each target asserts: `docs/designs/fuzzing-2026-10.md`; how to run: `docs/fuzzing.md`.

- **Run:** `./scripts/fuzz.sh --target json --time 60 --jobs 2` (a minute), `--replay` (the committed inputs), `--nightly` (what the 3 AM cron runs). `fuzz.sh` passes the host triple on every `cargo fuzz` call: a prebuilt musl cargo-fuzz otherwise targets musl and fails on every dependency.
- **A target asserts a property**, tying a check to what is later read without one; "does not panic" alone is the weakest form. Harness bodies may `panic!` and `unwrap()` (this crate is outside the panic ratchet), the code under test may not.
- **A finding:** minimise (`cargo fuzz tmin`), commit the input as `fuzz/regress/<target>/finding-<what>` with the fix, and say what it was in the commit message. Seeds are `seed-*`.
- **Harness traps seen:** `reaches(n, n)` is a no-op, duplicate keys in generated maps, varints that only use the low 32 bits, an epoch of zero in a clock, a `Link::from_raw` that fails Stacked Borrows under Miri (open).
- **This crate is `test` in `scripts/layers.conf`:** it may depend on anything; nothing may depend on it. A new target for a new leaf crate adds the crate here and under `fuzz/Cargo.toml` (its own lockfile; `fuzz.sh` checks the `[patch]` tables agree).
