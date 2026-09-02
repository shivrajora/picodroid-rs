# class-shrink

Build-time Java class/method name shrinker for picodroid.

Used by `picodroid`'s `build.rs` to apply the active release shrink map to compiled framework `.class` files before they're embedded in firmware. Maps live at `sdk/shrink-maps/v<semver>.toml` in the parent repo and are **release-versioned** and **append-only**: once a release ships its map, that map is frozen; new symbols added between releases are not shrunk until the next release folds them in.

The append-only invariant is what makes cross-version compatibility predictable: an old PAPK runs on new firmware as long as the firmware's map version ≥ the PAPK's map version.

Maps carry class names (`[[class]]`, `a/` framework and `b/` `java/**`
namespaces) and, since schema 2 / v0.16.0, member names (`[[member]]`,
owner-agnostic). This tool rewrites class names and *allocates* member
names; the member rewrite itself is the Gradle-side ASM pass
(`buildSrc/.../ShrinkMembersTask.kt`), which rebuilds the constant pool.

## Usage as a library

```toml
[build-dependencies]
class-shrink = { path = "tools/class-shrink" }
```

```rust
use class_shrink::{mapping::ShrinkMap, shrink};

let map = ShrinkMap::load(Path::new("sdk/shrink-maps/v0.16.0.toml"))?;
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
    --keep-contract sdk/api-contract.tsv \
    --reserve kotlin-shim/build/classes/java/main \
    --base sdk/shrink-maps/v0.15.0.toml \
    --version 0.16.0 \
    --out sdk/shrink-maps/v0.16.0.toml

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
