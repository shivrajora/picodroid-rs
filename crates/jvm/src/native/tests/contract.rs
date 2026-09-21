// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── Single-source-of-truth invariant ──────────────────────────────────────

/// Every class with a per-class entry in `BUILTIN_DISPATCH` must also appear in
/// `BUILTIN_CLASS_NAMES`. Without this, a class would dispatch correctly once
/// but fail virtual dispatch on subclasses because the interpreter could not
/// canonicalise its name to a stable `&'static str`.
#[test]
fn builtin_dispatch_classes_subset_of_names() {
    for &(dispatch_name, _hash, _fn) in BUILTIN_DISPATCH {
        assert!(
            BUILTIN_CLASS_NAMES.iter().any(|&n| n == dispatch_name),
            "class {dispatch_name:?} appears in BUILTIN_DISPATCH but is missing from BUILTIN_CLASS_NAMES"
        );
    }
}

/// `BUILTIN_METHODS` is keyed by exactly the classes `BUILTIN_DISPATCH`
/// serves — a dispatcher without rows would make the generated contract
/// reject every use of that class, and rows for a class nothing dispatches
/// would admit calls that die at run time.
#[test]
fn builtin_methods_cover_every_dispatch_class() {
    for &(name, _hash, _fn) in BUILTIN_DISPATCH {
        assert!(
            BUILTIN_METHODS.iter().any(|(c, _)| *c == name),
            "class {name:?} is in BUILTIN_DISPATCH but has no BUILTIN_METHODS entry"
        );
    }
    for &(name, rows) in BUILTIN_METHODS {
        assert!(
            BUILTIN_DISPATCH.iter().any(|(c, _, _)| *c == name),
            "class {name:?} has BUILTIN_METHODS rows but no BUILTIN_DISPATCH entry"
        );
        assert!(
            BUILTIN_CLASS_NAMES.contains(&name),
            "class {name:?} in BUILTIN_METHODS is missing from BUILTIN_CLASS_NAMES"
        );
        assert!(
            !rows.is_empty(),
            "class {name:?} has an empty BUILTIN_METHODS list"
        );
        for (i, (method, descs)) in rows.iter().enumerate() {
            assert!(
                !rows[..i].iter().any(|(m, _)| m == method),
                "{name}.{method} is listed twice in BUILTIN_METHODS"
            );
            for d in descs.iter() {
                assert!(
                    d.starts_with('(') && d.contains(')') && !d.ends_with(')'),
                    "{name}.{method}: {d:?} is not a JVM method descriptor"
                );
                if *method == "<init>" {
                    assert!(d.ends_with(")V"), "{name}.<init>: {d:?} must return void");
                }
            }
        }
    }
}

/// `BUILTIN_INTERFACE_METHODS` names interfaces the JVM canonicalises and
/// does not dispatch itself (their members resolve on the implementor).
#[test]
fn builtin_interface_methods_name_known_interfaces() {
    for &(iface, rows) in BUILTIN_INTERFACE_METHODS {
        assert!(
            BUILTIN_CLASS_NAMES.contains(&iface),
            "interface {iface:?} in BUILTIN_INTERFACE_METHODS is missing from BUILTIN_CLASS_NAMES"
        );
        assert!(
            !BUILTIN_DISPATCH.iter().any(|(c, _, _)| *c == iface),
            "{iface:?} has a dispatcher; list its methods in BUILTIN_METHODS instead"
        );
        assert!(!rows.is_empty(), "interface {iface:?} has no rows");
        for (method, descs) in rows.iter() {
            assert!(
                !descs.is_empty(),
                "{iface}.{method}: interface members are descriptor-exact"
            );
            for d in descs.iter() {
                assert!(
                    d.starts_with('(') && d.contains(')') && !d.ends_with(')'),
                    "{iface}.{method}: {d:?} is not a JVM method descriptor"
                );
            }
        }
    }
}

/// The dispatcher source a class's rows are matched against.
fn dispatcher_source(class: &str) -> &'static str {
    match class {
        c::java_lang_String => include_str!("../string.rs"),
        c::java_lang_StringBuilder => include_str!("../string_builder.rs"),
        c::java_util_ArrayList => include_str!("../collections.rs"),
        c::java_util_HashMap
        | c::java_util_LinkedHashMap
        | c::java_util_HashMap_KeySet
        | c::java_util_HashMap_Values
        | c::java_util_HashMap_EntrySet
        | c::java_util_Map_Entry => include_str!("../hashmap.rs"),
        c::java_util_HashSet | c::java_util_LinkedHashSet => include_str!("../hashset.rs"),
        c::java_util_Iterator => include_str!("../iterator.rs"),
        c::java_util_Random => include_str!("../random.rs"),
        c::java_lang_Enum => include_str!("../enumeration.rs"),
        c::java_lang_Class => include_str!("../class_obj.rs"),
        c::java_lang_Math => include_str!("../math.rs"),
        c::java_util_Arrays | c::java_lang_System => include_str!("../arrays.rs"),
        c::java_lang_Integer
        | c::java_lang_Boolean
        | c::java_lang_Long
        | c::java_lang_Float
        | c::java_lang_Double
        | c::java_lang_Character
        | c::java_lang_Byte
        | c::java_lang_Short => include_str!("../boxed.rs"),
        // Object and the Throwable family are dispatched from this module.
        _ => include_str!("../mod.rs"),
    }
}

/// Direction B of the builtin method table: every row names an arm that
/// exists. Text-level — the name must appear as a string literal in the
/// dispatcher's source — which is enough to catch a misspelt or stale row
/// without building a receiver per class. The reverse direction (an arm
/// with no row) is not checked here; it surfaces as a contract failure.
#[test]
fn builtin_method_rows_name_real_arms() {
    let interpreter = include_str!("../../interpreter/ops_invoke.rs");
    let mut missing = alloc::vec::Vec::new();
    for &(class, rows) in BUILTIN_METHODS {
        let source = dispatcher_source(class);
        for (method, _) in rows {
            // Arms match through `m::<name>` (never a literal); `<init>` is the
            // one name with no const.
            let original = crate::names::unshrink_member(method);
            let literal = if original.starts_with('<') {
                alloc::format!("\"{original}\"")
            } else {
                alloc::format!("m::{original}")
            };
            let served = match (class, *method) {
                // Resolved by the interpreter before dispatch.
                (c::java_lang_Object, m::getClass)
                | (c::java_util_ArrayList, m::sort)
                | (c::java_lang_Enum, m::valueOf) => interpreter.contains(&literal),
                _ => source.contains(&literal),
            };
            if !served {
                missing.push(alloc::format!("{class}.{method}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "BUILTIN_METHODS rows with no matching string literal in their dispatcher \
         (stale row or typo): {missing:?}"
    );
}

/// Every primitive sort funnels through one `u64`-key sort (see
/// `native::arrays`), so the float key transforms have to reproduce
/// `total_cmp` exactly — including the cases that make a naive bitwise
/// comparison wrong: `-0.0` below `+0.0`, negative values running backwards,
/// and signed NaNs at the two ends.
#[test]
fn arrays_sort_float_matches_total_cmp() {
    use crate::array_heap::ATYPE_FLOAT;
    let edges: [f32; 11] = [
        f32::NAN,
        -f32::NAN,
        0.0,
        -0.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        1.5,
        -1.5,
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        42.0,
    ];
    // Run both the insertion-sort path (< INSERTION_THRESHOLD) and the
    // quicksort path by padding the same edge values out past the threshold.
    for reps in [1usize, 3] {
        let input: alloc::vec::Vec<f32> = (0..reps).flat_map(|_| edges.iter().copied()).collect();
        let len = input.len();
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let idx = arrays.alloc(ATYPE_FLOAT, len as u16).unwrap();
        for (i, v) in input.iter().enumerate() {
            arrays.store(idx, i, v.to_bits() as i32).unwrap();
        }
        arrays_dispatch(
            m::sort,
            "([F)V",
            &[Value::ArrayRef(idx)],
            &mut strings,
            &mut objects,
            &mut arrays,
        )
        .unwrap();
        let got: alloc::vec::Vec<u32> = (0..len)
            .map(|i| arrays.load(idx, i).unwrap() as u32)
            .collect();
        let mut want = input.clone();
        want.sort_by(f32::total_cmp);
        let want: alloc::vec::Vec<u32> = want.iter().map(|v| v.to_bits()).collect();
        // Compare bit patterns, not values: NaN != NaN, and -0.0 == 0.0.
        assert_eq!(got, want, "f32 sort diverged from total_cmp (reps={reps})");
    }
}

#[test]
fn arrays_sort_double_matches_total_cmp() {
    use crate::array_heap::ATYPE_DOUBLE;
    let edges: [f64; 11] = [
        f64::NAN,
        -f64::NAN,
        0.0,
        -0.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        1.5,
        -1.5,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        42.0,
    ];
    for reps in [1usize, 3] {
        let input: alloc::vec::Vec<f64> = (0..reps).flat_map(|_| edges.iter().copied()).collect();
        let len = input.len();
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let idx = arrays.alloc(ATYPE_DOUBLE, len as u16).unwrap();
        for (i, v) in input.iter().enumerate() {
            arrays.store64(idx, i, v.to_bits() as i64).unwrap();
        }
        arrays_dispatch(
            m::sort,
            "([D)V",
            &[Value::ArrayRef(idx)],
            &mut strings,
            &mut objects,
            &mut arrays,
        )
        .unwrap();
        let got: alloc::vec::Vec<u64> = (0..len)
            .map(|i| arrays.load64(idx, i).unwrap() as u64)
            .collect();
        let mut want = input.clone();
        want.sort_by(f64::total_cmp);
        let want: alloc::vec::Vec<u64> = want.iter().map(|v| v.to_bits()).collect();
        assert_eq!(got, want, "f64 sort diverged from total_cmp (reps={reps})");
    }
}

/// The i64 key transform has to keep negatives below positives across the
/// sign boundary, including the extremes where a sign-bit flip is easy to
/// get wrong.
#[test]
fn arrays_sort_long_spans_sign_boundary() {
    use crate::array_heap::ATYPE_LONG;
    let input: [i64; 8] = [i64::MAX, -1, 0, i64::MIN, 1, -2, i64::MIN + 1, i64::MAX - 1];
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = arrays.alloc(ATYPE_LONG, input.len() as u16).unwrap();
    for (i, v) in input.iter().enumerate() {
        arrays.store64(idx, i, *v).unwrap();
    }
    arrays_dispatch(
        m::sort,
        "([J)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    let got: alloc::vec::Vec<i64> = (0..input.len())
        .map(|i| arrays.load64(idx, i).unwrap())
        .collect();
    let mut want = input.to_vec();
    want.sort_unstable();
    assert_eq!(got, want);
}
