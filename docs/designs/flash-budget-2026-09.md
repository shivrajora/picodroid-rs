# Flash budget, September 2026 — after the string work, where the bytes go now

> Measured 2026-09-02 against `86ebdfe` (map v0.16.0) on a `--shrink --release`
> build of `picoenvmon` for `pico_enviro_mon`. Successor to
> [flash-string-budget-2026-08.md](flash-string-budget-2026-08.md), whose §4,
> §5.1 and §5.2 have all landed. Every number came from the linked ELF, the
> embedded corpora, or a rebuilt image, unless a row is marked *(projected)*.
> Reproduction commands and one important trap are in §9.

## 0. TL;DR

The three string changes took the image from **1,009,792 → 943,959 B
(−65,833, −6.5 %)** and cut Java name text from 10.5 % of flash to **6.3 %**.
The remaining name text is mostly things the shrinker cannot touch
(descriptors, `<init>`, `Code`, kept `java/**` member names, app-private
names), so **string work is now a second-order lever**. The image is 77.5 %
ARM code, and that is where the next order of magnitude is:

| Lever | Saving | Status |
|---|---:|---|
| `opt-level = "s"` for the release profile (LVGL/FreeRTOS C follow via cc-rs) | **−224,552 B (−23.8 %)** | measured, §6.1 — needs a HIL benchmark before adoption |
| `opt-level = "z"` | −270,260 B (−28.6 %) | measured, §6.1 |
| C code alone at `-Os`, Rust untouched at 3 | **−81,552 B (−8.6 %)** | measured, §6.1 — no JVM-speed exposure |
| Retire `shrink_class` / `unshrink_class` in favour of `c::` consts | ~19 KB | measured components, §6.2 |
| App-driven tree-shake of the embedded SDK corpus | ~19.8 KB for this app | projected, §6.3 |
| Float parse/format in `core` (`Double.toString` round-trip search + `parseDouble`) | up to ~25 KB | measured components, §6.4 |
| LVGL config (ARGB8888 blend, blur, shadow, transform, unused widgets) | ~20–35 KB | measured components, §6.5 |
| `pdb` + USB CDC as a product-image feature gate | ~14 KB | measured components, §6.6 |
| App PAPK class/member obfuscation | ~9 KB, **outside the program region** | projected, §6.7 |

One bookkeeping correction that changes how the PAPK-side numbers should be
read: **the embedded PAPK does not live in the program region** (§3). The
`Flash:` line sums it in, but the linker places it in `PAPK_FLASH`, so
`picoenvmon`'s real `FLASH` usage is **889,664 B**, and every PAPK-side saving
in the earlier document (and §6.7 here) relieves the 1 MB PAPK slot, not the
896 K rp2040 ceiling.

## 1. The build under measurement

```bash
touch picodroid-core/build.rs   # project_sdk_embed_rerun_gap
./scripts/build.sh --board pico_enviro_mon --app picoenvmon --release --shrink
```

```text
text 943955   data 4   bss 507488
Flash: 943959 / 2883584 bytes (32% of program region; chip total 4194304)
RAM:   507492 / 532480 bytes (95%)
```

Toolchain: stable `rustc` (see `bench/parity/ratchet.toml` for the CI pin),
`profile.release` = `opt-level 3`, fat LTO, `codegen-units 1`,
`debug-assertions off`. Embedded SDK corpus: 145 classes, 79,169 B, from
`target/thumbv8m.main-none-eabihf/release/build/picodroid-core-fb74c225f43778d8/out/framework_classes_shrunk`.
Embedded app: `examples/picoenvmon/build/papk/picoenvmon.papk`, 50,199 B,
35 classes.

## 2. What the August work bought

| | 2026-08 (`1438e87`) | 2026-09 (`86ebdfe`) | Δ |
|---|---:|---:|---:|
| `Flash:` | 1,009,792 | 943,959 | **−65,833** |
| `.text` | 726,096 | 731,184 | +5,088 |
| `.rodata` | 207,452 | 155,740 | −51,712 |
| ├─ embedded SDK corpus | 103,712 (137 cls) | 79,169 (145 cls) | −24,543 |
| └─ Rust literals, tables, consts | 103,740 | 76,571 | −27,169 |
| `.papk_flash_init` | 72,992 | 54,295 | −18,697 |
| Java name text, all sources | ~106,065 (10.5 %) | 59,129 (6.3 %) | **−46,936** |

The `.text` growth is the §5.1 class-file boundary (+1,920) plus the member
`m::` consts and the descriptor-translating compares. The Rust `.rodata` drop
is larger than the string accounting alone predicts because the ASM
`ClassWriter(0)` rewrites also removed the attribute-name and orphaned `Utf8`
entries the earlier §6 had priced separately.

## 3. Section budget

| Section | Bytes | % of `Flash:` | Region |
|---|---:|---:|---|
| `.text` (ARM code) | 731,184 | 77.5 % | `FLASH` |
| `.rodata` | 155,740 | 16.5 % | `FLASH` |
| &nbsp;&nbsp;├─ embedded SDK `.class` corpus (145 files) | 79,169 | 8.4 % | |
| &nbsp;&nbsp;└─ Rust literals, tables, consts | 76,571 | 8.1 % | |
| `.papk_flash_init` | 54,295 | 5.8 % | **`PAPK_FLASH`** |
| &nbsp;&nbsp;├─ boot-meta sector (`papk-format/src/flash_image.rs`, 12 B used) | 4,096 | 0.4 % | |
| &nbsp;&nbsp;└─ `picoenvmon.papk` | 50,199 | 5.3 % | |
| `.data`, `.vector_table`, `.start_block`, `.init_array` | 2,740 | 0.3 % | `FLASH` |

`platforms/rp/mcus/rp/rp2350.x` defines `FLASH` (2816 K at `0x10000000`),
`FS_FLASH` and `PAPK_FLASH` (1 MB at `0x10300000`). `.papk_flash_init` is
placed in `PAPK_FLASH` by `build_support/papk.rs::embed_papk_flash_init`, but
`arm-none-eabi-size` folds every allocated read-only section into `text`, so
`scripts/lib.sh`'s `Flash:` line — and therefore `bench/parity/ratchet.toml`
and the rp2040 flash gate — counts it against the program region. The last
address linked into `FLASH` is `.gnu.sgstubs` at `0x100d9340`, i.e.
**889,664 B** of the 2,883,584 B region. On `testbench_rp2040` the same
correction is only ~4.9 KB (`helloworld.papk` + the meta sector), so the
896 K ceiling numbers stand, but the headroom there is ~48 KB rather than
~43 KB. Worth teaching `lib.sh` to subtract the `PAPK_FLASH` sections so the
gate measures what the linker enforces.

## 4. Java name text still in the image (59,129 B, 6.3 %)

| Source | Bytes | % flash |
|---|---:|---:|
| SDK corpus UTF-8 constant pool | 25,679 | 2.7 % |
| App PAPK UTF-8 constant pool | 22,582 | 2.4 % |
| Rust `.rodata` — JVM class names + descriptors | 10,868 | 1.2 % |
| **Total** | **59,129** | **6.3 %** |

### 4.1 Embedded SDK corpus (79,169 B, 145 classes)

32.4 % of the corpus is constant-pool UTF-8 (was 44.8 %):

| Role | Entries | Distinct | Bytes | % corpus |
|---|---:|---:|---:|---:|
| descriptor | 1,009 | 365 | 10,237 | 12.9 % |
| member-name (`NameAndType`) | 707 | 413 | 4,532 | 5.7 % |
| class-name | 483 | 176 | 3,303 | 4.2 % |
| method-name (`method_info`) | 572 | 456 | 3,190 | 4.0 % |
| string-literal | 103 | 94 | 1,920 | 2.4 % |
| attr-name | 163 | 4 | 1,573 | 2.0 % |
| field-name | 192 | 186 | 924 | 1.2 % |
| **Total** | | | **25,679** | **32.4 %** |

Attribute payloads: `Code` 25,990 (of which 14,706 is bytecode — 57 %, up
from 34 % — because the strip removed the sub-attributes), `ConstantValue`
1,504, `Exceptions` 482, `BootstrapMethods` 54. No debug attributes remain.

What is left is structurally hard to shrink further:

- **Descriptors (10.2 KB)** are already in `a/`/`b/` form; the residue is
  parenthesis/primitive syntax, and `(Lb/AQ;)V`-style strings repeat across
  classes (§6.8).
- **`<init>` ×109 (972 B), `Code` ×111 (770 B), `()V` ×110 (654 B),
  `ConstantValue` ×34 (528 B)** are JVMS-mandated spellings. Together with the
  other kept names (`toString` ×16, `close`, `append`, `currentTimeMillis`,
  the `access$NNN` synthetics) the identifier-like residue is 3,678 B.
- Cross-class duplication of identical `Utf8` text is **12,896 B** — more than
  the distinct text (12,783 B). That is the §6.8 shared-string-table case.

Largest classes, in original names: `SharedPreferences` 4,582,
`HttpURLConnection` 3,976, `View` 3,656, `SharedPreferences$Editor` 3,093,
`Thread` 2,971, `ThreadPoolExecutor` 2,441, `ViewPropertyAnimator` 2,341,
`AlertDialog` 2,271, `AlertDialog$Builder` 1,896, `java/util/Arrays` 1,715.

### 4.2 Embedded app PAPK (`picoenvmon`, 50,199 B, 35 classes)

45.0 % is constant-pool UTF-8:

| Role | Entries | Distinct | Bytes | % PAPK |
|---|---:|---:|---:|---:|
| descriptor | 474 | 191 | 7,522 | 15.0 % |
| member-name | 526 | 299 | 4,921 | 9.8 % |
| class-name | 258 | 92 | 4,844 | 9.6 % |
| string-literal | 183 | 149 | 3,021 | 6.0 % |
| field-name | 67 | 54 | 921 | 1.8 % |
| method-name | 73 | 49 | 803 | 1.6 % |
| attr-name | 52 | 4 | 550 | 1.1 % |
| **Total** | | | **22,582** | **45.0 %** |

Splitting the member names by what the map did to them: 1,483 B are shrunk
SDK targets or `<init>`, 590 B are kept contract names, and **4,572 B (333
entries) are app-private names** the SDK map never sees. App class names
(`picoenvmon/…`) cost 3,792 B as `Class` entries plus 2,790 B inside
descriptors; `Lpicoenvmon/util/Formatter;` alone appears nine times.
Cross-class duplication inside the PAPK is 10,862 B.

### 4.3 Rust `.rodata` literal tables (10,868 B)

| Kind | Occurrences | Distinct | Bytes |
|---|---:|---:|---:|
| framework class names, original form | 238 | 238 | 6,920 |
| framework class names, shrunk (`a/XX`) | 661 | 177 | 2,609 |
| method descriptors | 283 | 68 | 1,306 |
| type descriptors / `java/**` | 2 | 2 | 33 |

The three blobs behind the original-form names are unchanged from August and
are the subject of §6.2:

| Address | Bytes | What |
|---|---:|---|
| `0x100bd97c` | 2,556 | `unshrink_class` return values (generated `framework_unshrink.rs`, 300 arms) |
| `0x100bcd84` | 2,240 | `PICODROID_NATIVE_CLASSES` — still full names |
| `0x100bae34` | 1,337 | `b/` → `java/**` table (`pico_jvm::class_file::names`, +`JAVA_ORIGINALS` 712) |
| `0x100bc798` | 394 | JVM opcode mnemonics (`jvm/src/types.rs`) |

The dispatch method-name blob (1,977 B in August) is gone — that is the `m::`
mechanism working.

## 5. Where the rest of the image is

### 5.1 `.text` by origin (733,000 B accounted by `nm`)

| Bucket | Bytes | % `.text` | Biggest symbols |
|---|---:|---:|---|
| LVGL (C) | 184,150 | 25.1 % | `lv_draw_sw_blend_image_to_argb8888` 9,020; `lv_theme_default_init` 6,648; `lv_draw_sw_blur` 4,868; `…_to_rgb565_swapped` 4,784; `lv_draw_sw_transform` 4,462; `lv_draw_sw_box_shadow` 4,216 |
| `pico_jvm` | 151,906 | 20.7 % | `Executor::run` 32,602; `native::string::dispatch` 10,324; `collect_now` 8,604 |
| `picodroid_core` | 144,012 | 19.6 % | `boot::run_app` 13,798; `NativeMethodHandler::dispatch` 12,520; `dispatch_widget_events` 11,100; `run_activity` 6,078 |
| other / unattributed | 60,429 | 8.2 % | `img_draw_core` 5,244, `flex_update` 2,676, `USBCTRL_IRQ` 2,672, cortex-m-rt, `.Lanon` jump tables |
| littlefs | 31,830 | 4.3 % | `lfs_file_opencfg` 2,698 |
| `core` / `alloc`, non-float | 29,712 | 4.1 % | `FnOnce::call_once{{vtable.shim}}` 3,716; `OnceCell::try_init` 3,162; `StrSearcher::new` 1,594; `slice_error_fail_rt` 1,700 |
| `core` float parse + format | 25,330 | 3.5 % | dragon `format_shortest` 5,336, `format_exact` 4,604; `f64::from_str` 3,438; grisu 4,122 |
| libm | 22,248 | 3.0 % | `rem_pio2` 5,520, `fmod` 2,978, `pow` 2,952 |
| libc / compiler-rt | 17,390 | 2.4 % | `__aeabi_*`, memcpy/memset, soft-float helpers |
| FreeRTOS (C) | 15,749 | 2.1 % | |
| `core::fmt` + `Debug`/`Display` impls | 13,278 | 1.8 % | `<&T as Display>::fmt` 2,556; `Formatter::pad` 1,592; `JvmError as Debug` 1,138 |
| `shrink_names` | 12,128 | 1.7 % | `shrink_class` 7,764; `unshrink_class` 4,364 |
| `pdb` | 11,204 | 1.5 % | `run_pdb_task` 10,830 |
| `picodroid` (platforms/rp) | 10,462 | 1.4 % | `spi::write_raw` 1,910; `start_tasks` 1,404 |
| USB CDC | 2,672 | 0.4 % | |
| defmt | 500 | 0.1 % | |

### 5.2 Rust `.rodata` beyond the corpus (76,571 B)

| What | Bytes |
|---|---:|
| Printable string runs ≥ 4 chars, total | 40,052 |
| ├─ JVM names (§4.3) | 10,868 |
| ├─ log / panic message text (runs containing spaces) | ~10,700 |
| ├─ panic `Location` source paths (121 sites) | 6,354 |
| ├─ enum `Debug` names, littlefs error strings, misc identifiers | ~5,000 |
| └─ date-picker year list (`2026\n2025\n…`) 629 + hour/minute lists 179 | 808 |
| `core::num::dec2flt::table::POWER_OF_FIVE_128` | 10,416 |
| `lv_font_montserrat_14` (glyph bitmap 8,832, kerning 2,989 + 316, glyph dsc 1,264) | 13,617 |
| grisu `CACHED_POW10` | 1,296 |
| unicode tables (`grapheme_extend`, `white_space`) | 1,741 |
| remaining `.Lanon` tables, LVGL const tables, vtables | ~9,000 |

The remap experiment in §6.9 shows the source paths are mostly not
recoverable on stable: only 1,024 B came off.

## 6. Opportunities

Ranked by measured or projected bytes off the **program region**.

### 6.1 Optimisation level — measured, −224 KB to −270 KB

The release profile is `opt-level = 3`. cc-rs mirrors cargo's `OPT_LEVEL`
when it compiles LVGL and FreeRTOS, so the profile flag also selects `-O3`
for the C side (`build_support/lvgl.rs` adds only `-fshort-enums`). Rebuilt
on this tree, same lockfile and toolchain:

| Build | `Flash:` | Δ vs baseline | `.text` |
|---|---:|---:|---:|
| baseline, `opt-level = 3` | 943,959 | | 731,184 |
| `profile.release.opt-level = "s"` | 719,407 | **−224,552 (−23.8 %)** | 506,432 |
| `profile.release.opt-level = "z"` | 673,699 | **−270,260 (−28.6 %)** | 457,384 |
| C only at `-Os` (`CFLAGS_thumbv8m_main_none_eabihf=-Os`, Rust stays 3) | 862,407 | **−81,552 (−8.6 %)** | 650,016 |
| `"s"` everywhere, `[profile.release.package.pico-jvm] opt-level = 3` | 730,899 | −213,060 (−22.6 %) | 517,824 |
| `"s"` everywhere, `pico-jvm` **and** `picodroid-core` at 3 | 837,947 | −106,012 (−11.2 %) | 624,744 |

Where the `"s"` saving comes from, by `.text` bucket:

| Bucket | `3` | `"s"` | Δ |
|---|---:|---:|---:|
| LVGL (C) | 184,150 | 119,130 | −65,020 (−35 %) |
| `picodroid_core` | 144,012 | 88,210 | −55,802 (−39 %) |
| `pico_jvm` | 151,906 | 111,844 | −40,062 (−26 %) |
| other | 60,429 | 49,611 | −10,818 |
| littlefs | 31,830 | 22,136 | −9,694 |
| `core` float | 25,330 | 16,352 | −8,978 |
| `pdb` | 11,204 | 3,694 | −7,510 |
| FreeRTOS (C) | 15,749 | 9,973 | −5,776 |
| `core`/`alloc` other | 29,712 | 24,994 | −4,718 |
| libc / compiler-rt | 17,390 | 13,516 | −3,874 |
| `core::fmt` | 13,278 | 9,558 | −3,720 |
| libm | 22,248 | 18,538 | −3,710 |

This is a size/speed trade and the perf campaign
(`docs/perf-campaign-2026-08.md`, device wall-clock is reproducible to ±4 %)
is the instrument to price it: `hil-run.sh --app benchmark` on both testbench
boards under each variant. Note the existing rp2040 rule (`scripts/lib.sh`:
fat LTO grows that image, so `thumbv6m` links with `lto=false`) — re-measure
that interaction, since `-Os` + LTO may behave differently from `-O3` + LTO.

#### 6.1.1 What the split builds say

- **C at `-Os` alone is −81.5 KB with no Java-speed exposure.** LVGL drops
  184,150 → 119,130 and the LVGL statics in "other" (`img_draw_core`,
  `flex_update`, …) another 9.8 KB; every Rust bucket is byte-identical. It
  costs render throughput, which the UI benchmarks (not the JVM `benchmark`
  app) would have to price. The lever is a `CFLAGS` line in
  `build_support/lvgl.rs` / `freertos.rs`, independent of the Rust profile.
- **Per-package overrides leak through generics.** With `pico-jvm` pinned at
  3 and everything else at `"s"`, the `pico_jvm` bucket still shrinks by
  28,272 B: `Executor<H>::run` and the other `Executor<H>` methods are generic
  over the native handler and are monomorphised in the crate that
  instantiates them (`picodroid_core` / `picodroid`), so they take *that*
  crate's opt-level. Pinning `picodroid-core` too keeps most of the
  interpreter at 3 (and, because `picodroid-core`'s build script compiles
  LVGL, puts the C back at `-O3`) — but `pico_jvm` still loses 23.9 KB and
  `picodroid_core` 29.2 KB through instantiations that land in the leaf
  `picodroid` crate. The honest choices are therefore: the profile-wide
  setting (`"s"`: −224.5 KB, `"z"`: −270.3 KB), or C-only `-Os` (−81.5 KB),
  or profile-wide `"s"` plus `#[inline(never)]`/`optimize`-style pinning of
  the two or three interpreter hot functions once `#[optimize(speed)]`
  stabilises. Anything in between needs to be measured per variant, not
  reasoned about.

### 6.2 Retire runtime class-name translation — ~19 KB

> **Landed 2026-09-02** as [unconditional-shrink-2026-09.md](unconditional-shrink-2026-09.md)
> (map v0.17.0): ProGuard semantics for `--shrink`, no original name anywhere
> in the image, `Class.getName()` returns the mapped name. Measured on this
> build: **943,959 → 916,805 B (−27,154)** — `.text` −17,424, `.rodata`
> −9,460 — more than the ~19 KB priced below because the contract members
> and the JVM's own `java/**` literals went with it. `.rodata` now carries
> zero original `picodroid/**` or `java/**` spellings (§4.3 is empty).

August's #3/#5 priced this at ~4.7 KB of `.rodata`. The `.text` side was not
counted then and is larger:

| Piece | Bytes | Section |
|---|---:|---|
| `picodroid_core::shrink_names::shrink_class` (300-arm `match`) | 7,764 | `.text` |
| `picodroid_core::shrink_names::unshrink_class` | 4,364 | `.text` |
| `unshrink_class` original-name returns | 2,556 | `.rodata` |
| `PICODROID_NATIVE_CLASSES` in full names | 2,240 | `.rodata` |
| `pico_jvm::class_file::names` `b/` table + `JAVA_ORIGINALS` | 1,337 + 712 | `.rodata` |
| **Total** | **~18,970** | |

Both functions are live at runtime: `lifecycle.rs`, `service_lifecycle.rs`,
`display.rs`, `threads.rs`, `net/server_socket.rs` and
`pio/peripheral_manager` call `shrink_class` on every dispatch-site lookup or
native allocation, and every per-domain handler (`graphics/mod.rs:87`,
`io.rs:35`, `os.rs`, `net.rs`, `sensors.rs`, `pio.rs`, `mod.rs:372`) calls
`unshrink_class` at entry. The `m::` mechanism already proved the pattern:
generate one `c::` const per SDK class from the active map, match dispatch
arms and `DISPATCH_SITES` on `c::View` rather than the literal, emit
`PICODROID_NATIVE_CLASSES` through the same consts, and both translators
become dead code. Keep `unshrink_class` behind `cfg(test)` for the contract
and `method_tables` tests, which already use it that way. The `b/` table must
stay for `Class.getName()` and pre-0.15 PAPKs, so ~2 KB of the total is
non-recoverable; call it **~17 KB**. No-shrink images are byte-identical, as
with `m::`.

### 6.3 App-driven tree-shake of the SDK corpus — ~19.8 KB *(projected)*

The corpus embeds all 145 classes regardless of the app. Closing the
reference graph from `picoenvmon.papk`'s constant pools (62 SDK classes) plus
the classes Rust instantiates or upcalls by name (`DISPATCH_SITES`, the
`shrink_class("…")` allocation sites — 16 more) reaches **99 classes,
59,394 B**; the other **46 classes, 19,775 B** are unreachable for this app:
`java/util/Arrays` 1,715, `RadioGroup` 1,488, `EditText` 978, `AtomicLong`
884, `AtomicInteger` 873, `GestureDetector` 862, `Spinner` 828,
`CountDownLatch` 817, `Snackbar` 741, `Keyboard` 732, `SeekBar` 648,
`java/util/Collections` 601, `ProgressBar` 560, … and 33 smaller ones.

`framework_class_excludes` in `board.toml` is already the mechanism, applied
per board by hand. An app-driven variant would have `build.rs` read the PAPK
being embedded (it already has the path via `PICODROID_APK_PATH`), compute
this closure, and exclude the rest — with two rules: the Rust-side root set
must be generated, not hand-listed (the same `c::` generator from §6.2 can
emit it), and `pdb install` of a *different* app onto such an image must fail
with a clear "framework subset" error rather than `ClassNotFound` at runtime.
The SDK-side `.text` that only those classes reach (the LVGL widgets in §6.5,
their `lvgl_backend` arms) does not go away with the class files; that needs
§6.5.

### 6.4 Float parse and format — up to ~25 KB

`core` float support costs **38,956 B** (27,048 `.text` + 11,908 `.rodata`),
plus libm's 22.5 KB for `Math.*`. Three things pull it in:

1. **`Double.toString` / `Float.toString`** —
   `jvm/src/object_heap/mod.rs::java_float_layout` finds the shortest
   round-tripping digit string by formatting candidates with `{:.N}`
   (`flt2dec::format_exact`: dragon 4,604 + grisu 1,126 + `mul_pow10` 950 +
   `float_to_*_exact` 1,664) and parsing each back with `str::parse::<f64>`
   (`dec2flt`: `POWER_OF_FIVE_128` 10,416 + `from_str` 3,438 + `DecimalSeq`
   ~1,500).
2. **`Double.parseDouble`** (`jvm/src/native/boxed.rs:476`) — the same
   `dec2flt`, needed anyway.
3. **`String.format("%.Nf")`** and Rust-side `{}` on floats —
   `format_shortest` (dragon 5,336 + grisu 2,996 + `CACHED_POW10` 1,296).

Rust's `{}` on an `f64` *is* the shortest round-trip representation
(grisu with dragon fallback), so `java_float_layout` could take its digits
and exponent from `format_shortest` via `{:e}` and re-lay them out in Java
syntax, dropping the candidate loop and the parse. That removes the
`format_exact` path only if nothing else uses precision formatting (the
`string_format` module has its own `decimal_digits`; a grep for `{:.` in
`jvm/` and `picodroid-core/` is the check). `parseDouble` keeps `dec2flt`
unless a smaller, correctly-rounded parser is accepted — Java requires
correct rounding, and the 10 KB table is what buys it. Realistic:
**~8 KB** (exact-format path) low-risk; **~25 KB** if `parseDouble` moves to
a compact Eisel-Lemire without the 128-bit table, medium risk.

### 6.5 LVGL configuration — ~20–35 KB

LVGL is the largest bucket at 184 KB. Measured pieces that are configuration
or usage questions rather than intrinsic cost:

| Piece | Bytes | Note |
|---|---:|---|
| `lv_draw_sw_blend_image_to_argb8888` + `color_to_argb8888` | 10,564 | `LV_DRAW_SW_SUPPORT_ARGB8888 1` "needed internally for blending" — verify against 9.5's RGB565A8 path; the display is RGB565 |
| `lv_draw_sw_blur` | 4,868 | 9.5 backdrop blur; only used if a style sets it — gated by `LV_DRAW_SW_COMPLEX`, no finer switch upstream, so a `-D` or a vendored `#if` |
| `lv_draw_sw_box_shadow` | 4,216 | theme-default cards; drop if no `shadow_*` style is ever set |
| `lv_draw_sw_transform` | 4,462 | `ImageView.setScale`/rotation |
| `lv_theme_default_init` | 6,712 | `LV_USE_THEME_SIMPLE` is ~1/4 the size but restyles every widget |
| `lv_font_montserrat_14` | 13,617 | kerning tables are 3,305 of it; a converted font without kerning and with a trimmed glyph range is the usual embedded move |
| widgets the SDK wraps but this app never creates: `textarea` 3,974, `buttonmatrix` 3,370, `arc` 3,130, `roller` 2,760, `dropdown` 2,648, `calendar` 1,704, `keyboard` 922, `line` 788 | 19,296 | `LV_USE_*` per board or per app; pairs with §6.3 |
| `lv_vsnprintf` | 2,468 | LVGL's own printf; `LV_SPRINTF_USE_FLOAT`/custom |

Only the first three are safe without a feature decision. The widget row is
the one that needs the §6.3 root set: `LV_USE_CALENDAR 0` breaks `DatePicker`
for any app that uses it, so it wants to be derived from the same closure.

### 6.6 `pdb` and USB as a product-image feature — ~14 KB

`picodroid_core::pdb::run_pdb_task` (10,830), the USB CDC transport (2,672
`USBCTRL_IRQ` + ~330) and `install::*` (386) are always compiled. With the
app embedded at build time they serve development; a `no-pdb` feature for a
shipped `picoenvmon` image is ~14 KB, and the `defmt`/RTT path is separate
(1,490 B) so logging survives. Under `-Os` this bucket is 3.7 KB, so it
matters more at `opt-level 3`.

### 6.7 App PAPK obfuscation — 9.3 KB, in the PAPK slot *(landed 2026-09-02, `--shrink-app`)*

Shrinking `picoenvmon/*` class names to a third prefix (`c/`) saves 3,071 B
as `Class` entries and 2,790 B inside descriptors; renaming the 333
app-private member names (4,572 B) at 2–3 chars saves ~3,500 B. Together
**~9.4 KB of 50.2 KB** projected; measured **49,929 → 40,632 B (−9,297 B,
18.6 %)** for the stripped `picoenvmon` PAPK and 917,393 → 908,096 B for the
rp2350 release image. Landed as `scripts/build-apk.sh --shrink-app`
(`class-shrink cut-app`, see the shrinker reference): entry points are
*mapped* rather than kept (`papk-pack` spells the manifest entry through the
merged map, the `_MembersInjector` class follows its component's shrunk
name), and the merged map ships next to the PAPK as its retrace key.
Because the PAPK lives in `PAPK_FLASH` (§3), this relieves the 1 MB app slot
and OTA transfer time, not the program-region ceiling — which is why it sat
below §6.1–§6.6 despite being pure toolchain work.

### 6.8 Shared cross-class string table — ~23 KB upper bound *(projected)*

Identical `Utf8` text repeated across classes is 12,896 B in the SDK corpus
and 10,862 B in the PAPK. A corpus-wide string table (one dedup'd blob, 2-byte
indices in each class's pool) would recover most of it and also shrink the
`Class`/`NameAndType` pointer structure, but it changes the on-flash class
format and every `ClassFile` accessor. It is the only remaining lever on the
descriptor and `<init>`/`Code`/`()V` residue, and it composes with everything
above. Not worth doing before §6.3, which removes a third of the corpus.

### 6.9 Small and measured

- **`--remap-path-prefix`** for the `.cargo/registry` and `rust-src` prefixes:
  **−1,024 B** measured. The 121 `Location` strings are mostly precompiled
  `core` paths (`/rustc/<hash>/library/…`) that only `-Zbuild-std` could
  remap. Cheap, but not worth a rustflags change on its own.
- Opcode mnemonic table (394 B) — only needed by error text; `cfg` it with
  the trace features.
- Date-picker year list (629 B) and hour/minute lists (179 B) could be
  generated into a stack buffer at widget creation.
- `PICODROID_NATIVE_CLASSES` in shrunk form (August #3, 2,240 B) is
  subsumed by §6.2.

## 7. RAM, for completeness

`.bss` is 507,488 B and is not string-shaped: `ucHeap` 425,984 (the FreeRTOS
heap the JVM arena and everything else are carved from), LVGL's static pool
`work_mem_int` 49,152 (the board's 48 KB `LV_MEM_SIZE`), the render
`BAND_BUF` 12,960, `core1_stack` 2,048, `TOUCH_QUEUE` 2,048, and ~15 KB of
per-widget handle maps and animation slots. The "95 %" is a static
reservation, not pressure; RAM levers live in `docs/memory-diagnostics.md`
and the heap-census work, not here.

## 8. Recommended order

| # | Change | Saving | Risk | Effort |
|---|---|---:|---|---|
| 1a | C at `-Os` (`CFLAGS` in `build_support/lvgl.rs`, `freertos.rs`) | 81.5 KB | UI render speed, check with the UI benches | one line + ratchet |
| 1b | Benchmark profile-wide `opt-level = "s"` on HIL; adopt if the JVM `benchmark` delta is acceptable | 224.5 KB (incl. 1a) | JVM speed, needs measurement | profile line + ratchet + `lib.sh` rp2040 LTO rule |
| 2 | `c::` class consts; retire `shrink_class`/`unshrink_class`; emit `PICODROID_NATIVE_CLASSES` via them | ~17 KB | low — same pattern as `m::` | build.rs generator + arm rewrite |
| 3 | Teach `lib.sh`/ratchet to exclude `PAPK_FLASH` from `Flash:` | 0 B, correct gate | none | small |
| 4 | LVGL: ARGB8888 blend, blur, shadow off; font without kerning | ~18 KB | low–medium, visual check | `lv_conf.h` + font convert |
| 5 | `java_float_layout` via `format_shortest` | ~8 KB | low, conformance tests exist | `object_heap/mod.rs` |
| 6 | App-driven SDK tree-shake + derived `LV_USE_*` | ~20 KB + ~19 KB | medium — root discipline, `pdb install` guard | build.rs + board/app cfg |
| 7 | `no-pdb` product feature | ~14 KB | low | feature flag |
| 8 | App PAPK obfuscation (`c/` prefix + private members) | ~9 KB (PAPK slot) | medium | `class-shrink` + keep rules |
| 9 | Shared string table | ~23 KB | high — format change | after 6 |

Each step must advance `bench/parity/ratchet.toml` in the same commit, per
that file's header.

## 9. Reproducing these numbers

Scripts live in this session's scratchpad
(`/tmp/claude-1000/-home-shiv-projects-picodroid-rs/<session>/scratchpad/`):
`strings_report.py`, `jvmnames.py`, `runs.py`, `segment.py`, `classdir_cp.py`,
`papk_cp.py`, `attrs.py` as in August (retargeted to map v0.16.0, `[[class]]`
entries only, and the `[ab]/` prefixes), plus `syms.py` / `buckets.py` for
the `nm -S -C` bucket tables and the `opt-level` diff, and `experiments.sh` /
`experiments2.sh` for the rebuilds (each variant is a `--config` or `CFLAGS`
argument to `scripts/build.sh`; a full variant rebuild is ~40 s on this
machine, so measuring beats projecting).

Gotchas, two new:

- **The checkout is shared with other sessions.** Between the baseline build
  and a later `nm`, another session's `pico_enviro_mon_w` build replaced
  `target/thumbv8m.main-none-eabihf/release/picodroid`; the tell was 86
  `cyw43_*` symbols in a non-WiFi image and a `.rodata` end address that no
  longer matched. **Copy the ELF to the scratchpad immediately after the
  build, and check `arm-none-eabi-size` against the build log before every
  symbol-level read.** Every table in this document was re-derived from a
  verified copy.
- `nm` sizes for `.text` come to 733,000 B against a 731,184 B section
  (overlapping `.part.N`/ISRA clones); treat bucket totals as ±0.3 %.
- As before: `touch picodroid-core/build.rs`, use the freshest
  `picodroid-core-*/out/framework_classes_shrunk` under the *target* triple,
  and ignore `.text` string hits.
