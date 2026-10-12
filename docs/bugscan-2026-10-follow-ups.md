# Handover: findings left open after the 2026-10-10 bug scan

The 2026-10-10 scan of the codebase landed 40 one-bug-per-commit fixes on `main` (the series
`4a2a30a8..afbb1ce6`, merged with `cdc73f22`). This document handed over what the scan found and
did **not** fix. On 2026-10-11 a second round (branch `bugscan-followups-2026-10`, one commit per
item, each with a unit test or a sim-app probe, `./scripts/test.sh` green in both shrink modes)
closed every host-testable item; what remains needs the bench or a product decision.

## Still open

Nothing host-testable. The five items below were implemented on 2026-10-11 (second pass,
`2ab9d250..e1910e27`); what remains for A1 and A3 is the bench check
`docs/bench-handoff-2026-10-11.md` describes, which needs a testbench board with the XPT2046.

### A. Device findings — fixed 2026-10-11

| Item | Status | What changed |
| --- | --- | --- |
| A1 SPI ISR refill overran the RX FIFO | **closed** 2026-10-12 by the nightlies' before/after: `testbench_rp2040` printed the short-read line in 27–28 app logs every night through 10-11 (`54f9a219`) and in none of its 10-12 run (`23fe9f35`, first with the fix); the W slot went from 4 and 1 `spi1` lines (10-10, 10-11) to 0 across 326 logs including the 12 net rows. Bench the same day: `qa_ui` on `testbench_rp2350w` under 900 pdb calls in 5 min and `qa_life` twice on `testbench_rp2040`, no line | refill bounded by `xfer::may_queue_tx` (outstanding < `SPI_FIFO_DEPTH`); RORRIS counted into `RX_OVERRUNS` before the clear and printed in the short-read line; unit tests `the_refill_*` |
| A2 display SPI 62.5 MHz request ran at 75 MHz | fixed (option b) | the six RP2350 `board.toml`s ask for `75000000`; `const fn is_exact_hz` + a `const` assert in `hal/rp/display.rs` refuse an inexact `spi_freq`; test `the_display_request_is_rounded_up_at_150_mhz_and_exact_at_125` |
| A3 XPT2046 decoded 11 bits | fixed; bench check pending (corners by hand on one testbench) | 12-bit `decode`, three calibration tables doubled, `reject_lo` 100 → 200, `FakeBus` and the sim `FakeXptSpi` encode the chip's layout; tests `decode_takes_all_twelve_bits`, `full_scale_samples_reach_the_panel_edges` |
| A4 DMA write judged by stale ISR state | **closed** 2026-10-11: confirmed real and verified on `pico_display2_w`; an hour of `picoclock` under 799 scripted inputs plus three `graphicsbench` runs on `pico_touch_kit` logged nothing | `SpiOp::DmaWrite` recorded by `write_raw_start`; `xfer::transfer_finished` dispatches on the kind (`dma::busy`, `TFE`, `BSY`); `Idle` never finishes; tests `a_dma_write_*`. On the bench the kind-aware line still fired once per `graphicsbench` run (the nightlies had logged it for a week as `spi0: transfer finished with 0 of 0 bytes`): a band ending in the gap between the 2-tick `take` expiring and the register read. The wait now retries a zero-length `take` before taking the fallback; two full runs after that logged nothing. |
| A5 (2026-10-12) touch poll clocked into the panel on a shared bus | fixed; seen on `testbench_rp2350w` as a purple dash per band | `display::write_pixels_start` holds the panel's chip select until the next command collects the band, and the XPT2046 poll on the same bus did not count as one: its thirty bytes went into the ST7789 as fifteen pixels at the write pointer. `hal/rp/touch.rs::yield_display_bus` collects the band and releases the select before every panel read on a shared bus. Found while chasing the garbage-pixel report of the same day (claudeusage's RGB LED PWM on GP8, the testbench's DC line; `PeripheralManager` now refuses board-owned pads). |

### C10c. Modified UTF-8 in class and member names — supported 2026-10-11

`class-shrink`'s `ClassFile::normalize_utf8` (`modified_utf8_to_utf8`) re-encodes every
constant-pool `Utf8` entry to standard UTF-8; `papk-pack` applies it to every class it collects
and `shrink-dir` before matching the map. `class-link`'s refusal now means a malformed name. Tests
in both crates, including that the linker still refuses the unconverted bytes. Literals with
supplementary characters are UTF-8 byte strings now rather than unresolvable.

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
