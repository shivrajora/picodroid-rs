# The Android-shaped manifest: `<activity>`, `<service>`, `android:theme`

**Status: built 2026-10-09; the only form since 2026-10-26** (amendment of that date: the short
form is gone, the framework launches the entry Activity after `Application.onCreate`) (decision F4 of
[claudeusage-decisions-2026-10.md](claudeusage-decisions-2026-10.md), "the small half": components
and the theme, not intent filters or permissions). Amendments at the bottom record where execution
diverged from the plan.

## 0. Why

`PicodroidManifest.xml` named one entry class as an attribute (`activity="pkg/Main"`) and nothing
else: a Service was declared nowhere and `startService(new Intent(Svc.class))` named it directly,
and the theme was whichever `<style>` happened to be called `AppTheme`, which the app then applied
itself with `setTheme(R.style.AppTheme)` in `onCreate`. Android declares the surface:
`<application android:theme=…>` with `<activity android:name=…>` and `<service android:name=…>`
children, and the framework applies the theme before `onCreate`. The decision was to add that form,
keep the short form as the compatible spelling so no example breaks, make an undeclared Service a
build-time error (the build sees every class literal), and leave intent filters and permissions.

## 1. What an app writes

```xml
<manifest package="claudeusage" version="1.0">
    <application android:theme="@style/AppTheme" label="Claude Usage">
        <activity android:name=".ui.MainActivity">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
        <service android:name=".data.UsageService" />
    </application>
</manifest>
```

- **`<activity android:name>`**, repeatable. The one carrying the `MAIN` intent filter is the
  entry, else the first declared; the short-form `activity=` attribute may stay and must then name
  one of them. Names are read as Android reads them: dotted, slash-form, or relative to the manifest
  `package` with a leading dot.
- **`<service android:name>`**, repeatable. A Service the code starts and the manifest does not
  declare fails the build (Android ignores the start at run time; see §3.3).
- **`android:theme="@style/Name"`** on `<application>`: the `<style>` that `?attr/…` resolves
  against at build time and that the framework applies before the first Activity's `onCreate`. An
  app that names it drops its own `setTheme` call. Without the attribute the style called
  `AppTheme` still serves `?attr/`, and nothing is applied for the app (unchanged).
- **`<uses-permission>`** is read and ignored, so an Android manifest's permission lines do not
  break the build; nothing on this platform enforces a permission, and the reference page says so
  (decision F4: declaring without enforcing would be a false sandbox).
- The bare spellings `name=` / `theme=` are accepted beside `android:name` / `android:theme`
  (the parser is not namespace-aware; the prefix is part of the attribute name).
- Not read: `<meta-data>`, `<receiver>`, `<provider>`, per-activity themes (one theme per app),
  anything on `<intent-filter>` but the `MAIN` action. An unknown child of `<application>` is a
  build error naming what is supported.

The short form (`<application activity="pkg/Main"/>` alone) is unchanged: it declares nothing,
nothing is checked, and `AppTheme` is the theme by name. Every example but four keeps it.

## 2. What the build does

### 2.1 Gradle (`buildSrc`)

`ManifestSchema.kt` parses the children and `android:theme` into `activities`, `services`
(slash form) and `theme` (the style name), picks the entry as above, and keeps every existing
error. Two new tasks consume them:

- `verifyManifest` (`ManifestComponentsTask.kt`), before `packPapk`: a no-op for the short form.
  With components declared, it walks the app's bytecode with the same ASM scanner
  `verifyApiContract` uses (`classfile/ClassRefs.kt`) and takes, per method, the class literal
  that precedes `Intent.<init>(Class)` or `Intent.<init>(Context, Class)` as an Intent target. A
  target that extends `picodroid.app.Service` (walking superclasses through the app's own classes)
  must be a declared `<service>`; when the manifest declares activities, a target extending
  `Activity` must be a declared `<activity>`. The error lists the elements to add. `setClassName`
  is dynamic and is not checked.
- `packPapk` passes `--activity-decl`, `--service` (repeatable) and `--theme`; `generateR`
  passes `--theme`, so `R.java`'s ids and the packed table resolve `?attr/` against the same
  style.

### 2.2 `papk-pack`

- `--activity-decl` / `--service`: each class must be among the packed classes and extend the
  right base. `classcheck::super_class_name` reads a class file's superclass; the chain is walked
  through the packed classes to `picodroid/app/Activity` or `Service` (spelled through the shrink
  map, like the entry check's descriptors). A chain that reaches `java/lang/Object` fails; one that
  leaves the app's classes at a framework class this tool cannot see is let through. Under
  `--shrink-app` the declared names are spelled through the map, as the entry is.
- `--theme`: `res::compile_with` takes the style name, `Values.theme` replaces the `APP_THEME`
  constant in `theme_text`, and the style's `R.style` id (`res::style_id`) is the manifest value.
  A theme without a `res/` tree, or naming a style `res/values` does not define, is an error that
  names the style.
- Three manifest keys (`papk_format::keys`): `activities` and `services` (comma-joined, slash
  form) and `theme` (the id, decimal). They ride as extra entries, so no `ManifestSpec` literal in
  the nine places that build one had to change, and a `--repack` carries them like any extra.

### 2.3 The runtime

`resources::init_from_papk` reads `theme` into a cell beside the table; `Resources
.nativeManifestTheme()` returns it (0 for none) and `Resources.applyManifestTheme()` applies it
once per app. `Activity.performCreate` calls that before `onCreate`, which is where Android applies
the manifest theme, and before any view exists. An `application=` app's `Application.onCreate`
runs before any Activity, so a theme is in force from the first Activity on, not during
`Application.onCreate`; nothing visual exists yet then. `setTheme` still works for an app that
picks a theme at run time, and wins, since it runs later. The declared component lists are not
read at run time: the build already refused anything the lists would catch.

## 3. What is deliberately not here

- **Intent filters** beyond `MAIN`: one launcher entry per app, no implicit intents.
- **Permissions**: read and ignored. `WIFI`, `INTERNET`, the LED are open; enforcement is its own
  project (F4's reasoning stands).
- **A run-time refusal** of an undeclared Service. Android throws from `startService`; here the
  build is the gate, which is earlier and cannot be missed. An app whose Intent target is computed
  (`setClassName`) is not checked by either.
- **Link roots**: the decision doc expected the packer to add declared classes to a reachability
  root set. The packer packs every class under `--classes-dir` and has no reachability pass, so
  there is nothing to add; `validate_components` checks existence and the base class instead.

## 4. The examples

`claudeusage` moves to the full Android form (theme, the Activity with its `MAIN` filter, the
Service) and drops `setTheme`; `layoutdemo` names its theme and drops `setTheme`, and its three
`setTheme:` checks now verify the manifest theme was applied before `onCreate`; `servicedemo` and
`tutorial_service` declare their Service beside the short-form entry. The other hundred manifests
are unchanged.

## 5. Verification

- Every example APK builds (`./gradlew assemblePapk`, CI's job); `layoutdemo` on `pico_touch_kit`
  prints `=== ALL PASSED ===` with the theme checks; `claudeusage` and `servicedemo` build and run in
  the simulator.
- `cargo test -p papk-pack -p papk-format`: `super_class_name` on the fixture class, the new flags'
  parsing, `compile_with` resolving `?attr/` against a style not called `AppTheme`.
- A negative check by hand: add `startService(new Intent(Other.class))` to `servicedemo` without a
  `<service>` for it and the build fails with the element to add.

## 6. Amendments

- 2026-10-09: the component check lives in `buildSrc` (ASM over pre-shrink classes), not in
  `papk-pack`: the Rust tool has no bytecode walker, and the Gradle side already runs one per app
  for the API contract.
- 2026-10-09: the XSD models the bare `name` / `theme` attributes only; it declares no namespace
  for the `android:` prefix. The build accepts both spellings.
- 2026-10-18: `<receiver>` is read ([broadcasts-2026-10.md](broadcasts-2026-10.md) §3.5): a
  class plus the actions its `<intent-filter>` names, of which only `BOOT_COMPLETED`, `TIME_SET`
  and `TIMEZONE_CHANGED` are accepted (the rest are registered-only, and the build says so).
  The packer writes the `receivers` key and checks the class extends `BroadcastReceiver`; the
  component check requires a `<receiver>` for every class an explicit broadcast Intent names,
  once the manifest declares any component. The runtime delivers to a package that is not
  running by a headless run of it. "Not read: `<receiver>`" in §1 is retired.
- 2026-10-26: **the Android form is the only form** (follow-up 1 of
  [android-idiom-sweep-2026-10.md](../android-idiom-sweep-2026-10.md)). Every manifest under
  `examples/` and `system-apps/` was converted; the parser refuses the old spellings with the
  Android one to write:
  - `<application android:name>` names the Application class; `application=`, `activity=` and
    `main-class=` on `<application>` are build errors. The entry Activity is always an `<activity>`
    (the `MAIN` filter's, else the first). An app has an Application, an entry Activity or both.
    `main-class` survives only as `papk-pack --main-class` (fixtures); no manifest names one.
  - **The framework launches the entry Activity.** With both, `papk-pack` writes the
    `application` and the `activity` key (`EntryPoint::ApplicationActivity`), and
    `lifecycle::run_application` pushes the `activity` class after `Application.onCreate` when
    `onCreate` queued no Activity push (and no `Launch`), as Android launches the launcher
    Activity once the Application exists. A manifest-receiver run stays headless (the entry is not
    launched). An explicit `startActivity` from `onCreate` still wins (qa_life keeps one, to pass
    extras). Thirty apps' `startActivity`-from-`Application.onCreate` calls went; seventeen
    Application classes whose only job that was were deleted (those apps are Activity-only now),
    and `reclaimdemo`'s became a plain holder class. Pinned by
    `qa_shape_guards::run_application_launches_the_entry_activity_unless_headless`. The entry
    Activity is launched with no Intent (`getIntent()` is null), as an `activity=` boot always
    was; Android hands it an `ACTION_MAIN` one.
  - `android:` only: `android:name` / `android:label` / `android:theme` / `android:required`;
    bare `name=` / `label=` / `theme=` / `required=` are errors, and `<manifest>` must declare
    `xmlns:android` (so the files are well-formed namespaced XML; Python's ElementTree reads them in
    `test-future-version-rejection.sh`). Picodroid's own attributes stay bare: `icon` (an
    `assets/` file name, where Android's `android:icon` is a drawable reference),
    `<supports-screens design-width design-height>`.
  - `android:versionName` / `android:versionCode` replace `version` / `version-code`.
  - `android:label="@string/name"` is resolved by `papk-pack` against the compiled `res/values`
    (default configuration) and written as text; eight apps use it.
  - `<uses-permission>` belongs under `<manifest>` (still read and ignored); under `<application>`
    it is an error. `<uses-feature>` takes `android:name` / `android:required` (the default stays
    not-required, Android's is required). An unknown child of `<manifest>` is an error, as one of
    `<application>` was.
  - `verifyManifest` now runs for most apps (they declare components), which found undeclared
    Activities in `settings`, `fragmentdemo` and `pagerdemo` and an explicit-broadcast receiver in
    `qa_bcast`; all declared.
  - The XSD declares the `android:` attributes in their namespace
    (`sdk/PicodroidManifest-android.xsd`, imported); amendment 2026-10-09 on the XSD is retired.
  The §1 short-form paragraphs and §4 describe the 2026-10-09 state.
