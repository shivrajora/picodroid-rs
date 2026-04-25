# class-shrink

Build-time Java class/method name shrinker for picodroid.

Used by `picodroid`'s `build.rs` to apply the active release shrink map to compiled framework `.class` files before they're embedded in firmware. Maps live at `sdk/shrink-maps/v<semver>.toml` in the parent repo and are **release-versioned** and **append-only**: once a release ships its map, that map is frozen; new symbols added between releases are not shrunk until the next release folds them in.

The append-only invariant is what makes cross-version compatibility predictable: an old PAPK runs on new firmware as long as the firmware's map version ≥ the PAPK's map version.

## Usage as a library

```toml
[build-dependencies]
class-shrink = { path = "tools/class-shrink" }
```

```rust
use class_shrink::{mapping, shrink};

let map = mapping::load_from_toml("sdk/shrink-maps/v0.1.0.toml")?;
let rewritten = shrink::rewrite(original_class_bytes, &map)?;
```

## Usage as a CLI

```bash
# Print the active map version (semver or "0.0.0" sentinel)
class-shrink print-version --cargo-toml Cargo.toml --shrink-maps-dir sdk/shrink-maps

# Cut a new release map covering every non-kept class under <dir>
class-shrink cut-release \
    --classes-dir build/classes \
    --keep sdk/keep.toml \
    --out sdk/shrink-maps/v0.2.0.toml \
    --base sdk/shrink-maps/v0.1.0.toml

# Rewrite every .class file under --in using --map's classes
class-shrink shrink-dir \
    --in build/classes \
    --out build/classes-shrunk \
    --map sdk/shrink-maps/v0.1.0.toml
```

See `docs/shrinker.md` in the parent repo for the full map format and design.

## Status

Internal to the picodroid-rs project. Not published to crates.io.

## License

Apache-2.0
