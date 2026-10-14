# Completed: Flash budget, September 2026 — after the string work, where the bytes go now

Items closed out of [flash-budget-2026-09.md](../designs/flash-budget-2026-09.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

## 0. TL;DR

| Lever | Saving | Status |
|---|---:|---|
| C code alone at `-Os`, Rust untouched at 3 | **−81,552 B (−8.6 %)** | measured, §6.1 — no JVM-speed exposure |
| Retire `shrink_class` / `unshrink_class` in favour of `c::` consts | **−27,154 B** (943,959 → 916,805) | **landed 2026-09-02**, §6.2 |
| App PAPK class/member obfuscation (`--shrink-app`) | **−9,297 B** PAPK (49,929 → 40,632), **outside the program region** | **landed 2026-09-02**, §6.7 |
| One `Utf8` pool per class section (link layout 3, PAPK v3) | **−112,172 B rp2350, −39,540 B rp2040** (release `helloworld`) | **landed 2026-10-13**, §6.8 |

## 6. Opportunities

### 6.2 Retire runtime class-name translation — ~19 KB

> **Landed 2026-09-02** as [unconditional-shrink-2026-09.md](../designs/unconditional-shrink-2026-09.md)
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

### 6.8 Shared cross-class string table — landed 2026-10-13 as one `Utf8` pool per section

> **Landed 2026-10-13** as 6c205169 (link layout 3, PAPK major 3; write-up in
> [class-link-2026-09.md](../designs/class-link-2026-09.md), "Third round"). The
> design differs from the sketch below: rather than a 2-byte index per entry
> inside each class's pool, every distinct `Utf8` of a *section* lives once in a
> pool at the section's end, a pooled class keeps only each `Utf8` entry's tag
> byte, and every link-table word that held a byte offset holds a 16-bit pool
> reference; the pool's place is in the table's header, so a loaded class costs
> no RAM. Framework section: rp2040 no-shrink 261,296 → 219,256 B, rp2040 shrink
> 197,648 → 181,608 B, rp2350 no-shrink 561,804 → 460,796 B. Release
> `helloworld` firmware: rp2040 −39,540 B, rp2350 −112,172 B; the
> `pico_display2_w` debug image 2,358,688 → 2,199,976 B together with the
> October dedup and hash-dispatch rounds. Five to ten times the ~23 KB priced
> here, because the old sketch kept a 5-byte stub per entry and could only pool
> strings of 8 bytes or more.

Identical `Utf8` text repeated across classes is 12,896 B in the SDK corpus
and 10,862 B in the PAPK. A corpus-wide string table (one dedup'd blob, 2-byte
indices in each class's pool) would recover most of it and also shrink the
`Class`/`NameAndType` pointer structure, but it changes the on-flash class
format and every `ClassFile` accessor. It is the only remaining lever on the
descriptor and `<init>`/`Code`/`()V` residue, and it composes with everything
above. Not worth doing before §6.3, which removes a third of the corpus.

## 8. Recommended order

| # | Change | Saving | Risk | Effort |
|---|---|---:|---|---|
| 1a | C at `-Os` (`c_opt_level` in the MCU toml, `config::apply_c_opt_level`) — **landed 2026-09-08**, rp2040 in `773dc9a`, rp2350 plus frame pointers the same day | −92,928 B rp2350, −19,132 B rp2040 (§6.1 status) | UI render speed, unmeasured | done |
| 1b | Benchmark profile-wide `opt-level = "s"` on HIL; adopt if the JVM `benchmark` delta is acceptable — **measured 2026-09-08, rejected**: −166.9 KB `.text` for +29 % on the interpreter sections (§6.1 status) | — | JVM speed: three times the 10 % budget | not taken |
| 2 | `c::` class consts; retire `shrink_class`/`unshrink_class`; emit `PICODROID_NATIVE_CLASSES` via them — **landed 2026-09-02** (47bc221, map v0.17.0) | −27,154 B measured | — | done: `build_support/names.rs` + every arm on `c::`/`m::`/`d::` |
| 3 | Teach `lib.sh`/ratchet to exclude `PAPK_FLASH` from `Flash:` — **landed 2026-09-19**: `lib.sh::app_region_bytes` takes `.papk_flash_init` out of `Flash:`, the size lane logs `#app_region_bytes=` and `bench-backfill.py` takes it out of `flash_bytes`; `ratchet.toml` rebased by −4,928 B on both testbench boards (rp2040 836,968 of 917,248, 80,280 B free) | 0 B, correct gate | — | done |
| 8 | App PAPK obfuscation (`c/` prefix + private members) — **landed 2026-09-02** (1eed0b8, `--shrink-app`) | −9,297 B PAPK measured | — | done: `class-shrink cut-app` + the `main`/`injectMembers` keeps |
| 9 | Shared string table — **landed 2026-10-13** (6c205169) as one `Utf8` pool per class section, link layout 3 / PAPK v3 | −112,172 B rp2350, −39,540 B rp2040 measured (release `helloworld`) | — | done: `crates/class-link` build/validate/section, `papk-format` v3, `papk-pack --repack` |
| 4 | LVGL config: `LV_USE_LINE 0`, `lv_draw_sw_box_shadow.c` and `lv_draw_sw_blur.c` replaced by `pd_draw_sw_stubs.c` — **landed 2026-10-14**; ARGB8888 measured (8,424 B) and kept, it backs every rotated or scaled view; the fonts untouched by decision | −6,762 B `pico_display2_w` debug, −7,368 B rp2040 / −6,776 B rp2350 release | pixel-identical sim shots | done: `lv_conf.h`, `build_support/lvgl.rs`, a pd-lvgl-sys test pinning the dark theme and the unbound setters |
