---
title: "Android compatibility matrix"
description: "Which android.* classes and idioms Picodroid mirrors, where it diverges, and the picodroid alternative for each gap."
---

Picodroid's goal is that Android code and intuition transfer directly: class
names, method signatures, and semantics track `android.*`. Embedded constraints
force some divergences, and a few Android subsystems are intentionally absent.
This page is the authoritative list of what's full, partial, renamed-only, or
unsupported — and the picodroid alternative for every gap.

The Java SDK lives under `picodroid.*`. Code can also be written against
`android.*` imports and compiled through the [compat-aliases /
stub-jar](#android-import-compatibility) path; either way the same divergences
below apply.

## Status legend

| Status | Meaning |
|---|---|
| **Full** | API surface and semantics match Android closely enough to port unchanged. |
| **Partial** | Present, but a subset of methods/overloads or a documented behavior difference. |
| **Renamed** | Same shape, but only reachable as `picodroid.*` (or via the android alias) — not a real Android class. |
| **Unsupported** | No equivalent; use the listed alternative. |

## By package

### android.app

| API | Status | Notes / alternative |
|---|---|---|
| `Activity` | Full | Lifecycle (`onCreate`/`onStart`/`onResume`/`onPause`/`onStop`/`onRestart`/`onDestroy`), `startActivity`, `startActivityForResult` + `onActivityResult`, `setResult`, `getIntent`, `finish`. |
| `Application` | Full | `onCreate` entry point. |
| `Service` | Partial | Started + bound services, `onRebind`, `stopSelfResult`. No `IntentService`, no foreground-service notification contract. |
| `AlertDialog` / `AlertDialog.Builder` | Partial | Positive/negative/neutral buttons, `setItems`, single- and multi-choice. **List variants cap at ~12 rows** (LVGL renderer limit) and **a message set alongside items wins** (items are dropped, with a `Log.w`) — matching Android's message-vs-items precedence. |
| `Notification` / `NotificationManager` | Partial | Basic post/cancel. No channels, styles, or actions. |
| `Fragment`, `Loader`, `PendingIntent` | Unsupported | No Fragment system — compose with Activities + Views. |

### android.view

| API | Status | Notes / alternative |
|---|---|---|
| `View` | Partial | Geometry/visibility/enabled/tag/id, `OnClickListener`, `OnLongClickListener` + `performLongClick`, `OnTouchListener`, `OnKeyListener`. No `findViewById` (no resource IDs — keep references or use `setTag`/`getTag`); no `post`/`postDelayed` (use `getMainExecutor()` or animation timers). |
| `ViewGroup` / `ViewPropertyAnimator` | Partial | `animate()` with `translationX/Y`, `alpha`, `scaleX/Y`, `setInterpolator`, `withEndAction`. |
| `MotionEvent` | Partial | `getX`/`getY` are **view-relative**, `getRawX`/`getRawY` are screen-absolute, matching Android. **Coordinates are `int`, not `float`** (no FPU). |
| `GestureDetector` | Partial | `OnGestureListener` + `SimpleOnGestureListener`; slop/fling use raw coordinates. |
| `KeyEvent` | Partial | D-pad / button codes for button-only boards. |
| `LayoutInflater`, XML layouts, `Menu` | Unsupported | No resource/XML layout system — build View trees programmatically. |

### android.widget

| API | Status | Notes / alternative |
|---|---|---|
| `TextView`, `Button`, `LinearLayout`, `ImageView`, `Switch`, `CheckBox`, `ToggleButton`, `RadioButton`/`RadioGroup`, `ProgressBar`, `SeekBar`, `Toast`, `Spinner`, `NumberPicker`, `EditText`, `ListView` | Partial–Full | Core widgets present. See specific divergences below. |
| `ProgressBar` | Partial | `indeterminate()` is **creation-time only** — `setIndeterminate(boolean)` after construction is unsupported (LVGL can't morph bar↔spinner). |
| `Spinner.OnItemSelectedListener` | Partial | Full 4-arg `onItemSelected(parent, view, position, id)`; **`view` is always null** (LVGL rows have no Java wrapper) and `parent` is the `Spinner` (no `AdapterView`). |
| `ImageView` | Partial | `SCALE_FIT_CENTER`, `SCALE_CENTER`. Source is a bundled asset name (see [assets](/guides/assets/)). |
| `EditText` + `TextWatcher` | Partial | `TextWatcher` takes **`String`** (no `CharSequence`/`Editable`); **only `afterTextChanged` fires** in v1. |

### android.util

| API | Status | Notes / alternative |
|---|---|---|
| `Log` (`v`/`d`/`i`/`w`/`e`) | Full | Maps to defmt levels on device; the simulator prints every level as `[Tag] msg`. Filter by tag/level with `pdb logcat --stdin`. |

### android.graphics

| API | Status | Notes / alternative |
|---|---|---|
| `Color` | Full | Named constants + ARGB ints. |
| `drawable.GradientDrawable` | Partial | Solid/gradient fills, corner radius. |
| `Canvas`, `Paint`, `Bitmap` | Unsupported | Drawing is via LVGL widgets, not an immediate-mode `Canvas`. |

### android.content

| API | Status | Notes / alternative |
|---|---|---|
| `Intent` | Partial | Explicit (class-targeted) intents + extras. No implicit intents / `IntentFilter` resolution. |
| `Context` | Partial | `getMainExecutor`, `getDisplay`, service access. No `getSystemService` (services are exposed directly), no `getResources` (bundle files under `assets/` → generated `AssetConstants`), no `registerReceiver` (no `BroadcastReceiver`). |
| `SharedPreferences` / `Editor` | Full | Backed by LittleFS. |
| `DialogInterface` | Full | `OnClickListener`, `OnDismissListener`, `OnMultiChoiceClickListener`, button constants. |

### android.os

| API | Status | Notes / alternative |
|---|---|---|
| `SystemClock` | Full | `uptimeMillis`/`elapsedRealtime`. |
| `Handler` / `Looper` / `Message` | Unsupported | Use `getMainExecutor().execute(Runnable)` for "post to UI", and the animation engine's timers / internal popup timeouts for delayed work. There is no `postDelayed`. |
| `Bundle` | Partial | Intent extras only. |

### android.hardware

| API | Status | Notes / alternative |
|---|---|---|
| `Sensor` / `SensorManager` / `SensorEvent` / `SensorEventListener` | Partial | Board-dependent sensors; registration + event callbacks. |

### Concurrency

| API | Status | Notes / alternative |
|---|---|---|
| `Thread` (`picodroid.concurrent.Thread`) | Partial | On device, `start()` spawns a real FreeRTOS task. **In the simulator it is a no-op and logs a warning** — there is no threading in sim. |
| `Executor` / `Executors` (`mainExecutor` / `backgroundExecutor`) | Full | The recommended concurrency primitive — this is how you "post to the UI thread". |

### java.* standard library

| API | Status | Notes / alternative |
|---|---|---|
| `Object.clone()` / `Cloneable` | Partial | Shallow copy works, but **the `Cloneable` check is skipped** — `clone()` never throws `CloneNotSupportedException`. |
| `Throwable` | Partial | `addSuppressed`/`getSuppressed`/`getCause` stored; `ExceptionInInitializerError` wraps `<clinit>` throws. **A failed `<clinit>` does not poison the class** (no `NoClassDefFoundError` on re-access). |
| `Comparator` + `Collections.sort(List, Comparator)` | Full | Lambda comparators supported. |
| `Class.getName()` | Full | Returns dot-form (`pkg.Class`) per the Java spec. |
| `String.split` | Partial | Literal delimiters only — **no regex**. |
| `BufferedReader` / `InputStreamReader` | Unsupported | Use the byte-oriented `picodroid.io` streams (`FileInputStream`/`FileOutputStream`); there is no char-stream reader layer. |

## Cross-cutting divergences

- **Coordinates and sizes are `int` px.** There is no `float` `MotionEvent`
  coordinate, and no density-independent units — no `dp`/`sp`, no
  `getResources().getDisplayMetrics()`. Lay out in pixels.
- **No resources system.** No `R` class, no `res/` directory, no XML layouts,
  drawables, or strings. Bundle binary assets under `assets/` and reference
  them through the generated `AssetConstants` (see [assets](/guides/assets/)).
- **No `Handler`/`Looper`.** The main loop is an executor-driven dispatcher;
  use `getMainExecutor()` and animation timers.
- **Custom `Interpolator`s fall back to linear.** Standard interpolators
  (linear/accelerate/decelerate/accelerate-decelerate) map to native easing; an
  app-defined `Interpolator` can't be up-called from the native tick, so it
  falls back to linear with a `Log.w`.

## Android import compatibility

Apps can be written against `android.*` imports instead of `picodroid.*`:

- The build generates an `android.*` stub jar (a `picodroid.` → `android.`
  rename of the SDK) so the imports compile.
- The [class-shrinker](/reference/shrinker/)'s `--compat-aliases` pass rewrites
  the resulting `android/*` bytecode references to the real `picodroid/*`
  classes at pack time.

Enable it with `picodroid.compatAliases=true` (see `examples/androidport`).
**Scope: class-name aliasing only.** It does not provide `android.R`, the
resources system, or any class that has no `picodroid.*` equivalent — those
remain [Unsupported](#by-package) regardless of which import style you use.
