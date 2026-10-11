# Handover: findings left open after the 2026-10-10 bug scan

The 2026-10-10 scan of the codebase landed 40 one-bug-per-commit fixes on `main` (the series
`4a2a30a8..afbb1ce6`, merged with `cdc73f22`; every commit carries its own unit test or sim-app
probe). This document hands over what the scan found and did **not** fix: each item has its
location, what happens versus what should, a way to reproduce it, and a suggested fix. Items are
ordered by how sure the finding is and how much it costs an app.

Two kinds of items are mixed here and marked as such:

- **Confirmed** — traced through the code to a concrete wrong outcome; only the fix is missing.
- **Suspected** — the code reads wrong but the trigger or the impact was not pinned down.

Everything else the scan examined and found correct is listed at the end, so the next pass can
skip it.

## A. Device-only (needs the bench)

### A1. SPI ISR can overrun the 8-deep RX FIFO — confirmed by reading

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

### A2. Display SPI clock rounds up: 62.5 MHz requested, 75 MHz programmed — confirmed

`platforms/rp/src/hal/rp/spi/xfer.rs:70-73`: `scr = floor(pclk / 2f) - 1` picks the divisor
that gives at least the requested rate. At 150 MHz every RP2350 board's `spi_freq = 62500000`
yields SCR 0, which is 75 MHz, past the ST7789V's 16 ns write cycle. The unit test next to it only
re-derives the same formula. This is a product decision, not a one-line fix: `div_ceil` drops the
displays to 37.5 MHz (the SSP can only do 75, 37.5, 25 MHz at this PCLK); the alternative is to
change the four `board.toml` values to `75000000` so the number is honest. Measure page-turn time
before choosing.

### A3. XPT2046 decode is 11-bit — confirmed by reading

`crates/pd-drivers/src/xpt2046.rs:38-40`: `(b1 << 4) | (b2 >> 4)` is the 12-bit sample shifted
right by one; the LSB is discarded and the range is 0..2047. The board calibrations
(`cal_x_min = 1970`, `cal_x_max = 185`, …) are the usual 12-bit values halved, which is why
nothing mis-maps. Fix is `((b1 << 8 | b2) >> 3) & 0xFFF` plus doubling the four `cal_*` values on
each board (or recalibrating); the FakeBus test encodes the current layout and must change too.

### A4. `finish_isr_xfer!` judges a DMA write by the previous ISR transfer's state — suspected

`platforms/rp/src/hal/rp/spi/mod.rs:288-295`. `dma::start_write` never touches
`SpiXferState`, so on the DMA band path the predicate degenerates to `!BSY && !RNE`; a momentary
TX underrun at the sample instant would declare the band done and abort it. Gate the
`controller_finished` branch on the transfer kind, or test the RX-drain channel's busy bit.

## B. Framework and lifecycle

### B1. Activity push past the stack cap leaves the previous Activity paused — confirmed

`crates/picodroid-core/src/lifecycle/activity_stack.rs:95-126`. The new Activity is instantiated
and `prev.onPause` runs before `handler.push_activity`, which fails at `MAX_ACTIVITY_STACK`
(default 8). The rollback restores the view but never calls `onResume`, so the visible Activity
stays paused (anything it stopped in `onPause` never restarts), and the constructed instance is
orphaned without a callback. Fix: check `handler.activity_depth() >= MAX_ACTIVITY_STACK` before
`instantiate_component`. Verify with a sim app whose `onCreate` starts itself with a counter.

### B2. `onServiceDisconnected` is delivered on the app's own `unbindService` — confirmed

`crates/picodroid-core/src/service_lifecycle.rs:489-504` (`process_unbind`), also via
`unbind_owned_by` (`:204-222`). Android calls it only when the connection is lost. The standard
"rebind in onServiceDisconnected" pattern turns an `unbindService` in `onDestroy` into a bind by
a dying Activity, so the Service never reaches `onDestroy`. Fix: drop the
`invoke_connection_disconnected` call from `process_unbind` (keep it for a future
service-died path). Check the existing service tests before changing; some may pin the current
delivery.

### B3. `DatagramSocket` caps every datagram at 256 bytes — confirmed

`crates/picodroid-core/src/net/datagram_socket.rs:193` (`BUF_SIZE`), `:258`, `:295-305`.
`send` transmits a truncated datagram without error; `receive` reads into a 256-byte stack buffer
(the kernel discards the rest), copies without checking the packet's array length (an
`ArrayIndexOutOfBounds` fault for a shorter buffer) and ignores the packet offset. The docs show
`new byte[1500]` as the receive idiom; SNTP's 48 bytes are the only in-tree user. Fix: size by the
packet (fallible `Vec`, `IOException` above ~1472), honour `offset`. Verify against
`socat UDP-LISTEN:9000,fork EXEC:cat` with a 300-byte echo.

### B4. Chunked-body refill window shrinks with the response head — suspected

`crates/picodroid-core/src/net/http_connection.rs:729-744` with `parse_response_head`
(`:764-770`). `chunked_read` refills into `rx_buf[head_len..]`; a head that ends exactly at byte
1024 is accepted and then refilled into a zero-length slice, which reads as "unexpected end of
stream inside chunked body" on a well-formed response. Fix: refuse heads longer than
`RX_BUF_SIZE - IO_CHUNK` with the existing "response headers too large" error.

### B5. `requires-features` straddling the install peek reads as no requirement — suspected

`crates/pd-install/src/orchestrator.rs` (the `required.is_some_and(..)` check):
`find_manifest_value_in_prefix` returns `None` for an entry that crosses the peek, and the
builder writes `requires-features` last, after `label`, `icon` and the design keys. A PAPK with a
~330-character label and a required feature installs on a board without it. The install-time
image check added in `c6dc7349` reads the whole image before commit; re-checking the feature list
there (or refusing when the manifest does not fit the peek) closes it.

### B6. Compaction runs before `READY`, and the host budget assumed a 1.5 MB region — suspected

`orchestrator.rs` `place()` compacts inside the install; `tools/pdb/src/install.rs:18-21` sizes
`ERASE_TIMEOUT` (120 s) for a 1.5 MB region, while the touch kit's app region is 9984 KB
(≈2496 sectors at 45 ms each). A fragmented, nearly full touch kit times out on the host while the
device finishes and then waits for stream bytes that never come. Derive the host timeout from the
greeting's region size, or emit a keepalive during compaction.

## C. JVM

### C1. `invokestatic` initialises the constant-pool class, not the declaring class — confirmed

`crates/jvm/src/interpreter/ops_invoke.rs:225-238`: `ensure_class_initialized(class_bytes)`
runs before resolution. `Sub.f()` where `f` is inherited from `Base` runs `Sub.<clinit>` early
(JVMS 5.5 names only the declaring class). `getstatic`/`putstatic` were already fixed to the rule
(M8); apply the same: resolve first, then initialise `Target::Java { ci }`. Test next to
`clinit.rs::getstatic_through_subclass_initialises_only_the_declaring_class`.

### C2. Synchronized methods entered through an upcall never take their monitor — confirmed

`ops_invoke.rs:1054-1062` (`invoke_java_inner`) and `ops_reflect.rs:1006` (`run_frames`) push
the frame without `enter_synchronized`; only the main-loop `pending_frame` path does. A
`synchronized compareTo` under `Collections.sort`, a `synchronized toString` under
`String.format`, or a lambda target reached from a native runs unlocked. Fix: call
`enter_synchronized` before the push in `invoke_java_inner`; test with `upcall.rs`'s harness and
`synchronized.rs`'s `RecordingHandler`.

### C3. Return-value boxing failure escapes `run` and leaks monitors — confirmed by reading

`interpreter/mod.rs:786-789`: `box_primitive(..).ok_or(StackOverflow)?` leaves `run` without the
translation to a catchable `OutOfMemoryError`, and the nested-run cleanups in `invoke_java_inner`
and `run_frames` use `frames.truncate(base)`, dropping synchronized frames without
`monitor_exit`. Route the failure through the normal path and use `truncate_frames`.

### C4. `new Integer("42")` stores the String reference as the value — confirmed

`crates/jvm/src/native/boxed.rs:36-49`: the `<init>` arm admits any descriptor and writes
`args[1]` into field 0. `new Integer("42").intValue()` is 0; `new Boolean("true")` hands a
reference back where an int is expected. Fix: when the descriptor is `(Ljava/lang/String;)V`, run
the class's parser; or list only the primitive constructors so the contract rejects it at compile
time.

### C5. `new Random()` is deterministic across boots — confirmed

`crates/jvm/src/native/random.rs:30-38, 96`: the seed uniquifier starts at a constant and only
multiplies; nothing mixes a clock in. The n-th `new Random()` after every reset gets the same
seed. Mix `clock_nanos` or `currentTimeMillis` into the seed (the handler has both).

### C6. A class whose `<clinit>` fails stays "initialised" — confirmed divergence, low severity

`interpreter/mod.rs:252-254` marks the chain initialised before any `<clinit>` runs and nothing
un-marks it on failure; at `:901-905` an over-deep queue is cleared and the classes are
initialised with defaults and no initializer ever run. JVMS 5.5 says erroneous, then
`NoClassDefFoundError`. An erroneous bit in `StaticFieldStore` set by the wrap path fixes both.

### C7. No bytecode operand verification — confirmed path, exposure depends on PAPK trust

Operand reads index `code` directly (`ops_wide.rs:98-116`, `ops_control.rs:675-1011`, …), and
`class-link`'s validate never walks instructions. A truncated `wide`, a `tableswitch` with
`low = i32::MIN`, or a negative `npairs` panics the interpreter (a reset on device). A pack-time
instruction walker in `class-link::validate` (operand lengths, branch targets inside
`code_length`, `npairs >= 0`, `low <= high`) is the right place; no runtime cost.

### C8. A `char` at or above 0x80 wipes the whole String — confirmed, model question

`string_builder.rs:74`, `string.rs:359` and `string_format.rs` (`%c`) write the low byte of the
char as a lone byte; `StringTable::resolve` then fails the UTF-8 check and every caller's
`unwrap_or("")` reads the string as empty. `"23" + '°' + "C"` has length 0. The byte-string
model is documented; the expected degradation is the `'?'` that `String(byte[])` already applies,
or a two-byte UTF-8 encoding. One shared `char_byte` helper at the four sites.

### C9. HashMap iterators miss equal-size structural changes — suspected (semantic gap)

`iterator.rs:104` compares lengths, not a modification count; `remove(k); put(other, v)` inside
a `for (k : map.keySet())` shifts the backing `Vec` and silently skips or repeats an element where
Java throws `ConcurrentModificationException`. A `mod_count` per store fixes it; no out-of-bounds
read occurs today.

### C10. Smaller JVM items

- `Arrays.sort` and `Arrays.toString` allocate infallibly from Java-sized input
  (`arrays.rs:108-139, 328`): a 64 KB buffer on the RP2040 is an alloc-error abort rather than an
  `OutOfMemoryError`. `try_reserve_exact` like the rest of the crate.
- `class-link` accepts `A extends A` and cycles (`build.rs:178-187`, `section.rs:323-331`); the
  runtime's unbounded super-chain walks then hang. Walk the chain in `ClassSection::validate`.
- Modified UTF-8 (CESU-8 surrogate pairs) in class and member names is refused at pack time
  (`classfile.rs:217-223`); a legal identifier with a supplementary character cannot be packed.

## D. Graphics and widgets

### D1. About 30 widget natives hand a stale handle to LVGL unchecked — confirmed

With `LV_USE_CHECK_ARG 0`, LVGL dereferences the object at once, and `handle_table::lookup`
returns null for a deleted widget. A finished Activity's Views keep non-zero handles (only
`removeView` releases them), so a main-executor task left running from a finished Activity that
calls `sw.setChecked`, `editText.setText` or `seekBar.setProgress` is a null dereference: a
sanitizer panic in the sim, a HardFault on device. Sites: `text_view.rs:218`,
`edit_text.rs:265-281`, `check_box.rs`, `radio_button.rs`, `switch.rs`, `toggle_button.rs`,
`seek_bar.rs`, `linear_layout.rs:47-54`, `list_view.rs:149`, `button.rs:158-239`, `spinner.rs`
(the `register_listener` variants also root the Java object under key 0 for ever). Fix centrally:
`view::extract_native_handle` throws the existing "this view was released" `IllegalStateException`
when `!handle_table::is_live(id)`, or each site null-checks as `text_view::set_text` does.

### D2. A `null` String to a widget native is an uncatchable `InvalidReference` — confirmed

`crates/picodroid-core/src/graphics/view.rs:56-66` (`extract_string_at`). `EditText.setText(null)`,
`setHint(null)`, `CheckBox.setText(null)`, `ListView.addItem(null)`, `Canvas.drawText(null, …)`
abort the whole callback ("Activity lifecycle error: InvalidReference", the rest of `onCreate`
skipped). Android clears the text, or throws a catchable NPE for `drawText`/`measureText`. Map
`Null` to `""` in the text setters (as `Button`, `Toast` and `Snackbar` already do) and throw an
NPE for the Canvas ones.

### D3. `Spinner.setItems` truncates the option list at 127 bytes — confirmed

`graphics/lvgl/widgets/spinner.rs:49-55` copies the joined items into a 128-byte stack buffer;
twenty "Option NN" items show twelve and a fragment, and positions past 12 cannot be selected.
The same fixed buffers survive in `list_view.rs:145`, `check_box.rs:53`, `radio_button.rs:58`,
`button.rs:142`, `toast.rs:98`, `snackbar.rs:183,219`, `alert_dialog.rs:927`,
`number_picker.rs:142` and `toggle_button.rs`, and all slice at a byte offset (a multibyte char
can be split). Route them through `text_view::with_cstr` as `TextView.setText` was.

### D4. `INVISIBLE` and `setAlpha` share LVGL's `opa` style — confirmed

`graphics/lvgl/view_ops.rs:125-130, 196` and `sdk/java/picodroid/view/View.java:728-764`.
`setAlpha(0.3f); setVisibility(GONE); setVisibility(VISIBLE)` renders opaque while `getAlpha()`
says 0.3; `setVisibility(INVISIBLE); setAlpha(0.8f)` reveals the invisible view. Fix in Java:
re-apply the cached alpha on every transition to VISIBLE and skip the native write while
INVISIBLE. Visual only, so verify with a sim screenshot.

### D5. `SeekBar.setProgress`/`setMax` never report `fromUser = false` — confirmed

`sdk/java/picodroid/widget/SeekBar.java` (natives; `fireProgressChanged` hard-codes `true`) and
`graphics/lvgl/widgets/seek_bar.rs:93-99`. The standard "keep a label in sync via the listener"
idiom desyncs on programmatic changes. Fix: make `setProgress`/`setMax` Java wrappers over
renamed natives (`nativeSetProgress`, …; the Rust dispatch is by name, so `method_tables.rs` and
the API contract follow) that compare `getProgress()` before and after and call the listener with
`false`.

### D6. `DatePicker` months are 1-based — documented divergence that contradicts the project goal

`widgets/date_picker.rs:66` and `DatePicker.java`. Android's `getMonth()`, `updateDate` and
`OnDateChangedListener` are 0-based; ported `GregorianCalendar(y, picker.getMonth(), d)` code is
off by a month. Convert at the Java boundary and rename `setDate` to `updateDate`.

### D7. `AlertDialog` button map is undersized — suspected

`widgets/alert_dialog.rs:139`: `MAX_BUTTONS = 16` against 8 dialogs × 3 buttons;
`register_button` returns silently when full and that button never dispatches. `MAX_BUTTONS =
MAX_DIALOGS * 3` plus a `warn_full`.

### D8. `SwipeRefreshLayout.getChildCount()` counts the native spinner — confirmed

`ViewGroup.getChildCount()` is native (LVGL children) while `getChildAt` reads the Java list;
`swipe_refresh_layout.rs:127` creates the spinner inside the container, so `getChildAt(1)` is
null for `i < getChildCount()`. Return `mChildCount` from Java.

## E. SDK Java and Kotlin shim

### E1. `ViewPager2` keeps a stale fragment after `notifyDataSetChanged()` — confirmed

`sdk/java/picodroid/widget/ViewPager2.java:369-371`: `layout()` unbinds only when the item id at
the bound *position* is gone, never comparing it with the bound fragment's own id
(`FragmentStateAdapter.mFragmentId`). Remove item 0 while on page 1: the old page stays, labelled
position 1, and `saveState` later files it under the wrong id. Add a package-private
`boundItemId()` and unbind when `getItemId(mBoundItem) != boundId`.

### E2. A negative `setCurrentItem` before pages exist leaves the pager blank — suspected

`ViewPager2.java:129-133` stores the item unclamped when no adapter is attached; `swapBind` then
bails on `mCurrentItem < 0` for ever. Clamp to 0 there and in `setAdapter`/`layout()`.

### E3. `popBackStack(name, POP_BACK_STACK_INCLUSIVE)` pops one same-named entry — suspected

`FragmentManager.java:487-500`. Android also consumes the entries with the same name directly
beneath. After locating `index`, walk down while the name repeats.

### E4. Readers treat a zero-length read as end of stream — suspected

`sdk/java/java/io/BufferedReader.java:49-54` and `InputStreamReader.java:40-45` return EOF for
`n <= 0`; the JDK loops while `n == 0`. Bites only if a native read can answer 0; none of the
in-tree streams documents that, so confirm before changing.

### E5. Smaller items

- `ISO_LOCAL_TIME` / `ISO_LOCAL_DATE_TIME` never print a fraction (`DateTimeFormatter.java:23-26`
  are fixed `HH:mm:ss`); route them through `LocalTime.appendTo`.
- `JSONObject.numberToString(-0.0)` prints `0`; Android prints `-0`.
- Kotlin `split("")` ignores the empty delimiter (`StringsKt.java:474`); the stdlib yields
  `["", "a", "b", "c", ""]`.
- Kotlin `copyOfRange` throws `IndexOutOfBoundsException` for `from > to`; the stdlib throws
  `IllegalArgumentException`.

## F. Tooling

### F1. `papk-pack validate_components` passes under a shrink map — confirmed

`tools/papk-pack/src/main.rs:565-579`: the "does not extend" error fires only when the super
chain reaches `java/...` or `java/lang/Object`; under `--shrink-map` those spell `b/X`, so a
declared `<activity>` that does not extend `Activity` is accepted in a release build. Compute the
shrunk spelling of `java/lang/Object` and treat the `b/` namespace as the terminal miss.

### F2. `class-shrink` globs: `*` is literal in a pattern that also has `**` — confirmed

`tools/class-shrink/src/keep.rs:286-310`. `kotlin/**/Intrinsics*` never matches. Only
`kotlin/**` is in use today; the first mixed pattern added will silently keep nothing.

### F3. A shared string-literal / member-descriptor constant is left un-rewritten — suspected

`tools/class-shrink/src/shrink.rs:103-107` skips a Utf8 that is a string constant and not a
known descriptor; javac dedupes identical Utf8s, so `static final String SIG = "(Lpicodroid/…;)V"`
sharing a slot with a method of that descriptor leaves the member descriptor unshrunk while
callers are rewritten: `NoSuchMethod` at run time. Add own-member descriptor indices to the
`descriptors` set.

### F4. `papk-pack` `unescape_string` keeps mid-string double quotes — confirmed divergence

`tools/papk-pack/src/res.rs:312-319`: only a fully quoted value is treated as quoted; aapt2
treats every unescaped `"` as a whitespace-preservation toggle and removes it.

### F5. `values-<q>` overrides never reach base layouts or aliases — design-level divergence

`tools/papk-pack/src/res.rs:1421-1432, 1499-1531`: layouts are compiled against the base values
and overrides are resolved at build time, so `values-land/dimens.xml` changes
`getResources().getDimension()` but not a layout's `@dimen/gap`. If accepted, warn when a base
layout references a name a variant redefines.

### F6. Smaller tooling items

- `papk-pack --repack` ignores `--activity-decl`, `--service` and `--theme` (the input's values
  win; `WELL_KNOWN_KEYS` lacks the three keys).
- `class-shrink mapping.rs` accepts duplicate `from` rows and unknown keys silently.
- `build_support/board_cfg.rs` compares boolean keys as strings with inconsistent defaults
  (`has_canvas = 0` turns Canvas on, `has_json = True` turns JSON off); one `bool_key` helper that
  panics on anything but `true`/`false`. `queue_depth = 0` in `[background_pool]` is accepted.
- `hil-run.sh:1187` `port=$(timeout 15 pdb devices | …)` under `set -e` kills the legacy no-fleet
  nightly on a slow scan; `pre-commit:790,812` writes `lane.status` with a truncate-then-write
  the poller can read empty.
- `papk-format` iterators (`AssetIter`, `ManifestIter`, `ClassIter`) are not fused: a `next()`
  after a truncated record can fabricate an entry. Every in-tree caller stops at the first `None`.
- `papk-format` docs drift: `lib.rs:39-40` byte ranges, the writer's manifest key list, the
  "legacy v1.0" wording for `assets_offset == 0`.
- `ResTable::overrides()` drops a malformed block and shifts later indices, so `best_first` and
  `Resolved::value_of` disagree on a corrupt table; validate blocks in `ResTable::parse`.

## Checked and found correct

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
