# Broadcasts: `BroadcastReceiver`, `IntentFilter`, `registerReceiver`, `PendingIntent.getBroadcast`

**Status: in progress 2026-10-18.** §4 says which stages have landed; amendments at the bottom
record where execution diverged.

## 0. Why

The Android gap analysis listed "no system broadcasts": no `BroadcastReceiver`, `IntentFilter`
or `registerReceiver`. The costs were visible in the apps. `claudeusage`'s ViewModel woke once a
second to notice the minute changing, where Android has `ACTION_TIME_TICK`. Settings' Wi-Fi
screen re-read the join state every 500 ms "because picodroid has no broadcasts". `picoclock`
kept a Service alive to receive its own alarms because `PendingIntent.getBroadcast` did not
exist, and nothing re-armed its alarms after a time or zone change. An appliance had no
`BOOT_COMPLETED`. The runtime said so itself: a `registerReceiver` call hit the API hint "no
BroadcastReceiver — use a bound Service or a direct callback".

The runtime already had the shape a broadcast needs, three times over: a writer bumps a
generation counter, the main loop compares it once per tick, one static Java bridge is called
through a `dispatch_sites.rs` row, and Java fans out from a registry it owns
(`ConnectivityManager.fireLinkChange`, `WifiManager.fireEvent`, `AlarmManager.fireAlarm`). This
doc adds a fourth instance of that shape, with a fixed set of system actions behind a bitmask,
the receiver registry in Java, and the alarm table extended so a `PendingIntent` can target a
receiver.

## 1. What an app writes

```java
// Context-registered, exactly Android's idiom.
IntentFilter f = new IntentFilter(Intent.ACTION_TIME_TICK);
f.addAction(Intent.ACTION_TIME_CHANGED);
f.addAction(Intent.ACTION_TIMEZONE_CHANGED);
registerReceiver(tick, f);                         // or (tick, f, Context.RECEIVER_NOT_EXPORTED)
unregisterReceiver(tick);

// An alarm that delivers to a receiver instead of starting an Activity.
PendingIntent pi = PendingIntent.getBroadcast(this, id,
    new Intent(this, AlarmReceiver.class).putExtra("alarm", id), PendingIntent.FLAG_IMMUTABLE);
alarmManager.setExact(AlarmManager.RTC_WAKEUP, when, pi);

// In-app broadcasts.
sendBroadcast(new Intent("picoclock.ALARM_EDITED"));
```

## 2. The Android-facing API

| Android | Here | Notes |
|---|---|---|
| `BroadcastReceiver.onReceive(Context, Intent)` | same | main thread, between frames, like every callback; `context` is the Context the receiver was registered through |
| `IntentFilter(String)`, `IntentFilter(IntentFilter)`, `addAction`, `hasAction`, `matchAction`, `countActions`, `getAction(int)` | same | at most 4 actions per filter; no data, type or category matching |
| `Context.registerReceiver(r, f)` / `(r, f, flags)` | same; returns `null` | no sticky broadcasts; `RECEIVER_EXPORTED` / `RECEIVER_NOT_EXPORTED` accepted and without effect; at most 8 receivers per app; a second registration of the same receiver throws `IllegalArgumentException`, as on Android |
| `Context.unregisterReceiver(r)` | same | `IllegalArgumentException` if not registered; an Activity destroyed with a receiver still registered has it unregistered for it, with Android's "has leaked IntentReceiver" warning |
| `Context.sendBroadcast(Intent)` | same | asynchronous (one main-thread turn later); in-app only, `setPackage` is refused; an Intent naming a receiver class is delivered to a new instance of it, an action to the registered receivers; a receiver that throws is logged and the round goes on |
| `Intent(String)`, `setAction` / `getAction` | same | `action` is `Intent`'s fourth instance field, after the three the native `startActivity` reads by slot |
| `Intent.ACTION_TIME_TICK`, `ACTION_TIME_CHANGED` (`"android.intent.action.TIME_SET"`), `ACTION_TIMEZONE_CHANGED` + `EXTRA_TIMEZONE`, `ACTION_SCREEN_ON` / `OFF`, `ACTION_BOOT_COMPLETED`, `ACTION_MAIN` | same string values | Android's literal action strings, which the manifest packer already matched (`android.intent.action.MAIN`) |
| `WifiManager.SUPPLICANT_STATE_CHANGED_ACTION` (+ `EXTRA_NEW_STATE`, `EXTRA_SUPPLICANT_ERROR`), `NETWORK_STATE_CHANGED_ACTION`, `SCAN_RESULTS_AVAILABLE_ACTION`; `ConnectivityManager.CONNECTIVITY_ACTION` | same | network boards; `WifiManager.getLastError()` stays, no longer the only way to the join's verdict |
| `PendingIntent.getBroadcast(ctx, code, intent, flags)` | same | an explicit receiver class or an action; two `int` extras, as `getActivity`; multi-app boards |
| manifest `<receiver>` with `BOOT_COMPLETED` | see §4 stage 6 | one app runs at a time, so a delivery to another package is a headless run of it |
| ordered and sticky broadcasts, `sendOrderedBroadcast`, `abortBroadcast`, `goAsync`, `LocalBroadcastManager`, `WIFI_STATE_CHANGED_ACTION`, `BATTERY_*` | not here | no source, or no need; the API hints name the alternative |

## 3. How it works

### 3.1 The registry is Java's

`BroadcastReceiver` keeps the app's registrations in package-private statics — the receivers,
their filters and the Context each was registered through — allocated on the first
`registerReceiver`, so an app that never registers pays nothing. The heap roots them through
the static; the heap reset at an app switch (`boot::run_app`) clears them. Nothing in Rust holds
a receiver, so there is no root provider, no reset list entry for them and no teardown. The model
is `ConnectivityManager` (`sdk/java/picodroid/net/ConnectivityManager.java`): a `synchronized`
registry, a snapshot before each fan-out, unregister allowed from inside a callback.

Two Android details sit on top of that model. Before each `onReceive` the dispatcher re-checks
that the receiver is still registered, so one unregistered by an earlier receiver in the same
round is skipped (`LoadedApk.ReceiverDispatcher`'s `mForgotten`). And each call is wrapped: a
receiver that throws is logged and the round goes on, per receiver, the policy the main loop
already applies to a posted `Runnable`. Android would crash the app; picodroid never has.

An Activity's `performDestroy` calls `Context.unregisterLeakedReceivers()` after `onDestroy`
returned (where an app's own `unregisterReceiver` belongs): what that Activity registered and
left behind is dropped with Android's exact warning text. A `static boolean` on `Context` keeps
the destroy path from touching the registry, or loading the class, in an app that never
registered. Service-owned receivers are not reaped: `Service.onDestroy` has no Java trampoline.

### 3.2 One interest mask in Rust

The system actions are bits (`BroadcastReceiver.BIT_*`, mirrored in
`crates/picodroid-core/src/broadcasts.rs`). Every registration change recomputes the OR of the
registered filters' bits and, when it changed, pushes it through one native,
`BroadcastReceiver.nativeSetInterest(int)`, into an atomic. A tick with no listener costs one
load. The mask is on `run_app`'s reset list: the previous app's registrations died with its heap,
and their interest must not make the loop upcall into this app's empty registry.

### 3.3 One upcall per frame

`lifecycle/broadcast_events.rs` runs after the connectivity and Wi-Fi dispatches in
`run_activity`'s tick arm. With a non-zero interest mask it folds every source's edge since the
last tick into one mask, ANDs it with the interest, and makes at most one call:
`BroadcastReceiver.fireSystem(int mask)` (a `dispatch_sites.rs` row, `BROADCAST`). Java builds
one `Intent` per set bit, in bit order — a clock set before the tick that follows it — and fans
out. Ints cross the boundary, never strings, so no interning. The dispatch is skipped while a
cross-package launch is pending (`handler.has_pending_launch()`), the rule the alarm poll
applies: the Activity stack is about to be torn down, and an `onReceive` that called
`startActivity` would push into it.

The sources:

- `SCREEN_OFF` / `SCREEN_ON`: a pending bit set in `service_display_power`'s `Doze` / `Wake`
  arms (the same task as the loop; a plain cell).
- `TIME_TICK`: the wall-clock minute (`broadcasts::minute_of(elapsed + wall_offset)`) differs
  from the one the last tick saw; computed only while the bit is wanted, the "last minute"
  initialised at loop start so the first tick is silent. Delivered while the panel dozes too:
  the loop keeps ticking in doze, the CPU never sleeps, and Android's `TIME_TICK` is a
  non-wakeup alarm that fires whenever the CPU is awake.
- `TIME_CHANGED`: a generation bumped inside `os/system_clock.rs::anchor_wall_clock`'s
  `AtomicSection` — always for the Java `setCurrentTimeMillis` native, and for the timesync task
  only when the clock moved by two seconds or more (a six-hourly drift correction is not a
  "set"; Android's `NetworkTimeUpdateService` has the same threshold idea). Both writers run on
  core 0 under the section, so a load-then-store counter is sound. The loop's seen value starts
  at the current generation, so the simulator's boot anchor never broadcasts.
- `TIMEZONE_CHANGED`: a generation bumped in `time_service::set_zone_offset_minutes` when the
  offset actually changed. Java adds `EXTRA_TIMEZONE` from `TimeZone.getDefault().getID()`.
- Wi-Fi: when `WIFI_EVENTS` moved, the station status compared with the last one seen gives the
  supplicant bit, the scan generation the scan bit; `LINK_CHANGES` (already read by the
  connectivity dispatch) gives the network-state and connectivity bits.

### 3.4 `PendingIntent.getBroadcast` reuses the alarm table

`alarms::Entry` gains a target kind — Activity, receiver class, or action — that is part of the
identity a `set` replaces and a `cancel` names, and the 48-byte class field holds the receiver
class or the action string. `AlarmManager.fireAlarm` gains the kind: a receiver class is
instantiated through `Class.forName(...).newInstance()` and heard once (the same path as a
`sendBroadcast` to an explicit component); an action goes to `BroadcastReceiver.dispatch`, with a
warning when nothing is registered for it. The Context handed to `onReceive` is the Application.
No second table; the kind byte lands in the row's padding.

`run_application`'s Activity-less path — an app whose `Application.onCreate` pushed nothing —
used to drain its ops and exit without ever polling alarms, so a cold wake for such an app
burned both of its `MAX_WAKE_ATTEMPTS`. It now polls once, in a bounded loop, and delivers every
`Fire` for the running package before tearing down.

## 4. Stages

Each stage ships on its own with its own ratchet accept. Status is kept here.

| # | Stage | Status |
|---|---|---|
| 0 | This doc, index row, amendments in the docs it supersedes | 2026-10-18 |
| 1 | Java core: `BroadcastReceiver`, `IntentFilter`, `Intent.action` + constants, `Context.registerReceiver` / `unregisterReceiver` / `sendBroadcast`, the leak reap, the API hints, `examples/qa_bcast` | in progress |
| 2 | System actions from the loop: `broadcast_events.rs`, the `BROADCAST` row, screen, tick, time, zone | open |
| 3 | Network actions: supplicant, network state, scan, connectivity; Settings' Wi-Fi screen on receivers | open |
| 4 | `PendingIntent.getBroadcast`: the alarm kind, `fireAlarm` by kind, the headless poll | open |
| 5 | The consumers: `claudeusage` on `TIME_TICK`, `picoclock` on `getBroadcast` + `TIME_CHANGED` / `TIMEZONE_CHANGED` | open |
| 6 | Manifest `<receiver>` and `BOOT_COMPLETED`: XSD, Gradle, `papk-pack`, `papk-format`, the boot chain | open |
| 7 | Website, compatibility matrix, release notes; closed items moved | open |

### Where each stage lands

- **1** `sdk/java/picodroid/content/{BroadcastReceiver,IntentFilter,Intent,Context}.java`,
  `sdk/java/picodroid/app/Activity.java` (`performDestroy`), `crates/picodroid-core/src/broadcasts.rs`
  (the mask, the bits, `minute_of`), `native_handler/os.rs` (`nativeSetInterest`),
  `native_handler/method_tables.rs`, `native_handler/class_registry.rs` (the native class, the
  hints, the `Intent` slot test), `boot.rs` (the reset), `examples/qa_bcast`.
- **2** `crates/picodroid-core/src/lifecycle/broadcast_events.rs`, `lifecycle/mod.rs` (the call, the
  pending screen bits), `dispatch_sites.rs` (`BROADCAST`), `os/system_clock.rs` (the generation and
  the step threshold), `time_service/mod.rs` (the zone generation), `time_service/task.rs`.
- **3** `broadcast_events.rs`, `sdk/java/picodroid/net/wifi/WifiManager.java`,
  `sdk/java/picodroid/net/ConnectivityManager.java`, `system-apps/settings/.../WifiActivity.java`.
- **4** `sdk/java/picodroid/app/{PendingIntent,AlarmManager}.java`, `alarms.rs`,
  `native_handler/os.rs::alarm_set`, `lifecycle/alarm_events.rs`, `lifecycle/mod.rs::run_application`.
- **5** `examples/claudeusage/.../UsageViewModel.java`, `examples/picoclock/...`.
- **6** `sdk/PicodroidManifest.xsd`, `buildSrc/.../ManifestSchema.kt`, `ManifestComponentsTask.kt`,
  `PapkPackTask.kt`, `PicodroidPapkPlugin.kt`, `tools/papk-pack/src/main.rs`,
  `crates/papk-format/src/lib.rs`, `packages.rs`, `boot.rs`, `platforms/rp/src/boot_tasks.rs`,
  `sim_boot.rs`, `scripts/sim-run.sh` (a boot lane).
- **7** `website/src/content/docs/reference/compatibility-matrix.md`, `api/services.md`,
  `api/networking.md`, `reference/manifest.md`, `guides/debugging.md`, `reference/limits.md`,
  `project/release-notes.md`.

## 5. Caps and costs

| Thing | Cap | Precedent |
|---|---|---|
| receivers registered per app | 8 (`IllegalStateException` beyond) | `ConnectivityManager.MAX_CALLBACKS` |
| actions per `IntentFilter` | 4 | `WifiManager`'s callback table |
| system actions | bits of a `u32`; ten used | — |
| upcalls per frame | 1, carrying the mask | alarms, sensors |
| broadcast alarms | share the 12 rows / 8 per package | `alarms.rs` |

Flash is measured per stage with `parity-bench.sh --size-only` and accepted with a `size:`
trailer; the figures go in the amendments as each stage lands.

## 6. Verification

After every stage: `./scripts/sim.sh --app helloworld` prints `[HelloWorld] Hello, World!`,
`./scripts/pre-commit` ends `==> All checks passed.`, new rows pass `check-hil-conf.sh`, and
`./scripts/sim-run.sh --app qa_bcast` passes in both shrink modes.

- **1** `qa_bcast` ends `=== ALL PASSED ===`: the filter API, the registry's rules (null filter,
  double registration, the cap, unknown unregister, a package target), an asynchronous send with
  the action, the extra, the registering Context and the main thread, a receiver unregistered
  mid-round skipped, a throwing receiver not starving the next, an explicit-class send, and the
  leak reap after `ActB` finishes with a receiver registered. Host: `intent_field_slots_match_fields_rs`,
  `api_hint_lookup`, `every_native_class_is_registered`, `no_original_name_literals`.
- **2** `qa_bcast` sets the clock to half a second before a minute edge and hears
  `TIME_CHANGED` then `TIME_TICK` within two seconds; sets the zone and hears
  `TIMEZONE_CHANGED` with the extra; shortens the screen timeout and hears `SCREEN_OFF`, then
  `SCREEN_ON`. Host tests for `minute_of` and the mask; `every_site_resolves_under_active_shrink_map`.
- **3** a `net` row on `testbench_rp2350w`; the `settings` lane's Wi-Fi patterns unchanged.
- **4** `alarms.rs` tests for the kind; `run_alarm_smoke` through a receiver; a `getBroadcast`
  alarm in `qa_bcast`.
- **5** the `claudeusage` and `picoclock` rows; picoclock on the bench.
- **6** `cargo test -p papk-pack -p papk-format`; a boot lane; a Gradle negative check.

## 7. What is deliberately not here

- **Ordered and sticky broadcasts.** No second app to order against, and a sticky value is a
  field the app keeps. The API hints say so for `sendOrderedBroadcast`, `sendStickyBroadcast`
  and `abortBroadcast`.
- **`goAsync`.** A receiver runs on the main thread between frames; slow work goes to a
  `picodroid.concurrent.Thread`, as the hint says.
- **Cross-app broadcasts.** One app runs at a time. `sendBroadcast` to a `setPackage` Intent is
  refused rather than silently dropped.
- **`WIFI_STATE_CHANGED_ACTION`.** There is no Wi-Fi enable/disable; `getWifiState` answers
  whether the board has a radio.
- **Manifest receivers beyond the boot-time ones** (stage 6). `TIME_TICK` and the screen actions
  are registered-only on Android as well.

## 8. Docs this amends

`alarm-manager-2026-09.md` ("Not done: `PendingIntent.getBroadcast`"),
`wifi-provisioning-2026-09.md` ("No broadcast"), `manifest-components-2026-10.md` ("Not read:
`<receiver>`"), `android-parity-roadmap-2026-08.md` (a new item outside the T-table). Each gets a
dated amendment as the stage that changes it lands.

## 9. Amendments
