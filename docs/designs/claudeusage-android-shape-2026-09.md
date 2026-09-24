# Roadmap: `claudeusage` toward Android shape

**Status: round 1 landed 2026-09-22 (app-only, no SDK change); SDK asks 1, 2, 3 and 8 landed 2026-09-24 and the app uses them (items 10, 16, 44 and the chrome flattening); 4 to 7 open.**

`examples/claudeusage` is a four-screen desk display (Limits, Models, Burn rate, History) for a
Pico 2 W with a Pimoroni Display Pack 2.0. The project goal is that a picodroid app reads like the
same app written for Android. This document records every place the app, as first written on
2026-09-21, departed from that, tags each with why, and tracks what has been folded back.

Each item carries one tag:

- **SDK-forced**: the SDK has no Android-shaped API for it. Closing it is SDK work, listed under
  "SDK asks" at the end. Its platform-side twin, where one exists, is in
  [`claudeusage-gaps-roadmap-2026-09.md`](claudeusage-gaps-roadmap-2026-09.md).
- **App choice**: the SDK offers the Android shape and the app went another way, usually for a
  reason the RP2350 imposes and the code comments state.
- **SDK-shape**: the SDK API itself differs from `android.*`; the app merely uses it.

The last column says what round 1 did: **closed**, **kept** (with the reason), or **open**.

## 1. App structure and lifecycle

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 1 | One Activity with hand-rolled `Page` objects swapped in a `FrameLayout`, where Android would use Fragments in a `ViewPager2` or the Navigation component. | SDK-forced (no Fragment) | open |
| 2 | `Application.onCreate` called `startActivity(new Intent(MainActivity.class))`; Android declares the launcher Activity in the manifest. | App choice | **closed**: `PicodroidManifest.xml` declares `activity="claudeusage/ui/MainActivity"`; `ClaudeUsageApp` is gone. The boot path ignores `activity=` when `application=` is present, so an app cannot have both. |
| 3 | Theme colours set by assigning `picodroid.graphics.Theme` static fields. | SDK-shape | kept: now done in `MainActivity.onCreate` from `res/values/colors.xml` before any view exists. |
| 4 | A page's view tree is built a few views per main-thread post, with a build token to survive a page turn mid-build. Android inflates synchronously. | App choice | kept: one page in one tick overran the slow-handler budget on the RP2350. |
| 5 | `catch (OutOfMemoryError \| RuntimeException)` around view building, retrying next tick. | App choice | kept: same reason; a half-built page would otherwise stay invisible. |
| 6 | Lifecycle overrides declared `public`; Android's are `protected`. | SDK-shape | kept |
| 7 | `getDisplay()` called in `onCreate` with the result discarded. | App choice | **closed**: removed. |
| 8 | `onBackPressed()` overridden to a no-op instead of an `OnBackPressedCallback`; also unreachable because `onKey` consumes BACK first. | SDK-forced (no dispatcher) | kept as a guard: if the key catcher ever loses focus BACK would otherwise `finish()` the appliance. |
| 9 | No `onSaveInstanceState`: page index and the AUTO flag were lost on recreate. | App choice | **closed**: both saved in the `Bundle`; AUTO also persists in `SharedPreferences` as a setting. |

## 2. Input

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 10 | An invisible 1x1 `Button` holds focus so hardware keys reach an `OnKeyListener`; Android overrides `Activity.onKeyDown`. | SDK-forced | **closed** 2026-09-24: `Activity.onKeyDown` landed (gaps G8); the catcher is gone and BACK is consumed in `onKeyDown`, which is also what keeps the default `onKeyUp` from running `onBackPressed`. |
| 11 | `OnKeyListener.onKey(View, KeyEvent)` drops Android's `int keyCode` parameter. | SDK-shape | kept |
| 12 | Catcher hidden with `setAlpha(0f)` plus a background drawable rather than `View.INVISIBLE`. | App choice | kept: an invisible view cannot take focus. Now `android:alpha="0"` in the layout. |

## 3. Layout and drawing

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 13 | Every view placed in absolute pixels with `setPosition`/`setSize`; no XML, no LayoutParams, no weights. | App choice (chrome) / App choice (pages) | **closed for the chrome**: header, footer, page container and key catcher come from `res/layout/activity_main.xml` (`LinearLayout` rows, weights, gravity, `findViewById`). **Kept for the pages**: they are built incrementally (item 4), and their geometry is pixel art tuned to 320x240. |
| 14 | Screen size hard-coded as `Ui.WIDTH`/`Ui.HEIGHT`. | App choice | partly closed: the chrome is `match_parent`; page constants remain for the reason in 13. |
| 15 | Colours, strings and dimensions inline in Java. | App choice | **closed**: `res/values/colors.xml`, `strings.xml`, `dimens.xml`; `Palette` is resolved once from `Resources`; every user-visible string goes through `R.string`. |
| 16 | Right/centre-aligned labels are a `TextView` wrapped in a gravity-set `LinearLayout`. | SDK-forced (no `TextView.setGravity`) | **closed** 2026-09-24: `TextView.setGravity` landed; `Ui.labelRight` / `labelCentred` are one sized `TextView`. The tall display figure keeps its row, since a label cannot centre itself vertically. |
| 17 | Progress bars are nested `FrameLayout`s with `GradientDrawable`s. | SDK-forced (no styled `ProgressBar`, no `Canvas`) | **closed** 2026-09-23: `ProgressBar` tints per instance (gap G3), so `BarView` is one `ProgressBar`; the Limits gauges are `CircularProgressIndicator` rings (gap G2). |
| 18 | Bar charts are arrays of `FrameLayout` boxes. | SDK-forced (no `Canvas`) | open |
| 19 | Large numerals are PNG sprites in `ImageView`s. | SDK-forced (one font size) | closed 2026-09-23: `TextView.setTextSize(64)` |
| 20 | Sprites loaded from `assets/` by string path. | App choice | **closed**: `res/drawable/d0.png`… with `R.drawable.*` and `setImageResource`. |
| 21 | Sprites pre-composited onto the card colour. | SDK-forced (assets lose alpha) | closed 2026-09-23: no sprites left |
| 22 | `GradientDrawable` used as a fluent builder and re-allocated per colour change. | SDK-shape / App choice | kept: the SDK drawable is a builder that applies on `setBackground`; there is no mutate-in-place path. |
| 23 | Widgets built with no-arg constructors. | App choice | **closed**: every widget takes the `Context`. |
| 24 | `animate().alpha().setDuration().start()`. | cosmetic | kept |
| 25 | Hand-written diffing of every on-screen value. | App choice | kept: a full repaint cost 55 ms on the RP2350, over the slow-handler budget, every second. |
| 26 | Repaint throttled by a minute counter instead of `TextClock`. | App choice | kept: same reason; there is no `TextClock`. |

## 4. Threads, timing and data flow

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 27 | Repository was a static singleton started from `Application.onCreate`. | App choice | **closed**: it is `UsageService`, a started and bound `Service` with a `LocalBinder`; the Activity binds in `onStart` and unbinds in `onStop`. |
| 28 | Two raw, unnamed `Thread`s that never stop. | App choice | **closed**: named `usage-poll` and `usage-tick`, both exit when the Service is destroyed. |
| 29 | `picodroid.concurrent.Thread` instead of `java.lang.Thread`. | SDK-shape | kept |
| 30 | A thread that sleeps 1 s and posts a tick, instead of `Handler.postDelayed`. | SDK-forced (no `Handler`, by design) | kept |
| 31 | Poll loop idled by sleeping in 250 ms slices polling a volatile flag. | App choice | **closed**: `Object.wait(ms)` on a lock; `refreshNow()` and `onDestroy` call `notifyAll`. |
| 32 | Cross-thread handoff via `Executors.mainExecutor().execute(...)`. | SDK-shape | kept: the SDK's `runOnUiThread`. |
| 33 | One `Listener` slot with both a data callback and a 1 Hz tick, set in `onResume`/`onPause`. | App choice | kept: there is no `LiveData`; the tick stays in the Service so a wedged fetch can never freeze the countdowns. |
| 34 | Pre-allocated `Runnable` with `@SuppressWarnings("UnnecessaryLambda")`. | App choice | kept: one allocation per second for the life of the app. |
| 35 | App sets the wall clock from the bridge. | SDK-forced (no RTC, no NTP) | open |
| 36 | Connectivity polled with static `NetworkInfo.isConnected()`. | SDK-forced (no `NetworkCallback`) | open |

## 5. Networking and parsing

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 37 | `URL.openConnection()` returns `HttpURLConnection` uncast; `HttpInputStream` not `InputStream`. | SDK-shape | kept |
| 38 | Reply read into a shared `static byte[]`. | App choice | kept: one fetch at a time by construction; a fresh 1 KB buffer per poll is churn the RP2350 heap does not need. Noted in the code. |
| 39 | Cleartext HTTP to a LAN bridge. | SDK-forced (no TLS) | open |
| 40 | `conn.disconnect()` in `finally` because 16 handles exist. | idiom preserved | kept |
| 41 | `UsageSnapshot` is a mutable public-field bag with parallel arrays. | App choice | kept: it is written once by the fetch and read by the UI; a `List<ModelCap>` of records costs allocations for no reader. |
| 42 | `LinkState` as `int` constants with a name table. | App choice | **closed**: an `enum` with `shortText`/`advice` as instance methods. |
| 43 | Bridge host baked at build time through the `NetTestConfig` test hook. | App choice | **closed** as far as the app can: `UsageService` reads `bridge_host` from `SharedPreferences` with `NetTestConfig.HOST` as the default, so an installed unit can be repointed without a rebuild (via `pdb`, or a future settings screen). A `BuildConfig` block is still the right default source; see G7 in the gaps roadmap. |

## 6. Time and number formatting

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 44 | `TimeFormat` does epoch arithmetic by hand with a mutable static UTC offset. | SDK-forced (no `java.time`) | **closed** 2026-09-24: `java.time` landed; `TimeFormat.hm` is `LocalTime` in the bridge's offset, installed with `TimeZone.setDefault`, formatted with `DateTimeFormatter`. |
| 45 | Zero padding by hand instead of `String.format`. | App choice | **closed**: `String.format("%02d")` and friends. |

## 7. Hardware

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 46 | RGB LED via `PeripheralManager.openPwm("GP6")`, the Android Things shape. | SDK-shape (by design) | kept |
| 47 | PWM channels never closed. | App choice | **closed**: `RgbLed implements AutoCloseable`; `onDestroy` closes it. |
| 48 | Missing hardware detected by `catch (RuntimeException)`. | SDK-shape | kept |

## 8. Packaging

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 49 | `PicodroidManifest.xml` with a slash-separated class path, no `<activity>`, no permissions. | SDK-shape | kept; the Activity is now the declared entry point (item 2). |
| 50 | Gradle plugin `picodroid-papk` instead of `com.android.application`. | SDK-forced | open |

## What round 1 did not do, and why

- **Pages in XML.** A page holds 20 to 40 views. Inflating one in a single tick is the thing the
  incremental `Page.buildNext()` exists to avoid (item 4), and `LayoutInflater` has no
  "inflate a few views, come back next tick" mode. The pages also use absolute positions that
  the layout compiler does not express (no margins, no `translationX`). (Inflated
  `LinearLayout`s also carried the theme's 2 px border and padding until 2026-09-24, which every
  container in this app stripped; they are flat now.) The chrome is small enough to inflate in
  one go; the pages are not.
- **`ViewModel` / `LiveData`.** None in the SDK. The `Service` plus `Listener` pair is the nearest
  shape the SDK offers.
- **A settings screen for the bridge address.** Four buttons and no keyboard widget make typing
  an address impractical (gaps roadmap G7). The preference key exists so a later screen, or
  `pdb`, can set it.

## SDK asks (from this app's point of view)

Ordered by how much Android shape each would buy back here.

1. ~~`Activity.onKeyDown` / `onKeyUp` as the fallback when no view consumes a key (item 10; gaps G8).~~ Shipped 2026-09-24.
2. ~~`TextView.setGravity` (item 16).~~ Shipped 2026-09-24. `setTextSize` shipped 2026-09-23 (item 19; gaps G1).
3. ~~A borderless, padding-free container option for inflated `LinearLayout`/`FrameLayout`.~~
   Shipped 2026-09-24 the Android way: every `LinearLayout` / `FrameLayout` is flat by default
   (no border, fill, radius or padding), `setBackgroundColor` honours alpha, and layouts take
   `@android:color/transparent`; `Ui.flat` and its ten call sites are gone.
4. `Handler` / `View.postDelayed` or an equivalent one-shot timer on the main thread (item 30).
5. A minimal `Canvas` (item 18; gap G4). The styled `ProgressBar` (item 17; gap G3) and the ring
   gauge (gap G2) landed 2026-09-23.
6. `ConnectivityManager` with a `NetworkCallback` (item 36).
7. A `BuildConfig` block (item 43; gaps G7).
8. ~~`java.time` or at least `DateFormat`/`DateUtils` (item 44).~~ Shipped 2026-09-24 as a
   port of the JDK classes (fixed-offset zones only).

## Found on the way: resolution-cache growth (runtime, not app; fixed 2026-09-23)

**Fixed in `350c3552`.** The per-executor doubling caches and `cache_push` are gone: method,
field, static and `new` sites now resolve into fixed-size, four-way set-associative tables on the
shared heap (`crates/jvm/src/resolve_cache.rs`, 16 KB on the RP2350), allocated once and kept
across Runnables, so there is no growth request left to refuse. The account below is kept as the
record of what the soak showed. The 20,480 B and 22,528 B requests it mentions are unrelated to
the other large request the heap still makes, the collector's compaction buffer (G10 in the
gaps roadmap).

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

## Verification of round 1

- Sim (2026-09-22, against a live bridge): the four screens match the pre-change build pixel for
  pixel apart from the clock; the inflated chrome sits within a pixel of the old absolute layout;
  X syncs at once (the poll thread wakes from `wait`); Y toggles AUTO with no slow-handler warning
  and AUTO is still on after a restart; a refused host shows the status page with `Bridge down`;
  `[ClaudeUsage] ui ready`, `fetch: refused` and `state -> …` still appear, which is what
  `hil-tests.conf` greps. Old and new both ran 30 AUTO page turns over 8 minutes; see the section
  above for the one difference.
- Hardware (2026-09-22/23, `pico_display2_w` on the bench, live bridge at the PC's WiFi
  address): joins WiFi, syncs every 60 s, all four screens turn on B, X syncs at once, Y toggles
  AUTO with the preference write off the main thread; AUTO survives a power cycle. Two
  slow-handler findings, both fixed in the app: the sync-time refresh (chrome ~20 ms + page
  ~30 ms) now repaints the page in the tick after the chrome and only when the snapshot,
  freshness or minute changed; the page-turn start tick now constructs the page only, with the
  chrome repaint and first update in later ticks. One warning remains, 51 to 74 ms on the first
  tick after a page swap, and it stays there when that tick is emptied down to one log line.
  The redraw cannot be the cause (it runs as its own main-queue task, outside the timed span);
  the instrumented run showed every tick of a swap over budget, not just the first. Tracked as
  **D4 in `claudeusage-gaps-roadmap-2026-09.md`**: root-caused 2026-09-23 as runtime cost
  (cold resolution, frame allocation, native dispatch, interpretation from XIP), not the app;
  the runtime fixes in `350c3552` bring the small steps under budget, and the 2026-09-24
  follow-ups (batched style refreshes, a dispatch memo, one meter row or four bars per step)
  take the Burn and Models build steps under it too; History remains. The boot-time
  `pending-op drain` of ~90 ms is the Service start plus bind, one-off.
