---
title: "Launcher and app switching"
description: "What a multi-app board boots, how the launcher starts an app, and how control comes back."
---

A multi-app board (`max_installed_apps` above 1 in its `board.toml`; every RP2350 board today) holds several installed apps and a **launcher** built into the firmware. One app runs at a time. This page explains what boots, how the launcher starts an app, and how control comes back.

:::note[An app can also be brought back by an alarm]
Switching away from an app tears it down, but an alarm it set with
[`AlarmManager`](/api/services/#picodroidappalarmmanager) outlives it: when the alarm comes
due the framework starts that app again and delivers the Activity, the same switch in
reverse.
:::

## The launcher

The launcher lives in `system-apps/launcher/`. It is an ordinary Picodroid app (package `picodroid.launcher`), built like an example and linked into every multi-app firmware by the build. `pdb list` shows it as a `SYSTEM` row. It cannot be uninstalled and takes no space in the app region.

It shows one row per installed app: the icon (the manifest `icon`) or the first letter of the label, and the label (the manifest `label`, or the package name). Installed apps come first, then the other system apps. Tap a row, or move to it with the up and down buttons and press select, to start that app.

## What boots

The device picks the app to boot in this order:

1. `flash.sh --boot <what>`, built into the firmware: `app` (the app `--app` baked in), `launcher`, or a package name.
2. `boot_package = "<package>"` in `board.toml`, for a board that always runs one app.
3. The app `flash.sh --app` baked in (the boot default).
4. The launcher.
5. The installed app at the lowest sector.

A name that is not installed is skipped with a warning, and the next rule applies. So `flash.sh --app blinky` boots blinky, as it always did. `flash.sh --app blinky --boot launcher` boots the launcher, with blinky installed beside it.

In the simulator `PICODROID_BOOT` does the same, read when the simulator starts:

```bash
PICODROID_BOOT=launcher ./scripts/sim.sh --app blinky --system-apps
```

`--system-apps` builds the launcher and loads it into the simulated directory. Without it the simulator runs one app and exits when that app finishes, as before.

## Starting another app

An app starts another one with an Intent that names its package:

```java
PackageManager pm = getPackageManager();
Intent launch = pm.getLaunchIntentForPackage("com.example.weather");
if (launch != null) {
  startActivity(launch);
}
```

`getLaunchIntentForPackage` returns `null` when the package is not installed. Starting an Intent whose package is not installed throws `ActivityNotFoundException`.

There is no task stack. Starting another app ends the current one: every Activity gets `onPause`, `onStop` and `onDestroy`, services are destroyed, threads are stopped, and the heap is reset. Extras do not cross over. When the started app finishes (its last Activity calls `finish()`), the launcher comes back.

On a single-app board `startActivity` with a package target always throws `ActivityNotFoundException`.

## Coming back

- An app whose last Activity finishes returns to the launcher.
- On a board with buttons, BACK finishes the top Activity, as always. From the app's only Activity that returns to the launcher. The launcher itself ignores BACK.
- A touch-only board has no BACK button. An app started from the launcher must finish itself (a Close button, or `finish()` when its work is done). Otherwise it runs until the next install or reset.
- A device without a launcher (a single-app board, or a firmware built without one) waits for a `pdb install` after the app finishes, as it always did.
- If the launcher itself exits, the device starts it again once. A second exit in a row leaves the device waiting for a `pdb install`, so a broken launcher cannot loop.

## Listing what is installed

```java
PackageManager pm = getPackageManager();
List<PackageInfo> apps = pm.getInstalledPackages(0);
for (PackageInfo info : apps) {
  boolean system = (info.applicationInfo.flags & ApplicationInfo.FLAG_SYSTEM) != 0;
  CharSequence label = pm.getApplicationLabel(info.applicationInfo);
  Drawable icon = pm.getApplicationIcon(info.applicationInfo); // null when the app has none
}
```

See the [system API](/api/system/#picodroidcontentpmpackagemanager) for the whole query surface, and [`pdb list`](/reference/pdb-commands/#list) for the same directory from the host.

## The settings app

`system-apps/settings` (package `picodroid.settings`) sits beside the launcher in every multi-app firmware and shows in the launcher's list like any app. Every screen is a column of rows under a header row; a tap on the header (or BACK on a keypad board) goes up one level, and the root's header is Home, which finishes the app and brings the launcher back — the way a touch-only board gets home.

- **About** — the board, its MCU and the release (`Build`), the storage volume (`StatFs`) and the heap in use.
- **Apps** — one row per installed app (system apps are not listed; they cannot be uninstalled). A tap opens a dialog — the app's label, "Remove app and data?", Uninstall / Cancel — and Uninstall removes the app and its `/data/<package>` through `PackageManager.getPackageInstaller().uninstall(name)`, then the list is rebuilt. The settings app keeps running; nothing reboots.
- **Storage** — the volume, then each package's app bytes (its image) and data bytes (its directory, as the [storage cap](/api/storage/) counts it).

The same uninstall is available to any app: `getPackageManager().getPackageInstaller().uninstall("com.example.weather")` is synchronous and throws `IllegalArgumentException` for a package that is not installed, is a system app, or is the caller itself.

## Costs

A row of either app — a horizontal layout, an ellipsized label, a suffix, focusable — is about 20 ms of LVGL work on an RP2350, so neither app builds its rows inside `onCreate`: the header shows at once and the rows follow one per UI tick, each under the 50 ms the slow-handler watchdog allows, and a screen of seven packages is complete within about half a second with the display and the touch panel served throughout. The Storage screen fetches its numbers on `Executors.backgroundExecutor()` and fills the rows in as they arrive; that job runs 4 KB deep, which is why every multi-app board gives its pool workers 6 KB stacks. The package directory keeps each entry's manifest values (name, label, version, icon) as slices into the image, and the quota keeps a usage figure per package until its directory changes, so the queries behind the screens are a few native calls each.

The launcher is about 10 KB and the settings app about 18 KB of flash on every multi-app board; `bench/parity/ratchet.toml` records the exact figures. A single-app board embeds neither and drops the multi-app classes (`PackageInfo`, `ApplicationInfo`, `BitmapDrawable`, `PackageManager.NameNotFoundException`, `PackageInstaller`, `StorageStatsManager`, `StorageStats`) from its framework.
