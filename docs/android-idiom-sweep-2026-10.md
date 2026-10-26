# The Android-idiom sweep, 2026-10-26

> **Status:** landed 2026-10-26 (`6d213518`, docs in the commit after it). The follow-ups
> below are open.

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

## Follow-ups (open)

1. **Short-form manifests** remain accepted and are used by about a hundred demo and QA apps;
   only the showcase apps moved to the long form. Dropping the short form is the same kind of
   sweep.
2. **No Context in the DI graph** (Hilt's `@ApplicationContext`): picoclock and picoenvmon hold
   the Application statically for their modules. A provider of the application `Context` in the
   generated component would remove that.
3. **No `LinkProperties`:** an app has no unhidden way to read the address on a non-WiFi board,
   and in the sim `WifiInfo.getIpAddress()` is 0 because the simulated station never reports
   itself joined while the host link is up (a sim divergence: picoenvmon's Network screen shows
   "IP: -" in the sim).
4. **Remaining non-Android spellings:** `ProgressBar.indeterminate()`,
   `ImageView.setImageSource(String)`, `Intent.getTargetClassName()` (Android:
   `getComponent()`), `Notification.getContentTitle/Text`, `Display.update` /
   `setContentView`, `GestureDetector`'s listener shape, no `getSystemService(Class)`.
5. **Smaller divergences found:** `ArrayAdapter.add` / `clear` do not notify the list (Android's
   `notifyOnChange`); `ScrollView` paints the surface colour (Android's is transparent); manifest
   `label` takes no `@string/`; `<uses-permission>` is read only under `<application>`; an app with
   no Activity hears no `NetworkCallback` (netdemo, http_get, https_get keep a short wait on
   `getActiveNetwork()`).
6. **Seen, not caused by the sweep:** picoenvmon History's info dialog names sample 52 while the
   list shows 59; askclaude's DPAD_DOWN may be taken by its ScrollView; Settings shows "Not
   connected" rather than "Wrong password" when a join fails while the Wi-Fi screen is not
   resumed (the error now comes only from the broadcast, as on Android).
