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
- A fragment as its own `LifecycleOwner` and `ViewModelStoreOwner` (`new ViewModelProvider(this)`
  in a fragment, `by viewModels()`); today the Activity is the one owner and a fragment has
  `getViewLifecycleOwner()` (FR-12).

## FR-12: `ViewModel` and `LiveData`

**Status: landed 2026-09-30; the ratchet accept is open.** `picodroid.lifecycle`: `ViewModel`,
`ViewModelProvider` (+ `Factory`), `ViewModelStore`, `ViewModelStoreOwner`,
`HasDefaultViewModelProviderFactory`, `LiveData`, `MutableLiveData`, `Observer`, `Lifecycle`,
`LifecycleOwner`; `Activity` implements the three owner interfaces and
`Fragment.getViewLifecycleOwner()` follows the view. Pure Java, no native arm. What differs from
Android is in [designs/fragments-2026-09.md](designs/fragments-2026-09.md) §5; `claudeusage`'s
pages are the first user ([designs/claudeusage-fragment-shape-2026-09.md](designs/claudeusage-fragment-shape-2026-09.md)),
and `fragmentdemo`'s `stepLifecycle` is the conformance script (22 checks, both shrink modes).

Measured 2026-09-30 with `parity-bench.sh --size-only`, base and change in one worktree with one
lockfile (FR-2's method):

| Board | Before | After | Delta |
|---|---|---|---|
| `testbench_rp2350` | 1,487,932 | 1,503,620 | +15,688 B |
| `testbench_rp2040` | 970,564 | 985,548 | +14,984 B |

RAM is unchanged at rest on both. The twelve class files are 12,133 B of it (member-stripped):
`LiveData` 4,958, `ViewModelProvider` 1,819, `Lifecycle` 1,297, `ViewModelStore` 1,174,
`LiveData$ObserverWrapper` 1,073, `MutableLiveData` 455, `ViewModel` 325,
`HasDefaultViewModelProviderFactory` 266, `ViewModelProvider$Factory` 209, `ViewModelStoreOwner`
208, `LifecycleOwner` 188, `Observer` 161. The rest is their link tables and the growth of
`Activity` (both boards) and `Fragment` / `FragmentManager` (RP2350 only).

**The RP2040 keeps them**, unlike the six fragment classes. `Activity` implements
`LifecycleOwner`, `ViewModelStoreOwner` and `HasDefaultViewModelProviderFactory`, so those three
must resolve wherever `Activity` does; `LiveData` and `ViewModel` are useful to an Activity-only
app; and the board has 193,844 B free in its 1152K program region since 2026-09-29. If that
region tightens again, `framework_class_excludes` can drop the seven non-owner classes (about
11 KB): `Activity` only names them inside `getLifecycle()` and `getViewModelStore()`.

Open: the size ratchet is at 0 % growth, so the nightly `size-ratchet` lane fails until the
+15 KB is accepted (FR-2's recipe, `size:` trailer).

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
