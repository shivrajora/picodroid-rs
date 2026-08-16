// SPDX-License-Identifier: GPL-3.0-only
//! CYW43 WiFi driver + FreeRTOS+TCP compilation (WiFi-only boards).
//!
//! The functions here are family-parametric: `mcu_family` selects the
//! `src/hal/<family>/port` directory holding C glue and config headers.
//! FreeRTOS+TCP itself is currently RP-only; if a future family wants
//! networking it'll likely use a different IP stack (esp-idf/lwIP, …) and
//! should add a parallel module rather than extending this one.

use crate::config::collect_files;
use std::path::Path;

/// Compile the CYW43 WiFi driver (C sources from vendor/cyw43-driver).
///
/// `repo_root` is the absolute path to the repository root.
pub fn build_cyw43_driver(
    mcu_family: &str,
    freertos_config_dir: &str,
    repo_root: &Path,
    heap_kb: u32,
) {
    let cyw43_src = repo_root.join("vendor/cyw43-driver/src");
    if !cyw43_src.exists() {
        println!(
            "cargo:warning=CYW43 driver submodule not found at vendor/cyw43-driver — \
             run: git submodule update --init vendor/cyw43-driver"
        );
        return;
    }

    let port_dir = format!("src/hal/{mcu_family}/port");
    let cyw43_dir = repo_root.join("vendor/cyw43-driver");
    let freertos_include = repo_root.join("third_party/FreeRTOS-Kernel/include");

    let mut build = cc::Build::new();
    build
        .include(&cyw43_src)
        .include(&cyw43_dir)
        .include(&port_dir)
        .include(&freertos_include)
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

    // TODO(esp): family-specific FreeRTOS port include. Today only RP2350W
    // ships networking (CYW43+FreeRTOS+TCP). Future families using a
    // different IP stack should not consume this path.
    if mcu_family == "rp" {
        build.include(repo_root.join(
            "third_party/FreeRTOS-Kernel/portable/ThirdParty/\
                 Community-Supported-Ports/GCC/RP2350_ARM_NTZ/non_secure",
        ));
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
            "vendor/cyw43-driver is the unpatched upstream — run `git submodule sync && git submodule update --init vendor/cyw43-driver` to fetch the picodroid fork"
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
    build.file(format!("{port_dir}/net/cyw43_port.c"));
    build.file(format!("{port_dir}/net/libc_str.c"));

    build.compile("cyw43");

    println!("cargo:rerun-if-changed={}", cyw43_src.display());
    println!("cargo:rerun-if-changed={port_dir}/net");
    println!("cargo:rerun-if-changed={port_dir}/cyw43_configport.h");
}

/// Compile FreeRTOS+TCP (C sources from vendor/freertos-plus-tcp).
///
/// `repo_root` is the absolute path to the repository root.
pub fn build_freertos_tcp(
    mcu_family: &str,
    freertos_config_dir: &str,
    repo_root: &Path,
    heap_kb: u32,
) {
    let tcp_src = repo_root.join("vendor/freertos-plus-tcp/source");
    if !tcp_src.exists() {
        println!(
            "cargo:warning=FreeRTOS+TCP submodule not found at vendor/freertos-plus-tcp — \
             run: git submodule update --init vendor/freertos-plus-tcp"
        );
        return;
    }

    // Same guard as the cyw43 fork: refuse to build against pristine
    // upstream. The picodroid branch carries the RST-during-connect wake fix
    // (FreeRTOS_connect otherwise sleeps forever when the peer refuses).
    {
        let tcp_ip = tcp_src.join("FreeRTOS_TCP_IP.c");
        let text = std::fs::read_to_string(&tcp_ip).expect("read vendored FreeRTOS_TCP_IP.c");
        assert!(
            text.contains("PICODROID"),
            "vendor/freertos-plus-tcp is the unpatched upstream — run `git submodule sync && git submodule update --init vendor/freertos-plus-tcp` to fetch the picodroid fork"
        );
    }

    let port_dir = format!("src/hal/{mcu_family}/port");
    let cyw43_src = repo_root.join("vendor/cyw43-driver/src");
    let freertos_include = repo_root.join("third_party/FreeRTOS-Kernel/include");

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
        .include(freertos_config_dir)
        .define(
            "configTOTAL_HEAP_SIZE",
            format!("({heap_kb} * 1024)").as_str(),
        )
        .include(&port_dir)
        .include(&cyw43_src)
        .define("CYW43_CONFIG_FILE", "\"cyw43_configport.h\"")
        .define("CYW43_USE_SPI", "1")
        .define("CYW43_LWIP", "0")
        .warnings(false)
        .extra_warnings(false);

    // TODO(esp): see comment in build_cyw43_driver.
    if mcu_family == "rp" {
        build.include(repo_root.join(
            "third_party/FreeRTOS-Kernel/portable/ThirdParty/\
                 Community-Supported-Ports/GCC/RP2350_ARM_NTZ/non_secure",
        ));
    }

    for f in &c_files {
        build.file(f);
    }

    build.file(format!("{port_dir}/net/NetworkInterface_CYW43.c"));
    build.file(format!("{port_dir}/net/net_init.c"));

    build.compile("freertos_tcp");

    println!("cargo:rerun-if-changed={}", tcp_src.display());
    println!("cargo:rerun-if-changed={port_dir}/FreeRTOSIPConfig.h");
    println!("cargo:rerun-if-changed={port_dir}/net/NetworkInterface_CYW43.c");
    println!("cargo:rerun-if-changed={port_dir}/net/net_init.c");
}
