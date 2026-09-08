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
| M3 | NOT STARTED |

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
