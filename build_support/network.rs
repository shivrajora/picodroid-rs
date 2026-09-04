// SPDX-License-Identifier: GPL-3.0-only
//! FreeRTOS+TCP and link-driver compilation for a family's `build.rs`.
//!
//! Two builders, both family-neutral (docs/designs/network-seam-2026-09.md):
//!
//! - [`build_freertos_tcp`] compiles the vendored FreeRTOS+TCP plus the shared
//!   stack glue in `picodroid-core/net-freertos-tcp/` plus whatever link-driver
//!   sources the family lists in [`NetStackBuild`]. It never names a chip.
//! - [`build_cyw43_driver`] compiles the vendored cyw43 driver with the
//!   family's port file; a family whose `network_type` is `cyw43` calls it
//!   before `build_freertos_tcp`.
//!
//! Every translation unit is compiled against the *family's* `FreeRTOSConfig.h`
//! and kernel port headers, which is why the family's `build.rs` owns the
//! compile and core only owns the source.

use crate::config::collect_files;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The vendored FreeRTOS+TCP checkout (picodroid's fork), relative to the repo root.
const TCP_SUBMODULE: &str = "vendor/freertos-plus-tcp";
/// The vendored cyw43 driver checkout (picodroid's fork), relative to the repo root.
const CYW43_SUBMODULE: &str = "vendor/cyw43-driver";
/// The FreeRTOS kernel checkout, relative to the repo root.
const KERNEL_SUBMODULE: &str = "third_party/FreeRTOS-Kernel";

/// Where the shared stack glue lives (`net_init.c`, `libc_str.c`,
/// `FreeRTOSIPConfig.h`).
pub fn shared_dir(repo_root: &Path) -> PathBuf {
    repo_root.join("picodroid-core/net-freertos-tcp")
}

/// Map optional `net_*` board.toml keys to FreeRTOSIPConfig.h override
/// defines. The header's defaults are `#ifndef`-wrapped so a `-D` from here
/// wins; heap-constrained boards use these to shrink the IP stack's share of
/// the heap_4 arena. Values are validated as unsigned integers so a typo
/// fails the build here rather than as a C syntax error.
///
/// Every compile unit that includes FreeRTOSIPConfig.h must receive the same
/// overrides, so both builders take them.
pub fn net_config_overrides(props: &HashMap<String, String>) -> Vec<(String, String)> {
    const KEYS: [(&str, &str); 4] = [
        (
            "net_buffer_descriptors",
            "ipconfigNUM_NETWORK_BUFFER_DESCRIPTORS",
        ),
        ("net_tcp_rx_bytes", "ipconfigTCP_RX_BUFFER_LENGTH"),
        ("net_tcp_tx_bytes", "ipconfigTCP_TX_BUFFER_LENGTH"),
        ("net_tcp_win_segs", "ipconfigTCP_WIN_SEG_COUNT"),
    ];
    let mut defines = Vec::new();
    for (key, define) in KEYS {
        if let Some(v) = props.get(key) {
            let n: u32 = v.parse().unwrap_or_else(|_| {
                panic!("board.toml: {key} must be an unsigned integer, got '{v}'")
            });
            defines.push((define.to_string(), format!("({n})")));
        }
    }
    defines
}

/// Everything `build_freertos_tcp` needs from the family.
pub struct NetStackBuild<'a> {
    /// Absolute path to the repository root.
    pub repo_root: &'a Path,
    /// Directory holding the family's `FreeRTOSConfig.h` (e.g. `mcus/rp`).
    pub freertos_config_dir: &'a str,
    /// The kernel port's include directory
    /// (`third_party/FreeRTOS-Kernel/portable/<mcu toml freertos_port>`).
    pub kernel_port_include: &'a Path,
    /// The family's port directory (e.g. `src/hal/rp/port`), which holds
    /// `FreeRTOSIPConfig_family.h`.
    pub family_port_dir: &'a str,
    /// `heap_kb` from the MCU toml, injected as `configTOTAL_HEAP_SIZE`.
    pub heap_kb: u32,
    /// From [`net_config_overrides`].
    pub overrides: &'a [(String, String)],
    /// The link driver's C sources: `NetworkInterface_<X>.c`, a vendored
    /// `portable/NetworkInterface/<Vendor>/NetworkInterface.c` plus
    /// `Common/phyHandling.c` for an on-chip MAC, forwarders, ….
    pub link_sources: &'a [PathBuf],
    /// Extra include directories the link sources need (a driver's `src/`,
    /// `portable/NetworkInterface/include`, vendor HAL headers, …).
    pub extra_includes: &'a [PathBuf],
    /// Extra defines the link sources need (`CYW43_CONFIG_FILE`, …).
    pub extra_defines: &'a [(String, Option<String>)],
}

/// Compile FreeRTOS+TCP (IPv4 only, `BufferAllocation_2`), the shared stack
/// glue, and the family's link-driver sources into one `freertos_tcp` archive.
pub fn build_freertos_tcp(b: &NetStackBuild<'_>) {
    let tcp_src = b.repo_root.join(TCP_SUBMODULE).join("source");
    if !tcp_src.exists() {
        println!(
            "cargo:warning=FreeRTOS+TCP submodule not found at {TCP_SUBMODULE} — \
             run: git submodule update --init {TCP_SUBMODULE}"
        );
        return;
    }

    // Refuse to build against pristine upstream. The picodroid branch carries
    // the RST-during-connect wake fix (FreeRTOS_connect otherwise sleeps
    // forever when the peer refuses), and the socket layer's connect ladder
    // in picodroid-core is calibrated against it.
    {
        let tcp_ip = tcp_src.join("FreeRTOS_TCP_IP.c");
        let text = std::fs::read_to_string(&tcp_ip).expect("read vendored FreeRTOS_TCP_IP.c");
        assert!(
            text.contains("PICODROID"),
            "{TCP_SUBMODULE} is the unpatched upstream — run `git submodule sync && git submodule update --init {TCP_SUBMODULE}` to fetch the picodroid fork"
        );
    }

    let shared = shared_dir(b.repo_root);
    // A left-behind copy of the shared header in the family's port dir would
    // shadow it (or not) by include order; refuse rather than guess.
    let stale = Path::new(b.family_port_dir).join("FreeRTOSIPConfig.h");
    assert!(
        !stale.exists(),
        "{} exists: FreeRTOSIPConfig.h is shared (picodroid-core/net-freertos-tcp); \
         the family ships FreeRTOSIPConfig_family.h only",
        stale.display()
    );

    let freertos_include = b.repo_root.join(KERNEL_SUBMODULE).join("include");

    // The stack's own sources: no IPv6, no DHCPv6/ND/RA, no vendored
    // NetworkInterface drivers (the family lists the ones it wants in
    // `link_sources`), and BufferAllocation_2 only.
    let all_c_files = collect_files(&tcp_src, "c");
    let c_files: Vec<_> = all_c_files
        .into_iter()
        .filter(|p| {
            let s = p.to_string_lossy();
            !s.contains("IPv6")
                && !s.contains("DHCPv6")
                && !s.contains("_ND.c")
                && !s.contains("_RA.c")
                && !s.contains("portable/NetworkInterface/")
                && !s.contains("BufferAllocation_1")
                && !s.ends_with("CMakeLists.txt")
        })
        .collect();

    let mut build = cc::Build::new();
    build
        .include(tcp_src.join("include"))
        .include(tcp_src.join("portable/Compiler/GCC"))
        .include(&freertos_include)
        .include(b.kernel_port_include)
        .include(b.freertos_config_dir)
        .include(&shared)
        .include(b.family_port_dir)
        .define(
            "configTOTAL_HEAP_SIZE",
            format!("({} * 1024)", b.heap_kb).as_str(),
        )
        .warnings(false)
        .extra_warnings(false);
    for inc in b.extra_includes {
        build.include(inc);
    }
    for (k, v) in b.extra_defines {
        build.define(k, v.as_deref());
    }
    for (k, v) in b.overrides {
        build.define(k, v.as_str());
    }

    for f in &c_files {
        build.file(f);
    }
    // The shared glue: stack start-up + the application hooks, and the
    // string functions FreeRTOS+TCP's DNS files need on a libc-less target.
    build.file(shared.join("net_init.c"));
    build.file(shared.join("libc_str.c"));
    for f in b.link_sources {
        build.file(f);
    }

    build.compile("freertos_tcp");

    println!("cargo:rerun-if-changed={}", tcp_src.display());
    println!("cargo:rerun-if-changed={}", shared.display());
    // The whole port dir, not just the family header: a file that appears
    // there (a stale FreeRTOSIPConfig.h) must re-run the check above.
    println!("cargo:rerun-if-changed={}", b.family_port_dir);
    for f in b.link_sources {
        println!("cargo:rerun-if-changed={}", f.display());
    }
}

/// Compile the cyw43 WiFi driver (C sources from the vendored fork) with the
/// family's port file `{family_port_dir}/net/cyw43_port.c`.
///
/// Call it before [`build_freertos_tcp`]: the driver's one `strcmp` resolves
/// from `libc_str.c`, which lives in the `freertos_tcp` archive.
pub fn build_cyw43_driver(
    repo_root: &Path,
    freertos_config_dir: &str,
    kernel_port_include: &Path,
    family_port_dir: &str,
    heap_kb: u32,
    overrides: &[(String, String)],
) {
    let cyw43_dir = repo_root.join(CYW43_SUBMODULE);
    let cyw43_src = cyw43_dir.join("src");
    if !cyw43_src.exists() {
        println!(
            "cargo:warning=CYW43 driver submodule not found at {CYW43_SUBMODULE} — \
             run: git submodule update --init {CYW43_SUBMODULE}"
        );
        return;
    }

    let freertos_include = repo_root.join(KERNEL_SUBMODULE).join("include");

    let mut build = cc::Build::new();
    build
        .include(&cyw43_src)
        .include(&cyw43_dir)
        .include(family_port_dir)
        .include(shared_dir(repo_root))
        .include(&freertos_include)
        .include(kernel_port_include)
        .include(freertos_config_dir)
        .define(
            "configTOTAL_HEAP_SIZE",
            format!("({heap_kb} * 1024)").as_str(),
        )
        .define("CYW43_CONFIG_FILE", "\"cyw43_configport.h\"")
        .define("CYW43_USE_SPI", "1")
        .define("CYW43_LWIP", "0")
        .define("NDEBUG", None)
        .warnings(false)
        .extra_warnings(false);
    for (k, v) in overrides {
        build.define(k, v.as_str());
    }

    // The vendored driver must be picodroid's fork (shivrajora/cyw43-driver,
    // `picodroid` branch — see .gitmodules): it carries required gSPI
    // bring-up fixes (F2 boot gate, STATUS_ENABLE bus config, event-mask
    // bsscfg index, ioctl error-status logging).  A checkout pinned to
    // plain upstream fails WiFi bring-up at runtime, so fail the build
    // early with instructions instead.
    {
        let ll = cyw43_src.join("cyw43_ll.c");
        let text = std::fs::read_to_string(&ll).expect("read vendored cyw43_ll.c");
        assert!(
            text.contains("PICODROID"),
            "{CYW43_SUBMODULE} is the unpatched upstream — run `git submodule sync && git submodule update --init {CYW43_SUBMODULE}` to fetch the picodroid fork"
        );
    }

    let driver_sources = ["cyw43_ctrl.c", "cyw43_ll.c", "cyw43_spi.c", "cyw43_stats.c"];
    for src in &driver_sources {
        let p = cyw43_src.join(src);
        if p.exists() {
            build.file(&p);
        }
    }

    // The gSPI transport itself is Rust (`hal/rp/pio_spi.rs`, PIO-based);
    // its cyw43_spi_* symbols resolve from the Rust rlib at final link.
    build.file(format!("{family_port_dir}/net/cyw43_port.c"));

    build.compile("cyw43");

    println!("cargo:rerun-if-changed={}", cyw43_src.display());
    println!("cargo:rerun-if-changed={family_port_dir}/net/cyw43_port.c");
    println!("cargo:rerun-if-changed={family_port_dir}/cyw43_configport.h");
}
