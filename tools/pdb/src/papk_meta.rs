//! Tiny PAPK metadata reader for `pdb install`'s pre-flight compat check.
//!
//! Mirrors the PAPK format documented in `jvm/src/apk.rs` but only parses
//! what the install pre-flight needs — the `framework-map-version` manifest
//! entry. No allocations beyond the returned `&str`.

const HEADER_LEN: usize = 24;
const SECTION_HEADER_LEN: usize = 16;

/// Extract the value of the `framework-map-version` manifest key from a
/// PAPK byte buffer. Returns `None` if the magic is wrong, the manifest
/// section is malformed/truncated, or the key is absent.
///
/// `bytes` should be the full PAPK file (small — typical PAPKs are < 10 KB).
pub fn read_framework_map_version(bytes: &[u8]) -> Option<&str> {
    if bytes.len() < HEADER_LEN || &bytes[0..4] != b"PAPK" {
        return None;
    }
    let mani_off = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    if mani_off + SECTION_HEADER_LEN > bytes.len() {
        return None;
    }
    if &bytes[mani_off..mani_off + 4] != b"MANI" {
        return None;
    }
    let mani_len = u32::from_le_bytes([
        bytes[mani_off + 4],
        bytes[mani_off + 5],
        bytes[mani_off + 6],
        bytes[mani_off + 7],
    ]) as usize;
    let mani_data_start = mani_off + SECTION_HEADER_LEN;
    let mani_data_end = mani_data_start.checked_add(mani_len)?;
    if mani_data_end > bytes.len() {
        return None;
    }

    let mut p = mani_data_start;
    while p + 2 <= mani_data_end {
        let klen = u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
        p += 2;
        if p + klen > mani_data_end {
            return None;
        }
        let key = &bytes[p..p + klen];
        p += klen;
        if p + 2 > mani_data_end {
            return None;
        }
        let vlen = u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
        p += 2;
        if p + vlen > mani_data_end {
            return None;
        }
        let val = &bytes[p..p + vlen];
        p += vlen;
        if key == b"framework-map-version" {
            return std::str::from_utf8(val).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::read_framework_map_version;

    fn build_papk(manifest_entries: &[(&str, &str)]) -> Vec<u8> {
        let mut manifest = Vec::new();
        for (k, v) in manifest_entries {
            manifest.extend_from_slice(&(k.len() as u16).to_le_bytes());
            manifest.extend_from_slice(k.as_bytes());
            manifest.extend_from_slice(&(v.len() as u16).to_le_bytes());
            manifest.extend_from_slice(v.as_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"PAPK");
        out.extend_from_slice(&1u16.to_le_bytes()); // major
        out.extend_from_slice(&1u16.to_le_bytes()); // minor
        out.extend_from_slice(&2u32.to_le_bytes()); // sec count
        out.extend_from_slice(&24u32.to_le_bytes()); // manifest_offset
        out.extend_from_slice(&((24 + 16 + manifest.len()) as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved
        out.extend_from_slice(b"MANI");
        out.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // crc
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved
        out.extend_from_slice(&manifest);
        // No CLASSES section — read_framework_map_version doesn't look there.
        out
    }

    #[test]
    fn returns_value_when_present() {
        let buf = build_papk(&[
            ("main-class", "x/Y"),
            ("framework-map-version", "0.1.0"),
            ("package-name", "test"),
        ]);
        assert_eq!(read_framework_map_version(&buf), Some("0.1.0"));
    }

    #[test]
    fn returns_none_when_absent() {
        let buf = build_papk(&[("main-class", "x/Y")]);
        assert_eq!(read_framework_map_version(&buf), None);
    }

    #[test]
    fn returns_none_for_bad_magic() {
        let mut buf = build_papk(&[("framework-map-version", "0.1.0")]);
        buf[0] = b'X';
        assert_eq!(read_framework_map_version(&buf), None);
    }

    #[test]
    fn returns_none_for_truncated_input() {
        let buf = build_papk(&[("framework-map-version", "0.1.0")]);
        assert_eq!(read_framework_map_version(&buf[..30]), None);
    }
}
