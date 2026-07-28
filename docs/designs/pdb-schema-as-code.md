# Design: PDB schema-as-code (payload layouts into `pdb-protocol`)

> Produced 2026-07-27 from a PDB structure audit (host tool, device bridge,
> abstraction boundary). Execute from this doc; append amendments as reality
> diverges. Sibling of `family-neutral-residue.md`, whose stage 3d/3e moved
> the *logic* into `picodroid-core` — this work moves the remaining *layouts*
> into `pdb-protocol`.

## 0. Ground truth

`pdb-protocol` already owns the frame: magic, command and status bytes,
`CMD_INPUT` subtypes, `INSTALL_PEEK_BYTES`, the CRC. Both ends compile it, so
those cannot drift. What still drifts by hand is one level up — three payload
layouts encoded on one end and decoded on the other, each described only in a
doc comment on each side:

| Payload | Encoded | Decoded | Layout doc |
|---|---|---|---|
| PING greeting | `picodroid-core/src/pdb/mod.rs` (`ping_greeting`) | `tools/pdb/src/install.rs` (`parse_ping_payload`) | host doc comment |
| Sysmon response | `picodroid-core/src/pdb/sysmon.rs` (`encode`) | `tools/pdb/src/sysmon.rs` (magic offsets) | device doc comment + golden test |
| Input event | `tools/pdb/src/input.rs` (`encode_*`) | `picodroid-core/src/pdb/input.rs` (`rd_i32` etc.) | subtype doc comments in `pdb-protocol` |

Plus one table duplicated host-side: the Android keycode names
(`tools/pdb/src/input.rs` and the sim control channel in
`picodroid-core/src/hal/sim/display.rs`).

Two audit questions answered in the negative, recorded so they are not
re-litigated:

- **No Python host tool.** The schema *is* Rust; `tools/pdb` compiles it
  natively (`pub use pdb_protocol::*`). A Python port would reintroduce the
  hand-mirroring `pdb-protocol` exists to kill, or force an IDL + codegen to
  break even — and `scripts/hil-run.sh` invokes the release binary directly.
- **No external IDL.** serde/postcard/protobuf add dependencies to a
  deliberately zero-dep `no_std` crate and cannot express the contracts that
  matter here ("a 2.0 host reads exactly 18 bytes", "additions go at the
  end"). With both consumers in Rust, typed encode/decode functions plus
  golden-bytes tests *are* the schema.

## 1. Shape

`pdb-protocol` grows one module per payload; the crate invariants hold:
`#![no_std]`, zero dependencies, no features, no alloc.

- **Encoders write into caller-provided fixed buffers** and return the length
  — the device's existing pattern, only the function's home moves.
- **Decoders are zero-copy borrow views / stack enums.** The device never
  calls them; they are host-only code that happens to live next to the
  encoder so a layout change must touch both or fail the round-trip test.
- **State and I/O stay out.** The sysmon `PREV` sample, `SysmonSource`,
  `PdbTransport`, and the install traits are `picodroid-core`'s (B8 settled
  where `PREV` belongs). `pdb-protocol` never depends on `picodroid-core`;
  build facts like the framework-map-version are parameters.
- **Golden tests move with the layouts** — moved, never copied; a stale
  duplicate is exactly the drift they exist to catch. Every field gets a
  distinct fixture value (B8's swapped-fields lesson, learned three times).

## 2. Stages

- **A — greeting** (`greeting.rs`): version literals (`picodroid/2.1`,
  legacy `2.0`, the `picodroid/` prefix), `encode`, `Greeting::parse`,
  `is_legacy`. Host keeps policy (what to refuse); crate owns bytes. The
  18-byte legacy prefix is pinned by name.
- **B — sysmon** (`sysmon.rs`): `SysmonSample`/`TaskSample`, layout consts,
  `compute_cpu_pct`, `encode`, `MemDiag` tail, and a host-side `SysmonView`
  replacing `tools/pdb`'s magic offsets. `response_layout_is_frozen`
  relocates here.
- **C — input** (`input.rs`): `InputEvent` enum with `encode`/`decode`
  replacing `tools/pdb`'s `encode_*` and core's `rd_*` readers. Wire-level
  round-trip tests land in core: host-built frames through the mock
  transport into the real handlers, responses parsed with the shared
  decoders.
- **D — keycodes** (`keycodes.rs`): the Android keycode table and name
  lookup (alloc-free), shared by the host CLI and the sim control channel.
- **E — truth-telling**: `scripts/pdb.sh` stops carrying its own (drifted)
  help and defers to the binary; stale claims in `ARCHITECTURE.md`,
  `platforms/rp/src/hal/mod.rs`, and `hal/sim/pdb_usb.rs` are corrected.

Each stage passes the full pre-commit suite + sim smoke and records its
rp2040 release section deltas below. Wire bytes are identical throughout —
no version bump; the golden tests are the proof.

## 3. Deferred: a sim PDBP endpoint

Considered and deliberately deferred (2026-07-27): a `PdbTransport` over
TCP/pty in the sim would give the port surface a second real transport impl
and let the host CLI run wire-true against the sim (PING/SYSMON/INPUT;
install refused cleanly by stub coordinator/flash). It would also need a
`-s tcp:...` opener in `tools/pdb` (today three `serialport::new` sites).
Deferred because stage C's in-memory wire round-trips buy most of the
drift-detection at ~1% of the cost, and a *full* endpoint drags in sim
install/park/reboot semantics that do not exist and deserve their own
design. Revisit if a second MCU family lands or sim-side HIL rows become
worth their keep.

## 4. Measurement (amend per stage)

rp2040 `testbench_rp2040 --app helloworld --release`, `arm-none-eabi-size -A`.
Baseline = family-neutral-residue B9 "after 3f".

| Stage | `.text` | Δ | `.rodata` | Δ |
|---|---:|---:|---:|---:|
| Baseline (7b19a6a) | 704,432 | — | 196,312 | — |
| A — greeting | 704,448 | +16 | 196,312 | 0 |
| B — sysmon | 704,448 | 0 | 196,336 | +24 |
| C — input | 704,544 | +96 | 196,344 | +8 |
| D — keycodes | 704,544 | 0 | 196,344 | 0 |
