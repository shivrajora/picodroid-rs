//! This build script copies the `memory.x` file from the crate root into
//! a directory where the linker can always find it at build time.
//! For many projects this is optional, as the linker always searches the
//! project root directory -- wherever `Cargo.toml` is. However, if you
//! are using a workspace or have a more complicated build setup, this
//! build script becomes required. Additionally, by requesting that
//! Cargo re-run the build script whenever `memory.x` is changed,
//! updating `memory.x` ensures a rebuild of the application with the
//! new memory settings.

use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── Simple key=value TOML parser (no dependencies) ─────────────────────────

/// Parse a simple TOML file (flat key = value pairs, no tables/arrays).
/// Supports string values (quoted or unquoted), integer values, and booleans.
/// Lines starting with '#' are comments.
fn parse_toml(path: &str) -> HashMap<String, String> {
    let content = fs::read_to_string(path).unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));
    let mut map = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim().to_string();
            let val = val.trim();
            // Strip surrounding quotes if present
            let val = if (val.starts_with('"') && val.ends_with('"'))
                || (val.starts_with('\'') && val.ends_with('\''))
            {
                val[1..val.len() - 1].to_string()
            } else {
                val.to_string()
            };
            map.insert(key, val);
        }
    }
    map
}

/// Resolve the active board name from Cargo feature flags.
/// Scans CARGO_FEATURE_BOARD_* env vars set by Cargo.
fn resolve_active_board() -> Option<String> {
    // Known board names — must match directory names under boards/
    const BOARDS: &[&str] = &["testbench_rp2040", "testbench_rp2350", "pico_enviro_mon"];
    for board in BOARDS {
        let feature = format!("CARGO_FEATURE_BOARD_{}", board.to_uppercase());
        if env::var(&feature).is_ok() {
            return Some(board.to_string());
        }
    }
    None
}

/// Resolve the active MCU name from Cargo feature flags.
/// Falls back to the board.toml `mcu` field if no chip feature is active.
fn resolve_active_mcu(board_cfg: &HashMap<String, String>) -> String {
    // Check chip features — these take precedence since they determine the HAL crate.
    if env::var("CARGO_FEATURE_CHIP_RP2040").is_ok() {
        return "rp2040".to_string();
    }
    if env::var("CARGO_FEATURE_CHIP_RP2350").is_ok()
        || env::var("CARGO_FEATURE_CHIP_RP2350_HAL").is_ok()
    {
        return "rp2350".to_string();
    }
    // Fall back to board.toml mcu field
    board_cfg
        .get("mcu")
        .cloned()
        .expect("Cannot determine MCU: no chip feature active and board.toml has no 'mcu' key")
}

/// Find the MCU .toml file by searching mcus/<family>/<name>.toml.
fn find_mcu_toml(mcu_name: &str) -> String {
    let mcus_dir = Path::new("mcus");
    if let Ok(entries) = fs::read_dir(mcus_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let candidate = entry.path().join(format!("{mcu_name}.toml"));
                if candidate.exists() {
                    return candidate.to_string_lossy().into_owned();
                }
            }
        }
    }
    panic!("MCU definition not found: mcus/*/{mcu_name}.toml");
}

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let is_arm_embedded = target_arch == "arm";

    if is_arm_embedded {
        // ── Resolve board → MCU config from .toml files ────────────────────
        let board_name = resolve_active_board()
            .expect("No board feature active — enable board-testbench or similar");
        let board_toml_path = format!("boards/{board_name}/board.toml");
        let board = parse_toml(&board_toml_path);
        println!("cargo:rerun-if-changed={board_toml_path}");

        // MCU is determined primarily from Cargo chip features (which select
        // the HAL crate), falling back to board.toml's `mcu` field.
        let mcu_name = resolve_active_mcu(&board);

        let mcu_toml_path = find_mcu_toml(&mcu_name);
        let mcu = parse_toml(&mcu_toml_path);
        println!("cargo:rerun-if-changed={mcu_toml_path}");

        let mcu_family = mcu
            .get("family")
            .cloned()
            .unwrap_or_else(|| panic!("MCU toml missing 'family': {mcu_toml_path}"));

        // ── Linker script ──────────────────────────────────────────────────
        // Board can override with its own linker script (e.g. for external RAM).
        let memory_src = if let Some(ls) = board.get("linker_script") {
            ls.clone()
        } else {
            format!("mcus/{mcu_family}/{mcu_name}.x")
        };
        let memory_bytes =
            fs::read(&memory_src).unwrap_or_else(|e| panic!("Failed to read {memory_src}: {e}"));
        File::create(out.join("memory.x"))
            .unwrap()
            .write_all(&memory_bytes)
            .unwrap();
        println!("cargo:rustc-link-search={}", out.display());
        println!("cargo:rerun-if-changed={memory_src}");

        // ── FreeRTOS ───────────────────────────────────────────────────────
        let mut b = freertos_cargo_build::Builder::new();
        b.freertos("third_party/FreeRTOS-Kernel");

        let freertos_config_dir = format!("mcus/{mcu_family}");
        b.freertos_config(&freertos_config_dir);

        let freertos_port = mcu
            .get("freertos_port")
            .unwrap_or_else(|| panic!("MCU toml missing 'freertos_port': {mcu_toml_path}"));
        b.freertos_port(freertos_port);
        b.heap("heap_4.c");

        let pico_shim_c = mcu
            .get("pico_shim")
            .unwrap_or_else(|| panic!("MCU toml missing 'pico_shim': {mcu_toml_path}"));
        b.add_build_file(pico_shim_c);

        // Expose stub headers that shadow pico-sdk's pico.h, hardware/*.h, pico/multicore.h
        b.get_cc().include("src/hal/rp/port");
        // Some ports store portmacro.h in an include/ subdir
        if let Some(inc) = mcu.get("freertos_port_include") {
            b.get_cc().include(inc);
        }
        // Preprocessor flags required by the RP-specific FreeRTOS ports
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
        // rename the CMSIS handlers to pico-sdk isr_* names:
        //   vPortSVCHandler / SVC_Handler   → isr_svcall
        //   xPortPendSVHandler / PendSV_Handler → isr_pendsv
        //   xPortSysTickHandler / SysTick_Handler → isr_systick
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

    // Load board config for LVGL overrides (applies to both ARM and sim builds).
    let board_cfg = resolve_active_board().map(|name| {
        let path = format!("boards/{name}/board.toml");
        parse_toml(&path)
    });
    build_lvgl(out, &board_cfg);

    embed_framework_classes(out);
    embed_apk(out, is_arm_embedded);
    embed_papk_flash_init(out, is_arm_embedded);
}

/// Compile LVGL C sources into a static library.
fn build_lvgl(_out: &Path, board_cfg: &Option<HashMap<String, String>>) {
    let lvgl_src = Path::new("vendor/lvgl/src");
    if !lvgl_src.exists() {
        // LVGL submodule not checked out — skip (e.g. for CI builds without submodules)
        return;
    }

    let c_files = collect_files(lvgl_src, "c");
    if c_files.is_empty() {
        return;
    }

    // Filter out stdlib backends we don't use (clib, micropython, rtthread)
    // and GPU backends we disabled, and driver files (we use our own HAL)
    let c_files: Vec<_> = c_files
        .into_iter()
        .filter(|p| {
            let s = p.to_string_lossy();
            !s.contains("stdlib/clib")
                && !s.contains("stdlib/micropython")
                && !s.contains("stdlib/rtthread")
                && !s.contains("draw/vg_lite")
                && !s.contains("draw/nxp")
                && !s.contains("draw/sdl")
                && !s.contains("draw/renesas")
                && !s.contains("draw/opengles")
                && !s.contains("libs/thorvg")
                && !s.contains("others/vg_lite_tvg")
                && !s.contains("/drivers/")
        })
        .collect();

    let mut build = cc::Build::new();
    build
        // Include paths: project root (for lv_conf.h) and LVGL root (for lvgl.h)
        .include(".")
        .include("vendor/lvgl")
        .include("vendor/lvgl/src")
        // Required defines
        .define("LV_CONF_INCLUDE_SIMPLE", None)
        .define("LV_LVGL_H_INCLUDE_SIMPLE", None)
        // Suppress warnings from third-party code
        .warnings(false)
        .extra_warnings(false);

    // Board-specific LVGL overrides (take precedence over lv_conf.h via #ifndef guards).
    // Values are read from boards/<name>/board.toml; omitted keys use lv_conf.h defaults.
    if let Some(cfg) = board_cfg {
        if let Some(dpi) = cfg.get("lv_dpi") {
            build.define("LV_DPI_DEF", dpi.as_str());
        }
        if let Some(mem_kb) = cfg.get("lv_mem_kb") {
            let mem_val = format!("({mem_kb} * 1024U)");
            build.define("LV_MEM_SIZE", mem_val.as_str());
        }
    }

    // ARM gcc defaults to -fshort-enums, making C enums 1 byte when values
    // fit.  Our Rust FFI (lvgl_ffi.rs) mirrors this with u8 typedefs.  On
    // x86_64 (sim builds) enums are 4 bytes by default, which breaks struct
    // layout (e.g. lv_indev_data_t.state lands at the wrong offset).  Force
    // -fshort-enums on non-ARM targets so the C and Rust layouts match.
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if target_arch != "arm" {
        build.flag("-fshort-enums");
    }

    for f in &c_files {
        build.file(f);
    }

    build.compile("lvgl");

    // Rerun if any LVGL source or our config changes
    println!("cargo:rerun-if-changed=lv_conf.h");
    println!("cargo:rerun-if-changed=vendor/lvgl/src");
}

/// Compiles picodroid framework Java sources and embeds each `.class` file directly
/// into the firmware as a `&'static [u8]` array.
///
/// This mirrors Android's model where framework classes are part of the platform
/// (boot classpath / ART boot image) rather than packaged inside an APK.
///
/// Framework sources are compiled from `sdk/java/` using `javac`.
/// When `PICODROID_APK_PATH` is not set (e.g. `cargo test`), an empty stub is
/// emitted since all framework-dependent code is gated by `#[cfg(not(test))]`.
fn embed_framework_classes(out: &Path) {
    // Use PICODROID_APK_PATH as the "real firmware build" signal — same heuristic
    // as embed_apk().  When absent we're in a test/check build; emit empty stub.
    if env::var("PICODROID_APK_PATH").is_err() {
        fs::write(
            out.join("framework_classes.rs"),
            b"pub static FRAMEWORK_CLASSES: &[&[u8]] = &[];\n",
        )
        .unwrap();
        return;
    }

    let framework_dir = Path::new("sdk/java");

    // Emit rerun-if-changed for every framework .java file.
    let java_files = collect_files(framework_dir, "java");
    for f in &java_files {
        println!("cargo:rerun-if-changed={}", f.display());
    }

    // Compile framework sources into $OUT_DIR/framework_classes/.
    let classes_dir = out.join("framework_classes");
    fs::create_dir_all(&classes_dir).unwrap();

    let status = Command::new("javac")
        .arg("--release")
        .arg("8")
        .arg("-d")
        .arg(&classes_dir)
        .args(&java_files)
        .status()
        .expect(
            "javac not found — install a JDK to build picodroid firmware\n\
             (Ubuntu: apt-get install default-jdk-headless  |  macOS: brew install --cask temurin)",
        );
    assert!(
        status.success(),
        "javac failed while compiling picodroid framework classes"
    );

    // Collect compiled .class files in a deterministic order.
    let mut class_files = collect_files(&classes_dir, "class");
    class_files.sort();

    // Generate include_bytes! entries using absolute paths so they resolve
    // correctly when this file is include!()'d from src/app.rs.
    let mut entries = String::new();
    for f in &class_files {
        let abs = f
            .canonicalize()
            .unwrap_or_else(|_| f.clone())
            .display()
            .to_string();
        entries.push_str(&format!("    include_bytes!({abs:?}),\n"));
    }

    let content = format!("pub static FRAMEWORK_CLASSES: &[&[u8]] = &[\n{entries}];\n");
    fs::write(out.join("framework_classes.rs"), content).unwrap();
}

/// Recursively collect all files with the given extension under `dir`.
fn collect_files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut result = Vec::new();
    collect_files_recursive(dir, ext, &mut result);
    result
}

fn collect_files_recursive(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, ext, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(path);
        }
    }
}

/// Builds a ready-to-flash PAPK image (4 KB metadata sector + raw APK bytes) and
/// places it into the firmware ELF as a section at the `PAPK_FLASH` address.
///
/// When `probe-rs` flashes the ELF it writes this section directly into the
/// persistent PAPK flash region, so the baked-in app is always installed there
/// after every probe flash.  This prevents a stale PDB-installed app from
/// overriding the newly baked-in APK on the next boot.
///
/// Only emits the section for ARM embedded targets with `PICODROID_APK_PATH` set.
fn embed_papk_flash_init(out: &Path, is_arm_embedded: bool) {
    let is_sim = env::var("CARGO_FEATURE_SIM").is_ok();
    let apk_path = env::var("PICODROID_APK_PATH").ok();

    if !is_arm_embedded || is_sim || apk_path.is_none() {
        // Test / sim / clippy build: emit an empty stub so include!() compiles.
        fs::write(out.join("papk_flash_init.rs"), b"").unwrap();
        return;
    }

    let apk_path = apk_path.unwrap();
    let apk_bytes =
        fs::read(&apk_path).unwrap_or_else(|e| panic!("Cannot read APK at '{apk_path}': {e}"));
    let apk_len = apk_bytes.len();

    // Build the PAPK flash image: 4 KB metadata sector followed by raw APK bytes.
    // Matches the on-device layout expected by read_flash_papk().
    const PAPK_FLASH_MAGIC: u32 = 0x5044_4231; // "PDB1"
    const META_SIZE: usize = 4096;
    let mut image = Vec::with_capacity(META_SIZE + apk_len);
    image.extend_from_slice(&PAPK_FLASH_MAGIC.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes()); // flags
    image.extend_from_slice(&(apk_len as u32).to_le_bytes()); // len
    image.resize(META_SIZE, 0xFF); // pad metadata sector to 4 KB
    image.extend_from_slice(&apk_bytes);

    let bin_path = out.join("papk_flash_init.bin");
    fs::write(&bin_path, &image)
        .unwrap_or_else(|e| panic!("Cannot write papk_flash_init.bin: {e}"));
    let abs_bin = bin_path.canonicalize().unwrap();
    let total_size = image.len();

    // Generate Rust source: a static array placed in the .papk_flash_init section.
    let rs = format!(
        "#[link_section = \".papk_flash_init\"]\n\
         #[used]\n\
         static PAPK_FLASH_INIT: [u8; {total_size}] = *include_bytes!({path:?});\n",
        path = abs_bin.display().to_string()
    );
    fs::write(out.join("papk_flash_init.rs"), rs)
        .unwrap_or_else(|e| panic!("Cannot write papk_flash_init.rs: {e}"));

    // Emit a linker fragment that places .papk_flash_init at the PAPK_FLASH region.
    // PAPK_FLASH is defined in memory.x (rp2040) / memory_rp2350.x (rp2350).
    let ld =
        "SECTIONS {\n  .papk_flash_init : {\n    KEEP(*(.papk_flash_init))\n  } > PAPK_FLASH\n}\n";
    let ld_path = out.join("papk_flash_init.x");
    fs::write(&ld_path, ld).unwrap_or_else(|e| panic!("Cannot write papk_flash_init.x: {e}"));
    println!("cargo:rustc-link-arg=-T{}", ld_path.display());

    println!("cargo:rerun-if-changed={apk_path}");
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
fn embed_apk(out: &Path, is_arm_embedded: bool) {
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

    // For embedded targets the APK lives in the PAPK_FLASH region (written by
    // embed_papk_flash_init), so there is no need to duplicate it inside the
    // firmware binary.  APK_DATA is only populated for sim builds.
    if is_arm_embedded {
        fs::write(
            out.join("apk_data.rs"),
            b"pub static APK_DATA: &[u8] = &[];\n",
        )
        .unwrap();
        return;
    }

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
