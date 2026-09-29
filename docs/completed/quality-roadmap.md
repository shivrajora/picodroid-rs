# Completed: Quality Roadmap

Items closed out of [quality-roadmap.md](../quality-roadmap.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

## Host-dev velocity

### Thread support in sim — DONE 2026-07-28

Resolved, and not by the std::thread route sketched here: the simulator now compiles the real
FreeRTOS kernel (POSIX port) and runs `Thread.start()` as a real task
(`docs/designs/freertos-host-sim.md`, parity-audit M7/THR-01). The tradeoff this entry worried
about — host threads being truly concurrent where the device is cooperative — is answered by
construction, since the scheduler *is* the device's and runs one task at a time. threaddemo's
conf row now asserts its workers' output rather than just "Starting threads".

What remains hardware-only is core count: the POSIX port is single-core where the chip is
dual-core, so genuinely parallel races still need a board.

## Memory footprint

### `Value` 16 B → 8 B via a two-slot `Slot` storage type *(landed 2026-09-17)*

Done, with one departure: `long`/`double` are two 4-byte halves (the JVM-spec category-2
layout) and the object fields arena, the ArrayList/HashMap buffers and lambda captures hold
an 8-byte `Slot`; the 16-byte `Value` stays the transit type *and the frame type* — `Slot`
frames were built first and cost the release sim benchmark's integer sections ~45 % for
under a kilobyte of frames, so the interpreter's hot loop is untouched. Objects are ~40 %
smaller and the enviro boards' boot-claimed fields arena is 20 KB instead of 40 KB. The
hand-numbered native field-slot tables are now checked against the class files by
`native_field_tables_tests` in `picodroid-core`, and the GC's mark stack and compaction
scratch grow fallibly (the smaller objects let a full heap reach a growth step the old
infallible push aborted on). Measurements and what differed from the design:
`designs/value-slot-8b.md`, "As built".

### Exception side tables grow fallibly *(landed 2026-09-18)*

`exception_messages`, `suppressed` and `exception_causes` in `crates/jvm/src/object_heap/mod.rs`
were the last `ObjectHeap` tables keyed by object index that grew with a plain `Vec::push`.
That is the shape that reset the RP2040 (10 KB, resolution caches) and the touch kit (7,680 B,
the lambda registry) in the 2026-09-13 QA round. Now `register_exception_message`,
`register_exception_cause` and `add_suppressed` grow through `reserve_fallible` and return
`Exhausted`. `add_suppressed` does this for both the table and each owner's own list. The
callers are on the throw path, so a refusal does not become an `OutOfMemoryError` that would
replace the exception being thrown. Instead the entry is dropped and the original Throwable is
thrown without its message, cause or suppressed entry. The `<clinit>` wrap is the one
exception: an `ExceptionInInitializerError` whose cause cannot be recorded would lose the
original, so it counts as a failed wrap and the original is delivered unwrapped, as when the
wrapper cannot be allocated. `growth_tests` pins each table with a `with_budget` test.
`object_heap_tables_reserve_before_they_grow` in `native_handler/alloc_scan.rs` now fails
any `self.<table>` growth under `crates/jvm/src/object_heap/` that its function did not
reserve first. **Tradeoff:** a degraded Throwable loses its message or cause exactly when the
app is out of memory, which is when a developer most wants it, but that beats a reset.

The scan covers only `object_heap/`. Other long-lived JVM tables still push infallibly, but
they grow per class or per nesting level, not per object: `ClassObjectCache::entries`, and
`GcState`'s `parked_frames` and `shadow_roots` (`StaticFieldStore` grows fallibly since M8,
2026-09-26: `prepare` and `mark_initialized` refuse instead of pushing). The two synthesized exception messages (the `<clinit>` wrapper's, and
`Enum.valueOf`'s "No enum constant") are built in a `Vec::with_capacity` on the same throw
path.

## Memory-diagnostics follow-ups

### `pdb sysmon` shows no task table on the W board (task cap 12) — DONE 2026-09-16

`pdb-protocol::sysmon::MAX_TASKS` was 12 and the device hands an array of that size to
`uxTaskGetSystemState` (`platforms/rp/src/pdb/platform.rs`). FreeRTOS returns **zero** entries
when the array is smaller than the task count, so on `pico_enviro_mon_w` (14 tasks: cyw43,
IP-task and the app's network thread on top of the testbench's 10) the table was silently
empty — the "beyond this the table is truncated" comment was wrong. Found 2026-09-07 while
measuring the background-pool stack for `fix/dashboard-stall`. Resolved by raising
`MAX_TASKS` to 24 (the wire is self-describing — the header carries the count and the host
sizes its read from the frame length — so no protocol-version bump): +336 B in the previous
sample kept for CPU rates, +480 B of locals on the debug-bridge task's stack. Both sysmon
sources now `warn` when the live task count exceeds the cap, so the next board to outgrow
it says so instead of printing nothing.

## Long-term stability

### GC root registration that can't be forgotten — DONE 2026-07-26

Replace "remember to edit `gc_visit_roots` when adding a native listener map" with a central
root-provider registry: each native-side map/singleton holding JVM refs registers a visitor at
construction; `gc_visit_roots` iterates the registry. GC-rooting misses are the most frequent
serious bug class in the history (a59dc53 Display singleton, d3e052d VIEW_KEY_MAP, b9194cb
touch/swipe/click/dialog maps). **Tradeoff:** fixed-capacity registry boilerplate in no_std, a
small GC-walk overhead, and the registry itself is new unsafe-adjacent machinery — pair with
the GC-stress nightly mode as the detection net while it lands.

Delivered as audit P2-17 (`23fa075`), pulled forward as a shared-core-extraction enabler:
native maps/singletons register root providers, `gc_visit_roots` iterates the registry, and
both crates carry a source-scanning completeness guard so an unregistered JVM-ref-holding
module fails the tests rather than silently losing roots.

### Extend the LVGL header-parse drift guard — DONE 2026-07-25

Landed (audit P1-7): guards now cover `LV_KEY_*`, `LV_STATE_*`, `LV_PART_*`,
`LV_OBJ_FLAG_*`, `LV_COLOR_FORMAT_*`, `LV_DIR_*`, `LV_FLEX_*`,
`LV_IMAGE_ALIGN_*` (implicit-ordinal, underscore-member aware),
`LV_BUTTONMATRIX_*`, and the `#define` constants (`LV_IMAGE_HEADER_MAGIC`,
`LV_RADIUS_CIRCLE`, `LV_BUTTONMATRIX_BUTTON_NONE`), plus the previously
unguarded `LV_EVENT_FOCUSED/DEFOCUSED/DELETE` rows and a mirrored RGB565
guard in papk-pack (which bakes that byte into every image asset).
Deliberate exemptions (alias/composite values and trivially-stable one-off
families) are documented in the tests-module comment in
`picodroid-core/src/lvgl_ffi.rs`. Note: the original list here named
`LV_ALIGN_*`, but no such Rust constants exist — nothing to guard.

### ~~`IO_IRQ_BANK0` runs on both cores and services the button queue from core 1~~ — FIXED 2026-09-02

Found 2026-08-31 by the THR-04 / X1 trace — the one genuine cross-core race it turned up, and it is
outside the JVM. On `pico_enviro_mon_w` the vector is unmasked twice: on core 0 for the buttons
(`hal/rp/gpio.rs` `init_gpio_irq`, PROC0 routing) and on core 1 for the cyw43 host-wake line
(`gpio::hostwake::init`, PROC1 routing, called from the cyw43 task). Both cores share one RAM vector
table, and the handler body is not core-aware: after the host-wake block it unconditionally reads
`proc0_ints`, calls `enqueue_gpio_event` and clears `INTR`. So a host-wake interrupt on core 1 — one
per received frame — also services core 0's button path, and `enqueue_gpio_event`'s read-modify-write
of `GPIO_QUEUE` / `GPIO_QUEUE_HEAD` / `GPIO_DROPPED` (plain `static mut`s) races core 0's own ISR, the
UI task's `drain_gpio_event` and the PDB task's `inject`. Symmetrically, core 0's handler executes the
host-wake block and RMWs `proc1_inte`, racing `picodroid_cyw43_hostwake_rearm` on core 1 — a lost
re-arm degrades cyw43 RX to the 1000 ms poll fallback.

Fix: branch on `sio_hw->cpuid` at the top of the handler — core 1 runs only the host-wake block,
core 0 only the button loop (each core's `procN_ints` is already the right register for it). A few
lines, but HIL-only to validate, so it was not folded into X1's change: it needs a `pico_enviro_mon_w`
on the probe with button presses during traffic. **Tradeoff:** until then the race is a duplicated or
lost button event coincident with a received frame, and a rare host-wake re-arm loss; neither reaches
the JVM heap.

*2026-09-02:* fixed as prescribed — `IO_IRQ_BANK0` reads SIO CPUID first (`core_num()` in
`hal/rp/gpio.rs`): core 1 runs only the host-wake block and returns, core 0 only the button loop.
Bench evidence on `pico_enviro_mon_w` (`picoenvmon`, 1 Hz dashboard GETs from the host, 20 min per
run). Because `pdb input keyevent` feeds the queue directly and never enters the ISR, presses came
from a temporary GP15 output-enable toggler (drives the pad low exactly like button Y; ~3.3
presses/s through the real core-0 ISR), and temporary counters in the handler were reported from
`drain_gpio_event`:

| firmware | core 1 entered button loop | core 1 saw a pending button IRQ | core 0 saw host-wake bit | presses → down / up dispatched | HTTP ≥ 1 s / samples |
|---|---|---|---|---|---|
| main, unfixed | 28 595 | 5 (serviced from core 1) | 0 | 3088 → 3088 / 3087 | 9 / 852 |
| fixed | 0 | 6 (observed, not serviced) | 1 | 3064 → 3065 / 3064 | 14 / 908 |

The unfixed run's five collisions happened to lose nothing in this window — the race is real
but narrow — and the fixed handler never crossed cores; the one core-0 entry that found the
host-wake bit set left `PROC1_INTE` alone instead of masking it. The ≥ 1 s HTTP samples have the
same shape on both runs — 8 s curl timeouts mid-body (8 vs 11) plus one or two samples in the
1–1.5 s band — so nothing in the host-wake path moved, but the timeouts themselves are not
attributed here.

Two smaller residues from the same trace, both narrow, both recorded rather than fixed: `RESETS.RESET`
is RMW'd non-atomically from core 1 (cyw43 init, `pio_spi.rs`) and core 0 (`ensure_io_unreset` on any
Java `Gpio` call, `gpio.rs` / `dma.rs`) — RP2350's atomic-alias addresses would close it. (The
FreeRTOS+TCP `IP-task`, once absent from the boot-budget model, is charged since M9,
2026-09-26, with its queues and a per-socket charge at connect — parity-audit M9.)
