//! APK + framework class embedding; PAPK flash-init section generation.

use crate::config::collect_files;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Compile picodroid framework Java sources with `javac` and embed each
/// `.class` file as a `&'static [u8]` into `framework_classes.rs`.
///
/// Mirrors Android's model where framework classes are part of the platform
/// (boot classpath) rather than packaged inside an APK. When
/// `PICODROID_APK_PATH` is unset (test / `cargo check`) an empty stub is
/// emitted since all framework-dependent code is gated by `#[cfg(not(test))]`.
pub fn embed_framework_classes(out: &Path) {
    if env::var("PICODROID_APK_PATH").is_err() {
        fs::write(
            out.join("framework_classes.rs"),
            b"pub static FRAMEWORK_CLASSES: &[&[u8]] = &[];\n",
        )
        .unwrap();
        return;
    }

    let framework_dir = Path::new("sdk/java");

    let java_files = collect_files(framework_dir, "java");
    for f in &java_files {
        println!("cargo:rerun-if-changed={}", f.display());
    }

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

    let mut class_files = collect_files(&classes_dir, "class");
    class_files.sort();

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

/// Embed a pre-built `.papk` file into the firmware as `APK_DATA`.
/// For ARM targets the APK lives in the PAPK_FLASH region (see
/// [`embed_papk_flash_init`]), so this emits an empty stub there.
pub fn embed_apk(out: &Path, is_arm_embedded: bool) {
    println!("cargo:rerun-if-env-changed=PICODROID_APK_PATH");

    let apk_path = match env::var("PICODROID_APK_PATH") {
        Ok(p) => p,
        Err(_) => {
            fs::write(
                out.join("apk_data.rs"),
                b"pub static APK_DATA: &[u8] = &[];\n",
            )
            .unwrap();
            return;
        }
    };

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

    let abs_apk_path = std::fs::canonicalize(&apk_path)
        .unwrap_or_else(|e| panic!("Cannot resolve APK path '{apk_path}': {e}"));

    let generated = format!(
        "pub static APK_DATA: &[u8] = include_bytes!({path:?});\n",
        path = abs_apk_path.display().to_string(),
    );
    fs::write(out.join("apk_data.rs"), generated).unwrap();

    println!("cargo:rerun-if-changed={}", abs_apk_path.display());
}

/// Build a PAPK flash image (4 KB metadata sector + raw APK bytes) and emit
/// a linker section that places it at `PAPK_FLASH`. Only active for ARM
/// embedded targets with `PICODROID_APK_PATH` set.
pub fn embed_papk_flash_init(out: &Path, is_arm_embedded: bool) {
    let is_sim = env::var("CARGO_FEATURE_SIM").is_ok();
    let apk_path = env::var("PICODROID_APK_PATH").ok();

    if !is_arm_embedded || is_sim || apk_path.is_none() {
        fs::write(out.join("papk_flash_init.rs"), b"").unwrap();
        return;
    }

    let apk_path = apk_path.unwrap();
    let apk_bytes =
        fs::read(&apk_path).unwrap_or_else(|e| panic!("Cannot read APK at '{apk_path}': {e}"));
    let apk_len = apk_bytes.len();

    // Layout matches read_flash_papk() on-device.
    const PAPK_FLASH_MAGIC: u32 = 0x5044_4231; // "PDB1"
    const META_SIZE: usize = 4096;
    let mut image = Vec::with_capacity(META_SIZE + apk_len);
    image.extend_from_slice(&PAPK_FLASH_MAGIC.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes()); // flags
    image.extend_from_slice(&(apk_len as u32).to_le_bytes()); // len
    image.resize(META_SIZE, 0xFF);
    image.extend_from_slice(&apk_bytes);

    let bin_path = out.join("papk_flash_init.bin");
    fs::write(&bin_path, &image)
        .unwrap_or_else(|e| panic!("Cannot write papk_flash_init.bin: {e}"));
    let abs_bin = bin_path.canonicalize().unwrap();
    let total_size = image.len();

    let rs = format!(
        "#[link_section = \".papk_flash_init\"]\n\
         #[used]\n\
         static PAPK_FLASH_INIT: [u8; {total_size}] = *include_bytes!({path:?});\n",
        path = abs_bin.display().to_string()
    );
    fs::write(out.join("papk_flash_init.rs"), rs)
        .unwrap_or_else(|e| panic!("Cannot write papk_flash_init.rs: {e}"));

    // PAPK_FLASH region is defined in memory.x (rp2040) / memory_rp2350.x (rp2350).
    let ld =
        "SECTIONS {\n  .papk_flash_init : {\n    KEEP(*(.papk_flash_init))\n  } > PAPK_FLASH\n}\n";
    let ld_path = out.join("papk_flash_init.x");
    fs::write(&ld_path, ld).unwrap_or_else(|e| panic!("Cannot write papk_flash_init.x: {e}"));
    println!("cargo:rustc-link-arg=-T{}", ld_path.display());

    println!("cargo:rerun-if-changed={apk_path}");
}
