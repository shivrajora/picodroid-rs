# class-shrink

Build-time Java class/method name shrinker for picodroid.

Used by `picodroid`'s `build.rs` to apply the active release shrink map to compiled framework `.class` files before they're embedded in firmware. Maps live at `sdk/shrink-maps/v<semver>.toml` in the parent repo and are **release-versioned** and **append-only**: once a release ships its map, that map is frozen; new symbols added between releases are not shrunk until the next release folds them in.

The append-only invariant is what makes cross-version compatibility predictable: an old PAPK runs on new firmware as long as the firmware's map version ≥ the PAPK's map version.

Maps carry class names (`[[class]]`: `a/` for framework classes, `b/` for
`java/**`, and — only in a per-app map — `c/` for an app's own classes) and,
since schema 2 / v0.16.0, member names (`[[member]]`,
owner-agnostic; since v0.17.0 every name the runtime serves, the `java/**`
contract included). This tool rewrites class names and *allocates* member
names; the member rewrite itself is the Gradle-side ASM pass
(`buildSrc/.../ShrinkMembersTask.kt`), which rebuilds the constant pool.
Nothing in firmware translates a mapped name back: the Rust runtime is
compiled against constants generated from the same map
(`build_support/names.rs`), and `retrace` is the host-side inverse for
logs.

A release map never contains `c/` rows. `cut-app` (`build-apk.sh
--shrink-app`) copies the active release map and appends one app's classes
and private member names, continuing the release allocator; the result is a
per-PAPK build output (`build/apks/<app>.shrink-map.toml`), never a
versioned file.

## Usage as a library

```toml
[build-dependencies]
class-shrink = { path = "tools/class-shrink" }
```

```rust
use class_shrink::{mapping::ShrinkMap, shrink};

let map = ShrinkMap::load(Path::new("sdk/shrink-maps/v0.18.0.toml"))?;
shrink::shrink_directory(&classes_in, &classes_out, &map)?;
let short = map.member_target("setText"); // Some("uQ") under a member map
```

## Usage as a CLI

```bash
# Print the active map version (semver or "0.0.0" sentinel)
class-shrink print-version --cargo-toml Cargo.toml --shrink-maps-dir sdk/shrink-maps

# Cut a new release map: every non-kept class under <dir> (a/), the java/**
# names it references (b/), and — with --members — its method/field names
class-shrink cut-release --members \
    --classes-dir build/classes \
    --keep sdk/keep.toml \
    --extra-names sdk/api-contract.tsv \
    --contract sdk/api-contract.tsv \
    --reserve kotlin-shim/build/classes/java/main \
    --base sdk/shrink-maps/v0.17.0.toml \
    --version 0.18.0 \
    --out sdk/shrink-maps/v0.18.0.toml

# Cut one app's map on top of the release map (what build-apk.sh --shrink-app
# runs through the Gradle cutAppShrinkMap task): the app's classes under c/,
# its private members, every SDK and contract name reserved
class-shrink cut-app \
    --classes-dir examples/foo/build/classes-stripped \
    --base sdk/shrink-maps/v0.18.0.toml \
    --keep sdk/keep.toml \
    --reserve-names sdk/member-names.tsv \
    --reserve-names sdk/api-contract.tsv \
    --out build/apks/foo.shrink-map.toml

# Read a --shrink firmware's log with original names
./scripts/sim.sh --app foo --shrink 2>&1 | class-shrink retrace --map sdk/shrink-maps/v0.18.0.toml
# ... an app-shrunk PAPK needs its own merged map
./scripts/sim.sh --app foo --shrink --shrink-app 2>&1 | class-shrink retrace --map build/apks/foo.shrink-map.toml
# ... and --classes trees turn release firmware's `Class.method(pc=N)` frames
# into `Class.method(File.java:LINE)` (scripts/retrace.sh wraps all of this)
class-shrink retrace --classes sdk/build/classes/java/main --classes examples/foo/build/classes < rtt.log

# Rewrite every .class file under --in using --map's classes
class-shrink shrink-dir \
    --in build/classes \
    --out build/classes-shrunk \
    --map sdk/shrink-maps/v0.1.0.toml
```

See `website/src/content/docs/reference/shrinker.md` in the parent repo for the full map format and design.

## Status

Internal to the picodroid-rs project. Not published to crates.io.

## License

GPL-3.0-only. See [LICENSE](https://github.com/shivrajora/picodroid-rs/blob/main/LICENSE) in the repo root.
