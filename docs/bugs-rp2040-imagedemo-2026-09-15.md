# `imagedemo` HardFaults on the RP2040 — 2026-09-15

**Status: FIXED — verified on `testbench_rp2040` 2026-09-15.** An unaligned
`uint16_t` read of asset pixels in XIP flash. The packer now aligns every papk
section to 4 bytes; the asset registry refuses an unaligned descriptor rather
than letting the board fault on it.

`imagedemo` was an ERROR on the rp2040 slot in both shrink modes every night
since at least 2026-09-09 — older than the 2026-09-13 QA round, and the last
open item on that board (`qa-2026-09-13-followups.md` §11). The app logged
`ImageDemo ready` and then the board faulted, with no panic message.

## 1. Why six nights of backtraces pointed at the wrong place

Every nightly capture ended:

```
Firmware exited unexpectedly: Exception
Core 0
    Frame 0: __Thumbv6MABSLongThunk__ZN9picodroid3hal4chip5flash27flash_program_range_xip_off... @ 0x10099918
    Frame 1: HardFault @ 0x00000000
```

which reads as a fault inside a flash operation. It is not. `0x10099918` is
`HardFault` itself — cortex-m-rt's trampoline — and `nm` puts
`flash_program_range_xip_off`'s long-branch thunk immediately before it, so
probe-rs labels the address with the preceding symbol. The 2026-09-09 triage
had already established this much and stopped there, because the faulting PC
is *not* in that capture.

The reason it is missing is **probe-rs**: since 0.31 `run` enables
`catch_hardfault` by default, so the core halts at the exception vector
**before the handler body executes**. Any logging the handler does never runs,
and the stacked frame is never unwound. Two things follow, both now in place:

- `platforms/rp/src/main.rs`'s `HardFault` handler logs the stacked frame
  (`pc`, `lr`, `r0`–`r3`, `r12`, `xpsr`) before it halts. Cortex-M0+ has no
  CFSR/HFSR to say *why* a fault happened, so pc/lr are the whole story.
- To see it, the catch has to be off:

```bash
PROBE_RS_PROBE=2e8a:000c:<serial> probe-rs run --chip RP2040 --protocol swd \
    --no-catch-hardfault target/thumbv6m-none-eabi/debug/picodroid
```

Resolve the pc against the ELF with
`arm-none-eabi-addr2line -e <elf> -f -C <pc>`. Keep the flag off for the
nightly: with the catch disabled an RP2350 fault would spin in the handler
(no `bkpt` there — a `bkpt` without a debugger lands a Cortex-M33 in lockup)
and the row would time out instead of erroring.

## 2. What it actually was

With the catch disabled the handler speaks:

```
[ERROR] [fault] HardFault pc=0x10047e4c lr=0x1004865d r0=0x101018ab r1=0x00000001
                r2=0x101018ab r3=0x00000000 r12=0x00000000 xpsr=0x01000000
    Frame 4: transform_rgb565a8 @ 0x10047e4a
       third_party/lvgl/src/draw/sw/lv_draw_sw_transform.c:706
```

`lv_draw_sw_transform.c:706-707` is

```c
const uint16_t * src_tmp_u16 = (const uint16_t *)(src + (ys_int * src_stride) + xs_int * 2);
cbuf[x] = src_tmp_u16[0];
```

and `r0` — the source pointer — is **`0x101018ab`, an odd address**. A
Cortex-M0+ answers an unaligned halfword load with a HardFault; it has no
unaligned-access support to fall back on. The RP2350 (Cortex-M33) and the
x86 simulator both perform the same read without complaint, which is exactly
why only this board, and only hardware, ever saw it.

`0x101018ab` is inside the papk, which `graphics/assets.rs` hands to LVGL
**in place**: the descriptor's `data` points straight into XIP-mapped flash so
that pixels are never copied into a 160 KB heap. Whatever alignment the file
has is the alignment LVGL reads at.

Only a *scaled* image reaches this function. `imagedemo`'s first `ImageView`
is `SCALE_FIT_CENTER` on a 64×64 asset in a 160×160 box, so the transform path
runs on the first draw — after `onCreate` has returned and logged
`ImageDemo ready`, which is why the log always looked like a clean start.

## 3. Root cause: sections were packed back to back

`papk_format::AssetIter` pads each asset payload to a 4-byte boundary
**relative to its section**, and a test asserted exactly that
(`assets_data_is_4_byte_aligned_within_section`). Nothing aligned the section
itself. `PapkBuilder::build` laid out

```
assets_offset = classes_offset + SECTION_HEADER_LEN + classes_len
```

and `classes_len` is the sum of a set of class files — any length at all. For
`imagedemo` it left `assets_offset = 0x87f`, i.e. 3 mod 4, so the payload sat
at `0x88b + 28 = ` section-relative 0x1c — aligned within the section, and odd
in the file and in flash:

| | offset |
|---|---|
| papk image base (sector-aligned) | `0x10101000` |
| `assets_offset` | `0x87f` ← 3 mod 4 |
| section data | `0x1010188f` |
| `logo.png` payload | **`0x101018ab`** = the faulting `r0` |

Every fixture in the tree happened to land on a boundary anyway, so no test
ever exercised an odd one.

## 4. The fix

- **`papk-format::write`** — `section_after()` rounds every section start up
  to 4 bytes and `pad_to()` zero-fills the gap. The reader takes each
  section's offset from the file header, so the padding is invisible to it and
  to older readers. Cost: at most 3 bytes per section (`imagedemo`'s papk grew
  10,411 → 10,416 B). `papk-pack` builds through this builder, so every app
  picks it up.
- **`graphics/assets.rs`** — a descriptor whose `data` is not 2-byte aligned is
  skipped with `[assets] <name> skipped: pixel data is not 2-byte aligned`
  instead of being handed to LVGL. This is what covers the papks already
  installed on devices, which are still packed the old way: verified on the
  board, an unaligned `imagedemo` now runs to completion with one error line
  and no image, where it used to take the board down.

### Guards

- `write.rs::sections_and_asset_data_are_4_byte_aligned_in_the_file` sweeps
  class payload lengths 1..=8 — the ones that produce each residue — and
  asserts the *file-absolute* offsets, not the section-relative one. It fails
  on the unfixed writer (checked by reverting the rounding).
- `tests/golden.rs`'s two rebuild tests compared the builder's output to the
  pre-refactor `papk-pack` fixtures byte for byte, which the padding breaks by
  design. They now compare section header + data for each section and require
  the new offsets to be aligned, so the fixtures stay exactly as papk-pack
  produced them — they are also the proof that the reader still handles the
  unaligned files already in the field.

## 5. Two things to know when re-testing this

- **The papk does not get repacked when only the packer changes.**
  `PapkPackTask` declares the app's sources and manifest as inputs, not
  `tools/papk-pack` or the `papk-format` crate behind it, so Gradle considers
  a warm `build/papk/<app>.papk` up to date and `flash.sh` ships the stale
  bytes. The first flash after this fix landed the *old* layout and looked
  like the fix had failed. `rm build/apks/<app>.papk
  examples/<app>/build/papk/<app>.papk` forces the repack; a clean tree (CI,
  the nightlies) is unaffected.
- **`flash.sh` does not exit**, so its `probe-rs` keeps the probe; the next
  flash fails with `interface is busy (errno 16)`. Kill that one by pid
  (`pgrep -x probe-rs`, check `/proc/<pid>/cmdline` for the chip) — never
  `pkill -f probe-rs`, which would take out another slot's run.

## 6. Verification

On `testbench_rp2040`, aligned papk:

```
[INFO ] [assets] 1 images loaded
[INFO ] ImageDemo: loading bundled asset 'logo.png'
[INFO ] ImageDemo: scale-center applied
[INFO ] ImageDemo: ImageDemo ready
```

no fault, no `Firmware exited unexpectedly`.

The matrix row itself, which had been an ERROR in both modes every night since
2026-09-09:

```
./scripts/hil-run.sh --app imagedemo --board testbench_rp2040 --no-email
  PASS
  PASS
  PASS: 2  FAIL: 0  SKIP: 0  ERROR: 0
```

Host suites: `papk-format` 74 tests (3 of them new or rewritten) and
`pico-jvm` 727, both green; `./scripts/pre-commit` ends `All checks passed.`
