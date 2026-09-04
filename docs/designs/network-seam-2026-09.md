# Networking seams for FreeRTOS + FreeRTOS+TCP families

Status: in progress (2026-09-03). Branch `refactor/network-seam`, worktree
`.claude/worktrees/network-seam`, base `main` at `fedf6cb`.

Successor to `family-neutral-residue.md` §6 ("Phase N — networking") and to
`porting-seam-2026-09.md` E8, which deferred networking to "its own doc".
This is that doc. Every stage is executed from here; when reality differs,
an amendment is appended in §10, never a silent change.

## 0. Why this exists

Two repo words used below. A *seam* is a place where core code and family code meet: a trait, a macro, or a C symbol. The *ratchet* is the size check in pre-commit that fails when the firmware grows.

The ask: design the right platform interfaces for networking, with Ethernet
coming later, so that porting picodroid to a new chip family is easy as long
as the family runs FreeRTOS and FreeRTOS+TCP.

What the tree looked like before this work:

- One contract exists: `HalNet` (14 socket functions, `hal/traits.rs`) plus
  `set_hal_net!`. It is family-neutral and stays as it is.
- Everything below the sockets is RP-family code although most of it has no
  RP content: the FreeRTOS+TCP socket bindings (`hal/rp/net.rs`, 381 lines,
  zero RP lines), the stack glue C (`net_init.c`, `libc_str.c`,
  `FreeRTOSIPConfig.h`), and the cyw43 WiFi task (`wifi_task.rs`). A second
  family would copy about 700 lines word for word.
- The build hard-codes both the chip family (`mcu_family == "rp"` picks the
  kernel port include) and the WiFi chip (cyw43 defines and file names live
  inside the generic FreeRTOS+TCP build function; `network_type` accepts only
  `"cyw43"`).
- One RP-specific line hides in shared-looking C: the timer address
  `0x400B000C` in `net_init.c`. One cyw43-specific name hides there too:
  `pxCYW43_FillInterfaceDescriptor`.
- Java cannot tell WiFi from Ethernet. `FEATURE_WIFI` answers "any network",
  and `NetworkInfo` has no `getType()`.
- FreeRTOS+TCP already defines the driver contract Ethernet needs
  (`NetworkInterface_t`: `pfInitialise`, `pfOutput`, `pfGetPhyLinkStatus`).
  We invent no C-level driver interface. The vendored tree ships drivers for
  many on-chip MACs plus `Common/phyHandling.c` for MDIO PHYs.

Decisions made with the user before planning:

1. The seam must fit BOTH Ethernet shapes: an SPI MAC with an interrupt pin
   (WIZnet W5500 in MACRAW mode; the W5500-EVB-Pico boards are RP boards) and
   an on-chip MAC with an external PHY (STM32/NXP style, where the vendored
   driver runs its own EMAC task). No Ethernet driver is written here.
2. The cyw43 driver stays in `platforms/rp` as the reference link driver.
   Core gets the contracts and the shared layers only.
3. Java gets `picodroid.net.ConnectivityManager` (`TYPE_WIFI = 1`,
   `TYPE_ETHERNET = 9`), `NetworkInfo.getType()` and
   `PackageManager.FEATURE_ETHERNET`; `hasSystemFeature` answers by link kind.

Outcome: a FreeRTOS + FreeRTOS+TCP family writes one C file (its link driver,
against FreeRTOS+TCP's own driver struct), one small Rust type (`NetLink`),
one entropy function, one small config header and one spawn line. Sockets,
stack start-up, the five FreeRTOS+TCP application hooks and the IP config
policy are written once in core. Java learns the link kind from a build-time
table.

## 1. Scale

| File | Lines | Goes to | Stage |
|---|---|---|---|
| `picodroid-core/src/drivers/cyw43.rs` | 173 | `platforms/rp/src/hal/rp/cyw43/mod.rs` | S1 |
| `platforms/rp/src/hal/rp/net.rs` | 381 | `picodroid-core/src/hal/freertos_tcp/mod.rs` | S4 |
| `platforms/rp/src/hal/rp/wifi_task.rs` | 168 | ~40 to core (`run_link_task`, `picodroid_net_ip_event`), ~120 to `hal/rp/cyw43/link.rs`; file deleted | S4, S5 |
| `.../port/net/net_init.c` | 151 | `picodroid-core/net-freertos-tcp/` (~140) | S3 |
| `.../port/net/libc_str.c` | 43 | `picodroid-core/net-freertos-tcp/` | S3 |
| `.../port/FreeRTOSIPConfig.h` | 115 | `picodroid-core/net-freertos-tcp/` (~110) + `FreeRTOSIPConfig_family.h` (~20) | S3 |

Stays in the family: `NetworkInterface_CYW43.c` (224), `cyw43_port.c` (377),
`cyw43_configport.h` (164), `pio_spi.rs` (606), `trng.rs` (~110), the
host-wake block in `gpio.rs`. About 690 lines enter core; 173 leave it.

## 2. Decisions

### D1 — Shared C lives in `picodroid-core/net-freertos-tcp/`

A non-`src` directory in core, not `platforms/shared/net-freertos-tcp/` as
`family-neutral-residue.md` §6(1) said. There is already an example of this: `picodroid-core/freertos-host/`
is a C directory compiled by a build script. A porter's model stays "core is
not mine, `platforms/<family>` is mine". The shadow-twin guard compares only
the two `src` trees, so this location is safe. Compile ownership stays with the
family's `build.rs` (every FreeRTOS+TCP unit needs the family's
`FreeRTOSConfig.h`), exactly as §6(1) wanted.

### D2 — The link driver defines one fixed C symbol

`NetworkInterface_t *pxPicodroidNetLink_FillInterfaceDescriptor(BaseType_t, NetworkInterface_t *)`.
Bound at link time like the `__pd_*` shims; a missing driver is a link error;
no runtime registration; the Rust side needs no C types. A vendored driver with
its own `pxXXX_FillInterfaceDescriptor` gets a five-line forwarder.

### D3 — One entropy seam: `uint32_t picodroid_port_entropy32(void)`

Defined by the family in Rust with `#[no_mangle]`, as §6(2) planned. The LCG
and the RP timer address leave shared code. The family owns "how random is
this chip" and its own fallback (RP: TRNG word when ready, else a timer-mixed
LCG, XOR-mixing every TRNG word as before).

### D4 — The socket layer is `FreeRtosTcpNet` in core, behind feature `freertos-tcp`

`pub struct FreeRtosTcpNet; impl HalNet` in
`picodroid-core/src/hal/freertos_tcp/mod.rs`, gated on the core Cargo feature
`freertos-tcp` AND `cfg(has_network)` AND `not(any(test, feature = "sim"))`.
The family registers it with `set_hal_net!(FreeRtosTcpNet)`. Same shape as
`LittleFsHal` (feature `littlefs`, `set_hal_fs!(LittleFsHal)`). Changes what §6(3),
which wanted a board key: the IP stack is a family choice, not a board choice.
`not(test)` keeps `FreeRTOS_*` out of core's tests. `not(sim)` keeps host
sockets in the simulator, whose build also sees `has_network` from the W
board's `board.toml`.

The seam guard in `rtos/mod.rs` forbids kernel names (`xTask…`) in core, so
the moved connect ladder times itself with the clock facade
(`hal::system_clock::elapsed_realtime_nanos`) instead of `xTaskGetTickCount`.
The 1 ms-tick assumption behind `SO_RCVTIMEO` becomes a
`_Static_assert(configTICK_RATE_HZ == 1000, …)` in the shared C.

### D5 — Where the new names live

`NetLink` in `hal/traits.rs` (already scanned by the porting checklist test),
`LinkKind` in `hal/types.rs`, `run_link_task` and `picodroid_net_ip_event` in
`hal/freertos_tcp/mod.rs`. `EXPECTED_SEAM_ITEMS` goes 41 → 42.

### D6 — The runner is core code; the driver and the spawn stay in the family

Changes what §6(4) ("`wifi_task` → core over `Rtos::spawn`"). User decision 2. The
spawn stays in `boot_tasks.rs` so the `task_affinity` scans keep seeing it and
the family keeps choosing core, stack and priority. No new `TaskKind`.

### D7 — `FreeRTOSIPConfig.h` is shared; the family ships `FreeRTOSIPConfig_family.h`

The shared header's first include is the family header. The family header
defines `ipconfigIP_TASK_AFFINITY` (RP) and may override priority and stack
size; the shared header `#ifndef`-defaults those two and never defines
affinity. Keeps the RP affinity scan a file scan. A single-core family may
leave affinity out: FreeRTOS+TCP defaults it to 0 and only uses it when > 0.

### D8 — Java's link kind is a build fact

`build_support/board_cfg.rs` holds `KNOWN_NETWORK_TYPES: [(type, kind)]` =
`[("cyw43", "wifi")]` and emits `cfg(network_link_wifi)` or
`cfg(network_link_ethernet)` next to `network_<type>`. Core's Cargo features
become `network-wifi` / `network-ethernet` (each turns on `has-network` too);
`network-cyw43` goes away. Correct in sim and on device with no runtime state;
`hasSystemFeature` and `getType()` are `cfg!()`s. A new link chip is one table
row. `has_network = true` without `network_type` becomes a build error.

### D9 — `ConnectivityManager` is constants-only; `getType()` is a static native

`TYPE_NONE = -1`, `TYPE_WIFI = 1`, `TYPE_ETHERNET = 9` (Android's values).
`NetworkInfo.getType()` matches the existing static style of `isConnected()`.
`TYPE_NONE` gives the no-network stub an honest answer.

## 3. Seams

### 3.A Rust, core

```rust
// hal/types.rs
pub enum LinkKind { Wifi, Ethernet }          // for logs; Java reads the cfgs

// hal/traits.rs
pub trait NetLink {
    const KIND: LinkKind;
    const NAME: &'static str;                 // "cyw43", "w5500", "emac"
    const SERVICE_TIMEOUT_MS: Option<u32>;    // None: no host service loop
    fn init(&mut self) -> Result<(), i32>;    // before the stack exists
    fn mac(&mut self) -> [u8; 6];             // chip OTP, or board/family
    fn bring_up(&mut self);                   // after the stack started
    fn service(&mut self);                    // one pass per wake or timeout
}

// hal/freertos_tcp/mod.rs   (feature freertos-tcp, has_network, not(test|sim))
pub struct FreeRtosTcpNet;                    // impl HalNet, moved from net.rs
extern "C" { fn picodroid_net_stack_init(mac: *const u8); }
#[no_mangle] pub extern "C" fn picodroid_net_ip_event(up: u32, ip_nbo: u32);
pub fn run_link_task<L: NetLink>(mut link: L);
```

`run_link_task`: log `net: link {NAME} init`; `link.init()` (on `Err` log and
park forever on `task_wait_notification(Timeout::Forever)`); `link.mac()` and
log it; `picodroid_net_stack_init(mac)`; `link.bring_up()`; then if
`SERVICE_TIMEOUT_MS` is `Some(t)`: loop `{ task_wait_notification(Timeout::Ms(t)); link.service() }`,
else log and return (the family's task ends; task bodies may return because
the RP trampoline deletes the task afterwards).

### 3.B Rust, family reference (`platforms/rp/src/hal/rp/cyw43/link.rs`)

`Cyw43Link: NetLink` — `KIND = Wifi`, `NAME = "cyw43"`,
`SERVICE_TIMEOUT_MS = Some(1000)`; `init` = `cyw43::init`,
`set_poll_task(task_current())`, `wifi_set_up(STA)`, `hostwake::init()` (must
run on this task, on core 1); `mac` = `cyw43::get_mac()`; `bring_up` = the
SSID/PASS/AUTH `option_env!` join, moved word for word; `service` =
`INSTR_CYW43_POLLS += 1; cyw43::poll()`.

### 3.C C symbols

| Symbol | Defined by | Called by |
|---|---|---|
| `picodroid_net_stack_init(const uint8_t mac[6])` | shared `net_init.c` | core `run_link_task` |
| `pxPicodroidNetLink_FillInterfaceDescriptor(BaseType_t, NetworkInterface_t *)` | the link driver | shared `net_init.c` |
| `picodroid_port_entropy32(void)` | family (`hal/rp/entropy.rs`) | shared `net_init.c` (both random hooks) |
| `picodroid_net_ip_event(uint32_t, uint32_t)` | core Rust | shared `net_init.c` |
| `xGetPhyLinkStatus`, `pcApplicationHostnameHook`, `xApplicationDHCPHook_Multi` | link driver / shared C (unchanged) | FreeRTOS+TCP |

### 3.D Build (`build_support/network.rs`)

```rust
pub fn shared_dir(repo_root: &Path) -> PathBuf;      // picodroid-core/net-freertos-tcp
pub struct NetStackBuild<'a> {
    pub repo_root: &'a Path,
    pub freertos_config_dir: &'a str,                 // mcus/<family>
    pub kernel_port_include: &'a Path,                // FreeRTOS-Kernel/portable/<mcu toml freertos_port>
    pub family_port_dir: &'a str,                     // src/hal/<family>/port (FreeRTOSIPConfig_family.h)
    pub heap_kb: u32,
    pub overrides: &'a [(String, String)],            // net_config_overrides(board), unchanged
    pub link_sources: &'a [PathBuf],                  // NetworkInterface_<X>.c, phyHandling.c, forwarders
    pub extra_includes: &'a [PathBuf],
    pub extra_defines: &'a [(String, Option<String>)],
}
pub fn build_freertos_tcp(b: &NetStackBuild<'_>);    // no mcu_family, no cyw43 identity
pub fn build_cyw43_driver(repo_root, freertos_config_dir, kernel_port_include, family_port_dir, heap_kb, overrides);
```

`build_freertos_tcp` keeps the check that the submodule is picodroid's fork and the source filter, always
compiles `shared_dir/net_init.c` and `shared_dir/libc_str.c` (FreeRTOS+TCP's
DNS files need the string functions, not only cyw43), and panics if a stale
`{family_port_dir}/FreeRTOSIPConfig.h` exists. `build_cyw43_driver` must run
before `build_freertos_tcp` so `cyw43_ll.c`'s one `strcmp` resolves from the
FreeRTOS+TCP archive.

### 3.E Cfgs, features, board keys

- `board_cfg.rs`: `KNOWN_NETWORK_TYPES = [("cyw43", "wifi")]`,
  `LINK_KINDS = ["wifi", "ethernet"]`; check-cfgs for `has_network`, every
  `network_<type>`, every `network_link_<kind>`; the feature cross-check loops
  over `LINK_KINDS`.
- `picodroid-core/Cargo.toml`: `network-wifi = ["has-network"]`,
  `network-ethernet = ["has-network"]`, `freertos-tcp = []`.
- `picodroid-core/build.rs` no-board build (features only, no board.toml): by kind.
- `platforms/rp/Cargo.toml`: W boards forward `picodroid-core/network-wifi`;
  the always-on list gains `"freertos-tcp"`.
- No new board keys.

## 4. Stages

Each stage builds all five boards, passes `./scripts/pre-commit --full`, and
is checked on the Pico 2 W by hand. Lowest risk first. S6 depends only on S2.

- **CHECKS**: `cargo fmt --all`; the three sim smokes; `./scripts/pre-commit`;
  `--full` before push; build `pico_enviro_mon_w` by hand when C or cfgs changed.
- **HW-NET**: board lease, `.wifi-creds.env` in the environment,
  `PICODROID_NET_TEST_HOST` set, `flash.sh --release --board testbench_rp2350w --app netdemo`
  (release ELF, RTT; see A2 for why not debug). Expect `wifi: cyw43 set_up (STA)` → `wifi: mac …` →
  `wifi: join "…" requested` → `net: up, ip …` → `NetDemo: Network connected: true`
  → `NetDemo: Received N bytes` → `NetDemo: Done.`; then `http_get` with
  `status=200`.
- **SIZE-W**: `.text` of a release `testbench_rp2350w` build, recorded in §9.

| # | Stage | Hardware |
|---|---|---|
| S0 | Baseline, this doc | HW-NET, SIZE-W |
| S1 | cyw43 bindings leave core (no seam change) | HW-NET |
| S2 | Link kind is a build fact (cfgs, features) | HW-NET (cheap) |
| S3 | Shared C stack glue; entropy and descriptor seams; stack-generic build | HW-NET (DHCP proves entropy) |
| S4 | Socket layer to core (`FreeRtosTcpNet`) | HW-NET + the three netdemo failure cases |
| S5 | `NetLink` + `run_link_task`; cyw43 becomes a link driver | HW-NET + NONET soak |
| S6 | Java: `ConnectivityManager`, `getType()`, `FEATURE_ETHERNET` | picoenvmon joins on the W board |
| S7 | Close out: measurements, amendments, docs | — |

Stage details are in the approved plan; each stage's amendment below records
what actually happened.

## 5. End state: what stays in `platforms/rp`

- `hal/rp/cyw43/{mod.rs,link.rs}`: the reference link driver.
- `hal/rp/port/net/{NetworkInterface_CYW43.c,cyw43_port.c}`, `port/cyw43_configport.h`.
- `hal/rp/port/FreeRTOSIPConfig_family.h`: the affinity choice.
- `hal/rp/{pio_spi.rs,trng.rs,entropy.rs}` and the host-wake block in `gpio.rs`.
- `boot_tasks.rs`: the spawn of `run_link_task(Cyw43Link)` on core 1.
- `build.rs`: the `match network_type` dispatch.

## 6. Ethernet readiness

**W5500 over SPI on an RP board.** A `board.toml` with `network_type = "w5500"`
and pin keys; one table row `("w5500", "ethernet")`; `NetworkInterface_W5500.c`
defining the fill symbol, `xGetPhyLinkStatus`, and an RX drain; `W5500Link:
NetLink` (`SERVICE_TIMEOUT_MS = Some(1000)`, MAC from a board key or the chip
unique id, `bring_up` unmasks INT, `service` drains RX) plus an `IO_IRQ_BANK0`
arm for the INT pin; a `"w5500"` arm in `build.rs`; a spawn. Nothing in core,
nothing in `net-freertos-tcp/`, nothing in Java.

**On-chip MAC + PHY on a new family.** `FreeRTOSConfig.h` and
`FreeRTOSIPConfig_family.h`; the vendored `NetworkInterface.c` plus
`Common/phyHandling.c` plus a six-line forwarder for the fill symbol;
`build_freertos_tcp(&NetStackBuild { link_sources: […], extra_includes:
[portable/NetworkInterface/include, …] })`; `EmacLink: NetLink` with
`SERVICE_TIMEOUT_MS = None` (the runner returns after `bring_up`; the IP task
retries `pfInitialise` every 3 s until auto-negotiation finishes);
`picodroid_port_entropy32`; `set_hal_net!(FreeRtosTcpNet)`; a table row.

Both shapes use the same six items: `NetLink`, `run_link_task`,
`FreeRtosTcpNet`, `pxPicodroidNetLink_FillInterfaceDescriptor`,
`picodroid_port_entropy32`, `FreeRTOSIPConfig_family.h`.

## 7. Deferred and open

- `TaskKind::NetLink` (spawning the link task through the RTOS seam): until a
  family asks.
- `NetLink::init` returns `Result<(), i32>`; `bool` would also do.
- `TYPE_NONE` is Android's hidden value; drop it if the surface must stay
  strictly public Android.
- A toolchain that ships newlib may clash with `libc_str.c`; a future
  `libc_shims: false` field would handle it.
- W-board device rows in `scripts/hil-tests.conf` remain the NET-7 handover's job.
- The shrink map: new Java names stay un-shrunk until a release cut on `main`.

## 8. Docs and guards

- `picodroid_core::porting`: item 8 "The network"; re-exports `NetLink`,
  `LinkKind`, `FreeRtosTcpNet`, `run_link_task`; `EXPECTED_SEAM_ITEMS = 42`.
- Porting guide: "Networking is family-owned today" becomes "The network"
  (what core gives you, what you write, the reference).
- New host tests: `hal/freertos_tcp/config_guard.rs` (shared header invariants
  and the two seam symbols); `hal/rp/cyw43/config_guard.rs`
  (`CYW43_IOCTL_TIMEOUT_US` never below 500 000).
- `task_affinity.rs`: the affinity scan points at `FreeRTOSIPConfig_family.h`.
- Amendments: `family-neutral-residue.md` B18, `porting-seam-2026-09.md` A11.

## 9. Measurements

`.text` / `.data` / `.bss` of the release `testbench_rp2350w` firmware
(`parity-bench.sh --size-only`, app `helloworld`, credentials from
`.wifi-creds.env` loaded for every measurement so the baked SSID length is the
same each time). W boards are not ratcheted; this table is the record.

| Stage | text | data | bss | Δ text vs S0 |
|---|---|---|---|---|
| S0 baseline (`fedf6cb`) | 1,223,218 | 4 | 527,800 | 0 |
| S1 cyw43 bindings leave core | 1,223,218 | 4 | 527,800 | 0 |
| S2 link kind is a build fact | 1,223,218 | 4 | 527,800 | 0 |
| S3 shared C glue, entropy + descriptor seams | 1,223,158 | 4 | 527,800 | −60 |

## 10. Amendments

### A1 — Started before the `third_party/` rename landed (2026-09-03)

The shared checkout holds another session's uncommitted rename of `vendor/`
to `third_party/` (branch `chore/consolidate-third-party`, no commits beyond
`main`). This branch starts from `main` at `fedf6cb`, where the submodules
are still `vendor/lvgl`, `vendor/freertos-plus-tcp`, `vendor/cyw43-driver`.
The worktree links those paths to the shared checkout's physical
`third_party/` copies. Before S3 (which rewrites `build_support/network.rs`
and moves files under `port/`), merge `main` into this branch if the rename
has landed, and write the submodule paths as they are on `main` at that time.

### A2 — The debug-profile W firmware on `main` faults at boot; HW-NET uses `--release` (2026-09-03)

`flash.sh --board testbench_rp2350w --app netdemo` (the default debug profile)
programmed the chip, then probe-rs reported `Firmware exited unexpectedly:
Exception` before a single RTT line, and a later attach found a locked-up
core. The same worktree's non-W debug `helloworld` on `testbench_rp2350` ran
and printed. The W images leave almost no RAM for the boot stack that
flip-link carves from what `.data` + `.bss` leave free: 4,464 B in debug,
4,676 B in release (RAM is 532,480 B; the FreeRTOS heap arena is most of
`.bss`). Every validated Pico 2 W run on record used `--release`
(`reference_pico2w_wifi_debug_recipes`), so this is a pre-existing limit of
the debug W image, not something this branch introduced. HW-NET therefore
flashes `--release`; line-number traces are not available on the W board.
Worth its own follow-up: give the W boards a boot-stack budget, or fail the
link when the headroom drops below a floor.

### A3 — S0 hardware baseline is green (2026-09-03)

Release `netdemo` on `testbench_rp2350w`: `wifi: cyw43 set_up (STA)` →
`wifi: mac 2c:cf:67:…` → `wifi: join "…" requested` → `net: up, ip 192.168.1.90`
→ `NetDemo: Network connected: true (waited 5500 ms)` → `Sent 5 bytes` →
`Received 5 bytes` → `Done.` Release `http_get`: `GET … status=200
content-length=200`, `read 200 body bytes`, `POST … status=200`, `GET with
headers … status=200 message=OK`. The test host ran a TCP echo on :7000 and
a Python HTTP server on :8000 (`scratchpad/net_listeners.py`); both logged
the board's connections. The two `cyw43: ioctl cmd 263 error status -5`
lines are NET-1 (known, harmless).

### A4 — S1 done: cyw43 bindings live in the family (2026-09-03)

`picodroid-core/src/drivers/cyw43.rs` → `platforms/rp/src/hal/rp/cyw43/mod.rs`,
content unchanged; `wifi_task.rs` reaches it as `super::cyw43`. Core has no
cyw43 module any more (the `network-cyw43` feature and `network_cyw43` cfg
still exist until S2). The bindings are not re-exported from `hal/mod.rs`:
nothing outside the family's `hal::rp` needs them yet, and an unused
re-export is a clippy error under `--deny=warnings` (S5 adds the re-export
when `boot_tasks.rs` names `Cyw43Link`). Checks: W-board clippy clean, the
three sim smokes, `./scripts/pre-commit` (ratchet +0), release `netdemo` on
the Pico 2 W green (`net: up, ip 192.168.1.90`, echo round trip, `Done.`).
Size: identical to S0.

### A5 — S2: the link kind is a build fact (2026-09-03)

`build_support/board_cfg.rs` now holds `KNOWN_NETWORK_TYPES: [(type, kind)]`
= `[("cyw43", "wifi")]`, `LINK_KINDS = ["wifi", "ethernet"]` and
`network_link_kind()`. `emit_network_cfgs` declares check-cfgs for every
type and kind, emits `network_<type>` and `network_link_<kind>`, and now
refuses `has_network = true` without a `network_type` (that used to pass
silently). `assert_forwarded_features_match` loops over `LINK_KINDS`. Core's
Cargo features are `network-wifi` / `network-ethernet` (both imply
`has-network`); `network-cyw43` is gone, and the no-board build (features only, no board.toml) in
`picodroid-core/build.rs` emits the kind cfg, never a chip cfg. The two W
boards forward `picodroid-core/network-wifi`. Proven by hand on
`testbench_rp2350w`: no `network_type` → `board.toml: has_network = true
needs a network_type (known: ["cyw43"])`; `network_type = "w5500"` →
`board.toml: unknown network_type 'w5500' (known: ["cyw43"])`; forwarding
`network-ethernet` instead → `board 'testbench_rp2350w' declares network
link kind wifi=true but picodroid-core feature 'network-wifi' is off.`
Nothing in core gates on `network_cyw43` any more; the family still does
(host-wake, `pio_spi`, `trng`, the cyw43 task), which is what a per-chip cfg
is for. Checks: `./scripts/pre-commit --full` green (8m23s, ratchet +0), the three
sim smokes, release `netdemo` on the Pico 2 W green, W image byte-identical.

### A6 — S3: the stack glue is shared, the build is stack-generic (2026-09-03)

`net_init.c`, `libc_str.c` and `FreeRTOSIPConfig.h` moved to
`picodroid-core/net-freertos-tcp/` (README there). `net_init.c` binds two
link-time symbols instead of naming a chip: the link driver's
`pxPicodroidNetLink_FillInterfaceDescriptor` (renamed in
`NetworkInterface_CYW43.c`) and the family's `picodroid_port_entropy32`
(new `platforms/rp/src/hal/rp/entropy.rs`: TRNG word when buffered, else a
timer-mixed LCG; `trng.rs` now exposes `try_random_u32()` and its RP timer
line left the shared code). It also carries a `_Static_assert` that the tick
is 1 ms. The shared `FreeRTOSIPConfig.h` includes the family's
`FreeRTOSIPConfig_family.h` first (RP: the affinity choice and its
rationale) and never defines affinity itself; priority and stack are
`#ifndef` defaults. `build_support/network.rs` is stack-generic:
`build_freertos_tcp(&NetStackBuild { … })` takes the kernel port include,
the family port dir and the link sources; it refuses a stale
`FreeRTOSIPConfig.h` in the family port dir and watches that whole
directory so a file appearing there re-runs the check. `build_cyw43_driver`
lost its `mcu_family == "rp"` branch and the cyw43 identity left the
FreeRTOS+TCP unit. `platforms/rp/build.rs` gates on `has_network` and
dispatches on `network_type`. Two host tests pin the invariants:
`picodroid-core/src/hal/freertos_tcp/config_guard.rs` (shared header
invariants, the two seam symbols, no chip name in the shared glue) and
`platforms/rp/src/hal/rp/cyw43/config_guard.rs` (ioctl timeout floor, the
family header holds only family choices). The affinity scan in
`task_affinity.rs` reads the family header. Checks: `./scripts/test.sh`
(the new guards, the porting checklist, the affinity scans), the stale
header rejection proven by hand, `pico_enviro_mon_w` built by hand,
`./scripts/pre-commit --full` green (5m57s), sim smokes, release `netdemo`
on the Pico 2 W green (DHCP lease and TCP connect prove the entropy seam).
Size: −60 B on the W image.
