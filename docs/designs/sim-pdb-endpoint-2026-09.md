# The simulator as a `pdb` device (2026-09-13)

**Status: landed 2026-09-14** (`09d0b1bd`).

## 0. Problem

`pdb` (tools/pdb) only spoke to hardware: every command opened a USB CDC
serial port, and the simulator shipped no debug-bridge endpoint at all
(`pdb-schema-as-code.md` §3 deferred one on 2026-07-27 because "sim
install/park/reboot semantics do not exist"). The sim's FIFO control channel
reached the real installer, but through a different front door — on the JVM
task, with `NoCoordinator`, no wire, no park, no reboot — so nothing that
`pdb install` does on a device could be rehearsed without a board.

Goal: `pdb devices` lists a running simulator, and `pdb ping / install /
list / uninstall / input / sysmon` run against it executing the code the
device executes — the command loop, framing, install orchestrator, package
directory, the JVM park handshake, and a real reboot into the real boot path.

## 1. Shape

The device side was already trait-based and family-neutral. What a family
supplies, the simulator now supplies for the host, in
`crates/picodroid-core/src/hal/sim/pdb.rs`:

| Device (`platforms/rp`) | Simulator (`hal/sim/pdb.rs`) |
|---|---|
| `pdb/platform.rs::CdcTransport` — USB CDC, ISR-fed queue | `SimTransport` — Unix-domain socket, non-blocking, polled with `vTaskDelay` |
| `pdb/coordinator.rs::PdbCoreCoordinator` + `pending.rs` flags | `SimCoordinator` — the same two flags, the same 15 × 1 s "look again" wait, through the `rtos` seam |
| `pdb/platform.rs::FreeRtosSysmon` — `TaskStatus_t` pinned to the device config | `SimSysmon` — `TaskStatus_t` pinned to the host config (72 B, `c_ulong` widths) |
| `packagemanager::RpPapkFlash`, reset = chip reset | `SimPapkFlash` — handle onto `app_region`'s `MemRegion`; reset = end the scheduler, dump the region, exec the binary |
| `boot_tasks.rs` spawns "pdb" at `PRIORITY_RT_1` before the JVM task | `sim_boot::run` spawns "pdb" (`TaskKind::DebugBridge`) at `PRIORITY_RT_1` before the JVM task |
| `boot_tasks.rs` JVM supervisor loop with the park point | `sim_boot` JVM loop with the same park point |
| Host: eight `serialport::new` sites | Host: `tools/pdb/src/transport.rs`, one `Link` seam; `-s <tty>`, `-s <socket>`, `-s sim` |

Everything between those rows — `pdb::run_pdb_task`, `wait_for_magic`,
`Framed`, `install::install`, `packages::plan_install/compact/rescan_region`,
`handle_list`, `handle_uninstall`, `input::handle → HalSink` — is the shared
code, unchanged, now executed by the simulator exactly as by a device.

### Why a Unix socket

No port allocation, one file per simulator that a directory glob enumerates
(`pdb devices`), works on Linux and macOS, and `exec` keeps the PID so the
default path is stable across the reboot. TCP would have needed a port
registry; a pty would have needed the host tool to enumerate ptys, which
`serialport` does not.

### Why the reset is an exec

The POSIX port cannot restart its scheduler in place (`pthread_once` state,
`rtos_freertos.rs::end_scheduler`). Replacing the process runs the whole
boot sequence again — `fs::init_host_image`, `app_region::init`,
`sweep_orphans`, `select_boot` — which is what a device does at reset and
what the host tool's `wait_for_reboot` expects to observe. The sequence:

1. `SimPapkFlash::trigger_reset` (bridge task) sets a flag and calls
   `end_scheduler()`. The port joins the tick thread, releases the main
   thread from `start_scheduler`, and parks the caller forever.
2. `sim_boot::main` (main thread, no task running) prints the heap banner,
   sees the flag, and calls `hal::sim::pdb::reboot()`: the region is dumped
   to `<temp>/picodroid-sim/apps-<pid>.img` (raw `PAPK_REGION_LEN` bytes, a
   flash dump), stdout is flushed, and `Command::exec` restarts
   `current_exe()` with the same arguments and environment plus
   `PICODROID_SIM_WARM_BOOT=<dump>` and an explicit
   `PICODROID_SIM_PDB_SOCKET`.
3. The new process's `app_region::init` sees the warm-boot variable,
   registers the system apps as usual, restores the dump, deletes it, and
   rescans — and skips baking `PICODROID_APK_PATH` and installing
   `PICODROID_SIM_APPS`, because a device does not re-flash on reset.

Every fd std opened is close-on-exec (socket, control FIFO, LittleFS image,
the window's X connection) and is reopened from the inherited environment;
stdout/stderr carry on, so a lane's log keeps flowing to the same file.

Cold boots start from an erased region on purpose: the region models what
`flash.sh` just baked, and a region that outlived `sim.sh` launches would
make `sim.sh --app X` run whatever yesterday's session left installed.

### Why the coordinator is a twin and not shared

`platforms/rp/src/pdb/coordinator.rs` keeps the handshake family-side on
purpose (it encodes this family's flash topology), and the RP2040 flash
gate makes every byte on that path expensive. The simulator's twin mirrors
it flag for flag and wait for wait, through the `rtos` seam, so the JVM
loop in `sim_boot.rs` reads like `boot_tasks.rs`. Lifting both into core is
possible once a second family exists; it is not this change.

## 2. The park handshake, path by path

Both `app_region`'s deferred verbs and the bridge raise the same
`platform::STOP_JVM`; the JVM loop clears it at the top of each app. They
cannot double-service: both are consumed on the JVM task, after the
children have drained, `service_deferred()` first (its region writes finish
before any park), then the park check. While parked the JVM task is blocked
in a task notification and polls nothing, so no `apps` verb runs under the
bridge's writes; verbs queued during a park are served after a release and
lost across a reboot, as a device loses a queued command at reset.

| Path | Bridge task | JVM task | Outcome |
|---|---|---|---|
| Install accepted | request → `wait_for_park` → peek, compat, place, erase, stream, commit, rescan → STATUS_OK → `trigger_reset` | stops at the next poll → drains children → `service_deferred` → parks, notifies the bridge | scheduler ends → dump → exec → warm boot → `select_boot` |
| `ParkTimeout` (15 s) | error → `release` → `cancel_park_request` | eventually stops; no request → `next_image()` | STATUS_ERR; the switching loop carries on |
| Incompat / NoRoom / NoPackageName (before erase) | error → release → cancel | released; keeps `image` | the app resumes; nothing erased |
| CRC mismatch mid-stream (after erase) | error → release → cancel | released; if `directory_generation` moved (a compaction, an erased in-place copy), re-finds the running package by name, else `next_image()` | the app resumes when its run is intact, else the launcher |
| Host disconnects mid-install | `read_byte_timeout` → `None` after 2 s → `StreamTimeout`; the error write fails, the client is dropped, the listener accepts the next | as above | the simulator keeps running |
| Uninstall | park → erase → rescan → STATUS_OK → `wipe_package` → `trigger_reset` | parks | reboot as an install |
| Nothing left to run | — | `next_image()` is `None` → `end_scheduler()` → exit | unchanged `sim-run.sh` `term` semantics; `PICODROID_SIM_WAIT_FOR_INSTALL=1` waits for a park request instead, as a device does |

`cancel_park_request` also clears `PARKED` and notifies: the installer calls
`release()` then `cancel_park_request()`, and a JVM task reaching its park
point between the two would otherwise park against a request that is no
longer coming. The device's coordinator has the same window
(`orchestrator.rs`, the `ParkTimeout` branch); it is noted here, not
changed.

## 3. Who writes the region

`static mut REGION` stays in `app_region.rs`; `with_region` is the one
access path. Writers take turns: the JVM task while an app runs or between
apps (the `apps` verbs, a Java `PackageInstaller.uninstall`); the bridge
task only between a park it was granted and the release or reboot that ends
it, which `install()`/`uninstall()` enforce exactly as on a device; the main
thread before the scheduler starts (seeding) and after it ends (the dump).
No closure spans a kernel block. Read-only fixed fields
(`mapped_base`, `region_len`, `max_installed_apps`) are read from
`handle_ping` while the JVM runs, as on a device.

## 4. Blocking through the kernel

The bridge outranks the JVM (`PRIORITY_RT_1` = 21 over 15). On the POSIX
port a task blocked in a host `read()` looks *running* to the kernel, and
with time slicing off a running high-priority task starves everything
below it (`rtos_freertos.rs`, "The one invariant"). So the listener and the
accepted stream are non-blocking, `read_byte` polls with `vTaskDelay`
(1 ms with a host attached, 10 ms while waiting for one), and
`read_byte_timeout` is the same poll bounded to the device's 2 s. Writes
go straight to the socket; responses are under two kilobytes and the
kernel delivers what is queued even after the exec closes the fd, so
`drain_tx` is a flush. `spin_guard.rs` does not scan `hal/sim/`, and this is
a `vTaskDelay` poll, not a spin.

## 5. Sysmon on the host kernel

`TaskStatus_t` for `freertos-host/FreeRTOSConfig.h` (`configUSE_TRACE_FACILITY
1`, `configNUMBER_OF_CORES 1` so no affinity field, `configSTACK_DEPTH_TYPE
uint32_t`, run-time counter `uint32_t`, no `configRECORD_STACK_HIGH_ADDRESS`)
on a 64-bit host, where `UBaseType_t` is `unsigned long`:

```text
handle *void @0 | task_name *char @8 | task_number ulong @16 | task_state int @24 (+4)
current_priority ulong @32 | base_priority ulong @40 | run_time_counter u32 @48 (+4)
stack_base *void @56 | stack_high_water_mark u32 @64 (+4)      → 72 bytes, align 8
```

`freertos_rust::FreeRtosUBaseType` is `u32`, so the FFI is declared in the
module with `libc::c_ulong`; a test pins the size. Heap figures come from
`freertos_heap_shim`, i.e. the modeled device arena. `configGENERATE_RUN_TIME_STATS`
is off on the host, so CPU share reads `N/A`; the port's `ulPortGetRunTime()`
(`times()`-based) could feed it in two config lines if ever wanted.

## 6. The host tool

`tools/pdb/src/transport.rs`: `trait Link: Read + Write + Send { set_timeout }`,
implemented for `Box<dyn SerialPort>` and `UnixStream`; `open(target,
timeout)`; `classify`: an existing Unix socket → simulator; a missing path
ending in `.sock` → simulator (a simulator between its reboot's exec and its
rebind); anything else → serial at 115200. `-s sim` resolves, in `main`, to
the one socket under `temp_dir()/picodroid-sim/` that answers a PING and
refuses with the list when there are several. `devices::scan` appends the
simulators, tagged `[sim]`, and unlinks a socket whose connect is refused (a
killed simulator). `wait_for_reboot` skips the 4 s USB settle for a
simulator and polls the same path.

`scripts/pdb.sh` takes no board lease and injects no port for `-s sim` or
`-s <socket>`.

## 7. Several simulators at once

Each simulator listens on `pdb-<pid>.sock` under `temp_dir()/picodroid-sim/`
unless `PICODROID_SIM_PDB_SOCKET` names a path (Unix socket paths cap at
108 bytes; keep it short). Any number can run side by side: `pdb devices`
probes them all; `pdb -s sim` refuses with the candidates when there is
more than one, and `-s <socket>` names one. The LittleFS image's default
(`crates/picodroid-core/target/sim-fs.img`) is shared — give each
simulator its own `PICODROID_SIM_FS`, as the `sim-run.sh` lane does. The
control FIFO is per `sim-remote.sh` display already. A simulator that exits
unlinks its socket; a killed one leaves it for the next `pdb devices` to
prune.

## 8. Verification

- Unit: `hal/sim/pdb.rs` (transport over a `UnixStream::pair`, hang-up then
  accept, the handshake with a std thread as the JVM task, the late-park
  race, the 72-byte mirror), `packages.rs` (a dump restored into a fresh
  region rescans to the same directory), `tools/pdb/src/transport.rs`
  (target classification, open + timeout).
- `scripts/sim-run.sh --app pdb`: a headless launcher simulator driven by the
  release `pdb` binary over its own socket — ping, list, sysmon, an input
  tap that launches helloworld, an install that reboots (exec, warm boot,
  `ready: 2 apps`), a device-side reject of the other shrink mode's PAPK
  that the launcher survives, an uninstall that reboots, a final ping.
- Manual, 2026-09-13: every row above against a launcher simulator and a
  launcher-less one in `PICODROID_SIM_WAIT_FOR_INSTALL=1` mode; two
  simulators at once; a killed simulator's socket pruned. `pdb devices`
  listed the three bench boards beside the simulator.

## 9. Out of scope

A WiFi endpoint; region persistence across `sim.sh` launches; lifting the
park coordinator into core; CPU share in the host sysmon.
