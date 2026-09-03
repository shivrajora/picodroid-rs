---
title: "Shrinker"
description: "The build-time name shrinker: framework class and member names under --shrink, the app's own names under --shrink-app, and how to read a shrunk log."
---

Picodroid ships a build-time name shrinker (`tools/class-shrink/` plus a
Gradle ASM pass) that rewrites `.class` files to use short synthetic
names — `picodroid/pio/Gpio` → `a/S`, `java/lang/String` → `b/AQ`,
`setText` → `uQ` — the way ProGuard and R8 do. It trims kilobytes from
firmware flash and from every `.papk` without any change to Java source:
the Rust runtime is compiled against the same spellings, so nothing is
translated back at run time. `--shrink-app` extends the rename to an
app's own classes and members, and `scripts/retrace.sh` turns a shrunk
log back into original names.

**Shrinking is off by default**, matching Android's "R8 off by default"
behavior. Opt in on any build by passing `--shrink` to the top-level
script (`build.sh`, `flash.sh`, `sim.sh`, or `build-apk.sh`), which
sets `PICODROID_SHRINK=1`. Both firmware and PAPK builds honor the
same env var so the two sides always agree.

This doc is reference material. Day-to-day app development doesn't
need any of it.

## Design overview

Shrink maps are **release-versioned** and **append-only**:

- Each picodroid release can commit an immutable map file at
  `sdk/shrink-maps/v<semver>.toml`.
- The map keyed to the picodroid package version in the root `Cargo.toml`
  is the **active map**. "Keyed" = highest committed `v<semver>.toml`
  whose semver is ≤ the package version.
- If no map is committed at or below the current version, the active
  version is the sentinel `0.0.0` and no shrinking happens.
- Classes added to the framework between releases stay un-shrunk
  (retain their original full names) until the next release cut
  folds them in. Symbols in the active map are never renamed.

The append-only rule is what lets old PAPKs keep working on newer
firmware: every name a PAPK-at-version-P refers to is still present in
firmware-at-version-F ≥ P.

PAPK compatibility is enforced at load time
([papk-format/src/lib.rs](https://github.com/shivrajora/picodroid-rs/blob/main/papk-format/src/lib.rs) `verify_compat`): a PAPK with a map
version greater than the firmware's is rejected with
`PapkError::FrameworkVersionMismatch`.

## Active maps

Eighteen release maps are committed today, `v0.1.0` through `v0.18.0`:

| Map | Covers |
|-----|--------|
| `sdk/shrink-maps/v0.1.0.toml` | Original 42 framework classes from the first release cut. |
| `sdk/shrink-maps/v0.2.0.toml` | Adds classes introduced after v0.1.0 — `Executors` / `Executor` / `MainExecutor` / `BackgroundExecutor`, the `SensorManager` family (`Sensor`, `SensorEvent`, `SensorEventListener`, `SensorManager`), the HTTP client (`Url`, `HttpUrlConnection`, `HttpInputStream`, `HttpOutputStream`), and `KeyEvent` / `OnKeyListener`. Every v0.1.0 mapping is copied verbatim. |
| `sdk/shrink-maps/v0.3.0.toml` | Adds classes introduced after v0.2.0 — `picodroid.graphics.Theme`, the drawable family (`Drawable`, `GradientDrawable`, `GradientDrawable$Orientation`), gesture / animation surface (`GestureDetector`, `GestureDetector$OnGestureListener`, `OnTouchListener`, `ViewPropertyAnimator`), and the new dialog / keyboard widgets (`Toast`, `AlertDialog`, `AlertDialog$Builder`, `AlertDialog$1`, `Keyboard`). Every v0.2.0 mapping is copied verbatim. |
| `sdk/shrink-maps/v0.4.0.toml` | Adds the **DI + Service** surface — `picodroid.app.{Service, IBinder, Notification, Notification$Builder}`, `picodroid.content.{ServiceConnection, Intent, Context}`, and `picodroid.di.{ApplicationComponent, ActivitySingletonComponent}`. Every v0.3.0 mapping copied verbatim. |
| `sdk/shrink-maps/v0.5.0.toml` | Adds the **soft-keyboard polish** surface — `picodroid.widget.OnEditorActionListener`, `picodroid.view.inputmethod.EditorInfo`, plus internal anchor classes for the slide-up animation. Every v0.4.0 mapping copied verbatim. |
| `sdk/shrink-maps/v0.6.0.toml` | **Stable** — byte-identical to v0.5.0. The `picoenvmon` showcase app and the LTR559 driver landed without adding framework classes. |
| `sdk/shrink-maps/v0.7.0.toml` | Adds the **Tier C widget** surface — `picodroid.widget.{Snackbar, DatePicker, TimePicker, SwipeRefreshLayout}` and `picodroid.view.OnSwipeListener` (entries `a/CE`..`a/CI`). Every v0.6.0 mapping copied verbatim. |
| `sdk/shrink-maps/v0.8.0.toml` | **Stable** — byte-identical to v0.7.0. The PAPK ASSETS section (v1.1) and `imagedemo` landed outside the framework class set. |
| `sdk/shrink-maps/v0.9.0.toml` | **Stable** — byte-identical to v0.8.0. The Apache → GPL relicense, multi-family refactor (`platforms/`, `picodroid-core`), ESP32-S3 M1 scaffolding, and Activity Display singleton bootstrap shipped without adding framework classes. |
| `sdk/shrink-maps/v0.10.0.toml` | Adds the **Android-parity Tier 1/2** surface (+23 classes, 87 → 110) — `picodroid.view.ViewGroup` (+ `LayoutParams`), the adapter family (`Adapter`, `AdapterView`, `ArrayAdapter`, `BaseAdapter`), `picodroid.widget.CompoundButton`, `picodroid.content.DialogInterface`, and the typed listener interfaces (`View$OnClickListener`/`OnFocusChangeListener`, `CompoundButton$OnCheckedChangeListener`, `AdapterView$OnItemClickListener`, etc.). Every v0.9.0 mapping copied verbatim. |
| `sdk/shrink-maps/v0.11.0.toml` | Adds the **package-move + widget-completion** surface (+25 classes, 110 → 135) — `picodroid.app.AlertDialog` (+ `Builder`/`$1`, moved from `picodroid.widget`), `picodroid.content.SharedPreferences` (+ `Editor`), `picodroid.os.IBinder` (moved from `picodroid.app`), `picodroid.net.URL` / `HttpURLConnection` (Java-cased rename), `picodroid.text.{TextWatcher, InputType}`, `picodroid.view.Gravity`, `GestureDetector$SimpleOnGestureListener`, the `picodroid.view.animation` interpolator family, and `picodroid.widget.{NumberPicker, RadioButton, RadioGroup}`. Every v0.10.0 mapping copied verbatim. |
| `sdk/shrink-maps/v0.12.0.toml` | **Stable** — byte-identical to v0.11.0. The Pico 2 W networking bring-up, the FreeRTOS host simulator, the runtime-flash fixes, and the `picodroid-core` / `papk-format` / `pdb-protocol` extractions all landed outside the framework class set. |
| `sdk/shrink-maps/v0.13.0.toml` | **Stable** — byte-identical to v0.12.0. The typed `java.net` exceptions, `HttpURLConnection` header/timeout surface, `InetAddress.getByName`, `ServerSocket.setSoTimeout`, and `SystemClock.setCurrentTimeMillis` all landed as methods on classes the v0.11.0 cut already named; the JVM, GC, and memory-diagnostics work added no `sdk/java` classes. |
| `sdk/shrink-maps/v0.14.0.toml` | Adds the **concurrency + injection-point** surface (+14 classes, 135 → 149) — the pure-Java `java.util.concurrent` core set (`picodroid.concurrent.{Callable, Future, FutureTask, ExecutorService, ThreadPoolExecutor, TimeUnit, CountDownLatch, AtomicInteger, AtomicLong, AtomicBoolean, AtomicReference}`), `Thread$UncaughtExceptionHandler`, and the two injection points `javax.inject.Provider` / `picodroid.di.Lazy`. Every v0.13.0 mapping copied verbatim. |
| `sdk/shrink-maps/v0.15.0.toml` | Opens the **`b/` namespace for `java/**`** (+88 classes, 149 → 237) — `Object`, `String`, `StringBuilder`, the boxed types, the collection classes and interfaces, every builtin exception and the `java.lang.invoke` bootstrap names: every `java/**` class the framework references or pico-jvm serves. `a/` allocation is untouched; every v0.14.0 mapping copied verbatim. |
| `sdk/shrink-maps/v0.16.0.toml` | Schema 2: adds the **`[[member]]` section** — 868 method and field names of the framework mapped to 1–2-character targets (every v0.15.0 class mapping copied verbatim, class allocation untouched) and `member-floor = "0.16.0"`. Everything in `sdk/api-contract.tsv`'s member column, `<init>`, javac synthetics and names ≤ 2 chars stay verbatim. |
| `sdk/shrink-maps/v0.17.0.toml` | Maps the **last kept names** (+125 members, 868 → 993): the `java/**` contract members the runtime serves (`toString`, `hashCode`, `equals`, `hasNext`, …) and javac's `$` synthetics, previously kept because the Rust arms matched them by literal — the arms now match through the generated `m::` constants. `member-floor` re-based to `0.17.0`; only `main` and `injectMembers` stay verbatim. Classes unchanged (238). |
| `sdk/shrink-maps/v0.18.0.toml` | Adds the **Tier 1 small-methods** surface — one class (`java.util.Objects`, 238 → 239) and 14 members (993 → 1007): `getFloat` / `putFloat`, `DIRECTION_IN`, `createNewFile` / `mkdirs` / `getParent` / `getParentFile` / `getAbsolutePath`, `hash` / `isNull` / `nonNull` / `requireNonNull`, `intBitsToFloat`, `T_FLOAT`. `member-floor` stays `0.17.0`; every v0.17.0 mapping copied verbatim. |

## Scope

Maps rename **class names** (every release since v0.1.0) and, since
v0.16.0, **method and field names** (see [Member names](#member-names)).
Class names collapse into two synthetic packages, each with its own counter: `a/`
for framework classes (`picodroid/**`, `javax/**`; 42 of them in the first
cut) and, since v0.15.0, `b/` for the `java/**` classes pico-jvm serves
natively. Nothing translates either prefix back at run time: the Rust
runtime is compiled against constants generated from the active map
(`c::java_lang_String` is `"java/lang/String"` in a no-shrink build and
`"b/AQ"` under `--shrink` — see [Native dispatch](#native-dispatch--every-name-generated)),
so the JVM's tables, `catch` matching, `instanceof`, native dispatch and
`Class.getName()` all use the one spelling the class files carry. A
`--shrink` image contains no original name at all, exactly as a ProGuard
build does; `"x".getClass().getName()` returns `b.AQ`. Build without
`--shrink` for readable names, or pipe a shrunk log through
`scripts/retrace.sh` (see [Reading a shrunk log](#reading-a-shrunk-log)).

- Order: sort original internal names lexicographically.
- Suffix: bijective base-26 (`A`, `B`, …, `Z`, `AA`, `AB`, …), skipping
  Java reserved keywords.
- So `picodroid/app/Activity` → `a/A`,
  `picodroid/app/Application` → `a/B`, … `a/AP`.

The class-name step preserves the `.class` bytes outside the constant pool
byte-for-byte. Only `CONSTANT_Utf8_info` entries get rewritten — bare
class-name references and `Lfoo/Bar;` substrings inside descriptors. CP
indices stay stable so the trailing sections (attributes, `Code`) don't
need touching. (For a device build its input has already been through the
independent [debug-attribute strip](#debug-attribute-strip) and, under a
member map, the [member rename](#member-names).)

### Member names

Since map v0.16.0 the `[[member]]` section renames the framework's method
and field names — `setText` → `uQ`, `nativeCreate` → `mU` — in the SDK
corpus and in every app PAPK. The map is **keyed by bare name, not by
owner**: `onCreate` renames the same way in `Activity`, in `Application`,
and in your `MainActivity` override, and at every call site, so overrides
and callers stay in lockstep without any per-class analysis. JVM method
and field namespaces are disjoint, so one target serves both kinds.

What is mapped: every method and field name the SDK declares, javac's
synthetics (`$VALUES`, `lambda$…`, `access$…`) included, **and every
member name in `sdk/api-contract.tsv`** — `toString`, `equals`,
`hashCode`, `run`, `compare`, `compareTo`, `hasNext`, `next`, `close`,
`getMessage`, `name`, `ordinal`, `get`, `put`, `size`, `append`, … Those
belong to `java/**` classes that have no class file (pico-jvm serves them
from Rust arms), and until v0.17.0 they were kept because the arms
matched them by literal. The arms now match through the generated `m::`
constants, so an app's `Point.toString()` and the interpreter's `"" +
point` rename in lockstep and the mapped name is found exactly as the
original was.

What is never mapped (`cut-release` enforces all of it):

- `<init>` / `<clinit>`, names of two characters or fewer, and every
  member of a `kotlin/**` class (the shim is reserved, not renamed) or of
  an annotation interface;
- `[[member]]` entries in `sdk/keep.toml` (`main`, `injectMembers` — app
  entry points invoked by literal from Rust on classes the SDK never
  declares, kept the way ProGuard keeps `main`).

Targets are `a`–`z`, `A`–`Z`, then two characters (letter + letter/digit)
with all-lowercase pairs excluded — apps declare members called `id`,
`io`, `eq`, `of`, and the kotlin-shim has `to` — and never equal any name
spelled anywhere in the SDK corpus, the shim, the contract or the keep
list. Two characters cover the ~870 names in the first cut with room to
spare.

The rewrite is an ASM pass (`buildSrc`'s `ShrinkMembersTask`, a
`ClassRemapper` over the `[[member]]` rows), not the Rust class-name tool:
`ClassWriter(0)` rebuilds the constant pool, so a `Utf8` slot javac
shared between a member name and an `ldc` string literal — every enum
constant's name — comes out as two, and `TimeUnit.SECONDS.name()` still
says `SECONDS`. It runs before the Rust class pass: for app PAPKs as the
`shrinkMembers` task between the optional strip and `shrinkClasses`, for
the SDK corpus as `:sdk:shrinkMembersStripped` / `:sdk:shrinkMembersRaw`,
which `build.rs` invokes (map path as `-Ppicodroid.shrinkMap`) whenever the
active map has member rows. `papk-pack --shrink-map` translates the
`onCreate` it looks for in the entry point the same way.

**Compatibility.** Class renames are additive, so an older PAPK keeps
resolving on newer firmware. A map that renames *existing* members
breaks that rule, so it moves the member floor
(`compat::MEMBER_SHRINK_FLOOR`, recorded as `member-floor` in every map
from v0.16.0 on): v0.16.0 introduced member renames, v0.17.0 extended them
to the contract members, and firmware at or past the floor rejects a
shrunk PAPK cut before it with `FrameworkVersionMismatch` (`pdb install`
says *PAPK was shrunk before method/field names were*). Every workflow
rebuilds PAPK and firmware together; the check turns the stale case into
a clear error instead of a runtime `NoSuchMethod`. Un-keeping another
member later would need another floor.

## Enabling shrinking

Off by default. Opt in with `--shrink` on any top-level script:

```bash
./scripts/build.sh     --app helloworld --shrink
./scripts/flash.sh     --app blinky     --shrink
./scripts/sim.sh       --app helloworld --shrink
./scripts/build-apk.sh --app helloworld --shrink
```

The flag exports `PICODROID_SHRINK=1`, which both `build.rs` and
`build-apk.sh` pick up. Without it, both sides emit the `0.0.0`
sentinel and no framework `.class` bytes are touched.

## App shrinking (`--shrink-app`)

`--shrink-app` (on `build-apk.sh`, `build.sh`, `flash.sh`, `sim.sh`;
env `PICODROID_SHRINK_APP=1`) renames the **app's own** classes and
private members too, ProGuard-style. It requires `--shrink`: the app's
member targets are allocated by continuing the release map's counter,
so there is nothing to build on without that map.

What happens, per PAPK build (`buildSrc` task `cutAppShrinkMap`):

1. `class-shrink cut-app` copies the active release map and appends
   - every class the app defines, under a third prefix `c/` (`c/A`,
     `c/B`, …), except `kotlin/**` (the shim is kept, see below);
   - every method/field name an app class declares that is longer than
     two characters and not already in the release map (SDK overrides
     such as `onCreate` rename in lockstep through the release rows),
     not kept (`main`, `injectMembers`), and not spelled by a kept class.
     Targets skip every name the app tree, `sdk/member-names.tsv`,
     `sdk/api-contract.tsv`, the keep list and the release map spell.
2. `shrinkMembers`, `shrinkClasses` and `papk-pack` read that **merged**
   map instead of the release map. `papk-pack` spells the manifest entry
   (`main-class` / `activity` / `application`) through it.
3. `build-apk.sh` copies the merged map next to the PAPK as
   `build/apks/<app>.shrink-map.toml`. That file is the PAPK's retrace
   key: `./scripts/retrace.sh build/apks/<app>.shrink-map.toml < log`.

The firmware is untouched: it resolves app classes only through the
PAPK's own class table, and `framework-map-version` still names the
release map. The one structural dependency is the `@Inject` support —
`lifecycle.rs` derives `<runtime class name, $→_>_MembersInjector` from
the component's name — so `cut-app` names an injector after its
component's shrunk name (`c/A` → `c/A_MembersInjector`); an injector
whose component is kept is kept too.

Consequences and limits:

- `Class.getName()` on an app class returns `c.A`; string literals are
  never rewritten, so an app that compares `getName()` against its own
  name literal sees a mismatch (`examples/classlit` accepts either).
- Log lines, stack traces and `pdb` output spell `c/A.qZ` — retrace with
  the app map, not the release map.
- Default-package classes and packages named `a/`, `b/` or `c/` are
  rejected by `cut-app` (they would alias the synthetic prefixes).
- Generic `Signature` attributes are not rewritten; device PAPKs strip
  them, an unstripped (sim) PAPK keeps original names inside them.
- The kotlin-shim stays verbatim: its classes are keep-globbed and its
  member names are neither candidates nor targets.

## How builds consume the active map

When `PICODROID_SHRINK=1`, `class-shrink print-version` resolves the
active version from the root `Cargo.toml` + `sdk/shrink-maps/`. Both
sides of the build call it:

1. **Firmware (`build.rs`)**: after Gradle has compiled — and, for a
   debug-assertions-off build, [stripped](#debug-attribute-strip) — the
   framework classes, if shrinking is on and the active version isn't
   `0.0.0`, applies the map to them and embeds the shrunk output via
   `FRAMEWORK_CLASSES`. Also writes `framework_mapping_version.rs`
   (the version string the firmware advertises) and `names.rs` (the
   `c::` / `m::` / `d::` constants, spelled through the same map).

2. **Apps (`scripts/build-apk.sh`)**: if shrinking is on, runs
   `class-shrink shrink-dir` on the app's `.class` output. The release
   map covers framework classes only, so by default the app's own
   classes pass through unchanged — only cross-references like
   `Lpicodroid/app/Application;` in the app's super_class get
   rewritten. `--shrink-app` (below) extends that to the app itself.

3. **PAPK manifest**: `papk-pack` writes the active version (or
   `0.0.0` when shrinking is off) into the `framework-map-version`
   manifest key.

4. **Load time**: `platforms/rp/src/app.rs` calls `papk.verify_compat(FRAMEWORK_MAP_VERSION)`
   right after parsing. A PAPK built with mismatched shrink settings
   (one side `0.0.0`, the other non-zero) is rejected with a hard
   error asking to rebuild.

## Debug-attribute strip

Independent of the shrink map, and applied to everything bound for a device:
the `.class` files a device firmware carries lose the attributes pico-jvm
never reads there. `StackMapTable` has no reader at all — picodroid does not
run a bytecode verifier — and `LineNumberTable` + `SourceFile` are read only
by a firmware built with the `line-numbers` cargo feature, which feeds the
`(File.java:42)` in stack traces. `scripts/flash.sh` enables that feature for
its default debug profile and drops it for `--release`, and builds the PAPK
to match (`build-apk.sh --strip-debug --keep-lines` keeps just those two
attributes). Release firmware therefore carries none of the three: about
15 KB of SDK corpus on every board plus 10–15 % of each PAPK, and its stack
traces print the bytecode offset, `(pc=9)`, which `scripts/retrace.sh`
resolves on the host (below). The measurements and the design are in
[docs/designs/flash-string-budget-2026-08.md](https://github.com/shivrajora/picodroid-rs/blob/main/docs/designs/flash-string-budget-2026-08.md)
§4.

How it is applied:

- **SDK corpus (`build.rs`)** — when `CARGO_CFG_DEBUG_ASSERTIONS` is absent
  (every `build.sh` / `flash.sh` build, and `--release` sim builds), the
  build script runs `./gradlew :sdk:stripClasses` and embeds
  `sdk/build/classes-stripped/java/main` instead of `compileJava`'s tree;
  the shrink step then runs on top, unchanged. The generated
  `framework_classes.rs` records the choice in
  `FRAMEWORK_CLASSES_DEBUG_STRIPPED`, and a `picodroid-core` test pins it to
  `!cfg!(debug_assertions)`.
- **PAPKs** — `scripts/build-apk.sh --strip-debug` (Gradle property
  `-Ppicodroid.stripDebug=true`), which every device path passes:
  `build.sh` / `flash.sh`, `hil-run.sh`, the size ratchet and pre-commit's
  firmware snapshot. `sim.sh` and the other host paths leave it off, so a
  dev-profile sim run still shows `at Foo.bar(:42)` for app frames, and a
  Java PAPK built without the flag is byte-identical to before.

The rewrite is the ASM strip Kotlin apps already go through
([ClassStrip.kt](https://github.com/shivrajora/picodroid-rs/blob/main/buildSrc/src/main/kotlin/picodroid/classfile/ClassStrip.kt)):
it also drops annotations, `InnerClasses`, `Signature` and the other
attributes the JVM skips, and because each class is re-serialised without a
source reader the constant pool is rebuilt — the orphaned
`"LineNumberTable"`, `"Foo.java"`, … entries go with it. The result has no
`StackMapTable`, so a HotSpot JVM refuses to load it; only pico-jvm may
consume `sdk/build/classes-stripped/` or a `--strip-debug` PAPK.

To inspect: `javap -v` on a class under `sdk/build/classes-stripped/java/main`
(or its shrunk copy under
`target/<triple>/<profile>/build/picodroid-core-*/out/framework_classes_shrunk`)
lists no `LineNumberTable` / `SourceFile` / `StackMapTable`; `papk-info`
shows the per-class sizes of a PAPK built with and without the flag.

Two things to keep in mind:

- `./gradlew :examples:<app>:install` builds its PAPK in the same Gradle
  invocation, so it ships unstripped unless you pass
  `-Ppicodroid.stripDebug=true` — larger, not incorrect.
- Anything that wants to read annotations from SDK or app classes (a
  `@KeepName`-style keep, say) must read the compiler's output, not what
  ships: the strip removes them.

## Compatibility rules

`verify_compat` accepts these combinations and rejects all others:

| Firmware    | PAPK        | Accepted? | Why |
|-------------|-------------|-----------|-----|
| `0.0.0`     | `0.0.0`     | Yes       | Both unshrunk, names match. |
| `v` ≥ 0.17.0 | `v'` < 0.17.0 | No (`FrameworkVersionMismatch`) | PAPK predates the member floor: names the firmware now spells mapped are verbatim in the PAPK. |
| `v` (≥1)    | `v'` (≥1) and `v' ≤ v` | Yes | Append-only maps: every shrunk name the PAPK uses is still present in firmware. |
| `v` (≥1)    | `v'` (≥1) and `v' > v` | No (`FrameworkVersionMismatch`) | PAPK may reference shrunk names added after firmware's release. |
| `0.0.0`     | non-zero    | No        | PAPK's shrunk refs don't exist in unshrunk firmware. |
| non-zero    | `0.0.0`     | No        | PAPK's original refs don't exist in shrunk firmware. |
| anything    | unversioned (legacy, pre-M1) | Only if firmware is `0.0.0` (`FrameworkVersionMissing` otherwise) | Backward compat. |

## Native dispatch — every name generated

Rust never spells a Java class, member or descriptor as a string literal.
`build_support/names.rs` — run by both `jvm/build.rs` and
`picodroid-core/build.rs` — turns three committed lists into three
`const` modules whose *values* are whatever the loaded framework spells:

| Module | Source | Example | no-shrink | `--shrink` |
|---|---|---|---|---|
| `c::` | `sdk/class-names.tsv` + the contract's classes | `c::picodroid_pio_Gpio` | `"picodroid/pio/Gpio"` | `"a/S"` |
| `m::` | `sdk/member-names.tsv` + the contract's members | `m::setValue` | `"setValue"` | `"uQ"` |
| `d::` | `sdk/descriptors.tsv` | `d::String__V` | `"(Ljava/lang/String;)V"` | `"(Lb/AQ;)V"` |

Every `(class, method)` match arm in `picodroid-core/src/native_handler/**`
and `jvm/src/native/**`, every `DISPATCH_SITES` row, every
`PICODROID_NATIVE_CLASSES` / `BUILTIN_CLASS_NAMES` entry, every `catch`
and `instanceof` table, and every Rust-side allocation goes through them:

```rust
use crate::shrink_names::{c, m};

pub fn dispatch(class_name: &str, method_name: &str, ctx: &mut NativeContext<'_>)
    -> Option<Result<Option<Value>, JvmError>>
{
    match (class_name, method_name) {
        (c::picodroid_pio_Gpio, m::setValue) => ...,
        // ...
    }
}
```

A `const` in a match pattern compiles to exactly the code the literal
did, so dispatch costs nothing in either mode, and a shrink build has no
second copy of any name — the `unshrink_class` match this replaced was
12 KB of code plus 5 KB of original names in every shrink image. The
lists are kept current by picodroid-core's `class_names_are_current` /
`member_names_are_current` tests (`scripts/gen-api-contract.sh`
regenerates them); `sdk/descriptors.tsv` is hand-maintained. Three tests
keep the invariant: `handled_rows_use_member_consts`,
`no_original_name_literals` (no `"java/…"`, `"picodroid/…"`, `"(L…;)…"`
or served-member literal in any non-test source of either crate), and
`names_agree_with_pico_jvm` (both crates generated the same spellings).
`pre-commit --full` additionally links the rp2350 `--release --shrink`
image and greps it (`scripts/check-shrunk-image.sh`). Test-only
`unshrink_class` / `unshrink_member` / `unshrink_descriptor` and the
fixture respeller `pico_jvm::names::spelled` let the corpus-reading tests
and the JVM's hand-assembled class-file fixtures run in both lanes.

## Reading a shrunk log

A `--shrink` firmware prints mapped names in `Class.getName()`, uncaught
exception banners, stack traces and `pdb` output. `scripts/retrace.sh`
(`class-shrink retrace --map <file>`) is the host-side inverse, as
ProGuard's `retrace` is: it substitutes `a/DK` / `a.DK` / `b/AK` / `b.AK`
tokens and member targets in `.name(` position back to their originals
and passes everything else through.

It also resolves stack-trace frames. A release firmware carries no
`LineNumberTable` and prints `at pkg.Class.method(pc=9)`; `retrace.sh`
reads the unstripped class trees this checkout compiled — the SDK's
`sdk/build/classes/java/main` always, the app's with `--app <name>` — and
rewrites the frame to the `at pkg.Class.method(Class.java:42)` the sim and
a debug-profile device print themselves. Names are un-shrunk first, so the
two compose. A frame that resolves to nothing is left alone; overloads of
one name that disagree list every candidate (`Class.java:12|40`), since
the frame carries no descriptor. The trees must come from the same sources
the device is running.

```bash
./scripts/sim.sh --app foo --shrink 2>&1 | ./scripts/retrace.sh
./scripts/retrace.sh sdk/shrink-maps/v0.18.0.toml < build/hil/logs/foo.shrink.log
./scripts/retrace.sh --app foo < rtt-release.log   # (pc=N) -> (Foo.java:LINE)
```

## Keep list

`sdk/keep.toml` declares names the shrinker must never touch:

- `picodroid/annotation/KeepName` (exact): the annotation class used
  by future method/field keeps in Java source. Such a keep must read
  `compileJava`'s output — the
  [debug-attribute strip](#debug-attribute-strip) removes annotations from
  what ships.
- `kotlin/**` (glob): the hand-written stdlib shim that rides inside
  Kotlin apps' PAPKs (`kotlin-shim/`). kotlinc-compiled app classes name
  these classes literally, and maps are generated from the SDK set anyway,
  so this documents the invariant more than it enforces it.

- `[[member]]` names (`main`, `injectMembers`): app entry points Rust
  invokes by literal on classes the SDK never declares.

Nothing else is kept. New framework surface referenced from Rust goes
through the generated constants (`scripts/gen-api-contract.sh` adds the
name to the lists); adding a keep is a permanent cost in every shrink
image and, for a member, a compatibility floor.

## Cutting a release

Update `sdk/shrink-maps/` whenever you bump the picodroid package
version:

```bash
# Fresh-compile the framework to a scratch dir.
TMP=$(mktemp -d)
find sdk/java -name '*.java' -print0 \
  | xargs -0 javac --release 8 -Xlint:-options -d "$TMP"

# The kotlin-shim's member names must never become member targets.
./gradlew :kotlin-shim:compileJava -q

# Generate the map. --base copies the previous release verbatim so the
# append-only invariant is enforced automatically; --extra-names feeds the
# java/** names the framework never references itself from the committed
# list of everything pico-jvm serves; --members allocates method/field
# targets for everything the SDK declares plus the --contract member
# column, reserving every name the --reserve tree spells. --floor (not
# shown) re-bases member-floor for a cut that renames names the previous
# map left verbatim.
cargo run -p class-shrink -- cut-release --members \
  --classes-dir "$TMP" \
  --keep sdk/keep.toml \
  --extra-names sdk/api-contract.tsv \
  --contract sdk/api-contract.tsv \
  --reserve kotlin-shim/build/classes/java/main \
  --base sdk/shrink-maps/v<previous>.toml \
  --version <new> \
  --out  sdk/shrink-maps/v<new>.toml

# Commit both the map and the Cargo.toml version bump in the same commit.
```

From that commit onwards, `build.rs` and `scripts/build-apk.sh` pick
up the new map automatically.

## What's committed

- `tools/class-shrink/` — the shrinker binary and library.
- `sdk/keep.toml` — keep list.
- `sdk/shrink-maps/v*.toml` — one file per release, immutable.
- `sdk/class-names.tsv`, `sdk/member-names.tsv`, `sdk/descriptors.tsv` —
  the inputs of the generated `c::` / `m::` / `d::` constants.
- `picodroid-core/src/shrink_names.rs` and `jvm/src/names.rs` — one-line
  modules that `include!` the generated `names.rs` from each crate's
  `OUT_DIR`.

## What's generated at build time (OUT_DIR)

Always emitted:

- `framework_mapping_version.rs` — `pub const FRAMEWORK_MAP_VERSION: &str = "…";`
  (`"0.0.0"` when shrinking is off).
- `names.rs` — the `c::` / `m::` / `d::` constant modules
  (`build_support/names.rs`), spelled through the active map; original
  spellings when shrinking is off. Plus test-only reverse translators.
- `framework_classes.rs` — `pub static FRAMEWORK_CLASSES: &[&[u8]] = &[…];`
  pointing at (shrunk or raw) class files, plus
  `FRAMEWORK_CLASSES_DEBUG_STRIPPED`. For a debug-assertions-off build the
  raw tree is `sdk/build/classes-stripped/java/main`, written by
  `:sdk:stripClasses` (see [Debug-attribute strip](#debug-attribute-strip)).

Emitted only when shrinking is on and a map is active:

- `framework_classes_shrunk/…` — shrunk class files.

## CI coverage

Both [scripts/sim-run.sh](https://github.com/shivrajora/picodroid-rs/blob/main/scripts/sim-run.sh) and
[scripts/hil-run.sh](https://github.com/shivrajora/picodroid-rs/blob/main/scripts/hil-run.sh) run the full test matrix
twice — once with shrinking off, once with it on. Each result is
tagged with `[no-shrink]` or `[shrink]` so regressions on either side
are obvious. Pass `--mode no-shrink`, `--mode shrink`, or `--mode both`
(default) to narrow the run.

The HIL suite also exercises rejection paths — three test rows per mode
(see [scripts/hil-tests.conf](https://github.com/shivrajora/picodroid-rs/tree/main/scripts/hil-tests.conf)):

| Row                            | What it tests |
|--------------------------------|---------------|
| `install-reject-host`          | Build a PAPK in the OPPOSITE shrink mode of the firmware; assert `pdb` refuses pre-flight and the device still PINGs after. |
| `install-reject-device`        | Same as above but with `--skip-host-check`; assert the device returns `STATUS_INCOMPAT` in Phase A and stays alive. |
| `install-reject-future`        | Synthesize a future map (`v0.<MIN+1>.0.toml`) via [scripts/test-future-version-rejection.sh](https://github.com/shivrajora/picodroid-rs/blob/main/scripts/test-future-version-rejection.sh), build a PAPK against it, assert rejection. Only meaningful in shrink mode. |

After every rejection, `hil-run.sh` runs a `pdb ping` to confirm the
device is responsive — a successful rejection must not have erased flash
or rebooted.

## `pdb install` pre-flight

`pdb install` has two compatibility gates so a bad install never reboots
the device:

1. **Host pre-flight** in [tools/pdb/src/install.rs](https://github.com/shivrajora/picodroid-rs/blob/main/tools/pdb/src/install.rs):
   after PING, before sending the install header, parse the PAPK manifest
   for `framework-map-version`, compare to the firmware's version learned
   from the new PING greeting, and exit with a clear error if `compat::check`
   rejects.
2. **Device-side check** in [picodroid-core/src/install/orchestrator.rs](https://github.com/shivrajora/picodroid-rs/blob/main/picodroid-core/src/install/orchestrator.rs):
   after stopping the JVM but before erasing flash, peek the first
   `INSTALL_PEEK_BYTES` (512) of the PAPK off the wire, run `compat::check`,
   and reply `STATUS_INCOMPAT` on mismatch. The host inlines those bytes
   right after the install header so the peek doesn't stall.

The PING greeting was bumped from `picodroid/2.0` to `picodroid/2.1` and
gained a trailing `[u8 len][N bytes]` field for the firmware's
`framework-map-version`. `pdb install` hard-refuses old `picodroid/2.0`
firmware (you must reflash via SWD) since it can't verify compatibility.

For testing, `pdb install` accepts two flags (used by the HIL reject rows):
`--skip-host-check` (bypass the host pre-flight) and `--expect-rejected`
(invert exit codes — refusal = success).

## Diagnosing version mismatch

`PapkError::FrameworkVersionMismatch` means the PAPK was packaged
against a shrink map newer than what the firmware knows. Rebuild the
PAPK against matching firmware:

```bash
./scripts/build-apk.sh --app <name>
```

`PapkError::FrameworkVersionMissing` means the PAPK predates the
manifest key but the firmware has a shrink map active. Again, rebuild.
