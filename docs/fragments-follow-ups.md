# Follow-up backlog: Fragments and `ViewPager2` — 2026-09-27

Android's fragment model landed on `main` on 2026-09-27 (`508e1933`: `picodroid.app.Fragment`,
`FragmentManager`, `FragmentTransaction`, `FragmentFactory`, `picodroid.widget.ViewPager2`,
`FragmentStateAdapter`, the `<ViewPager2>` layout tag, the `fragmentdemo` and `pagerdemo` rows;
`82068a59`: `claudeusage`'s four screens as fragments in a pager). Pure Java over the
Activity's existing trampolines; sim-verified in both shrink modes; design, every deviation and
every number in [designs/fragments-2026-09.md](designs/fragments-2026-09.md).

What remains is **follow-up work, not blockers**: each item below is self-contained, with its
evidence and where to start. Status lines are kept here as items close.

## FR-1: Push to origin and the first nightly

**Status: closed 2026-09-27.** Pushed with v0.35.0; CI green on `5dadeb74`, and the
2026-09-27 afternoon sim matrix (`a66329d2`) passed `fragmentdemo`, `pagerdemo` and
`claudeusage` in both shrink modes. As written: `main` was four commits ahead of `origin/main` (the TLS
backlog, the two fragment commits, this file); the repo rule is push only when asked. After the
push, `gh run list --limit 3` shows CI (the two new example APKs build there). The 3 AM `sim-run`
gains the `fragmentdemo` row (pinned to `pico_display2_w` for the two BACKs its `test.ctrl`
sends), the `pagerdemo` row and the changed `claudeusage` row, in both shrink modes; the shrink
lane is what proves the `X.class.getName()` factory idiom night after night. Expect two known
reds on the first morning: the size-ratchet lane (FR-2) and the `claudeusage` row's TIMED OUT
(FR-5).

## FR-2: Accept the flash, and decide the RP2040 reserve

**Status: closed 2026-09-27, by `5dadeb74`.** Accepted at `testbench_rp2350` +39,820 B; the
`testbench_rp2040` came in 3,984 B *smaller* (903,824 B, the debug-profile trims of `a66329d2`),
under `G1_HARD`, so the reserve did not have to move. The `size-ratchet` lane passes. As
written: the nightly `size-ratchet` lane fails at 0 % growth from the
first night after the push. Measured 2026-09-27 with `parity-bench.sh --size-only` against a
baseline rebuilt on the same machine ([designs/fragments-2026-09.md](designs/fragments-2026-09.md)
§7):

| Board | Committed baseline | Base rebuilt here | With fragments | Real delta |
|---|---|---|---|---|
| `testbench_rp2350` | 1,314,260 | — | 1,353,968 | ≈ +37.7 KB (the six classes 36.3 KB, `Activity` +1.3 KB) |
| `testbench_rp2040` | 907,808 | 909,784 | 911,208 | +1,424 B (`Activity`'s hooks, `LayoutInflater`'s case; the classes are excluded) |

Two things make the accept a decision rather than a command. The committed baseline came from
another toolchain: `main` before fragments already reads 1,976 B larger on the RP2040 here, and
the ratchet's hard gate `G1_HARD` (908,000 B, `scripts/bench-report.py`, a deliberate reserve
under the 917,248 B program region) is therefore crossed by `main` itself on this machine; with
fragments the RP2040 sits 6,040 B under the linker ceiling. So either the gate moves in the same
commit, or the accept is measured on the toolchain the baseline used (the `rustc` pin named in
`bench/parity/ratchet.toml`'s header). The accept itself:

```bash
D=$(mktemp -d)
PICODROID_SIZE_RUN_DIR=$D ./scripts/parity-bench.sh --size-only \
  --boards testbench_rp2040,testbench_rp2350
./scripts/bench-report.py --ratchet --sizes-from "$D" --accept   # rewrites ratchet.toml
```

committed with a `size: +N B flash on <board>` trailer, on a clean tree (the measurement is of
the working tree). What would give bytes back first, if the RP2350 growth is judged too much:
shorter exception strings in `FragmentManager` and `FragmentTransaction`, fewer convenience
accessors on `Fragment` (`require*`, `getString`, `startActivity`), one `findFragment` loop
instead of two. The classes came out at about twice the parity roadmap's 1.5–3 KB-per-class rule:
a class with forty methods is mostly constant pool.

## FR-3: Cut the next shrink map

**Status: closed 2026-09-27.** Map v0.35.0, cut on `main` together with TLS-3 for the v0.35.0
release, names the Fragment and `ViewPager2` classes and their members.

## FR-4: A swipe turning the pager

**Status: open, two parts.** `ViewPager2` registers its `OnSwipeListener` on itself and on
every page root (`SWIPE_LEFT` / `SWIPE_UP` next, `SWIPE_RIGHT` / `SWIPE_DOWN` previous, no wrap,
nothing when user input is disabled), and no run has exercised it.

- **Hardware.** Flash `pagerdemo` on `pico_touch_kit` (GT911 touch) and swipe left over the
  page, or drive it with `pdb input swipe 220 100 40 100 150`: expect `[PagerDemo] selected 1`
  then `page 0 destroyed`. `claudeusage` calls `setUserInputEnabled(false)` (buttons only), so
  `pagerdemo` is the check.
- **Simulator.** `input swipe x1 y1 x2 y2 ms` on the control channel (the `"swipe"` arm of
  `handle_input_command` in `crates/picodroid-core/src/hal/sim/display.rs`, an interpolated
  touch drag) raised no LVGL gesture on 2026-09-27: a 40 ms drag, a 300 ms drag and a
  hand-stepped `touch down`, five `touch move`s and `touch up` all left `swipedemo`'s listener
  silent, while the verb's own commit record says `gesturedemo` saw swipes when it was added.
  Bisect: run `gesturedemo` under the same drag (if it fires, the difference is the
  `OnSwipeListener` path, not the injection); compare the drag's per-read step against LVGL's
  gesture limit and minimum velocity in `lv_conf.h` and the pointer indev's read period. Once it
  fires, restore `pagerdemo`'s swipe step (`swipe -> 1` in the row, `testbench_rp2350` for its
  touch panel) and its `test.ctrl`.

## FR-5: The `claudeusage` row reads TIMED OUT until `sim-run` kills on match

**Status: closed 2026-09-27.** The `claudeusage` row (`sim`, 60 s, `pico_display2_w`) matches
all four patterns (`ui ready`, `page -> Claude usage`, `discovery: failed`, `state -> …`) but its
Activity never exits, and `sim-run.sh` used to let the deadline expire and call that a failure.
`d8339e1e` (the 2026-09-27 nightly fixes, merged as `f6f9aee6` while this backlog was being
written) stops a row's app once every pattern has matched (`patterns matched; stopping the
app`); re-run on that tree, the row is PASS in both shrink modes.

## FR-6: `./scripts/test.sh` does not compile on `main`

**Status: closed 2026-09-27, by `a66329d2`.** `kick()`'s call is now
`#[cfg(all(feature = "sim", network_link_wifi))]` (the second option below), so the host tests
compile without a Wi-Fi link. As written: since `171070fc` (2026-09-27, Wi-Fi provisioning), which is on
`origin/main`:

```text
error[E0433]: cannot find `wifi` in `sim`
   --> crates/picodroid-core/src/hal/wifi.rs:630:22
    |
630 |     crate::hal::sim::wifi::service();
    |                      ^^^^ could not find `wifi` in `sim`
note: found an item that was configured out
   --> crates/picodroid-core/src/hal/sim/mod.rs:92:9
error: could not compile `picodroid-core` (lib test) due to 1 previous error
```

`hal/mod.rs` compiles `hal::wifi` under `any(test, network_link_wifi)`, so the host tests see
it, but `hal/sim/mod.rs` gates `hal::sim::wifi` on `all(network_link_wifi, feature = "sim")`,
so `kick()`'s `#[cfg(feature = "sim")]` call has no target under `cargo test` without a Wi-Fi
link. Either gate the sim module on `all(any(test, network_link_wifi), feature = "sim")` (it
must then build with no board link at all) or make the call
`#[cfg(all(feature = "sim", network_link_wifi))]` with an empty arm otherwise. CI runs the tests
in both shrink modes: the `Testing` job of the CI run for `67b75cda` on `origin/main` is red
(its `Linting`, `Building` and framework sim-smoke jobs are red too; TLS-1 covers that run). Unrelated to
fragments; it blocked the unit-test lane of this round (the name-table and papk-pack tests were
run with `cargo test -p` directly).

## FR-7: A thread sleeping across an Activity reclaim dies with `InvalidReference`

**Status: closed 2026-09-27, by `d8339e1e` as far as the repro shows.** Found writing
`fragmentdemo` on `f1803997`: a covering Activity whose `onResume` started a
`picodroid.concurrent.Thread` that slept 150 ms and then posted `finish()` to the main executor
(the `qa_life` `T.later` helper) never finished; the log ended with
`Thread.start: picodroid/concurrent/Thread left the interpreter: InvalidReference` right after
the covered Activity was reclaimed under `PICODROID_DONT_KEEP_ACTIVITIES=1`, and the same helper
worked when no reclaim happened mid-sleep. The demos were written around it (a
`ScheduledExecutorService`, or `finish()` straight from `onResume` as `reclaimdemo` does). Put
back into `SecondActivity.onResume` on the merged tree, the thread runs to its `finish()` (the
Activity round trip took 219 ms, against 54–67 ms with an immediate `finish()`) and the
`fragmentdemo` row passes in both shrink modes with no such line. `d8339e1e`'s thread-slot fix
fits the shape: a finished thread's freed slot was the next one `Thread.start` handed out, and
the first thread's trailing cleanup then took the newcomer's entry — a timer thread ending
while the reclaim's own thread traffic reuses slots is exactly that. If the line comes back,
start there (`crates/picodroid-core/src/threads.rs`, `terminate_by_obj`), then the parked-frame
rooting in [designs/jvm-run-lock-2026-09.md](designs/jvm-run-lock-2026-09.md).

## FR-8: The hardware run and the budget checks

**Status: closed 2026-09-27, on `5dadeb74`.** Results below the plan. The "before" build is
the pre-fragment app (`examples/claudeusage` at `f1803997`) on today's firmware, not the
`f1803997` firmware; the fragment classes are all the app side can reach, so the difference is
the port.

- **Board.** `pico_display2_w` (debug build) with the live bridge: `discovery: found`, then 8 B,
  4 A and 6 B presses 0.6 s apart. Every press delivered in order, each with its `page ->` and
  `built`. Y went home, a long Y turned AUTO on, and AUTO turned about 7 pages. After a power
  cycle AUTO kept turning with no key pressed. There were no `slow handler` lines, no errors,
  and only the known benign `spi0: … 0 of 0 bytes` warning.
- **Memory.** Sim `--mem-diag`, 30 AUTO turns, with a fixed recorded payload replayed on port
  8791 for both builds. From turn 2 to turn 30 the new build's live heap went 16,914 → 17,244 B
  and the old build's went 15,884 → 16,214 B. That is the same +330 B in both: +8 objects,
  3 classes parsed late. The LVGL pool stayed flat in both (20,288 B used new, 19,848 B old).
  After the first minute `nused` held flat at about 271 KB new and 256 KB old. Most of the
  +15 KB is class metadata (parsed 75,974 against 64,340 B). The live Java heap is only +1 KB.
  There was no `OOM: tried` line. One difference: the native low-water mark is 30 KB lower,
  122,600 B free against 152,784. It dropped once, on one Limits → Models turn, as the
  object/field storage grew a chunk; it did not recur over the remaining turns. The census
  shows no `Bundle` among the top classes.
  `fragmentdemo`: the LVGL pool read 7,912 → 7,928 B across the pops and the re-created
  Activity, and 6,336 B at the end. Its final heap delta was −776 B.
- **Page-turn budget.** Sim `--sched-diag` with `PICODROID_TRACE_SPANS=1`, 30 AUTO turns:
  0 `slow handler` lines in both builds, and the same three `HOG fs` findings (16–18, 9 and 9 ms,
  LittleFS) in both. Mean idle was 98.8 % in both. Every span was ≤ 1 ms (79 at 1 ms new,
  63 old), so the host is too fast to tell the two apart. The board run above is the budget
  evidence.
- **Pixels.** Screenshots rather than `fbhash` (Xvfb, `scrot`, PIL diff), because band hashes
  from the fades are partial rectangles that do not compose into a final frame. All five page
  shots (Limits, Models, Burn rate, History, Limits) are byte-identical between the builds,
  including the header. The status screen (bridge down) differs only in the retry countdown
  digit (14 s against 15 s). One logging difference: the new build logs `page -> Limits`
  before `page -> Claude usage`, because the pager builds page 0 under the status screen (§6
  of the design); nothing about it is visible.
- **Cost.** `fragmentdemo` with `PICODROID_SLOW_HANDLER_MS=10`, and again at 1 ms: no line.
  Traced, 115 of 118 spans were 0 ms. The rest were the 9 ms Runnable that runs the demo's 11
  replace-and-pop rounds (about 0.8 ms a round), a 2 ms drain for the Activity re-creation and
  one 1 ms Runnable. The logs showed `replace took 0 ms` and `activity round trip took 67 ms`.

Not run: `heapcensus` on the board, and an `fbhash` comparison on the device.

The plan, as written:

- **`claudeusage` on the board.** `flash.sh --board pico_display2_w --app claudeusage` with a
  live bridge on the LAN: the four screens on B, B held every half second, Y home, Y long for
  AUTO, a power cycle keeping AUTO; no `slow handler` line on a turn. D4 of the gaps roadmap
  (every tick of a page swap overran the budget; closed 2026-09-25) must still hold now that a
  turn is the pager's three ticks (unbind, bind, promote) plus the page's own build chain.
- **Memory.** `python3 examples/claudeusage/bridge/claude_usage_bridge.py --demo`, then
  `./scripts/sim.sh --board pico_display2_w --app claudeusage --mem-diag`, AUTO on
  (`sim-ctrl.sh input keyevent --longpress 4`), `sim-ctrl.sh heapcensus` after turn 2 and turn
  30: live heap flat, no `OOM: tried`, only the four saved page Bundles new. In `fragmentdemo`,
  `sim-ctrl.sh memstats` after a `replace`: the LVGL pool regains Home's widgets.
- **Page-turn budget.** The same run with `--sched-diag`; `slow handler` count per turn against
  the pre-fragment build (`f1803997`).
- **Pixels.** Both builds with `PICODROID_EXTRA_FEATURES=parity-fbhash` and the demo bridge,
  `input keyevent 20` per page, `sim-ctrl.sh fbhash`: band CRCs in rows 26..216 identical for
  the four screens and the status screen.
- **Cost.** `PICODROID_SLOW_HANDLER_MS=10 ./scripts/sim.sh --app fragmentdemo`: no span per
  tick beyond what an Activity switch shows. The sim already measured a `replace` at 0–1 ms
  against 54–67 ms for an Activity round trip.

## FR-9: What the fragment model still lacks

**Status: open, by demand.** Each of these costs flash on every RP2350 board (the RP2040
testbench excludes the six classes), and FR-2's per-class numbers say to measure before adding:

- `getChildFragmentManager` (nested fragments: a pager inside a fragment).
- `ViewGroup.addView(child, index)`, for the z-order of several fragments in one container;
  today add order is z-order and a popped fragment comes back on top.
- `LV_OBJ_FLAG_EVENT_BUBBLE` so a swipe that starts on a clickable child reaches the pager;
  there is no `onInterceptTouchEvent`.
- `Lifecycle.State` and `Fragment.SavedState` as types, if an app needs source-identical
  Android code; today `setMaxLifecycle` takes an `int` and saved state is a `Bundle`.
- `setOffscreenPageLimit(n ≥ 1)` honoured: it is stored and logged, one page stays alive.
- `startActivityForResult`, `setRetainInstance`, transitions and menus.

## FR-10: A bridge-backed nightly row that turns real pages

**Status: open.** The sim row proves the status screen only (`discovery: failed` is one of its
patterns). A row whose bridge `sim-run` starts itself (as `net-lib.sh::start_net_listeners`
starts the TLS listener), with a `test.ctrl` that waits for data and presses B four times and
patterns `page -> Models`, `page -> Burn rate`, `page -> History`, `page -> Limits`, would
exercise the port every night; FR-8's `heapcensus` numbers could be asserted there too.

## FR-11: Keys on a board without buttons

**Status: open, small.** `input back` and `input keyevent` are refused on `testbench_rp2350`
(`no buttons on this board`: the verb resolves keycodes to pins on the device side, as
`pdb input` does), which is why `fragmentdemo` pins `pico_display2_w`. Android's
`input keyevent` works on any device. A simulator-only fallback that delivers the `KeyEvent`
to the foreground Activity when the board has no pin for it would let key-driven rows run on
the default board; it would be the one place the sim's input path differs from the device's,
so say so in the control channel's help text if it is added.
