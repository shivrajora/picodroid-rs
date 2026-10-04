# Roadmap: `claudeusage` toward Android shape

**Status: round 1 landed 2026-09-22; the fragment-shape round 2026-09-30 with `picodroid.lifecycle`; the remaining-shape round 2026-10-01 ([claudeusage-remaining-shape-2026-09.md](claudeusage-remaining-shape-2026-09.md)): pages in XML through `AsyncLayoutInflater`, the data flow as repository, ViewModel and immutable UI state, and the SDK signatures and semantics it leaned on. What is left open below waits on the owner decisions F1 to F9 of that document, or is kept on purpose.**

Completed items: [completed/claudeusage-android-shape-2026-09.md](../completed/claudeusage-android-shape-2026-09.md) — rows 1–5, 7–10, 12–23, 25–28, 30–33, 36, 42–45, 47, 51–53, 56, 58–72; SDK asks 1–9; "Found on the way: resolution-cache growth".

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
| 6 | Lifecycle overrides declared `public`; Android's are `protected`. | SDK-shape | kept (B10): widening in a subclass is legal Java, so app code written the Android way compiles as it is. |

## 2. Input

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 11 | `OnKeyListener.onKey(View, KeyEvent)` drops Android's `int keyCode` parameter. | SDK-shape | kept |

## 3. Layout and drawing

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 24 | `animate().alpha().setDuration().start()`. | cosmetic | kept |

## 4. Threads, timing and data flow

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 29 | `picodroid.concurrent.Thread` instead of `java.lang.Thread`. | SDK-shape | kept: owner decision F3. |
| 34 | Pre-allocated `Runnable` with `@SuppressWarnings("UnnecessaryLambda")`. | App choice | kept, smaller: `UsagePage.repaint`, posted once per published state. The post is the tick budget (the chrome's repaint and a page's must not share a tick), not Android idiom; it goes when a combined repaint is measured under budget on the board (E6). |
| 35 | App sets the wall clock from the bridge, and keeps the zone offset in `TimeFormat`. | SDK-forced (no platform time service) | open: owner decision F5. |

## 5. Networking and parsing

| # | Deviation | Tag | Round 1 |
|---|---|---|---|
| 37 | `URL.openConnection()` returns `HttpURLConnection` uncast; `HttpInputStream` not `InputStream`. | SDK-shape | kept: owner decision F3. |
| 38 | Reply read into a shared `static byte[]`. | App choice | kept: one fetch at a time by construction; a fresh 1 KB buffer per poll is churn the RP2350 heap does not need. Noted in the code. |
| 39 | Cleartext HTTP to a LAN bridge. | App choice (HTTPS exists now) | open: owner decision F6. |
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
| 49 | `PicodroidManifest.xml` with a slash-separated class path, no `<activity>`, no `<service>`, no permissions; the theme is chosen by name (`AppTheme`), not by `android:theme`. | SDK-shape | kept: owner decision F4. |
| 50 | Gradle plugin `picodroid-papk` instead of `com.android.application`. | SDK-forced | open |

## 9. Fragment shape (2026-09-30)

What the move onto Fragments left un-Android, from the review in
[claudeusage-fragment-shape-2026-09.md](claudeusage-fragment-shape-2026-09.md). Items 51, 53 and
56 are closed and in the completed doc; these remain, each kept on purpose.

| # | Deviation | Tag | Status |
|---|---|---|---|
| 54 | `picodroid.app.Fragment`, `picodroid.widget.ViewPager2`, `picodroid.lifecycle.*` where Android has `androidx.fragment.app`, `androidx.viewpager2.*`, `androidx.lifecycle`; `FragmentStateAdapter(Activity)` where Android takes a `FragmentActivity`. | SDK-shape | kept: flattening androidx into `picodroid.*` is the project rule. A layout may now spell the element `<picodroid.widget.ViewPager2>`, the fully qualified form Android requires; the app keeps the short one. |
| 55 | `pager.setUserInputEnabled(false)`. | idiom preserved | kept: four buttons and no touch panel, so keys turn the pages. The Android API used as intended. |
| 57 | `MainActivity.getDefaultViewModelProviderFactory()` is overridden to construct `UsageViewModel`, and `onCreateView(String, Context, AttributeSet)` to construct the three custom views the layouts name; on Android both are reflection. | SDK-forced (no reflection) | kept: owner decision F2. Both overrides are valid Android code. |

## 10. Left after the remaining-shape round (2026-10-01)

| # | Deviation | Tag | Status |
|---|---|---|---|
| 74 | `UsagePage.onInflated` calls `View.close()` on a page that finished inflating after its fragment's view was destroyed. | SDK-shape | kept: picodroid frees a view's widget when it is removed, and this one was never added. Not an Android method, documented as such. |
| 75 | The keys are `KEYCODE_DPAD_UP` / `DOWN` / `CENTER` and `KEYCODE_BACK`, and BACK is swallowed so the appliance never finishes. | SDK-shape | open: owner decision F1. The `KEYCODE_BUTTON_*` constants exist; no board maps a button to them. |
| 76 | `Palette`: the colours the code picks between at run time (severity, stale ink, LED), resolved once from `res/values`. | App choice | kept: what a layout can say, it says in XML through the theme and styles; a colour chosen from data is code on Android too (`getColor`). |
| 77 | `BridgeDiscovery` is a UDP broadcast, not `NsdManager`. | SDK-forced (no mDNS) | open: owner decision F7. |
| 78 | The generated protobuf messages are mutable with chained setters, not immutable with builders. | SDK-shape | open: owner decision F8. |
| 79 | `UsageService` is `START_STICKY` with a poll loop that never stops, and no notification. | App choice | kept: an appliance (owner decision F9 records it). |

## What round 1 did not do, and why

- **Pages in XML.** A page holds 20 to 40 views, and inflating one in a single tick is what the
  incremental `Page.buildNext()` existed to avoid. *2026-10-01:* done. `AsyncLayoutInflater`
  inflates a slice per tick, and the layout compiler expresses the pages (margins and gravity in a
  `FrameLayout`, `<include>`, shapes, styles, custom views): rows 4, 5, 13 and 14 in the
  completed doc.
- **`ViewModel` / `LiveData`.** None in the SDK at the time. *2026-09-30:* `picodroid.lifecycle`
  has both, and the pages use them (section 9). *2026-10-01:* the ViewModel owns the data source
  and publishes immutable state (rows 33 and 52 in the completed doc).
- **A settings screen for the bridge address.** Four buttons and no keyboard widget make typing
  an address impractical (gaps roadmap G7). The preference key exists so a later screen, or
  `pdb`, can set it.

## SDK asks (from this app's point of view)

None open. Asks 1 to 9 are in the completed doc: ask 4 (a one-shot timer on the main thread) was
answered by the scheduled executor, now `Executors.mainScheduledExecutor()` (`Handler` and
`postDelayed` are rejected by design), and ask 7 (`BuildConfig`) landed 2026-09-27 and the app
uses it since 2026-10-01. What the app would still gain from is not an ask of the SDK but a
decision: F1 to F9 in [claudeusage-remaining-shape-2026-09.md](claudeusage-remaining-shape-2026-09.md).

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
