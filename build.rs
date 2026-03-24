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

    embed_apk(out);
}

/// Embeds a pre-built `.papk` file into the firmware as a `&'static [u8]` constant.
///
/// The APK path is read from the `PICODROID_APK_PATH` environment variable, which
/// must be set before invoking `cargo build`.  Use `scripts/build.sh --app <name>`
/// which handles this automatically, or set it manually:
///
/// ```sh
/// PICODROID_APK_PATH=build/apks/helloworld.papk \
///   cargo build --no-default-features --features chip-rp2040
/// ```
fn embed_apk(out: &Path) {
    println!("cargo:rerun-if-env-changed=PICODROID_APK_PATH");

    let apk_path = match env::var("PICODROID_APK_PATH") {
        Ok(p) => p,
        Err(_) => {
            // PICODROID_APK_PATH is not set.  This is expected during `cargo test`
            // because all APK-dependent code in app.rs is gated by #[cfg(not(test))].
            // Generate a stub so the include! compiles (it will not be evaluated in
            // test builds).
            fs::write(
                out.join("apk_data.rs"),
                b"pub static APK_DATA: &[u8] = &[];\n",
            )
            .unwrap();
            return;
        }
    };

    assert!(
        Path::new(&apk_path).exists(),
        "APK file not found: {apk_path}\n\
         Build it first with: ./scripts/build-apk.sh --app <name>"
    );

    // Generate a small Rust snippet that embeds the APK via include_bytes!.
    // The absolute path ensures include_bytes! can find the file regardless
    // of the working directory during compilation.
    let abs_apk_path = std::fs::canonicalize(&apk_path)
        .unwrap_or_else(|e| panic!("Cannot resolve APK path '{apk_path}': {e}"));

    let generated = format!(
        "pub static APK_DATA: &[u8] = include_bytes!({path:?});\n",
        path = abs_apk_path.display().to_string(),
    );
    fs::write(out.join("apk_data.rs"), generated).unwrap();

    // Re-run if the APK file itself changes (e.g. after ./scripts/build-apk.sh).
    println!("cargo:rerun-if-changed={}", abs_apk_path.display());
}
