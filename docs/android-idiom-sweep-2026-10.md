# The Android-idiom sweep, 2026-10-26

> **Status:** landed 2026-10-26 (`6d213518`, docs in the commit after it); the second round
> closed follow-ups 1–4 and 6 the same day. What is still open is listed at the end.

## Why

A survey of the in-repo apps against Android found that most remaining divergences were apps
not using an Android form that existed: no `res/` in seven showcase apps, no-Context
constructors, `Intent(Class)`, `SensorManager.getInstance()`, static `NetworkInfo.isConnected()`
polling where `NetworkCallback` exists, `setSize` arithmetic where weights and margins work, hand
insertion sorts, short-form manifests, no ViewModel / LiveData in picoclock or picoenvmon, and
comments justifying old idioms by limits that are gone. What kept them alive was the SDK still
offering the non-Android spellings. Picodroid has no outside users, so the spellings were
deleted and the apps migrated in the same sweep; the build now teaches the Android form. The
rationale is in [`decisions.md`](decisions.md) (2026-10-26).

## What changed in the SDK

| Before | After |
|---|---|
| No-Context constructors on every widget, `Button(String)`, `Button(Context, String)`, `ToggleButton(String, String)`, `SeekBar(int)`, `ArrayAdapter(T[])` / `()` / `(Context)` / `(Context, T[])`, `GestureDetector(listener)`, `AlertDialog.Builder()`, `Notification.Builder()` | deleted; Android's constructors only (`Notification.Builder(Context, String)` added) |
| `Intent(Class)` | deleted; `Intent(Context, Class)` |
| `View.setSize` / `setPosition` public, `setLayoutParams` stored only | `setLayoutParams` applies at once (a parented view re-takes size, weight, gravity, margins; params of a plainer class clear what earlier ones set; a parentless view is sized now; null throws); `setSize` / `setPosition` package-private; `setContentView` applies the root's params or `MATCH_PARENT` |
| `LinearLayout.setSpacing`, `GridLayout.setSpacing` | deleted (Java, Rust arms, LVGL helpers); margins |
| `SharedPreferences.open` | private; `getSharedPreferences` |
| `*Manager.getInstance()`, `PackageManager` / `Resources.getInstance()`, `Display.getInstance()`, static `NetworkInfo`, `WifiManager.getLastError`, `Drawable.applyTo` | `@Hide` (see below) |
| `ScanResult.isSecured`, `DatePicker.setDate` (one-based) / `getDay`, `GradientDrawable.setGradient`, chaining setters, int orientations | deleted; `capabilities`, `updateDate` / `getDayOfMonth`, `GradientDrawable(Orientation, int[])` / `setColors` / `setOrientation`, `void` setters, `setCornerRadius(float)`, `Orientation` constants |
| `TimePicker.setTime` | private; `setHour` / `setMinute` |
| — | added: `picodroid.R` (framework row ids with Android's values), `Context.getApplicationContext()`, `Activity` / `Service.getApplication()`, `Context.getDisplay()` (moved from `Activity`), `TextView.setText(int)`, default params for a never-sized self-drawing child in `addView(View)` |

**`@Hide`** (`picodroid.annotation.Hide`) is Android's `@hide`: a member the framework reaches
across its own packages stays `public`; `verifyApiContract` reads the annotation from the SDK's
classes on the app's compile classpath and fails an app that references the member, or an
override of it, printing the annotation's hint (the Android spelling). Compile-only, stripped
from firmware: no flash.

**Runtime fix found on the way:** the main-thread queue and the scheduled table were initialised
at the top of the activity loop, after `Application.onCreate`, so a post or schedule from there
(a `NetworkCallback`'s first `onAvailable`, a `LiveData.postValue`) was dropped. `boot` now
initialises both before the app's first callback.

**Bug fixed:** picoclock's `Buzzer` opened GP13 through `PeripheralManager`, which the touch kit
reserves for its `[audio]` buzzer, so `openPwm` threw and alarms only flashed the LED. The app
rings through `ToneGenerator` now (`Ringer.java`); verified in the sim on `pico_touch_kit`
(PWM log on GP13 at 1760 / 2349 Hz, no "no buzzer" line). Not yet heard on the board.

## The apps

Every example and system app migrated (Context constructors, `Intent(Context, Class)`, layout
params / weights / margins, `getSystemService`, `getMainExecutor()` / `View.post` outside the
executor demos, `Collections.sort`). Beyond that:

- **picoclock:** ToneGenerator ringer; ViewModels (`ClockViewModel`, `AlarmListViewModel`,
  `AlarmEditViewModel`, `SetTimeViewModel`) over a `LiveData<Alarm[]>` store; the service's
  ticker thread replaced by one scheduled task armed while ringing; stale-limit comments and
  their workarounds deleted; long-form manifest.
- **picoenvmon / picoenvmon_kt:** `res/values` strings, colours, dimens; `LiveViewModel`,
  `HistoryViewModel`, a `NetworkStatus` LiveData fed by `registerDefaultNetworkCallback`;
  long-form manifests.
- **weather, askclaude, calculator, tutorial_screens, tutorial_service:** `res/values` strings
  (calculator has no concatenated UI text), `NetworkCallback` in weather, calculator's `flatten()`
  and `MAX_FINITE` workarounds deleted (`make_flat`, `Double.isFinite`), the tutorials'
  Application classes deleted (HomeActivity is the MAIN entry), long-form manifests.
- **settings:** `capabilities`-string security test, the supplicant-error broadcast for the
  wrong-password status, a `NetworkCallback` for the connected status; geometry unchanged
  (`board-lib.sh` coordinates hold).
- **QA suites:** each check of a removed shortcut now checks the Android form; new checks for
  `setLayoutParams` on a parented view (width, margins, plainer params clear the weight;
  sabotage-checked), `setLayoutParams(null)`, `setContentView` without params, the gradient
  constructor. `qa_store` / `prefs_demo` disk-roundtrip checks now go through the one shared
  instance (cross-boot persistence remains the disk proof).

## Follow-ups

The first round's six follow-ups, and what the second round (2026-10-26) did with each:

1. **Short-form manifests: closed.** The Android form is the only one: `<application
   android:name>`, the entry Activity declared with the MAIN filter, `android:versionName` /
   `versionCode`, the `android:` prefix required, `<uses-permission>` under `<manifest>`; the old
   attributes fail the build with the spelling to write. All 113 manifests migrated. The framework
   launches the entry Activity after `Application.onCreate` (an explicit `startActivity` there still
   wins; receiver runs stay headless), so 30 apps lost their launch call and 17 Application
   classes were deleted. Design amendment in `designs/manifest-components-2026-10.md`.
2. **Context in the DI graph: closed.** `@picodroid.di.ApplicationContext Context` and
   `Application` are injectable anywhere, as with Hilt; an unqualified `Context` is a compile error
   naming the fix. picoclock and picoenvmon(_kt) dropped their static application context.
3. **`LinkProperties`: closed.** `ConnectivityManager.getLinkProperties(network)` with
   `LinkProperties.getLinkAddresses()` / `LinkAddress.getAddress()` (the link's IPv4 address on
   any link, the simulator's host link included); picoenvmon and netdemo read the address there.
4. **Remaining spellings: closed.** `Intent.getComponent()` (`getTargetClassName` is `@Hide`),
   `getSystemService(Class)` / `getSystemServiceName`, `Notification.extras` with `EXTRA_TITLE` /
   `EXTRA_TEXT` (the getters went), `DisplayDebug.setContentView` / `renderFrame` for a bench
   (`Display.setContentView` / `update` are `@Hide`), `ListView.addItem` deleted and
   `Spinner.setItems` private, `GestureDetector` in Android's shape (`onTouchEvent`, the six
   callbacks, `SimpleOnGestureListener`; the old one never saw a fling since the runtime reuses one
   `MotionEvent`, hence `MotionEvent.obtain(MotionEvent)`), `ProgressBar(ctx)` the spinner and
   `R.attr.progressBarStyleHorizontal` the bar, `ImageView.ScaleType` / `setImageTintList` /
   `setColorFilter` (`setImageSource`, `setScale`, the `SCALE_*` ints went).
5. **Smaller divergences: closed** except the last. `ArrayAdapter` notifies on change
   (`setNotifyOnChange`); `ScrollView` draws no background; manifest labels take `@string/`;
   `<uses-permission>` is read under `<manifest>`. Still open: an app with no Activity runs no event
   loop, so it hears no `NetworkCallback` (its `Application.onCreate` returns and the run ends).
6. **Seen bugs: closed.** picoenvmon History's 52-vs-59 was `ListView.setSelection` leaving the
   keypad focus on a re-bound row; it now moves the focus with the selection when a row holds it
   (qa_list row pins it). askclaude's DPAD_DOWN was not a bug: the panel had dozed and the waking
   key is swallowed by design. The Settings wrong-password status is Android's behaviour and stays.

Still open after round 2:

- An app with no Activity hears no `NetworkCallback` (follow-up 5).
- The entry Activity's `getIntent()` is null; Android hands it an `ACTION_MAIN` Intent.
- A manifest `icon` names a file under `assets/`, not `@drawable/` (imagedemo keeps a copy of its
  logo under `assets/` for it).
- `ProgressBar.setIndeterminate(true)` throws on a horizontal bar (LVGL cannot switch a bar into
  a spinner); `ImageView.ScaleType.FIT_START` / `FIT_END` draw as `FIT_CENTER`.
- menudemo's sim row times out now and then (a key sent right after `menu shown` is lost: a DOWN
  or the held SELECT lands before the list takes the focus); main's nightly shows it too
  (2026-10-26, 2026-10-29).
