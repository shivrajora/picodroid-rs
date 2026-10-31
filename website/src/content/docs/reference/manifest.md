---
title: "PicodroidManifest.xml reference"
description: "Schema, entry points, and build/runtime wiring for an app's PicodroidManifest.xml."
---

Every Picodroid app ships exactly one `PicodroidManifest.xml` at the root of its
directory. It is written as Android writes `AndroidManifest.xml`: the package,
the Application class, the Activities, Services and receivers the app has, and
the Activity the launcher opens.

A minimal manifest, for an app with an `Application` and no screen:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
          package="helloworld" android:versionName="1.0">
    <application android:name=".HelloWorld" />
</manifest>
```

An app with screens declares them, and the one with the `MAIN` / `LAUNCHER`
intent filter is what the framework launches:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
          package="myapp" android:versionName="1.2" android:versionCode="3">
    <uses-permission android:name="android.permission.INTERNET" />
    <application android:name=".MyApp" android:label="@string/app_name"
                 android:theme="@style/AppTheme">
        <activity android:name=".ui.MainActivity">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
        <service android:name=".data.SyncService" />
    </application>
</manifest>
```

The build discovers the app, compiles its Java, and packs everything into a
`.papk` bundle (see [Build](/get-started/build/)). At runtime the firmware reads
the entry point out of the packed manifest and dispatches to it.

## Schema

The manifest is validated at build time by the Gradle `buildSrc` parser
(`ManifestSchema.kt`). It fails the Gradle build immediately, at configuration
time, with a clear error rather than letting a malformed manifest reach the
packer.

**Spelling.** Android's attributes carry the `android:` prefix and only that
spelling is read: `android:name`, `android:label`, `android:theme`,
`android:versionName`, `android:versionCode`, `android:required`. The root must
declare `xmlns:android="http://schemas.android.com/apk/res/android"`, as an
Android manifest does; the parser is not namespace-aware, so the prefix must be
`android`. Picodroid's own attributes, which Android has no counterpart for
(`icon`, `design-width`, `design-height`), are bare. A bare `name=`, `label=` or
`theme=`, and the pre-October-2026 short form (`<application application=…>`,
`activity=`, `main-class=`, `version=`, `version-code=`), fail the build with the
spelling to write instead.

The `<manifest>` root:

- **`xmlns:android` (required).** Android's namespace, as above.
- **`package` (required, non-blank).** The app's identity: it becomes the
  `package-name` key of the PAPK, which the device's package directory keys
  on (one install per package; reinstalling a package replaces its previous
  copy). Reverse-DNS (`com.acme.weather`) and plain (`helloworld`) names are
  both accepted; the example apps use the plain form. A component name with a
  leading dot is relative to it.
- **`android:versionName` (optional, default `"1.0"`).** The human-readable
  version name, taken verbatim.
- **`android:versionCode` (optional, default `1`).** A positive integer that
  must grow with every release of the package: the package directory orders
  installs by it. Anything else fails the build.
- **`<application>` (exactly one).** See below.
- **`<uses-permission android:name="…"/>` (optional, repeatable).** Read and
  ignored. Nothing on this platform enforces a permission (WiFi, the network and
  the LEDs are open to every app); the element is accepted so an Android
  manifest's lines carry over, and it grants nothing. It belongs under
  `<manifest>`, as on Android; under `<application>` it is a build error.
- **`<supports-screens design-width=".." design-height=".."/>` (optional).**
  The logical size the app lays out against; both attributes are positive
  integers and both are required when the element is present. See
  [Screens and features](#screens-and-features).
- **`<uses-feature android:name=".." android:required="true|false"/>`
  (optional, repeatable).** A hardware feature the app uses, by its
  `PackageManager.FEATURE_*` name. Unlike Android, `android:required` defaults
  to `false`. The installer refuses an app that requires a feature the board
  lacks.

Any other child of `<manifest>` fails the build naming the supported ones.

The `<application>` element:

- **`android:name`**: the `Application` subclass, if the app has one. Names
  are read as Android reads them: dotted (`myapp.MyApp`), slash-form, or
  relative to the manifest `package` with a leading dot (`.MyApp`). A
  Kotlin or Java class outside the manifest package is written in full.
- **`android:label`**: the display name a launcher shows; defaults to the
  package name. Either text or `@string/name`, which the packer resolves
  against `res/values` (the default configuration) and writes as text; a
  missing string, or a label reference with no `res/` tree, fails the pack.
- **`android:theme="@style/Name"`**: see [Components and the
  theme](#components-and-the-theme).
- **`icon`**: the file name of a PNG under the app's `assets/` directory (see
  [Bundled image assets](/guides/assets/#app-icon)). It must exist there, or the
  build fails at configuration time, and it should be a small square: 48×48 is
  the convention; `papk-pack` warns past 64×64 or when the sides differ. This is
  Picodroid's attribute: Android's `android:icon` names a drawable resource.
- **Children:** `<activity>`, `<service>` and `<receiver>`; anything else fails
  the build naming what is supported.

The exact error strings, so you can grep this page if you hit one:

- `PicodroidManifest.xml not found: <absolute path>`
- `<file>: expected <manifest> root, got <other-tag>`
- `<file>: <manifest> must declare xmlns:android="http://schemas.android.com/apk/res/android", as an Android manifest does`
- `<file>: <manifest> missing 'package' attribute`
- `<file>: <manifest version="…">: write android:versionName, Android's spelling` (and `version-code` / `android:versionCode`)
- `<file>: <manifest> android:versionCode must be a positive integer, got '<value>'`
- `<file>: <tag> name="…">: write android:name, Android's spelling` (and `label`, `theme`, `required`)
- `<file>: <manifest> does not take <<tag>> (supported: <application>, <uses-permission>, <uses-feature>, <supports-screens>)`
- `<file>: missing <application> element` and `<file>: more than one <application> element`
- `<file>: <application application="<class>"> is the old short form: write <application android:name="…">`
- `<file>: <application activity="<class>"> is the old short form: declare the Activity under <application> as <activity android:name="…"> with the MAIN / LAUNCHER <intent-filter>`
- `<file>: <application main-class="<class>"> is not supported: an app's entry is an Application (android:name) or an <activity>`
- `<file>: the app has no entry: name the Application class (<application android:name=".App">), declare an <activity>, or both`
- `<file>: <uses-permission> belongs under <manifest>, not <application>, as on Android`
- `<app>: PicodroidManifest.xml icon="<name>" is not a file under assets/`
- `<file>: <supports-screens> design-width must be a positive integer, got '<value>'` (and
  `design-height`)
- `<file>: <uses-feature> missing 'android:name' attribute`
- `<file>: <uses-feature> android:required must be 'true' or 'false', got '<value>'`
- `<file>: <activity> missing 'android:name' attribute` (and `<service>`, `<receiver>`, `<uses-permission>`)
- `<file>: <activity> '<class>' is declared twice` (and `<service>`, `<receiver>`)
- `<file>: <activity> android:theme is not supported — there is one theme per app; put android:theme on <application>`
- `<file>: <activity> label '@string/…' is a reference; write the text itself (labels are not resolved from res/)` (and `<application>`)
- `<file>: two activities carry the MAIN intent filter ('<a>', '<b>'); one app has one entry`
- `<file>: <application> does not take <<tag>> (supported: <activity>, <service>, <receiver>)`
- `<file>: <application> android:theme must name a <style>, got '<value>'`
- `PicodroidManifest.xml declares components, and the code starts some it does not declare (Android refuses these at run time; declare them under <application>):` followed by the `<service android:name=…/>` / `<activity …/>` lines to add (the `verifyManifest` task)
- `manifest <service android:name="<class>"> is not among the packed classes` and `… does not extend picodroid.app.Service` (and `<activity>` / `Activity`; `papk-pack`)
- `the manifest names android:theme="@style/<Name>", and res/values defines no <style name="<Name>">` (and `… the app has no res/ tree to define it in`)
- `the manifest names android:label="@string/<name>", and res/values defines no <string name="<name>">` (and `… the app has no res/ tree to define it in`)

### DOCTYPE is rejected

The parser disables DTD processing entirely (`disallow-doctype-decl`). A
manifest containing a `<!DOCTYPE …>` declaration fails to parse — the XML
parser's own exception propagates, there is no custom Picodroid message. This is
XXE / billion-laughs hardening: no external entities, no DTD. Just don't put a
DOCTYPE in your manifest.

### Editor validation (XSD)

`sdk/PicodroidManifest.xsd` mirrors the `ManifestSchema.kt` rules above for
editor validation and autocomplete; it imports
`sdk/PicodroidManifest-android.xsd`, which declares the `android:` attributes in
Android's namespace. It is **not** wired into the build: the Gradle parser
remains the authoritative check.

Referencing it is opt-in per file via `xsi:noNamespaceSchemaLocation`. The
`helloworld` example and the `newApp` template do so:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
          xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
          xsi:noNamespaceSchemaLocation="../../sdk/PicodroidManifest.xsd"
          package="myapp" android:versionName="1.0">
    <application android:name=".Myapp" />
</manifest>
```

The "an Application or an Activity" rule uses an XSD 1.1 assertion, so its live
check needs a 1.1-aware validator (IntelliJ / Android Studio); other editors
still get structure and attribute autocomplete.

## Entry points

An app has an `Application` class (`<application android:name>`), an entry
Activity, or both; with neither the build fails. The entry Activity is the
`<activity>` whose `<intent-filter>` carries the `MAIN` action, else the first
declared.

- **Both:** the framework runs `Application.onCreate()` and then launches the
  entry Activity, as Android launches the launcher Activity once the
  Application is created. The Application does not call `startActivity` for
  it. It still may: an Activity that `onCreate` starts itself (with extras, or
  a different one) is launched instead, and one that hands the device to another
  package launches nothing.
- **An Activity only:** the runtime instantiates the entry Activity and drives
  its lifecycle directly.
- **An Application only:** `onCreate()` runs, the Services it started run their
  course, and the app ends: a headless app, as an Android app with no launcher
  Activity. `helloworld`, `servicedemo` and the console-style test apps are of
  this kind.

The entry Activity's `getIntent()` returns `null` (Android hands it an
`ACTION_MAIN` Intent); an Activity started with an Intent has it.

```xml
<application android:name=".MyApp">
    <activity android:name=".MainActivity">
        <intent-filter>
            <action android:name="android.intent.action.MAIN" />
            <category android:name="android.intent.category.LAUNCHER" />
        </intent-filter>
    </activity>
</application>
```

```java
package myapp;

import picodroid.app.Application;

public class MyApp extends Application {
  @Override
  public void onCreate() {
    // App-wide setup; the framework launches MainActivity afterwards.
  }
}
```

See [Multi-screen apps](/tutorials/multi-screen-app/) and
[Background services](/tutorials/background-service/).

## Components and the theme

What this platform reads of an Android manifest's components
([manifest-components-2026-10.md](https://github.com/shivrajora/picodroid-rs/blob/main/docs/designs/manifest-components-2026-10.md)):

- **`<activity android:name="…">`**, repeatable. One may carry the `MAIN`
  intent filter (two is an error). `android:label="…"` is the Activity's title
  (`getTitle`, the [app bar](/api/ui/#app-bar-toolbar-and-setactionbar)), text or
  `@string/name` like the application's; without one it takes the application's
  label.
- **`<service android:name="…">`**, repeatable. Declared components are
  checked twice at build time: `papk-pack` checks that each class is packed and
  extends `picodroid.app.Activity` / `Service`, and the Gradle `verifyManifest`
  task checks that every `new Intent(context, X.class)` the code starts names a declared
  component (every Service, and every Activity once activities are declared).
  Android refuses an undeclared component at run time; here the build does, with
  the element to add. `Intent.setClassName` is dynamic and is not checked.
- **`android:theme="@style/Name"`** on `<application>`: the `<style>` in
  `res/values` that `?attr/…` resolves against at build time and that the
  framework applies before the first Activity's `onCreate`, as Android does, so
  the app does not call `setTheme`. Without it, the style called `AppTheme`
  serves `?attr/` and nothing is applied ([styles and the
  theme](/guides/resources/#styles-and-the-theme)). One theme per app: an
  `android:theme` on an `<activity>` is a build error.
- **`<receiver>`** declares a `BroadcastReceiver` the framework starts: for
  the actions its `<intent-filter>` names — `BOOT_COMPLETED`, `TIME_SET` and
  `TIMEZONE_CHANGED` are the ones a manifest receiver may declare; the other
  system actions reach registered receivers only, and the build says so — and
  for an explicit broadcast Intent or `PendingIntent.getBroadcast` that names
  the class. One app runs at a time, so a delivery to an app that is not
  running is a headless run of it: `Application.onCreate`, then `onReceive`
  with a plain Context, no Activity (the entry Activity is not launched, and
  one the app starts is dropped with a log line); it happens at boot before the
  boot app, when an app hands over to the launcher, or by interrupting the
  launcher. The class must extend `picodroid.content.BroadcastReceiver`.
- Not read: `<meta-data>`, `<provider>`, anything in an `<intent-filter>` but
  the `MAIN` action and a receiver's actions (a `<category>` is accepted and
  ignored).

## App identity

A launcher lists installed apps by three values, all optional:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
          package="com.acme.weather" android:versionName="1.4" android:versionCode="5">
    <application android:label="@string/app_name" icon="icon.png">
        <activity android:name="weather.MainActivity">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
```

They reach the PAPK as the `version-code`, `label` (as text, a `@string/`
resolved) and `icon` manifest keys (`papk-info` prints them). A PAPK whose
manifest lacks them still installs: a missing code reads as 1, a missing label
as the package name, a missing icon as none.

## Screens and features

Two more elements, both optional, from
[app portability](https://github.com/shivrajora/picodroid-rs/blob/main/docs/designs/app-portability-2026-10.md):

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="dragdemo">
    <application>
        <activity android:name=".DragDemoActivity">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
    <supports-screens design-width="320" design-height="480" />
    <uses-feature android:name="picodroid.hardware.touchscreen" android:required="true" />
</manifest>
```

**`<supports-screens>`** names the logical size the app was laid out against. The
runtime then shows the app in a window of exactly that size on every board —
`Display.getWidth()`/`getHeight()`, every `dp`, every coordinate the app uses and
every overlay the framework draws are the design's. On a larger panel the window
is centred and the rest of the panel stays dark; on a smaller one the window
starts at the panel's origin and the screen pans to what does not fit (a drag on
a touch board, the focus on a four-key one). The simulator logs the placement at
launch: `window 320x240 @ (0,120) on the 320x480 panel`, or
`window: the 320x240 panel` when nothing was declared. Declare a design size for
an app drawn in pixels against one panel; leave it out — the default — for an
app whose layout stretches with `match_parent` and `layout_weight`, which runs
at the panel's own size (see
[Writing a layout once for every board](/guides/resources/#layouts)).

**`<uses-feature>`** declares a hardware feature by the name
`PackageManager.hasSystemFeature` answers to: `picodroid.hardware.touchscreen`,
`picodroid.hardware.wifi`, `picodroid.hardware.ethernet`. With
`android:required="true"` the installer — `pdb install`, the launcher's installer and
the simulator's `apps install` — refuses the app on a board that lacks the
feature, before anything is erased:
`device rejected install: ERR — requires a feature this board lacks (uses-feature)`.
The default is `android:required="false"` (Android's is `true`), which is informational: the stock widgets
work with a finger and with four keys alike, so only an app that reads raw
touch (a drawing pad, a drag demo) has to insist. A board whose firmware was
flashed with such an app directly, bypassing the installer, runs it and logs
`app requires <feature>, which this board lacks` once.

They reach the PAPK as the `design-width`, `design-height` and
`requires-features` (comma-separated, required ones only) manifest keys;
`papk-pack` takes them as `--design-size WxH` and `--requires-feature <name>`.

## How it is wired

Each app's `build.gradle.kts` applies a single plugin:

```kotlin
plugins {
    id("picodroid-papk")
}
```

The `picodroid-papk` plugin compiles the Java, optionally shrinks framework
references, and packs a `build/papk/<name>.papk` bundle (see
[What the PAPK carries](#what-the-papk-carries)). It also:

- Adds the `:sdk` project as an `implementation` dependency, so every app
  compiles against the Picodroid SDK.
- Roots the Java source set at the project directory and includes `**/*.java`.
  That means **both layouts work**: sources nested under `java/<pkg>/` (what
  `newApp` scaffolds) and `.java` files dropped flat in the project directory.

### The PAPK package name is `package=`

The `package-name` key the PAPK carries is the manifest's `package=` attribute,
and it is the identity everything on the device keys on: the package directory
holds one install per package, a reinstall replaces the previous copy, and
per-app storage lives under it. (Until September 2026 the key was the Gradle
project name instead; nothing consumed it then.)

The Gradle project name is a separate thing: the `examples/<name>/`
**directory name**, which picks the Gradle task names and the output file
`build/papk/<name>.papk`. Apps are auto-discovered by `settings.gradle.kts`:
any `examples/<name>/` directory containing a file literally named
`PicodroidManifest.xml` becomes a subproject. No edit to `settings.gradle.kts`
is needed to add an app.

```kotlin
// Auto-discover every examples/<name>/ that ships a PicodroidManifest.xml.
rootDir.resolve("examples").listFiles()
    ?.filter { it.isDirectory && it.resolve("PicodroidManifest.xml").isFile }
    ?.sortedBy { it.name }
    ?.forEach { include(":examples:${it.name}") }
```

By convention the classes' Java package, the `package=` value, and the
directory name all match (e.g. `helloworld`). Nothing enforces
that match — a reverse-DNS `package=` over a short directory name is fine —
but keeping them identical avoids confusion.

Source for the plugin and discovery:
[buildSrc/](https://github.com/shivrajora/picodroid-rs/tree/main/buildSrc) and
[settings.gradle.kts](https://github.com/shivrajora/picodroid-rs/blob/main/settings.gradle.kts).

### What the PAPK carries

A `.papk` is a flat little-endian container: a 28-byte header, then a MANIFEST
section, a CLASSES section, and an ASSETS and a RESOURCES section when the app
has [images](/guides/assets/) or [resources](/guides/resources/). Every section
starts on a 4-byte boundary, because the device reads all of them in place from
flash.

- **MANIFEST** holds the values of this page as key/value strings: the entry
  point (`application`, `activity`, or `application` then `activity` for an
  app with both; `main-class` only from `papk-pack --main-class`), `package-name`, `version`,
  `framework-map-version`, and `version-code`, `label`, `icon`, `design-width`,
  `design-height` and `requires-features` when set; an Android-form manifest
  adds `activities` and `services` (comma-joined, slash form), `theme` (the
  theme's `R.style` id, decimal) and one `label:<class>` entry per activity
  `android:label`.
- **CLASSES** holds every `.class` file with a *link table* `papk-pack` built
  for it (the record a class loader would otherwise parse at run time, a hash of
  each superclass and interface name, a signature hash per method and a 4-byte
  descriptor per method reference) and a class index sorted by name hash. The
  runtime parses nothing when the app starts and finds classes and methods by
  hash. The framework's own classes are embedded in the firmware in the same
  form.

This is format version 3; `pdb install`, `papk-info` and the firmware build read
no other, and a board does not load a file of another version left in its app
region. Packing also refuses a class file over 65,535 bytes and two classes
whose names hash alike.

`./scripts/papk-info.sh build/apks/<name>.papk` prints the header, the manifest
keys, the classes with their sizes, the assets and the resource counts, and
fails on a file that any of the above would refuse.

## Scaffolding a new app

The fastest way to start is the `newApp` Gradle task:

```bash
./gradlew newApp -Pname=myapp
```

The `-Pname` value must match `^[a-z][a-z0-9_]*$` — lowercase first letter, then
lowercase letters, digits, or underscores. A bad name fails with:

```text
app name must match [a-z][a-z0-9_]* — got '<name>'
```

Omitting `-Pname` fails with `missing -Pname=<appname>`, and an existing target
fails with `already exists: <dir>`.

It generates three files under `examples/myapp/`:

- `java/myapp/Myapp.java` — an `Application` subclass with an `onCreate()` stub.
- `PicodroidManifest.xml` — `package="myapp"`, `android:versionName="1.0"`,
  and `<application android:name=".Myapp">`.
- `build.gradle.kts` — applies `id("picodroid-papk")`.

The class name is the app name with its first letter uppercased. From there,
follow [Your first app](/get-started/first-app/).

## Shrinking

Under `--shrink` the [shrinker](/reference/shrinker/) rewrites only framework
references — the `picodroid.*` and `java.*` classes and the member names the
release map covers. The release map is cut from the framework source tree
alone, so your app's own class names pass through unchanged, and the
`application` / `activity` values the packer writes (e.g. `myapp/MyApp`) are
identical in the shrunk and unshrunk PAPK. Only the references baked into your
`.class` bytes (the `picodroid/app/Application` superclass, an `onCreate`
override) are rewritten, and the firmware's copies are rewritten the same way.

Under `--shrink --shrink-app` your own classes are renamed too (`myapp/MyApp`
→ `c/A`) and `papk-pack` spells the manifest entry, the declared `activities`
and `services`, and a layout's custom view class names through the per-app map,
so the manifest you write keeps saying `.MyApp` while the packed manifest
names the class that is actually in the PAPK. The entry points Rust invokes by
name — `main` and the generated `injectMembers` — are kept by `sdk/keep.toml`.
You never need a keep entry for your own classes in either mode. See
[App shrinking](/reference/shrinker/#app-shrinking---shrink-app).

## Runtime dispatch and errors

At load time the firmware first runs a compatibility check (below), loads the
framework and app classes, then dispatches on the manifest entry point:

1. `application`: `Application.onCreate()`, then the `activity` value when the
   PAPK carries one and `onCreate` queued no Activity itself (see
   [Entry points](#entry-points)).
2. `activity` alone: that Activity.
3. `main-class`: its `static void main` (a PAPK packed by hand with
   `papk-pack --main-class`; a manifest cannot name one).

If a named class is missing, the JVM's name-based lookup fails. The firmware
**logs the error and returns cleanly** rather than panicking — a bad APK should
not brick the boot path. The log line depends on the entry point:

- `application` → `failed to instantiate Application <class>` or
  `Application.onCreate error: <err>`
- `activity` → `failed to instantiate initial Activity <class>` (after an
  Application) or `failed to instantiate boot Activity <class>`, and lifecycle
  invocation errors as `Activity lifecycle error: <err>`
- `main-class` → on hardware `JVM error: <err>`; in the simulator
  `[jvm] error: <err>`

`papk-pack` refuses a manifest whose classes are not packed, so these mean a
PAPK built by hand or a class the build could not see. See
[Debugging](/guides/debugging/) and [Troubleshooting](/guides/troubleshooting/).

### Install compatibility check

Every PAPK embeds a `framework-map-version` key, written by the packer. The
firmware only loads a file that is format version 3 and whose link tables match
their class bytes (checked when the app region is scanned at boot); at load time
it compares the key against the version it was built with (`verify_compat`). The two sides share one rule: both unshrunk (`0.0.0`) is OK,
both shrunk with a compatible map is OK, and an asymmetric `--shrink` setting (or
a PAPK built newer than the firmware) is rejected.

The same check runs host-side during `pdb install`, so an incompatible PAPK is
refused before flashing:

```text
Refusing to install: PAPK is incompatible with running firmware.
  ...
  Rebuild the PAPK with matching --shrink setting (see reference/shrinker in the docs).
```

The fix is almost always to rebuild the PAPK with the same `--shrink` setting as
the firmware. See [Hot-swap installs](/get-started/hot-swap/),
[the shrinker reference](/reference/shrinker/), and
[Troubleshooting](/guides/troubleshooting/).
