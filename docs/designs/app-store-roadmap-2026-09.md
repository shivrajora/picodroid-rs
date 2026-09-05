# Roadmap: launcher and app store — installing apps onto Picodroid over the network

> Produced 2026-09-04 from a survey of the install path at `730cec8`. Every
> claim about what exists today was checked against source; sessions are
> ordered by dependency. Amendments are appended at the bottom and OVERRIDE
> the body where they conflict. Execute from this doc; append an amendment
> when reality diverges.
>
> Vocabulary assumed: PAPK (the container in `papk-format`), the shrink map
> and its `framework-map-version` (`docs/shrinker.md`), `run_app`
> (`picodroid-core/src/boot.rs`), the install orchestrator
> (`picodroid-core/src/install/`), and the JVM core / comm core split on
> RP2040 and RP2350.

## 0. Why this exists

Today an app reaches a device in exactly one way: a developer runs `pdb
install` over the debug transport and the device reboots into the single
PAPK slot. That is a developer workflow. A launcher and an app store turn
Picodroid into something an end user can put apps on without a probe, and
that needs pieces the runtime does not have: more than one installed app,
a way to tell apps apart, a way to trust an image that arrived over WiFi,
and a network path into the installer.

This document lists those pieces, what they build on, and the order to land
them in. It is a roadmap, not a design for each piece; each session opens
with its own design pass.

## 1. What exists and carries over

The install path is further along than "one slot" suggests. Nothing below
needs to be rewritten; every session extends it.

| Piece | Where | What it gives the store |
|-------|-------|-------------------------|
| PAPK container: manifest (key/value), classes, assets, per-section CRC32 field | `papk-format/src/lib.rs` | Additive manifest keys for package identity; the CRC field is already on disk (emitted as `0 = unchecked` today) |
| Compat rule between a PAPK's `framework-map-version` and the firmware's | `compat/src/lib.rs` | An older PAPK runs on newer firmware down to the member-shrink floor (append-only maps); a PAPK from the future is refused. This is most of an app ABI story |
| Install orchestrator: peek header + manifest before erase, park the JVM core, stream 256-byte pages, verify, commit the boot-meta page last, reset | `picodroid-core/src/install/orchestrator.rs`, `slot.rs` | Atomic install (a failed install still boots the previous app), transport-agnostic via `InstallTransport`, flash-agnostic via `PapkSlotFlash` |
| Fixed 1 MB slot per chip (4 KB meta + 1020 KB data), memory-mapped, classes loaded in place | `platforms/rp/src/hal/rp/flash.rs`, `mcus/rp/*.x` | The slot model: a PAPK is a contiguous XIP region, not a file |
| `run_app` re-entry: heap reset, class-set republish, background pool drain | `picodroid-core/src/boot.rs` | Switching apps without a reboot — the PDB reload path already does this |
| LittleFS region (128 KB rp2040, 256 KB rp2350) with a `SharedPreferences` on top | `picodroid-core/src/fs/`, `sdk/java/picodroid/content/SharedPreferences.java` | Per-package data and the package index |
| `HttpURLConnection`, `Socket`, `InetAddress`; HTTPS throws `UnsupportedOperationException` | `sdk/java/picodroid/net/` | The download path, minus TLS |
| `PackageManager` with only `hasSystemFeature` | `sdk/java/picodroid/content/pm/PackageManager.java` | The class the store's queries hang off |
| `Intent`, `startActivity` within one app | `sdk/java/picodroid/content/Intent.java` | The launch API; needs a cross-package resolver behind it |

## 2. Constraints that shape every decision

- **Flash, not RAM, decides which boards get a store.** RP2040 boards have
  2 MB: 896 KB firmware (the flash gate), 128 KB LittleFS, 1 MB slot. There
  is no room for a second slot, let alone a launcher and store in firmware.
  RP2350 boards have 4 MB with 2816 KB reserved for firmware, of which the
  image uses well under half. **The store targets RP2350 (`testbench_rp2350w`,
  `pico_enviro_mon_w`) only.** RP2040 keeps `pdb install`.
- **A PAPK must stay contiguous and memory-mapped.** `run_app` hands
  `&'static [u8]` slices of the slot straight to the class loader; nothing is
  copied into RAM. Installed apps therefore live in fixed flash regions, not
  as LittleFS files.
- **Flash erase and program disable XIP on both cores.** The orchestrator
  parks the JVM core for the duration of an install. A network install runs
  the WiFi driver on the comm core while the comm core is also programming
  flash from RAM. LittleFS writes already survive this in short windows; a
  megabyte of pages during a live TCP stream is an untested regime and gets a
  dedicated soak in S5.
- **RAM cannot hold a PAPK.** 520 KB on RP2350, most of it the JVM heap.
  Downloads stream into flash page by page, exactly as the USB installer does
  now. That is why signature verification must be streaming (S3).
- **The ABI is the shrink map.** Under `--shrink`, framework class and member
  names are renamed per map version. The compat rule makes older PAPKs run on
  newer firmware down to the floor. The store must therefore serve one build
  per (framework-map-version floor) and the device must report its version
  before the catalog is filtered (S7).
- **No RP2040 regression.** Every session lands behind a board feature; the
  rp2040 flash ratchet must not move.

## 3. Sessions

Each session is one PR with its own design pass, sim coverage where the sim
can express it, and an HIL check on `testbench_rp2350w`. Status is tracked
in the table in §5.

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

### S2 — PackageManager and PackageInstaller

Fill in the Android surface, natives in Rust over the S1 index:

- `PackageManager.getInstalledPackages()`, `getPackageInfo(name)`,
  `getLaunchIntentForPackage(name)`, `getApplicationLabel`, `getApplicationIcon`
- `PackageInstaller` with `Session`: `openSession()`, `openWrite(name, offset,
  len)` returning an `OutputStream` whose bytes go straight to the
  orchestrator's page stream, `commit(callback)`, `abandon()`. This is the
  Android API shape, and it maps one-to-one onto the streaming installer:
  the session is an `InstallTransport` fed from Java.
- `PackageInstaller.uninstall(name)`.

The `Session` writes cross from the JVM core to the comm core, so the
orchestrator's `CoreCoordinator` gains a variant where the requesting app
is the one being parked: the session must finish the stream from the comm
core after the JVM core parks, which means the bytes are pulled by the comm
core from a socket it owns (S5), not pushed by Java. S2 lands the API with
`pdb`-fed sessions; S5 wires the socket.

### S3 — Integrity and authenticity

- Emit real per-section CRC32 in `build-apk.sh`; verify them on the device
  before commit (the field and the verify-before-commit hook already exist).
- Add a `SIGN` section: Ed25519 over the file header and every other
  section's bytes, key id, timestamp. The orchestrator hashes pages as they
  stream (SHA-512 is what Ed25519 needs, streaming is natural) and verifies
  the signature before the meta page is committed. A bad signature leaves
  the slot erased and the previous meta absent, which reads as "no app" —
  never as a half-installed one.
- Store public keys baked into firmware as a small key ring; `pdb install`
  keeps a `--unsigned` path gated on a firmware `debug-install` feature so
  developer iteration does not need signing.
- Cost estimate to measure, not assume: `ed25519-dalek` in `no_std` with
  `sha2` is on the order of 20–30 KB of flash on Thumb-2. RP2350-only, so
  it does not touch the rp2040 gate.

### S4 — Network transport for the installer

An `InstallTransport` backed by a TCP stream owned by the comm core:

- HTTP/1.1 `GET` with `Range` on the package URL; resumable across WiFi
  drops by re-requesting from the last committed page.
- Backpressure: the transport reads at most one flash page ahead; the
  existing `PEEK_BUF_LEN` header peek still runs before erase.
- The XIP-off windows during program must not starve the CYW43 driver.
  Measure retransmits and driver-side timeouts during a 1 MB install; this
  is the item most likely to need an amendment.
- Sim: the transport talks to a local HTTP server started by `sim.sh
  --store-url`.

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

### S6 — System apps: launcher and store in firmware

- Build `launcher` and `store` as ordinary `picodroid.*` apps under
  `examples/`, but link them into the `SYS_PAPK` region from S1 at firmware
  build time, exactly as the baked-in app is embedded today. They are never
  in an installable slot, so installing an app cannot overwrite them.
- Boot policy: boot into the launcher unless a board.toml `boot-package`
  names an installed package (kiosk mode, and the current single-app
  behaviour for boards that want it).
- The launcher is a grid of `getInstalledPackages()` with labels and icons;
  the store is the S7 client. Both are held to the same size discipline as
  every SDK class: they cost flash on every RP2350 board.

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

### S8 — Store client protocol and catalog

The device side of the store, spoken between the S6 store app's natives and
a server. Wire format decisions are in §4.

- **Handshake**: device sends `DeviceInfo { board, firmware_version,
  framework_map_version, member_floor, features[], installed[] {package,
  version_code} }`. Server answers `Catalog` filtered to what will run and a
  list of `Update` entries.
- **Catalog entry**: `package, version_code, version_name, label,
  description, icon (small LVGL-native asset), size, download_url,
  sha512, signature key id`.
- **Download**: `download_url` is fetched by S4's transport as raw bytes.
  The PAPK is never wrapped in a protobuf message.
- **Catalog signature**: the catalog response carries a detached Ed25519
  signature over its bytes, checked with the S3 key ring, so a plain-HTTP
  store cannot be spoofed into offering a different (even validly signed)
  package.
- Server: a small service that stores unshrunk app sources or class jars and
  builds one PAPK per `(package, version, framework-map-version)` on demand
  with `build-apk.sh --shrink --shrink-app`, keeping the app shrink map for
  retrace of crash reports. Reference implementation in `tools/store-server`
  (Rust, so it links `papk-format` and `compat` directly and can never
  disagree with the device about compatibility).

### S9 — Permissions

Apps today can reach every native. A store implies that a package declares
what it needs:

- `uses-permission` from S0 becomes enforced: `INTERNET`, `GPIO`, `SENSORS`,
  `STORAGE`, `NOTIFICATIONS`, named as `picodroid.permission.*` to mirror
  `android.permission.*`.
- Enforcement at the native boundary: the dispatcher knows the current
  package (S7's stack top) and its granted set; a native in a gated group
  throws `SecurityException`.
- `Context.checkSelfPermission`, `Activity.requestPermissions` with a
  system dialog, grants persisted per package in LittleFS and wiped on
  uninstall.
- Install-time display in the store: the catalog entry lists the
  permissions from the manifest, the store shows them before download.

### Deferred, tracked here so it is not forgotten

- **Firmware OTA.** A store that cannot update the framework hits the map
  floor within a few releases. A/B firmware is not possible in the RP2350
  budget; a bootloader-staged single-image update is. Separate design.
- **Per-package data isolation** for files beyond `SharedPreferences`
  (`openFileOutput`, `getFilesDir`): namespaced by package in LittleFS,
  wiped on uninstall. Small; lands with whichever session first needs it.
- **Device attestation** to the store. Not needed until the store gates
  anything on device identity.

## 4. Wire format and protocol

**Decision: protobuf for every control-plane message; raw bytes for the
package payload; HTTP/1.1 as the transport.**

Why protobuf holds up on this device:

- Binary varint encoding keeps `DeviceInfo` and a 20-entry `Catalog` inside
  a few KB, which matters when `Socket.send` is bounded at 256 B per call
  and the heap is 400 KB.
- A schema (`proto/store.proto`) is the single source of truth for the
  device, the server and `pdb`, and unknown fields are skipped, so the
  server can evolve ahead of firmware in the field.
- Field presence and enums are explicit, which JSON on this device would
  have needed a hand-written validator for.

How it is implemented on the device matters more than the format:

- **Decode in Rust, not Java.** `protobuf-javalite` is hundreds of KB of
  classes and would never fit. Generated Java message classes plus a tiny
  hand-written decoder would fit but would be app code paying for framework
  work. Instead the S8 natives decode with `micropb` (`no_std`, no `alloc`
  required, generates plain structs from `.proto`) and hand the store app
  typed `picodroid.*` objects. The Java side of the store never sees a wire
  byte.
- **Never embed the PAPK in a message.** A 1 MB `bytes` field would have to
  be buffered to be decoded. The catalog carries a URL; S4 streams it.
- Both `tools/store-server` and the device generate from the same `.proto`
  in the build, with a pinned `protoc` so the two cannot drift.

Alternatives considered:

- **CBOR** (`minicbor`, `no_std`): no schema compile step, self-describing,
  about the same size on the wire. It would be the pick if we did not want a
  schema; we do, because the server and the device are separately deployed
  and a `.proto` is the contract that keeps them honest. Not chosen.
- **JSON**: 3–5× the bytes, a parser the device does not have, and no
  schema. Not chosen.
- **CoAP + DTLS** instead of HTTP + TLS: smaller headers and a block-wise
  transfer that maps well onto page streaming, but the device already has an
  HTTP client, every hosting option speaks HTTP, and `Range` requests give
  the same resumability. Revisit only if S4 shows HTTP framing overhead is a
  real cost, which is unlikely at 1 MB per install.
- **gRPC**: HTTP/2 framing on top of protobuf; nothing on the device speaks
  HTTP/2 and the streaming benefits are not needed. Plain protobuf bodies
  over HTTP/1.1 (`Content-Type: application/x-protobuf`) get the schema
  benefit with none of the cost.

## 5. Status

| Session | Title | Status |
|---------|-------|--------|
| S0 | Package identity in the manifest | NOT STARTED |
| S1 | Multi-slot flash layout and package index (RP2350) | NOT STARTED |
| S2 | PackageManager and PackageInstaller | NOT STARTED |
| S3 | CRC + Ed25519 signatures, streaming verify | NOT STARTED |
| S4 | Network `InstallTransport` over HTTP `Range` | NOT STARTED |
| S5 | TLS 1.3 client (position 2) | NOT STARTED |
| S6 | Launcher and store as firmware system apps | NOT STARTED |
| S7 | Cross-package launch and task stack | NOT STARTED |
| S8 | Store protocol (protobuf) and reference server | NOT STARTED |
| S9 | Permissions | NOT STARTED |

Dependencies: S1 needs S0. S2 needs S1. S3 is independent and should land
before S4 is used against anything but a local server. S4 needs S2. S6 needs
S2 and S1. S7 needs S6. S8 needs S4, S3 and S6. S5 and S9 can land any time
after S4 and S7 respectively.

Minimum demo (an app installed from a laptop-hosted store onto a
`testbench_rp2350w`, launched from the launcher): S0, S1, S2, S3, S4, S6,
S7, S8.

## 6. Open questions

- Slot count versus slot size on RP2350: 4 × 512 KB assumes apps stay under
  the size of today's largest example after `--shrink-app`. Measure
  `picoenvmon` shrunk before fixing the number.
- Whether the store app is one package with the launcher or two: one
  package saves a re-entry when opening the store; two mirrors Android.
  Decide in S6's design pass.
- Whether `PackageInstaller.Session.openWrite` should exist on the Java
  side at all, given the bytes are pulled by the comm core in practice.
  Keeping it preserves the Android API; the implementation may make it a
  thin front over "give me a URL".

## Amendments

None yet.
