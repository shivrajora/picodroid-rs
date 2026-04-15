# Shrink maps

Committed, append-only mappings from original Java class/method/field names
to their shortened forms. Each file `v<semver>.toml` is tied to a **released**
picodroid version and is immutable once merged.

## How the active map is resolved

On every firmware and PAPK build, tooling reads the `version` field of the
root `Cargo.toml` and picks the **highest** committed map file whose semver
is ≤ that version. If none exists (no releases have been cut), the active
map version is the sentinel `0.0.0`, which means **no shrinking**.

`class-shrink print-version` performs this resolution. Both `build.rs` and
`scripts/build-apk.sh` call it, so firmware and PAPKs always agree on the
active version for a given commit.

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

## Versioning & PAPK compatibility

Each PAPK stores `framework-map-version` in its manifest. At load time the
firmware rejects a PAPK whose map version is greater than the firmware's
active version (a PAPK built against a newer release cannot run on older
firmware). Equal-or-lower is accepted, because the append-only rule
guarantees every name the PAPK uses is still present.

## Cutting a release

Use the `class-shrink` tool. From the repo root:

```bash
# Fresh-compile the framework to a scratch dir.
TMP=$(mktemp -d)
find sdk/java -name '*.java' -print0 \
  | xargs -0 javac --release 8 -Xlint:-options -d "$TMP"

# Generate the map. Pass --base <previous-release-map> to enforce
# append-only: existing entries are copied verbatim and only net-new
# classes get fresh short names.
cargo run -p class-shrink -- cut-release \
  --classes-dir "$TMP" \
  --keep sdk/keep.toml \
  --base sdk/shrink-maps/v<previous>.toml \
  --out  sdk/shrink-maps/v<new>.toml
```

Then bump the `version` field in the root `Cargo.toml` and commit both
files together. From that commit onwards, both `build.rs` and
`scripts/build-apk.sh` automatically pick up the new map.

## Current releases

- `v0.1.0.toml` — first release cut. Shrinks all 42 framework classes
  outside `java/**` (the pico-jvm built-in handler hardcodes those names,
  so they stay literal).
