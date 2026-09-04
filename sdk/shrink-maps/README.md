# Shrink maps

Committed, append-only mappings from original Java class/method/field names
to their shortened forms. Each file `v<semver>.toml` is tied to a **released**
picodroid version and is immutable once merged.

## How the active map is resolved

Shrinking is **off by default**. Pass `--shrink` to the top-level scripts
(`build.sh`, `flash.sh`, `sim.sh`, `build-apk.sh`) or set
`PICODROID_SHRINK=1` to turn it on for a build. Both firmware (`build.rs`)
and PAPK builds honor the same env var, so the two always agree.

When shrinking is on, tooling reads the `version` field of the root
`Cargo.toml` and picks the **highest** committed map file whose semver is
≤ that version. If none exists, the active map version falls back to the
`0.0.0` sentinel and nothing is rewritten.

`class-shrink print-version` performs this resolution. It's invoked by
`build.rs` and `scripts/build-apk.sh` only when `PICODROID_SHRINK=1`.

## Append-only rule

Cutting a new release (M3's `class-shrink cut-release --version <x.y.z>`
command) must:

1. Copy every entry from the previous release map verbatim. **Never rename
   an existing entry.** This is what lets old PAPKs keep running on newer
   firmware.
2. Allocate new short names for symbols introduced since the previous
   release, continuing the deterministic allocator from where the previous
   release left off.
3. Write the result to `v<new-version>.toml` and commit it together with
   the `Cargo.toml` version bump.

Anything added to the framework between releases stays un-shrunk (full
names in `.class` files) until the next release folds it in. This keeps
the release→map relationship one-to-one and avoids churn on every commit.

## App maps are build outputs, not release maps

A release map never carries `c/` rows. `--shrink-app` (`build-apk.sh`)
cuts a **per-PAPK** map at build time — `class-shrink cut-app` copies the
active release map and appends the app's own classes under `c/` and its
private member names, resuming the release allocator so no target
collides. That merged file lands next to the PAPK
(`build/apks/<app>.shrink-map.toml`), is the PAPK's retrace key, and is
regenerated on every build; nothing under `sdk/shrink-maps/` changes.

## Versioning & PAPK compatibility

Each PAPK stores `framework-map-version` in its manifest. At load time the
firmware rejects a PAPK whose map version is greater than the firmware's
active version (a PAPK built against a newer release cannot run on older
firmware). Equal-or-lower is accepted, because the append-only rule
guarantees every name the PAPK uses is still present — with one floor:
a map that renames names an older map spelled verbatim moves
`compat::MEMBER_SHRINK_FLOOR` (0.16.0 for the first member map, 0.17.0
when the `java/**` contract members joined it), and firmware at or past
the floor rejects a shrunk PAPK cut before it. Un-keeping a member later
would need another floor.

## Cutting a release

Use the `class-shrink` tool. From the repo root:

```bash
# Fresh-compile the framework to a scratch dir.
TMP=$(mktemp -d)
find sdk/java -name '*.java' -print0 \
  | xargs -0 javac --release 8 -Xlint:-options -d "$TMP"

# The kotlin-shim's member names must never become member targets.
./gradlew :kotlin-shim:compileJava -q

# Generate the map. Pass --base <previous-release-map> to enforce
# append-only: existing entries are copied verbatim and only net-new
# classes get fresh short names. --extra-names feeds the java/** names
# the framework never references itself (RuntimeException, Iterator, …)
# from the committed list of everything pico-jvm serves. --members maps
# method/field names too: everything the SDK declares plus the
# --contract member column (every java/** member the runtime serves —
# toString, equals, hasNext, …; since v0.17.0 they are mapped, not
# kept), every name the --reserve tree spells is never a target, and
# --version becomes member-floor on the first member cut. Add --floor
# only for a cut that renames names the previous map left verbatim —
# it re-bases the floor and every older shrunk PAPK stops loading.
cargo run -p class-shrink -- cut-release --members \
  --classes-dir "$TMP" \
  --keep sdk/keep.toml \
  --extra-names sdk/api-contract.tsv \
  --contract sdk/api-contract.tsv \
  --reserve kotlin-shim/build/classes/java/main \
  --base sdk/shrink-maps/v<previous>.toml \
  --version <new> \
  --out  sdk/shrink-maps/v<new>.toml
```

Then bump the `version` field in `platforms/rp/Cargo.toml` (the root
`Cargo.toml` is a virtual workspace) and commit both files together. From
that commit onwards, both `build.rs` and `scripts/build-apk.sh`
automatically pick up the new map.

## Namespaces

Shrunk names live in three synthetic packages, each allocated from its own
counter so the suffix sequences never collide:

| Prefix | Holds |
|---|---|
| `a/` | framework classes (`picodroid/**`, `javax/**`) |
| `b/` | `java/**` classes pico-jvm serves natively — the ones defined in `sdk/java`, every one the framework references, and every owner in `sdk/api-contract.tsv` |
| `c/` | an app's own classes — only in the per-app map `--shrink-app` cuts at build time (see [App maps are build outputs](#app-maps-are-build-outputs-not-release-maps)); a release map never has one |

Nothing translates either prefix at run time. The Rust side names every
class, member and descriptor through constants generated from the active
map (`build_support/names.rs`: `c::picodroid_view_View`, `m::toString`,
`d::String__V`), so a `--shrink` firmware's tables, `catch` matching,
`instanceof`, native dispatch and `Class.getName()` all use the mapped
spelling and the image carries no original name — ProGuard semantics.
Build without `--shrink` for readable names, or pipe a shrunk log through
`scripts/retrace.sh`. All three prefixes are reserved: `cut-app` rejects an
app class in package `a`, `b` or `c`.

## Current releases

| Map | Notes |
|---|---|
| `v0.1.0.toml` | First release cut — 42 framework classes outside `java/**`. |
| `v0.2.0.toml` | + `Executors` family, `SensorManager` family, HTTP client, `KeyEvent` / `OnKeyListener`. |
| `v0.3.0.toml` | + `Theme`, drawables, gesture / animation surface, dialog / keyboard widgets. |
| `v0.4.0.toml` | + Service / DI surface (`Service`, `IBinder`, `Notification`, `ServiceConnection`, manual DI components). |
| `v0.5.0.toml` | + Soft-keyboard polish (`OnEditorActionListener`, `EditorInfo`). |
| `v0.6.0.toml` | Stable — byte-identical to v0.5.0 (`picoenvmon` + LTR559 added no framework classes). |
| `v0.7.0.toml` | + Tier C widgets (`Snackbar`, `DatePicker`, `TimePicker`, `SwipeRefreshLayout`, `OnSwipeListener`). |
| `v0.8.0.toml` | Stable — byte-identical to v0.7.0 (PAPK 1.1 bundled assets land outside the framework). |
| `v0.9.0.toml` | Stable — byte-identical to v0.8.0 (relicense, multi-family refactor, ESP32-S3 M1, Display singleton bootstrap). |
| `v0.10.0.toml` | + 23 classes (87 → 110): Android-parity Tier 1/2 typed-listener interfaces, the `Adapter` pattern (`Adapter`, `AdapterView`, `ArrayAdapter`, `BaseAdapter`), `ViewGroup` / `LayoutParams`, `CompoundButton`, and `DialogInterface` / `DisplayDebug`. v0.9.0 entries copied verbatim. |
| `v0.11.0.toml` | + 25 classes (110 → 135): `AlertDialog` moved to `picodroid.app`, `SharedPreferences`, `IBinder` moved to `picodroid.os`, `URL` / `HttpURLConnection` renamed to Java casing, `TextWatcher`, `Gravity`, `InputType`, `GestureDetector.SimpleOnGestureListener`, the animation interpolator family, `NumberPicker`, and `RadioButton` / `RadioGroup`. v0.10.0 entries copied verbatim. |
| `v0.12.0.toml` | Stable — byte-identical to v0.11.0 (the Pico 2 W networking bring-up, FreeRTOS host sim, and crate extractions added no framework classes). |
| `v0.13.0.toml` | Stable — byte-identical to v0.12.0 (the networking-maturity, JVM-correctness, and memory work extended existing classes rather than adding new ones). |
| `v0.14.0.toml` | + 14 classes (135 → 149): the `java.util.concurrent` core set (`Callable`, `Future`, `FutureTask`, `ExecutorService`, `ThreadPoolExecutor`, `TimeUnit`, `CountDownLatch`, the four `Atomic*` types), `Thread.UncaughtExceptionHandler`, and the DI injection points `javax.inject.Provider` / `picodroid.di.Lazy`. v0.13.0 entries copied verbatim. |
| `v0.15.0.toml` | + 88 `java/**` classes under the new `b/` namespace (149 → 237): everything the framework references or pico-jvm serves — `Object`, `String`, `StringBuilder`, the boxed types, the collection classes and interfaces, every builtin exception, the `java.lang.invoke` bootstrap names. The 149 `a/` entries copied verbatim; `a/` allocation is untouched. |
| `v0.16.0.toml` | Schema 2: + 868 `[[member]]` rows — every method and field name the framework declares, keyed by bare name; `member-floor = 0.16.0`. Classes unchanged (238). |
| `v0.17.0.toml` | + 125 members (868 → 993): the `java/**` contract members the runtime serves (`toString`, `hashCode`, `equals`, `hasNext`, …) and javac's `$` synthetics, previously kept; `member-floor` re-based to 0.17.0. Only `main` and `injectMembers` stay verbatim. Classes unchanged. |
| `v0.18.0.toml` | + 1 class (238 → 239): `java/util/Objects`; + 14 members (993 → 1007): the Tier 1 fills — `getFloat` / `putFloat`, `DIRECTION_IN`, `createNewFile` / `mkdirs` / `getParent` / `getParentFile` / `getAbsolutePath`, `hash` / `isNull` / `nonNull` / `requireNonNull`, `intBitsToFloat`, `T_FLOAT`. `member-floor` stays 0.17.0. |
| `v0.19.0.toml` | + 1 class (239 → 240): `picodroid/net/ConnectivityManager`; + 4 members (1007 → 1011): `TYPE_NONE` / `TYPE_WIFI` / `TYPE_ETHERNET` / `FEATURE_ETHERNET` (`getType` was already a target). Member floor unchanged (0.17.0). |

See [`reference/shrinker`](https://shivrajora.github.io/picodroid-rs/reference/shrinker/) for the full design and per-release detail.
