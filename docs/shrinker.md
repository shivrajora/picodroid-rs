# Class-name Shrinker

Picodroid ships a build-time class-name shrinker (`tools/class-shrink/`)
that rewrites framework `.class` files to use short synthetic names
(`picodroid/pio/Gpio` → `a/S`, etc.). It trims a few kilobytes from
firmware Flash and from every `.papk` without any change to Java source
or to the native dispatch layer — the translation is completely
transparent at runtime.

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
([jvm/src/apk.rs](../jvm/src/apk.rs) `verify_compat`): a PAPK with a map
version greater than the firmware's is rejected with
`PapkError::FrameworkVersionMismatch`.

## Active maps

Two release maps are committed today:

| Map | Covers |
|-----|--------|
| `sdk/shrink-maps/v0.1.0.toml` | Original 42 framework classes from the first release cut. |
| `sdk/shrink-maps/v0.2.0.toml` | Adds the classes introduced after v0.1.0 — `Executors` / `Executor` / `MainExecutor` / `BackgroundExecutor`, the `SensorManager` family (`Sensor`, `SensorEvent`, `SensorEventListener`, `SensorManager`), the HTTP client (`Url`, `HttpUrlConnection`, `HttpInputStream`, `HttpOutputStream`), and `KeyEvent` / `OnKeyListener`. Every v0.1.0 mapping is copied verbatim so PAPKs built against v0.1.0 firmware continue to run on v0.2.0 firmware. |

## v1 scope

v1 shrinks **class names only**. Method and field names stay untouched
for now — a later release map can add them (still append-only). The
original 42 non-`java/**` framework classes collapse into a single
synthetic package `a/`:

- Order: sort original internal names lexicographically.
- Suffix: bijective base-26 (`A`, `B`, …, `Z`, `AA`, `AB`, …), skipping
  Java reserved keywords.
- So `picodroid/app/Activity` → `a/A`,
  `picodroid/app/Application` → `a/B`, … `a/AP`.

The `.class` bytes outside the constant pool are preserved byte-for-byte.
Only `CONSTANT_Utf8_info` entries get rewritten — bare class-name
references and `Lfoo/Bar;` substrings inside descriptors. CP indices
stay stable so the trailing sections (attributes, `Code`,
`StackMapTable`) don't need touching.

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

## How builds consume the active map

When `PICODROID_SHRINK=1`, `class-shrink print-version` resolves the
active version from the root `Cargo.toml` + `sdk/shrink-maps/`. Both
sides of the build call it:

1. **Firmware (`build.rs`)**: after `javac`, if shrinking is on and the
   active version isn't `0.0.0`, applies the map to the compiled
   framework classes and embeds the shrunk output via
   `FRAMEWORK_CLASSES`. Also writes `framework_mapping_version.rs`
   (the version string the firmware advertises) and
   `framework_unshrink.rs` (the reverse-translation table).

2. **Apps (`scripts/build-apk.sh`)**: if shrinking is on, runs
   `class-shrink shrink-dir` on the app's `.class` output. The map
   covers framework classes only, so the app's own classes pass
   through unchanged — only cross-references like
   `Lpicodroid/app/Application;` in the app's super_class get
   rewritten.

3. **PAPK manifest**: `papk-pack` writes the active version (or
   `0.0.0` when shrinking is off) into the `framework-map-version`
   manifest key.

4. **Load time**: `src/app.rs` calls `papk.verify_compat(FRAMEWORK_MAP_VERSION)`
   right after parsing. A PAPK built with mismatched shrink settings
   (one side `0.0.0`, the other non-zero) is rejected with a hard
   error asking to rebuild.

## Compatibility rules

`verify_compat` accepts these combinations and rejects all others:

| Firmware    | PAPK        | Accepted? | Why |
|-------------|-------------|-----------|-----|
| `0.0.0`     | `0.0.0`     | Yes       | Both unshrunk, names match. |
| `v` (≥1)    | `v'` (≥1) and `v' ≤ v` | Yes | Append-only maps: every shrunk name the PAPK uses is still present in firmware. |
| `v` (≥1)    | `v'` (≥1) and `v' > v` | No (`FrameworkVersionMismatch`) | PAPK may reference shrunk names added after firmware's release. |
| `0.0.0`     | non-zero    | No        | PAPK's shrunk refs don't exist in unshrunk firmware. |
| non-zero    | `0.0.0`     | No        | PAPK's original refs don't exist in shrunk firmware. |
| anything    | unversioned (legacy, pre-M1) | Only if firmware is `0.0.0` (`FrameworkVersionMissing` otherwise) | Backward compat. |

## Native dispatch — reverse translation

Every `(class, method)` match arm in `src/system/native_handler/**`
uses the **original** internal names (e.g. `"picodroid/pio/Gpio"`). At
each dispatcher's entry we call `crate::shrink_names::unshrink_class`
once, so the incoming shrunk name is translated back to the original
literal before the match runs:

```rust
pub fn dispatch(class_name: &str, method_name: &str, ctx: &mut NativeContext<'_>)
    -> Option<Result<Option<Value>, JvmError>>
{
    let class_name = crate::shrink_names::unshrink_class(class_name);
    match (class_name, method_name) {
        ("picodroid/pio/Gpio", "setValue") => ...,
        // ...
    }
}
```

`unshrink_class` is code-generated by `build_support/papk.rs` as a
`match` on `&'static str`. When no map is active it's an identity
passthrough — zero cost beyond one function call.

## Keep list

`sdk/keep.toml` declares names the shrinker must never touch. In v1:

- `java/**` (glob): pico-jvm's built-in handler hardcodes these names,
  and every PAPK refers to them literally.
- `picodroid/annotation/KeepName` (exact): the annotation class used
  by future method/field keeps in Java source.

Add an entry here before adding new framework surface that Rust
references by name in a way the reverse-translation layer can't cover.

## Cutting a release

Update `sdk/shrink-maps/` whenever you bump the picodroid package
version:

```bash
# Fresh-compile the framework to a scratch dir.
TMP=$(mktemp -d)
find sdk/java -name '*.java' -print0 \
  | xargs -0 javac --release 8 -Xlint:-options -d "$TMP"

# Generate the map. --base copies the previous release verbatim so the
# append-only invariant is enforced automatically.
cargo run -p class-shrink -- cut-release \
  --classes-dir "$TMP" \
  --keep sdk/keep.toml \
  --base sdk/shrink-maps/v<previous>.toml \
  --out  sdk/shrink-maps/v<new>.toml

# Commit both the map and the Cargo.toml version bump in the same commit.
```

From that commit onwards, `build.rs` and `scripts/build-apk.sh` pick
up the new map automatically.

## What's committed

- `tools/class-shrink/` — the shrinker binary and library.
- `sdk/keep.toml` — keep list.
- `sdk/shrink-maps/v*.toml` — one file per release, immutable.
- `src/shrink_names.rs` — one-line module that `include!`s the generated
  `unshrink_class` function from `OUT_DIR`.

## What's generated at build time (OUT_DIR)

Always emitted:
- `framework_mapping_version.rs` — `pub const FRAMEWORK_MAP_VERSION: &str = "…";`
  (`"0.0.0"` when shrinking is off).
- `framework_unshrink.rs` — `unshrink_class(name) -> &str`. Identity
  passthrough when shrinking is off; a reverse-lookup match when on.
- `framework_classes.rs` — `pub static FRAMEWORK_CLASSES: &[&[u8]] = &[…];`
  pointing at (shrunk or raw) class files.

Emitted only when shrinking is on and a map is active:
- `framework_classes_shrunk/…` — shrunk class files.

## CI coverage

Both [scripts/sim-run.sh](../scripts/sim-run.sh) and
[scripts/hil-run.sh](../scripts/hil-run.sh) run the full test matrix
twice — once with shrinking off, once with it on. Each result is
tagged with `[no-shrink]` or `[shrink]` so regressions on either side
are obvious. Pass `--mode no-shrink`, `--mode shrink`, or `--mode both`
(default) to narrow the run.

The HIL suite also exercises rejection paths — three test rows per mode
(see [scripts/hil-tests.conf](../scripts/hil-tests.conf)):

| Row                            | What it tests |
|--------------------------------|---------------|
| `install-reject-host`          | Build a PAPK in the OPPOSITE shrink mode of the firmware; assert `pdb` refuses pre-flight and the device still PINGs after. |
| `install-reject-device`        | Same as above but with `--skip-host-check`; assert the device returns `STATUS_INCOMPAT` in Phase A and stays alive. |
| `install-reject-future`        | Synthesize a future map (`v0.<MIN+1>.0.toml`) via [scripts/test-future-version-rejection.sh](../scripts/test-future-version-rejection.sh), build a PAPK against it, assert rejection. Only meaningful in shrink mode. |

After every rejection, `hil-run.sh` runs a `pdb ping` to confirm the
device is responsive — a successful rejection must not have erased flash
or rebooted.

## `pdb install` pre-flight

`pdb install` has two compatibility gates so a bad install never reboots
the device:

1. **Host pre-flight** in [tools/pdb/src/install.rs](../tools/pdb/src/install.rs):
   after PING, before sending the install header, parse the PAPK manifest
   for `framework-map-version`, compare to the firmware's version learned
   from the new PING greeting, and exit with a clear error if `compat::check`
   rejects.
2. **Device-side check** in [src/packagemanager/install.rs](../src/packagemanager/install.rs):
   after parking core 0 but before erasing flash, peek the first
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
