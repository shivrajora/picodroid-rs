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
pub fn build_cyw43_driver(mcu_family: &str, freertos_config_dir: &str) {
    let cyw43_src = Path::new("vendor/cyw43-driver/src");
    if !cyw43_src.exists() {
        println!(
            "cargo:warning=CYW43 driver submodule not found at vendor/cyw43-driver — \
             run: git submodule update --init vendor/cyw43-driver"
        );
        return;
    }

    let port_dir = format!("src/hal/{mcu_family}/port");

    let mut build = cc::Build::new();
    build
        .include("vendor/cyw43-driver/src")
        .include("vendor/cyw43-driver")
        .include(&port_dir)
        .include("third_party/FreeRTOS-Kernel/include")
        .include(freertos_config_dir)
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
        build.include(
            "third_party/FreeRTOS-Kernel/portable/ThirdParty/\
             Community-Supported-Ports/GCC/RP2350_ARM_NTZ/non_secure",
        );
    }

    let driver_sources = [
        "vendor/cyw43-driver/src/cyw43_ctrl.c",
        "vendor/cyw43-driver/src/cyw43_ll.c",
        "vendor/cyw43-driver/src/cyw43_spi.c",
        "vendor/cyw43-driver/src/cyw43_stats.c",
    ];
    for src in &driver_sources {
        if Path::new(src).exists() {
            build.file(src);
        }
    }

    build.file(format!("{port_dir}/net/cyw43_bus_spi.c"));
    build.file(format!("{port_dir}/net/cyw43_port.c"));
    build.file(format!("{port_dir}/net/libc_str.c"));

    build.compile("cyw43");

    println!("cargo:rerun-if-changed=vendor/cyw43-driver/src");
    println!("cargo:rerun-if-changed={port_dir}/net");
    println!("cargo:rerun-if-changed={port_dir}/cyw43_configport.h");
}

/// Compile FreeRTOS+TCP (C sources from vendor/freertos-plus-tcp).
pub fn build_freertos_tcp(mcu_family: &str, freertos_config_dir: &str) {
    let tcp_src = Path::new("vendor/freertos-plus-tcp/source");
    if !tcp_src.exists() {
        println!(
            "cargo:warning=FreeRTOS+TCP submodule not found at vendor/freertos-plus-tcp — \
             run: git submodule update --init vendor/freertos-plus-tcp"
        );
        return;
    }

    let port_dir = format!("src/hal/{mcu_family}/port");

    let all_c_files = collect_files(tcp_src, "c");
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
        .include("vendor/freertos-plus-tcp/source/include")
        .include("vendor/freertos-plus-tcp/source/portable/Compiler/GCC")
        .include("third_party/FreeRTOS-Kernel/include")
        .include(freertos_config_dir)
        .include(&port_dir)
        .include("vendor/cyw43-driver/src")
        .define("CYW43_CONFIG_FILE", "\"cyw43_configport.h\"")
        .define("CYW43_USE_SPI", "1")
        .define("CYW43_LWIP", "0")
        .warnings(false)
        .extra_warnings(false);

    // TODO(esp): see comment in build_cyw43_driver.
    if mcu_family == "rp" {
        build.include(
            "third_party/FreeRTOS-Kernel/portable/ThirdParty/\
             Community-Supported-Ports/GCC/RP2350_ARM_NTZ/non_secure",
        );
    }

    for f in &c_files {
        build.file(f);
    }

    build.file(format!("{port_dir}/net/NetworkInterface_CYW43.c"));
    build.file(format!("{port_dir}/net/net_init.c"));

    build.compile("freertos_tcp");

    println!("cargo:rerun-if-changed=vendor/freertos-plus-tcp/source");
    println!("cargo:rerun-if-changed={port_dir}/FreeRTOSIPConfig.h");
    println!("cargo:rerun-if-changed={port_dir}/net/NetworkInterface_CYW43.c");
    println!("cargo:rerun-if-changed={port_dir}/net/net_init.c");
}
