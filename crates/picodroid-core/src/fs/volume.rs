// SPDX-License-Identifier: GPL-3.0-only
//! Mount-or-format: the one decision the filesystem makes at boot.
//!
//! Pure — a backing store in, a mounted filesystem out — so the recovery
//! rule can be tested on the host with a RAM store while `fs` itself stays
//! `cfg(not(test))` (it is reached through a `#[path]` shim in `lib.rs`).

use littlefs_rust::{Config, Error, Filesystem, Storage};

/// The read cache, the program cache and every open file's cache. LittleFS
/// defaults each to the 4 KB block size; the volume holds preference files
/// of a few hundred bytes, so 512 B (a multiple of the 256 B program size
/// and a divisor of the block) costs one extra flash read on a file over
/// that size and gives the arena back 7 KB at mount plus 3.5 KB per open
/// file (claudeusage gaps roadmap H8).
const CACHE_BYTES: u32 = 512;
/// The block allocator's lookahead bitmap: 64 B tracks 512 blocks, four
/// times the largest volume (512 KB of 4 KB blocks). Was the block size.
const LOOKAHEAD_BYTES: u32 = 64;

fn config_for(block: u32, prog: u32, read: u32, block_count: u32) -> Config {
    let mut cfg = Config::new(block, block_count);
    cfg.read_size = read;
    cfg.prog_size = prog;
    cfg.block_cycles = 500;
    // A geometry with a program size over the cache (none today) keeps
    // LittleFS's own rule: the cache is at least one program unit.
    cfg.cache_size = CACHE_BYTES.max(prog).max(read);
    cfg.lookahead_size = LOOKAHEAD_BYTES;
    cfg
}

/// Mount `storage` with the geometry given, formatting it first when what is
/// there cannot be this firmware's volume.
///
/// Mount first; format only if the mount says the volume is unusable. The
/// order matters: formatting unconditionally would erase a working
/// filesystem on every boot, and the failure would look like "persistence
/// is broken" rather than like a bug here. Two mount failures qualify:
///
/// * `Corrupt` — no superblock (a blank chip, first boot) or a torn one;
/// * `Invalid` — a superblock that is not ours: another board's image left
///   its geometry behind (the Pico 2 W slot booting a touch-kit build, QA
///   2026-09-13 §5), or an incompatible on-disk version. Failing the mount
///   made every open fail until a `probe-rs erase`; the device's only
///   writer is this firmware, so a foreign volume is as good as blank.
///
/// Anything else (`Io`, `NoMemory`) is reported as is: formatting would not
/// help and might destroy a volume a transient fault only hid.
pub fn open_volume<S: Storage>(
    storage: S,
    block: u32,
    prog: u32,
    read: u32,
    block_count: u32,
) -> Result<Filesystem<S>, Error> {
    let config = config_for(block, prog, read, block_count);
    match Filesystem::mount(storage, config) {
        Ok(fs) => Ok(fs),
        Err((e @ (Error::Corrupt | Error::Invalid), mut recovered)) => {
            let why = match e {
                Error::Corrupt => "no usable superblock",
                _ => "foreign superblock (geometry or version)",
            };
            crate::pd_warn!("[fs] mount failed: {}; formatting the volume", why);
            let cfg = config_for(block, prog, read, block_count);
            Filesystem::format(&mut recovered, &cfg)?;
            Filesystem::mount(recovered, cfg).map_err(|(e, _)| e)
        }
        Err((e, _)) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::open_volume;
    use littlefs_rust::{OpenFlags, RamStorage};

    const BLOCK: u32 = 4096;

    fn plant_marker(store: RamStorage, block_count: u32) -> RamStorage {
        let fs = open_volume(store, BLOCK, 256, 16, block_count).expect("mount");
        {
            let file = fs
                .open("/marker", OpenFlags::WRITE | OpenFlags::CREATE)
                .expect("create");
            file.write(b"here").expect("write");
            file.close().expect("close");
        }
        fs.unmount().expect("unmount")
    }

    #[test]
    fn a_blank_store_is_formatted_and_a_second_mount_keeps_files() {
        let store = plant_marker(RamStorage::new(BLOCK, 16), 16);
        let fs = open_volume(store, BLOCK, 256, 16, 16).expect("remount");
        assert!(fs.exists("/marker"), "a clean remount must not format");
    }

    #[test]
    fn a_foreign_block_count_formats_instead_of_failing() {
        // Formatted for a 16-block device, mounted as an 8-block one: the
        // superblock's block_count disagrees with the config, which
        // littlefs reports as Invalid.
        let store = plant_marker(RamStorage::new(BLOCK, 16), 16);
        let fs = open_volume(store, BLOCK, 256, 16, 8).expect("a foreign geometry must format");
        assert!(
            !fs.exists("/marker"),
            "the foreign volume's contents are gone after the format"
        );
        let file = fs
            .open("/after", OpenFlags::WRITE | OpenFlags::CREATE)
            .expect("the formatted volume is writable");
        file.close().expect("close");
    }
}
