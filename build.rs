//! This build script copies the `memory.x` file from the crate root into
//! a directory where the linker can always find it at build time.
//! For many projects this is optional, as the linker always searches the
//! project root directory -- wherever `Cargo.toml` is. However, if you
//! are using a workspace or have a more complicated build setup, this
//! build script becomes required. Additionally, by requesting that
//! Cargo re-run the build script whenever `memory.x` is changed,
//! updating `memory.x` ensures a rebuild of the application with the
//! new memory settings.

use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let is_arm_embedded = target_arch == "arm";

    if is_arm_embedded {
        let chip_rp2350 = env::var("CARGO_FEATURE_CHIP_RP2350").is_ok();

        // Copy the appropriate memory layout into OUT_DIR so the linker finds it.
        let memory_src = if chip_rp2350 {
            "memory_rp2350.x"
        } else {
            "memory.x"
        };
        let memory_bytes =
            fs::read(memory_src).unwrap_or_else(|e| panic!("Failed to read {memory_src}: {e}"));
        File::create(out.join("memory.x"))
            .unwrap()
            .write_all(&memory_bytes)
            .unwrap();
        println!("cargo:rustc-link-search={}", out.display());
        println!("cargo:rerun-if-changed={memory_src}");

        let mut b = freertos_cargo_build::Builder::new();

        // Path to FreeRTOS kernel or set ENV "FREERTOS_SRC" instead
        b.freertos("third_party/FreeRTOS-Kernel");
        b.freertos_config("src"); // Location of `FreeRTOSConfig.h`
                                  // ARM_CM33_NTZ/non_secure for Cortex-M33 (RP2350), ARM_CM0 for Cortex-M0+ (RP2040)
        let freertos_port = if chip_rp2350 {
            "GCC/ARM_CM33_NTZ/non_secure"
        } else {
            "GCC/ARM_CM0"
        };
        b.freertos_port(freertos_port);
        b.heap("heap_4.c"); // Set the heap_?.c allocator to use from
                            // 'FreeRTOS-Kernel/portable/MemMang' (Default: heap_4.c)

        b.compile().unwrap_or_else(|e| panic!("{}", e.to_string()));

        println!("cargo:rerun-if-changed=src/FreeRTOSConfig.h");

        // This FreeRTOS-Kernel port (Development Branch) uses CMSIS-style handler
        // names (SVC_Handler, PendSV_Handler, SysTick_Handler).  cortex-m-rt's
        // linker script uses PROVIDE(SVCall = DefaultHandler) etc.  A strong
        // assignment in a linker script fragment overrides PROVIDE(), wiring the
        // cortex-m-rt vector-table slots directly to the FreeRTOS naked-asm
        // handlers.  -u forces portasm.o out of libfreertos.a so the symbols exist.
        for sym in &["SVC_Handler", "PendSV_Handler", "SysTick_Handler"] {
            println!("cargo:rustc-link-arg=-u");
            println!("cargo:rustc-link-arg={sym}");
        }
        let vectors_ld = out.join("freertos-vectors.x");
        std::fs::write(
            &vectors_ld,
            b"SVCall  = SVC_Handler;\nPendSV  = PendSV_Handler;\nSysTick = SysTick_Handler;\n",
        )
        .unwrap();
        println!("cargo:rustc-link-arg=-T{}", vectors_ld.display());
    }

    compile_java(out);
}

fn compile_java(out: &Path) {
    let classes_dir = out.join("classes");
    fs::create_dir_all(&classes_dir).unwrap();

    // Collect all .java files under java/
    let mut java_files: Vec<PathBuf> = Vec::new();
    collect_java_files(Path::new("java"), &mut java_files);

    if java_files.is_empty() {
        return;
    }

    let result = std::process::Command::new("javac")
        .args(["--release", "8", "-cp", "java/framework/java", "-d"])
        .arg(&classes_dir)
        .args(&java_files)
        .status();

    let status = match result {
        Ok(s) => s,
        Err(e) => panic!(
            "Failed to run javac: {e}\n\
             Install a JDK and ensure javac is on PATH.\n\
             On macOS: brew install openjdk && brew link openjdk --force\n\
             Then add to your shell profile:\n\
               export PATH=\"$(brew --prefix openjdk)/bin:$PATH\""
        ),
    };

    if !status.success() {
        panic!(
            "javac compilation failed (exit code: {:?}).\n\
             Make sure a JDK (not just JRE) is installed.\n\
             On macOS: brew install openjdk && brew link openjdk --force",
            status.code()
        );
    }

    println!("cargo:rerun-if-changed=java/");

    // Generate java_classes.rs embedding each .class as a &[u8] constant
    let mut generated = String::new();
    let mut class_files: Vec<PathBuf> = Vec::new();
    collect_class_files(&classes_dir, &mut class_files);

    for class_path in &class_files {
        let rel = class_path.strip_prefix(&classes_dir).unwrap();
        let const_name = rel
            .to_string_lossy()
            .replace(['/', '\\', '.', '-'], "_")
            .to_uppercase();
        // Remove trailing _CLASS suffix from the extension replacement, then re-add
        let const_name = const_name.trim_end_matches("_CLASS").to_string() + "_CLASS";
        generated.push_str(&format!(
            "#[allow(dead_code)]\npub static {const_name}: &[u8] = include_bytes!({path:?});\n",
            const_name = const_name,
            path = class_path.display().to_string(),
        ));
    }

    fs::write(out.join("java_classes.rs"), generated).unwrap();
}

fn collect_java_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_java_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "java") {
                out.push(path);
            }
        }
    }
}

fn collect_class_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_class_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "class") {
                out.push(path);
            }
        }
    }
}
