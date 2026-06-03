---
title: "JVM tunables"
description: "Five [jvm] board.toml knobs that trade CPU against memory: GC frequency, heap chunking, inline-array threshold, activity-stack depth, pending-op queue depth."
---

The Picodroid JVM exposes five compile-time knobs that let a board choose its own CPU↔memory tradeoff without touching JVM source. Override them in your board's `[jvm]` section; leave them out to keep the defaults.

## TL;DR

| Key | Default | Range | Trades | Lower when… | Raise when… |
|---|---|---|---|---|---|
| `gc_alloc_threshold` | 256 | 16..=8192 | GC pause frequency ↔ peak heap | RAM is tight; tolerable pause cost | RAM is plentiful; pauses hurt latency |
| `slot_chunk_shift` | 6 | 3..=8 | Worst-case contiguous alloc ↔ slot metadata overhead | Heap fragments under FreeRTOS pressure | Heap is contiguous and large |
| `inline_array_data` | 8 | 0..=32 | Per-array struct size ↔ arena churn | Arrays are mostly large | Most arrays are short (≤16 ints) |
| `activity_stack_depth` | 8 | 1..=32 | UI nav nesting ↔ RAM | Single-screen app | Deep modal/wizard flows |
| `pending_op_queue` | 8 | 1..=64 | Lifecycle-op throughput ↔ RAM | Quiet UI | App fires many `startService`/`startActivity` per frame |

All five are compile-time `pub const`s, inlined at every use site. Zero RAM cost. Changing them changes the binary, not the running JVM.

## Why these knobs exist

Each tunable started as a hardcoded constant tuned to one board; over time the project hit cases where the right value diverged per target.

- **`gc_alloc_threshold`** was the literal `const GC_THRESHOLD: u16 = 256` in the interpreter. Commit `ceeae97` lowered it to 128 for an RP2350 OOM, then `93c5297` reverted after a 15% sim regression in string workloads. The value is genuinely board-dependent.
- **`slot_chunk_shift`** falls out of the `ChunkedSlots` refactor (commit `b43c413`) that unblocked picoenvmon on hardware. Before that, `Vec<Option<JvmObject>>` doubled its capacity on growth, eventually demanding a 90 KB contiguous block that the FreeRTOS heap could not serve once fragmented. Fixed-size chunks cap the worst-case request at `1 << shift × sizeof::<Option<JvmObject>>()`. Smaller chunks survive harsher fragmentation.
- **`inline_array_data`** is the inline-vs-arena threshold in `ArrayHeap`. Small arrays live in the slot struct; larger arrays go to a shared arena. Raising the threshold pulls more arrays inline (fewer arena allocations, faster access for short arrays) at the cost of a bigger slot struct. Lowering it is the opposite trade.
- **`activity_stack_depth`** and **`pending_op_queue`** size two fixed-capacity arrays on the RP native handler (Activity LIFO + lifecycle-op FIFO). The defaults cover any realistic Android-shaped UI; raise them only if you have nested modal flows or a single Activity that queues many `startService`/`startActivity` calls per frame.

Each one is a pure board-vs-board policy choice. The defaults match what the original hardcoded source said, so nothing changes for a board that doesn't opt in.

## How values reach the binary

The JVM crate is `no_std` and cannot read `board.toml` directly, so values flow through environment variables that `jvm/build.rs` snapshots at compile time. The two platform-side knobs take a shorter path because `platforms/rp/build.rs` already parses `board.toml`.

```
board.toml [jvm]
        │
        ├─ JVM-side (3 knobs)
        │       │
        │       │  scripts/lib.sh::apply_jvm_env
        │       ▼
        │  PICODROID_JVM_GC_ALLOC_THRESHOLD
        │  PICODROID_JVM_SLOT_CHUNK_SHIFT
        │  PICODROID_JVM_INLINE_ARRAY_DATA
        │       │
        │       │  jvm/build.rs (validates ranges)
        │       ▼
        │  $OUT_DIR/tunables.rs   ───►  jvm/src/tunables.rs   ───►  pub const at use site
        │
        └─ Platform-side (2 knobs)
                │
                │  platforms/rp/build.rs::emit_jvm_config (validates ranges)
                ▼
           $OUT_DIR/jvm_state_config.rs   ───►   state.rs   ───►   pub const at use site
```

Both paths end at a `pub const`. Constants are inlined; no value is ever stored in RAM, and no runtime indirection happens at the use site. A `cargo:rerun-if-env-changed=PICODROID_JVM_*` directive in `jvm/build.rs` re-compiles when you switch boards mid-session.

When you invoke `cargo build` directly (without `./scripts/sim.sh` / `flash.sh`), the wrapper scripts don't run and the env vars are unset. In that case the JVM picks the documented defaults — the same values you would get from a board with no `[jvm]` block.

## Tuning workflow

`perfbench` is the measurement instrument. Its composite SCORE folds wall time, GC cycle count, and peak heap into a single number you can grep for.

The formula (from [`examples/perfbench/java/perfbench/PerfBench.java`](https://github.com/shivrajora/picodroid-rs/blob/main/examples/perfbench/java/perfbench/PerfBench.java)):

```
score = wall_ms + W_GC × gc_cycles + peak_kb / W_PEAK_DIV
        where W_GC = 1 and W_PEAK_DIV = 10
```

Lower SCORE means a better tradeoff. The loop:

1. **Baseline** — `./scripts/sim.sh --app perfbench --board <board>`. Note the `SCORE ...` line.
2. **Change one knob** in that board's `board.toml`. Just one — interactions between knobs are real, and isolating the effect of each move is what makes future maintainers trust the chosen value.
3. **Re-run** the same command. Compare the new SCORE.
4. **Decide** — keep the change if SCORE drops *and* per-test lines tell a coherent story (peak heap down without GC count exploding, etc.). Document the rationale next to the value in `board.toml` so the next maintainer knows *why*.

Repeat on hardware (`./scripts/flash.sh`) before declaring a tuning final — the sim heap is generous and rarely fragments the way FreeRTOS does on an MCU.

## Recipes

Three concrete starting points for common shapes of board:

### Heap-constrained board (RP2040-class)

```toml
[jvm]
gc_alloc_threshold = 128   # collect more often, shrink the high-water mark
slot_chunk_shift   = 4     # 16-slot chunks; halves the worst-case contiguous request
```

This mirrors the picoenvmon heap-budget work — small heap, fragmenting allocator. The drawback is more GC pauses, which is acceptable because the board's bottleneck is RAM, not CPU.

### CPU-constrained board with plenty of RAM (RP2350 / RP2350W with display thread)

```toml
[jvm]
gc_alloc_threshold = 512   # halve the GC rate; trade peak heap for wall time
inline_array_data  = 16    # short array operations stay in the slot struct
```

Trades a higher peak heap for shorter wall time and fewer pauses. Watch peak heap with perfbench; if it climbs above what your display + framework leave free, back off.

### Deep-nav UI (modal dialogs over a wizard)

```toml
[jvm]
activity_stack_depth = 16
pending_op_queue     = 16
```

Doubles two fixed-size buffers in the platform native handler. Each entry is small (≤ 24 bytes), so the RAM cost is roughly 200 extra bytes — well under what the freed heap gains from avoiding silent `false`-returning enqueue failures during a heavy lifecycle burst.

## Limits and pitfalls

- **Out-of-range values fail the build.** Both `jvm/build.rs` and `platforms/rp/build.rs::emit_jvm_config` validate against the declared range and `panic!` with a clear citation. The accepted bounds are designed so that even the extremes are safe — the upper bound on `gc_alloc_threshold` (8192) does not OOM any board the project has tested, and the lower bound on `slot_chunk_shift` (3 = 8-slot chunks) does not measurably slow index math.
- **`const_assert`s in `jvm/src/tunables.rs` are a second line of defence** against a corrupted generated file. They fire at type-check time with a clear message.
- **Direct `cargo build` skips the script bridge.** If you're not using `./scripts/sim.sh` or `./scripts/flash.sh`, the `PICODROID_JVM_*` env vars are unset and the JVM picks defaults. Either export them yourself or stick with the wrapper scripts.
- **Cache invalidation is automatic.** `cargo:rerun-if-env-changed` directives in `jvm/build.rs` mean a board switch (different env values exported by the wrapper script) re-compiles just the affected crates.
- **One knob at a time.** Interaction effects exist — e.g. lowering `gc_alloc_threshold` while also lowering `slot_chunk_shift` overweights memory at the cost of CPU. Tune one, measure, then move on.
- **Activity-stack and pending-op caps are not Java-visible errors.** Enqueue overflows return `false` and are logged but do not throw a `RuntimeException`. The defaults are conservative on purpose; if your UI legitimately needs more depth, raise these explicitly.

## See also

- [`[background_pool]`](/reference/porting-guide/#background_pool--optional-thread-pool-tuning) — adjacent thread-pool tuning in the same `board.toml` schema.
- [`perfbench`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/perfbench) — the speed + memory composite-score benchmark used in the tuning workflow.
- [`jvm/build.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/jvm/build.rs) — env-var reader for the three JVM-side knobs.
- [`platforms/rp/build.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/build.rs) — `emit_jvm_config` for the two platform-side knobs.
- [`scripts/lib.sh`](https://github.com/shivrajora/picodroid-rs/blob/main/scripts/lib.sh) — `apply_jvm_env` shell-side bridge from `board.toml` to environment.
- [Porting guide](/reference/porting-guide/) — full `board.toml` schema, MCU contract.
- [Advanced configuration](/reference/advanced-config/) — files outside `board.toml`.
