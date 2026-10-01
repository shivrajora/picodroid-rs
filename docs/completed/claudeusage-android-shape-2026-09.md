# Completed: Roadmap: `claudeusage` toward Android shape

Items closed out of [claudeusage-android-shape-2026-09.md](../designs/claudeusage-android-shape-2026-09.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

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
