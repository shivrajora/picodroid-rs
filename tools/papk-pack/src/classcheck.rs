// SPDX-License-Identifier: GPL-3.0-only
//! Minimal class-file inspection for build-time entry-point validation.
//!
//! Decodes just enough of a `.class` file — the constant pool's Utf8 entries
//! and the methods table — to answer "does this class declare a method named
//! N with descriptor D (and the right flags)?". Everything else is skipped.
//! Returns `None` on any malformed input so the caller can degrade to a
//! softer check rather than reject a class it merely failed to parse.

/// Method access flag `ACC_STATIC` (JVMS §4.6).
pub const ACC_STATIC: u16 = 0x0008;

fn u16_at(data: &[u8], p: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*data.get(p)?, *data.get(p + 1)?]))
}

fn u32_at(data: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *data.get(p)?,
        *data.get(p + 1)?,
        *data.get(p + 2)?,
        *data.get(p + 3)?,
    ]))
}

/// Whether `data` (a class file) declares a method named `name` whose
/// descriptor starts with `desc_prefix` (use `""` to match any descriptor)
/// and whose flags include all of `required_flags`. `None` if the class file
/// can't be parsed.
pub fn class_has_method(
    data: &[u8],
    name: &str,
    desc_prefix: &str,
    required_flags: u16,
) -> Option<bool> {
    if data.len() < 10 || &data[0..4] != b"\xCA\xFE\xBA\xBE" {
        return None;
    }
    let cp_count = u16_at(data, 8)? as usize;
    // Decode Utf8 entries by CP index; other tags are skipped by fixed size.
    let mut utf8: Vec<Option<&[u8]>> = vec![None; cp_count];
    let mut p = 10usize;
    let mut i = 1usize;
    while i < cp_count {
        let tag = *data.get(p)?;
        p += 1;
        match tag {
            1 => {
                // CONSTANT_Utf8: u16 length + bytes.
                let len = u16_at(data, p)? as usize;
                p += 2;
                utf8[i] = Some(data.get(p..p + len)?);
                p += len;
            }
            // Long (5) / Double (6) occupy two CP slots.
            5 | 6 => {
                p += 8;
                i += 1;
            }
            7 | 8 | 16 | 19 | 20 => p += 2, // Class/String/MethodType/Module/Package
            15 => p += 3,                   // MethodHandle
            3 | 4 | 9 | 10 | 11 | 12 | 17 | 18 => p += 4, // Int/Float/*ref/NameAndType/Dynamic
            _ => return None,               // unknown tag — bail
        }
        i += 1;
    }
    // access_flags(2) this_class(2) super_class(2) interfaces_count(2) + interfaces
    let iface_count = u16_at(data, p + 6)? as usize;
    p += 8 + iface_count * 2;
    // Fields: count + each [access(2) name(2) desc(2) attrs(2) + attr bytes].
    p = skip_members(data, p)?;
    // Methods: same layout. Inspect each.
    let method_count = u16_at(data, p)? as usize;
    p += 2;
    for _ in 0..method_count {
        let flags = u16_at(data, p)?;
        let name_idx = u16_at(data, p + 2)? as usize;
        let desc_idx = u16_at(data, p + 4)? as usize;
        let attr_count = u16_at(data, p + 6)? as usize;
        p += 8;
        p = skip_attributes(data, p, attr_count)?;
        let m_name = utf8.get(name_idx).copied().flatten();
        let m_desc = utf8.get(desc_idx).copied().flatten();
        if m_name == Some(name.as_bytes())
            && m_desc.is_some_and(|d| d.starts_with(desc_prefix.as_bytes()))
            && (flags & required_flags) == required_flags
        {
            return Some(true);
        }
    }
    Some(false)
}

/// Skip a fields-or-methods table: count(2) + each member (8 fixed bytes +
/// its attributes). Returns the position just after the table.
fn skip_members(data: &[u8], mut p: usize) -> Option<usize> {
    let count = u16_at(data, p)? as usize;
    p += 2;
    for _ in 0..count {
        let attr_count = u16_at(data, p + 6)? as usize;
        p += 8;
        p = skip_attributes(data, p, attr_count)?;
    }
    Some(p)
}

/// Skip `count` attributes, each `name(2) length(4) + length bytes`.
fn skip_attributes(data: &[u8], mut p: usize, count: usize) -> Option<usize> {
    for _ in 0..count {
        let len = u32_at(data, p + 2)? as usize;
        p += 6 + len;
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal class "C" extends java/lang/Object with one method:
    //   public static void main(java.lang.String[])  (no Code attr — fine,
    //   the scanner doesn't read method bodies).
    // CP (cp_count=7):
    //   #1 Utf8 "main"  #2 Utf8 "([Ljava/lang/String;)V"  #3 Utf8 "C"
    //   #4 Class->#3    #5 Utf8 "java/lang/Object"         #6 Class->#5
    static CLASS_WITH_MAIN: &[u8] = &[
        0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, // magic + version
        0x00, 0x07, // cp_count = 7
        0x01, 0x00, 0x04, b'm', b'a', b'i', b'n', // #1
        0x01, 0x00, 0x16, b'(', b'[', b'L', b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g',
        b'/', b'S', b't', b'r', b'i', b'n', b'g', b';', b')', b'V', // #2
        0x01, 0x00, 0x01, b'C', // #3
        0x07, 0x00, 0x03, // #4 Class -> #3
        0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b',
        b'j', b'e', b'c', b't', // #5
        0x07, 0x00, 0x05, // #6 Class -> #5
        0x00, 0x21, 0x00, 0x04, 0x00, 0x06, // access, this=#4, super=#6
        0x00, 0x00, // interfaces_count = 0
        0x00, 0x00, // fields_count = 0
        0x00, 0x01, // methods_count = 1
        0x00, 0x09, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, // public static main, 0 attrs
        0x00, 0x00, // class attributes_count = 0
    ];

    #[test]
    fn finds_static_main() {
        assert_eq!(
            class_has_method(
                CLASS_WITH_MAIN,
                "main",
                "([Ljava/lang/String;)V",
                ACC_STATIC
            ),
            Some(true)
        );
    }

    #[test]
    fn rejects_wrong_descriptor_and_missing_name() {
        // Right name, wrong descriptor.
        assert_eq!(
            class_has_method(CLASS_WITH_MAIN, "main", "()V", ACC_STATIC),
            Some(false)
        );
        // Missing method entirely.
        assert_eq!(
            class_has_method(CLASS_WITH_MAIN, "onCreate", "", 0),
            Some(false)
        );
    }

    #[test]
    fn requires_the_flag() {
        // main exists but is static; require a non-static (0x0000 ok, but a
        // bogus required flag like ACC_ABSTRACT 0x0400 must fail to match).
        assert_eq!(
            class_has_method(CLASS_WITH_MAIN, "main", "([Ljava/lang/String;)V", 0x0400),
            Some(false)
        );
    }

    #[test]
    fn malformed_returns_none() {
        assert_eq!(class_has_method(b"not a class", "main", "", 0), None);
    }
}
