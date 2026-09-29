# Follow-up backlog: Fragments and `ViewPager2` — 2026-09-27

Android's fragment model landed on `main` on 2026-09-27 (`508e1933`: `picodroid.app.Fragment`,
`FragmentManager`, `FragmentTransaction`, `FragmentFactory`, `picodroid.widget.ViewPager2`,
`FragmentStateAdapter`, the `<ViewPager2>` layout tag, the `fragmentdemo` and `pagerdemo` rows;
`82068a59`: `claudeusage`'s four screens as fragments in a pager). Pure Java over the
Activity's existing trampolines; sim-verified in both shrink modes; design, every deviation and
every number in [designs/fragments-2026-09.md](designs/fragments-2026-09.md).

What remains is **follow-up work, not blockers**: each item below is self-contained, with its
evidence and where to start. Status lines are kept here as items close.

Completed items: [completed/fragments-follow-ups.md](completed/fragments-follow-ups.md) — FR-3, FR-5, FR-7, FR-8.

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
(FR-5; closed since, [completed/fragments-follow-ups.md](completed/fragments-follow-ups.md)).

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

## FR-4: A swipe turning the pager

**Status: closed 2026-09-28.** The swipe did not work on any board, sim or device, and nothing
was wrong with the injection. LVGL 9 sets `LV_OBJ_FLAG_GESTURE_BUBBLE` on every object that has a
parent, so a gesture climbs from the pressed object to the screen and only the screen receives
`LV_EVENT_GESTURE`. No `OnSwipeListener` below the screen was ever called, and neither was the
`SwipeRefreshLayout` pull-down. `pagerdemo` on `pico_touch_kit` shows the same: with the old
code, `pdb input swipe 220 100 40 100 150` over the page does nothing.

- **Fix.** Registering a swipe listener now clears the flag on that view (`events/swipe.rs`). A
  swipe that starts on the view, or on a descendant that still bubbles, stops there: the nearest
  view with a listener, as on Android. `SwipeRefreshLayout`'s container clears it too. The
  container is also no longer scrollable: the theme's border gave its full-size child 4 px of
  overflow each way, and every drag past the scroll limit became an elastic scroll before the
  50 px gesture distance was reached. That is why `swipedemo` stayed silent even for a gesture
  aimed at the layout itself. A pull-down that lands on a descendant with its own listener goes to
  the enclosing `SwipeRefreshLayout` first (`intercept`, Android's `onInterceptTouchEvent`); every
  other direction stays with the child.
- **Evidence.** In the sim, the `pagerdemo` row gains a swipe step: its `test.ctrl` swipes left
  (page 1) and right (page 0 again). A new `swipedemo` row checks left, right, pull-down
  (`refresh 1`, not `swipe down`) and up. Both rows pass in both shrink modes, and so do
  `fragmentdemo` and `claudeusage`. On `pico_touch_kit`, `pagerdemo` built with
  `PICODROID_DONT_KEEP_ACTIVITIES=1` and driven by `pdb input swipe` reached `=== ALL PASSED ===`.
  The same build without the fix stayed on page 0. The board swipes were injected through the
  GT911 touch override, not made with a finger.
- **Consequence for FR-9.** A swipe that starts on a clickable child now reaches the pager too:
  the child is only LVGL's pressed object, and the gesture bubbles up from it to the page root's
  listener. `LV_OBJ_FLAG_EVENT_BUBBLE` is not needed for that. The one case left is a child with a
  swipe listener of its own, which keeps the swipe, as on Android.

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

## FR-9: What the fragment model still lacks

**Status: open, by demand.** Each of these costs flash on every RP2350 board (the RP2040
testbench excludes the six classes), and FR-2's per-class numbers say to measure before adding:

- `getChildFragmentManager` (nested fragments: a pager inside a fragment).
- `ViewGroup.addView(child, index)`, for the z-order of several fragments in one container;
  today add order is z-order and a popped fragment comes back on top.
- `onInterceptTouchEvent` in general. A swipe over a clickable child already reaches the pager
  (FR-4); only `SwipeRefreshLayout`'s pull-down is intercepted.
- `Lifecycle.State` and `Fragment.SavedState` as types, if an app needs source-identical
  Android code; today `setMaxLifecycle` takes an `int` and saved state is a `Bundle`.
- `setOffscreenPageLimit(n ≥ 1)` honoured: it is stored and logged, one page stays alive.
- `startActivityForResult`, `setRetainInstance`, transitions and menus.

## FR-10: A bridge-backed nightly row that turns real pages

**Status: open.** The sim row proves the status screen only (`discovery: failed` is one of its
patterns). A row whose bridge `sim-run` starts itself (as `net-lib.sh::start_net_listeners`
starts the TLS listener), with a `test.ctrl` that waits for data and presses B four times and
patterns `page -> Models`, `page -> Burn rate`, `page -> History`, `page -> Limits`, would
exercise the port every night; FR-8's `heapcensus` numbers ([completed/fragments-follow-ups.md](completed/fragments-follow-ups.md)) could be asserted there too.

## FR-11: Keys on a board without buttons

**Status: open, small.** `input back` and `input keyevent` are refused on `testbench_rp2350`
(`no buttons on this board`: the verb resolves keycodes to pins on the device side, as
`pdb input` does), which is why `fragmentdemo` pins `pico_display2_w`. Android's
`input keyevent` works on any device. A simulator-only fallback that delivers the `KeyEvent`
to the foreground Activity when the board has no pin for it would let key-driven rows run on
the default board; it would be the one place the sim's input path differs from the device's,
so say so in the control channel's help text if it is added.
