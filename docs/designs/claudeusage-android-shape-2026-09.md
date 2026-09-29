# Roadmap: `claudeusage` toward Android shape

**Status: round 1 landed 2026-09-22 (app-only, no SDK change); SDK asks 1, 2, 3 and 8 landed 2026-09-24 and the app uses them (items 10, 16, 44 and the chrome flattening); ask 5 (`Canvas`) landed 2026-09-26 (item 18); item 1 (Fragments in a `ViewPager2`) closed 2026-09-27; 4 and 7 open. D4 (page-turn stalls) closed 2026-09-25, see the gaps roadmap.**

Completed items: [completed/claudeusage-android-shape-2026-09.md](../completed/claudeusage-android-shape-2026-09.md) — rows 1, 2, 7, 9, 10, 15–21, 23, 27, 28, 30, 31, 36, 42–45, 47; SDK asks 1, 2, 3, 5, 6, 8, 9; "Found on the way: resolution-cache growth".

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
| 3 | Theme colours set by assigning `picodroid.graphics.Theme` static fields. | SDK-shape | kept: now done in `MainActivity.onCreate` from `res/values/colors.xml` before any view exists. |
| 4 | A page's view tree is built a few views per main-thread post, with a build token to survive a page turn mid-build. Android inflates synchronously. | App choice | kept: one page in one tick overran the slow-handler budget on the RP2350. *2026-09-27:* now inside each Fragment: `onCreateView` returns the page's empty root at once and `onViewCreated` starts the `buildNext` chain, each post guarded by `isAdded()` and `getView()` (the Android idiom for work posted from a fragment) in place of the build token. |
| 5 | `catch (OutOfMemoryError \| RuntimeException)` around view building, retrying next tick. | App choice | kept: same reason; a half-built page would otherwise stay invisible. *2026-09-27:* in `UsagePage`, which empties its root and rebuilds on the service's next tick. |
| 6 | Lifecycle overrides declared `public`; Android's are `protected`. | SDK-shape | kept |
| 8 | `onBackPressed()` overridden to a no-op instead of an `OnBackPressedCallback`; also unreachable because `onKey` consumes BACK first. | SDK-forced (no dispatcher) | kept as a guard: if the key catcher ever loses focus BACK would otherwise `finish()` the appliance. |

## 2. Input

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 11 | `OnKeyListener.onKey(View, KeyEvent)` drops Android's `int keyCode` parameter. | SDK-shape | kept |
| 12 | Catcher hidden with `setAlpha(0f)` plus a background drawable rather than `View.INVISIBLE`. | App choice | kept: an invisible view cannot take focus. Now `android:alpha="0"` in the layout. |

## 3. Layout and drawing

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 13 | Every view placed in absolute pixels with `setPosition`/`setSize`; no XML, no LayoutParams, no weights. | App choice (chrome) / App choice (pages) | **closed for the chrome**: header, footer, page container and key catcher come from `res/layout/activity_main.xml` (`LinearLayout` rows, weights, gravity, `findViewById`). **Kept for the pages**: they are built incrementally (item 4), and their geometry is pixel art tuned to 320x240. |
| 14 | Screen size hard-coded as `Ui.WIDTH`/`Ui.HEIGHT`. | App choice | partly closed: the chrome is `match_parent`; page constants remain for the reason in 13. |
| 22 | `GradientDrawable` used as a fluent builder and re-allocated per colour change. | SDK-shape / App choice | kept: the SDK drawable is a builder that applies on `setBackground`; there is no mutate-in-place path. |
| 24 | `animate().alpha().setDuration().start()`. | cosmetic | kept |
| 25 | Hand-written diffing of every on-screen value. | App choice | kept: a full repaint cost 55 ms on the RP2350, over the slow-handler budget, every second. |
| 26 | Repaint throttled by a minute counter instead of `TextClock`. | App choice | kept: same reason; there is no `TextClock`. |

## 4. Threads, timing and data flow

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 29 | `picodroid.concurrent.Thread` instead of `java.lang.Thread`. | SDK-shape | kept |
| 32 | Cross-thread handoff via `Executors.mainExecutor().execute(...)`. | SDK-shape | kept: the SDK's `runOnUiThread`. |
| 33 | One `Listener` slot with both a data callback and a 1 Hz tick, set in `onResume`/`onPause`. | App choice | kept: there is no `LiveData`; the tick stays in the Service, on the main thread, so a wedged fetch can never freeze the countdowns. |
| 34 | Pre-allocated `Runnable` with `@SuppressWarnings("UnnecessaryLambda")`. | App choice | kept: one allocation per second for the life of the app. |
| 35 | App sets the wall clock from the bridge. | SDK-forced (no RTC, no NTP) | open |

## 5. Networking and parsing

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 37 | `URL.openConnection()` returns `HttpURLConnection` uncast; `HttpInputStream` not `InputStream`. | SDK-shape | kept |
| 38 | Reply read into a shared `static byte[]`. | App choice | kept: one fetch at a time by construction; a fresh 1 KB buffer per poll is churn the RP2350 heap does not need. Noted in the code. |
| 39 | Cleartext HTTP to a LAN bridge. | SDK-forced (no TLS) | open |
| 40 | `conn.disconnect()` in `finally` because 16 handles exist. | idiom preserved | kept |
| 41 | `UsageSnapshot` is a mutable public-field bag with parallel arrays. | App choice | kept: it is written once by the fetch and read by the UI; a `List<ModelCap>` of records costs allocations for no reader. |

## 6. Time and number formatting

Items 44 and 45 are closed; see [completed/claudeusage-android-shape-2026-09.md](../completed/claudeusage-android-shape-2026-09.md).

## 7. Hardware

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 46 | RGB LED via `PeripheralManager.openPwm("GP6")`, the Android Things shape. | SDK-shape (by design) | kept |
| 48 | Missing hardware detected by `catch (RuntimeException)`. | SDK-shape | kept |

## 8. Packaging

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 49 | `PicodroidManifest.xml` with a slash-separated class path, no `<activity>`, no permissions. | SDK-shape | kept; the Activity is now the declared entry point (item 2, in the completed doc). |
| 50 | Gradle plugin `picodroid-papk` instead of `com.android.application`. | SDK-forced | open |

## What round 1 did not do, and why

- **Pages in XML.** A page holds 20 to 40 views. Inflating one in a single tick is the thing the
  incremental `Page.buildNext()` exists to avoid (item 4), and `LayoutInflater` has no
  "inflate a few views, come back next tick" mode. The pages also use absolute positions that
  the layout compiler does not express (no margins, no `translationX`). (Inflated
  `LinearLayout`s also carried the theme's 2 px border and padding until 2026-09-24, which every
  container in this app stripped; they are flat now.) The chrome is small enough to inflate in
  one go; the pages are not. *2026-09-27:* the pager is in the XML (`<ViewPager2>` inside
  `page_host`); the pages are still built in code, for the same reason.
- **`ViewModel` / `LiveData`.** None in the SDK. The `Service` plus `Listener` pair is the nearest
  shape the SDK offers.
- **A settings screen for the bridge address.** Four buttons and no keyboard widget make typing
  an address impractical (gaps roadmap G7). The preference key exists so a later screen, or
  `pdb`, can set it.

## SDK asks (from this app's point of view)

Ordered by how much Android shape each would buy back here.

4. `Handler` / `View.postDelayed` or an equivalent one-shot timer on the main thread (item 30, in the completed doc).
7. A `BuildConfig` block (item 43, in the completed doc; gaps G7).

## Verification of round 1

- Sim (2026-09-22, against a live bridge): the four screens match the pre-change build pixel for
  pixel apart from the clock; the inflated chrome sits within a pixel of the old absolute layout;
  X syncs at once (the poll thread wakes from `wait`); Y toggles AUTO with no slow-handler warning
  and AUTO is still on after a restart; a refused host shows the status page with `Bridge down`;
  `[ClaudeUsage] ui ready`, `fetch: refused` and `state -> …` still appear, which is what
  `hil-tests.conf` greps. Old and new both ran 30 AUTO page turns over 8 minutes; see "Found on the way: resolution-cache growth" in
  [completed/claudeusage-android-shape-2026-09.md](../completed/claudeusage-android-shape-2026-09.md) for the one difference.
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
  take the Burn and Models build steps under it too; the 2026-09-25 round (the RP2350's
  flash clock at the pico-sdk's divider instead of the ROM's, `hal/rp/xip.rs`; History four
  bars per step and pre-filled; the Models first paint one card per tick) leaves no page turn
  with a span over 50 ms. The boot-time `pending-op drain` of ~90 ms is the Service start
  plus bind, one-off.
