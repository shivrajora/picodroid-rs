# Design: Extract the shared no_std papk-format crate (audit P1-5)

> Produced 2026-07-25 by the audit fix session's design panel (4 parallel
> design agents, each adversarially critiqued against source; the critique's
> verdict and amendments are at the bottom and OVERRIDE the design body where
> they conflict). Execute from this doc; update it if reality diverges.

## DESIGN
# Design: shared `papk-format` crate extraction

## 0. Ground truth (what exists today)

The PAPK v1.x container (24-byte file header, 16-byte section headers, MANI/CLSS/ASST TLV sections) is independently re-derived at **six** sites, not five — the audit missed one:

| # | Site | Role | Tested? |
|---|------|------|---------|
| 1 | `jvm/src/apk.rs` (921 ln) | authoritative zero-copy `no_std`/no-alloc runtime parser; `Papk::verify_compat` delegates to `compat::check`; ASSETS (LVGL pixel) section support | yes, extensively |
| 2 | `tools/papk-pack/src/main.rs` | writer — header consts at :20-31, `build_manifest_data` / `build_classes_data` / `build_assets_data` / `build_section_header` / `build_papk` (:441-522) | no |
| 3 | `tools/papk-info/src/main.rs` :7-190 | full reader (header/manifest/classes/assets) | format code untested (only `fmt_size`/`tag_name`) |
| 4 | `tools/pdb/src/papk_meta.rs` | manifest-only reader: `validate_structure` + `read_framework_map_version` | yes |
| 5 | `build_support/papk.rs` | build-time embedder (`embed_apk`, `embed_papk_flash_init`); `#[path]`-included by `platforms/rp/build.rs` **and** `picodroid-core/build.rs`; treats `.papk` as opaque bytes, but owns the PDB1 flash-init sector layout (duplicated again in `hal/rp/flash.rs:19` and `hal/sim/flash.rs:8`) | no |
| 6 | `platforms/rp/src/packagemanager/install.rs:226` | `extract_framework_map_version` — **prefix-tolerant** manifest peek over a partial wire buffer (deliberately accepts a manifest whose tail is beyond the peeked bytes) | yes |

Consumers of `jvm::apk` (`rg 'apk::'`): only `platforms/rp/src/app.rs` (parse, `classes()`, entry-point lookup, `verify_compat(FRAMEWORK_MAP_VERSION)`) and `platforms/rp/src/system/picodroid/graphics/assets.rs` (parse, `assets()`). No pico-jvm-internal module uses `apk` — it is a pure leaf, which is exactly why it (and the `compat` dep it drags in) is jvm's "reusability leak" (audit §5/§6.1).

`compat` is already `#![no_std]`, dep-free, and shared by jvm / pdb / platforms-rp / picodroid-core.

papk-pack itself anticipates this crate: `main.rs:450` — "Argument bundling is deferred to the planned shared papk-format crate (docs/code-health-audit-2026-07.md §6.1), which owns the manifest shape."

## (a) Crate location, name, no_std strategy

- **Location:** `papk-format/` at repo root, sibling of `compat/` (the established precedent for a shared no_std leaf consumed by both device and host). Add to `[workspace] members` in the root `Cargo.toml`.
- **Names:** package `papk-format`, lib `papk_format`. Version `0.1.0`, `license = "GPL-3.0-only"`, `edition = "2021"`.
- **no_std strategy:**
  - Crate root: `#![no_std]`. The parser (everything moved from `jvm/src/apk.rs` + the `papk_meta.rs` / `install.rs` scanners) is **core-only — not even alloc** (it already is today: iterators over `&'a [u8]`). This keeps the firmware constraint trivially satisfied and is stricter than the required "no_std+alloc".
  - Writer is gated: `[features] write = []`, with `#[cfg(feature = "write")] extern crate alloc;` and the builder module behind `#[cfg(feature = "write")]`. The writer needs only `alloc` (`Vec`/`String`), never `std` — so it stays usable from any host or build-script context without a `std` feature at all. `default = []` (parser only).
  - Error types implement `core::fmt::Display` (pdb/papk-info need printable errors; `StructuralError` already only uses `std::fmt` cosmetically — port to `core::fmt`).

```toml
# papk-format/Cargo.toml
[package]
name = "papk-format"
version = "0.1.0"
edition = "2021"
license = "GPL-3.0-only"
description = "PAPK (Picodroid APK) container format: no_std zero-copy parser, alloc-gated writer. Single source of truth for the on-disk layout."

[dependencies]
compat = { path = "../compat" }

[features]
default = []
write = []          # enables the alloc-based builder

[dev-dependencies]
proptest = "1"      # host-only; round-trip + no-panic property tests
```

`compat` stays a **separate crate** (it is a policy rule, not a format; pdb and packagemanager call `compat::check` directly on wire-provided versions with no `Papk` in hand). `papk-format -> compat` is a no_std->no_std edge and preserves the `Papk::verify_compat` behavior verbatim. (Alternative considered and rejected: an optional `compat-check` feature — adds cfg noise for zero firmware savings since every real consumer wants it.)

The format doc-comment (the ASCII layout block at the top of `jvm/src/apk.rs`) moves to `papk-format/src/lib.rs` and becomes the single format spec, per the audit's "parser + writer + the format doc".

## (b) Public API

```rust
// ── papk_format (crate root, no_std, core-only) ──────────────────────────

pub const MAGIC: &[u8; 4] = b"PAPK";
pub const SUPPORTED_VERSION_MAJOR: u16 = 1;   // parser bound
pub const VERSION_MAJOR: u16 = 1;             // writer emits
pub const VERSION_MINOR: u16 = 1;             // writer emits (1 since framework-map-version)
pub const FILE_HEADER_LEN: usize = 24;
pub const SECTION_HEADER_LEN: usize = 16;
pub const TAG_MANIFEST: u32 = u32::from_le_bytes(*b"MANI");
pub const TAG_CLASSES:  u32 = u32::from_le_bytes(*b"CLSS");
pub const TAG_ASSETS:   u32 = u32::from_le_bytes(*b"ASST");

/// Well-known manifest keys — ends the b"framework-map-version" string
/// literals scattered across four crates.
pub mod keys {
    pub const MAIN_CLASS: &[u8] = b"main-class";
    pub const ACTIVITY: &[u8] = b"activity";
    pub const APPLICATION: &[u8] = b"application";
    pub const PACKAGE_NAME: &[u8] = b"package-name";
    pub const VERSION: &[u8] = b"version";
    pub const FRAMEWORK_MAP_VERSION: &[u8] = b"framework-map-version";
}

/// Moved verbatim from jvm/src/apk.rs (same variants, same semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PapkError { BadMagic, UnsupportedVersion, Truncated, MissingSection,
                     FrameworkVersionMissing, FrameworkVersionMismatch }
impl core::fmt::Display for PapkError { /* new, additive */ }

/// Raw file header — needed by papk-info's dump view (today's private
/// FileHeader in papk-info/src/main.rs:44-52 becomes this).
#[derive(Debug, Clone, Copy)]
pub struct FileHeader {
    pub version_major: u16, pub version_minor: u16, pub section_count: u32,
    pub manifest_offset: u32, pub classes_offset: u32, pub assets_offset: u32, // 0 = absent
}
impl FileHeader {
    /// Magic + length check only — does NOT enforce version_major, so
    /// papk-info can still dump a future-versioned file's header.
    pub fn parse(data: &[u8]) -> Result<Self, PapkError>;
}

#[derive(Debug, Clone, Copy)]
pub struct SectionHeader { pub tag: u32, pub length: u32, pub crc32: u32 }

// ── The zero-copy parser: Papk<'a>, moved verbatim from jvm/src/apk.rs ──
pub struct Papk<'a> { /* data + three offsets, as today */ }
impl<'a> Papk<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, PapkError>;          // unchanged
    pub fn manifest(&self) -> Result<ManifestIter<'a>, PapkError>;    // unchanged
    pub fn classes(&self) -> Result<ClassIter<'a>, PapkError>;        // unchanged
    pub fn assets(&self) -> Result<Option<AssetIter<'a>>, PapkError>; // unchanged
    pub fn main_class(&self) -> Option<&'a str>;                      // unchanged
    pub fn activity(&self) -> Option<&'a str>;                        // unchanged
    pub fn application(&self) -> Option<&'a str>;                     // unchanged
    pub fn framework_map_version(&self) -> Option<&'a str>;           // unchanged
    pub fn verify_compat(&self, firmware_version: &str) -> Result<(), PapkError>; // unchanged
    /// Promoted from private: generic manifest lookup (papk-info/others).
    pub fn manifest_value(&self, key: &[u8]) -> Option<&'a str>;
    /// New, additive — papk-info needs per-section header + data for its
    /// "(N bytes @ 0xOFF) tag \"MANI\"" lines.
    pub fn manifest_section(&self) -> Result<(SectionHeader, &'a [u8]), PapkError>;
    pub fn classes_section(&self) -> Result<(SectionHeader, &'a [u8]), PapkError>;
    pub fn assets_section(&self) -> Result<Option<(SectionHeader, &'a [u8])>, PapkError>;
    pub fn file_header(&self) -> FileHeader;
}

// Iterators + entries, moved verbatim:
pub struct ManifestIter<'a>; pub struct ManifestEntry<'a> { pub key: &'a [u8], pub value: &'a [u8] }
pub struct ClassIter<'a>;    pub struct ClassEntry<'a>    { pub name: &'a [u8], pub data: &'a [u8] }
pub struct AssetIter<'a>;    pub struct AssetEntry<'a>    { pub name: &'a [u8], pub width: u16,
                                 pub height: u16, pub cf: u8, pub stride: u16, pub data: &'a [u8] }

// ── Streaming / pre-flight scanners (mod scan) ──────────────────────────
// Moved from tools/pdb/src/papk_meta.rs, byte-for-byte semantics:
pub enum StructuralError { TooShort, BadMagic, ManifestOutOfBounds,
                           ManifestBadMagic, ManifestMalformed }  // + core Display
pub fn validate_structure(bytes: &[u8]) -> Result<(), StructuralError>;

/// STRICT whole-file scan (pdb pre-flight semantics): any TLV walking past
/// the buffer => None. Generalization of read_framework_map_version.
pub fn find_manifest_value<'a>(bytes: &'a [u8], key: &[u8]) -> Option<&'a str>;

/// PREFIX-TOLERANT scan (device install-peek semantics, moved from
/// platforms/rp/src/packagemanager/install.rs:226): the buffer may be a
/// prefix of the file; a manifest section extending past the buffer is
/// walked only as far as buffered. These two MUST remain distinct
/// functions — merging them re-opens the pdb "100-byte stub accepted"
/// regression or breaks the device peek, respectively.
pub fn find_manifest_value_in_prefix<'a>(prefix: &'a [u8], key: &[u8]) -> Option<&'a str>;

// ── Writer (mod write, cfg(feature = "write"), alloc-only) ──────────────
pub enum EntryPoint<'a> { MainClass(&'a str), Activity(&'a str), Application(&'a str) }

/// Owns "the manifest shape" (resolves papk-pack's deferred
/// too_many_arguments note at main.rs:450-452).
pub struct ManifestSpec<'a> {
    pub entry: EntryPoint<'a>,
    pub package_name: &'a str,
    pub version: &'a str,
    pub framework_map_version: &'a str,
}

pub struct AssetSpec<'a> {
    pub name: &'a str, pub width: u16, pub height: u16,
    pub cf: u8,        // opaque LVGL color-format byte; papk-format does NOT
                       // know LVGL — LV_COLOR_FORMAT_RGB565 stays in papk-pack,
                       // cf_label stays in papk-info (see risk on drift)
    pub stride: u16,   // papk-pack currently always writes 0
    pub data: &'a [u8],
}

pub enum BuildError { NameTooLong, ValueTooLong, TooManyEntries } // + Display

pub struct PapkBuilder<'a> { /* manifest, classes, assets */ }
impl<'a> PapkBuilder<'a> {
    pub fn new(manifest: ManifestSpec<'a>) -> Self;
    pub fn manifest_entry(&mut self, key: &str, value: &str) -> &mut Self; // future keys; appended after the fixed four
    pub fn class(&mut self, jvm_name: &str, bytes: &'a [u8]) -> &mut Self;
    pub fn asset(&mut self, asset: AssetSpec<'a>) -> &mut Self;
    /// Emission is byte-identical to today's papk-pack build_papk():
    /// header(24) [major=1, minor=1 always] · MANI hdr+data · CLSS hdr+data ·
    /// ASST hdr+data only when >=1 asset (else assets_offset=0, section_count=2);
    /// manifest key order: entry key, package-name, version,
    /// framework-map-version, then extras; asset records padded to 4-byte
    /// boundaries before data and before the next record; crc32/reserved = 0.
    pub fn build(&self) -> Result<Vec<u8>, BuildError>;
}
```

`BuildError` length validation (name > u16::MAX etc.) replaces today's silent `as u16`/`as u32` truncation. For every input papk-pack can produce today this is byte-identical output; it only turns a latent corrupt-output path into an error. Flagged in risks.

## (c) Per-consumer migration

**Re-export question, answered up front: consumers import `papk_format` directly; pico-jvm does NOT re-export.** A `pub use papk_format as apk;` shim would keep papk-format (and transitively compat) in jvm's dependency graph and public API — the exact leak §6.1 says this change should fix. There are only two importing files (both in platforms/rp), so the churn a re-export would save is two `use` lines.

Order of operations (each step leaves the tree green):

1. **Create `papk-format/`** — move `jvm/src/apk.rs` content (doc header, consts, `PapkError`, iterators, `Papk`, helpers, tests) verbatim; add `scan` module (from `papk_meta.rs` + `install.rs:226` semantics) and `write` module (from papk-pack's `build_*` fns). Add to workspace members. `./scripts/test.sh` for the new crate alone.

2. **jvm (`pico-jvm`):** delete `jvm/src/apk.rs`; remove `pub mod apk;` from `jvm/src/lib.rs:68`; update the lib.rs doc example that references `pico_jvm::apk::Papk` to point at `papk_format::Papk`. Removing a pub module is semver-breaking for the published-shaped crate — bump `0.1.1 -> 0.2.0`. No other jvm module touches `apk` (verified by rg), so no code changes beyond lib.rs.

3. **platforms/rp:** add `papk-format = { path = "../../papk-format" }` to `[dependencies]` (default features — parser is core-only, fine on thumbv6m).
   - `src/app.rs:5`: `use pico_jvm::apk::Papk;` -> `use papk_format::Papk;`. All call sites (`parse`, `classes`, `main_class`/`activity`/`application`, `verify_compat(FRAMEWORK_MAP_VERSION)`) compile unchanged.
   - `src/system/picodroid/graphics/assets.rs:25`: same one-line swap; `assets()` iteration unchanged. Its test fixture builders can stay hand-rolled (they pin XIP-alignment behavior) or move to `PapkBuilder` — keep hand-rolled to preserve the independent-bytes property.
   - `src/packagemanager/install.rs`: delete local `extract_framework_map_version` (:226-276) and its test module; call `papk_format::find_manifest_value_in_prefix(&peek_buf[..peeked], papk_format::keys::FRAMEWORK_MAP_VERSION)` at :100. `compat::check` call and `compat` dep stay as-is (it checks against the wire/firmware version, no `Papk` in hand). The deleted tests move into papk-format's suite — especially the "key may be in the unscanned tail -> None (not error)" case.

4. **tools/papk-pack:** dep `papk-format = { path = "../../papk-format", features = ["write"] }`. Delete consts :20-31 and `write_u16_le`/`write_u32_le`/`write_str_u16`/`build_manifest_data`/`build_classes_data`/`build_assets_data`/`build_section_header`/`build_papk` (:276-522). Keep CLI parsing, `collect_classes`, `validate_entry_point`/`classcheck`, `collect_assets`/`decode_png_to_rgb565`, and `LV_COLOR_FORMAT_RGB565` (LVGL semantic, host-only `image` dep stays out of papk-format's graph). `main()` maps `Args` -> `ManifestSpec` + `EntryPoint` (the existing "exactly one of the three" CLI check picks the variant) -> `PapkBuilder` -> `fs::write`.

5. **tools/papk-info:** dep `papk-format` (default features). Delete :7-188 (consts, read helpers, `FileHeader`, `SectionHeader`, `parse_file_header`, `parse_section_header`, `section_data`, `parse_manifest`, `parse_classes`, `parse_assets`). `run()` becomes: `FileHeader::parse` for the header block (still dumps future-minor files), then `Papk::parse` + `manifest_section()`/`classes_section()`/`assets_section()` for the "(N bytes @ 0xOFF)" lines, iterators for rows (`ClassEntry.data.len()` replaces the old `(name, size)` tuples, `AssetEntry` maps onto the table row struct). Presentation code (`cf_label`, tables, `fmt_size`, `tag_name`) stays. papk-info's untested parsing risk disappears — it now runs the tested parser.

6. **tools/pdb:** dep `papk-format` (default features). Delete `src/papk_meta.rs` and `mod papk_meta;` from main.rs. In `install.rs`: `papk_meta::validate_structure(&papk)` -> `papk_format::validate_structure(&papk)`; `papk_meta::read_framework_map_version(&papk)` -> `papk_format::find_manifest_value(&papk, papk_format::keys::FRAMEWORK_MAP_VERSION)`. `StructuralError` Display strings move with it (error text unchanged). `compat` dep stays. This also resolves the audit's Tier-2 note (papk_meta locked in a bin — papk-info can now share it, via the crate).

7. **build_support/papk.rs** — the `#[path]`-included module: build scripts *can* consume workspace crates, just not via the module itself — the dep must be declared in `[build-dependencies]` of **every package whose build.rs includes the module**. The precedent already exists in this exact file: `class_shrink::mapping::ShrinkMap` works because both `platforms/rp/Cargo.toml:68` and `picodroid-core/Cargo.toml:33` declare `class-shrink` as a build-dependency. So: add `papk-format = { path = "...", features = ["write"] }` (or default, see below) to `[build-dependencies]` of **both** `platforms/rp` and `picodroid-core` — even though picodroid-core's build.rs only calls `emit_framework_map_version`/`embed_framework_classes`, the whole module (including any `use papk_format::...`) is compiled into both build scripts, so both must resolve the import. Host-only, zero firmware cost; resolver = "2" keeps build-dep features from unifying into the target graph.
   - **Strictly behavior-preserving step:** nothing — `embed_apk`/`embed_papk_flash_init` treat the `.papk` as opaque bytes and duplicate no section layout, so the minimal migration for site 5 is "declare the build-dep and add nothing" or even defer entirely.
   - **Recommended (small, additive, clearly separable commit):** in `embed_papk_flash_init` (and the non-sim `embed_apk` branch), run `papk_format::validate_structure(&apk_bytes)` and panic with a clear message on failure — turning a truncated/corrupt `.papk` from a device boot-loop into a build error. Uses default features only (no `write` needed).
   - **Out of scope (recorded follow-up):** the PDB1 flash-init sector (`PAPK_FLASH_MAGIC = 0x5044_4231`, `META_SIZE = 4096`) is a *different* mini-format duplicated across `build_support/papk.rs:429-430`, `hal/rp/flash.rs:19`, `hal/sim/flash.rs:8`. A `papk_format::flash_image` module could own it, but that is a separate contract with its own consumers — don't fold it into this refactor.

## (d) Test plan

All inside `papk-format` unless noted; run via `./scripts/test.sh` (never bare `cargo test` — default target is thumbv6m).

1. **Moved suites (must pass unmodified in assertion content):**
   - All 20 tests from `jvm/src/apk.rs` (header/manifest/classes/application/bad-magic/truncated/verify_compat table/ASSETS alignment + truncation). Keep their hand-rolled byte-level fixture builders — they pin the on-disk layout *independent of the crate's own writer*, guarding against writer+parser drifting together.
   - All 12 tests from `tools/pdb/src/papk_meta.rs` (`validate_structure` regressions incl. the 100-byte-stub case).
   - The prefix-peek tests from `platforms/rp/src/packagemanager/install.rs` (incl. "key in unscanned tail => None").
2. **Golden fixture (format-unchanged proof):** before the refactor, build one real app papk with the *current* papk-pack (with and without `--assets-dir`) and check the small files into `papk-format/tests/fixtures/`. Tests assert (a) `Papk::parse` extracts the known manifest/classes/assets, and (b) reconstructing the same content through `PapkBuilder` reproduces the fixture **byte-for-byte** (manifest key order, minor=1, padding, section_count). This is the single strongest "behavior-preserving, format unchanged" check.
3. **Pack->parse round-trip property tests (proptest, dev-dep, host-only):** generate arbitrary manifests (entry variant + package/version/fmv strings + 0..8 extra k/v), 0..16 classes (names ≤ 64 bytes, blobs ≤ 4 KiB), 0..4 assets (dims ≤ 64, arbitrary cf/stride, data ≤ 1 KiB incl. odd lengths to force padding); `build()` then parse and assert field-for-field equality, asset `data` 4-byte alignment within the section, `assets_offset == 0` iff no assets, `section_count` 2/3.
4. **No-panic robustness property tests:** for each generated file, (a) parse every strict prefix `&file[..n]` — must return `Err`/`None`, never panic (overflow-checks are on in test profile, so the `pos + len` arithmetic is exercised hard); (b) single-byte corruptions at header/section-header offsets. Same harness runs `validate_structure` and both `find_manifest_value*` scanners.
5. **Scanner-semantics differential test:** for a papk whose manifest straddles a cut point, `find_manifest_value_in_prefix(prefix, ...)` may return the key while `find_manifest_value(prefix, ...)` must return `None` — pins the two functions apart forever.
6. **Consumer-level checks:** papk-pack gets its first real test — a tempdir integration test packing fixture `.class` files and asserting `papk_format::Papk` parses the output (kills "writer untested"). papk-info/pdb/platforms-rp keep compiling against the new API (their behavior is unchanged by construction).
7. **Project gates (per CLAUDE.md, after the code is believed done, not during):** `./scripts/sim.sh --app helloworld`, `--app benchmark`, blinky under 5 s alarm; then `./scripts/pre-commit` (fmt, clippy RP2040+RP2350 via the feature-pinned invocations, embedded build — which is what proves the crate truly builds no_std on thumbv6m — and full tests).

## (e) `jvm/Cargo.toml` afterwards

Yes — the `compat` dependency leaves jvm entirely (apk.rs was its only user; `rg 'compat::' jvm/src` confirms). pico-jvm becomes a pure interpreter crate, fixing the §6.1 reusability leak:

```toml
[package]
name = "pico-jvm"
version = "0.2.0"        # bumped: pub mod apk removed (semver-breaking)
# ... unchanged metadata ...

[dependencies]
libm = { version = "0.2", default-features = false }
# compat: REMOVED (moved with apk.rs into papk-format)
# papk-format: deliberately NOT added — jvm neither parses nor re-exports PAPK

[features]
parity-metrics = []
mem-diag = []
```

Dependency graph after: `papk-format -> compat`; `platforms/rp -> {pico-jvm, papk-format, compat, ...}` (+ build-dep `papk-format`); `picodroid-core -> {pico-jvm, compat, ...}` (+ build-dep `papk-format`); `papk-pack -> papk-format(write) + image`; `papk-info -> papk-format`; `pdb -> {papk-format, compat, serialport, crc32fast}`; `pico-jvm -> libm` only.

## Migration commit sequence (each green)

1. `feat(papk-format): new crate — parser moved from jvm/src/apk.rs, scan + write modules, round-trip tests` (jvm still has its copy; duplicate momentarily).
2. `refactor(jvm): drop apk module; pico-jvm 0.2.0` + platforms/rp switched to papk_format (steps 2-3 must land together since app.rs/assets.rs import via jvm).
3. `refactor(tools): papk-pack/papk-info/pdb consume papk-format` (steps 4-6).
4. `feat(build): validate .papk structure at embed time` (step 7 hardening, optional/separable).
Full gates (sim smoke x3 + `./scripts/pre-commit`) after each commit; git hook's 15-min suite once, `--no-verify` for follow-ups on the validated tree per established practice.

## KEY FILES
/home/shiv/projects/picodroid-rs/jvm/src/apk.rs
/home/shiv/projects/picodroid-rs/jvm/src/lib.rs
/home/shiv/projects/picodroid-rs/jvm/Cargo.toml
/home/shiv/projects/picodroid-rs/tools/papk-pack/src/main.rs
/home/shiv/projects/picodroid-rs/tools/papk-pack/Cargo.toml
/home/shiv/projects/picodroid-rs/tools/papk-info/src/main.rs
/home/shiv/projects/picodroid-rs/tools/papk-info/Cargo.toml
/home/shiv/projects/picodroid-rs/tools/pdb/src/papk_meta.rs
/home/shiv/projects/picodroid-rs/tools/pdb/src/install.rs
/home/shiv/projects/picodroid-rs/tools/pdb/Cargo.toml
/home/shiv/projects/picodroid-rs/build_support/papk.rs
/home/shiv/projects/picodroid-rs/platforms/rp/build.rs
/home/shiv/projects/picodroid-rs/platforms/rp/Cargo.toml
/home/shiv/projects/picodroid-rs/platforms/rp/src/app.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/picodroid/graphics/assets.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/packagemanager/install.rs
/home/shiv/projects/picodroid-rs/picodroid-core/build.rs
/home/shiv/projects/picodroid-rs/picodroid-core/Cargo.toml
/home/shiv/projects/picodroid-rs/compat/src/lib.rs
/home/shiv/projects/picodroid-rs/Cargo.toml
/home/shiv/projects/picodroid-rs/docs/code-health-audit-2026-07.md

## RISKS
Writer byte-exactness: PapkBuilder must reproduce papk-pack's exact emission (manifest key order entry/package-name/version/framework-map-version, VERSION_MINOR=1 always, assets_offset=0 + section_count=2 when no assets, 4-byte asset padding, crc32/reserved=0). Any drift changes shipped .papk bytes. Mitigated by the checked-in golden-fixture byte-equality test built with the PRE-refactor papk-pack.
The two manifest scanners have deliberately different semantics: strict (pdb pre-flight — merging toward tolerant re-opens the '100-byte stub accepted' install regression) vs prefix-tolerant (device install peek over a partial wire buffer — merging toward strict breaks installs whose manifest straddles the peek boundary). They must stay separate functions with a differential test pinning them apart.
pico-jvm public API break: removing `pub mod apk` is semver-breaking for a crate with published-style metadata; requires the 0.1.1 -> 0.2.0 bump and touches the lib.rs doc example. Any out-of-tree user of pico_jvm::apk breaks (none in-repo).
build_support/papk.rs is compiled into BOTH platforms/rp and picodroid-core build scripts; adding `use papk_format` there without adding the [build-dependencies] entry to BOTH Cargo.tomls fails only picodroid-core's build script — easy to miss if only rp is tested. (class-shrink precedent shows the correct dual declaration.)
Feature unification: papk-format appears with `write` (alloc) in host tools/build-deps and without it in the thumbv6m firmware graph. Workspace resolver = "2" keeps these separate, but the pre-commit embedded build must be the gate that proves no alloc/std leaks into the no_std parser path (e.g. a stray non-cfg'd `extern crate alloc`).
BuildError length validation replaces papk-pack's silent `as u16`/`as u32` truncation — byte-identical for all currently-valid inputs, but an input that previously produced a silently corrupt papk (e.g. >64 KiB class name) now errors. Strictly an improvement, but technically a behavior change on invalid inputs.
RP2040 896K flash ceiling: the parser code is moved, not duplicated, so image size should be unchanged — but verify the RP2040 embedded build in pre-commit anyway; do not attempt LTO tweaks (known to grow the image).
The `cf` byte stays an opaque u8 in the format crate while LV_COLOR_FORMAT_RGB565 (papk-pack) and cf_label (papk-info) keep their own LVGL value tables — remaining, accepted duplication that can drift from vendored LVGL (same class of risk as project_lvgl_ffi_constants); note it in both files' comments.
The audit undercounted: platforms/rp/src/packagemanager/install.rs:226 is a sixth format re-derivation (prefix-tolerant peek). If the migration only covers the five audited sites, the worst duplicate (device-side, hand-rolled, subtly different semantics) survives.
Scheduling: nightly 3AM sim-run / 4AM hil-run cron will exercise intermediate states if commits land across a night; land the jvm-module-removal and platforms/rp-switch as one commit (they are inseparable anyway) and keep every commit green through the full pre-commit suite.

## SCOPE
Medium: one new ~900-line crate (mostly moved code: ~500 ln parser+tests from jvm/src/apk.rs, ~150 ln scan from papk_meta.rs + packagemanager, ~150 ln writer from papk-pack, ~100 ln new property/golden tests), minus ~600 lines deleted across 5 consumer sites; 8 Cargo.toml edits (workspace root, new crate, jvm, platforms/rp, picodroid-core, papk-pack, papk-info, pdb). 4 sequenced commits, each through the full ~15-min pre-commit suite plus sim smoke; roughly 1 focused day including gate runs. No format change, no Java/SDK surface change, no shrink-map or native-registry interaction.

## CRITIQUE VERDICT: needs_changes

### ISSUES
- BUILD-BREAKING OMISSION (commit 2): bumping pico-jvm 0.1.1 -> 0.2.0 breaks both dependents' path deps, which pin a version requirement: platforms/rp/Cargo.toml:39 `pico-jvm = { path = "../../jvm", version = "0.1" }` and picodroid-core/Cargo.toml:25 `pico-jvm = { path = "../jvm", version = "0.1" }`. Cargo verifies the version req against the path dep; the plan's step 2/commit 2 edits neither, so the tree is red at exactly the commit the design promises is green.
- GROUND-TRUTH ERROR: site 6 is marked 'Tested? yes' but its tests have never run. platforms/rp/src/packagemanager/mod.rs:4-5 gates `pub mod install;` behind `#[cfg(not(any(test, feature = "sim")))]` — under `cargo test` the whole module (including its `#[cfg(test)] mod tests` at install.rs:277) is compiled out, and it's also absent from sim builds. The 4 extract_framework_map_version tests are dead code. Corollary: the design's step-3 claim that the moved tests include a "key may be in the unscanned tail -> None (not error)" case is false — no such test exists; none of the 4 dead tests exercise a manifest straddling the peek boundary. That test must be written new, and the resurrected tests need their first-ever execution (they look correct on inspection, but 'moved verbatim, must pass unmodified' overstates the safety net).
- BEHAVIOR CHANGE DISGUISED AS REFACTOR (papk-info): the claim 'behavior unchanged by construction' is false for corrupt files. Today parse_classes/parse_assets (main.rs:124-188) return Err ("Truncated class name", "Truncated asset pixel data", etc.) and papk-info exits 1; ClassIter/AssetIter silently return None on the same truncation, so the migrated papk-info would print a shorter table, a wrong 'N classes' summary line, and exit 0. For a dump tool whose main job includes inspecting suspect papks, that is a real diagnostic regression. The proposed Papk API exposes no declared entry count to detect it. Secondary: today papk-info dumps header AND sections of a future-major file (parse_file_header never checks version_major); the new flow's Papk::parse returns UnsupportedVersion for major!=1, losing the section dump — minor, but should be stated as accepted, not implied unchanged.
- no_std TEST-COMPILATION HOLE: `#![no_std]` at the papk-format crate root applies under cfg(test) too. The apk.rs tests being moved use `alloc::boxed::Box`/`alloc::vec::Vec` and compile today only because jvm/src/lib.rs:66 has a crate-level `extern crate alloc;`. Core-only papk-format needs `#[cfg(test)] extern crate alloc;` (and `#[cfg(test)] extern crate std;` or `#![cfg_attr(not(test), no_std)]` for the proptest harness, which requires std). 'Moved verbatim' is off by these mandatory lines — trivial, but the design presents verbatim-ness as the correctness argument.
- API SKETCH DEFECTS: (1) PapkBuilder::class takes `jvm_name: &str` and manifest_entry takes `key: &str, value: &str` with no `'a` — borrowed storage is impossible as written; either make them `&'a str` or specify owned (alloc String) storage. (2) papk-pack deletion range ':276-522' contradicts the keep-list: it contains the Asset struct (:336-343), collect_assets (:347-376), and decode_png_to_rgb565 (:385-411), all of which the same sentence keeps. Correct ranges: delete :276-331 and :413-522. (3) papk-info deletion range ':7-188' includes AssetInfo (:144-150), yet print_assets_table (kept) consumes &[AssetInfo] and the design says 'AssetEntry maps onto the table row struct' — keep AssetInfo (built from AssetEntry) or respecify the print function.
- MINOR GROUND-TRUTH DRIFT: apk.rs has 19 tests, not 20; papk_meta.rs has 11 tests, not 12. And there is no 'lib.rs doc example that references pico_jvm::apk::Papk' — jvm/src/lib.rs mentions apk only at :68 (`pub mod apk;`); the example lives in apk.rs's own module doc (:60-69) and moves with the file (update its `pico_jvm::apk::` path there). Also compat/src/lib.rs:6 doc-comment says the device-side caller is "pico-jvm's Papk::verify_compat" — stale after the move.
- 32-BIT ARITHMETIC BLIND SPOT in the no-panic test plan: the parser's `pos + len` / `offset + SECTION_HEADER_LEN` arithmetic (e.g. apk.rs:278-285, 471, 439-448) is exercised by proptest on a 64-bit host where u32-derived offsets can never overflow usize; on the 32-bit device (release, overflow-checks off) a hostile assets_offset near u32::MAX wraps instead. Pre-existing behavior, moved verbatim — not a regression — but the design's claim that the property tests exercise the overflow arithmetic 'hard' is only true for the host word size; sim (64-bit) cannot see the device-side wrap (same class as the known handle-table sim-blindness).

### AMENDMENTS
1) Commit 2 must also edit the two dependent version requirements: platforms/rp/Cargo.toml:39 and picodroid-core/Cargo.toml:25 change `version = "0.1"` to `version = "0.2"` (or drop the version key from these path deps entirely, which is the lower-maintenance option for an unpublished crate). Add both files to the step-2/commit-2 checklist. 2) Correct the ground-truth table: site 6's tests are dead code (packagemanager/mod.rs:4-5 cfg-gates `mod install` out of both `test` and `sim` builds — mark it 'tests exist but never compile'). Reframe step 3 accordingly: the 4 install.rs tests get their first-ever execution inside papk-format, and the prefix-straddles-the-cut-point test (design test 5) is NEW, not moved — write it before deleting extract_framework_map_version so the tolerant semantics are pinned by a passing test first. 3) Extend the parser API with declared-count accessors — e.g. `Papk::class_count() -> Result<u32, PapkError>` and `Papk::asset_count() -> Result<Option<u32>, PapkError>` (or expose `remaining()` on ClassIter/AssetIter) — and have migrated papk-info compare yielded vs declared and exit with an error on shortfall, preserving today's "Truncated class name"-class diagnostics; add a papk-info-level statement that future-major files now stop after the header dump (accepted change). 4) Amend the crate-root spec: `#![no_std]` plus `#[cfg(test)] extern crate alloc;` and `#[cfg(test)] extern crate std;` (or `#![cfg_attr(not(test), no_std)]`), noting the moved tests keep assertion content but gain these harness lines; confirm proptest runs as host-only dev-dep. 5) Fix the API sketch: `class(&mut self, jvm_name: &'a str, bytes: &'a [u8])`, `manifest_entry(&mut self, key: &'a str, value: &'a str)` (or document owned String storage in the alloc-gated writer); correct papk-pack deletion ranges to :276-331 + :413-522 (keep :333-411), and keep papk-info's AssetInfo row struct (constructed from AssetEntry) or respecify print_assets_table. 6) Cosmetic ground-truth fixes: 19 apk.rs tests, 11 papk_meta tests; replace the "lib.rs doc example" edit with "update the module-doc example inside the moved file (apk.rs:60-69) from `pico_jvm::apk::` to `papk_format::`"; add compat/src/lib.rs:6 doc-comment to the touch list. 7) Optional hardening rider for the risks section: during the move, convert the parser's offset arithmetic to `checked_add` (papk_meta::validate_structure already does this at :60-62 — precedent in-repo), since the host-only proptests cannot observe 32-bit release wrap on device; if deferred, record it as a known sim-blind gap. With these folded in, the extraction plan (crate shape, no_std strategy, strict/tolerant scanner separation, non-re-export decision, dual build-dep declaration, golden-fixture byte-equality gate, commit sequencing) is verified sound against the actual code.
