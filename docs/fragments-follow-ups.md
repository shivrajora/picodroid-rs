# Follow-up backlog: Fragments and `ViewPager2` — 2026-09-27

Android's fragment model landed on `main` on 2026-09-27 (`508e1933`: `picodroid.app.Fragment`,
`FragmentManager`, `FragmentTransaction`, `FragmentFactory`, `picodroid.widget.ViewPager2`,
`FragmentStateAdapter`, the `<ViewPager2>` layout tag, the `fragmentdemo` and `pagerdemo` rows;
`82068a59`: `claudeusage`'s four screens as fragments in a pager). Pure Java over the
Activity's existing trampolines; sim-verified in both shrink modes; design, every deviation and
every number in [designs/fragments-2026-09.md](designs/fragments-2026-09.md).

What remains is **follow-up work, not blockers**: each item below is self-contained, with its
evidence and where to start. Status lines are kept here as items close.

Completed items: [completed/fragments-follow-ups.md](completed/fragments-follow-ups.md) — FR-1 to FR-8.

## FR-9: What the fragment model still lacks

**Status: open, by demand.** Each of these costs flash on every RP2350 board (the RP2040
testbench excludes the six classes), and FR-2's per-class numbers say to measure before adding:

- `getChildFragmentManager` (nested fragments: a pager inside a fragment).
- `ViewGroup.addView(child, index)`, for the z-order of several fragments in one container;
  today add order is z-order and a popped fragment comes back on top.
- `onInterceptTouchEvent` in general. A swipe over a clickable child already reaches the pager
  (FR-4, [completed](completed/fragments-follow-ups.md)); only `SwipeRefreshLayout`'s pull-down is intercepted.
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
