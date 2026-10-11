# Handover: findings left open after the 2026-10-10 bug scan

The 2026-10-10 scan of the codebase landed 40 one-bug-per-commit fixes on `main` (the series
`4a2a30a8..afbb1ce6`, merged with `cdc73f22`). This document handed over what the scan found and
did **not** fix. On 2026-10-11 a second round (branch `bugscan-followups-2026-10`, one commit per
item, each with a unit test or a sim-app probe, `./scripts/test.sh` green in both shrink modes)
closed every host-testable item; what remains needs the bench or a product decision.

## Still open

### A. Device-only (needs the bench)

#### A1. SPI ISR can overrun the 8-deep RX FIFO — confirmed by reading

`platforms/rp/src/hal/rp/spi/mod.rs:142-147` (the step-3 refill) and `:156` (the step-5 clear).
The refill loop is bounded only by TX-FIFO-not-full. After the 8-byte seed, TXIM asserts with
four bytes still in flight; the ISR drains three and refills to a full TX FIFO, so nine bytes are
outstanding against an eight-deep RX FIFO. Any ISR latency of five byte-times or more (the USB
ISR at the same NVIC priority sending a 64-byte packet byte by byte is enough) overruns one byte,
and step 5 clears RORIC without ever reading RORIS, so the overrun leaves no trace. This is the
shape of the "neither overrun nor receive-timeout asserted" short read documented at `:264-273`
and papered over by `controller_finished`.

Fix: bound the refill by outstanding bytes, as pico-sdk does:
`state.tx_idx - state.rx_idx < 8` (optionally count RORIS before clearing it). Verify on the
bench with a touch poll under USB load; the short-read fallback should stop firing.

#### A2. Display SPI clock rounds up: 62.5 MHz requested, 75 MHz programmed — confirmed

`platforms/rp/src/hal/rp/spi/xfer.rs:70-73`: `scr = floor(pclk / 2f) - 1` picks the divisor
that gives at least the requested rate. At 150 MHz every RP2350 board's `spi_freq = 62500000`
yields SCR 0, which is 75 MHz, past the ST7789V's 16 ns write cycle. The unit test next to it only
re-derives the same formula. This is a product decision, not a one-line fix: `div_ceil` drops the
displays to 37.5 MHz (the SSP can only do 75, 37.5, 25 MHz at this PCLK); the alternative is to
change the four `board.toml` values to `75000000` so the number is honest. Measure page-turn time
before choosing.

#### A3. XPT2046 decode is 11-bit — confirmed by reading

`crates/pd-drivers/src/xpt2046.rs:38-40`: `(b1 << 4) | (b2 >> 4)` is the 12-bit sample shifted
right by one; the LSB is discarded and the range is 0..2047. The board calibrations
(`cal_x_min = 1970`, `cal_x_max = 185`, …) are the usual 12-bit values halved, which is why
nothing mis-maps. Fix is `((b1 << 8 | b2) >> 3) & 0xFFF` plus doubling the four `cal_*` values on
each board (or recalibrating); the FakeBus test encodes the current layout and must change too.

#### A4. `finish_isr_xfer!` judges a DMA write by the previous ISR transfer's state — suspected

`platforms/rp/src/hal/rp/spi/mod.rs:288-295`. `dma::start_write` never touches
`SpiXferState`, so on the DMA band path the predicate degenerates to `!BSY && !RNE`; a momentary
TX underrun at the sample instant would declare the band done and abort it. Gate the
`controller_finished` branch on the transfer kind, or test the RX-drain channel's busy bit.

### C10c. Modified UTF-8 in class and member names — design-level

`class-link` refuses CESU-8 surrogate pairs in class and member names at pack time
(`classfile.rs::check_utf8`): a legal identifier with a supplementary character cannot be packed.
The runtime reads names as `&str` straight from flash, so accepting them means re-encoding the
Utf8 entries to standard UTF-8 in the packer (class-shrink already rewrites constant pools). Rare in
practice; left open.

### Noted while fixing

- `ToggleButton` keeps its on/off texts in two 31-byte slots (`toggle_button.rs::TEXT_BUF_SIZE`);
  longer texts are cut. Every other widget text native takes any length since this round.
- The String model is "UTF-8 bytes": `charAt`/`length` count bytes, `toCharArray` hands the bytes
  back (high bytes as negative values), while a `char` from U+0080 up appended to a `StringBuilder`
  is its UTF-8 sequence and the `java.io` readers decode UTF-8 into chars. A `toCharArray` →
  `append(char)` round trip of non-ASCII text therefore does not reproduce the string; the docs
  (`api/core.md`, the embedded-gotchas guide) say so.

## Fixed on 2026-10-11

Branch `bugscan-followups-2026-10`, one commit per item; the test or sim probe is named in each
commit message.

| Item | Commit | What changed |
| --- | --- | --- |
| B1 stack cap leaves the top paused | `836a546a` | depth checked before `instantiate_component`; qa_life `ActD` probe |
| B2 `onServiceDisconnected` on own unbind | `45871f2b` | silent on both unbind paths; site removed; docs; qa_life asserts 0 |
| B3 `DatagramSocket` 256-byte cap | `825d5728` | packet-sized fallible buffers, `DatagramPacket` window API (`offset`, `bufLength`), `net/datagram_window.rs` |
| B4 chunked refill window | `f4d82391` | a chunked head past `RX_BUF_SIZE - IO_CHUNK` is "response headers too large" |
| B5 `requires-features` behind the peek | `0060e0bc` | whole-manifest re-check before the commit |
| B6 host erase timeout | `e4a00b24` | scales with the greeting's `max_papk` |
| C1 `invokestatic` initialises the CP class | `eb1fd1a9` | initialise the resolved target's class |
| C2 synchronized via upcall | `21b04287` | `enter_synchronized` at both push sites; `truncate_frames` on the error paths |
| C3 boxing failure escapes `run` | `222f535c` | collect, retry, then a catchable `OutOfMemoryError`; `collect_garbage` factored out |
| C4 `new Integer("42")` | `afa8aba0` | String constructors parse |
| C5 deterministic `new Random()` | `2b2459bd` | `random::mix_seed_entropy`, fed by the framework at JVM start |
| C6 failed `<clinit>` stays initialised | `1912e58f` | erroneous state, `NoClassDefFoundError` (new builtin) |
| C7 no operand verification | `3796d3e1` | `class-link::bytecode` walker in the link derivation (pack time and install); found a bad branch in a jvm test class (`21cbb24c`) |
| C8 chars ≥ 0x80 wipe a String | `28aa5943`, `56db79a2` | `string::char_bytes` (UTF-8) at every char sink; readers decode and writers encode UTF-8; `String(byte[])` keeps valid UTF-8 |
| C9 iterator CME on equal-size churn | `b2f4b6ce` | per-buffer modification counts |
| C10 `Arrays.sort`/`toString` infallible | `bd6e0387`, `9eac64d0` | `reserve_fallible`; alloc-scan baseline row dropped |
| C10 super-chain cycles | `0d65043f` | builder and section validator refuse a loop |
| (found) GC mark bitmaps grew infallibly | `ab9c9a19` | reserved up front; refusal = collection that frees nothing |
| D1 stale handle to LVGL | `81d32bdf` | router refuses a non-live handle with `IllegalStateException`; qa_life `ActC` probe |
| D2 null String to a widget native | `f4c77f07` | setters clear, Canvas text throws NPE |
| D3 127-byte widget buffers | `ed689dfe` | `with_cstr` everywhere |
| D4 INVISIBLE vs `setAlpha` | `8442b2d6` | alpha re-applied on every VISIBLE; no native write while INVISIBLE |
| D5 `SeekBar` `fromUser` | `b5f8b47a` | Java wrappers over `nativeSetProgress`/`nativeSetMax` report `false` |
| D6 `DatePicker` 1-based months | `addc27f4` | `updateDate`/`init`/`getDayOfMonth`, 0-based; `setDate`/`getDay` deprecated aliases |
| D7 `AlertDialog` button map | `497da10c` | `MAX_DIALOGS * 3`, `warn_full` |
| D8 `getChildCount` counts the spinner | `8d95f678` | Java count; `nativeGetChildCount` for `ListView` |
| E1/E2 `ViewPager2` stale page, negative item | `d5f11c3c` | unbind on id mismatch; clamp at 0 |
| E3 `popBackStack` INCLUSIVE | `695708be` | pops every same-named entry beneath |
| E4 zero-length read as EOF | `fe41b215` | readers loop while a read answers 0 |
| E5 ISO time fraction, JSON `-0`, Kotlin `split("")`, `copyOfRange` | `f12c6212`, `51fcaeba`, `dd80471a`, `65a01694` | |
| F1 `validate_components` under a shrink map | `c95ac490` | `b/` namespace is the terminal miss |
| F2 `*` literal next to `**` | `1df37315` | one backtracking glob matcher |
| F3 shared literal/descriptor constant | `ea36fb10` | own-member descriptors join the set |
| F4 mid-string quotes | `ede18657` | aapt2 semantics |
| F5 variant values vs base layouts | `683b7fed` | a warning naming layout, value and variant |
| F6 repack keys, map parser, board booleans, scripts, fused iterators, docs, override validation | `d50da756`, `5845f5e5`, `7d0c3c04`, `1ce9894d`, `773a8120`, `196959e6`, `6e74b1c7` | |

`sdk/api-contract.tsv`, `sdk/member-names.tsv` and `sdk/class-names.tsv` follow the new surface
(`7390cb51`).

## Checked and found correct (2026-10-10)

So the next pass need not redo them: interpreter arithmetic, conversions, control flow, stack ops,
arrays and `wide`; exception-table matching and EIIE wrapping; synchronized via bytecode invokes;
default-method selection; field and static resolution; the resolve cache; `Math.round`, min/max,
`floorDiv/Mod`; boxed parsing and caches; `Random`'s generator; `Arrays.sort` stability; `ArrayList`
bounds; `key_eq`; the map/list/iterator stores; `heap4`; `pd-json`; `pdb-protocol` framing and
CRC; `pd-tls` DER; `http-head` parsing; the XIP-off window, I2C/SPI/DMA/PIO drivers, `pdb_usb`
ZLPs, cyw43 deadlines, PSRAM timing; the install orchestrator's commit ordering and CRC; compat
versioning; `flash_layout`; threads, executors, the time task's retry ladder, the system clock;
lifecycle push/pop/recreate ordering; packages and placement; the storage sandbox and quota; the
fragment callback order, LiveData and ViewModel; `java.io` streams; `java.time` arithmetic;
`Bundle`, `Intent`, `SharedPreferences` ordering, JSON, protobuf, `KeyEvent` dispatch; the Kotlin
shim's collections, ranges, comparisons and most string helpers; `class-link` layout and bounds;
`papk-format` parsing bounds; `class-shrink` constant-pool handling, descriptors, renaming and
retrace; `papk-pack` colours, dimensions and id order.
