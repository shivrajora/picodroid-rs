# `claudeusage` on Fragments: where it still is not Android (handover, 2026-09-30)

Status: **landed 2026-09-30**, see "What landed" at the end. Written after a review of `examples/claudeusage` on `main`
following `82068a59` (the four screens as Fragments in a `ViewPager2`). That refactor closed item 1
of [claudeusage-android-shape-2026-09.md](claudeusage-android-shape-2026-09.md) and shrank item 4.
It also introduced the six differences below. Each section gives the evidence, the Android shape,
the options and a recommendation. Sections 1 and 2 share one fix, which needs SDK work. Sections 3
and 6 are app-only and take minutes.

Background reading: [fragments-2026-09.md](fragments-2026-09.md) (the SDK's fragment model and its
deviations, §5 lists what is not provided), [../fragments-follow-ups.md](../fragments-follow-ups.md)
(FR-9: what the model still lacks; FR-2: measure the flash cost per class before adding one).

## The code in question

- [UsagePage.java](../../examples/claudeusage/java/claudeusage/ui/UsagePage.java): the abstract
  `Fragment` every screen extends. It owns the build-a-few-views-per-tick chain (`buildNext`,
  `paintNext`, `viewGen`, the OOM catch), which stays: it is the RP2350 tick budget (item 4 of the
  shape doc), not part of this handover.
- [MainActivity.java](../../examples/claudeusage/java/claudeusage/ui/MainActivity.java): the
  pager, the chrome, the Service binding, and `refresh()` / `updatePage()`, which feed the pages.
- [UsagePagerAdapter.java](../../examples/claudeusage/java/claudeusage/ui/UsagePagerAdapter.java):
  the `FragmentStateAdapter`.

## 1. The pages reach into the Activity through a cast

**Evidence.** `UsagePage.onAttach` does `host = (MainActivity) context`, and the page then calls
five package-private methods on it:

| call | where in `UsagePage` | what it is for |
|---|---|---|
| `host.palette()` | `onAttach` | colours resolved from `res/values` |
| `host.destroyed()` | `gone()` | stop a build chain after the Activity is gone |
| `host.onPageBuilt()` | `buildStep` | triggers `MainActivity.warmChrome()` once |
| `host.repo()`, `host.hasData()` | `firstPaint` | the data, and whether the first paint can run |
| `host.fadeMs()` | `firstPaint` | `@integer/fade_ms` |

**Android shape.** A Fragment does not know its Activity's concrete class. Shared data comes from
`ViewModelProvider(requireActivity())` / `activityViewModels()`, resources from `getResources()`,
and "the host is gone" from `isAdded()` / `getView() == null`. When a callback to the host is
really needed, it goes through an interface the Activity implements.

**Options.**

- **(a) App-only, today.** Most of the coupling needs no SDK work:
  - `palette`: `new Palette(getResources())` in the fragment, or a static cached in `Palette`.
    Mind the 2026-09-23 finding in the shape doc: every `Palette` field read from a new class
    costs resolution-cache entries.
  - `fadeMs`: `getResources().getInteger(R.integer.fade_ms)`.
  - `destroyed()`: `!isAdded()` already covers it, since the fragment is detached when the
    Activity is destroyed. Check that the SDK detaches on destroy before you rely on it.
  - `onPageBuilt()`: an interface (`PageHost`) or a callback the Activity registers.
  - `repo()` and `hasData()` are what is left. They go through the same interface, or wait for (b).
- **(b) SDK: a minimal `ViewModel` + `ViewModelProvider` + `LiveData` + `LifecycleOwner`**, with
  `Fragment.getViewLifecycleOwner()`. The fragments design doc lists `ViewModel` / `Lifecycle`
  owners as not provided. The SDK has no `LiveData`, `ViewModel` or `LifecycleOwner` class today.
  This also closes section 2 and shape-doc item 33 (the Service `Listener` with its 1 Hz `onTick`).

**Recommendation.** Do (a) for `palette`, `fadeMs` and `destroyed` now. Then do (b), and move
`repo` / `hasData` into a `UsageViewModel` that the Activity fills from the bound Service.
Before adding the classes, measure each one's flash cost on an RP2350 board (FR-2's method).
Decide whether the RP2040 testbench excludes them, as it does the six fragment classes.

## 2. The Activity pushes data into the visible page

**Evidence.** `MainActivity.refresh()` repaints the chrome. It then posts `updatePage()`, which
finds the visible fragment with `findFragmentByTag(statusShowing ? "status" : "f" +
pager.getCurrentItem())` (`visiblePage()`) and calls `page.onUsage(repo, now)` on it. The pages
never subscribe to anything.

**Android shape.** Each fragment observes the data itself, in `onViewCreated`:
`viewModel.usage.observe(getViewLifecycleOwner(), this::render)`. The Activity knows nothing about
which page is showing. Code that relies on `FragmentStateAdapter`'s `"f" + itemId` tag works on
Android but is a well-known anti-pattern, because the tag is an implementation detail. Note that
the SDK's own `FragmentStateAdapter` javadoc presents that lookup as the intended way, "as on
Android". If that is softened, fix the javadoc too.

**Fix.** It depends on 1(b). Once the ViewModel exists, `updatePage()` and `visiblePage()` go away,
and `UsagePage.onUsage` becomes the observer.

**Trap: the tick budget.** Today the chrome repaint (~20 ms on the RP2350) and the page repaint
(~30 ms) run on **separate** main-thread ticks. That is why `refresh()` posts `updatePage()`
instead of calling it (see the comment there and the D4 history in the gaps roadmap). Android's
`LiveData.setValue` dispatches synchronously to every active observer. If the Activity's chrome
observer and the page's observer fire in the same tick, the 50 ms combined overruns the
slow-handler budget again. Options:
- the page's observer posts its own repaint to the next tick;
- the chrome stays on the Service listener and only the page observes `LiveData`.

Keep the per-minute gating (`updatedSnapshot` / `updatedFresh` / `updatedMinute` in `UsagePage`)
whichever you pick. Re-check on hardware with the slow-handler warnings, not only in the sim.

## 3. `UsagePage` overrides lifecycle methods without calling `super`

**Evidence.** `UsagePage.onAttach`, `onCreate`, `onViewCreated` and `onDestroyView` never call
`super`. The concrete pages (`StatusPage`, `ModelsPage`, `BurnPage`, `HistoryPage`) do call
`super.onCreate`, but that lands in `UsagePage.onCreate`, which stops there. It works only because
the SDK's `Fragment` callbacks are empty (`sdk/java/picodroid/app/Fragment.java`: `onAttach`,
`onCreate`, … are `{}`).

**Android shape.** The `FragmentManager` sets a flag in each base callback and throws
`SuperNotCalledException` when an override skipped it. Code like this crashes at the first attach.

**Fix (app, now).** Add the four `super` calls at the top of each override.

**SDK question (decide, then do or record).** Should picodroid enforce the `super` calls?
`Activity` deliberately tolerates a skipped `super.onCreate` (`Activity.java`:
`dispatchCreate()` "covers an override that skipped super.onCreate"). Enforcing it for fragments
would be Android-faithful but inconsistent with `Activity`. It could also break other apps:
the only other `Fragment` subclasses are `examples/pagerdemo/.../PageFragment.java` and
`examples/fragmentdemo/.../HomeFragment.java` / `HeadlessFragment.java`, so check those. If the
answer is "tolerate", add a line saying so to the deviations list in `fragments-2026-09.md`.

## 4. Package and tag names differ from androidx

**Evidence.**
- `picodroid.app.Fragment`, `FragmentManager`, `FragmentTransaction`: Android has
  `androidx.fragment.app.*`.
- `picodroid.widget.ViewPager2` and `FragmentStateAdapter`: Android has
  `androidx.viewpager2.widget.ViewPager2` and `androidx.viewpager2.adapter.FragmentStateAdapter`.
- The layout tag is a bare `<ViewPager2>` (`res/layout/activity_main.xml`). Android requires the
  fully qualified class name for anything outside `android.widget` / `android.view`.
- `FragmentStateAdapter(Activity)`: Android takes a `FragmentActivity`.

**Recommendation.** Keep the packages. Flattening androidx into `picodroid.*` follows the project
rule (`CLAUDE.md`: apps import `picodroid.*`, named to mirror Android). Optional and cheap: let the
layout packer also accept `<picodroid.widget.ViewPager2>`, so a layout written the
fully-qualified way compiles (`tools/papk-pack/src/res.rs`). Any rename means the shrink maps are
append-only: follow the SDK rename procedure. No app change.

## 5. Swiping is off: `pager.setUserInputEnabled(false)`

The board has four buttons and no touch panel, so keys turn the pages. This is the Android API used
as intended. **No action.** It is listed so the next reviewer doesn't flag it.

## 6. A doc row overstates what the code checks

**Evidence.** Item 4 of [claudeusage-android-shape-2026-09.md](claudeusage-android-shape-2026-09.md)
says each build post is "guarded by `isAdded()` and `getView()`". The code (`UsagePage.gone()`)
checks `gen != viewGen || !isAdded() || root == null || host.destroyed()`. It uses no `getView()`;
`root` is the page's own copy of the view, and `viewGen` strands a chain whose view was recreated.

**Fix.** Either make the code match the idiom (`getView() == null` in place of `root == null`;
`viewGen` stays, because a recreated view is non-null too), or correct the doc row. The first is
better, and it falls out naturally if section 1(a) replaces `host.destroyed()`.

## Verification

- `./scripts/sim.sh --app helloworld` (CLAUDE.md smoke) and `./scripts/pre-commit`.
- `claudeusage` in the sim against a demo bridge, all four pages and the status screen:
  `python3 examples/claudeusage/bridge/claude_usage_bridge.py --demo &` then
  `./scripts/sim.sh --board pico_display2_w --app claudeusage`. Keys `1`–`4` are A, B, X, Y.
  Check B turning every page, Y home, Y held for AUTO, and a bridge killed mid-run returning to the
  status screen.
- The nightly row in `scripts/hil-tests.conf` (`claudeusage|sim|…`) greps `ui ready`,
  `page -> Claude usage`, `discovery: failed` and `state -> …`. `examples/claudeusage/test.ctrl`
  waits for `page -> Claude usage`. Keep those log lines.
- Pixel identity: replay one bridge payload and diff screenshots before and after (the
  `claudeusage_pixel_ab` recipe in memory).
- Hardware (`pico_display2_w`, `flash.sh --board pico_display2_w --app claudeusage` with WiFi
  creds): no new slow-handler warnings on a page turn or on a sync. This is the check that catches
  the section 2 trap.

## Bookkeeping when done

- Update [claudeusage-android-shape-2026-09.md](claudeusage-android-shape-2026-09.md): add these
  items, close what lands, and fix the item 4 wording (section 6). It also has three stale rows
  from before the refactor: #8 (`onBackPressed` is no longer overridden), #12 (the key catcher is
  gone) and #13 (says the chrome uses weights; it uses fixed dp widths).
- If `ViewModel` / `LiveData` land, record them under FR-9 in
  [../fragments-follow-ups.md](../fragments-follow-ups.md) and in §5 of
  [fragments-2026-09.md](fragments-2026-09.md).

## What landed (2026-09-30)

| Section | Outcome |
|---|---|
| 1 | Done, (a) and (b). No page names `MainActivity`: the palette is `Palette.of(getResources())`, `fade_ms` comes from `getResources()`, `isAdded()` covers a destroyed host (`FragmentManager.dispatchDestroy` detaches every fragment), the data comes from `UsageViewModel`, and `onPageBuilt` goes through the `UsagePage.Host` interface. The SDK gained `picodroid.lifecycle`. |
| 2 | Done. Each page observes `model.usage()` with `getViewLifecycleOwner()`; `updatePage()` and `visiblePage()` are gone, and the `FragmentStateAdapter` javadoc no longer recommends the tag lookup. For the trap, the first option: the Activity stays the Service's listener and paints the chrome, then publishes; each page's observer only posts its repaint to the next tick. The per-minute gating is unchanged. |
| 3 | Done in the app and in `fragmentdemo` / `pagerdemo`. SDK decision: tolerate a skipped `super`, as `Activity` does; recorded in `fragments-2026-09.md` §5 and the `Fragment` javadoc. |
| 4 | Packages kept. The layout packer accepts `<picodroid.widget.X>` for every inflatable view. |
| 5 | No action. |
| 6 | The code now matches the idiom (`getView() == null`); the doc row says why `viewGen` stays. |

Choices the handover left open:

- **What the `LiveData` carries.** The bound `UsageService`, published again on every change and
  tick, `null` while unbound. An immutable state object per publish is an allocation a second.
- **`ViewModel` scope.** The Activity instance. There are no configuration changes, so a
  ViewModel is cleared on destroy, `recreate()` included.
- **No reflection.** `Activity.getDefaultViewModelProviderFactory()` is the override point, so a
  fragment writes `new ViewModelProvider(requireActivity()).get(X.class)` as on Android.
- **Owners.** The Activity (lifecycle and ViewModel store) and a fragment's view
  (`getViewLifecycleOwner()`); a fragment itself is neither.
- **RP2040.** Not excluded; cost and reasoning under FR-12 in
  [../fragments-follow-ups.md](../fragments-follow-ups.md): +15.7 KB on the RP2350, +15.0 KB on
  the RP2040, no RAM at rest.

Verified 2026-09-30:

- Sim rows `claudeusage`, `fragmentdemo` (98 checks, 22 of them new: `LiveData`, the view and
  Activity lifecycles, `ViewModelProvider`) and `pagerdemo`, each in both shrink modes.
- Pixel identity: the app from before this change and the new one, one replayed demo-bridge
  payload each, six screenshots (the four pages by B, then A, then Y home), all identical.
- Hardware, `pico_display2_w` build on the bench against the live bridge: two laps of the pages
  by B, two A, X sync, Y home, a minute tick and two polls. No `slow handler` line on the plain
  debug build, cold or warm.
- Not exercised: Y held for AUTO and a bridge killed mid-run. Neither path changed.

Open: the size-ratchet accept (FR-12).
