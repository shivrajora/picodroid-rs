# Fragments and `ViewPager2` — 2026-09

**Status: built 2026-09-27. `picodroid.app.Fragment` / `FragmentManager` / `FragmentTransaction` /
`FragmentFactory` and `picodroid.widget.ViewPager2` / `FragmentStateAdapter`, pure Java, on every
RP2350 board; `examples/fragmentdemo` and `examples/pagerdemo` pass in both shrink modes;
`claudeusage`'s four screens are fragments in a pager. Numbers below are from the sim on the
tree this doc landed with; the flash cost is in §7.**

## 1. Why

`claudeusage-android-shape-2026-09.md` row 1: one Activity with hand-rolled `Page` objects
swapped in a `FrameLayout`, where Android would use Fragments in a `ViewPager2`. The parity
roadmap had parked Fragments behind the resource system ("shape without substance" without
layouts and ids); T3.2 landed on 2026-09-19 and the first app that wanted them was already
written.

### Were Fragments added to Android for performance? No.

Android 3.0 (2011) introduced them for tablets: modular UI sections an Activity can combine or
swap at runtime. The docs today: "A Fragment represents a reusable portion of your app's UI. A
fragment defines and manages its own layout, has its own lifecycle … must be hosted by an
activity or another fragment"; "Fragments introduce modularity and reusability into your
activity's UI"; "fragments can be added, replaced, or removed. And you can keep a record of these
changes in a back stack that is managed by the activity"; "multiple instances of the same
fragment class within the same activity, in multiple activities, or even as a child of another
fragment". No performance claim is made. The cost benefits that came with the model are
practical: a fragment swap stays inside one Activity (no ActivityManager round trip, no new
window), `onDestroyView` lets off-screen content drop its view hierarchy while the logic object
and its saved state stay, and `ViewPager2`'s `FragmentStateAdapter` keeps only nearby pages
alive.

### Each reason, and how it holds here

| Android reason | picodroid | Proven by |
|---|---|---|
| A modular, reusable UI piece with its own lifecycle, hosted by an Activity | `Fragment` with Android's callbacks in Android's order, driven from `Activity`'s `perform*` trampolines | `fragmentdemo`: the order against the host's, the same `HomeFragment` and `DetailFragment` re-hosted by a re-created Activity |
| A back stack inside one Activity | `addToBackStack` / `popBackStack`; the default `onBackPressed` pops before `finish()` | `fragmentdemo`: `input back` pops without `H1.onDestroy`; ten replace-and-pop rounds |
| A switch cheaper than an Activity switch | A transaction is Java only: no Rust pending op, no LVGL root swap, no keypad-group push, no Intent, no reclaim pass | `fragmentdemo` logs `replace took 0–1 ms` against `activity round trip took 54–67 ms` (sim; the round trip includes a reclaim and a re-creation) |
| Off-screen content frees its views, keeps its state | `onDestroyView` ends in `removeView` (the LVGL subtree freed); the pager keeps one page alive, the others as Bundles | `pagerdemo`: `page 0 destroyed` on the turn, `visits` back on return; `fragmentdemo`: `container.getChildCount()` 1 → 0 → 1 |
| State survives the host's destruction | Per-fragment `onSaveInstanceState` nested under `android:support:fragments`; a `FragmentFactory` re-instantiates | Both demos under `PICODROID_DONT_KEEP_ACTIVITIES=1`: arguments, saved ints, tags, the back stack and the pager's page come back |

## 2. What an app writes

```java
public class DetailFragment extends Fragment {
  public DetailFragment() { super(R.layout.fragment_detail); }
  @Override public void onViewCreated(View view, Bundle saved) { … view.findViewById(R.id.title) … }
}

getSupportFragmentManager().beginTransaction()
    .replace(R.id.container, new DetailFragment(), "detail")
    .addToBackStack(null)
    .commit();

pager.setAdapter(new FragmentStateAdapter(this) {
  @Override public int getItemCount() { return 4; }
  @Override public Fragment createFragment(int position) { return PageFragment.newInstance(position); }
});
pager.registerOnPageChangeCallback(new ViewPager2.OnPageChangeCallback() {
  @Override public void onPageSelected(int position) { dots.select(position); }
});
```

The API surface is the compatibility matrix's two rows and `api/ui.md`'s two sections. Every
deviation is listed in §5.

## 3. How it works

**Host wiring, no Rust.** `Activity` keeps a lazily created `FragmentManager` (`mFragments`,
made on the first `getSupportFragmentManager()`, so an app without fragments never resolves the
class — which is what lets the RP2040 testbench leave the classes out) and an `int` host state.
The `perform*` trampolines Rust already calls do the dispatch where androidx's `FragmentActivity`
does it relative to an override that calls super first: base `onCreate` restores the saved
fragments and moves them to CREATED (so a `commit()` + `executePendingTransactions()` in the
app's `onCreate` body creates a fragment at once, as on Android); `performCreate` repeats
`dispatchCreate` idempotently after the body; `performStart` starts the fragments before
`onStart`; `performResume` resumes them after `onResume` (Android's `onPostResume`); pause,
stop and destroy dispatch before the matching callback; `performSaveInstanceState` appends the
manager's Bundle after the app's `onSaveInstanceState`; `onBackPressed` pops first.

**The state machine** (`FragmentManager`). Fragment states are `int`s: `INITIALIZING 0, ATTACHED
1, CREATED 2, VIEW_CREATED 3, STARTED 4, RESUMED 5`. A fragment's expected state is
`min(hostState, maxLifecycle)`, capped at CREATED when it is not added (detached, or waiting in
the back stack) and at INITIALIZING when it is removed and in no back stack entry. A sweep
(`moveAllToExpectedState`) walks `mActive` backwards moving fragments **down** first, then
forwards moving them **up** — so within a `replace` the outgoing page's widgets are freed before
the incoming page's are allocated, the lowest LVGL-pool peak. Each step is one callback:
`onAttach` → `onCreate(saved)` → create the view (`findViewById(containerId)` on the host,
`onCreateView(inflater, container, saved)`, `container.addView(view, layoutParams)`, `GONE` if
hidden, `onViewCreated`, `onViewStateRestored`) → `onStart` → `onResume`, and down `onPause` →
`onStop` → `onDestroyView` then `parent.removeView(view)` (or `close()` when parentless) →
`onDestroy` → `onDetach`. A callback that throws leaves the fragment at the step it reached; the
`executing` guard is reset in `finally`, so a failing transaction never wedges the next.

**Transactions and the back stack** (`FragmentTransaction`). Ops are Android's numbers in
parallel `int[]` / `Fragment[]` arrays (`cmd`, `arg`, `old`), three ints per op. `replace` is
expanded at execution into the removes it implied plus its add and the expansion is stored back,
so a pop reverses exactly what ran; `setMaxLifecycle` records the previous cap in `old`. The
committed transaction *is* the back stack record (androidx's `BackStackRecord`, one class file
instead of two). `commit()` appends to the manager's pending list and posts one drain Runnable
to the main executor — one post per commit and no "posted" flag, because the 64-deep main queue
drops a post when full, and the next commit, lifecycle move or `executePendingTransactions()`
drains in order anyway. `commitNow()` runs the record directly. Both refuse after the host saved
its state and from inside a callback, with Android's messages. A pop reverses the ops of the
popped records last first, then sweeps.

**Save and restore.** `saveAllState` writes, per savable fragment, `cls` (`getClass().getName()`
— shrink-mapped when shrunk), `who` (the manager's int id), `cid`, `tag`, `flags`, `max`, `args`
and the `onSaveInstanceState` Bundle; the added list as whos; and each back stack record as
`name` plus an `int[]` of `(cmd, who, arg, old)`. A fragment added without a container whose
view someone else placed (a pager page) is **not** saved: restored, it would get a view nobody
attaches; its owner keeps its state (§4). `restoreSaveState` runs in base `onCreate` through the
`FragmentFactory` the app installed **before** `super.onCreate`; without one it logs a warning
and restores nothing (Android's default factory reflects on the class name; there is no
reflection here, and a thrown exception would leave the app's `onCreate` body unrun and the
screen blank). A back stack entry naming a fragment that did not come back ends the restored
stack there.

## 4. `ViewPager2` and `FragmentStateAdapter`

A `FrameLayout` subclass, `<ViewPager2>` in layouts (class code 18: one `case` in
`LayoutInflater`, one constant and one `ALL` tuple in `papk-format`, checked by papk-pack's
`codes_match_the_java_sdk`). The History page of `claudeusage` alone is 243 KB of the RP2350's
372 KB arena, so the pager keeps **one page alive** and turns in three main-thread ticks, the
split the app already used: *unbind* (the adapter saves the outgoing fragment's state under its
item id and removes it — `onDestroyView` runs on the live tree, then the manager frees it through
the pager, its parent), *bind* (the adapter's `createFragment`, `setInitialSavedState` if a
Bundle was kept, a container-less `add(f, "f" + id)` capped at STARTED, then the pager adds the
view with its `LayoutParams` and registers its swipe listener on it), *promote* (the cap raised
to RESUMED, `onPageSelected`, then the fade or the immediate `onPageScrolled(i, 0f, 0)` +
IDLE). A `setCurrentItem` during a turn retargets it (the old build token). `smoothScroll` is a
fade of the incoming page, LVGL having no scroller for this and the outgoing page being gone a
tick earlier; the state goes SETTLING → IDLE and DRAGGING is never reported.
`setOffscreenPageLimit` stores the value and logs once. Swipes: the pager and every page root
carry the `OnSwipeListener`. Registering one clears LVGL's `GESTURE_BUBBLE` on that view, so a
gesture that starts anywhere over the page, a clickable child included, climbs to the page root
and stops there (FR-4 in the follow-ups: before 2026-09-28 the flag was left set and every gesture
went to the screen). `saveState()` / `restoreState(Bundle)` hold the page index and the adapter's per-page
Bundles; the Activity calls them, there being no view-state hierarchy to do it.

## 5. Deviations from Android, all deliberate

- A view given up in `onDestroyView` is freed at once and never re-added (`ViewGroup.removeView`
  frees; Android keeps detached trees).
- `int` lifecycle states, `setMaxLifecycle(Fragment, int)`; no `Lifecycle.State` enum (~1 KB of
  flash for the enum alone, per the Canvas measurements).
- `setInitialSavedState(Bundle)` and `saveFragmentInstanceState → Bundle`; no `SavedState`.
- `FragmentFactory.instantiate(String)`; no `ClassLoader`, no default reflective factory; without
  a factory nothing is restored, with a warning.
- Views append to their container in the order fragments reach `VIEW_CREATED`; no
  `addView(child, index)`.
- Pager: one page alive, fade for `smoothScroll`, no DRAGGING, one `onPageScrolled` per turn,
  `saveState` / `restoreState` explicit, `setAdapter(FragmentStateAdapter)` (no
  `RecyclerView.Adapter`), page fragments not saved with the Activity's other fragments.
- Not provided: child fragment managers, `startActivityForResult` on a fragment, animations and
  transitions, `setRetainInstance`, menus, the Fragment Result API, page transformers, fake
  drags, `android:orientation` on the pager.
- A lifecycle override that skips `super` is tolerated; Android's `FragmentManager` throws
  `SuperNotCalledException`. Decided 2026-09-30: `Activity` already tolerates a skipped
  `super.onCreate` (`performCreate` covers it), the base callbacks are empty, and enforcing it
  for fragments alone would cost a flag and a check per callback for an inconsistency. Every
  fragment in `examples/` calls `super` since that date, so the examples are valid Android.
- `ViewModel` and `LiveData` (2026-09-30, `picodroid.lifecycle`): `Fragment.getViewLifecycleOwner()`
  is there, created on first use and one per view; the fragment itself is not a `LifecycleOwner`
  or a `ViewModelStoreOwner`, so a ViewModel is shared through
  `new ViewModelProvider(requireActivity())`. The Activity is both owners. A ViewModel lives as
  long as its Activity instance (no configuration changes), there is no reflective default
  factory (`Activity.getDefaultViewModelProviderFactory()` is the override point, as
  `FragmentFactory` is for fragments), and `Lifecycle` states are `int`s with no observer API
  beyond `LiveData`. Cost and the RP2040 decision: FR-12 in
  [`../fragments-follow-ups.md`](../fragments-follow-ups.md).

## 6. `claudeusage` on it

`Page` became `UsagePage extends Fragment`: `onAttach` takes the palette from the host,
`onCreate` resolves the title, `onCreateView` returns the page's empty root at once,
`onViewCreated` starts the `buildNext` chain on the main executor with every post guarded by
`isAdded()` and `getView()` (the build token, in the Android idiom), the first paint waits for
data and ends in the page's own fade, and `onUsage` carries the minute/snapshot/freshness diff
that `updatePage` had. The four pages are the same classes minus their constructors;
`UsagePagerAdapter` is the old `create(int)` switch; the status screen is a fragment
`replace()`d into `page_host` over the pager. `MainActivity` lost `showPage` … `discardPrebuilt`
(some 200 lines): keys and AUTO call `pager.setCurrentItem(…, false)`, the dots move from
`onPageSelected`, the prebuilt page is gone because the pager's first page is always built
underneath the status screen, and `warmChrome` runs once the first page is built.

## 7. Cost

Measured 2026-09-27 with `parity-bench.sh --size-only` on the development machine, against a
baseline rebuilt on the same machine from the tree this landed on (the committed ratchet was
measured with another toolchain: main alone reads 1,976 B larger here on the RP2040, so the
deltas below are the real ones and the ratchet's absolute numbers are not):

| Board | Base here | With fragments | Delta | Of which |
|---|---|---|---|---|
| `testbench_rp2350` | 1,314,260 (committed) | 1,353,968 | ≈ +37.7 KB (39,708 against the committed number) | the six stripped classes 36.3 KB: `FragmentManager` 12.4, `ViewPager2` 7.9, `FragmentTransaction` 6.5, `Fragment` 4.7, `FragmentStateAdapter` 4.2, `FragmentFactory` 0.6; `Activity` +1.3 |
| `testbench_rp2040` | 909,784 | 911,208 | +1,424 | `Activity`'s hooks and fields, `LayoutInflater`'s case; the classes are excluded, and the name vocabulary costs nothing (unreferenced constants) |

The class files came out about twice the roadmap's "1.5–3 KB per class" rule: a class with
forty methods is mostly constant pool. What would cut it, in order: shorter exception strings,
fewer convenience accessors (`require*`, `getString`, `startActivity`), and one `findFragment`
loop instead of two. RAM zero at rest: nothing is allocated on a lifecycle sweep; a fragment is
about 14 fields, a manager four `ArrayList`s and a dozen fields, a transaction two small arrays.

The RP2040 sits 6,040 B under its linker ceiling after this and above the ratchet's hard gate
(`G1_HARD` = 908,000, a deliberate reserve) — which main already crosses on this toolchain. The
accept, and whether the reserve moves, is a decision for the accept commit, not this doc.

## 8. Verification

- `sim-run` rows, both shrink modes (the shrink lane is what proves the `X.class.getName()`
  factory idiom): `fragmentdemo` (76 checks, `pico_display2_w` for the two BACKs from
  test.ctrl), `pagerdemo` (recreate and reclaim restores), `claudeusage` (status screen, B twice
  on it).
- The papk-pack drift test for class code 18; the name-table currency tests after
  `gen-api-contract.sh`.
- A swipe turning the pager: at first not verified, and in fact broken on every board (the
  gesture went to the screen). Fixed and covered on 2026-09-28: `pagerdemo`'s row swipes both
  ways, and so did `pico_touch_kit` over `pdb` (FR-4).

## 9. Found on the way

- A `picodroid.concurrent.Thread` sleeping across an Activity reclaim (`sleep` then post to the
  main executor, the `qa_life` `T.later` helper) died with `Thread.start: … left the interpreter:
  InvalidReference`. The demos use the main-thread `ScheduledExecutorService` instead, or
  `finish()` straight from `onResume` as `reclaimdemo` does. Open JVM question.
- `input back` / `input keyevent` are refused on `testbench_rp2350` (no buttons): a key-driven
  sim row pins `pico_display2_w`.

## 10. Not doing, and follow-ups

Child fragment managers (nested fragments) when an app needs a pager inside a fragment;
`addView(child, index)` for the z-order of several fragments in one container; `Lifecycle.State`
and `Fragment.SavedState` if an app needs source-identical Android code; a bridge-backed nightly
row for `claudeusage` that turns real pages. Tracked, with status, as
FR-1 to FR-11 in [`../fragments-follow-ups.md`](../fragments-follow-ups.md).
