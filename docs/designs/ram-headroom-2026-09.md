# Spending the RAM the class-link work freed (handover, 2026-09-29)

Status: **proposal, nothing built.** Written for whoever picks this up
after the class-link work (`f5d93245`, docs/designs/class-link-2026-09.md).
It says what the headroom is, where it can go, what each option is likely
worth, how to measure it, and the traps. The recommendation is at the end.

## 1. What is free now, and where

The parsed class metadata (`Parsed`, ~76 KB on the claudeusage History
page, the largest heap consumer) is gone: class metadata is a pack-time
link table read in place from flash, and a registered class costs 16 B in
the class table. Nothing else about the programs changed — same objects,
same allocation and GC counts.

| where | before | after | Δ |
|---|---|---|---|
| JVM arena in use, claudeusage History page (sim census `nused`) | 266,200 B | 185,112 B | **−81,088** |
| largest free block in the arena | 108,048 B | 215,736 B | ×2 |
| device free heap, boot page (`pdb sysmon`, testbench_rp2350, mem-diag) | 115,824 B | 168,192 B | **+52,368** |
| device min-free-heap watermark | 111,376 B | 158,632 B | +47,256 |
| static RAM (`DATA+BSS`), rp2350 / rp2040 | 414,336 / 246,692 | 414,336 / 246,684 | ±0 |

Two things follow from *where* the gain is:

- **It is arena headroom, not static RAM.** The freed bytes were heap
  allocations inside the FreeRTOS heap the device links
  (`heap_kb − jvm_loop_ram_kb − hot_ram_kb`: 408 − 36 − 48 = 324 KB on the
  RP2350 boards). Anything that "spends" it has to come out of that arena
  too — `hot_ram_kb` and `jvm_loop_ram_kb` do exactly that by design
  (`crates/build_support/hot_ram.rs`, `platforms/rp/mcus/rp/rp2350.toml`).
- **The app already benefits without a change.** The busiest page has twice
  the largest free block and the GC has more room before it is forced
  (the benchmark's alloc-failure retries went from 27 to 26 at the device
  heap size — `retry_after_gc` is a full collection each). Whatever is spent
  should leave a margin: the History page needs ~243 KB of arena on the
  display board (its `hot_ram_kb` comment), and the sim charges an app
  60–80 KB more than the board for the same screens (gaps roadmap G11), so
  size decisions are made on the board.

The 52 KB device figure, not the 81 KB sim figure, is the budget to reason
with: the sim models `Parsed` at host pointer widths, the board had the
32-bit version plus fragmentation, and `pdb sysmon` is what the board says.

## 2. Options

Ranked by expected value per KB. Each is a knob or list that already exists.

### 2a. More hot code in SRAM (`hot_ram_kb`) — but the obvious functions are already there

The RP2350 executes from flash through a 16 KB XIP cache; the SRAM hot-path
work (docs/designs/sram-hotpath-2026-09.md) put 48 KB of the page-turn
working set into `.data` and took a third of the CPU per page turn off.
This is the lever the profile *seems* to point at — and the first thing to
know is that it is mostly spent:

- The class-link profile of graphicsbench (class-link doc, "Where the CPU
  goes") has LVGL's style lookup at the top (`get_prop_core` 12 %,
  `get_selector_style_prop` 5.4 %, `lv_obj_get_style_prop_internal` 3 %,
  `lv_style_prop_get_default` 2.6 %), then `lv_event_send` 7.3 %, the RGB565
  blend 3.1 %, `lv_array_at` 2.6 %, fonts, `lv_memset`. **Every one of those
  is already in `platforms/rp/mcus/rp/hot-ram-lvgl.txt`** and executes from
  RAM (the `__Thumbv7ABSLongThunk_lv_obj_get_style_prop_internal` entry in
  the profile is the long-branch thunk into `.data`). Their share is their
  cost *from RAM*.
- The sram-hotpath measurements show the knee: a 12 KB LVGL set captured
  45 % of the baseline's busy samples, 24 KB 51 %, 39 KB 54 %. The next
  24 KB bought three points.

So raising `hot_ram_kb` buys little unless the list changes. What is *not*
in the lists and shows in the profile, with rough shares of busy CPU on the
graphics bench:

| candidate | share | note |
|---|---|---|
| `compiler_builtins` `memset`/`memcpy`/`memcmp` (`__aeabi_memcpy4/8`) | ~2.5 % | cannot be tagged; needs strong symbols in the platform crate under `link_section = ".data"` with `#![no_builtins]` (sram-hotpath §6.4), ~1.1 KB |
| `u64_div_rem` in the clock path | 1.7 % | placement helps a little; a 32-bit millisecond derivation removes it (sram-hotpath §6.5) |
| `class_link::layout::Linked::cp_name_and_type` / `cp_member_ref` / `cp_class_name`, `core::str::from_utf8` | 3.3 % together | the native path's name decode — new code, not yet in the `classfile` hot group; ~1.5 KB of `.data` |
| `ObjectHeap::alloc_with_defaults`, `StringTable::intern_dyn_owned` / `intern`, `resolve_ldc`, `op_new` | ~2 % | allocation path; small functions |
| `PicodroidNativeHandler::dispatch_module` / `dispatch`, `BuiltinHandler::dispatch` | 1.9 % | the module walk; `dispatch_module` is a `match` over 14 modules |
| `lv_text_get_next_line`, `lv_label_event`, `lv_obj_event`, `lv_area_is_in`, `lv_obj_get_scrollbar_area` | ~3 % | label/event functions just below the current list's cut |

Realistic yield: another 8–12 KB of `.data` for perhaps 5–8 % of the busy
CPU on a graphics-heavy screen — worth having, not transformative. The
measurement is the sram-hotpath recipe: `parity-metrics` spans of a
claudeusage page-turn lap, then the DWT PC sampler (Appendix B there; the
class-link doc has the working recipe and traps — the probe selector is
`2e8a:000c:<serial>`, reset with `reset_and_halt` + `DEMCR.TRCENA` + `run`,
sample in windows and keep the ones where `Executor::run` appears).

Trap: the W boards' core-0 main stack is short (their radio statics), and
the linker asserts an 8,192 B floor (`flash_layout.rs::MAIN_STACK_FLOOR_BYTES`);
`.data` growth is what previously tripped it (the 2026-09-17 touch-kit
fault). Check `_stack_start - _stack_end` on the linked ELF of every RP2350
board, not just the one measured.

### 2b. Cut the work rather than move it (the algorithmic levers)

Same profile, bigger prizes, no RAM needed — listed here because they are
what the CPU profile actually says and they compete for the same attention:

1. **LVGL style lookups (~23 % of the graphics bench's busy CPU).** The
   number of lookups, not their fetch cost. `LV_OBJ_STYLE_CACHE`, or fewer
   local styles per widget in `graphics/lvgl/widgets/*` (every
   `lv_obj_set_style_*` on an object adds a local style the lookup chain
   walks). Measure with the profile before and after; the style cache costs
   RAM per object (this *is* a way to spend the headroom — see 2c).
2. **The page-turn fade** renders the whole screen eight times
   (sram-hotpath §6.1); halving the steps or fading only the changed region
   is worth more than any placement.
3. **The native path's name decode (3.3 %)**: the handler API takes `&str`,
   so a native target decodes three names per call. A hash-keyed handler
   API is the clean fix (class-link doc, follow-ups) — a larger change.

### 2c. LVGL's own pool (`lv_mem_kb`)

48 KB on every board, cut from lv_conf.h's 64 KB default on the W boards
because their radio statics left the core-0 stack short (see 2a's trap;
`testbench_rp2350w/board.toml`). A full pool is a silent `while(1)` spin
inside LVGL (memory note `lvgl_pool_oom_spin`), which is the nastiest
failure mode in the tree. Two uses for the headroom here:

- restore 64 KB where the stack floor allows it — measure the linked stack
  headroom per board first; on the W boards the 48 KB was a *stack* decision,
  not a heap one, so this needs the `.data`/stack arithmetic redone, and
  `lv_mem_kb` is `.bss`, not arena, so it competes with the arena for the
  same SRAM; or
- keep 48 KB and turn on `LV_OBJ_STYLE_CACHE` (2b.1), whose per-object cost
  lands in this pool — then the pool may need the room anyway.

Either way the gate is the pool's high-water mark on the busiest screens
(`lv_mem_monitor` — the `lv=19960/72952` field in the sim's memmon snapshot
is used/total), not a guess.

### 2d. The RP2040 (160 KB heap)

Same mechanism, same benefit: the arena has ~50 KB more room for
picoenvmon-class apps, which were the boards that used to OOM
(`project_picoenvmon_heap_budget`). There is no `hot_ram_kb` on the RP2040
(its flash is XIP'd the same way but the program region, now 1152K after
`app_region_kb = 768`, has 215 KB free and RAM is the scarce resource), so
the choice there is between leaving the headroom to apps and reclaiming some
of the 48 KB `lv_mem` for the arena. Recommendation: leave it to the apps
until an RP2040 app shows a need; the RP2040 has never been fast, it has
been tight.

### 2e. Leave it as headroom

The multi-app work installs third-party apps whose working sets cannot be
budgeted in advance, and the History page's 243 KB is the largest known
screen today, not the largest possible one. Keeping ~30 KB of the 52 KB
unallocated is the conservative floor whatever else is done.

## 3. Recommendation

1. **Do the algorithmic LVGL work first (2b.1, 2b.2)** — it is the biggest
   number in the profile and needs no RAM; the style cache, if adopted, is
   the one legitimate way to spend arena on rendering speed, and its cost is
   measurable per object.
2. **Then re-profile and extend the hot lists** with what is left above the
   cut (2a's table) — 8–12 KB, `hot_ram_kb` 48 → 60 on the RP2350 boards,
   verifying the stack floor on the W boards.
3. **Keep at least 30 KB** of the device's 52 KB unallocated (2e).
4. Leave `lv_mem_kb` and the RP2040 alone until a measurement asks
   (2c, 2d).

## 4. How to measure (all recipes exist)

- **Heap on the board:** mem-diag release + shrink build of claudeusage,
  `pdb sysmon` on the boot page and after three `input keyevent` presses —
  free heap, min free, JVM live, post-GC floor, largest free
  (docs/memory-diagnostics.md; the class-link doc's memory table has the
  current numbers to compare against).
- **Heap in the sim:** `--mem-diag` with the control FIFO, `heapcensus`
  after three `tap B` — the census, the snapshot (`nused`, `nfree`, largest
  block, `lv=used/total`) and the per-class table.
- **CPU:** parity-metrics spans of a page-turn lap (`invoke_us`, `native_us`,
  `resolve_us` per span) plus the DWT PC sampler for attribution; the
  before-column for a graphics workload is in the class-link doc.
- **Static RAM and stack:** the size lane (`parity-bench.sh --size-only`) for
  `DATA+BSS`; `_stack_start - _stack_end` from the linked ELF for the stack
  headroom on each W board.
- **Regression gate for whatever ships:** the 3 AM sim nightly and the 4 AM
  HIL fleet run every `qa_*` app and the diagnostics soaks; the memory
  ledger (`docs/memory-diagnostics.md`) is where a new per-object cost such
  as the style cache must be attributed.

## 5. Not to do

- Do not raise `hot_ram_kb` without changing the lists; the current 48 KB
  already holds every function the profile ranks highly, and the sram
  measurements show the next 15 KB of the same list bought three points.
- Do not spend the sim's 81 KB; the board has 52.
- Do not touch `lv_mem_kb` on the W boards without redoing the stack-floor
  arithmetic that set it to 48 KB.
- Do not re-measure the interpreter's dispatch path looking for more here:
  after the class-link work it is ~4 % of a graphics workload's CPU in
  total (`op_invoke` 0.8, native finalize/dispatch 0.8, resolve cache 0.4,
  `find_class` 0.2, `name_eq` 0.3, graphics module dispatch 0.4, the handler
  walk 1.9), and the one item left with a visible share — the native path's
  name decode — needs the API change in 2b.3, not RAM.
