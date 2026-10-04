# Completed: Roadmap: `claudeusage` toward Android shape

Items closed out of [claudeusage-android-shape-2026-09.md](../designs/claudeusage-android-shape-2026-09.md), moved here on 2026-09-28 (and again on
2026-10-01, section 10) so the original lists only open work. Text is as it stood when moved; ids keep their meaning.

## 1. App structure and lifecycle

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 1 | One Activity with hand-rolled `Page` objects swapped in a `FrameLayout`, where Android would use Fragments in a `ViewPager2` or the Navigation component. | SDK-forced (no Fragment) | **closed** 2026-09-27: `picodroid.app.Fragment` and `picodroid.widget.ViewPager2` landed ([`fragments-2026-09.md`](../designs/fragments-2026-09.md)); the four screens are `UsagePage extends Fragment` subclasses in a `<ViewPager2>` inside `page_host`, made by `UsagePagerAdapter extends FragmentStateAdapter`, the dots row driven by `OnPageChangeCallback.onPageSelected`, and the status screen a fragment `replace()`d into `page_host` over the pager. The pager keeps one page alive and hands the outgoing page's state to the adapter, which is what `discardPage()` did by hand; the prebuilt page is gone, since the pager's first page is always built underneath the status screen. |
| 2 | `Application.onCreate` called `startActivity(new Intent(MainActivity.class))`; Android declares the launcher Activity in the manifest. | App choice | **closed**: `PicodroidManifest.xml` declares `activity="claudeusage/ui/MainActivity"`; `ClaudeUsageApp` is gone. The boot path ignores `activity=` when `application=` is present, so an app cannot have both. |
| 7 | `getDisplay()` called in `onCreate` with the result discarded. | App choice | **closed**: removed. |
| 9 | No `onSaveInstanceState`: page index and the AUTO flag were lost on recreate. | App choice | **closed**: both saved in the `Bundle`; AUTO also persists in `SharedPreferences` as a setting. |
| 8 | `onBackPressed()` overridden to a no-op instead of an `OnBackPressedCallback`. | SDK-forced (no dispatcher) | **closed** (recorded 2026-09-30; gone since `Activity.onKeyDown` landed, item 10): `onBackPressed` is not overridden. `onKeyDown` consumes BACK's press, so the default `onKeyUp` never runs it, as on Android. |

## 2. Input

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 12 | Key catcher hidden with `setAlpha(0f)` plus a background drawable rather than `View.INVISIBLE`. | App choice | **closed** (recorded 2026-09-30): the catcher went with item 10. |
| 10 | An invisible 1x1 `Button` holds focus so hardware keys reach an `OnKeyListener`; Android overrides `Activity.onKeyDown`. | SDK-forced | **closed** 2026-09-24: `Activity.onKeyDown` landed (gaps G8); the catcher is gone and BACK is consumed in `onKeyDown`, which is also what keeps the default `onKeyUp` from running `onBackPressed`. |

## 3. Layout and drawing

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 15 | Colours, strings and dimensions inline in Java. | App choice | **closed**: `res/values/colors.xml`, `strings.xml`, `dimens.xml`; `Palette` is resolved once from `Resources`; every user-visible string goes through `R.string`. |
| 16 | Right/centre-aligned labels are a `TextView` wrapped in a gravity-set `LinearLayout`. | SDK-forced (no `TextView.setGravity`) | **closed** 2026-09-24: `TextView.setGravity` landed; `Ui.labelRight` / `labelCentred` are one sized `TextView`. The tall display figure keeps its row, since a label cannot centre itself vertically. |
| 17 | Progress bars are nested `FrameLayout`s with `GradientDrawable`s. | SDK-forced (no styled `ProgressBar`, no `Canvas`) | **closed** 2026-09-23: `ProgressBar` tints per instance (gap G3), so `BarView` is one `ProgressBar`; the Limits gauges are `CircularProgressIndicator` rings (gap G2). |
| 18 | Bar charts are arrays of `FrameLayout` boxes. | SDK-forced (no `Canvas`) | **closed** 2026-09-26: `ui/TrendChart` and `ui/WeekChart` are `View` subclasses that draw their bars (and History's day letters) in `onDraw(Canvas)`; see [`canvas-2026-09.md`](../designs/canvas-2026-09.md). |
| 19 | Large numerals are PNG sprites in `ImageView`s. | SDK-forced (one font size) | closed 2026-09-23: `TextView.setTextSize(64)` |
| 20 | Sprites loaded from `assets/` by string path. | App choice | **closed**: `res/drawable/d0.png`… with `R.drawable.*` and `setImageResource`. |
| 21 | Sprites pre-composited onto the card colour. | SDK-forced (assets lose alpha) | closed 2026-09-23: no sprites left |
| 23 | Widgets built with no-arg constructors. | App choice | **closed**: every widget takes the `Context`. |

## 4. Threads, timing and data flow

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 27 | Repository was a static singleton started from `Application.onCreate`. | App choice | **closed**: it is `UsageService`, a started and bound `Service` with a `LocalBinder`; the Activity binds in `onStart` and unbinds in `onStop`. |
| 28 | Two raw, unnamed `Thread`s that never stop. | App choice | **closed**: named `usage-poll`, which exits when the Service is destroyed; the tick thread is gone (row 30). |
| 30 | A thread that sleeps 1 s and posts a tick, instead of `Handler.postDelayed`. | SDK-forced (no `Handler`, by design) | **closed 2026-09-26**: `ScheduledExecutorService.scheduleAtFixedRate` on the main thread, the `java.util.concurrent` shape; H1 in the gaps roadmap. |
| 31 | Poll loop idled by sleeping in 250 ms slices polling a volatile flag. | App choice | **closed**: `Object.wait(ms)` on a lock; `refreshNow()` and `onDestroy` call `notifyAll`. |
| 36 | Connectivity polled with static `NetworkInfo.isConnected()`. | SDK-forced (no `NetworkCallback`) | **closed** 2026-09-26: `ConnectivityManager` gained Android's `NetworkCallback`; `UsageService` registers one in `onCreate`, `onAvailable` wakes the poll thread and `onLost` paints NO_WIFI itself. Nothing polls the link. |

## 5. Networking and parsing

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 42 | `LinkState` as `int` constants with a name table. | App choice | **closed**: an `enum` with `shortText`/`advice` as instance methods. |
| 43 | Bridge host baked at build time through the `NetTestConfig` test hook. | App choice | **closed** as far as the app can: `UsageService` reads `bridge_host` from `SharedPreferences` with `NetTestConfig.HOST` as the default, so an installed unit can be repointed without a rebuild (via `pdb`, or a future settings screen). A `BuildConfig` block is still the right default source; see G7 in the gaps roadmap. *2026-09-25:* the address is now discovered: `BridgeDiscovery` broadcasts one UDP query and the bridge answers, so nothing is baked or typed unless the LAN blocks broadcasts (`bridge_host` pins, `NetTestConfig.HOST` is the fallback). The Android shape would be `NsdManager` / DNS-SD; the runtime has no multicast DNS, so the app carries the two-datagram protocol itself. |

## 6. Time and number formatting

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 44 | `TimeFormat` does epoch arithmetic by hand with a mutable static UTC offset. | SDK-forced (no `java.time`) | **closed** 2026-09-24: `java.time` landed; `TimeFormat.hm` is a `LocalTime` in the bridge's `ZoneOffset`, formatted with `DateTimeFormatter`. Kept to those four classes rather than `LocalDateTime.ofInstant(…, ZoneId.systemDefault())`: each class an app touches is parsed into RAM, and this app is at the heap's edge (gaps G11). |
| 45 | Zero padding by hand instead of `String.format`. | App choice | **closed**: `String.format("%02d")` and friends. |

## 7. Hardware

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 47 | PWM channels never closed. | App choice | **closed**: `RgbLed implements AutoCloseable`; `onDestroy` closes it. |

## 9. Fragment shape (2026-09-30)

From the review in [claudeusage-fragment-shape-2026-09.md](../designs/claudeusage-fragment-shape-2026-09.md).

| # | Deviation | Tag | Status |
|---|---|---|---|
| 51 | `UsagePage.onAttach` cast its `Context` to `MainActivity` and called six package-private methods on it (`palette`, `destroyed`, `onPageBuilt`, `repo`, `hasData`, `fadeMs`). | App choice / SDK-forced (no `ViewModel`) | **closed** 2026-09-30: the palette is `Palette.of(getResources())` (resolved once, cached), `fade_ms` comes from `getResources()`, `isAdded()` covers a destroyed host (the manager detaches every fragment on destroy), the data comes from `new ViewModelProvider(requireActivity()).get(UsageViewModel.class)`, and `onPageBuilt` goes through the `UsagePage.Host` interface. No page names `MainActivity`. |
| 53 | `UsagePage.onAttach`, `onCreate`, `onViewCreated` and `onDestroyView` never called `super`; Android throws `SuperNotCalledException`. | App choice | **closed** 2026-09-30: all four call it, as do the fragments of `fragmentdemo` and `pagerdemo`. The SDK still tolerates a skipped `super`, as `Activity` does; `fragments-2026-09.md` §5 says so. |
| 56 | The roadmap said every build post was guarded by `isAdded()` and `getView()`; the code checked its own `root` field and `host.destroyed()`. | doc | **closed** 2026-09-30: the code is `gen != viewGen \|\| !isAdded() \|\| getView() == null`, and row 4 says why `viewGen` stays. |

## 10. The remaining-shape round (2026-10-01)

Closed by the work in
[claudeusage-remaining-shape-2026-09.md](../designs/claudeusage-remaining-shape-2026-09.md); the
letter-number in each row is that document's item. Rows 59 and up were first recorded there.

| # | Deviation | Tag | Status |
|---|---|---|---|
| 3 | Theme colours set by assigning `picodroid.graphics.Theme` static fields. | SDK-shape | **closed** (E9): `res/values/themes.xml` declares `AppTheme` and `MainActivity.onCreate` calls `setTheme(R.style.AppTheme)`. Layouts and shapes read it with `?attr/…`, resolved when the app is built; styles carry the label kinds. `Palette.applyTheme` is gone. |
| 4 | A page's view tree is built a few views per main-thread post, with a build token to survive a page turn mid-build. | App choice | **closed** (E1): each page is `res/layout/page_*.xml`, inflated by `AsyncLayoutInflater` a slice per tick into the empty host `onCreateView` returns. `buildNext`, `step`, `viewGen` and `restart` are gone; the callback checks the host is still the fragment's view. |
| 5 | `catch (OutOfMemoryError \| RuntimeException)` around view building, retrying next tick. | App choice | **closed** (E3): the inflater owns the retry (an `OutOfMemoryError` part-way drops the partial tree and starts again on a later tick). No catch in the app. |
| 13 | Every view placed in absolute pixels with `setPosition`/`setSize`; no XML, no LayoutParams, no weights. | App choice | **closed** (A2, E2, E8): the chrome uses `layout_weight` for its flexible children and `android:gravity` on the labels themselves (the four wrapper rows are gone; the footer's widths summed to 316, so its hint now ends on the margin like the header's, 4 px right of where it was). The pages are XML: a `FrameLayout` child is placed by `layout_margin*` and `layout_gravity`, repeated parts are `<include>`d, cards and dots are `<shape>` drawables, and `RingView`, `TrendChart` and `WeekChart` are elements made in `MainActivity.onCreateView` and sized by `onMeasure`. No `setPosition`, `setSize` or `setSpacing` left in the app. |
| 14 | Screen size hard-coded as `Ui.WIDTH`/`Ui.HEIGHT`. | App choice | **closed**: the constants are gone with the code-built pages; the page layouts are `match_parent` with dp geometry inside the cards. |
| 22 | `GradientDrawable` used as a fluent builder and re-allocated per colour change. | SDK-shape / App choice | **closed** (E7): `View.setBackgroundTintList` recolours a shape background in place (`Ui.tint`); the drawables come from `res/drawable`. |
| 25 | Hand-written diffing of every on-screen value. | App choice | **closed** (E4): the SDK's setters do nothing when the value is unchanged (`setText`, `setTextColor`, `setAlpha`, `setVisibility`, `setEnabled`, the tint lists), as Android's do. `Line`, `BarView` and every `shown*` field are gone; a page calls `setText`. |
| 26 | Repaint throttled by a minute counter instead of `TextClock`. | App choice | **closed** differently (D3): the timing is the UI's. `UsageViewModel` runs a once-a-second main-thread task only while it is observed and publishes a state only when one would be painted differently, which on the data screens is once a minute. There is still no `TextClock`. |
| 32 | Cross-thread handoff via `Executors.mainExecutor().execute(...)`. | SDK-shape | **closed** (B6): `Context.getMainExecutor()` and `Activity.runOnUiThread` exist; the app uses the first. |
| 33 | One `Listener` slot with both a data callback and a 1 Hz tick, set in `onResume`/`onPause`. | App choice | **closed** (D1, D2, D3): no `Listener`. The Service publishes into `UsageRepository`; the ViewModel observes that and the Activity observes the ViewModel for the chrome, as the pages do for themselves. The Service keeps only the trend sampling, on a 150 s timer. |
| 52 | The pages' `LiveData` carries the bound `UsageService` itself, published on every change and every tick. | App choice | **closed** (D4, C4): the `LiveData` carries an immutable `UsageUiState`, published when something on screen changed; the ViewModel holds the repository, which has no `Context`. The Service is started and never bound. |
| 58 | `UsagePage.Host`, for the one call a page made on its host (`onPageBuilt`, which warmed the chrome). | idiom preserved | **closed** (E5): `warmChrome` and `Host` are gone. With pack-time class linking the first refresh's cold cost is 15 µs of a 1 ms span in the simulator's `parity-metrics` trace (175 resolutions, 10 µs; class initialisation 5 µs), where it was a third of 57 ms on the board before. To confirm on the board. |
| 59 | The ViewModel factory returned a `UsageViewModel` for any class asked. | App choice | **closed** (A1): it checks the class and throws `IllegalArgumentException` otherwise. |
| 60 | The pace tick on the ring was a second `CircularProgressIndicator` stacked on the first. | App choice | **closed** (A4): `RingView` is one `View` drawing the track, the fill and the tick in `onDraw(Canvas)`; pixel-identical. |
| 61 | `onKeyDown` / `onKeyUp` returned `true` for keys the app does not handle. | App choice | **closed** (A5): the default arms call `super`. |
| 62 | The bridge fallback address came from the `NetTestConfig` test hook. | App choice | **closed** (A3): `picodroidBuildConfig { fieldFromProperty("BRIDGE_HOST", …) }` and `BuildConfig.BRIDGE_HOST`, with the same property and environment names. |
| 63 | `ServiceConnection`, `bindService`, `Intent(Class)` and `IBinder` without `Binder` differed from Android's signatures. | SDK-shape | **closed** in the SDK (B1 to B4): Android's signatures, `ComponentName`, `Binder`, `Intent(Context, Class)`; the callbacks are interface calls, so a base class may declare them. The app itself no longer binds. |
| 64 | `String.format(getString(id), args)` in a dozen places, with the format strings pre-resolved into fields. | SDK-shape | **closed** (B5): `getString(int, Object...)` on `Context`, `Fragment` and `Resources`; the fields are gone. |
| 65 | `InetAddress.getByAddress(int, int, int, int)` and `new InetAddress(packet.getAddress())`. | SDK-shape | **closed** (B9): `getByAddress(byte[])`, and `DatagramPacket.getAddress()` returns the `InetAddress`. |
| 66 | `SharedPreferences` was not thread-safe and `apply()` wrote synchronously; the app pushed one write to a background executor and another to the main thread. | SDK-semantics | **closed** (C1): one instance per file, safe from any thread; `apply()` writes behind. Both workarounds are deleted. |
| 67 | `Executors.newSingleThreadScheduledExecutor()` ran its tasks on the main thread. | SDK-semantics | **closed** (C2): that one is `Executors.mainScheduledExecutor()`; the JDK name is a thread of its own. |
| 68 | A `TextView` with no text showed "Text"; `Line` blanked every inflated label. | SDK-semantics | **closed** (C3): LVGL's widget defaults are off, so a label, a checkbox and a spinner start empty. |
| 69 | The first chrome paint took a tick of its own after `onServiceConnected`, and the page dots another. | Perf | **closed** (E6, in part): there is no connect callback, and a dot's colour is one tint (a style set with no layout) so the dots share the page-turn tick. The page's own deferral stays: row 73 in the roadmap. |
| 70 | A result from the poll thread reached the screens as three callbacks' worth of state. | App choice | **closed**: one immutable `UsageData` per result, so the first sync is one publish and one repaint (3,002 bytecodes in the simulator's trace where the first cut of the repository took 6,052). |
| 71 | `Ui.java` held the pages' geometry as constants kept in step with `dimens` by hand. | App choice | **closed** (E2): the geometry is in the layouts; `Ui` is the stale-dimming constant and `tint`. |
| 72 | `LinearLayout.setSpacing`, which Android does not have, spaced the page dots and the rate row. | SDK-shape | **closed** in the app (E2): `layout_marginRight` on the dots, `layout_marginLeft` on the unit. The SDK method stays for the apps that use it. |
| 73 | A page repainted on the tick after the one that delivered the state, and the Models page painted its two cards on two ticks the first time. | Perf | **closed** (E6, 2026-10-03, on `pico_display2_w`): a page binds and paints in the tick its layout finishes, the Models page paints both cards at once, and a painted page repaints inside the `LiveData` call beside the chrome; no `slow handler` line across 25 turns, five syncs and the minute ticks. One deferral stays: the first paint of a page built before the data came takes the next tick, because with cold call sites and the chrome's repaint it measured 58 ms. |

## SDK asks (from this app's point of view)

Ordered by how much Android shape each would buy back here.

1. ~~`Activity.onKeyDown` / `onKeyUp` as the fallback when no view consumes a key (item 10; gaps G8).~~ Shipped 2026-09-24. ~~Long-press and auto-repeat (`onKeyLongPress`, `getRepeatCount()`).~~ Shipped 2026-09-26: the app's four buttons carry eight actions, in Android's `startTracking` / `onKeyLongPress` / `isCanceled` shape.
2. ~~`TextView.setGravity` (item 16).~~ Shipped 2026-09-24. `setTextSize` shipped 2026-09-23 (item 19; gaps G1).
3. ~~A borderless, padding-free container option for inflated `LinearLayout`/`FrameLayout`.~~
   Shipped 2026-09-24 the Android way: every `LinearLayout` / `FrameLayout` is flat by default
   (no border, fill, radius or padding), `setBackgroundColor` honours alpha, and layouts take
   `@android:color/transparent`; `Ui.flat` and its ten call sites are gone.
5. ~~A minimal `Canvas` (item 18; gap G4).~~ Shipped 2026-09-26 as Android's `onDraw(Canvas)`
   over a retained display list. The styled `ProgressBar` (item 17; gap G3) and the ring gauge
   (gap G2) landed 2026-09-23.
6. ~~`ConnectivityManager` with a `NetworkCallback` (item 36).~~ Shipped 2026-09-26 with
   `Network`, `NetworkCapabilities` and `NetworkRequest`; callbacks arrive on the main thread
   from the event loop, a link already up is announced right after `register` returns.
8. ~~`java.time` or at least `DateFormat`/`DateUtils` (item 44).~~ Shipped 2026-09-24 as a
   port of the JDK classes (fixed-offset zones only).
4. ~~`Handler` / `View.postDelayed` or an equivalent one-shot timer on the main thread (item
   30).~~ Answered 2026-09-26 by the scheduled executor, `Executors.mainScheduledExecutor()`
   since 2026-10-01. `Handler` and `postDelayed` are rejected by design.
7. ~~A `BuildConfig` block (item 43; gaps G7).~~ Shipped 2026-09-27
   (`picodroidBuildConfig { fieldFromProperty(...) }`); `claudeusage` uses it since 2026-10-01.
9. ~~`Fragment` and a `ViewPager2` (item 1).~~ Shipped 2026-09-27: `picodroid.app.Fragment`,
   `FragmentManager`, `FragmentTransaction`, `FragmentFactory` and `picodroid.widget.ViewPager2`,
   `FragmentStateAdapter`, `<ViewPager2>` in layouts; see
   [`fragments-2026-09.md`](../designs/fragments-2026-09.md).

## Found on the way: resolution-cache growth (runtime, not app; fixed 2026-09-23)

**Fixed in `350c3552`.** The per-executor doubling caches and `cache_push` are gone: method,
field, static and `new` sites now resolve into fixed-size, four-way set-associative tables on the
shared heap (`crates/jvm/src/resolve_cache.rs`, 16 KB on the RP2350), allocated once and kept
across Runnables, so there is no growth request left to refuse. The account below is kept as the
record of what the soak showed. The 20,480 B and 22,528 B requests it mentions are unrelated to
the other large request the heap used to make, the collector's compaction buffer (G10 in the
gaps roadmap, closed 2026-09-25: a fixed 4 KB slice claimed at boot, compaction in passes).

A soak of the new app with AUTO cycling the four screens logged, in the sim, `[sim] OOM: tried
20480 B` on every other page turn (15 in 30 turns; the pre-change app: 0 in 30), with 50 to 60 KB
free but no block larger than 18 KB. The request is the JVM's method-resolution cache
(`crates/jvm/src/interpreter/helpers.rs::cache_push`) doubling from 256 to 512 entries (40 B each
on the 64-bit sim; 10,240 B for the same doubling on the RP2350). The Android-shaped app resolves
more distinct members than the old one, and the caches are keyed per accessing class: a `Palette`
*instance* read from ten classes costs ten entries per colour where the old `static final` ints
cost none (javac inlines them), and `LayoutInflater`, `SharedPreferences`, the `Service` lifecycle
and `String.format` boxing add method entries. The app crosses the boundary on its third page,
when its own live set has fragmented the heap; the push is fallible, so it declines to memoise and
asks for the whole block again at every later page. No crash and no functional effect in an
8-minute soak: a re-resolve per page turn, and a log line per attempt in the sim (the device
allocator is silent).

**Tried and reverted:** growing the caches by a fixed 64-entry step instead of doubling. A `Vec`
still needs one contiguous block for the whole enlarged table, so the step only moved the failure
to the field cache at 640 entries (22,528 B asked, 17,448 B largest block; 10 failures in 58
turns). The runtime is unchanged by this round.

**What would fix it (runtime, not this app):** store the caches in chunked slots (as
`ChunkedSlots` already does for the object and string stores) so growth is a chunk, not the table;
or bound each cache and stop retrying once a growth has failed, so a full cache costs one
re-resolve per miss and no allocator call. The app-side alternative, `static final` colours
inlined by javac, would trade the resource-backed palette (item 15) for cache entries; not taken.
