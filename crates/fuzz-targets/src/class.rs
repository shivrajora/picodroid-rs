// SPDX-License-Identifier: GPL-3.0-only
//! Class sections and the class linker: the tables the JVM reads through
//! `Link::from_raw` once the deep check has passed.

use class_link::build::{build_section, link_class as link, simple_class, unpooled_class};
use class_link::bytecode::check_code;
use class_link::{name_hash, ClassSection, Link, Linked};
use papk_format::AlignedBuf;

/// Any bytes as a class section (the CLSS section of a PAPK).
pub fn class_section(data: &[u8]) {
    // Sections are read in place and must be 4-aligned; flash is.
    let buf = AlignedBuf::new(data);
    let Ok(s) = ClassSection::parse(&buf) else {
        return;
    };
    if s.validate().is_ok() {
        read_validated(&s);
    } else {
        read_unvalidated(&s);
    }
}

/// What the JVM assumes of a section that passed `validate`, checked: every
/// class is readable, its table re-derived through the unchecked
/// `Link::from_raw` is the table the checked path hands out, every name is
/// where the index says, and every method's bytecode lies inside its class
/// and passes the checker the linker ran.
pub fn read_validated(s: &ClassSection<'_>) {
    for i in 0..s.len() {
        let c = s
            .class(i)
            .unwrap_or_else(|| panic!("class {i} of a validated section is unreadable"));
        same_table_through_from_raw(&c);
        let name = c.name().expect("a validated class has a name");
        assert_eq!(s.find_class(name), Some(i), "the index misplaces class {i}");
        for f in c.link.fields().iter().chain(c.link.static_fields()) {
            assert!(c.field_name(f).is_some() && c.field_descriptor(f).is_some());
        }
        for m in c.link.methods() {
            assert!(c.method_name(m).is_some() && c.method_descriptor(m).is_some());
            if m.has_code() {
                // `method_max_stack` and friends read at −8, −6 and −4.
                assert!(m.code_offset >= 8, "code at {}", m.code_offset);
                let code = c.method_code(m);
                assert_eq!(code.len(), c.method_code_len(m), "bytecode past the class");
                assert_eq!(
                    check_code(code),
                    Ok(()),
                    "linked bytecode the checker refuses"
                );
            }
        }
        read_class(&c);
    }
    for e in s.index() {
        let c = s.class(e.idx as usize).expect("the index names a class");
        assert_eq!(name_hash(c.name().unwrap()), e.hash, "index hash");
    }
    read_literals(s);
}

/// The same reads with nothing assumed: an unvalidated section must refuse
/// or answer, never panic.
pub fn read_unvalidated(s: &ClassSection<'_>) {
    for c in s.classes() {
        same_table_through_from_raw(&c);
        if let Some(name) = c.name() {
            let _ = s.find_class(name);
        }
        for m in c.link.methods() {
            let _ = (
                c.method_code(m),
                c.method_max_stack(m),
                c.method_max_locals(m),
            );
        }
        read_class(&c);
    }
    for e in s.index() {
        let _ = s.class(e.idx as usize);
    }
    read_literals(s);
}

/// `ClassSection::class` is the whole of `Link::from_raw`'s contract: a
/// table it handed out, read back from its first word alone, must be the
/// same words over the same pool. This is the `unsafe` the JVM leans on;
/// run under ASan (and Miri over the regression set) it is checked for
/// real.
fn same_table_through_from_raw(c: &Linked<'_>) {
    // SAFETY: `c` came from `ClassSection::class`, which checked the
    // table's pool words against the section.
    let raw = unsafe { Link::from_raw(c.link.as_ptr()) };
    assert_eq!(raw.words(), c.link.words());
    assert_eq!(raw.pool().as_ptr(), c.link.pool().as_ptr());
    assert_eq!(raw.pool().len(), c.link.pool().len());
}

/// Every safe accessor of one class and its table.
fn read_class(c: &Linked<'_>) {
    let l = &c.link;
    let _ = (c.name(), c.super_name(), l.super_idx(), l.access_flags());
    let _ = (
        l.name_hash(),
        l.super_hash(),
        l.own_slots(),
        l.source_file_idx(),
    );
    for f in l.interfaces() {
        let _ = (c.interface_name(f), f.hash());
    }
    for f in l.fields().iter().chain(l.static_fields()) {
        let _ = (c.field_name(f), c.field_descriptor(f), f.slot(), f.kind());
    }
    for m in l.methods() {
        let _ = (c.method_name(m), c.method_descriptor(m), l.method_index(m));
        let _ = (c.method_name_index(m), c.method_descriptor_index(m));
        let _ = (
            c.method_max_stack(m),
            c.method_max_locals(m),
            c.method_code(m),
        );
    }
    for i in 0..l.cp_count() {
        let _ = (l.cp_tag(i), l.cp_word(i), l.cp_offset(i));
        let _ = (l.string_desc(i), l.methodref_desc(i), c.utf8(i));
        let _ = (
            c.cp_class_name(i),
            c.cp_member_ref(i),
            c.cp_name_and_type(i),
        );
        let _ = c.cp_string_utf8(i);
    }
    for d in l.strings() {
        let _ = c.string_bytes(d);
    }
}

fn read_literals(s: &ClassSection<'_>) {
    let lits = s.literals();
    for i in 0..lits.len() {
        let _ = (lits.row(i), lits.bytes(i));
    }
}

/// Any bytes as a `.class` file, through the linker papk-pack runs. What
/// links must build a section the deep check accepts — the builder and the
/// validator held to one rule — and must unpool back to the bytes it came
/// from, which is what `papk-pack --repack` relies on.
pub fn link_class(data: &[u8]) {
    let Ok(section) = link(data) else {
        return;
    };
    let buf = AlignedBuf::new(&section);
    let s = ClassSection::parse(&buf).expect("the linker's own section parses");
    if let Err(e) = s.validate() {
        panic!("the linker built a section the deep check refuses: {e:?}");
    }
    read_validated(&s);
    let c = s.class(0).expect("one class");
    assert_eq!(
        unpooled_class(&c).as_deref(),
        Ok(data),
        "unpooling does not give back the class that was linked"
    );
}

/// The pack-time bytecode checker, on its own and inside a class: any code
/// it accepts must link, and any it refuses must not. Input: the method's
/// bytecode.
pub fn bytecode_check(data: &[u8]) {
    // A class is at most 64 KB; longer code fails to link for that reason
    // alone, which is not the disagreement this target looks for.
    let data = &data[..data.len().min(4096)];
    let alone = check_code(data);
    let class = simple_class(b"F", b"java/lang/Object", Some(data));
    let linked = build_section(&[&class]);
    match (&alone, &linked) {
        (Ok(()), Err(e)) => panic!("checked code that does not link: {e:?}"),
        (Err(e), Ok(_)) => panic!("linked code the checker refuses: {e:?}"),
        _ => {}
    }
    if let Ok(section) = linked {
        let buf = AlignedBuf::new(&section);
        read_validated(&ClassSection::parse(&buf).expect("parses"));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_trivial_method_links() {
        // iconst_0, ireturn
        super::bytecode_check(&[0x03, 0xAC]);
    }
}
