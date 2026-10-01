# `claudeusage`: what is still not Android (handover, 2026-09-30)

Status: **open, nothing started.** Written after a review of `examples/claudeusage` on `main` at
`73ceb866` (the `picodroid.lifecycle` commit). It lists every place the app still reads differently
from the same app written for Android, grouped into work packages. Each package gives the
evidence, the Android shape, the fix and how big it is.

Earlier rounds, for context:

- [claudeusage-android-shape-2026-09.md](claudeusage-android-shape-2026-09.md) is the running
  tracker (items 1–58). Rows cited below as **#N** are its rows. Items here marked **new** are
  not in it yet.
- [claudeusage-fragment-shape-2026-09.md](claudeusage-fragment-shape-2026-09.md) is the previous
  handover (Fragments, `ViewModel`, `LiveData`), landed.
- [claudeusage-gaps-roadmap-2026-09.md](claudeusage-gaps-roadmap-2026-09.md) is the platform side
  (G, H, P and D items).

## How to read this

Each item has one owner tag:

- **App**: the SDK already offers the Android shape; only the app changes.
- **SDK signature**: an SDK API exists but its name or signature differs from Android's.
- **SDK semantics**: the name matches Android and the behaviour does not. These are the worst
  kind, because code that looks right behaves differently.
- **SDK gap**: the Android API does not exist.
- **Perf**: the app leaves the Android idiom to stay inside the RP2350's per-tick budget. Closing
  it means making the platform absorb the cost.

Suggested order: A (minutes, app-only), B (cheap, wide benefit), C (correctness), then D or E by
appetite. F needs decisions from the project owner before any work.

## A. App-only fixes, doable today

| # | What | Evidence | Fix |
|---|---|---|---|
| A1 | The ViewModel factory returns a `UsageViewModel` for any class asked. | `MainActivity.getDefaultViewModelProviderFactory()`: `return (T) new UsageViewModel();` behind `@SuppressWarnings("unchecked")`. | Check `modelClass == UsageViewModel.class`, else throw `IllegalArgumentException`, as an Android factory does. |
| A2 | The chrome uses fixed dp widths that sum to 320, and wrapper layouts that exist only to align one label. | `res/layout/activity_main.xml`: `title_width`, `plan_width`, `clock_width`, `banner_width`…; `clock_box`, `sync_box`, `banner_box`, `home_box` each wrap one `TextView`. `page_height` is a fixed 190dp. | `layout_weight="1"` on the flexible child and on `page_host`; `android:gravity` on the `TextView` itself (`TextView.setGravity` landed 2026-09-24; check the packer maps `gravity` for a `TextView`). The packer already compiles `layout_weight`, `layout_gravity` and `gravity` (`tools/papk-pack/src/res.rs`). Pixel-compare before and after. |
| A3 | The bridge fallback address comes from the `NetTestConfig` test hook. | `UsageService.onCreate`: `NetTestConfig.HOST`; `build.gradle.kts`: `picodroidNetTest { enabled = true }`. | `picodroidBuildConfig { fieldFromProperty(...) }` and `BuildConfig.BRIDGE_HOST` (landed 2026-09-27, gaps G7 amendment; `askclaude` is the worked example). Keep the property and environment names, or update `README.md` and `test.env`. |
| A4 | The pace tick on the ring is a second `CircularProgressIndicator` stacked on the first. | `ui/RingView.java`: `ring` and `tick`. | One `View` subclass drawing the arc and the tick in `onDraw(Canvas)`, as `TrendChart` does. Check `Canvas` has `drawArc` first, and the 2048 B per-view op budget. Lowest value of the four; skip if `drawArc` is missing. |
| A5 | `onKeyDown` / `onKeyUp` return `true` for keys the app does not handle. | `MainActivity.onKeyDown`: `default: return true;`. | `return super.onKeyDown(code, event)` in the default arm. The four real keys are all handled, so behaviour does not change. |

## B. SDK signature fixes (cheap; additive overloads)

Add the Android signature beside the existing one, move the app to it, and decide per item whether
the old one stays. Each new SDK member needs its rows in `sdk/member-names.tsv` before Rust or
the contract generator sees it, and a new native arm needs a `method_tables.rs` row.

| # | SDK today | Android | Notes |
|---|---|---|---|
| B1 | `ServiceConnection.onServiceConnected(IBinder)`, `onServiceDisconnected()` | `(ComponentName name, IBinder service)`, `(ComponentName name)` | Needs a minimal `picodroid.content.ComponentName`. The callbacks are upcalls from native: check how the runtime resolves them (upcalls resolve on the exact class). |
| B2 | `bindService(Intent, ServiceConnection)` returns `void` | `boolean bindService(Intent, ServiceConnection, int flags)` with `Context.BIND_AUTO_CREATE` | Add the three-argument overload and the constant. |
| B3 | `new Intent(Class<?>)` | `new Intent(Context, Class<?>)` | Add the two-argument constructor. `Intent` is slot-addressed in native code: do not reorder or add fields. |
| B4 | `IBinder` is an empty interface; there is no `Binder` | `class Binder implements IBinder` | Add `picodroid.os.Binder`. The app's `LocalBinder` then `extends Binder` with a `getService()` method in place of the public `service` field. |
| B5 | `getString(int)` only | `getString(int resId, Object... formatArgs)` on `Context`, `Fragment` and `Resources` | The app writes `String.format(getString(id), args)` in about a dozen places and pre-resolves format strings into fields. The fields may stay (they are a perf choice); the call sites get shorter. |
| B6 | `Executors.mainExecutor()` | `Context.getMainExecutor()`, `Activity.runOnUiThread(Runnable)` | Thin wrappers. #32. |
| B7 | Keys are `KEYCODE_DPAD_UP/DOWN/CENTER` and `KEYCODE_BACK` | `KEYCODE_BUTTON_A/B/X/Y` (96, 97, 99, 100) | See F1: this one needs a decision first. |
| B8 | `GradientDrawable.setColor` etc. return `this` | return `void` | Returning `this` is source-compatible with Android-style code, so this only matters the other way round. Low priority. #22. |
| B9 | `InetAddress.getByAddress(int, int, int, int)`, public `new InetAddress(int)` | `InetAddress.getByAddress(byte[])`, `DatagramPacket.getAddress()` returning an `InetAddress` | `BridgeDiscovery` uses both. Add the `byte[]` form; make `DatagramPacket.getAddress()` return the object. |
| B10 | Lifecycle overrides on `Activity` are `public` | `protected` | #6. Widening in a subclass is legal Java, so app code written the Android way already compiles. Record and leave. |

## C. SDK semantics (same name as Android, different behaviour)

**C1. `SharedPreferences` is not thread-safe, and `apply()` writes synchronously.** New.

- Evidence: `sdk/java/picodroid/content/SharedPreferences.java` class comment ("Not thread-safe")
  and `Editor.apply()` (`commit()` inline, with a comment saying so).
- The app works around both, in opposite directions. `MainActivity.saveAuto` pushes `apply()` to
  `Executors.backgroundExecutor()` because the write overran the tick budget. `UsageService.probe`
  posts its write to the main thread because the object is not thread-safe.
- Android: one process-wide instance per file, safe from any thread; `apply()` updates memory at
  once and writes on a background thread.
- Fix: synchronise the in-memory map, and have `apply()` hand the file write to the framework's
  background executor, coalescing writes per file. `commit()` stays synchronous.
- Traps: an `apply()` followed by power loss must leave either the old file or the new one (the
  current truncate-and-rewrite does not, check it); the boot path's `SharedPreferences.load` was the
  slow span in the 2026-09-26 device QA; runtime flash writes must restore fast XIP.
- Then delete both workarounds in the app.

**C2. `Executors.newSingleThreadScheduledExecutor()` runs its tasks on the main thread.** New.

- Evidence: `sdk/java/picodroid/concurrent/Executors.java` javadoc: "Its single thread is the
  *main* thread… a task that blocks stalls the UI."
- A Java developer expects a dedicated thread. Code ported from Android that does I/O in a
  scheduled task freezes the UI here.
- Options: (a) rename the main-thread one (for example `Executors.mainScheduledExecutor()`) and
  keep the `java.util.concurrent` name for a real thread; (b) keep it and accept the difference.
- `Handler` / `postDelayed` are rejected by design and must not be proposed as the fix. The
  "SDK ask 4" line in the shape doc is stale for the same reason: the scheduled executor is the
  sanctioned timer.
- Recommendation: (a). The app's use (`UsageService.scheduler`, a 1 Hz tick that touches UI state)
  wants the main-thread one under its honest name.

**C3. A `TextView` with no text shows "Text".** New; verify first.

- Evidence: `ui/Line.java`: "the widget default is "Text"", and the constructor blanks every
  inflated label. No Java or Rust code sets that string, so it is LVGL's `lv_label` default.
- Android: an empty `TextView` is empty.
- Fix: set the empty string when the label is created (`widgets/text_view.rs`), then drop the
  blanking in `Line`. Grep other apps for the same workaround.

**C4. The ViewModel holds the bound `Service`.** New; part of #52.

- Evidence: `UsageViewModel`: `MutableLiveData<UsageService>`.
- Android: a ViewModel must not reference a `Context`; lint flags it as a leak. It is harmless
  here only because the ViewModel dies with its Activity. Closed by D1.

## D. Data flow: finish the ViewModel

Today the ViewModel is a passive holder. The Activity is the Service's one `Listener`, paints the
header and footer itself, and calls `model.publish(repo)`; the Service drives a 1 Hz `onTick`
into the Activity, which republishes, and each page filters that down to once a minute.

| # | Today | Android shape |
|---|---|---|
| D1 | `MainActivity` implements `UsageService.Listener` and calls `model.publish(repo)` / `publish(null)`. #33, #52. | The ViewModel owns the data source and exposes immutable UI state; nobody outside it calls a setter. |
| D2 | The chrome is painted from the Service callback (`refreshChrome`), the pages from `LiveData`: two data paths. `hasData()` is a plain read. New. | The Activity observes the same `LiveData` as the fragments. |
| D3 | The Service owns the 1 Hz tick; the UI gates repaints on the minute changing. #26, #33. | The UI owns its timing: `TextClock` for the clock, and countdowns ticked by the view layer. |
| D4 | The `LiveData` carries the mutable Service, re-published every second. #52. | An immutable state object, published when something changed. |

Fix, in the order that keeps each step shippable:

1. Give the Service `LiveData` fields for the snapshot and the link state, in place of the
   `Listener` (or keep the listener internal to the ViewModel). The ViewModel exposes them. The
   poll thread already hands results to the main thread, so `setValue` there is safe.
2. Publish only on change. The snapshot object is already written once and then only read
   (`UsageSnapshot`), so it can be the immutable state as it stands: no new allocation per tick.
   That answers the allocation objection recorded against #52.
3. Move the once-a-minute repaint to a UI-side timer (a main-thread scheduled task in the
   ViewModel or Activity, running only while started). The Service keeps only the trend sampling.
4. The Activity observes for the chrome.

Trap: the tick budget. The chrome repaint (~20 ms on the RP2350) and a page repaint (~30 ms) must
not share a tick. `LiveData.setValue` dispatches to every active observer inside the call. Today
each page's observer posts its repaint to the next tick (`UsagePage.onUsage`); keep that, or make
the chrome observer the one that defers. Re-check on hardware for `slow handler` lines.

Optional SDK additions that would shrink this further: `TextClock`; a `Lifecycle.State` enum and
`LifecycleObserver` (today `Lifecycle` is a concrete class with `int` states and a public
`setCurrentState`, and a `Fragment` is not itself a `LifecycleOwner` or `ViewModelStoreOwner`).

## E. The tick budget in app code (the largest remaining gap)

Everything here exists because building or repainting a screen in one main-thread tick overruns
the slow-handler budget on the RP2350. An Android developer would write none of it. It is the bulk
of `UsagePage.java` and a good part of every page class.

| # | App code | Android | Platform fix |
|---|---|---|---|
| E1 | Pages are built a few views per tick: `buildNext` / `paintNext` step machines, `viewGen`, `restart`. #4. | Inflate `R.layout.page_x` in `onCreateView`. | An incremental inflater, shaped like androidx `AsyncLayoutInflater` (`inflate(resId, parent, callback)`), that slices the work across ticks inside the framework. |
| E2 | Pages are placed with absolute `setPosition` / `setSize` from constants in `Ui.java`, kept in step with `dimens` by hand. #13, #14. | XML layouts with margins. | `layout_margin*` in the packer (it warns "layout_margin" unsupported today) and enough layout attributes to express the pages. Removes `LinearLayout.setSpacing`, which Android does not have. |
| E3 | `catch (OutOfMemoryError \| RuntimeException)` around building, retry next tick. #5. | None. | Falls out of E1 if the inflater owns the retry, or stays as a smaller guard. |
| E4 | `Line` and the `shown*` fields in `RingView`, `BarView`, `LimitCard`, `MeterRow`: every setter is diffed by hand. #25. | Call `setText`. | Make the SDK setters no-ops when the value is unchanged (`setText`, `setTextColor`, `setProgress`, `setBackground`, `setAlpha`, `setVisibility`), in Java before the native call. Then delete `Line`. Measure one page repaint before and after. |
| E5 | `warmChrome()`: runs `String.format` on placeholder values to resolve call sites before the first real refresh; `UsagePage.Host.onPageBuilt` exists only to trigger it. New; #58. | None. | A runtime problem (cold call-site resolution). Pack-time class linking (`f5d93245`) may already have removed the cost: measure the first refresh without `warmChrome`, and delete it and `Host` if it is under budget. |
| E6 | Work deferred by one tick to split a span: `onServiceConnected` posting `onConnected`; `movePageDot`; the pages' `repaint`. New. | None. | Re-measure after E4 and E5; each deferral can go when the combined span fits. |
| E7 | Dots and pills get a new `GradientDrawable` per colour change (`Ui.fill`). #22. | `setSelected` with a state-list drawable, or `setBackgroundTintList`. | `View.setBackgroundTintList` (the SDK has `ColorStateList` for `ProgressBar` already), or mutate-in-place on the drawable. |
| E8 | `TrendChart` / `WeekChart` size themselves with `setSize` in the constructor and cannot be declared in XML. New. | `onMeasure` + `setMeasuredDimension`; a custom view tag in the layout. | `onMeasure` on `View`; custom-view tags need a per-app factory, since there is no reflection (see F2). |
| E9 | Colours go through a `Palette` object and static `Theme` fields. #3. | `themes.xml`, `?attr/colorPrimary`, styles. | Theme attributes in the resource compiler. Large; only with E2. |

Start with E4 and E5: both are measurable in a day and delete app code. E1 and E2 together are a
project of their own and are what would let the pages move to XML.

## F. Decisions for the project owner

These are choices, not tasks. Each changes more than this app.

- **F1. What the four buttons are.** `platforms/rp/boards/pico_display2_w/board.toml` maps A, B, X,
  Y to key codes 19, 20, 23, 4 (`DPAD_UP`, `DPAD_DOWN`, `DPAD_CENTER`, `BACK`). Y being BACK is
  why the app must swallow BACK so it never finishes. Android's names for such buttons are
  `KEYCODE_BUTTON_A/B/X/Y`. The DPAD/BACK mapping is what lets stock widgets (lists, the keyboard,
  Settings) navigate with four buttons, so a remap would break that. Options: keep; or add the
  `BUTTON_*` constants and let an app opt in to raw button codes.
- **F2. A no-argument `Class.newInstance()`.** `sdk/java/java/lang/Class.java` says reflection is
  "intentionally out of scope". That one decision is why `FragmentFactory` and
  `getDefaultViewModelProviderFactory()` overrides (#57) exist, and why custom views cannot be
  named in XML (E8). A constructor-only form would remove all three. Costs to weigh: the shrinker
  must keep no-arg constructors of anything instantiated this way, and class linking is now done
  at pack time.
- **F3. `picodroid.concurrent` versus `java.util.concurrent`; `picodroid.net` versus `java.net`.**
  #29, #37. The SDK already ships `java.time` and `java.util` classes under their real names, so
  the JDK-named packages are possible. Moving `Executors`, `TimeUnit`, `Thread`, `URL`,
  `InetAddress` and friends would let ported code compile unchanged, at the price of a rename
  across every app and the append-only shrink maps.
- **F4. The manifest.** `buildSrc/.../ManifestSchema.kt` reads `package`, `version`,
  `version-code` and an `<application>` with `activity`, `application`, `main-class`, `label`,
  `icon`. There is no `<activity>`, `<service>`, intent filter or `<uses-permission>`, and
  `UsageService` is not declared anywhere. #49. An Android-shaped manifest is mostly packer work;
  permissions would be declarative only unless something enforces them.
- **F5. Who sets the clock.** The app calls `SystemClock.setCurrentTimeMillis` from the bridge's
  time and keeps a static zone offset in `TimeFormat` (#35). `SntpClient` now exists but leaves
  anchoring to each app. Android apps cannot set the clock; a platform time service that anchors
  it once the network is up (and a way to set the zone) would remove this from every app. Check
  what TLS certificate validation already assumes.
- **F6. HTTP or HTTPS to the bridge.** HTTPS landed, so cleartext is now a choice (#39). The bridge
  keeps the OAuth token on the PC, and a LAN bridge has no certificate a device could verify.
  Likely "keep, and say so in the README".
- **F7. `NsdManager`.** `BridgeDiscovery` is a hand-rolled UDP broadcast (#43, gaps G7). The
  Android shape needs multicast DNS in the network stack.
- **F8. Protobuf builders.** The generated messages (`claudeusage/proto/*.java`, from
  `scripts/gen-proto.sh`) are mutable with chained setters; protobuf-javalite messages are
  immutable and built through `newBuilder()`. A builder per message costs an allocation and
  flash per class.
- **F9. The polling Service.** `START_STICKY` and a poll loop that never stops. On Android this
  would be a foreground service with a notification, or polling scoped to the visible UI. For an
  appliance it is defensible; record it as kept.

## Kept on purpose (do not reopen without a reason)

- `Handler` / `Looper` / `postDelayed`: rejected by design.
- `picodroid.*` package names for Android and androidx classes (#54): the project rule.
- `pager.setUserInputEnabled(false)` (#55): the board has no touch panel.
- `PeripheralManager` PWM for the LED (#46, #48): the Android Things shape, by design.
- The shared static fetch buffer (#38) and `UsageSnapshot`'s parallel arrays (#41): heap choices.
- `View.close()`: not an Android method, documented as such; it now leaves its parent first, so
  the old trap (a closed view still reachable from its parent) is gone.
- The `picodroid-papk` Gradle plugin (#50).

## Verification

- After any change under `crates/`, `platforms/`, `sdk/` or `system-apps/`:
  `./scripts/sim.sh --app helloworld`, then `./scripts/pre-commit`.
- The app in the sim against a demo bridge:
  `python3 examples/claudeusage/bridge/claude_usage_bridge.py --demo &` then
  `./scripts/sim.sh --board pico_display2_w --app claudeusage`. Keys `1`–`4` are A, B, X, Y.
  Exercise B through all pages, Y home, Y held for AUTO, X sync, X held for rediscovery, and a
  bridge killed mid-run. The last two fragment rounds did not exercise AUTO or the killed bridge.
- The nightly row (`scripts/hil-tests.conf`, `claudeusage|sim|…`) and
  `examples/claudeusage/test.ctrl` grep `ui ready`, `page -> Claude usage`, `discovery: failed`
  and `state -> …`. Keep those log lines. FR-10 in `docs/fragments-follow-ups.md` (a bridge-backed
  row that turns real pages) is still open and would cover most of the above every night.
- Layout changes (A2, E2): pixel A/B against a replayed bridge payload.
- Anything touching the tick budget (C1, D, E): `pico_display2_w` on the bench, no `slow handler`
  lines across page turns, a sync and a minute tick.
- SDK additions cost flash: measure with `parity-bench.sh --size-only` and note it in the commit.
  The size ratchet is at 0 % growth and the +15 KB from `picodroid.lifecycle` is not yet accepted
  (FR-12), so the nightly `size-ratchet` lane is already red until that is done.
- B-package changes touch other apps: `fragmentdemo`, `pagerdemo` and every app with a bound
  Service must still build (CI builds every example APK).

## Bookkeeping when done

- Add the **new** items here to
  [claudeusage-android-shape-2026-09.md](claudeusage-android-shape-2026-09.md) (next free row is
  59), close rows as they land, and move closed rows to the completed doc.
- Its "SDK asks" section is stale: ask 4 (a one-shot timer) is answered by the scheduled executor,
  and ask 7 (`BuildConfig`) landed 2026-09-27; `docs/README.md` already notes the latter.
- Update this doc's row in `docs/README.md` as packages land.
