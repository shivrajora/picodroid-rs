---
title: "Launcher and app switching"
description: "What a multi-app board boots, how the launcher starts an app, and how control comes back."
---

A multi-app board (`max_installed_apps` above 1 in its `board.toml`; every RP2350 board today) holds several installed apps and a **launcher** built into the firmware. One app runs at a time. This page explains what boots, how the launcher starts an app, and how control comes back.

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

## Costs

The launcher is about 10 KB of flash on every multi-app board; `bench/parity/ratchet.toml` records the exact figure. A single-app board embeds nothing and drops the launcher-facing classes (`PackageInfo`, `ApplicationInfo`, `BitmapDrawable`, `PackageManager.NameNotFoundException`) from its framework.
