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
    // Put `memory.x` in our output directory and ensure it's
    // on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());

    let mut b = freertos_cargo_build::Builder::new();

    // Path to FreeRTOS kernel or set ENV "FREERTOS_SRC" instead
    b.freertos("third_party/FreeRTOS-Kernel");
    b.freertos_config("src"); // Location of `FreeRTOSConfig.h`
    b.freertos_port("GCC/ARM_CM0"); // Port dir relativ to 'FreeRTOS-Kernel/portable'
    b.heap("heap_4.c"); // Set the heap_?.c allocator to use from
                        // 'FreeRTOS-Kernel/portable/MemMang' (Default: heap_4.c)

    // b.get_cc().file("More.c");   // Optional additional C-Code to be compiled

    b.compile().unwrap_or_else(|e| panic!("{}", e.to_string()));

    // By default, Cargo will re-run a build script whenever
    // any file in the project changes. By specifying `memory.x`
    // here, we ensure the build script is only re-run when
    // `memory.x` is changed.
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=src/FreeRTOSConfig.h");

    compile_java(out);

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
        .args(["--release", "8", "-cp", "java", "-d"])
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
            "pub static {const_name}: &[u8] = include_bytes!({path:?});\n",
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
            } else if path.extension().map_or(false, |e| e == "java") {
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
            } else if path.extension().map_or(false, |e| e == "class") {
                out.push(path);
            }
        }
    }
}
