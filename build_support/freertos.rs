//! FreeRTOS kernel compilation + linker glue (vectors, init_array).

use std::collections::HashMap;
use std::path::Path;

/// Compile the FreeRTOS kernel for the selected MCU family and emit the
/// linker fragments that wire CMSIS handler names to the kernel's naked-asm
/// handlers and keep `.init_array` in FLASH.
pub fn build(
    out: &Path,
    mcu: &HashMap<String, String>,
    mcu_toml_path: &str,
    mcu_family: &str,
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

    b.get_cc().include("src/hal/rp/port");
    if let Some(inc) = mcu.get("freertos_port_include") {
        b.get_cc().include(inc);
    }
    b.get_cc().define("LIB_PICO_MULTICORE", "1");
    b.get_cc().define("LIB_PICO_SYNC", "0");
    b.get_cc().define("LIB_PICO_TIME", "0");
    b.get_cc()
        .define("configUSE_DYNAMIC_EXCEPTION_HANDLERS", "0");

    b.compile().unwrap_or_else(|e| panic!("{}", e.to_string()));

    println!("cargo:rerun-if-changed={freertos_config_dir}/FreeRTOSConfig.h");

    // This FreeRTOS-Kernel port (Development Branch) uses CMSIS-style handler
    // names (SVC_Handler, PendSV_Handler, SysTick_Handler).  cortex-m-rt's
    // linker script uses PROVIDE(SVCall = DefaultHandler) etc.  A strong
    // assignment in a linker script fragment overrides PROVIDE(), wiring the
    // cortex-m-rt vector-table slots directly to the FreeRTOS naked-asm
    // handlers.  -u forces portasm.o out of libfreertos.a so the symbols exist.
    // RP-specific FreeRTOS ports with configUSE_DYNAMIC_EXCEPTION_HANDLERS=0
    // rename the CMSIS handlers to pico-sdk isr_* names.
    for sym in &["isr_svcall", "isr_pendsv", "isr_systick"] {
        println!("cargo:rustc-link-arg=-u");
        println!("cargo:rustc-link-arg={sym}");
    }
    let vectors_ld = out.join("freertos-vectors.x");
    std::fs::write(
        &vectors_ld,
        b"SVCall  = isr_svcall;\nPendSV  = isr_pendsv;\nSysTick = isr_systick;\n",
    )
    .unwrap();
    println!("cargo:rustc-link-arg=-T{}", vectors_ld.display());

    // Place .init_array in FLASH instead of RAM.  GCC's
    // __attribute__((constructor)) (used by the FreeRTOS RP port) emits
    // .init_array entries with the SHF_WRITE flag, which causes the default
    // linker rules to place them in RAM right after .bss/.uninit.  This
    // merges them into one LOAD segment with FileSiz == MemSiz, making UF2
    // converters (elf2uf2-rs, picotool) reject the ELF because the BSS
    // region appears to contain loadable data.  Moving .init_array to FLASH
    // keeps the BSS segment clean (NOBITS, FileSiz=0).
    let _ = mcu_family; // currently unused but reserved for family-specific tweaks
    let init_array_ld = out.join("init-array-flash.x");
    std::fs::write(
        &init_array_ld,
        b"SECTIONS {\n\
          \x20 .init_array : {\n\
          \x20   __init_array_start = .;\n\
          \x20   KEEP(*(.init_array .init_array.*))\n\
          \x20   __init_array_end = .;\n\
          \x20 } > FLASH\n\
          } INSERT AFTER .rodata;\n",
    )
    .unwrap();
    println!("cargo:rustc-link-arg=-T{}", init_array_ld.display());
}
