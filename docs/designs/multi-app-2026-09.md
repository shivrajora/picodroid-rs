# Design: multi-app — a dynamic app region, a package directory, and what a launcher needs

> Produced 2026-09-07 by a planning session (three parallel audits over the
> PAPK/manifest/pdb path, the flash layout, boot and installer, and the
> storage/PackageManager/lifecycle surface; then a design pass whose product
> decisions were settled with the owner). Every claim about what exists was
> checked against source at `4d8d409`. Amendments are appended at the bottom
> and OVERRIDE the design body where they conflict. Execute from this doc;
> append an amendment when reality diverges.
>
> Successor to the multi-app half of `app-store-roadmap-2026-09.md` — its S0,
> S1, the query half of S2, S6, S7 and the deferred per-package storage. That
> document's amendment A2 records what changed. Its vocabulary is assumed:
> PAPK, the shrink map and `framework-map-version`, `run_app`, the install
> orchestrator, and the JVM core / comm core split on RP2040 and RP2350.

## 0. Why this exists

A device holds exactly one app. `flash.sh --app X` links X into a fixed 1 MB
`PAPK_FLASH` slot and `pdb install` overwrites that slot; the last
`Activity.finish()` ends the app and the JVM task waits forever for the next
install (`platforms/rp/src/boot_tasks.rs:177-186`). Nothing identifies an app
beyond its entry class: the `package-name` the PAPK carries is the Gradle
directory name and nothing reads it; there is no label, no icon, no version
code. Storage is one LittleFS root every app writes into as it pleases
(`SharedPreferences.java:19` writes `/prefs/<name>`).

The product wants: system apps bundled with the firmware release — a
**launcher** that boots first and a **settings** app — with board config able
to name a different boot app; a manifest that identifies apps, icon included;
`pdb install` as the only install path for now, refusing an app when there is
no room; and per-app non-volatile storage that the OS isolates. One app runs
at a time; multitasking is out of scope.

Three decisions taken with the owner shape everything below:

1. **The app region is an allocator, not slots.** Installed apps are
   contiguous, self-describing runs of 4 KB sectors placed first-fit, the map
   is rebuilt by scanning at boot, and compaction closes gaps when an install
   needs a contiguous run that only exists in pieces (D3–D6). No fixed
   per-app maximum, no persisted table.
2. **Storage is one LittleFS with a per-package chroot**, growing on demand,
   with a system reserve and a per-app cap (D10). The FS region itself is
   fixed at build time — LittleFS fixes its block count at format — but
   within it an app's directory starts at 8 KB and grows as it writes.
3. **Settings v1 is About + Apps (uninstall) + Storage.**

The work is three sessions, each one PR with sim coverage and an HIL check:

| Session | Stages | Delivers |
|---|---|---|
| 1 (this document's first execution) | M0, M1 | manifest identity + icon; the flash layout, the region allocator with compaction, the package directory, `pdb list` / `pdb uninstall` / no-room refusal |
| 2 | M2 | system apps in firmware, boot policy, app switching, the launcher, `PackageManager` queries |
| 3 | M3 | the storage sandbox, reserve and cap, the settings app |

## 1. What exists and carries over (checked at `4d8d409`)

| Piece | Where | Carries over as |
|---|---|---|
| PAPK container: 24-byte header, MANI/CLSS/ASST sections, key/value manifest | `papk-format/src/lib.rs:16-51`, `write.rs:208-240` | Unchanged container; three additive manifest keys (D9) |
| `PicodroidManifest.xml` → Gradle plugin → `papk-pack` | `buildSrc/.../ManifestSchema.kt`, `PicodroidPapkPlugin.kt:345-361`, `tools/papk-pack/src/main.rs` | The identity attributes ride the same pipeline; `package-name` becomes `package=` (was `target.name`, `PicodroidPapkPlugin.kt:348`, consumed by nothing) |
| ASSETS section: PNG decoded on the host to RGB565, `ImageView.setImageSource(name)` | `tools/papk-pack/src/main.rs:372-438`, `picodroid-core/src/graphics/assets.rs` | The icon is an ASSETS entry named by the manifest |
| Boot-meta sector: `[magic][flags][len]`, meta written last | `papk-format/src/flash_image.rs` | Grows a `seq` word and a commit page (D3) |
| One slot per chip, chip-gated constants | `platforms/rp/mcus/rp/rp2040.x`, `rp2350.x`, `platforms/rp/src/hal/rp/flash.rs:5-20` | The region and its constants are generated from board.toml (D2) |
| Installer: peek + compat before erase, park the JVM core, 256-byte pages, CRC, commit, reset | `picodroid-core/src/install/orchestrator.rs:106-188`, `slot.rs:37-116` | Same choreography; placement is decided between the compat check and the erase (D5) |
| `run_app` re-entry: heap reset, class-set republish, pool drain | `picodroid-core/src/boot.rs:193-264` | App switching is a re-entry (M2) |
| JVM supervisor loop | `platforms/rp/src/boot_tasks.rs:149-199` | Asks the directory for the next image instead of blocking (D7, M2) |
| pdb over USB CDC: PING/INSTALL/SYSMON/INPUT, greeting with max PAPK + fmv | `pdb-protocol/src/lib.rs:57-98`, `greeting.rs`, `tools/pdb/src/install.rs` | Additive: two commands, two statuses, a greeting tail (D8) |
| LittleFS + `HalFs` + pure-Java `SharedPreferences` | `picodroid-core/src/fs/`, `hal/traits.rs:253-268`, `sdk/java/picodroid/content/SharedPreferences.java` | Chroot at the native seam leaves the Java untouched (D10) |
| `PackageManager.hasSystemFeature` only | `sdk/java/picodroid/content/pm/PackageManager.java` | Grows the query surface in M2 |
| Sim loads the PAPK at run time from `PICODROID_APK_PATH`; no slot model | `build_support/papk.rs:462-499`, `picodroid-core/src/hal/sim/` | Gains an in-memory app region seeded from the environment (D6, M1) |
| Guards: seam count, ratchet, no Java-name literals, native tables | `picodroid-core/src/porting.rs:176`, `bench/parity/ratchet.toml`, `native_handler/member_names.rs`, `method_tables.rs` | §7 |

## 2. Decisions

### D1 — Multi-app is a board capability; RP2040 stays single-app

MCU tomls carry the geometry defaults (`fs_kb`, `app_region_kb = 1024`,
`max_installed_apps = 1`; rp2040 also `boot2_bytes = 256`) and board.toml
overrides them. The cfg `has_multi_app` is `max_installed_apps > 1`, emitted
the way `has_json` is (`build_support/board_cfg.rs:97-114`); in M2 it drives
a `MULTI_APP_CLASSES` exclude list the way `JSON_CLASSES` does (`:79-95`),
so a board without it pays no flash for launcher-facing SDK classes.

One code model, not two: the region allocator runs on every board with a
directory of `max_installed_apps` entries. On a single-app board a fresh
package replaces the installed one — today's behaviour — and everything
that only a multi-app board needs (compaction, `pdb list`/`uninstall`, the
greeting tail, the no-room refusal) is `#[cfg(has_multi_app)]`. The rp2040
image is allowed to grow only by the shared meta-format and directory code,
accepted explicitly in the ratchet.

### D2 — RP2350 layout: one generator for the linker script and the constants

All four rp2350 boards, laid out top-down from the end of flash:

| Region | Origin | Length | Holds |
|---|---|---|---|
| `FLASH` | `0x10000000` | 2048K | firmware; in M2 also the system apps' PAPKs as `.rodata` |
| `FS_FLASH` | `0x10200000` | 512K | LittleFS (128 blocks) |
| `PAPK_FLASH` | `0x10280000` | 1536K | the app region: 384 sectors |

RP2040 is byte-identical to today (BOOT2, `FLASH` 896K−0x100, `FS_FLASH`
128K at `0x100E0000`, `PAPK_FLASH` 1024K at `0x10100000`). The formula is
`APP = [end − app_region, end)`, `FS` just below, `FLASH` from the origin
(plus boot2) up to `FS`; with the MCU defaults it reproduces both current
scripts exactly, which is the test that pins it.

`build_support/flash_layout.rs` computes the layout from the MCU and board
props, renders the `MEMORY {}` block plus `__fs_start`/`__fs_end` into the
`memory.x` that `boards::place_memory_x` writes (the MCU `.x` files keep only
their SECTIONS tails), and emits `flash_layout.rs` — `PAPK_REGION_OFFSET`,
`PAPK_REGION_LEN`, `FS_OFFSET`, `FS_LEN`, `MAX_INSTALLED_APPS`, `PROGRAM_LEN`
— into both OUT_DIRs through `board_cfg::emit_neutral`. `scripts/lib.sh`
computes `PROGRAM_FLASH_MAX` from the same keys instead of scraping
`LENGTH(FLASH)` out of the script (`lib.sh:226-241`). The region names
`FLASH` (`build_support/freertos.rs:127-142` places `.init_array` by it) and
`PAPK_FLASH` (`embed_papk_flash_init`) and the `__fs_*` symbols
(`fs_region_bounds`) are contracts the generator keeps.

Moving `FS_FLASH` and the region on rp2350 reformats existing dev boards
once and drops whatever `pdb install` put in the old slot. Release note.

### D3 — Runs are self-describing; the map is rebuilt by scanning

A run is `[4 KB meta sector][PAPK padded to 4 KB]` at any sector of the
region. The meta sector (`papk-format/src/flash_image.rs`) has two written
pages:

```text
page 0 @0:    [magic "PDB1"][flags u32][len u32][seq u32]
page 1 @256:  [magic "PDBC"]
```

A run is an installed app only when both pages parse, the PAPK behind it
passes `validate_structure`, and it carries `package-name`. No page is ever
programmed twice: an install writes the image, then both pages; a
relocation writes the header page, copies the image, then the commit page.
A power loss anywhere leaves a whole run or a commit-less one — never a
half-copied image that reads as installed. `flags` bit 0 `BOOT_DEFAULT` is
set by the build script on the baked app and inherited by a reinstall of the
same package. `seq` orders runs: an install or relocation writes one more
than the highest on the device (the baked image is 0), so when two runs
name the same package — a power loss between committing an upgrade and
erasing the old copy — the higher `seq` wins, tie to the lower sector.

There is no persisted allocation table. The directory (D4) is rebuilt by a
sector-stride scan of the region: at each sector read the meta header; on a
hit, take the run and skip past it; otherwise step one sector. That is at
most 384 header reads from XIP on rp2350 — well under a millisecond — plus
one manifest parse per run. A table sector would duplicate the meta pages
and add a torn-write hazard of its own. Uninstall erases the whole run, so
stale image bytes can never carry a meta page that looks like an app.
Commit-less runs and the losers of a duplicate are erased at boot.

### D4 — The package directory

`picodroid-core/src/packages.rs` holds a static
`[Option<Entry>; MAX_INSTALLED_APPS + SYSTEM_MAX]` (`SYSTEM_MAX = 2`):

```rust
pub struct Entry {
    image: &'static [u8],      // the PAPK, in place in flash (or the sim's buffer)
    first_sector: u16, sectors: u16,
    flags: u8, kind: Kind,     // Kind::App | Kind::System (M2)
    seq: u32,
}
```

About 20 bytes an entry; the rp2350 image leaves ~18 KB of RAM headroom
(`bench/parity/ratchet.toml`, `scripts/lib.sh` main-stack floor), so no
strings live here — name, label, icon and versions are re-read from the
image on demand (`Papk::package_name()` and friends). The module has no
`pub trait`, so `porting.rs`'s seam count stays at 42; the family supplies
the region's mapped base and length, and the sim supplies its buffer.

### D5 — Allocation policy

`plan_install(package, len)` with `need = 1 + ceil(len / 4096)` sectors:

1. `len > region_len − 4 KB` → `TooLarge` (it could never fit).
2. Directory full and the package not installed → `NoRoom`; a single-app
   board instead replaces its installed app.
3. Package already installed → first-fit in the free space *excluding* its
   old run (non-destructive upgrade: the old run is erased after the new one
   commits); else first-fit counting the old run as free (destructive, as
   today: the old run is erased first).
4. No fit but total free ≥ need → compact (D6) and retry.
5. Otherwise `NoRoom { need, largest_free, total_free, installed, max }`.

The peek that already precedes the erase (`INSTALL_PEEK_BYTES = 512`,
`orchestrator.rs:138-160`) also yields `package-name`: it is the second
manifest entry, a few dozen bytes in, and
`find_manifest_value_in_prefix` returns `None` rather than reading past a
straddling entry (`papk-format/src/scan.rs:191-238`). `None` is a refusal
on a multi-app board.

### D6 — Compaction

Runs sorted by sector are slid toward the region start one at a time,
through a 256-byte RAM buffer: read the source page through the mapped
region, program it at the destination; erase each destination sector before
its first page. Per move: write the destination header page (seq = current
max + 1, no commit page) → copy the data sectors in ascending order → write
the commit page → erase the old meta sector if the slide did not already
overwrite it. A move into fully free space is loss-free at every instant.
An overlapping slide (gap smaller than the run) destroys the old meta sector
mid-copy, so between that moment and the commit page a power loss loses
*that app* — never a corrupt or phantom one — for a window of seconds. The
device is parked for the install already; the host waits up to 120 s for
READY and says "compacting". Worst case is the whole region: 384 sectors at
roughly 60–80 ms each (erase, 16 page programs, XIP off and on around each),
25–30 s.

### D7 — Boot selection

M1: the run with `BOOT_DEFAULT` (lowest sector if several), else the
lowest-sector run, else nothing — in which case the JVM task waits for a
`pdb install` instead of today's `expect("PAPK flash region invalid")`
(`platforms/rp/src/main.rs:107-108`). M2 adds, in precedence order:
`flash.sh --boot <package|launcher|app>` (a compile-time override),
board.toml `boot_package`, the `BOOT_DEFAULT` run, the launcher, the first
run. `flash.sh --app X` therefore keeps booting X, which every HIL row
relies on; a product board names its kiosk app in board.toml; a bare
multi-app board boots the launcher.

### D8 — pdb protocol

- `PROTOCOL_VERSION = "picodroid/2.2"`: same length, every offset unchanged,
  a 2.1 host still installs.
- Greeting tail after the framework-map-version, additive:
  `[u8 max_apps][u8 installed][u32 largest_free][u32 total_free]`;
  `max_papk` becomes the region minus one meta sector. Single-app boards
  send no tail; the host prints the tail as `apps N/M, free X KB (largest Y KB)`.
- `CMD_LIST = 0x04`: text rows `sector package version-code version label
  size flags` and a `free:` footer. `CMD_UNINSTALL = 0x05`: payload is the
  package name; park → erase the run → reset. `STATUS_NO_ROOM = 0xFB` with
  the D5 numbers and a `pdb uninstall <pkg>` hint; `STATUS_NOT_FOUND = 0xFA`;
  `STATUS_ERR "system package"` / `"no package-name"`.
- Host: `pdb list`, `pdb uninstall <pkg>`, a `package-name` pre-flight in
  `pdb install`, and the READY timeout raised for compaction.

### D9 — Manifest identity

`package-name` is the manifest's `package=` attribute; `label`, `icon` and
`version-code` are additive keys (`papk-format/src/lib.rs::keys`) with typed
optional fields on `ManifestSpec`, emitted after `framework-map-version` only
when set, so every existing input stays byte-identical (the golden fixtures
prove it). The icon must name a packed asset — `papk-pack` errors, the
Gradle plugin errors earlier at configuration time — and should be a small
square: 48×48 is the convention, `papk-pack` warns past 64×64. `papk-pack`
also gains `--repack <in.papk>` (copy every section, override identity keys)
and `--pad-asset <bytes>`, which is how the HIL harness mints fixtures that
differ only in package name and size.

### D10 — Storage (M3)

One LittleFS mount: a second costs ~12 KB of RAM the rp2350 image does not
have. The `picodroid/io/*` natives (`picodroid-core/src/native_handler/io.rs`)
prefix every path with `/data/<package>/` and reject `..` and empty
segments, so an app cannot even name another app's files — the same model
Android enforces below the app, here at the only seam Java can reach storage
through. `SharedPreferences.java` and every existing example keep working
unchanged (`/prefs/<name>` lands under the package). `/system/` is
framework-only. `Context` gains `getFilesDir`, `openFileOutput`,
`openFileInput`, `fileList`, `deleteFile`, `getPackageName`; `picodroid.os.StatFs`
reports what an app may still write. Policy: `fs_system_reserve_kb`
(default 64; app writes fail below it, system packages exempt) and
`app_data_cap_kb` (default 25 % of the FS, 0 = unlimited; 4 KB-block
accounting, walked at app start, deltas after). Uninstall wipes
`/data/<package>` (`HalFs` gains `list_dir` and `remove_dir_all`).

### D11 — System apps and switching (M2)

`system-apps/launcher` and `system-apps/settings` (packages
`picodroid.launcher`, `picodroid.settings`) are Gradle projects discovered
like `examples/`, built to PAPKs before the firmware and embedded with
`include_bytes!` into `.rodata` — no linker region, no waste, and the ratchet
counts them honestly. The sim loads them at run time from
`PICODROID_SYSTEM_APKS` with the `sim-runtime` marker trick
(`scripts/sim.sh:159-173`). The supervisor loop asks `packages::next_image()`:
a pending cross-package launch, else the launcher when the exiting app was
not it, else the same image again; a pending park request still wins. There
is no package task stack — the last `finish()` returns home, as finishing a
task does on Android. `Intent` gains a package target;
`PackageManager.getInstalledPackages`, `getPackageInfo`,
`getLaunchIntentForPackage`, `getApplicationLabel`, `getApplicationIcon`
arrive with `PackageInfo`, `ApplicationInfo`, `ActivityNotFoundException`, a
`BitmapDrawable` over another run's ASSETS entry and
`ImageView.setImageDrawable`. The launcher is a column of focusable rows
(icon + label) that works with the Enviro+ four-button keypad and the
testbench touch panel. Settings v1: About (board, firmware and map version,
storage), Apps (installed list, uninstall through
`PackageInstaller.uninstall(String)`, erasing the run from the JVM task with
the primitives LittleFS already uses), Storage (bytes per package).

## 3. Seams

```rust
// papk-format
pub mod keys { pub const VERSION_CODE; pub const LABEL; pub const ICON; }
impl Papk { fn package_name(); fn version(); fn label(); fn icon(); fn version_code() -> Option<u32>; }
pub mod flash_image {
    pub const HEADER_LEN: usize = 16; pub const COMMIT_OFFSET: usize = 256;
    pub const META_READ_LEN: usize = 260; pub const FLAG_BOOT_DEFAULT: u32 = 1;
    pub struct BootMeta { len, flags, seq }
    pub fn build_header_page(len, flags, seq); pub fn build_commit_page();
    pub fn build_meta_pages(len, flags, seq) -> [u8; 512];
    pub fn parse_header(bytes, max_len); pub fn is_committed(bytes); pub fn parse_meta(bytes, max_len);
}

// picodroid-core::install (no new pub trait; seam count stays 42)
pub unsafe trait PapkRegionFlash {            // was PapkSlotFlash
    const REGION_OFFSET: u32; const REGION_LEN: usize; const SECTOR_SIZE: usize;
    const MAX_INSTALLED_APPS: usize;
    fn mapped_base() -> *const u8;
    unsafe fn erase_range(flash_offset: u32, len: usize);
    unsafe fn program_range(flash_offset: u32, data: &[u8]);
    fn reset() -> !;
}
pub struct PapkRegion<F> { target: u32 }        // was PapkSlot<F>; the selected run's first sector
pub unsafe trait PapkFlash {
    fn region_len(&self) -> usize; fn mapped_base(&self) -> *const u8;
    fn select_run(&mut self, first_sector: u32);
    unsafe fn erase_run(&mut self, first_sector: u32, sectors: u32);
    unsafe fn erase_region(&mut self, papk_len: usize);          // the selected run
    unsafe fn write_page(&mut self, page_index: u32, page: &[u8; 256]) -> bool;
    unsafe fn write_meta_header(&mut self, len: u32, flags: u32, seq: u32);
    unsafe fn write_meta_commit(&mut self);
    unsafe fn commit_metadata(&mut self, len: u32, flags: u32, seq: u32);
    unsafe fn copy_page(&mut self, src_sector: u32, dst_sector: u32, page: u32);
    fn trigger_reset(&mut self) -> !;
}
pub fn run_install(transport, coordinator, flash, papk_len);
pub fn run_uninstall(transport, coordinator, flash, first_sector, sectors);

// picodroid-core::packages
pub fn rescan(base: *const u8, len: usize);         // boot, and after every install/uninstall/compaction
pub fn entries() -> impl Iterator<Item = &'static Entry>;
pub fn find(package: &str) -> Option<&'static Entry>;
pub fn boot_image() -> Option<&'static [u8]>;
pub fn plan_install(package: Option<&str>, len: usize) -> Result<Plan, InstallError>;
pub fn compact(flash: &mut impl PapkFlash);
pub fn next_seq() -> u32;

// pdb-protocol
pub const CMD_LIST: u8 = 0x04; pub const CMD_UNINSTALL: u8 = 0x05;
pub const STATUS_NO_ROOM: u8 = 0xFB; pub const STATUS_NOT_FOUND: u8 = 0xFA;
greeting::encode_with_apps(max_papk, fmv, max_apps, installed, largest_free, total_free, out);
```

Board and MCU keys (`build_support/flash_layout.rs`): `boot2_bytes`, `fs_kb`,
`app_region_kb`, `max_installed_apps`; M2 adds `boot_package`; M3 adds
`fs_system_reserve_kb`, `app_data_cap_kb`.

## 4. Stages

| # | Stage | Scope | Session | Device code | HIL |
|---|---|---|---|---|---|
| M0 | Manifest identity | `papk-format` keys/accessors/writer, boot-meta v2, `papk-pack` flags + `--repack`/`--pad-asset`, Gradle plugin, XSD, `imagedemo` label + icon, docs | 1 | boot-meta parse only | existing rows |
| M1a | Layout | `flash_layout.rs` generator, MCU/board keys, `memory.x` rendering, `lib.sh` gate; rp2040 proven byte-identical | 1 | none | — |
| M1b | Region + directory | `PapkRegionFlash`/`PapkRegion`, `packages.rs` scan and cleanup, `MemRegion` for tests and the sim | 1 | boot scans the region | — |
| M1c | Install policy | placement, non-destructive upgrade, compaction, `run_uninstall`, new `InstallError`s | 1 | yes | `install`, `install-compact` |
| M1d | Protocol + host | greeting tail, `CMD_LIST`/`CMD_UNINSTALL`, statuses, `pdb list`/`uninstall`, refusal text | 1 | yes | `list`, `uninstall`, `install-reject-noroom` |
| M1e | Boot | `main.rs` scans, supervisor runs `boot_image()`, empty region waits | 1 | yes | boot rows |
| M1f | Sim | in-memory region from `PICODROID_APK_PATH` + `PICODROID_SIM_APPS`, `sim-ctrl apps list/install/uninstall` for non-running packages | 1 | — | sim rows |
| M2 | System apps + switching | D7, D11 | 2 | yes | launcher rows |
| M3 | Storage + settings | D10, settings app | 3 | yes | storage rows |

## 5. Status

| Stage | Status |
|---|---|
| M0 | DONE 2026-09-07 (028e5c1) |
| M1a–M1f | DONE 2026-09-07 (see A1) |
| M2 | DONE 2026-09-07 (see A2) |
| M3 | DONE 2026-09-09 (Stage 0 merged as 73665a4; M3a–M3e merged to main as 1c68435; map v0.22.0 cut on main; see A3) |

## 6. Deferred and open

- **Sim install of the running package** needs the M2 switching loop; M1's
  `sim-ctrl apps install` refuses it.
- **The overlapping-slide window** (D6): a journaled two-copy move would
  close it at the cost of a scratch area; revisit if a field device ever
  loses an app to it.
- **Intent extras across packages**: a launch intent carries none in M2
  (the caller's heap is reset); serialise the small extras table if an app
  needs it.
- **A runtime boot-app override in Settings** (over board.toml
  `boot_package`) needs a persisted `/system/settings` store; not in M2/M3.
- **Store, signatures, TLS, permissions, firmware OTA** stay with
  `app-store-roadmap-2026-09.md` (S3, S4, S5, S8, S9 and its deferred list).

## 7. Docs and guards

Docs touched: `reference/manifest.md`, `guides/assets.md` (M0);
`reference/pdb-commands.md`, `reference/limits.md`, `reference/porting-guide.md`
(board and MCU keys, the install seam wording, the file table),
`get-started/hot-swap.md` (M1); `api/*.md`, `examples.md` (M2, M3).

Guards that pin this design: `papk-format/tests/golden.rs` (unchanged
fixtures = byte-identical writer); the layout test that reproduces both
current linker scripts from the MCU defaults; `porting.rs`
`EXPECTED_SEAM_ITEMS = 42` and the guide's seam names; the rp2040 ratchet
(`bench/parity/ratchet.toml`) with an explicit `size:` acceptance; the
`packages` unit tests over `MemRegion` (scan, cleanup, placement,
compaction, install after compaction through `run_install`); the pdb
greeting golden bytes; and in M2 the class-registry and method-table
cross-checks for every new native.

## 8. M3 execution plan (2026-09-08)

> Planned after M2 shipped (map v0.21.0, `fceb7fc`). Every claim about
> what exists was checked against source at `fceb7fc`. This section
> refines D10 and the settings half of D11; where it is more specific than
> the body, it wins, and A1/A2 still override the body where they say so.
> Execute from here; append A3 when reality diverges.

M3 delivers three things: the **storage sandbox** (every app sees its own
root, on every board), the **quota** (a system reserve and a per-app cap,
multi-app boards only), and the **settings app** (About, Apps with
uninstall, Storage). One PR, five commits (M3a–M3e), sim coverage for each,
then the bench.

### 8.1 What M3 builds on (checked at `fceb7fc`)

| Piece | Where | M3 uses it as |
|---|---|---|
| `picodroid/io/*` natives: File / FileInputStream / FileOutputStream over a `backend` that is `hal::fs` (the registered `HalFs`) on sim and device and an in-memory map under `cfg(test)` | `picodroid-core/src/native_handler/io.rs:30-51`, `:300-390` | the one seam every app path passes through; the sandbox goes in front of `backend::*` |
| `HalFs`: `exists/is_file/is_dir/length/delete/mkdir/rename/truncate/read_at/write_at`, no handles, failures as `false`/`0`/`-1` | `picodroid-core/src/hal/traits.rs:253-268`, impl `fs/hal_impl.rs` | grows `list_dir` and `space`; nothing else |
| LittleFS reached through one serial worker task; `with_fs` runs a closure on it (inline pre-scheduler) | `picodroid-core/src/fs/mod.rs:234-262`, `executors/serial_worker.rs` | the erase for a Java-side uninstall rides the same worker (P6) |
| The wrapper crate offers `read_dir`/`list_dir`, `stat` (size, type), `remove` (fails on a non-empty dir), `fs_size` (allocated blocks) | `littlefs-rust-0.1.0/src/filesystem.rs:257-324` | `list_dir` and `space` are thin over these |
| Sim FS image: host file, `PICODROID_SIM_FS_KB` default 256 | `picodroid-core/src/fs/storage_host.rs:25-44` | sized from the board instead (P9) |
| `packages::running()` — the executing package, copied into a 64-byte `Name` by `run_app` | `picodroid-core/src/packages.rs:123,562-569`, `boot.rs:289` | the sandbox's key |
| `Context.getPackageName` and the `PackageManager` natives, one value per call, `intern_dyn` for strings | `picodroid-core/src/native_handler/os.rs:30-99` | the pattern for every new native |
| `ATYPE_REF` arrays store `encode_ref(Value)` slots | `jvm/src/array_heap.rs:36-58` | `File.list()` returns a `String[]` from one native |
| `PlatformHooks` + `set_platform_hooks!`: six `__pd_host_*` hooks | `picodroid-core/src/host.rs:119-150` | gains `uninstall_run` (no new seam item) |
| Flash primitives park core 1 per call; **at most one flash op in flight** — the fs worker and the install's JVM park uphold it | `platforms/rp/src/hal/rp/flash.rs:199-216`, `core1_park.rs` | why the Java-side erase is serialised on the fs worker |
| PDB and the JVM share core 0; `uninstall` = park → `erase_run` → `rescan_region`; `run_uninstall` adds the reset | `platforms/rp/src/pdb/coordinator.rs`, `picodroid-core/src/install/orchestrator.rs:345-380`, `pdb/packages.rs:93-131` | the pdb path additionally wipes `/data/<pkg>` |
| Sim region verbs: `do_uninstall` over `MemRegion`, deferral when the target is running | `picodroid-core/src/hal/sim/app_region.rs:203-255` | the sim's `uninstall_run` |
| Layout tunables from MCU + board toml → `flash_layout.rs` consts and the `has_multi_app` cfg | `build_support/flash_layout.rs:97-146,253-260` | `fs_system_reserve_kb`, `app_data_cap_kb` |
| `MULTI_APP_CLASSES` excluded on single-app boards, mirrored in Gradle | `build_support/board_cfg.rs:66-118`, `buildSrc/.../ApiContract.kt:174-213` | the new classes join the list |
| Build plumbing knows only the launcher: pre-commit prologue, `hil_build_firmware`, the sim-run launcher lane; `build_system_apks` itself discovers `system-apps/*` | `scripts/pre-commit:253-255,410-412`, `hil-run.sh:544-564`, `sim-run.sh:274-376`, `lib.sh:613-642` | generalised to every system app |
| Ratchet: rp2040 893,243 B release; on 2026-09-08 the rp2040 **debug** builds stood at 913,083 (plain) and 915,355 (`handle-table-32` leg) of 917,248 B | `bench/parity/ratchet.toml`, `lib.sh::PROGRAM_FLASH_MAX`, `build/pre-commit/2026-09-08T181701/arm6.log` | the constraint on what single-app boards may gain; why Stage 0 exists |

### 8.2 Decisions

**P1 — The sandbox is on every board, at the natives.** Every `picodroid/io`
native maps the app's path before it reaches `backend::*`: strip the leading
`/`, split on `/`, drop empty and `.` segments, reject `..` (the operation
fails the way `HalFs` reports failure: `false`, `-1`, or `IOException` on
the write path), and prefix `/data/<package>/`. The app's `/` is
`/data/<package>`; `renameTo` maps both ends, so no path can leave the
directory. The buffer is 256 bytes on the native's stack — `/data/` + a
64-byte name + the path — so an app path is at most 184 bytes; longer
fails. A run with no package name (a PAPK from before M0, single-app boards
only) keeps today's root: the mapping is the identity when `running()` is
`None`. The package directory (and `/data`) is created on the first
creating operation of a run, once per run, flagged in a static that
`set_running` clears. Single-app boards pay the mapping and nothing else
(P3); their apps keep one API with everything else. Existing files at the
root of a dev board become invisible to apps — release note, no migration.

**P2 — The package's view mirrors Android's shape.** `Context.getDataDir()`
is `/` (the package root), `getFilesDir()` is `/files` (created on the
call, as Android guarantees), `openFileOutput(name, mode)` writes
`/files/<name>` (`MODE_PRIVATE` or `MODE_APPEND`; a name containing `/`
throws `IllegalArgumentException`), `openFileInput`, `fileList()` and
`deleteFile()` work on the same directory. `SharedPreferences` keeps
`/prefs/<name>` (`SharedPreferences.java:19`) and lands under the package
untouched, as D10 says. A directory costs LittleFS a metadata pair (8 KB),
so an app using both is 24 KB of metadata: written in limits.md, accepted.

**P3 — The quota is a multi-app feature; `StatFs` and `Build` are not.**
The reserve, the cap, the usage walk, the orphan sweep,
`StorageStatsManager`/`StorageStats` and `PackageInstaller` are
`cfg(has_multi_app)` / in `MULTI_APP_CLASSES`: single-app boards have no
system packages to protect and no second app to fence off. `StatFs` and
`Build` are ordinary Android APIs and ship on every board (owner decision,
§8.9); on a single-app board `StatFs` reports the raw volume, there being
no cap. Stage 0 (§8.5) makes the rp2040 room for them.

**P4 — Accounting.** `usage(package) = Σ ceil(size / 4096) × 4096 over its
files + 8192 per directory, the package directory included`. It is walked
lazily at the first storage operation of a run (not in `run_app`: an app
that never touches storage pays nothing) and kept as deltas after: a write
that grows a file by `blocks(new) − blocks(old)`, `truncate`, `delete`,
`mkdir` (+8 KB), `rename` (0 inside one package). The walk uses
`HalFs::list_dir` from core, one worker round trip per directory, depth
bounded at 4. The reserve check is FS-wide: `HalFs::space()` returns
`(total, free)` from `fs_size()`, and a growing write by an installed app
is refused when `free − growth < FS_SYSTEM_RESERVE`; system packages are
exempt from both rules. Reads, deletes and truncates are never refused.

**P5 — Refusals are `IOException`s.** Today a failed backend write is
`Err(JvmError::InvalidReference)` (`io.rs:172`) — a VM fault, not something
an app can catch. M3 turns every write-path failure into
`java.io.IOException` (quota: `"no space left on device"`; the rest: the
op and path), and `FileOutputStream.write(...)` (all three overloads),
`Context.openFileOutput/openFileInput` and `File.createNewFile()` declare
`throws IOException`, as `java.io` does. The ripple: `SharedPreferences.java`
wraps its writes (commit returns `false`), `bootcount` (`BootCount.java:46`),
`prefs_demo` (two `createNewFile` calls) and the new `filesdemo`; bugbash
only reads, and Kotlin callers need nothing. `FileNotFoundException` is not
served (`sdk/api-contract.tsv` has `IOException` and
`InterruptedIOException` only), so the two `open*` methods declare the
parent; an app catching `IOException` compiles unchanged — noted in the
storage doc.

**P6 — Uninstall from Java.** `PackageManager.getPackageInstaller()` returns
the `PackageInstaller` singleton; `uninstall(String packageName)` is one
native, `cfg(has_multi_app)`. Core does the checks — not installed, a
system package, or the running package → `IllegalArgumentException` with a
message — then calls the new `PlatformHooks::uninstall_run(first_sector,
sectors) -> bool`, wipes `/data/<package>` (generic over `HalFs`: `list_dir`
deepest-first, `delete` each), and returns. The RP implementation submits
the erase to the fs worker (`fs::exclusive(f)`, a `WORKER.submit` beside
`with_fs`) so the single-flash-op invariant holds by construction — the
JVM task blocks on the worker exactly as a `File.delete()` does — erases
with a fresh `RpPapkFlash`, and rescans; no reset. The simulator's
implementation erases the `MemRegion` run and rescans, the tail of
`app_region::do_uninstall`. The screen freezes for the erase (≈45 ms a
sector; a 50 KB app is under a second). A `PackageManager` index is stale
after this; the Java side re-resolves by name on every call (`nativeIndexOf`),
so `PackageInfo` objects stay usable, and the launcher rebuilds its rows
at every `onCreate`. A `BitmapDrawable` of the uninstalled package would
read erased flash (0xFF, harmless) — the settings app shows no icons.

**P7 — The settings app.** `system-apps/settings` (package
`picodroid.settings`, Java package `settings`, label "Settings", a 48×48
icon), four screens, every one a column of 40 px focusable rows under a
40 px header row whose tap (or BACK) goes up one level:

| Screen | Rows |
|---|---|
| Settings (root) | About, Apps, Storage; the header row is Home: `finish()` returns to the launcher |
| About | board, MCU, firmware version, map version (from `Build`), storage total/used/free (`StatFs`), used heap (`Runtime`) |
| Apps | one row per installed non-system app: label, version; a tap opens an `AlertDialog` (title = the label, message `Remove app and data?`, buttons `Uninstall` / `Cancel`); `Uninstall` calls `PackageInstaller.uninstall` and rebuilds the list |
| Storage | one row per package (system apps included): `<label>  app <KB> / data <KB>` from `StorageStatsManager` |

The dialog is the framework's own (`lvgl/widgets/alert_dialog.rs`): a fixed
200 px card on a 240 px scrim, 80 px buttons, the positive one focused by
default on keypad boards. Its title and message stay one line each so the
geometry never varies, and the bench taps the positive button at
coordinates pinned once from a simulator screenshot (§8.6). Rows are 40 px
from the top, header first, so row *n*'s centre is `y = 20 + 40n`. Log
lines the harness keys on:
`Settings: ready`, `Settings: apps <n>`, `Settings: uninstalled <pkg>`,
`Settings: storage <pkg> <appBytes> <dataBytes>`.

**P8 — Data goes with the package.** Every uninstall path wipes
`/data/<package>`: `pdb uninstall` (`pdb/packages.rs::handle_uninstall`,
through `with_fs` from the comm task before the reset), the sim's
`apps uninstall`, and P6. A boot sweep (`main.rs` after `fs::init`, the
sim after `app_region::init`; multi-app boards) removes every `/data/*`
directory that names no installed or system package — a power loss between
erase and wipe, or an app removed by reflashing. `/system/` is reserved:
nothing under `/data` can reach it, and nothing in M3 writes it.

**P9 — The sim's filesystem is the board's.** `storage_host.rs` takes its
size from `board_cfg::flash::FS_LEN` (512 KB on the rp2350 boards, 128 KB
on the testbench_rp2040) unless `PICODROID_SIM_FS_KB` overrides it, and an
existing image of another size is recreated with a log line rather than
failing to mount. So `StatFs`, the cap and the sweep behave in the sim as
on the device.

### 8.3 Java surface

Everywhere:

```java
// picodroid.content.Context
public static final int MODE_APPEND = 32768;
public File getDataDir();
public File getFilesDir();
public FileOutputStream openFileOutput(String name, int mode) throws IOException;
public FileInputStream openFileInput(String name) throws IOException;
public String[] fileList();
public boolean deleteFile(String name);
// picodroid.io.File
public native String[] list();          // null when not a directory
public File[] listFiles();
public native boolean createNewFile() throws IOException;   // was boolean, silent
// picodroid.io.FileOutputStream — write(byte[],int,int), write(byte[]), write(int) throw IOException
```

Multi-app boards (`MULTI_APP_CLASSES`):

```java
// picodroid.os.StatFs — StatFs(String path), restat(String); the path is accepted and ignored (one volume)
public long getTotalBytes(); public long getFreeBytes(); public long getAvailableBytes();
public long getBlockSizeLong(); public long getBlockCountLong(); public long getAvailableBlocksLong();
// getAvailableBytes() is what this app may still write: min(free − reserve, cap − usage); system apps see free.
// picodroid.app.usage.StorageStatsManager — Context.getSystemService(Context.STORAGE_STATS_SERVICE)
public StorageStats queryStatsForPackage(String packageName) throws PackageManager.NameNotFoundException;
// picodroid.app.usage.StorageStats — getAppBytes() (the run's sectors × 4096), getDataBytes() (P4 usage), getCacheBytes() = 0
// picodroid.content.pm.PackageInstaller — PackageManager.getPackageInstaller()
public void uninstall(String packageName);   // IllegalArgumentException: not installed / system / running
// picodroid.os.Build — BOARD, HARDWARE (mcu), VERSION.RELEASE (firmware package version), VERSION.INCREMENTAL (framework map version)
```

`queryStatsForPackage` drops Android's `UUID` and `UserHandle` parameters
(one volume, one user); `PackageInstaller.uninstall` drops the
`IntentSender` (synchronous). Both divergences go in the API docs.

### 8.4 Seams

```rust
// picodroid-core::hal::HalFs (hal/traits.rs) — two methods, no new trait: seam count stays 42
fn list_dir(path: &str, out: &mut Vec<DirEntry>) -> bool;   // DirEntry { name: String, dir: bool, size: u32 }
fn space() -> (u64, u64);                                   // (total, free) bytes; (0, 0) when unavailable

// picodroid-core::host::PlatformHooks (+ __pd_host_uninstall_run in set_platform_hooks!)
fn uninstall_run(first_sector: u32, sectors: u32) -> bool;

// picodroid-core::fs (feature = "littlefs")
pub fn exclusive<R>(f: impl FnOnce() -> R + Send) -> R;    // run f on the fs worker; inline pre-scheduler

// picodroid-core::native_handler::io (io.rs becomes io/{mod,backend,sandbox,quota}.rs)
sandbox::resolve(package: Option<&str>, path: &str, buf: &mut [u8; 256]) -> Result<&str, Rejected>;
quota::charge(delta_blocks: i32) -> Result<(), Full>;  quota::usage(package) -> u64;  quota::reset();
storage::wipe_package(package);  storage::sweep_orphans();

// picodroid-core::board_cfg::flash (generated): FS_SYSTEM_RESERVE_BYTES, APP_DATA_CAP_BYTES (0 = unlimited)
// picodroid-core::packages: no change; running()/find()/is_system()/rescan_region() are enough
```

Board and MCU keys (`build_support/flash_layout.rs`): `fs_system_reserve_kb`
(MCU default 64) and `app_data_cap_kb` (default: a quarter of `fs_kb`; `0`
= unlimited). The generator asserts `reserve < fs_kb` and
`cap ≤ fs_kb − reserve` (or 0). Both keys are documented in
advanced-config.md; a product board that wants one big app sets the cap
to 0.

### 8.5 Stages

| # | Stage | Scope | Guards and tests | Checkpoint |
|---|---|---|---|---|
| M3-0 | rp2040 C at `-Os` (own PR, first) | `c_opt_level = "s"` in `rp2040.toml`, applied by `config::apply_c_opt_level` to LVGL, the kernel and the network C for that MCU's target only; ratchet re-baselined | rp2350 bytes unchanged proves the scope; `--full` | DONE 2026-09-08 on `feat/rp2040-c-os`: release 893,243 → 802,679 B, debug 913,083 → 822,616, `handle-table-32` 915,355 → 824,888; RAM and every RP2350 image unchanged; `-fno-jump-tables` rides along (rust-lld links no libgcc). The rp2040 HIL matrix waits for a board on the bench |
| M3a | Sandbox + files API (all boards) | `io/` split, `sandbox.rs` + unit tests (mapping table: `..`, empty, `.`, trailing `/`, too long, no package = identity); `HalFs::list_dir` in `LittleFsHal` and the test map; `File.list/listFiles`; the P2 `Context` methods; P5 (`IOException`, `throws`, ripple); P9; `examples/filesdemo` | `method_tables.rs` rows, `class_registry`, api-contract regen (`scripts/gen-api-contract.sh`), `no_original_name_literals` (path words that collide with served member names become `build_support/names.rs` consts, as `BOOT_APP` did) | **both rp2040 debug legs measured** before M3b starts (§8.7) |
| M3b | Quota + stats (multi-app) | keys → consts; `quota.rs` (walk, deltas, reserve, cap) + tests over the test map; `HalFs::space`; `StatFs` and `Build` (every board; `Build`'s natives read the board and MCU names from board_cfg, `CARGO_PKG_VERSION`, `FRAMEWORK_MAP_VERSION`); `StorageStatsManager`/`StorageStats`; P8 wipes + boot sweep; `MULTI_APP_CLASSES` + `ApiContract.kt` mirror; `examples/quotademo` | quota tests: fill to the cap → `IOException`, delete frees, mkdir counts 8 KB, system exempt, reserve refuses before the cap when the FS is nearly full; sweep test over the test map | sim: `quotademo` passes with the board's 512 KB image |
| M3c | Uninstall from Java | `PlatformHooks::uninstall_run`, `fs::exclusive`, RP + sim impls; `PackageInstaller` + `getPackageInstaller()`; the three refusals; wipe | `packages`-style test over `MemRegion` (uninstall → rescan → `find` is `None`, data gone); the sim verb and the native share `do_uninstall`'s tail | sim: `apps list` after a Java uninstall |
| M3d | Settings app | `system-apps/settings` (P7); `build_system_apks --prebuilt-dir DIR` builds every `system-apps/*` PAPK into `DIR` and is what pre-commit, `hil_build_firmware` and sim-run call; the launcher lane's `ready: 1 apps` becomes `ready: 2 apps` | the existing map-version check covers both PAPKs; `check-shrunk-image.sh` (Java package `settings`, never `picodroid.*`) | sim: launcher → Settings → About/Apps/Storage → Home, screenshots |
| M3e | Harness, docs, release | §8.6 rows and lanes; §8.8 docs; release notes; ratchet `size:` trailers; `pre-commit --full`; bench | `--full` green except the expected `shrink_image` leak (new classes until the cut) | bench on testbench_rp2350 (all rows) and testbench_rp2040 (`filesdemo`, `bootcount`, `prefs_demo`); merge; then `release(shrink): cut map v0.22.0` on main on the owner's go-ahead |

Order matters three times: Stage 0 before everything, because M3a does not
fit the rp2040 without it (§8.7); M3a next because it is the stage that
changes single-app boards, so the rp2040 number is known before anything
else is built on it; M3c before M3d because the settings app's Apps screen
is otherwise untestable.

### 8.6 Harness

Sim and bench rows in `scripts/hil-tests.conf`:

```text
filesdemo|term|60|FilesDemo[]:] === ALL PASSED ===
quotademo|term|120|QuotaDemo[]:] === ALL PASSED ===|rp2350
helloworld|pdb|300|running: picodroid.settings;Settings: uninstalled helloworld;free: largest|settings-uninstall
```

`filesdemo` (every board): `getFilesDir`, `openFileOutput` + `openFileInput`
round trip, `MODE_APPEND`, `fileList` and `File.list` contents, `deleteFile`,
`mkdirs` two deep, `renameTo`, and the refusals — `new File("../x")
.createNewFile()` throws, `new File("/data/other/x").exists()` is false
after writing it (it landed under this package), a 200-byte path fails.
`quotademo` (rp2350 boards): `StatFs` numbers sum, writes until
`IOException` at the cap, `delete` restores `getAvailableBytes()`,
`StorageStats.getDataBytes()` matches P4's rule, `mkdir` costs 8 KB.
`settings-uninstall` (multi-app firmware, touch panel; SKIPped otherwise):
flash helloworld `--boot launcher`, clear other packages as `launch` does,
then `tap 120 60` (Settings is row 1: installed apps sort first), wait
`Settings: ready`, `tap 120 100` (Apps), wait `Settings: apps 1`,
`tap 120 60` (the first app row), then the dialog's `Uninstall` button at
the pinned coordinates, expect `Settings: uninstalled helloworld`, then
`pdb list` without a helloworld row and
`running: picodroid.settings`. The `list` row's pattern gains
`picodroid.settings`; `launch` and `launch-soak` are unchanged (row 0 stays
the app). `sim-run.sh` gains `run_settings_smoke`, the same taps over the
control FIFO (its screenshot is where the dialog's button coordinates are
pinned from), and `run_launcher_smoke`'s count pattern moves to 2. The
bootcount persistence recipe (+1 per reflash, +2 per power cycle) holds:
the file is `/data/bootcount/bootcount` now, same package every time.

### 8.7 Flash and RAM

rp2350 (2,048 KB region, 1,004,755 B release at M2): expect about +20 KB —
the settings PAPK (10–12 KB stripped), five classes, the quota and
`Build`/`StatFs`/stats natives — accepted in the ratchet with `size:`
trailers. rp2040: M3a, `StatFs` and `Build` land there — the mapping, `list_dir`,
`File.list`, the `Context` methods, the `IOException` path, two small
classes — about 3.5–4 KB on the **debug** images by the stripped-corpus
yardstick (`File.class` 1,290 B for 13 methods, `SystemClock.class` 237 B
for 3 natives). The `--full` run of 2026-09-08
(`build/pre-commit/2026-09-08T181701/arm6.log`) put the plain debug build
at 913,083 and the `handle-table-32` leg at 915,355 of 917,248 B — 1,893 B
left — so M3 does not fit without Stage 0: the C code (LVGL, the kernel)
at `-Os` for the rp2040 only, measured at −81.5 KB on rp2350 in
`flash-budget-2026-09.md` §6.1 with every Rust bucket byte-identical. Its
cost is render throughput, on a dev board. Landed 2026-09-08: −90.5 KB on
every rp2040 image (release 893,243 → 802,679 B; 92 KB of headroom on the
tightest debug leg). M3a still ends with both debug legs measured. RAM: a few statics (usage, the
ensured-directory flag), a 256-byte path buffer on the JVM task's stack
inside the natives, and at most four nested `list_dir` round trips; the
fs worker's 8 KB stack (`FS_STACK_WORDS`) is untouched. Main-stack
headroom on rp2350 (17,800 B at M2) does not move.

### 8.8 Docs

`api/storage.md` (the sandbox, the `Context` file API, `File.list`, the
quota, `StatFs`, `StorageStatsManager`, path limits, the `IOException`
change, the `FileNotFoundException` note); `api/system.md` (`Build`,
`PackageInstaller`); `guides/launcher.md` (a Settings section; going Home
on a touch board); `reference/advanced-config.md` (the two keys);
`reference/limits.md` (184-byte paths, 8 KB per directory, the defaults);
`reference/pdb-commands.md` (`uninstall` wipes data; `list` shows both
system rows); `reference/porting-guide.md` (`HalFs::list_dir`/`space`,
`PlatformHooks::uninstall_run`, `fs::exclusive`); `examples.md`
(`filesdemo`, `quotademo`); release notes (root files on dev boards are no
longer visible to apps, the sim image is recreated at the board's size).

### 8.9 Owner decisions (2026-09-08)

Asked with a recommendation each, answered the same day:

1. **Sandbox on single-app boards too** (P1) — yes.
2. **`StatFs` and `Build`** (P3) — every board; `StorageStatsManager`,
   `StorageStats` and `PackageInstaller` stay multi-app only.
3. **`getFilesDir()` is `/files`, an 8 KB directory** (P2) — yes.
4. **Uninstall confirmation** (P7) — an `AlertDialog`, not a two-tap row;
   the harness taps its positive button at pinned coordinates.
5. **`throws IOException` on the write path** (P5) — yes.
6. **The `-Os` lever** (§8.7) — before M3, as its own PR, rp2040 only
   (Stage 0 in §8.5). *Later the same day the owner extended it to the
   rp2350 and dropped the C frame pointers on both MCUs, as a flash-size
   pass in its own right (`flash-budget-2026-09.md` §6.1 status): rp2350
   release 1,004,755 → 911,827 B, rp2040 802,679 → 783,547 B.*

### 8.10 Deferred from M3

`getCacheDir()` (another 8 KB directory nobody asked for), `FileNotFoundException`
(a java/** class, pico-jvm side), `File.lastModified()` (LittleFS keeps no
times), a `data` column in `pdb list`, a BACK affordance for touch boards
(still open from A2), a runtime boot-app override in Settings (needs the
`/system/settings` store, §6), migration of pre-M3 root files, and
permissions (S9 of the roadmap).

## Amendments

### A1 (2026-09-07) — what M1 changed against the body

- **Stale runs are occupied space** (D3/D5). The body said a commit-less
  run is "not free space either until cleanup" without saying why: a new
  run placed over a commit-less header would be hidden by it at the next
  scan, because the header's span skips past the new run's meta sector.
  `packages::sorted_runs` therefore counts the stale runs the last scan
  found as occupied; `cleanup` (boot, and the sim's init) erases them and
  rescans. Pinned by `a_commit_less_run_is_stale_and_cleanup_erases_it`.
- **Sequence numbers during compaction.** `plan_install` reserves a `seq`,
  compaction consumes one per moved run, and the re-plan reserves the next,
  so an install that compacts first lands with `seq` one higher than the
  moved runs — monotonic, not the body's implied "the install's seq is
  fixed before compaction". Pinned by
  `fragmented_free_space_compacts_then_installs`.
- **`PapkFlash` carries `max_installed_apps`** and `plan_install` takes it
  as a parameter, so the single-app rule (replace whatever is installed) is
  a runtime policy the tests exercise with capacity 1 and 8 in one binary,
  while the directory's static capacity stays the generated
  `MAX_INSTALLED_APPS`.
- **The greeting's apps tail is in bytes**, not KB, and `pdb ping` prints
  `apps N/M, free T KB (largest L KB)`; the HIL harness keys its SKIP for
  the package rows on that text.
- **No `run_install` in the simulator**: `install`/`uninstall` (everything
  but the reset) exist for it and for the tests; the device's `run_*`
  wrappers add the reset.
- **The boot log's `[packages]` line** is `sector N: <package> <version>
  (<code>) <bytes> bytes [boot]`; the sim adds `[sim] apps:` lines from its
  control verbs.
- **Flash cost, release images (the ratchet)**: rp2350 +21,712 B for the
  whole feature (directory, placement, compaction, `list`/`uninstall`, the
  greeting tail); rp2040 +5,304 B for what a single-app board keeps — the
  region scan with its structural validation, cleanup, the single-app plan
  and the two-page boot-meta — after `has_multi_app` gated the rest (the
  multi-app placement, first-fit, compaction and the formatted no-room line
  were another 5.4 KB before the gate). M0's boot-meta v2 was +197 B on
  rp2040; the layout generator itself was proven byte-identical on rp2040
  (one differing byte, the shifted line number of `main.rs`'s `expect`).
  The rp2350 debug image is 981,952 B of the 2,048 KB program region with
  17,800 B of main-stack headroom.

### A2 (2026-09-07) — what M2 changed against the body

Written in plain language. Each point says what the code does now and why
it differs from the text above.

- **Launcher only.** M2 ships `system-apps/launcher`. The settings app is
  M3 work, as the stage table says; D11's "two system apps" stays the
  target. `SYSTEM_MAX = 2` is unchanged.
- **Where system apps come from.** `scripts/lib.sh::build_system_apks`
  builds every `system-apps/*` PAPK with the same flags as the app under
  test and exports `PICODROID_SYSTEM_APKS` (a colon-separated list) and
  `PICODROID_BOOT`. `picodroid-core/build.rs` reads them and writes
  `system_apks.rs`: the images as 4-byte aligned `.rodata`, the launcher's
  package name, and the boot override. Both variables are exported even
  when empty, because `flash.sh` runs cargo twice and an unset-then-set
  variable would rebuild the firmware without the launcher and flash that.
  The launcher's Java package is `launcher`, not `picodroid.launcher`: a
  `picodroid/…` class name inside the embedded PAPK would fail the
  shrunk-image check.
- **A launcher must match the firmware's map.** The build checks each
  system app's `framework-map-version` against the firmware's and fails on
  a mismatch, so a launcher that `verify_compat` would refuse can never be
  linked in.
- **Boot order (D7)** is `packages::select_boot`: the `--boot` override,
  the board's `boot_package`, the run flagged `BOOT_DEFAULT`, the launcher,
  the lowest run. `flash.sh --boot app` means the baked run; a name that is
  not installed is skipped with a warning. The simulator reads
  `PICODROID_BOOT` at run time instead of baking it in, so switching needs
  no rebuild. The two words are generated into `system_apks.rs`
  (`BOOT_APP`, `BOOT_LAUNCHER`): `app` is also a served member name, and
  the no-literal guard would flag it spelled out in core.
- **After an app exits (D11).** The body said "else the same image again".
  The code does this instead: `packages::next_image()` returns the package
  a cross-package `startActivity` asked for; else the launcher, when the
  app that exited was not it; else nothing, and the supervisor waits for an
  install exactly as a single-app board does. Re-running a finished app in
  a loop was never useful. A launcher that exits is started again once; a
  second exit in a row is treated as a fault and the device waits for an
  install, so a broken launcher cannot loop. An app stopped for an install
  keeps its image, so a refused install resumes the same app.
- **How a launch leaves the app.** `Intent.setPackage` sets a new field
  (slot 6, declared last; a test pins the slot). The `startActivity`
  native records the target with `packages::request_launch` and queues
  `PendingActivityOp::Launch`, which the lifecycle loop answers with
  `Break`: every Activity gets onPause, onStop and onDestroy, services are
  destroyed, and `run_app` returns. A package that is not installed throws
  `ActivityNotFoundException`. The name is kept, not the image, so a
  reinstall in between launches the new copy.
- **Threads of the leaving app.** A launch and a natural exit raise no
  `STOP_JVM` by themselves, so the device supervisor raises it when Java
  threads are still alive after `run_app` returns; the simulator has its
  own `STOP_JVM` flag behind `stop_requested()`. One app runs at a time.
- **The simulator.** `sim_boot` runs the same loop as the device (boot
  image, stop what the app left, ask `next_image`), exits when the answer
  is nothing, and boots from the package directory rather than a second
  copy of the app (`apk_data()` and `embed_apk` are gone). `sim.sh
  --system-apps` opts in; `sim.sh --app X` alone still runs one app and
  exits. A package verb that names the running app stops it first
  (`STOP_JVM`) and runs after it is gone: a reinstall launches the new
  copy, an uninstall gives way to the launcher.
- **Per-board classes.** `MULTI_APP_CLASSES` (`PackageInfo`,
  `ApplicationInfo`, `PackageManager$NameNotFoundException`,
  `BitmapDrawable`) are dropped on single-app boards, mirrored in the
  Gradle contract check (a board is multi-app when its own board.toml sets
  `max_installed_apps` above 1; the MCU defaults are 1). Every new
  `PackageManager` and `BitmapDrawable` native arm is `cfg(has_multi_app)`.
  `Context.getPackageName` and `ActivityNotFoundException` ship everywhere.
- **Icons across packages.** `assets::register_icon` boxes a descriptor
  that points into the other package's image in place (flash or `.rodata`)
  and hands `BitmapDrawable` a handle; `assets::clear` now frees the
  descriptors instead of leaking them, because app switching makes the
  reset a steady-state path. A focusable view also scrolls into view when
  it takes focus (`LV_OBJ_FLAG_SCROLL_ON_FOCUS`), so the launcher's column
  can be walked with the buttons.
- **`pdb list`** adds a `system` row per system app and a `running` line
  naming the executing package. Text only, so an older host ignores them;
  the protocol stays `picodroid/2.2`. `LIST_BUF` is 1280 bytes.
- **Touch-only boards have no BACK.** An app started from the launcher on
  such a board must finish itself; otherwise it runs until the next
  install or reset. Open: a BACK affordance for touch boards.
- **Harness.** `hil-tests.conf` gains `launch` (blinky with `--boot
  launcher`, `pdb input tap 120 20` on row 0, `running: blinky`) and
  `launch-soak` (helloworld, twenty taps, the free heap must not drift),
  both SKIPped on single-app firmware or without a touch panel; the `list`
  row expects the `SYSTEM` row and `running:`. Every bench firmware links
  the launcher (`hil_build_firmware`), and every new probe-rs and pdb call
  takes stdin from `/dev/null` (the 734be0f rule). `sim-run.sh` gains a
  launcher lane driven over the control FIFO. The pre-commit prologue
  builds the launcher once (`PICODROID_PREBUILT_SYSTEM_APKS`).
- **A fresh flash wins (D3).** The body says the higher `seq` wins when
  two runs name the same package. That made a freshly flashed app lose to
  an older copy `pdb install` had left further up the region — with a
  higher sequence, and on the bench built for the other shrink mode, so it
  failed `verify_compat` and the device fell through to the launcher. The
  baked run (sector 0, sequence 0; an install never writes sequence 0)
  now wins a duplicate outright, and the old copy is erased at cleanup.
  Pinned by `a_fresh_bake_beats_an_older_install_of_the_same_package`.
- **Flash cost (release images, the ratchet).** rp2350 +27,600 B: the
  launcher PAPK (about 9.5 KB stripped), the new SDK classes, the
  PackageManager and BitmapDrawable natives, the icon registry and the
  switching loop. rp2040 +7,988 B for what a single-app board keeps: the
  larger `PackageManager`, `Intent` and `Context` class files,
  `ActivityNotFoundException`, `getPackageName`, the system-entry and boot
  selection code and the supervisor loop; its release image is at 893,243
  of 917,248 bytes. RAM is unchanged on both. Recorded in
  `bench/parity/ratchet.toml` with this change's `size:` trailers.

### A3 (2026-09-08) — what M3 changed against §8

Written in plain language, as A2 was. Each point says what the code does
now and why it differs from §8.

- **Stage 0 first, and the numbers it bought.** The rp2040's `-Os` C
  (`c_opt_level = "s"`, `-fno-jump-tables` for the Thumb-1 libgcc helpers
  rust-lld cannot supply) landed on main as `73665a4` before M3 started:
  release 893,243 → 802,679 B. M3 then cost the rp2040 debug image
  +10,700 B in all (M3a +7,180: the `Context` methods, `File.list`, the
  sandbox, the `IOException` path, `list_dir`; M3b +3,312: `StatFs`,
  `Build`, the quota's single-app stubs; M3c +208) and the
  `handle-table-32` leg the same; RAM +8 B. About three times §8.7's
  estimate for M3a, well inside the room Stage 0 made.
- **Where the sandbox lives.** Not under `native_handler` (which is
  `cfg(not(test))`) but in a crate-level `storage/` module —
  `storage::sandbox` (the mapping, `ensure_package_dir`),
  `storage::quota`, `wipe_package`, `sweep_orphans` — reached from the
  `picodroid/io` natives, which became `native_handler/io/mod.rs`. The
  natives' own in-memory test backend is gone: every build reaches
  `crate::hal::fs`, the test build through `TestHal` (which gained
  `list_dir` and a 512 KB `space()`). The io natives' tests still run
  through the `#[path]` shim in `lib.rs`, so `throw_io` is local to the
  module (in the shim, `super` is the crate root).
- **The app-path cap is 185 bytes**, not 184: `/data/` + a 64-byte
  package name + the `/` the mapping adds + the path must fit 256.
- **The package directory and the run counter.** `packages::run_generation()`
  (bumped by `set_running`; a load and a store, the Cortex-M0+ having no
  read-modify-write atomics) keys both the sandbox's "directory made"
  flag and the quota's "walked for this run" flag, instead of a hook from
  `packages` into the sandbox. When the sandbox makes the directory it
  invalidates the quota's walk: quotademo caught a walk that had run
  before the directory existed (a `StatFs` call first) and never counted
  its 8 KB, so eight blobs fit instead of seven.
- **`HalFs` gained two methods**, `list_dir` and `space`, no new trait
  (the seam count stays 42); the LittleFS impl records the volume's block
  count at mount for `space()`.
- **Where the quota is enforced.** `FileOutputStream.write` charges the
  growth before the bytes land, so a refused write leaves the file as it
  was (`IOException("storage cap reached")` / `("no space left on
  device")`); truncate and delete credit; `mkdir` charges its 8 KB pair
  first and answers `false` when refused; `createNewFile` costs nothing.
  The counter's read-modify-writes sit in the JVM's `AtomicSection`. A
  system package is exempt but counted.
- **`Build.VERSION.RELEASE` is the framework map version** and there is
  no `INCREMENTAL`: core carries one version. `BOARD` and `HARDWARE` come
  from a new generated `build_info.rs` (`host` for a boardless build).
  *Revised 2026-09-09 (QA): `RELEASE` is the firmware's package version,
  generated into `build_info.rs` beside the board. The map version is the
  `0.0.0` sentinel on every unshrunk build — the default `flash.sh` image
  — and Settings > About read "Release 0.0.0"; the two coincide on a
  `--shrink` image, where the map is cut for the release.*
- **`StorageStatsManager` is reached through `getSystemService`** with
  `Context.STORAGE_STATS_SERVICE = "storagestats"`; `Context` names the
  multi-app-only class in bytecode, which a single-app board tolerates
  the way it tolerates `PackageManager`'s query methods.
- **Uninstall from Java.** `PlatformHooks::uninstall_run(first, sectors)`
  (no new seam item). The RP family runs the erase on the LittleFS worker
  through `fs::exclusive` and rescans back on the JVM task; the simulator
  erases its region on the JVM task; the test platform answers `false`.
  `packages::uninstall_target` holds the decision (not installed / system
  / running) and is unit-tested; Java gets `IllegalArgumentException` for
  those and `IllegalStateException` when the platform could not.
- **`pdb uninstall` wipes before the reset**: the handler calls
  `uninstall` then `storage::wipe_package` then `trigger_reset`, instead
  of `run_uninstall`.
- **The simulator's boot sweep runs only with `--system-apps`**: a plain
  `sim.sh --app X` installs X alone, and sweeping every other app's data
  on each such run made `bootcount` restart at 1 in the verification
  chain. The device sweeps at every boot.
- **The literal guard's prose list gained `list`, `install`,
  `uninstall`**: the simulator's control-channel verbs share spellings with
  `File.list` and `PackageInstaller.uninstall` but are FIFO text, never a
  Java name. A related trap: the guards stop reading a source at its
  first `#[cfg(test)]` before a `mod` line, so a `#[cfg(test)] mod x;`
  near the top of a dispatcher blanks it for them.
- **`defmt` cannot format a `String`**: the sweep logs `name.as_str()`.
- **The settings app** is four Activities over one `Screens` helper
  (40 px header + 40 px rows, `GradientDrawable` header band), no icons,
  and its Apps screen rebuilds itself from the main executor after the
  dialog closes. The dialog's Uninstall button on the 240-px boards is
  tapped at (160, 118) in LVGL pixels — not pinned from a screenshot as
  §8.6 planned (a tap over the control channel never registered in the
  windowed simulator) but taken from the dialog's fixed geometry
  (`lvgl/widgets/alert_dialog.rs`: a 200 px card at y 40, 80 px buttons)
  so that it lands inside the button with or without the theme's row gap,
  and proven by the headless drive uninstalling helloworld through it.
- **Build plumbing** that named only the launcher — the pre-commit
  prologue, `hil_build_firmware` — now loops over `system-apps/*`;
  `sim-run.sh` gained the `settings` lane; the `list` row expects
  `picodroid.settings`; new rows `filesdemo` (every board), `quotademo`
  (rp2350 boards) and `settings-uninstall`.
- **Sizes (release, the ratchet), against main's baselines after its
  `-Os`-everywhere pass (`f105789`):** rp2350 911,827 → 947,891 B
  (+36,064: the settings PAPK, 18,340 B stripped, the six classes, the
  quota and the natives); rp2040 783,547 → 793,663 B (+10,116).
  RAM: rp2040 +8 B, rp2350 +0 B. Before that pass the branch measured
  1,004,755 → 1,040,819 B and 802,679 → 812,811 B.
- **The release** (2026-09-09, on main after the merge): package 0.22.0
  and map v0.22.0, cut with `class-shrink cut-release --members` from
  v0.21.0 — + 6 classes (249 → 255) and + 40 members (1125 → 1165),
  member floor still 0.17.0 — which clears the shrunk-image leak §8.5
  expected. `Build.VERSION.RELEASE` on a `--shrink` image reads `0.22.0`.

### A4 (2026-09-09) — the QA pass over v0.22.0

A full pass over the multi-app feature on the simulator (both boards,
headless and windowed with screenshots) and on the bench's Pico 2 W
(`pico_enviro_mon_w`), one commit per defect. What it found, in the order
it matters:

- **The keypad focus ring was reordered by `requestFocus`.** LVGL's
  `lv_group_add_obj` is not idempotent — it removes a member and re-appends
  it at the tail — so the settings root, which focuses its second row, had
  the ring `header, Apps, Storage, About` and "down, select" from About went
  Home. `set_view_focusable` / `request_view_focus` now add a view only
  when it is not a member (`lv_obj_get_group`).
- **A shown `AlertDialog` was not modal for the keypad.** Its buttons joined
  the Activity's group, so "down" walked out of the uninstall dialog onto
  the rows behind the scrim and "select" opened a second dialog for another
  app (and could uninstall it). A dialog now gets a modal group of its own
  (`events::enter_modal_group` / `leave_modal_group`) for as long as any
  dialog is shown.
- **Focus was invisible.** The theme's focus outline draws outside the
  object and a full-width row in a zero-padding column clips it away —
  the launcher and every settings screen showed no selection at all. A
  focusable view now also gets a 2 px light border, inside its bounds, in
  the focused states.
- **The dialog was a 240 px square on the 320 px testbench**: a tap beside
  the scrim reached the rows behind the dialog. The scrim is the display
  and the card is centred on it; the harness computes the Uninstall
  button's position from the board's width (`lib.sh::settings_dialog_ok`).
- **`Build.VERSION.RELEASE` read `0.0.0`** on every unshrunk image — the
  default `flash.sh` build — because it was the framework map version,
  whose sentinel that is. It is the firmware's package version now
  (`build_info::RELEASE`); the two coincide on a `--shrink` image.
- **Columns did not scroll.** A `LinearLayout` does not scroll (as on
  Android), so the launcher's seventh app and the Storage screen's rows
  past the fold were unreachable — by drag on the testbench, where a
  swipe over a row launched the app instead, and by the keypad on the
  Enviro. The launcher and every settings screen wrap their column in a
  `ScrollView`; the About and Storage rows are focusable so the keypad can
  walk and scroll them. Rows are one line (a long label is cut with an
  ellipsis, the numbers always shown), the Apps list is sorted like the
  launcher's, and both sort case-insensitively.
- **Simulator**: `apps install` read the PAPK under the simulated heap cap
  (an 800 KB app was "out of memory"), so compaction could never be
  exercised there; the package verbs were serviced only from an Activity's
  tick, so an `Application` with no Activity (blinky) could not be
  uninstalled or reinstalled; and an install under a running app could
  compact the region — move runs — while the interpreter held slices into
  it. Now the file is read outside the cap, the verbs are polled from the
  JVM task's stop hook too, and any install stops the running app first
  and brings it (or the reinstalled copy) back, the way a device resets —
  a launcher that comes back lists the new app. Refusals print in words.
- **The device kept naming the exited app as `running`** in `pdb list`
  while it waited for an install; the supervisor clears it.
- Cosmetic: the dialog's button row no longer has a frame, and the card no
  scrollbar.

Verified: every scenario above on both simulated boards, the `launcher`
and `settings` `sim-run.sh` lanes in both shrink modes, an 8-cycle
launcher/settings switching soak with no heap drift, `filesdemo`,
`quotademo`, the bootcount persistence rules (an upgrade keeps data, an
uninstall wipes it, the boot sweep removes an orphan) and
`pre-commit --full`; on the bench (`pico_enviro_mon_w`, `flash.sh --boot launcher`, six apps
installed over `pdb`), the keypad walk through every settings screen with
RTT attached — About reads `0.22.0`, the dialog holds the keypad, an
uninstall from Settings, BACK and Home — then the directory at 8/8: the
no-room refusal with its hint, an upgrade at capacity, the host's
too-large refusal, `pdb uninstall`, a 782 KB and a 1.2 MB install, and a
compaction (`compacting qa.fill.a: sector 199 -> 2`, the 1.2 MB run placed
behind it). Two observations for later: the Storage and Apps screens stall
the UI tick for 190–290 ms on the device — every `PackageManager` native
re-parses manifests from XIP and `StorageStats` walks each package's
directory, which D4's "no strings in the directory" makes the price of a
screen — and the SDK has no `TextView.setSingleLine`/`setEllipsize`
(a map cut), so the system apps cut labels by character count.

### A5 (2026-09-09) — one line, cut with an ellipsis

The second A4 observation is closed. The SDK has `TextView.setSingleLine`,
`setEllipsize(TextUtils.TruncateAt)` and `setMaxLines`, with their getters and
a new `picodroid.text.TextUtils`, over LVGL's label long modes — `DOTS` for an
ellipsis, `CLIP` for a bare single line, `SCROLL_CIRCULAR` for `MARQUEE` — and
a `max_height` cap of N lines, because LVGL puts its dots on the last line that
fits the box, and the cap holds for explicit heights too (a taller `setSize`
shrinks to the limit; the documented divergence). `getText()` still returns
the whole text: LVGL overwrites its own buffer with the dots, and the native
reverts them for the read. The Java state is one packed `int` per label; a
padding change re-applies the cap, which includes the padding. A private
native on `TextView` is an `invokespecial`, so a `Button` receiver reaches the
same arm and the impl resolves it to the child label.

The launcher row's label and every settings row are a weighted single-line
label beside the icon or the suffix, so the version and the storage numbers
always show; `Screens.fit` and the 7-px-per-character heuristic are gone. The
rows' geometry — 40 px, tapped at `y = 20 + 40n` — is unchanged, so the sim
and bench lanes are too. `examples/ellipsizedemo` pins the behaviour as a sim
lane (heights against a one-line reference, `getText()` under the dots, the
getters, lifting the limit, a padding change). The two `TextUtils` classes are
un-shrunk until the next map cut clears the one expected `shrink_image` leak.
