# `Value` 16 B → 8 B: two-slot longs behind a `Slot` storage type

**Status: evaluated and designed 2026-09-03, not started.** Tracked in
`../quality-roadmap.md` (Memory footprint). Origin: the
`perf-campaign-2026-08.md` "what is left" bullet, which called this the
largest memory lever left and asked for a design doc. This is that doc: the
feasibility verdict, the design, the expected saving, the risks, and the
stages to build it. Every code fact below was verified against the tree on
2026-09-03; line numbers are from that day.

## Short answer

**Yes, it is feasible. The work is contained. The saving is real but modest.
Measure first, then build.**

Four things drive that answer:

1. **There is only one way to get to 8 bytes.** Today a `Value` can hold a
   64-bit `long` or `double`. An 8-byte value cannot hold a 64-bit number plus
   a type tag. So `long` and `double` must be split into two 4-byte halves,
   stored in two slots. This is exactly how the JVM spec lays them out, so it
   is a fidelity improvement, not a hack.

2. **We can do it without touching most of the code.** Keep today's 16-byte
   `Value` as the type that opcode handlers and native methods pass around.
   Add a new 8-byte `Slot` type for what is actually *stored* in memory. Only
   the places that store values change (about 12 files in `jvm/`). The 1,350
   production sites that pattern-match on `Value`, and all 400 native method
   arms, stay as they are. I checked with a `rustc` probe: the proposed
   `Slot` is 8 bytes on x86_64, thumbv6m and thumbv8m.

3. **The saving is a few KB to 20 KB depending on the board.** Objects get
   about 40 % smaller. The memory where object fields live halves. On the
   enviro boards, the 40 KB claimed at boot for object fields becomes
   20 KB. That is about 11 % of picoenvmon's peak heap. Arrays, strings,
   class metadata and LVGL are not affected, and those are the bigger
   consumers. So this is the largest lever *left*, not a large lever.

4. **The costs are known.** Long and double arithmetic gets a little slower.
   RP2040 flash may grow 1 to 3 KB. Five hand-written tables in
   `picodroid-core` that number object fields by hand will silently break
   unless we fix them and add a test.

## What is true today (all verified in code)

- `Value` is defined at `jvm/src/types.rs:15`. It has eight variants:
  `Int`, `Long`, `Float`, `Double`, three kinds of reference (each a `u16`
  index), and `Null`. `Long` and `Double` force 8-byte alignment, so the
  enum is 16 bytes on every target.
- One compile-time check pins the size:
  `jvm/src/object_heap/mod.rs:795-796`. Nothing else depends on the layout.
- Values are stored in bulk in only a few places:
  - each call frame's locals and operand stack (`jvm/src/frame.rs:12-13`);
  - the object fields arena, one 16-byte slot per field no matter the type
    (`jvm/src/object_heap/mod.rs:95`);
  - ArrayList and HashMap backing buffers (`object_heap/mod.rs:105-106`);
  - lambda captures (`object_heap/mod.rs:83`);
  - static fields (`jvm/src/static_fields.rs:16`, a small table).
- Arrays are already packed and hold no `Value`. Strings and the constant
  pool hold no `Value`. `platforms/rp` has zero `Value` sites.
- Long and double are handled inconsistently today. On the operand stack
  they take one slot (`ops_stack.rs:13`). In locals they take two slots, the
  second one a dead `Null` filler (`frame.rs:34-49`). In object fields they
  take one slot. Argument counting counts values, not slots
  (`helpers.rs:181`). A `long` local costs 32 bytes.
- The garbage collector is precise. It reads the tag to find references
  (`gc/mod.rs:60-68`). Any new layout must keep a tag on every slot.

## The design

### The new storage type

Add to `jvm/src/types.rs`:

```rust
/// What frames, the fields arena and the side tables hold.
/// `Value` stays the type opcodes and natives see.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slot {
    Int(i32), Float(f32),
    // A long or double is two slots. Low half first (lower index in
    // locals and fields, pushed first on the stack, so high half is on top).
    LongLo(u32), LongHi(u32), DoubleLo(u32), DoubleHi(u32),
    Reference(u16), ObjectRef(u16), ArrayRef(u16),
    Null,
}
const _: () = assert!(core::mem::size_of::<Slot>() == 8);
```

Why four half variants instead of one generic "half"? It costs no bytes and
it keeps three things: the GC stays precise, a lone or mismatched half is
detected as bad bytecode (the same safety net that caught the `Null` versus
`Int(0)` bug in commit `17ab8bc`), and a generic `pop()` can rebuild the
right `Value` without looking at a descriptor.

Small helpers go with it: `Value::to_slots`, `Slot::assemble(lo, hi)`,
`Slot::from_narrow(Value)` (fails for long/double; used by the side tables,
which only hold references), and `Value::slot_width()`.

### Frames: the accessors hide the change

`jvm/src/frame.rs` stores `Vec<Slot>` for locals and stack.

- `push(Value)` writes one or two slots.
- `pop()` returns a `Value`. It pops one slot. If that slot is a high half,
  it pops the low half too and rebuilds the number. The rebuild path is
  marked cold and out of line so the common case stays small.
- `load_local` and `store_local` work the same way at `idx` and `idx + 1`.
- New slot-exact `push_slot` and `pop_slot` for `ops_stack.rs`. With two-slot
  longs, the whole `dup`/`pop2` family becomes the plain slot shuffles the
  spec describes. The `is_cat2` check and the four-way `dup2_x2` match go
  away.
- `Frame::new(args: &[Value])` expands longs into two slots. A new
  `Frame::from_slots(&[Slot])` copies slots straight in, for Java-to-Java
  calls.

Because of this, every opcode file (`ops_math`, `ops_convert`,
`ops_control`, `ops_arrays`, `ops_locals`, `ops_wide`, `ops_constants`), the
return path at `interpreter/mod.rs:689-698`, and `helpers::box_primitive`
compile unchanged. They only ever see `Value`.

### Method calls (`jvm/src/interpreter/ops_invoke.rs`)

- Rename `helpers::count_args` to `count_arg_slots` and count `J` and `D` as
  two. The rename makes the compiler point at all three callers (`:57`,
  `:59`, `:520`).
- The inline argument buffer becomes `[Slot; 16]`. Same 128 bytes as today.
- Calling Java: `Frame::from_slots` on the popped slots.
- Calling a native: decode the slots into `[Value; 8]` using the existing
  `helpers::ParamKinds` iterator (`helpers.rs:209`), then pass `&[Value]`
  exactly as today. `NativeContext`, every native arm, and upcalls do not
  change.
- The receiver peek at `:77` and `:336` still works, since the receiver is
  the first slot. Lambda captures and the lambda argument vector become
  slots. The string `<init>` swap at `:395` compares `Slot::ObjectRef`.

### Object fields (`jvm/src/object_heap/mod.rs`)

- The arena becomes `Vec<Slot>`. The per-object `fields_cap` and
  `field_count` (both `u8`) now count slots. 255 is still far above any SDK
  class. Only 168 of 4,808 fields in the SDK and examples are `long` or
  `double`.
- `get_field` rebuilds a long when it reads a low half. `set_field` writes
  one or two slots inside the existing single atomic section. A torn read of
  a non-volatile long is allowed by the Java spec and is no worse than
  today's multi-word write.
- `helpers::field_slot_in` (`helpers.rs:344`) advances by the field's width.
  `alloc_with_defaults` (`:409-515`) sizes and zero-fills the same way.
- `default_field_count_for_native` (`:25`): boxed `Long` and `Double` need
  two slots (`native/boxed.rs:475`).
- `fields_slice` returns `&[Slot]`. Arena compaction does not change.
- Update the size assert to 8. Keep `FIELDS_ARENA_CHUNK` at 256 slots (now
  2 KB per step) and fix the comment at `:322-326`, the sentinel text in
  `docs/memory-diagnostics.md:119`, and `gc/tests.rs:1036-1037` and `:1078`.

### Side tables

`list_store.rs`, `map_store.rs` and `LambdaProxy.captures` store `Slot`.
A HashMap entry drops from 32 to 16 bytes. Their APIs keep `Value`
parameters. A long passed in is refused with `None`; it cannot happen, since
elements are always references. The census uses `size_of::<Slot>()`.

### Garbage collector (`jvm/src/gc/mod.rs`)

`push_ref` takes a `&Slot`. Frames, parked frames, fields, lists, maps and
captures are walked as slots. `shadow_roots` and the root callbacks in
`picodroid-core/src/gc_roots.rs` stay on `Value`; they only carry references.
Fix the stale "8 bytes" comment at `gc/mod.rs:115`.

### Static fields: leave alone

`StaticEntry` keeps a 16-byte `Value`. The table is tiny and already costs
32 bytes per entry from two fat pointers. Changing it touches 57 sites for
no measurable gain.

### The hidden hazard: hand-numbered field slots

Five places in `picodroid-core` reach into SDK objects using constants
written by hand, one number per field:

- `picodroid-core/src/native_handler/io.rs:17-28`
- `picodroid-core/src/pio/fields.rs`
- `picodroid-core/src/hardware/sensors/mod.rs:52-66` (`fields` for `Sensor`,
  `event_fields` for `SensorEvent`)
- `picodroid-core/src/graphics/fields.rs` (`motion_event`, used by
  `lifecycle.rs:1530` and `:1882`)
- `jvm/src/native/random.rs:44` and `:232` (slots 0 and 1 of `Random`)

With two-slot longs, every constant after a `long` or `double` field moves
by one. For example `pwm::DUTY_CYCLE` goes from 2 to 3. Every
`alloc_with_field_count(.., LAST + 1)` must add the last field's width. No
test checks these today. Add one: a `picodroid-core` unit test that reads the
SDK class files in `sdk/build/classes/java/main` and asserts each constant
matches `field_slot_in`. Then fix what it reports.

## Expected saving (Stage 0 replaces these with measurements)

| Where | Today | After | Source |
|---|---|---|---|
| Object with *n* ordinary fields | 12 + 16n B | 12 + 8n B | `JvmObject` stays 12 B |
| picoenvmon live objects (44 objects, 174 slots) | 3,312 B | about 1,970 B | `mem-session-2026-08.md` |
| Enviro boards' boot-claimed fields arena (2560 slots) | 40,960 B | 20,480 B | `board.toml:32` and `:75` |
| One HashMap entry | 32 B | 16 B | `map_bufs` |
| One call frame (5 to 7 slots on average) | 80 to 110 B | 40 to 55 B | javap survey |
| A `long` local | 32 B | 16 B | filler becomes the real high half |
| A `long` field, or a `long` on the stack | 16 B | 16 B | no change |
| Arrays, strings, class metadata, LVGL, statics | no change | no change | untouched |

Frames are not where the bytes are. The javap survey over SDK and examples
gives an average `max_stack` of 2 to 3 and `max_locals` of 2 to 3.5. A
10-deep call chain saves about half a KB. The fields arena and its boot
reservation are the real saving.

## Costs and risks

- **Long and double get slower.** Each long or double opcode does two extra
  slot moves and a tag check. On the device benchmark, the long section is
  about 4 % of total time (6.9 s of about 175 s in
  `bench/parity/history.csv`). The total will stay under the 4 % floor that
  the perf doc says is unmeasurable on device. The sim sections will show
  it. Accept it and record the number.
- **RP2040 flash.** `pop()` and `load_local()` are inlined into hundreds of
  opcode arms. A tag check in each copy could add 1 to 3 KB. Keep the
  rebuild path cold and out of line. The size ratchet in
  `./scripts/pre-commit` reports the real number. Headroom is 67.6 KB.
- **The hand-numbered tables.** Covered by the new test above.
- **The `prereserve_fields_values` tunable** keeps its slot count. Its byte
  cost halves. Boards can keep 2560 and bank 20 KB, or raise it. Default:
  keep.
- **Docs go stale.** `docs/parity-audit.md:65` and the table at `:455`,
  `object_heap/mod.rs:322-326`, the sentinel text in
  `memory-diagnostics.md`, and `jvm-tunables.md:20`. All doc edits.
- **Test churn is small.** Native tests build arguments with `Value::Long`,
  which does not change. Only `frame.rs` tests, interpreter tests that look
  at `locals` or `stack` directly (about 46 sites), and the GC arena-step
  test change.
- **No sim versus device drift.** `Slot` has no pointers, like `Value`.
  `Frame` and `Option<JvmObject>` stay the same size on every target.

## Stages

Work on a worktree branch, `feat/value-slot-8b`. Stage 0 lands its design
doc first. Stages 1 to 3 are one PR.

### Stage 0: measure, then write the design doc (go/no-go gate)

1. Run the sim with `--mem-diag` for `helloworld`, `benchmark`, `perfbench`,
   `picoenvmon` (on `pico_enviro_mon_w`) and `threaddemo`. Capture the
   `[memmon] storage … fields_cap=…` lines and the `heapcensus` object and
   side-table rows. Add a one-line frame census (sum of locals and stack
   capacity over live frames). Nothing reports frame bytes today.
2. Write `docs/designs/value-slot-8b.md`: this evaluation, the measured
   numbers, the two-slot rules, and the accessor contract.
3. Gate: go ahead if the measured slot-typed capacity (fields arena, lists,
   maps, frames) is more than one arena step (4 KB) on any app or board. The
   enviro reservation alone clears this. If nothing does, close the item the
   way S5 was closed and stop.

### Stage 1: `Slot` type, frames, and method calls (`jvm/`)

`types.rs` (the enum, helpers, size assert), `frame.rs` (storage, accessors,
`from_slots`, slot-exact push and pop), `ops_stack.rs` (spec-exact
shuffles), `helpers.rs` (`count_arg_slots`, width helper), `ops_invoke.rs`
(slot marshalling, native decode through `ParamKinds`, lambda captures),
`gc/mod.rs` (`push_ref` on `Slot`). Stages 1 and 2 cannot compile on their
own, since `fields_slice` feeds the same `push_ref`. Land them as one commit
series with separate commits for review.

### Stage 2: object fields and side tables (`jvm/src/object_heap/`)

`mod.rs` (arena, get and set with widths, `alloc_with_defaults`,
`default_field_count_for_native`, asserts, census), `list_store.rs`,
`map_store.rs`, `LambdaProxy`, and the width in `helpers::field_slot_in`.

### Stage 3: `picodroid-core` slot tables plus the test

Renumber the five hand tables and their `alloc_with_field_count` widths. Add
the class-file-driven test. Fix the widths in `boxed.rs`.

### Stage 4: docs, tunables, ratchet

Update `parity-audit.md` (OBJ-04 and the V1 table), the sentinel text in
`memory-diagnostics.md`, the units in `jvm-tunables.md`, the chunk comment
in `object_heap`, the comment at `gc/mod.rs:115`, and point the
`perf-campaign-2026-08.md` "what is left" bullet at the design doc. Record
the benchmark deltas in the design doc.

### Not in scope

- Typed `pop_long` and `push_long` fast paths in `ops_math` and
  `ops_convert`. Do these only if the long and double sections regress more
  than the design doc is willing to carry.
- 4-byte untagged slots with a separate reference map. That is a second
  cliff: it touches every opcode and native, and needs the stack maps the
  device build strips.
- Shrinking `StaticEntry`.

## How to verify

1. `./scripts/test.sh`: the `jvm/` unit tests (frame, GC arena-step
   arithmetic, field tests including
   `alloc_without_defaults_still_leaves_slots_unset`), plus the new
   slot-constant test in `picodroid-core`.
2. The sim smoke test from CLAUDE.md: `./scripts/sim.sh --app helloworld`,
   `--app benchmark`, and `perl -e 'alarm 5; exec @ARGV' ./scripts/sim.sh --app blinky`.
   Then the long and double heavy paths: the `benchmark` sections
   `long_arithmetic` and `double_arithmetic`, `perfbench`, `picoenvmon` on
   `pico_enviro_mon_w` (its `FileInputStream.pos`, `SensorEvent.timestamp`
   and PWM doubles exercise every renumbered table), and `threaddemo`
   (parked frames).
3. Re-run the `--mem-diag` census on the Stage 0 set and put a before and
   after table in the design doc. The growth sentinel must stay quiet.
4. `./scripts/pre-commit --full`. The size ratchet reports the RP2040 flash
   delta. The `handle-table-32` and `mem-diag` legs run.
5. Hardware: `benchmark` sections on `testbench_rp2350`, and a picoenvmon
   navigate-and-serve soak on an enviro board. A torn long read across tasks
   is the one hazard the sim cannot show.
