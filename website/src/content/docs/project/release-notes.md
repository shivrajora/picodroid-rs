---
title: "Release notes"
description: "User-facing changes for Picodroid v0.4.0 onward."
---

This page covers everything that landed in releases v0.4.0 through v0.10.0. Earlier history is in `git log v0.1.0...v0.3.0`.

## v0.10.0 — 2026-06-02

The Android-parity release. Folds in the typed-listener, adapter, and focus-navigation surface that had been accumulating on `main` since v0.9.0, plus a wave of JVM heap and garbage-collector fixes that keep long-running, callback-driven apps alive.

**Android parity**

- **Typed listener interfaces (Tier 1)** and the **`Adapter` pattern (Tier 2)** land as first-class developer surface: `ViewGroup` + `ViewGroup.LayoutParams`, `Adapter` / `AdapterView` / `ArrayAdapter` / `BaseAdapter`, `CompoundButton`, and `DialogInterface`. Listener interfaces now match `android.*` shapes — `View.OnClickListener` / `OnFocusChangeListener`, `AdapterView.OnItemClickListener`, `CompoundButton.OnCheckedChangeListener`, `Spinner.OnItemSelectedListener`, `SeekBar.OnSeekBarChangeListener`, `DatePicker.OnDateChangedListener`, `TimePicker.OnTimeChangedListener`, `SwipeRefreshLayout.OnRefreshListener`, `Keyboard.OnReadyListener`.
- `ArrayAdapter` now renders correctly — `Object.toString()` resolves through the JVM, so adapter-backed `ListView`s show real item text.
- **Context constructors + `Display` cleanup (Tier 4)** round out the parity work.

**Keypad & focus navigation**

- New **View focus API** (`setFocusable` / `requestFocus`) backed by per-Activity LVGL focus groups, plus real **D-pad item selection in `ListView`**. This is what makes button-only devices (no touchscreen) fully navigable.
- `AlertDialog` is now keypad-dismissable (BACK cancels, ENTER confirms) and is torn down whenever its Activity leaves the foreground — no more leaked dialogs.

**JVM & runtime**

- `invokestatic` now walks the superclass chain per JVMS §5.4.3.3.
- **Garbage-collector fixes for callback-driven apps:** Views and dialogs referenced only by native listener maps (key / touch / click / dialog) are now GC roots, fixing input that died ~15 s into a session. Also plugs a native-state root leak and a GC-starvation path.
- **Heap shrink:** `helloworld` peak heap drops 51 KB → 25 KB via a `JvmObject` layout rework (single `Box<[Value]>` field store, `class_idx` side table, tightened layout guard). New **chunked-slot heap storage** plus an RP2350 heap bump 384 KB → 416 KB.
- Past JVM optimisations are now tunable from a board's `[jvm]` `board.toml` section.

**Robustness**

- Bad-APK and poisoned-mutex paths log and early-return instead of panicking.
- A covered Activity no longer receives `onServiceConnected` (fixes a stale bound-service use-after-free) and has its dialogs dismissed when pushed under another Activity; further stale-view UAF and duplicate-launch hardening.

**picoenvmon showcase**

- Pimoroni **Pico Enviro+ Pack** bring-up — display plus I2C BME688 / LTR559 sensors.
- Redesigned to a hub-menu **4-button navigation** model (A=up / B=down / X=open / Y=back), smoothed `HomeActivity` to 1 Hz via a bound service, and fixed the sensordemo "1 event then silent" phantom-IRQ bug.

**Tooling, simulator & docs**

- The simulator now **emulates the physical buttons** via the keyboard plus a headless control channel, runs the real XPT2046 touch driver, and synthesizes BME688 / LTR559 readings instead of zeros.
- New `perfbench` (unified speed + memory) and `graphicsbench` (LVGL render pipeline) benchmarks, each with a composite SCORE.
- Documentation migrated to an **Astro Starlight** site, with a central reference page for the `[jvm]` tunables. Example apps coalesced 59 → 51.

Shrink map: **+23 classes (87 → 110)** covering the Tier 1/2 listener and adapter surface; v0.9.0 entries copied verbatim.

## v0.9.0 — 2026-05-06

The largest release yet. Bundles the licensing, multi-family, and lifecycle work that had been accumulating on `main` since v0.8.0.

**Licensing**

- Project relicensed Apache-2.0 → **GPL-3.0-only** (no Classpath Exception). Shipped a [Contributor License Agreement](/project/cla/) (Harmony FLA-style) and a dual-licensing framework — see [Licensing](/project/licensing/) for details.

**Multi-family architecture**

- `platforms/<family>/` directory replaces the flat `src/hal/<family>/` layout. RP code now lives under `platforms/rp/`; ESP scaffolding lives under `platforms/esp/`.
- New `picodroid-core/` workspace member holds cross-family shared code (no HAL imports).
- HAL CONTRACT v1 — the required public-symbol set every family must expose — is documented in `platforms/rp/src/hal/mod.rs` and compile-time enforced via `platforms/rp/src/hal/contract.rs`.
- Build pipeline generalized via `build_support/{config,freertos,network,boards}.rs` for shared path resolution.

**ESP32-S3 / Lilygo T-Deck Plus (M1)**

- First Xtensa target lands as **Milestone 1** — compile-only. The firmware produces a valid `xtensa-esp32s3-none-elf` ELF and flashes via `espflash`, but FreeRTOS, networking, display, and the LVGL stack are no-ops at this milestone. See the [ESP32-S3 quickstart](/get-started/esp32s3/) and the full [toolchain reference](/reference/esp32s3-toolchain/).
- New cargo aliases `b-tdeck-plus` / `r-tdeck-plus` register the Xtensa target — see [Cargo aliases](/reference/cargo-aliases/).

**Lifecycle and dispatch**

- `Activity` now bootstraps the `Display` singleton **before** `onCreate()`, eliminating a class of null-pointer dereferences in app code that touched the display in `onCreate`.
- `pdb install` no longer panics when the running app never starts an Activity (e.g. a `blinky`-style LED loop).
- `main_queue` splits tick coalescing from cross-task wakes, reducing wakeup latency on busy frames.

**LVGL**

- Bumped 9.2.2 → 9.5.0 (already in v0.6.0; v0.9.0 enables `LV_DRAW_SW_SUPPORT_RGB565A8` on top, fixing aliased rendering for `ImageView.setScaleType` / `setScale`).

**Build & CI**

- `.actrc` lets `act` run the GitHub Actions workflows locally — see [Advanced configuration → .actrc](/reference/advanced-config/#actrc).
- macOS toolchain hardening: switched off the broken `gcc-arm-embedded` cask onto the formula; fixed `libudev-dev` and absolute APK path issues for HIL testing.

Shrink map: byte-identical to v0.8.0 (no new framework classes).

## v0.8.0 — 2026-05-02

**PAPK 1.1 — bundled image assets.** PAPKs gained an `ASST` section that carries pre-decoded PNG images as LVGL-native RGB565 structures mapped to XIP flash. `ImageView.setImageSource("foo.png")` becomes a name-keyed lookup with no on-device PNG decoder. See [Bundled image assets](/guides/assets/) and the new `imagedemo` example.

`papk-pack` and `papk-info` learned the asset table; the runtime resolver registers assets at boot via LVGL's image cache.

Shrink map: byte-identical to v0.7.0 — bundled assets land outside the framework class set.

## v0.7.0 — 2026-05-01

**Tier C widget framework.** Five new widgets (and one new listener) ship in this release:

- [`Snackbar`](/api/ui/#picodroidwidgetsnackbar) — toast with a clickable action lozenge.
- [`DatePicker`](/api/ui/#picodroidwidgetdatepicker) — `lv_calendar` binding.
- [`TimePicker`](/api/ui/#picodroidwidgettimepicker) — `lv_roller` binding, with 12-hour / AM-PM mode.
- [`SwipeRefreshLayout`](/api/ui/#picodroidwidgetswiperefreshlayout) — pull-to-refresh container.
- [`OnSwipeListener`](/api/ui/#picodroidviewonswipelistener) — per-View swipe-direction primitive.
- [`ImageView`](/api/ui/#picodroidwidgetimageview) gained `setScaleType` / `setTint` / `setScale`.
- [`ProgressBar`](/api/ui/#picodroidwidgetprogressbar) gained an indeterminate variant via `ProgressBar.indeterminate()` (`lv_spinner`).

Shrink map: 5 new entries (`a/CE`..`a/CI`); v0.6.0 entries copied verbatim.

## v0.6.0 — 2026-04-30

**Showcase release.** No new framework classes — the [`picoenvmon`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/picoenvmon) feature-showcase app and the LTR559 driver shipped this release. `picoenvmon` demonstrates the manual DI pattern (`ApplicationComponent` / `ActivitySingletonComponent`) in production-shape code.

Shrink map: stable, byte-identical to v0.5.0.

## v0.5.0 — 2026-04-29

**Soft-keyboard polish.** The system soft keyboard:

- Slides up from the bottom edge over ~150 ms when an `EditText` gains focus, and slides back down on dismiss.
- Forwards the OK key through a new `OnEditorActionListener` interface before its default close behavior runs.
- Dismisses on tap-outside.

Plus a new `EditorInfo` constants surface (`TYPE_NUMBER` / `TYPE_EMAIL` / `TYPE_PHONE` / `TYPE_PASSWORD` / `TYPE_TEXT`) for `EditText.setInputType`.

See [`EditText`](/api/ui/#picodroidwidgetedittext) and the polish notes under [`Keyboard`](/api/ui/#picodroidwidgetkeyboard).

Shrink map: 2 new entries (`OnEditorActionListener`, `EditorInfo`); v0.4.0 entries copied verbatim.

## v0.4.0 — 2026-04-27

**DI + Service framework (Preview).** Introduced the `picodroid.app.Service` lifecycle plus the manual DI components used by `picoenvmon`. New surface:

- [`Service`](/api/services/#picodroidappservice) — `onCreate` / `onStartCommand` / `onBind` / `onUnbind` / `onRebind` / `onDestroy`.
- [`IBinder`](/api/services/#picodroidappibinder), [`Notification`](/api/services/#picodroidappnotification-and-startforeground) (with `Notification.Builder`), and `startForeground(int, Notification)` for foreground services.
- [`ServiceConnection`](/api/services/#picodroidcontentcontext--start--bind--stop) for binding lifecycle.
- [Manual DI components](/api/services/#manual-di-applicationcomponent--activitysingletoncomponent): `ApplicationComponent`, `ActivitySingletonComponent`.

Also includes the `servicedemo` example which drives the full Service v1 lifecycle in one non-UI run.

Shrink map: ~10 new entries covering the DI + Service surface; v0.3.0 entries copied verbatim.

## Older releases

For v0.1.0–v0.3.0, see `git log` and the original `docs/` history. Highlights:

- v0.3.0 — `Theme`, gestures (`GestureDetector`, `OnTouchListener`), animations (`ViewPropertyAnimator`), dialogs (`AlertDialog`), `Toast`, `Keyboard`.
- v0.2.0 — `SensorManager` family (BME688), HTTP client, `KeyEvent` / `OnKeyListener`, `Executors` (main + background).
- v0.1.0 — first release cut: 42 framework classes covering peripherals, storage, basic widgets, the JVM core.
