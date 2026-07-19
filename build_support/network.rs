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

    let driver_sources = ["cyw43_ctrl.c", "cyw43_ll.c", "cyw43_spi.c", "cyw43_stats.c"];
    for src in &driver_sources {
        let p = cyw43_src.join(src);
        if p.exists() {
            build.file(&p);
        }
    }

    build.file(format!("{port_dir}/net/cyw43_bus_spi.c"));
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
