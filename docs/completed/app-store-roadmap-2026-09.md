# Completed: Roadmap: launcher and app store — installing apps onto Picodroid over the network

Items closed out of [app-store-roadmap-2026-09.md](../designs/app-store-roadmap-2026-09.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

## 3. Sessions

### S0 — Package identity in the manifest

Add manifest keys, all additive, emitted by `build-apk.sh` from the app's
Gradle metadata:

- `package` (reverse-DNS, the identity everything else keys on)
- `version-code` (monotonic integer), `version-name`
- `label`, `icon` (an asset name in the ASSETS section)
- `min-framework-map-version`
- `uses-feature` (comma-separated `PackageManager.FEATURE_*` names)
- `uses-permission` (S9 consumes this; S0 only carries it)

`papk-info` prints them; `pdb install` warns on a missing `package`. No
device behaviour changes. Existing PAPKs without the keys keep installing
through PDB (legacy path) until S2 makes `package` mandatory for slots.

### S1 — Multi-slot flash layout and the package index

Replace the single `PAPK_FLASH` region on RP2350 with a partition table:

- `SYS_PAPK`: one protected region for the launcher + store image (S6).
- `APP_PAPK[0..N]`: N fixed slots. First cut: 4 × 512 KB out of the
  2816 KB firmware reservation, leaving the firmware its measured size plus
  headroom. Slot size is a board.toml key so a board can trade count for
  size.
- Each slot keeps its own 4 KB boot-meta sector, so the existing
  `PapkSlot<F>` arithmetic applies per slot with a slot-base parameter.

`PapkSlotFlash` grows a slot index; `read_mapped` takes a slot base. A
package index in LittleFS (`/pm/index`, protobuf-encoded, see §4) maps
`package → slot, version-code, install time`; it is rebuilt from the slot
boot-meta pages on boot if missing or corrupt, so the slots stay the source
of truth. Uninstall erases the meta sector of the slot and drops the index
row.

`pdb install` gains `--slot` and `--package`; the sim gets an in-memory
slot array so S2 onward has coverage without a board.

### S5 — TLS

The device has no TLS. Two positions, choose after measuring:

1. **Signatures only (S3), plain HTTP.** Integrity and authenticity come
   from the package and catalog signatures; TLS would add only privacy of
   what the device downloads. Cheapest, and enough to ship an internal
   store.
2. **TLS 1.3 client** via `embedded-tls` (`no_std`, Rust, client-only,
   TLS 1.3 only) with the store's certificate pinned rather than a root
   store. Expected 60–80 KB of flash plus ~16 KB RAM per connection on
   RP2350. Needed before a public store.

Recommendation: ship S4 on position 1 with the catalog and packages signed,
and land position 2 as its own session once flash is measured. Either way
`HttpURLConnection` stops throwing on `https` when 2 lands.

### S7 — Cross-package launch and the task stack

`startActivity(Intent)` with a target outside the current package:

- Resolve through the S2 index; unknown package → `ActivityNotFoundException`.
- Tear down the current app (`run_app` re-entry: heap reset, background
  pool drain, sensor deregistration all exist) and re-enter `run_app` on the
  target slot.
- A task stack of packages, depth-limited (4 is plenty): when the last
  `Activity` of a package finishes, pop and re-enter the previous package.
  The launcher is the bottom of the stack and cannot be popped.
- `onSaveInstanceState` is not carried across a re-entry in this session;
  document it and defer.

### Deferred, tracked here so it is not forgotten

- **Per-package data isolation** for files beyond `SharedPreferences`
  (`openFileOutput`, `getFilesDir`): namespaced by package in LittleFS,
  wiped on uninstall. Small; lands with whichever session first needs it.

## 5. Status

| Session | Title | Status |
|---------|-------|--------|
| S0 | Package identity in the manifest | DONE 2026-09-07 as multi-app M0 (A2) |
| S1 | Multi-slot flash layout and package index (RP2350) | DONE 2026-09-07 as multi-app M1 — dynamic region, no index (A2, A3) |
| S5 | TLS 1.3 client (position 2) | DONE 2026-09-27 as `docs/designs/tls-2026-09.md`: `embedded-tls` 0.19 `rustpki`, full chain verification against a compiled-in root store (not a pinned key), `HttpsURLConnection` |
| S7 | Cross-package launch and task stack | DONE 2026-09-07 as multi-app M2 — exit returns home, no task stack (A2) |
