# Device serial numbers (2026-10-14)

**Status: built 2026-10-14, stages 1–8 (`44df949b`..); open questions decided the same
day (§4). All three bench boards run 2.4 firmware (the touch kit's RP2350B reads
`1C05163AC1ACB0AA` in BOOTSEL and in the app). Remaining: the two-night fleet check
below.** Replaces the
tty-name and USB-position identities the host tools use today.

## 0. Problem

A picodroid device has no identity of its own. Its USB descriptor has no serial string
(`iSerialNumber = 0`, `crates/picodroid-core/src/pdb/usb_cdc.rs`), and its PING greeting
names the board type, not the board. So every tool identifies a device by **where** it is,
and all of those break:

- **`pdb -s /dev/ttyACMn`** names a tty, which changes whenever the device
  re-enumerates. The kernel gives the CDC the lowest free minor, and on a bench with
  several boards that is often a different one. This is the cause of the
  `helloworld:pdb-install` HIL flake: 11 FAILs from 10-05 to 10-14 on every slot, and the
  same warning in ~5–10 % of `pdb-install-stress` cycles. `install::wait_for_reboot` polls
  the old name for 20 s while the board answers on a new one. The kernel logs show it:
  - 10-12: the rp2040 board went from ttyACM5 to ttyACM4.
  - 10-14: the touch kit went from ttyACM3 to ttyACM0, and the W slot's debug probe then
    took ttyACM3.
- **Following the USB bus position** (`1-8.3.4`) fixes that on Linux only, because it
  reads sysfs. It also names a socket on the hub, not a board: a board moved to another
  port, or swapped on a slot, is lost. It was tried on 2026-10-14 and reverted for these
  reasons.
- **`hil-run.sh::pin_pdb_port`** picks the bench board by the `max PAPK: N KB` text of
  its greeting. That is a property of the firmware, not the board, and it also pins a tty
  name.
- **Without `-s`, the reboot poll** takes the first picodroid VID/PID on the host, which
  can be a neighbour that then answers "Install complete".

## 1. What Android does

One value, the device serial, appears in every place a device is identified:

| Where | Android |
|---|---|
| The device | `ro.serialno`, set from the bootloader's `androidboot.serialno` |
| USB | The gadget's `iSerialNumber` string, the same value |
| App API | `android.os.Build.getSerial()` (API 26). It requires `READ_PRIVILEGED_PHONE_STATE` since API 29. `Build.SERIAL` is deprecated and reads `Build.UNKNOWN` for apps targeting O+. |
| Settings | About phone → Model / Status → Serial number |
| adb device list | `adb devices` prints `<serial>\t<state>`, with state `device`, `offline`, `unauthorized` or `no permissions`. `adb devices -l` adds `usb:<path> product: model: device: transport_id:`. |
| adb target selection | `adb -s <serial>`, `ANDROID_SERIAL=<serial>`, `-d` (the one USB device), `-e` (the one emulator), `-t <transport_id>`. With several devices and none chosen: `error: more than one device/emulator`. |
| Scripting | `adb get-serialno`, `adb wait-for-device` (and `wait-for-usb-device`, `-local`) |
| Emulators | serial `emulator-<console port>`, e.g. `emulator-5554` |

adb finds a device by its serial, and so it finds it again after a reboot whatever
`/dev` node or USB port the device comes back on.

## 2. Design

### 2.1 One serial, three places it appears

On the RP families the serial is 16 uppercase hex characters (64 bits), e.g.
`E6633861A37B9038`. This is the same format as the RP2350 bootrom's BOOTSEL serial,
the Pico SDK's `pico_get_unique_board_ascii` and the Debug Probe serials already in
`fleet.conf`, so users see the same number they see in `lsusb` and `probe-rs list`.
(The RP2040 bootrom shows one fixed BOOTSEL serial on every chip, `E0C9125B0D9B`, so on
the RP2040 the reference is the Pico SDK's value instead; see §2.2.)

In general a serial is 1 to `SERIAL_MAX` (32) characters of `[0-9A-Za-z._:-]`: safe in a
USB string, a shell word and an adb-style `-s` argument. Each family formats it the way
its vendor's own USB bootloader does, so `lsusb` shows the same value in both modes; where
the bootloader shows no per-chip serial (the RP2040), it follows the vendor SDK's unique
ID.

The platform computes it once at boot. It then appears in three places:

1. **USB `iSerialNumber`**: string descriptor 3 (§2.3).
2. **The PING greeting**: a new length-prefixed field (§2.4).
3. **`picodroid.os.Build.getSerial()`**: the app API (§2.5).

All three come from the same value, as `ro.serialno` does on Android, so the host can
find a device by its USB serial before opening any port and then confirm it over PING.

### 2.2 Where the ID comes from: the family seam

**Core owns everything about the serial except how the chip's ID is read and formatted.**
A new family (STM32, nRF, ESP) writes one hook body and makes one boot call. It then gets
the USB descriptor, the PING field, `Build.getSerial()`, the Settings row, pdb selection
and the bench scripts without further work.

**Core: `crates/picodroid-core/src/identity.rs`** (family-neutral, no chip `cfg`):

```rust
pub const SERIAL_MAX: usize = pdb_protocol::greeting::SERIAL_MAX; // 32
/// A validated serial: 1..=SERIAL_MAX chars of [0-9A-Za-z._:-].
pub struct Serial { /* [u8; SERIAL_MAX] + len */ }
impl Serial {
    /// Uppercase hex of a raw chip ID of up to 16 bytes. None when the bytes
    /// are all 0x00 or all 0xFF (unprogrammed, or a flash without the command).
    pub fn from_id_bytes(id: &[u8]) -> Option<Serial>;
    /// A family-formatted string, for a vendor bootloader that shows another
    /// form (ST DFU's 12-character fold, an ESP MAC, the simulator).
    pub fn from_ascii(s: &str) -> Option<Serial>;
}
/// Once, from family boot: single-threaded, filesystem mounted, scheduler,
/// other cores and USB not started.
pub fn init();
/// The serial; "unknown" (Build.UNKNOWN) if init() never ran.
pub fn serial() -> &'static str;
```

`init()` holds the policy, so no family re-implements it:

1. `host::device_serial()`. If it returns `Some`, that is the serial.
2. Otherwise `/system/serial`: load it, or create it from TRNG bytes through the existing
   `picodroid_port_entropy_bytes` hook (`net/tls.rs`), waiting briefly as `tls::seed()`
   does. It is saved with the tmp-then-rename pattern of `power.rs`, so it survives any
   reflash that keeps the filesystem.
3. With no entropy either, the serial is `unknown`, logged once.

The fallback is decision logic over a small load/save trait, so it is unit-tested without
a filesystem.

**The hook: one required `PlatformHooks` method** (`crates/picodroid-core/src/host.rs`),
with the same shape as `uninstall_run`: trait fn, `extern "Rust"` shim,
`set_platform_hooks!`.

```rust
/// This chip's identity, formatted as the vendor's own USB bootloader shows
/// it. None = no hardware ID; core falls back to a persisted random serial.
/// Called once, from identity::init().
fn device_serial() -> Option<identity::Serial>;
```

It has no default, so a new family has to answer it on purpose. This is the "an explicit
answer is a decision" rule of `family-neutral-residue.md` D6. The objection to the D4
hooks, that they encoded one family's flash topology, does not apply: every family has
to answer this question.

**The family side (RP):** `platforms/rp/src/hal/rp/chip_id.rs`, plus a call to
`identity::init()` in `main.rs` after `fs::init` and before `start_tasks`.

| Family | `device_serial()` | Constraint on the boot call |
|---|---|---|
| RP2040 | `from_id_bytes` of the QSPI flash's unique ID (JEDEC `0x4B`), the source of the Pico SDK's `pico_unique_board_id` and so of every RP2040-based Debug Probe's serial. The RP2040 bootrom cannot help: its BOOTSEL descriptor shows the same `E0C9125B0D9B` on every chip (seen on `testbench_rp2040`, 2026-10-14, next to its flash ID `E66258881755602D`). A flash that does not answer reads as all 0xFF, and the core fallback takes over. | XIP is off for the read, so it must run before core 1 starts. It uses a RAM-resident SSI transfer inside `flash.rs`'s `with_xip_disabled!`, which restores XIP by re-running the RAM copy of boot2. |
| RP2350 | `from_id_bytes` of the factory chip ID in OTP, through `rp235x_hal::rom_data::sys_info_api::chip_info()` (rp235x-hal 0.4, a safe API): the wafer-ID word, then the device-ID word, each big-endian. That is the BOOTSEL order, checked on the W slot on 2026-10-14 (`68B1A1217192254D` in BOOTSEL and in the app). No flash command, no XIP exit. | none |
| Simulator | `from_ascii("emulator-<pid>")`, stable across a sim reboot because the reboot `exec`s the same pid. It mirrors adb's `emulator-5554`. | none |
| STM32 (worked example) | The 96-bit UID at the series' `UID_BASE`, through `from_ascii` of ST's 12-character fold (`UID0 + UID2` as 8 hex digits, then the top 16 bits of `UID1`), so it matches the system DFU bootloader. Verify per series. | none (memory-mapped) |
| nRF52 (worked example) | `from_id_bytes(FICR.DEVICEID[0..2])` | none |
| ESP32-S3 (worked example) | `from_ascii` of the eFuse MAC as the USB-JTAG serial shows it | none |

The greeting encoder, the USB descriptor and `Build.getSerial()`'s native read only
`identity::serial()`. Nothing outside `identity.rs` knows where the value came from. The
`identity.rs` module doc is the porting checklist.

### 2.3 USB descriptor

- Set `iSerialNumber = 3` in `device_descriptor`.
- Add `STR3`, a string descriptor of 2 + 2 × the serial's length (34 B on RP). It is built
  at runtime into a `'static` buffer, because `STR0..STR2` are `const` tables and this
  value is not known at build time. The RP driver's `ep0_send` sends from the pointer
  after it returns, so the buffer must be `'static`.
- **Core serves every string; family drivers only route.** A new
  `usb_cdc::string_descriptor(index) -> Option<&'static [u8]>` returns `STR0..STR3`. The
  RP driver's `handle_get_descriptor` (`platforms/rp/src/hal/rp/pdb_usb/mod.rs`) calls it
  instead of matching indexes 0 to 2 itself; today index 3 stalls there. A new family's
  USB driver does the same and serves the serial without knowing it exists.

Cost: 34 B of RAM and a few dozen bytes of code.

The OS benefits on every platform:

- **Linux:** `/dev/serial/by-id/usb-Picodroid_PDB…_<serial>-if00` appears.
- **macOS:** the device is named `/dev/cu.usbmodem<serial>…` instead of being named after
  its location ID.
- **Windows:** a COM port number is kept per serial instead of per hub port.

The `serialport` crate reports `UsbPortInfo::serial_number` on all three, which is what
makes the host side portable (§2.6).

### 2.4 PING greeting: `picodroid/2.4`

The serial goes **before the apps tail**, for the same reason the board name did in 2.3.
The apps tail is optional and detected by length, so nothing can follow it unambiguously.

```text
[14] version string                ("picodroid/2.4\0")
[4]  max PAPK, u32 LE
[1]  framework-map-version length, [N] bytes
[1]  board name length, [M] bytes          (2.3+)
[1]  serial length, [K] bytes              (2.4+, K ≤ SERIAL_MAX = 32)
[10] apps tail, multi-app firmware only
```

- `Greeting` gains `serial: &str`, which is empty below minor 4.
- Add a `SERIAL_FIELD_MINOR = 4` constant.
- The golden test pins the 2.4 prefix, and a 2.3 greeting must still parse.

### 2.5 App API

This mirrors `android.os.Build`:

```java
/** Value used when a build property is unknown. */
public static final String UNKNOWN = "unknown";

/** @deprecated Use {@link #getSerial()}. Reads {@link #UNKNOWN}, as on Android O+. */
@Deprecated public static final String SERIAL = UNKNOWN;

/** The hardware serial number, e.g. {@code E6633861A37B9038}. */
public static String getSerial() { ... }
```

Picodroid has no permission model, so `getSerial()` returns the serial to every app. When
permissions land, it should throw `SecurityException` without
`READ_PRIVILEGED_PHONE_STATE`, as Android does. Record that divergence in the Javadoc.

`Build.SERIAL` stays `UNKNOWN` on purpose, so that code ported from a modern Android app
behaves the same way.

Settings → About adds a **Serial number** row, as Android's About phone does.

### 2.6 `pdb`: adb's device selection

| adb | pdb |
|---|---|
| `adb devices` | `pdb devices`: adb's `List of devices attached` header, then one row per device: serial, state, board and app space left (`9720 KB free`, or `764 KB max` on single-app firmware), in space-padded columns. The board and the space go beyond adb (§4, decision 3). State is `device` when it answers PING, or `offline` when the USB serial is visible but it does not answer (mid-boot, wedged, busy installing). A device on firmware older than 2.4 is listed by its port name. With no devices, the output is the header alone and the exit code is 0. This replaces today's `no picodroid devices found` line. |
| `adb devices -l` | `pdb devices -l` adds `port:/dev/ttyACM3 release:0.37.0`, plus `usb:1-8.3.4` where the OS exposes it (Linux), as adb does. |
| `adb -s <serial>` | `pdb -s <serial>`. A port path and a socket path are still accepted, so existing invocations keep working. `-s sim` stays as an alias for `-e`. |
| `ANDROID_SERIAL` | `PICODROID_SERIAL`. `-s` takes precedence over it. |
| `adb -d` / `-e` | `pdb -d` (the one USB device) and `pdb -e` (the one simulator). |
| `adb get-serialno` | `pdb get-serialno` |
| `adb wait-for-device` | `pdb wait-for-device` (with `-s`: wait for that serial), exit 0 when it answers PING |
| `error: more than one device/emulator` | The same wording, listing the serials |

How a serial is resolved:

1. **Simulator.** `emulator-<pid>` maps to `sim_socket_dir()/pdb-<pid>.sock` when that
   exists. A sim started with `PICODROID_SIM_PDB_SOCKET` (as `sim-run.sh` does) has
   another socket name. For that case pdb PINGs each socket `sim_sockets()` lists and
   matches the greeting serial; `sim-run.sh` keeps passing its socket path.
2. **USB device.** Call `serialport::available_ports()` and keep entries with picodroid
   VID/PID and `serial_number == serial`. This opens no port, so a neighbour's CDC and the
   debug probes' UARTs are never written to. It is portable wherever `serialport` is
   (4.9, already a dependency, reports `serial_number` on Linux, macOS and Windows).
   Auto-detect also stops PINGing ports without the picodroid VID/PID: today
   `devices::scan_serial` PINGs every serial port when none matches.
3. **Confirm.** As built, only the reboot poll compares the answering greeting's serial
   with the one it is following. A mismatch can only happen on a host where two devices
   share an ID, and a confirming PING on every command would cost a round trip on every
   `pdb input` a soak sends.

**Waiting for a reboot.** `install::wait_for_reboot` and `packages::uninstall` take the
serial from the first greeting, before the reboot. Each poll then re-resolves that serial
through steps 1 and 2, PINGs it, and checks that the answering greeting carries the same
serial. The tty name, the USB position and the VID/PID scan are no longer used. A device
on firmware older than 2.4 has no serial; for it, the poll does what it does today, and
the fallback is removed one release later.

**Portability.** Selection stops depending on `/sys`, and nothing in it is
family-specific: an STM32 board on picodroid's VID/PID shows up in `pdb devices` with no
host change. The remaining Unix-only code is the simulator transport
(`transport.rs` imports `std::os::unix` unconditionally, so pdb does not build on
Windows today), and CI does not build pdb on macOS. If Windows support is wanted, that
layer has to be split separately; this plan does not make it worse.

### 2.7 Bench scripts

- **`fleet.conf` keeps `board_usb_path`.** A slot is a physical place wired to a probe,
  and boards get swapped on a slot (the W slot holds whatever was put in), so a fixed
  board serial per slot would go stale. Update the comment that says "the device has no
  serial string".
- **`hil_pdb_run` and `pdb.sh`** resolve slot to serial on every call and pass
  `-s <serial>`. A reboot inside a pdb call can no longer lose the board. As built, the
  serial comes from sysfs, not from `pdb devices -l`: `fleet-lib.sh::usb_path_pdb_target`
  reads `/sys/bus/usb/devices/<board_usb_path>/serial` once the tty is enumerated, and
  falls back to the tty on firmware older than 2.4. `pdb devices` PINGs every board on
  the bench, including a neighbour that another session's lease covers and that may be
  mid-install. A PING in that stream would break its framing, while a sysfs read opens
  nothing. The enumeration wait stays as it was (tty present, then target).
- **`pin_pdb_port`** (bench without a fleet config, so one board) pins by serial:
  `PICODROID_SERIAL` (which replaces the script-only `PICODROID_PDB_PORT`), or the serial
  of the one device whose `devices` row has the matching board column. This replaces the `max PAPK` text match and drops the comment
  noting that `pdb devices` rows must keep it.
- **Gradle's `install` task** (`buildSrc/.../RunAppTask.kt`) runs `pdb install` with no
  `-s`, so it relies on auto-detect and the "more than one device" error. It picks up
  `PICODROID_SERIAL` without any change.
- **`sim-run.sh`** keeps passing the socket path, which still works, or passes
  `emulator-<pid>`.
- **Other dependents.** Grep `scripts/` and `docs/` for consumers of `pdb devices` output
  and `-s` usage (`hil-run.sh`, `pdb.sh`, `sim.sh` help, `sim-run.sh`,
  `docs/designs/multi-app-2026-09.md`, `edge-llm-roadmap-2026-08.md`) and update the
  examples. The website shows the old format in `reference/pdb-commands.md` (already
  stale: no board), `get-started/hot-swap.md`, `guides/troubleshooting.md` and
  `get-started/simulator.md`.

## 3. Stages

Each stage builds, passes `./scripts/pre-commit`, and is one commit.

1. **Protocol.** `pdb-protocol`: greeting 2.4 with the serial field, `SERIAL_MAX`,
   `SERIAL_FIELD_MINOR`. Golden tests for 2.4, 2.3 and 2.2 greetings.
2. **Core seam.** `identity.rs` (`Serial`, `init`, `serial`, the `/system/serial`
   fallback) and the `PlatformHooks::device_serial` hook, implemented in the simulator
   (`emulator-<pid>`) and `TestHooks`. The RP glue returns `None` until stages 3 and 4,
   so the fallback serves it meanwhile. `usb_cdc::string_descriptor()` with `STR3` and
   `iSerialNumber`, the RP driver routed through it, and a descriptor test like the
   existing `STR1`/`STR2` ones. The greeting encoder sends the serial. Sim smoke test.
3. **RP2350 ID.** OTP chip ID in `chip_id.rs`. Flash a board, then check that `lsusb -v`
   shows the serial and that it matches what the bootrom's BOOTSEL descriptor shows on
   that chip. This covers five of the six bench boards, the touch kit (RP2350B) among
   them. **Done 2026-10-14** on the W slot: the first build printed the two words
   swapped, and the fix put the wafer ID first. To reach BOOTSEL without a button, blank
   flash sector 0 over SWD (`probe-rs download --binary-format bin --base-address
   0x10000000` of 4 KB of 0xFF, then `probe-rs reset`) and reflash with `flash.sh`
   afterwards.
4. **RP2040 ID.** Flash unique ID read in `chip_id.rs`. This is the stage with new
   `unsafe`, in a RAM-resident XIP-off routine; the Pico SDK's `flash_do_cmd` is the
   reference. It goes through the spin-guard and unsafe-ratchet rules (`platforms/rp`
   +1 block and +2 fns, accepted), and is measured on `testbench_rp2040`, the only RP2040
   board. **Done 2026-10-14:** `E66258881755602D`. The board boots normally after the
   read (boot2 restore). BOOTSEL cannot serve as the check here (§2.2).
5. **App API.** `Build.UNKNOWN`, `SERIAL`, `getSerial()` and the native, plus the Settings
   → About row. Add a `qa_*` assertion that `getSerial()` matches the greeting's serial.
   The shrink map and the SDK member-name guard need the new members.
6. **pdb.** Serial resolution; `-s <serial>`, `-d`, `-e`, `PICODROID_SERIAL`;
   `devices [-l]`, `get-serialno`, `wait-for-device`; `wait_for_reboot` by serial with a
   fallback for firmware older than 2.4. Unit tests run against a fake port list (put
   `available_ports` behind a small trait or function parameter).
7. **Scripts.** The §2.7 changes, `test-device-lock.sh` fixtures, and the docs.
8. **Regression row.** Add a `helloworld|pdb|90|Install complete|install-renumber` row to
   `hil-tests.conf`. It holds the board's current tty open (an `O_NONBLOCK` open in a
   helper process) across the install, which forces the kernel to give the board a new
   minor. Pre-fix `pdb` fails this deterministically: verified 2026-10-14 on
   testbench_rp2040, where it went from ACM2 to ACM6 with "did not respond to PING". This
   turns the flake into a test that always fails if the bug comes back. **Done
   2026-10-14** as `helloworld|pdb|90|Connected:;Install complete|install-renumber`. On
   testbench_rp2040 it passes (ACM4 → ACM6, "Install complete."), and the pre-stage-6 pdb
   (`049194ae`) fails it with the old 20 s warning. Without a fleet config the runner
   finds the tty from the `devices -l` row, and with one from sysfs.

**Verification:**

- Run `./scripts/hil-run.sh` on all three slots, both shrink modes. Check that the
  `pdb-install` rows pass and that no `pdb-install-stress` cycle prints the 20 s warning.
- Then watch two consecutive 4 AM fleet nights with zero
  `did not respond to PING` lines across all `*.pdb-*.log` files.
- Run the size ratchet: about +100 B of flash and +50 B of RAM per board, to be accepted.

## 4. Decisions (2026-10-14)

1. **RP2350: the OTP chip ID**, not the flash unique ID. It uses a safe HAL call, has no
   XIP exit and no new `unsafe` on five boards, belongs to the MCU, and is what the Pico
   SDK uses on RP2350. The flash ID would have given one mechanism for both families,
   at the cost of a QMI direct-mode routine (the `psram.rs` pattern) on every RP2350
   board. OTP `RANDID` was rejected because no other tool shows it. Stage 3 confirmed the
   byte order against the BOOTSEL serial.
2. **RP2040: the flash unique ID (`0x4B`)**, even though only one board uses it. It is
   the Pico SDK's unique board ID, the RP2040 bootrom offers nothing better (its BOOTSEL
   serial is one fixed string), and it survives a filesystem wipe. A random `/system/serial` would have
   needed no `unsafe`, but it would match nothing and change on every reformat; it
   remains the core fallback. Adding the `rp2040-flash` crate was rejected: it would
   duplicate `with_xip_disabled!`'s XIP and boot2 handling.
3. **`pdb devices`: adb's header and first two fields, plus the board and the app space**
   (§2.6). Decided first as adb's exact format, then changed the same day at the owner's
   request: each row also shows the board and the app space left (`N KB free`, or
   `N KB max` on single-app firmware), because that is how a bench of several boards is
   told apart at a glance. The columns are space-padded so that rows of different serial and
   board lengths line up, and offline rows hold `-` so every row has the same fields. The
   serial and the state are still `$1` and `$2`, so a script written for adb reads them.
   Nothing outside the repo parses the output: the Gradle `install` task uses
   auto-detect, and pdb is only installed from source.
4. **`getSerial()` is ungated**, and the divergence is in the Javadoc. Anyone on the host
   can already read the value with `lsusb`. A system-apps-only gate would need a new
   `SecurityException` SDK class and a definition of "the caller" on single-app firmware,
   and a manifest permission is in effect the same gate (on Android,
   `READ_PRIVILEGED_PHONE_STATE` is privileged-only). Revisit when a permission model
   lands.
5. **Portability seam** (§2.2): core owns the serial. Families supply only
   `PlatformHooks::device_serial`, and USB drivers route string descriptors through
   `usb_cdc::string_descriptor`.
