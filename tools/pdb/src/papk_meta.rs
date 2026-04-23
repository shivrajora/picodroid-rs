//! Tiny PAPK metadata reader for `pdb install`'s pre-flight compat check.
//!
//! Mirrors the PAPK format documented in `jvm/src/apk.rs` but only parses
//! what the install pre-flight needs — the `framework-map-version` manifest
//! entry. No allocations beyond the returned `&str`.

const HEADER_LEN: usize = 24;
const SECTION_HEADER_LEN: usize = 16;

/// Reasons a buffer fails PAPK structural validation. Distinct from "manifest
/// key absent" (see `read_framework_map_version`'s `None`) — these all mean
/// the file isn't a PAPK and the install should refuse regardless of compat
/// mode.
#[derive(Debug, PartialEq, Eq)]
pub enum StructuralError {
    /// File is shorter than the fixed PAPK header.
    TooShort,
    /// First 4 bytes aren't `PAPK`.
    BadMagic,
    /// Declared manifest offset + section header doesn't fit in the buffer.
    ManifestOutOfBounds,
    /// Manifest section header isn't `MANI`.
    ManifestBadMagic,
    /// A manifest TLV entry's length fields walk past the section end.
    ManifestMalformed,
}

impl std::fmt::Display for StructuralError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::TooShort => "file is smaller than a PAPK header",
            Self::BadMagic => "magic bytes are not 'PAPK'",
            Self::ManifestOutOfBounds => "manifest offset points past end of file",
            Self::ManifestBadMagic => "manifest section header is not 'MANI'",
            Self::ManifestMalformed => "manifest TLV entries overflow the section",
        };
        f.write_str(s)
    }
}

/// Validate that `bytes` is structurally a PAPK: header present, magic
/// matches, and the manifest section parses to completion. A Ok result does
/// NOT imply any particular manifest key is present — callers still need
/// `read_framework_map_version` for that.
///
/// Errors here are unconditional refusals at the host level: they exist
/// independently of compat mode, and in particular are not gated on
/// `--skip-host-check` (that flag only bypasses *compat* checks, not
/// structural validity — streaming a truncated PAPK to flash bricks the
/// device regardless of version arithmetic).
pub fn validate_structure(bytes: &[u8]) -> Result<(), StructuralError> {
    if bytes.len() < HEADER_LEN {
        return Err(StructuralError::TooShort);
    }
    if &bytes[0..4] != b"PAPK" {
        return Err(StructuralError::BadMagic);
    }
    let mani_off = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    if mani_off
        .checked_add(SECTION_HEADER_LEN)
        .is_none_or(|e| e > bytes.len())
    {
        return Err(StructuralError::ManifestOutOfBounds);
    }
    if &bytes[mani_off..mani_off + 4] != b"MANI" {
        return Err(StructuralError::ManifestBadMagic);
    }
    let mani_len = u32::from_le_bytes([
        bytes[mani_off + 4],
        bytes[mani_off + 5],
        bytes[mani_off + 6],
        bytes[mani_off + 7],
    ]) as usize;
    let mani_data_start = mani_off + SECTION_HEADER_LEN;
    let mani_data_end = mani_data_start
        .checked_add(mani_len)
        .ok_or(StructuralError::ManifestMalformed)?;
    if mani_data_end > bytes.len() {
        return Err(StructuralError::ManifestMalformed);
    }

    // Walk the manifest TLVs to confirm every key/value length stays in range.
    let mut p = mani_data_start;
    while p < mani_data_end {
        if p + 2 > mani_data_end {
            return Err(StructuralError::ManifestMalformed);
        }
        let klen = u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
        p += 2;
        if p + klen > mani_data_end {
            return Err(StructuralError::ManifestMalformed);
        }
        p += klen;
        if p + 2 > mani_data_end {
            return Err(StructuralError::ManifestMalformed);
        }
        let vlen = u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
        p += 2;
        if p + vlen > mani_data_end {
            return Err(StructuralError::ManifestMalformed);
        }
        p += vlen;
    }

    Ok(())
}

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
    use super::{read_framework_map_version, validate_structure, StructuralError};

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

    #[test]
    fn validate_accepts_well_formed_papk() {
        let buf = build_papk(&[("framework-map-version", "0.1.0")]);
        assert_eq!(validate_structure(&buf), Ok(()));
    }

    #[test]
    fn validate_accepts_papk_without_fmv_key() {
        // Legacy no-key case — structural validity is independent of which
        // keys are present. compat::check is responsible for catching "no
        // framework-map-version key", not validate_structure.
        let buf = build_papk(&[("main-class", "x/Y")]);
        assert_eq!(validate_structure(&buf), Ok(()));
    }

    #[test]
    fn validate_rejects_short_buffer() {
        // The regression: a 100-byte stub must be refused.  Previously
        // read_framework_map_version returned None and compat::check treated
        // "None vs 0.0.0 firmware" as symmetric-versionless → accept.
        let buf = vec![b'P', b'A', b'P', b'K'];
        assert_eq!(validate_structure(&buf), Err(StructuralError::TooShort));
        let stub = vec![0u8; 100];
        assert!(validate_structure(&stub).is_err());
    }

    #[test]
    fn validate_rejects_bad_magic() {
        let mut buf = build_papk(&[("framework-map-version", "0.1.0")]);
        buf[0] = b'X';
        assert_eq!(validate_structure(&buf), Err(StructuralError::BadMagic));
    }

    #[test]
    fn validate_rejects_manifest_offset_out_of_bounds() {
        let mut buf = build_papk(&[("framework-map-version", "0.1.0")]);
        // Corrupt manifest offset (bytes 12..16) to point past end of file.
        let bad_off = (buf.len() as u32 + 4096).to_le_bytes();
        buf[12..16].copy_from_slice(&bad_off);
        assert_eq!(
            validate_structure(&buf),
            Err(StructuralError::ManifestOutOfBounds)
        );
    }

    #[test]
    fn validate_rejects_manifest_bad_section_magic() {
        let mut buf = build_papk(&[("framework-map-version", "0.1.0")]);
        // Manifest section header starts at offset 24 by construction.
        buf[24] = b'X';
        assert_eq!(
            validate_structure(&buf),
            Err(StructuralError::ManifestBadMagic)
        );
    }

    #[test]
    fn validate_rejects_manifest_overrunning_tlv() {
        let mut buf = build_papk(&[("framework-map-version", "0.1.0")]);
        // TLV key length lives at offset 24+16 = 40 (first TLV's klen u16).
        // Inflate it past the section end.
        buf[40..42].copy_from_slice(&0xffffu16.to_le_bytes());
        assert_eq!(
            validate_structure(&buf),
            Err(StructuralError::ManifestMalformed)
        );
    }
}
