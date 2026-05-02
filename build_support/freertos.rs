//! FreeRTOS kernel compilation + linker glue (vectors, init_array).
//!
//! All family-specific knobs live in `mcus/<family>/<mcu>.toml`. This module
//! reads them and feeds them to `freertos_cargo_build` + the linker. To add
//! a new chip family, populate the TOML keys below; no code changes here.
//!
//! TOML schema (all keys read by this module):
//!
//! - `freertos_port` — kernel port path, relative to FreeRTOS-Kernel/portable.
//! - `pico_shim` — extra C source compiled with the kernel.
//! - `freertos_port_extra_includes` — semicolon-separated extra C include
//!   paths.
//! - `freertos_c_defines` — semicolon-separated `KEY=VALUE` (or bare KEY)
//!   preprocessor defines applied to the kernel build.
//! - `freertos_vector_aliases` — semicolon-separated `CMSIS=portasm`
//!   pairs. Each pair forces the named portasm symbol out of the kernel
//!   archive (`-u`) and emits a strong linker assignment binding it to the
//!   CMSIS vector slot.
//! - `init_array_segment` — destination memory region for `.init_array`.
//!   When set, this module emits an `INSERT AFTER .rodata` linker fragment.
//!   Leave unset on platforms that don't need it.

use std::collections::HashMap;
use std::path::Path;

use crate::config::parse_str_list;

/// Compile the FreeRTOS kernel for the selected MCU and emit any linker
/// fragments declared by the MCU TOML.
pub fn build(
    out: &Path,
    mcu: &HashMap<String, String>,
    mcu_toml_path: &str,
    _mcu_family: &str,
    freertos_config_dir: &str,
) {
    let mut b = freertos_cargo_build::Builder::new();
    b.freertos("third_party/FreeRTOS-Kernel");
    b.freertos_config(freertos_config_dir);

    let freertos_port = mcu
        .get("freertos_port")
        .unwrap_or_else(|| panic!("MCU toml missing 'freertos_port': {mcu_toml_path}"));
    b.freertos_port(freertos_port);
    b.heap("heap_4.c");

    let pico_shim_c = mcu
        .get("pico_shim")
        .unwrap_or_else(|| panic!("MCU toml missing 'pico_shim': {mcu_toml_path}"));
    b.add_build_file(pico_shim_c);

    if let Some(includes) = mcu.get("freertos_port_extra_includes") {
        for inc in parse_str_list(includes) {
            b.get_cc().include(inc);
        }
    }

    if let Some(defines) = mcu.get("freertos_c_defines") {
        for def in parse_str_list(defines) {
            if let Some((k, v)) = def.split_once('=') {
                b.get_cc().define(k, v);
            } else {
                b.get_cc().define(&def, None::<&str>);
            }
        }
    }

    b.compile().unwrap_or_else(|e| panic!("{}", e.to_string()));

    println!("cargo:rerun-if-changed={freertos_config_dir}/FreeRTOSConfig.h");

    // CMSIS vector-table aliases. The FreeRTOS-Kernel port may use CMSIS
    // names (SVC_Handler, …) or pico-sdk-style (isr_svcall, …); the TOML
    // declares the binding so the linker wires cortex-m-rt's vector slots
    // to the kernel's actual handlers. -u forces the portasm objects out
    // of libfreertos.a so the symbols exist.
    if let Some(spec) = mcu.get("freertos_vector_aliases") {
        let mut aliases = String::new();
        for entry in parse_str_list(spec) {
            let (cmsis, portasm) = entry.split_once('=').unwrap_or_else(|| {
                panic!(
                    "freertos_vector_aliases entry not 'CMSIS=portasm' in {mcu_toml_path}: {entry}"
                )
            });
            println!("cargo:rustc-link-arg=-u");
            println!("cargo:rustc-link-arg={portasm}");
            aliases.push_str(&format!("{cmsis} = {portasm};\n"));
        }
        if !aliases.is_empty() {
            let vectors_ld = out.join("freertos-vectors.x");
            std::fs::write(&vectors_ld, aliases.as_bytes()).unwrap();
            println!("cargo:rustc-link-arg=-T{}", vectors_ld.display());
        }
    }

    // .init_array placement. RP needs this in FLASH because GCC's
    // __attribute__((constructor)) emits .init_array entries with SHF_WRITE,
    // which the default rules place in RAM right after .bss/.uninit. That
    // merges them into one LOAD segment with FileSiz == MemSiz, making UF2
    // converters reject the ELF. Moving .init_array to FLASH keeps the BSS
    // segment clean (NOBITS, FileSiz=0). Other families (e.g. ESP) don't
    // need this and should leave `init_array_segment` unset.
    if let Some(seg) = mcu.get("init_array_segment") {
        let init_array_ld = out.join("init-array-flash.x");
        std::fs::write(
            &init_array_ld,
            format!(
                "SECTIONS {{\n\
                 \x20 .init_array : {{\n\
                 \x20   __init_array_start = .;\n\
                 \x20   KEEP(*(.init_array .init_array.*))\n\
                 \x20   __init_array_end = .;\n\
                 \x20 }} > {seg}\n\
                 }} INSERT AFTER .rodata;\n"
            ),
        )
        .unwrap();
        println!("cargo:rustc-link-arg=-T{}", init_array_ld.display());
    }
}
