---
title: "PicodroidManifest.xml reference"
description: "Schema, entry-point styles, and build/runtime wiring for an app's PicodroidManifest.xml."
---

Every Picodroid app ships exactly one `PicodroidManifest.xml` at the root of its
directory. It declares the app's package and names the single class the runtime
hands control to at launch.

A minimal manifest:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest package="helloworld" version="1.0">
    <application application="helloworld/HelloWorld" />
</manifest>
```

The build discovers the app, compiles its Java, and packs everything into a
`.papk` bundle (see [Build](/get-started/build/)). At runtime the firmware reads
the entry point out of the manifest and dispatches to it.

## Schema

The manifest shape is validated at build time by the Gradle `buildSrc` parser
(`ManifestSchema.kt`). It fails the Gradle build immediately — at configuration
time — with a clear error rather than letting a malformed manifest reach the
packer.

The `<manifest>` root:

- **`package` (required, non-blank).** Names the app's package. Missing or
  blank both fail. (Note: this value is validated but does not become the PAPK
  package name — see [How it is wired](#how-it-is-wired).)
- **`version` (optional, default `"1.0"`).** Missing or blank falls back to
  `"1.0"`. Any non-blank string is accepted verbatim; there is no format check.
- **`<application>` (required).** At least one must exist. Only the first
  `<application>` element is read; any extras are silently ignored.

The exact error strings, so you can grep this page if you hit one:

- `PicodroidManifest.xml not found: <absolute path>`
- `<file>: expected <manifest> root, got <other-tag>`
- `<file>: <manifest> missing 'package' attribute`
- `<file>: missing <application> element`
- `<file>: <application> must set exactly one of 'main-class', 'activity', or 'application'`
- `<file>: <application> sets multiple of 'main-class'/'activity'/'application' — pick one`

### DOCTYPE is rejected

The parser disables DTD processing entirely (`disallow-doctype-decl`). A
manifest containing a `<!DOCTYPE …>` declaration fails to parse — the XML
parser's own exception propagates, there is no custom Picodroid message. This is
XXE / billion-laughs hardening: no external entities, no DTD. Just don't put a
DOCTYPE in your manifest.

### Editor validation (XSD)

`schema/PicodroidManifest.xsd` mirrors the `ManifestSchema.kt` rules above —
required `package`, optional `version`, and the exactly-one-of
`main-class` / `activity` / `application` entry point — for editor validation and
autocomplete. It is **not** wired into the build; the Gradle parser remains the
authoritative check (the one-of rule is already enforced there), so the schema
can't drift the build.

Referencing it is opt-in per file via `xsi:noNamespaceSchemaLocation`. The
`helloworld` example and the `newApp` template do so:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest package="myapp" version="1.0"
          xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
          xsi:noNamespaceSchemaLocation="../../schema/PicodroidManifest.xsd">
    <application application="myapp/MyApp" />
</manifest>
```

Manifests without the reference stay valid — the build never reads it. The
one-of constraint uses an XSD 1.1 assertion, so the live red-squiggle for "two
entry points set" needs a 1.1-aware validator (IntelliJ / Android Studio); other
editors still get structure and attribute autocomplete.

## Entry-point styles

The `<application>` element must set **exactly one** of three attributes, and
its value must be non-blank. Setting none, or more than one, is a build error
(see the strings above).

Every value is a **JVM internal name** in slash form: `pkg/Class`, never the
dotted `pkg.Class`. The leading segment is the package, separated from the class
name by `/`. (Nested packages use more slashes: `com/example/foo/MyApp`.) The
dotted form would not match the compiled class's internal name and would fail at
runtime.

### `application=` — full app with lifecycle

Use this for anything beyond a single screen: an app that starts Activities,
binds services, or needs the `onCreate()` setup hook. This is the form every
example app uses, and what `newApp` scaffolds.

```xml
<application application="myapp/MyApp" />
```

```java
package myapp;

import picodroid.app.Application;
import picodroid.content.Intent;

public class MyApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(this, MainActivity.class));
  }
}
```

`Application.onCreate()` runs once at launch. From there you drive the UI by
launching Activities with `startActivity(...)`. See
[Multi-screen apps](/tutorials/multi-screen-app/) and
[Background services](/tutorials/background-service/).

### `activity=` — single-screen UI

Use this when the whole app is one screen and you don't need an `Application`
subclass. The runtime instantiates the named `Activity` and drives its lifecycle
directly.

```xml
<application activity="myapp/MainActivity" />
```

```java
package myapp;

import picodroid.app.Activity;
import picodroid.widget.TextView;

public class MainActivity extends Activity {
  @Override
  public void onCreate() {
    TextView root = new TextView();
    root.setText("Hello");
    setContentView(root);
  }
}
```

See the [UI API](/api/ui/) for building view trees.

### `main-class=` — console / static main

Use this for a non-UI program — a console or benchmark-style app whose logic
lives in a `static void main`. The runtime invokes the static `main` method and
nothing else.

```xml
<application main-class="myapp/Main" />
```

```java
package myapp;

import picodroid.util.Log;

public class Main {
  public static void main(String[] args) {
    Log.i("Main", "Hello from main!");
  }
}
```

## How it is wired

Each app's `build.gradle.kts` applies a single plugin:

```kotlin
plugins {
    id("picodroid-papk")
}
```

The `picodroid-papk` plugin compiles the Java, optionally shrinks framework
references, and packs a `build/papk/<name>.papk` bundle. It also:

- Adds the `:sdk` project as an `implementation` dependency, so every app
  compiles against the Picodroid SDK.
- Roots the Java source set at the project directory and includes `**/*.java`.
  That means **both layouts work**: sources nested under `java/<pkg>/` (what
  `newApp` scaffolds) and `.java` files dropped flat in the project directory.

### The PAPK package name comes from the directory, not `package=`

This is the one part of the manifest that does not behave the way intuition
suggests. The PAPK's package name is taken from the **Gradle project name**,
which for an auto-discovered app is the `examples/<name>/` **directory name** —
**not** the manifest's `package=` attribute. The `package=` attribute is
required and validated, but it is then discarded and never propagated into the
PAPK.

Apps are auto-discovered by `settings.gradle.kts`: any `examples/<name>/`
directory containing a file literally named `PicodroidManifest.xml` becomes a
subproject. No edit to `settings.gradle.kts` is needed to add an app.

```kotlin
// Auto-discover every examples/<name>/ that ships a PicodroidManifest.xml.
rootDir.resolve("examples").listFiles()
    ?.filter { it.isDirectory && it.resolve("PicodroidManifest.xml").isFile }
    ?.sortedBy { it.name }
    ?.forEach { include(":examples:${it.name}") }
```

By convention the slash-form entry point's leading segment, the `package=`
value, and the directory name all match (e.g. `helloworld`). Nothing enforces
that match — but keeping them identical avoids confusion.

Source for the plugin and discovery:
[buildSrc/](https://github.com/shivrajora/picodroid-rs/tree/main/buildSrc) and
[settings.gradle.kts](https://github.com/shivrajora/picodroid-rs/blob/main/settings.gradle.kts).

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
- `PicodroidManifest.xml` — `package="myapp"`, `version="1.0"`, and
  `application="myapp/Myapp"`.
- `build.gradle.kts` — applies `id("picodroid-papk")`.

The class name is the app name with its first letter uppercased. From there,
follow [Your first app](/get-started/first-app/).

## Shrinking

The class-name [shrinker](/reference/shrinker/) **never rewrites an app's own
class names** — it only rewrites framework references (`picodroid.*` classes from
`sdk/java`). The active shrink map is built from the framework source tree alone;
your app's classes are never fed into it, so they pass through unchanged.

This means your `application=` / `activity=` / `main-class=` value (e.g.
`myapp/MyApp`) is **identical in the shrunk and unshrunk PAPK** — it stays valid
under `--shrink`. Only the *superclass reference* baked into your `.class` bytes
(e.g. `picodroid/app/Application`) gets rewritten when shrinking is on, and that
is transparent because the firmware's matching framework classes are rewritten
the same way. You never need a keep entry for your own classes.

## Runtime dispatch and errors

At load time the firmware first runs a compatibility check (below), loads the
framework and app classes, then dispatches on the manifest entry point. Unlike
the build-time "exactly one of" rule, the runtime uses a fixed **precedence** —
it does not re-check that only one key is set:

1. `application` (highest precedence)
2. `activity`
3. `main-class`

A normally-built PAPK only ever carries one of these, so precedence is moot. A
hand-built PAPK with several keys would simply use the highest one.

If the named class is missing or its name is mistyped (for example, a dotted
`myapp.MyApp` that can't match the slash-form internal name), the JVM's
name-based lookup fails with `MethodNotFound`. The firmware **logs the error and
returns cleanly** rather than panicking — a bad APK should not brick the boot
path. The log prefix depends on the entry-point type:

- `application` → `Application.onCreate error: <err>`
- `activity` → `failed to instantiate initial Activity <class>`, and lifecycle
  invocation errors as `Activity lifecycle error: <err>`
- `main-class` → on hardware `JVM error: <err>`; in the simulator
  `[jvm] error: <err>`

If you see one of these in the [simulator](/get-started/simulator/) or device
log, the usual cause is a typo'd entry-point string or a dotted name. See
[Debugging](/guides/debugging/) and [Troubleshooting](/guides/troubleshooting/).

### Install compatibility check

Every PAPK embeds a `framework-map-version` key, written by the packer. At load
time the firmware compares it against the version it was built with
(`verify_compat`). The two sides share one rule: both unshrunk (`0.0.0`) is OK,
both shrunk with a compatible map is OK, and an asymmetric `--shrink` setting (or
a PAPK built newer than the firmware) is rejected.

The same check runs host-side during `pdb install`, so an incompatible PAPK is
refused before flashing:

```text
Refusing to install: PAPK is incompatible with running firmware.
  ...
  Rebuild the PAPK with matching --shrink setting (see docs/shrinker.md).
```

The fix is almost always to rebuild the PAPK with the same `--shrink` setting as
the firmware. See [Hot-swap installs](/get-started/hot-swap/),
[the shrinker reference](/reference/shrinker/), and
[Troubleshooting](/guides/troubleshooting/).
