# Beyond the interpreter loop: what else earns a place in SRAM

> Measured 2026-09-26 on `pico_display2_w` (RP2350A, 150 MHz, 16 KB XIP cache,
> flash clock divider 2, `Executor::run` already in `.data` since `2351d02e`),
> tree = main `69059b03` plus the measurement lever described in §3, on the
> worktree branch `sram-hotpath`. Debug profile (`opt-level = 3`, no LTO — the
> profile `flash.sh` builds), `parity-metrics` counters compiled in, the
> `claudeusage` app against the live bridge. Every number below is from the
> board; nothing here is estimated.
>
> **Status: option D (everything, §0) landed 2026-09-26 for every RP2350 board** —
> `hot_ram_kb = 48` in each board.toml (the landed images grow `.data`
> by 34.8 KB in debug and 37.5 KB under the release profile's fat LTO; the arena is
> 372 → 324 KB), the `hot-in-ram` feature the chip feature forwards, the lists in
> `platforms/rp/mcus/rp/hot-ram-{lvgl,freertos}.txt` (`build_support::hot_ram`, §7).
> The touch kit's arena goes 300 → 252 KB; see the amendment at the end for what picoclock
> measured there. The follow-ups placement cannot address are P1–P5 in
> `claudeusage-gaps-roadmap-2026-09.md`.

## 0. Summary

The interpreter loop was the right first move, but it is 2.4 % of the busy
samples on a page turn. Three more groups of code are worth their RAM, and
together they remove about a third of the CPU work of turning a page:

| variant | in RAM | RAM cost | CPU work per press (profile) | Java spans per page turn | render per lap |
|---|---|---:|---:|---:|---:|
| baseline | `Executor::run` only | — | 1.00 | 205 ms | 9.55 s |
| V1 | + 101 hot LVGL functions (style lookup, event send, RGB565 blend, font, TLSF) | +13.3 KB | 0.78 | 198 ms | 8.08 s |
| V2 | + 30 hot FreeRTOS functions (critical sections, tick, context switch, queue, heap_4) | +4.8 KB | 0.88 | 190 ms | 9.33 s |
| V3 | + 20 JVM helpers around the loop (invoke, frame, field, constant-pool, class lookup) | +19.6 KB | 0.82 | 135 ms | 9.52 s |
| V4 | V1 + V2 + V3 | +37.7 KB | 0.68 | 118 ms | 8.79 s |
| V5 | LVGL set widened to 129 functions | +26.4 KB | 0.77 | 185 ms | 8.26 s |
| V6 | V3 without its five bulky helpers (the lean JVM set) | +14.2 KB | 0.84 | 145 ms | 9.60 s |

"CPU work per press" is the count of non-idle PC samples over a 60 s run of
one press every 0.6 s, relative to the baseline (§2 explains the sampler);
it is the most robust column, because it counts every cycle the main task
spends whatever the page. "Java spans" is the sum of the timed main-loop
spans of one page visit (the `slow handler` report at a 3 ms threshold, §2),
the number the 50 ms budget is judged on. "Render" is the LVGL tick's
render-plus-flush time summed over a 24-press lap; it swings ±5 % between
laps of the same image, so read V1/V4/V5 as "the fade got 8–15 % cheaper",
not as three distinct results.

V5 and V6 locate the knees. Doubling the LVGL budget from 12 to 24 KB
(V1 → V5) buys one more percent of the work: the LVGL hot set is 12 KB. The
lean JVM set (V6, 14.2 KB) keeps three quarters of V3's Java-span gain
(−30 % against −34 %) for 5.4 KB less; the five bulky helpers it leaves in
flash (`dispatch_native_inner` and friends, 5.4 KB) are worth about 4 % of a
page turn's Java time.

Recommendation, by RAM budget:

- **5 KB to spend:** V2, the FreeRTOS set. 4.8 KB buys 12 % of the work and
  it is the same on every board and every app: every `lv_malloc`, every
  queue post, every tick goes through `vPortRecursiveLock`,
  `vTaskEnterCritical` and `xTaskIncrementTick`, all fetched from flash today.
- **15 KB:** V6 or V1, depending on the app. A page-build-heavy app (many
  invokes per tick) wants the JVM helpers; a render-heavy one (scrolling,
  fades, big repaints) wants the LVGL set. On this app the JVM set is the
  bigger single win for the 50 ms budget (Java spans −34 %), the LVGL set the
  bigger win for total CPU (−22 %).
- **35–40 KB:** V4, everything. A third of the work gone, Java spans −43 %,
  two thirds of the busy cycles executing from SRAM. On the display board the
  arena is 372 KB and the History page needs 243 KB, so the 38 KB fits with
  90 KB to spare; on the RP2350B touch kit (300 KB arena, picoclock peak
  276 KB with the radio up) it does not on those numbers, and that board should take V2 plus
  one of the 15 KB sets. (Decision 2026-09-26: every RP2350 board opts in, the touch kit
  included; what picoclock measured there afterwards is in the amendment at the end.)

Two things placement cannot fix are also visible in the profile and are
listed in §6: the fade animation itself (the RGB565 blend is 17 % of what is
left after V4, now executing from RAM at full speed — it is compute, not
fetch) and the name-based lookups the JVM still does per native call
(`from_utf8`, `find_class`, `memcmp`).

## 1. Where the time goes today

Sampled with the DWT PC sample register (§2) during page turns, 179,622
samples in 60 s, of which 75.7 % were busy (not in `wfi`). Shares of the
busy samples, grouped, with how many distinct functions the group touched and
their total size in the image:

| group | share of busy | functions touched | bytes of code touched |
|---|---:|---:|---:|
| LVGL object, style and event code (`get_prop_core`, `lv_event_send`, `get_selector_style_prop`, `lv_obj_draw`, …) | 31.3 % | 280 | 32 KB |
| LVGL draw and blend (`lv_draw_sw_blend_color_to_rgb565_swapped` alone 9.6 %, fonts, `lv_memset`) | 23.8 % | 97 | 20 KB |
| FreeRTOS kernel and port (`vPortRecursiveLock`, tick, critical sections, context switch, queues, heap_4) | 9.1 % | 79 | 7 KB |
| JVM invoke path (`op_invoke`, `finalize_invoke`, `dispatch_native_inner`, `Frame::new_in`, `count_args`, …) | 7.2 % | 28 | 24 KB |
| JVM class-file reads (`find_class`, `cp_utf8`, `cp_fieldref`, `cp_methodref`, `from_utf8`) | 6.3 % | 19 | 5 KB |
| compiler builtins (`memcmp`, `memset`, `memcpy`, 64-bit division) | 4.8 % | 51 | 5 KB |
| JVM field ops (`op_fields`, `field_slot_cached`, `get_field`/`set_field`) | 3.2 % | 6 | 6 KB |
| JVM heap, GC, natives | 2.6 % | 103 | 44 KB |
| `Executor::run` (already in RAM) | 2.4 % | 1 | 29.6 KB |
| native dispatch (`dispatch_module`, the handler's `dispatch`) | 1.3 % | 14 | 28 KB |
| clock reads (`elapsed_realtime_nanos`; partly the counters' own cost) | 1.1 % | 5 | 0.7 KB |
| everything else | 4.9 % | 140 | 26 KB |

97.2 % of the busy samples were fetched from flash. The picture is two
different kinds of hot: LVGL is *wide* (a few hundred small functions, each
warm), the JVM is *narrow* (a handful of medium functions, each very warm).
That is why the LVGL set is chosen by sample density (samples per byte, §3)
and comes out as a hundred functions averaging 120 B, while the JVM set is a
short list of named helpers.

Twenty functions account for half the busy samples:

```
 9.59%  lv_draw_sw_blend_color_to_rgb565_swapped     1434 B
 4.89%  get_prop_core                                 220 B
 2.78%  lv_event_send                                 188 B
 2.43%  pico_jvm::interpreter::Executor<H>::run      (RAM)
 2.29%  get_selector_style_prop                       124 B
 2.08%  ops_invoke::op_invoke                        3280 B
 2.05%  lv_font_get_bitmap_fmt_txt                    536 B
 1.55%  lv_style_prop_get_default                     180 B
 1.51%  ops_fields::op_fields                        2768 B
 1.46%  class_file::find_class                        132 B
 1.43%  lv_obj_draw                                  1196 B
 1.35%  ClassFile::cp_utf8                            188 B
 1.31%  lv_memset                                     108 B
 1.25%  core::str::from_utf8                          552 B
 1.24%  vPortRecursiveLock                             92 B
 1.13%  lv_obj_get_style_prop_internal                 40 B
 0.98%  event_send_core                               256 B
 0.95%  memset                                        248 B
 0.95%  ops_invoke::finalize_invoke                  1810 B
 0.94%  __pd_hal_clock_elapsed_realtime_nanos
```

## 2. How it was measured

**Profile.** A 60-line host tool built on the `probe-rs` crate
(`pcsampler`, Appendix B)
sets `DEMCR.TRCENA` and then reads `DWT_PCSR` (0xE000101C) of core 0 in a
loop over SWD while the target runs — about 3,000 samples a second, no halts,
no instrumentation, no effect on timing. `probe-rs profile pcsr` does the
same but attributes each sample to the innermost *inlined* frame and drops
everything under 0.05 %, which lost 40 % of the samples; the raw histogram is
attributed to the containing symbol with `nm -S` instead, so every sample
lands on something that can be moved. The probe cannot be shared: the RTT
attach of `flash.sh` is stopped (only the `probe-rs` whose environment names
this probe's serial) before a profile.

**Scenario.** `claudeusage` with the live bridge, keys through the `pdb`
host tool (`input keyevent 20`, DPAD_DOWN). Two runs per image: a lap of 24
presses 2.0 s apart (six visits of each of the four pages, each visit's
build steps and first paint complete before the next press), from which the
per-page span totals come; then 60 s of a press every 0.6 s (84 presses),
during which the PC sampler runs.

**Spans.** `parity-metrics` builds print every slow main-loop span with the
JVM's counters; for these runs the threshold was lowered from 50 ms to 3 ms
(a measurement-only edit, reverted) so that every step of a page turn
reports, and the LVGL tick got a probe that prints render-plus-flush time per
second (`render:` lines). The per-page numbers are the sum of all reported
spans between one `page ->` line and the next, averaged over the six visits.

**Image sizes.** `.data` and the main-stack headroom read off the linked
ELF (`__sdata`/`__edata`, `_stack_start`/`_stack_end`), the same way
`print_memory_usage` reads them. On the display board the baseline image
leaves 25.6 KB of stack headroom over the 8 KB floor, so V1 and V2 fit as
is; V3, V4 and V5 were linked with `jvm_loop_ram_kb = 76` in `rp2350.toml`
(arena 332 KB instead of 372) purely to make room — the arena size has no
effect on any timing here.

## 3. The lever: named functions into `.data`, per group

Nothing in the linker script changed. `cortex-m-rt`'s `.data` output section
collects `*(.data .data.*)`, is copied from flash at reset, and is already
executable (the flash routines and `Executor::run` live there), so any input
section named `.data.<anything>` executes from RAM.

- **Rust** (`pico-jvm`, `picodroid-core`): the two build scripts read
  `PICODROID_HOT_RAM="invoke,invoke2,fields,classfile"` and emit one
  `hot_ram_<group>` cfg per token (declared with `rustc-check-cfg`). The
  functions carry
  `#[cfg_attr(hot_ram_invoke, link_section = ".data.hot.invoke")]` and
  `#[cfg_attr(hot_ram_invoke, inline(never))]`, exactly as `run` does under
  `loop-in-ram`. Without the variable the attributes vanish and the image is
  byte-identical to today's.
- **C** (LVGL in `pd-lvgl-sys`, the FreeRTOS kernel in the platform crate):
  both archives are compiled with `-ffunction-sections`, so every function
  is its own `.text.<name>` input section. `build_support::lvgl::hot_ram_rename`
  runs `arm-none-eabi-objcopy --rename-section .text.<name>=.data.hot.<name>`
  over the finished archive for every name in `PICODROID_HOT_RAM_C`. No LVGL
  or FreeRTOS source is touched, and the choice is per function rather than
  per `LV_ATTRIBUTE_FAST_MEM` tag (which would drag in the blend variants of
  every unused colour format). The list is generated from the profile:
  functions in the archive ranked by samples per byte, accumulated up to a
  byte budget (`pick_c.py`). A budget of 12 KB captured 45 % of the baseline's
  busy samples, 24 KB 51 %, 39 KB 54 % — the knee is at about 12 KB.
- **The groups measured**, as tagged in the tree:
  - `invoke` (V6, V3, V4): `op_invoke`, `finalize_invoke`, `count_args`,
    `find_method_cached`, `pop_frame`, `Frame::new_in`, `ResolveCache::method`.
  - `invoke2` (V3, V4 only): `dispatch_native`, `dispatch_native_inner`,
    `find_method_walking_cached`, `find_method`, `field_slot_declared` —
    the bulky ones (5.4 KB together for 1.4 % of busy).
  - `fields`: `op_fields`, `field_slot_cached`, `ObjectHeap::get_field`,
    `ObjectHeap::set_field`.
  - `classfile`: `find_class`, `name_hash`, `name_eq`, `cp_utf8`,
    `cp_fieldref`, `cp_methodref`, `methods`, `method_code`.
  - LVGL 12 KB (V1, V4): 101 functions, headed by the style lookup chain
    (`get_prop_core`, `get_selector_style_prop`, `lv_style_prop_get_default`,
    `lv_obj_get_style_prop_internal`, `lv_style_get_prop_internal`), event
    dispatch (`lv_event_send`, `event_send_core`, `lv_obj_send_event`,
    `lv_obj_event_base`), `lv_draw_sw_blend_color_to_rgb565_swapped`,
    `lv_font_get_bitmap_fmt_txt`, `lv_memset`, `lv_area_intersect`, the TLSF
    allocator's inner functions and `lv_malloc_core`/`lv_free_core`.
  - LVGL 24 KB (V5): the same plus 28 more, mostly the label and rect draw
    setup (`lv_draw_label_iterate_characters`, `lv_obj_init_draw_rect_dsc`,
    `lv_draw_sw_fill`, `lv_draw_sw_blend`, `lv_obj_event`, `lv_label_event`,
    `lv_obj_redraw`, `lv_obj_refr`, `lv_memcpy`, `lv_timer_handler`).
  - FreeRTOS (V2, V4): 30 functions, 4.5 KB — `vPortRecursiveLock`,
    `vTaskEnterCritical`/`ExitCritical` (and the ISR forms),
    `vTaskSuspendAll`/`xTaskResumeAll`, `xTaskIncrementTick`,
    `vTaskSwitchContext`, `isr_pendsv`, `isr_systick`, `xQueueSemaphoreTake`,
    `xQueueGenericSend`, `pvPortMalloc`, `vPortFree`, `prvYieldForTask`,
    `prvCheckForRunStateChange`, the mask set/clear helpers.

## 4. Results in detail

### 4.1 Busy samples per group, thousands per 60 s run (84 presses each)

| group | baseline | V1 LVGL | V2 RTOS | V3 JVM | V4 all | V5 LVGL24 | V6 JVM lean |
|---|---:|---:|---:|---:|---:|---:|---:|
| LVGL object/style/event | 42.5 | 26.1 | 39.1 | 38.3 | 28.2 | 25.7 | 39.0 |
| LVGL draw/blend | 32.3 | 23.4 | 30.9 | 30.4 | 27.9 | 25.2 | 30.8 |
| FreeRTOS | 12.4 | 11.7 | 4.8 | 10.6 | 5.2 | 9.1 | 9.7 |
| JVM invoke | 9.8 | 9.1 | 8.7 | 2.3 | 2.3 | 8.7 | 3.3 |
| JVM fields | 4.3 | 3.9 | 4.1 | 1.2 | 1.0 | 4.0 | 1.5 |
| JVM class-file | 8.6 | 8.2 | 8.3 | 6.0 | 6.0 | 8.1 | 6.1 |
| JVM heap/GC/other | 3.5 | 3.4 | 3.2 | 2.4 | 2.2 | 3.1 | 2.5 |
| `Executor::run` (RAM) | 3.3 | 3.1 | 3.1 | 3.0 | 3.1 | 3.0 | 3.2 |
| builtins/mem | 6.5 | 8.3 | 6.4 | 5.7 | 7.2 | 7.7 | 5.8 |
| native dispatch | 1.8 | 1.6 | 1.6 | 1.6 | 1.2 | 1.6 | 1.5 |
| clock | 1.5 | 1.3 | 1.4 | 1.4 | 1.5 | 1.4 | 1.5 |
| other | 6.7 | 4.4 | 6.0 | 5.9 | 4.2 | 4.6 | 6.3 |
| **total busy** | **135.9** | **106.6** | **120.2** | **111.1** | **92.1** | **104.6** | **113.6** |
| share executing from RAM | 2.8 % | 40 % | 6 % | 10 % | 67 % | 49 % | 10 % |

Each set removes 25–40 % of its own group's samples in the LVGL case and
60–77 % in the FreeRTOS and JVM cases; the JVM helpers are the densest code
on the board (`op_invoke` 3.3 KB carried 2.1 % of all busy samples). The
groups barely interact: V4's totals are within 3 % of what adding the three
single-set deltas predicts. `builtins/mem` goes *up* when neighbours speed
up — `memcmp`/`memset` are called from the moved code and are now a larger
share of a smaller total; they cannot be tagged (they come from
`compiler_builtins`) and are the next thing after V4, see §6.

### 4.2 Java spans per page visit (ms, mean of six visits; `invoke`/`fields`/`native` are the counters' split)

| page | baseline | V1 | V2 | V3 | V4 | V5 | V6 |
|---|---:|---:|---:|---:|---:|---:|---:|
| Limits | 249 (invoke 155, native 73, fields 51) | 222 | 212 | 153 (95 / 60 / 24) | 134 (76 / 44 / 23) | 204 | 162 (100 / 60 / 28) |
| Models | 247 (154 / 71 / 44) | 236 | 234 | 166 (99 / 59 / 24) | 140 (79 / 42 / 21) | 218 | 177 (108 / 61 / 25) |
| Burn rate | 174 (100 / 41 / 36) | 180 | 175 | 121 (64 / 34 / 20) | 106 (51 / 25 / 18) | 171 | 131 (71 / 35 / 23) |
| History | 152 (90 / 37 / 30) | 155 | 140 | 99 (56 / 31 / 16) | 93 (47 / 23 / 16) | 146 | 108 (62 / 32 / 18) |
| **all pages** | **205** | **198** | **190** | **135** | **118** | **185** | **145** |

The bytecode count per visit is the same within 10 % across runs (the page
content depends on the day's data), so the per-visit totals compare
directly. With the JVM helpers in RAM an invoke costs 0.35 ms instead of
0.57 (Limits: 155 ms over ~270 invokes → 95 ms), a field op 0.06 instead of
0.13; the natives' 73 → 60 ms is the LVGL calls they make still fetching
from flash, and V4's 44 ms is with the LVGL set in RAM too. `cpu=` (the
task's FreeRTOS run-time counter) matched the wall-clock spans within 1 %
in every run: nothing else was running.

### 4.3 What the CPU was doing in V4

After all three sets, 67 % of the busy samples execute from SRAM and the top
of the profile is compute, not fetch:

```
16.8%  lv_draw_sw_blend_color_to_rgb565_swapped   (RAM)  — the fade's 8 full-screen sweeps
 7.2%  get_prop_core                              (RAM)  — LVGL style property lookup
 3.4%  Executor::run                              (RAM)
 3.0%  get_selector_style_prop                    (RAM)
 2.8%  lv_event_send                              (RAM)
 2.7%  lv_font_get_bitmap_fmt_txt                 (RAM)
 2.1%  class_file::find_class                     (RAM)
 1.6%  ClassFile::cp_utf8                         (RAM)
 1.4%  __pd_hal_clock_elapsed_realtime_nanos      flash
 1.4%  vPortRecursiveLock                         (RAM)
 1.2%  memcmp                                     flash
 1.1%  core::str::from_utf8                       flash
 1.1%  u64_div_rem                                flash
```

## 5. Caveats

- **Debug profile, counters in.** These are `flash.sh` builds: `opt-level 3`,
  no LTO, `parity-metrics`. The release profile's fat LTO makes the same
  functions about 20 % larger (`run`: 29.6 → 35.6 KB), so budget the RAM
  costs at ×1.2 for release. The counters cost the clock reads and 64-bit
  divisions visible in the profile (`clock` 1.1 %, part of `builtins`);
  they are the same in every variant.
- **Render varies ±5 % between laps** of the same image (fade steps land on
  tick boundaries). The LVGL set's render gain is 8–15 % depending on the
  lap; the profile's group counts (§4.1) are the stable measure and say
  25–40 % of the LVGL samples went away.
- **Same-named statics.** `objcopy` renames every `.text.<name>` in the
  archive, so a static function that several LVGL files define under one
  name (`evaluate`, `dispatch`, `draw_main`) moves in all of them. Two or
  three hundred bytes at most in these lists.
- **`.data` grows the boot copy** by the same bytes (microseconds) and the
  flash image not at all (the code was in flash already; it now also has a
  RAM address).
- **The RP2040** has no room for any of this (264 KB total) and was not
  measured; the RP2350B touch kit was not measured either, but its XIP cache
  and flash clock are the same, so the shares should carry.

## 6. Not fixable by placement, but in the same profile

1. **The fade.** `lv_draw_sw_blend_color_to_rgb565_swapped` is the single
   largest consumer before and after (9.6 % → 16.8 % of what is left), and
   it is executing from RAM in V4. A page turn renders the whole 320×240
   screen eight times for the fade plus one blank sweep; halving the fade
   steps, or fading only the changed region, is worth more than any further
   placement.
2. **Style lookups.** `get_prop_core` and its callers are 12 % of busy
   (7 % in V4, from RAM). LVGL's style cache (`LV_OBJ_STYLE_CACHE`) or
   fewer local styles per object would cut the number of lookups rather
   than their fetch cost.
3. **Name-based lookups per native call.** `find_class` (a hashed linear
   scan over every loaded class), `cp_utf8`, `from_utf8` (UTF-8 validation of
   a constant-pool name, per call, at `names.rs:38` and `native/mod.rs:1180`)
   and `memcmp` are together 5–6 % of busy and stay so in V4. Caching the
   class index or the `&str` per call site, or skipping validation for
   names the parser already checked, would remove them.
4. **`memcmp`/`memset`/`memcpy`** are 2.5 % and live in `compiler_builtins`,
   which cannot be tagged. Providing the three as strong symbols in the
   platform crate under `#[link_section = ".data"]` (with `#![no_builtins]`
   so the loop is not turned back into a `memcpy` call) is the way to move
   them; ~1.1 KB.
5. **64-bit division in the clock path** (`u64_div_rem`, `__udivmoddi4`,
   1–1.3 %): `now_ms()` divides nanoseconds by 10⁶ on every span start and
   end, and `SystemClock` does the same for the app. A microsecond clock
   with a 32-bit millisecond derivation would make it a shift and a multiply.

## 7. How it landed, and reproducing

The measurement lever (env-selected groups) became a per-board switch:

- `board.toml`: `hot_ram_kb = N` — the RAM the hot sets take, subtracted from
  the arena beside `jvm_loop_ram_kb` (`board_cfg::mcu_arena_kb`), and the
  switch for the C side.
- `platforms/rp/Cargo.toml`: the chip feature `chip-rp2350` forwards `hot-in-ram`, which
  enables `pico-jvm/hot-in-ram`; the JVM helpers carry
  `#[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]`.
  `platforms/rp/build.rs` refuses a build where the key and the feature
  disagree, as it does for the loop.
- `platforms/rp/mcus/rp/hot-ram-lvgl.txt` and `hot-ram-freertos.txt`: the
  C lists (Appendix A), applied by `build_support::hot_ram::retarget` to
  `liblvgl.a` (from `pd-lvgl-sys`'s build script) and `libfreertos.a` (from
  the platform's), device targets only.

To re-derive the lists after an LVGL or kernel update: profile with the
sampler below, rank the archive's functions by samples per byte with
`pick_c.py` against `liblvgl.a` / `libfreertos.a` from a build *without* the
switch (a retargeted archive no longer has the `.text.<fn>` names), and stop
at the knee.

```bash
# the sampler (probe must be free: stop the RTT probe-rs first)
pcsampler 2e8a:000c:<probe-serial> 60 raw.txt   # then attr.py / groups.py against the ELF
```

The function lists are in Appendix A and the sampler in Appendix B. The
landed switch (above), the tagged functions and the `render:` probe (under
`parity-metrics` only) are in the tree; the 3 ms threshold used for the runs
was reverted. `pick_c.py`, `attr.py`, `groups.py`, `spans.py`, the raw histograms
and the RTT logs of the seven runs were kept in the session's scratch
directory and are not in the repository.

## Amendment A1 (2026-09-26, later): every RP2350 board opts in

The recommendation in §0 kept the touch kit out on picoclock's last measured
peak (276 KB). The decision was to opt in every RP2350 board, so the touch
kit was measured rather than reasoned about: `pico_touch_kit`, picoclock,
arena 300 → 252 KB, radio up, driven through `pdb input tap`/`swipe` into
the Set-time screen (three scrolls) and the Alarms list and back, twice.

| point | free heap | lowest ever |
|---|---:|---:|
| after boot, clock face | 50.1 KB | 33.9 KB |
| after Set-time (scrolled) | 44.7 KB | 33.9 KB |
| after Alarms | 43.2 KB | 33.9 KB |
| second lap, back on the clock | 41.7 KB | 33.9 KB |

No allocation failure in the RTT log; the lowest-ever figure is the boot
transient. The 276 KB predates the M8–M10 memory work and is withdrawn. The
touch kit's main-stack headroom on this image is 30.7 KB (18 KB before: the
48 KB arena cut is larger than the 35 KB of code that moved). `picoenvmon`
was pre-flighted in the simulator at the RP2350A's new 324 KB arena
(`PICODROID_HEAP_LIMIT_KB=324 sim.sh -b pico_enviro_mon_w -a picoenvmon`):
network, NTP and weather up, four minutes without an allocation failure,
peak 178 KB at app load. `testbench_rp2350` and `pico_enviro_mon_w` link with
45 KB and 40 KB of stack headroom.

## Appendix A. The function lists

LVGL 12 KB (V1, V4), `PICODROID_HOT_RAM_C`:

```
lv_obj_get_style_prop_internal
lv_event_get_param
get_prop_core
get_selector_style_prop
cleanup_event_list
lv_event_send
lv_event_get_code
lv_memset
lv_style_get_prop_internal
lv_array_at
block_link_next
lv_obj_event_base
lv_obj_send_event
lv_style_prop_get_default
lv_area_intersect
lv_draw_sw_blend_color_to_rgb565_swapped
lv_draw_blur_dsc_init
lv_malloc_core
lv_obj_is_event_trickle
lv_display_send_event
lv_obj_is_hidden
lv_indev_active
lv_color_format_get_bpp
lv_tlsf_block_size
lv_area_increase
mapping_insert
lv_tlsf_malloc
block_insert
event_send_core
lv_area_move
lv_draw_dispatch
remove_free_block
lv_free_core
lv_color_to_u16
lv_font_get_bitmap_fmt_txt
adjust_request_size
width_to_stride
block_locate_free
obj_invalidate_area_internal.part.0
block_merge_next
lv_event_push_and_send
lv_malloc_zeroed
block_split
block_prepare_used
lv_free
mask_mix
lv_draw_get_available_task
is_transformed
lv_obj_get_display
lv_obj_get_style_space_left_internal.constprop.0
lv_draw_buf_goto_xy
lv_tlsf_free
lv_obj_get_style_space_right_internal.constprop.0
lv_color_mix
lv_color_make
lv_obj_invalidate
lv_obj_style_apply_recolor
normal_apply_layer_recolor.isra.0
invalidate_area_core
lv_obj_get_style_space_top_internal.constprop.0
lv_draw_dispatch_layer
lv_draw_sw_mask_apply
block_remove
disp_event_cb
lv_obj_area_is_visible
lv_text_encoded_letter_next_2
lv_draw_cleanup_task
lv_draw_finalize_task_creation
lv_obj_get_self_width
lv_font_get_glyph_dsc_fmt_txt
lv_draw_rect_dsc_init
lv_text_is_marker
evaluate
lv_refr_get_top_obj
lv_font_get_glyph_bitmap_internal
lv_area_is_in
lv_draw_unit_draw_letter_internal
lv_draw_sw_get_blend_handler
lv_draw_add_task
_calculate_draw_buf_size
lv_obj_init_draw_blur_dsc
lv_obj_get_style_recolor_recursive
lv_obj_get_scroll_bottom
lv_obj_get_content_coords
lv_obj_draw
lv_realloc_core
lv_palette_main
get_glyph_dsc_id.part.0.isra.0
refr_obj_and_children
lv_draw_buf_reshape
lv_font_get_glyph_dsc_internal
lv_draw_label_dsc_init
lv_inv_area
draw_buf_flush
lv_obj_get_scroll_right
lv_draw_mask_radius
refr_configured_layer
lv_font_get_glyph_width_internal
dispatch
lv_area_is_out
lv_text_utf8_next
```

FreeRTOS (V2, V4):

```
vClearInterruptMask,vPortRecursiveLock,ulSetInterruptMask,vTaskEnterCritical,i
sr_systick,vTaskExitCritical,vTaskExitCriticalFromISR,xTaskGetCurrentTaskHandl
e,vTaskSuspendAll,freertos_rs_take_semaphore,freertos_rs_give_semaphore,vPortF
ree,xTaskIncrementTick,xTaskResumeAll,isr_pendsv,vTaskEnterCriticalFromISR,prv
CheckForRunStateChange,prvYieldForTask,xQueueSemaphoreTake,pvPortMalloc,vTaskS
witchContext,prvCopyDataToQueue,vTaskYieldWithinAPI,xQueueGenericSend,xQueueGi
veFromISR,xTaskCheckForTimeOut,prvUnlockQueue,xTaskRemoveFromEventList,prvAddC
urrentTaskToDelayedList,prvTimerTask
```

LVGL 24 KB (V5) adds:

```
draw_letter_cb,drop_shadow_init,lv_area_get_size,lv_display_is_invalidation_en
abled,lv_draw_buf_create_ex,lv_draw_buf_flush_cache,lv_draw_label,lv_draw_labe
l_iterate_characters,lv_draw_layer_go_to_xy,lv_draw_sw_blend,lv_draw_sw_fill,l
v_draw_sw_mask_radius_init,lv_label_event,lv_label_mark_need_refr_text,lv_laye
r_reset,lv_memcpy,lv_obj_event,lv_obj_get_scroll_left,lv_obj_get_style_space_b
ottom_internal.constprop.0,lv_obj_init_draw_label_dsc,lv_obj_init_draw_rect_ds
c,lv_obj_is_style_any_height_content,lv_obj_redraw,lv_obj_refr,lv_obj_refresh_
style,lv_obj_set_local_style_prop,lv_style_set_prop,lv_text_get_width,lv_timer
_handler,lv_tlsf_realloc,refr_area
```

## Appendix B. The PC sampler

```rust
//! Sample DWT_PCSR of core 0 while the target runs; write "pc count" lines.
use anyhow::Result;
use probe_rs::probe::{list::Lister, DebugProbeSelector};
use probe_rs::{MemoryInterface, Permissions};
use std::collections::HashMap;
use std::io::Write;
use std::time::{Duration, Instant};

const DWT_PCSR: u64 = 0xE000_101C;


fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let selector: DebugProbeSelector = args[1].parse()?;
    let secs: u64 = args[2].parse()?;
    let out = &args[3];
    let lister = Lister::new();
    let probe = lister.open(selector)?;
    let mut session = probe.attach("RP235x", Permissions::default())?;
    let mut core = session.core(0)?;
    // DEMCR.TRCENA: the DWT (and its PCSR) is only accessible with trace enabled.
    const DEMCR: u64 = 0xE000_EDFC;
    let demcr = core.read_word_32(DEMCR)?;
    core.write_word_32(DEMCR, demcr | (1 << 24))?;
    let ctrl = core.read_word_32(0xE000_1000)?;
    eprintln!("DEMCR {:08x} -> {:08x}, DWT_CTRL {:08x} (NOTRCPKT bit27={}, NOPRFCNT bit24={})", demcr, demcr | (1<<24), ctrl, (ctrl>>27)&1, (ctrl>>24)&1);
    let mut hist: HashMap<u32, u32> = HashMap::new();
    let start = Instant::now();
    let mut n = 0u64;
    let mut last_report = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        let pc = core.read_word_32(DWT_PCSR)?;
        *hist.entry(pc).or_insert(0) += 1;
        n += 1;
        if last_report.elapsed() > Duration::from_secs(10) {
            eprintln!("{} samples in {:?}", n, start.elapsed());
            last_report = Instant::now();
        }
    }
    let mut f = std::fs::File::create(out)?;
    let mut v: Vec<_> = hist.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (pc, c) in v {
        writeln!(f, "{pc:08x} {c}")?;
    }
    eprintln!("done: {n} samples in {:?}", start.elapsed());
    Ok(())
}
```

Cargo.toml: `probe-rs = "0.31"`, `anyhow = "1"`. Attribute the histogram with `nm -S --defined-only` on the flashed ELF (bisect on symbol start), and treat `__wfi` as idle.
