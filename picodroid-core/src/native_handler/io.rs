// SPDX-License-Identifier: GPL-3.0-only
//! `java/io/*` native methods — minimal File / FileInputStream / FileOutputStream
//! backed by LittleFS on hardware and by an in-process map in sim.

use crate::shrink_names::m;
use alloc::vec::Vec;
use pico_jvm::{
    array_heap::{ArrayHeap, ATYPE_BYTE},
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
    NativeContext,
};

// ── field slot layouts (must match Java field declaration order) ───────────
mod fields {
    pub mod file {
        pub const PATH: usize = 0;
    }
    pub mod fis {
        pub const PATH: usize = 0;
        pub const POS: usize = 1;
    }
    pub mod fos {
        pub const PATH: usize = 0;
        pub const POS: usize = 1;
    }
}

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let class_name = crate::shrink_names::unshrink_class(class_name);
    match (class_name, method_name) {
        ("picodroid/io/File", m::exists) => Some(file_bool(ctx, backend::exists)),
        ("picodroid/io/File", m::isFile) => Some(file_bool(ctx, backend::is_file)),
        ("picodroid/io/File", m::isDirectory) => Some(file_bool(ctx, backend::is_dir)),
        ("picodroid/io/File", m::length) => Some(file_length(ctx)),
        ("picodroid/io/File", m::delete) => Some(file_bool(ctx, backend::delete)),
        ("picodroid/io/File", m::mkdir) => Some(file_bool(ctx, backend::mkdir)),
        ("picodroid/io/File", m::renameTo) => Some(file_rename_to(ctx)),
        ("picodroid/io/FileInputStream", m::read) => Some(fis_read(ctx)),
        ("picodroid/io/FileInputStream", m::available) => Some(fis_available(ctx)),
        ("picodroid/io/FileOutputStream", m::initStream) => Some(fos_init_stream(ctx)),
        ("picodroid/io/FileOutputStream", m::write) => Some(fos_write(ctx)),
        ("picodroid/io/FileOutputStream", m::flush) => Some(Ok(None)),
        _ => None,
    }
}

// ── File helpers ───────────────────────────────────────────────────────────

fn file_bool(
    ctx: &mut NativeContext<'_>,
    op: impl FnOnce(&str) -> bool,
) -> Result<Option<Value>, JvmError> {
    let path = resolve_path_field(ctx.args, ctx.objects, ctx.strings, fields::file::PATH)?;
    Ok(Some(Value::Int(op(path) as i32)))
}

fn file_length(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let path = resolve_path_field(ctx.args, ctx.objects, ctx.strings, fields::file::PATH)?;
    Ok(Some(Value::Long(backend::length(path))))
}

fn file_rename_to(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let from = resolve_path_field(ctx.args, ctx.objects, ctx.strings, fields::file::PATH)?;
    let dest = as_obj(ctx.args.get(1))?;
    let dest_ref = ctx
        .objects
        .get_field(dest, fields::file::PATH)
        .ok_or(JvmError::InvalidReference)?;
    let dest_idx = as_string_ref(&dest_ref)?;
    let to = ctx
        .strings
        .resolve(dest_idx)
        .ok_or(JvmError::InvalidReference)?;
    Ok(Some(Value::Int(backend::rename(from, to) as i32)))
}

// ── FileInputStream.read(byte[], int, int) ─────────────────────────────────

fn fis_read(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let this = as_obj(ctx.args.first())?;
    let arr_idx = as_array(ctx.args.get(1))?;
    let (off, len) = checked_range(
        ctx,
        arr_idx,
        as_int(ctx.args.get(2))?,
        as_int(ctx.args.get(3))?,
    )?;
    if len == 0 {
        return Ok(Some(Value::Int(0)));
    }

    let path_ref = ctx
        .objects
        .get_field(this, fields::fis::PATH)
        .ok_or(JvmError::InvalidReference)?;
    let path_idx = as_string_ref(&path_ref)?;
    let path = ctx
        .strings
        .resolve(path_idx)
        .ok_or(JvmError::InvalidReference)?;
    let pos = get_long_field(ctx.objects, this, fields::fis::POS);

    let mut buf: Vec<u8> = Vec::new();
    let n = backend::read_at(path, pos as u64, &mut buf, len);
    if n <= 0 {
        // 0 = EOF returns -1 per InputStream contract; -1 from backend = error.
        return Ok(Some(Value::Int(-1)));
    }
    let written = store_bytes_into_array(ctx.arrays, arr_idx, off, &buf[..n as usize])?;
    ctx.objects
        .set_field(this, fields::fis::POS, Value::Long(pos + written as i64))
        .ok_or(JvmError::InvalidReference)?;
    Ok(Some(Value::Int(written as i32)))
}

fn fis_available(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let this = as_obj(ctx.args.first())?;
    let path = resolve_path_field(ctx.args, ctx.objects, ctx.strings, fields::fis::PATH)?;
    let pos = get_long_field(ctx.objects, this, fields::fis::POS);
    let size = backend::length(path);
    let remaining = (size - pos).max(0);
    Ok(Some(Value::Int(remaining.min(i32::MAX as i64) as i32)))
}

// ── FileOutputStream.initStream(String, boolean) — static ──────────────────

fn fos_init_stream(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let path_idx = as_string_ref(ctx.args.first().ok_or(JvmError::InvalidReference)?)?;
    let path = ctx
        .strings
        .resolve(path_idx)
        .ok_or(JvmError::InvalidReference)?;
    let append = as_int(ctx.args.get(1))? != 0;
    if append {
        Ok(Some(Value::Long(backend::length(path))))
    } else {
        backend::truncate(path);
        Ok(Some(Value::Long(0)))
    }
}

// ── FileOutputStream.write(byte[], int, int) ───────────────────────────────

fn fos_write(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let this = as_obj(ctx.args.first())?;
    let arr_idx = as_array(ctx.args.get(1))?;
    let (off, len) = checked_range(
        ctx,
        arr_idx,
        as_int(ctx.args.get(2))?,
        as_int(ctx.args.get(3))?,
    )?;

    let path = resolve_path_field(ctx.args, ctx.objects, ctx.strings, fields::fos::PATH)?;
    let pos = get_long_field(ctx.objects, this, fields::fos::POS);

    let bytes = load_bytes_from_array(ctx.arrays, arr_idx, off, len)?;
    let n = backend::write_at(path, pos as u64, &bytes);
    if n < 0 {
        return Err(JvmError::InvalidReference);
    }
    ctx.objects
        .set_field(this, fields::fos::POS, Value::Long(pos + n as i64))
        .ok_or(JvmError::InvalidReference)?;
    Ok(None)
}

/// Validate a `(byte[], off, len)` triple the way `java.io` does: negative
/// values or a window past the array end throw IndexOutOfBoundsException.
/// Without this a negative `len` went through `as usize` straight into a
/// `vec![0u8; len]` in the backend.
fn checked_range(
    ctx: &mut NativeContext<'_>,
    arr_idx: u16,
    off: i32,
    len: i32,
) -> Result<(usize, usize), JvmError> {
    let arr_len = ctx
        .arrays
        .length(arr_idx)
        .ok_or(JvmError::InvalidReference)? as usize;
    let ok = off >= 0 && len >= 0 && (off as usize).saturating_add(len as usize) <= arr_len;
    if !ok {
        return Err(
            match ctx.objects.alloc("java/lang/IndexOutOfBoundsException") {
                Some(idx) => JvmError::Exception(idx),
                None => JvmError::StackOverflow,
            },
        );
    }
    Ok((off as usize, len as usize))
}

// ── arg / field extraction ─────────────────────────────────────────────────

fn as_obj(v: Option<&Value>) -> Result<u16, JvmError> {
    match v {
        Some(Value::ObjectRef(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn as_array(v: Option<&Value>) -> Result<u16, JvmError> {
    match v {
        Some(Value::ArrayRef(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn as_int(v: Option<&Value>) -> Result<i32, JvmError> {
    match v {
        Some(Value::Int(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn as_string_ref(v: &Value) -> Result<u16, JvmError> {
    match v {
        Value::Reference(i) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn get_long_field(objects: &ObjectHeap, this: u16, slot: usize) -> i64 {
    match objects.get_field(this, slot) {
        Some(Value::Long(v)) => v,
        _ => 0,
    }
}

fn resolve_path_field<'a>(
    args: &[Value],
    objects: &ObjectHeap,
    strings: &'a StringTable,
    slot: usize,
) -> Result<&'a str, JvmError> {
    let this = as_obj(args.first())?;
    let v = objects
        .get_field(this, slot)
        .ok_or(JvmError::InvalidReference)?;
    let idx = as_string_ref(&v)?;
    strings.resolve(idx).ok_or(JvmError::InvalidReference)
}

fn load_bytes_from_array(
    arrays: &ArrayHeap,
    idx: u16,
    off: usize,
    len: usize,
) -> Result<Vec<u8>, JvmError> {
    let n = arrays.length(idx).ok_or(JvmError::InvalidReference)? as usize;
    if off.saturating_add(len) > n {
        return Err(JvmError::InvalidReference);
    }
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let raw = arrays
            .load(idx, off + i)
            .ok_or(JvmError::InvalidReference)?;
        out.push(raw as i8 as u8);
    }
    Ok(out)
}

fn store_bytes_into_array(
    arrays: &mut ArrayHeap,
    idx: u16,
    off: usize,
    bytes: &[u8],
) -> Result<usize, JvmError> {
    let atype = arrays.atype(idx).ok_or(JvmError::InvalidReference)?;
    if atype != ATYPE_BYTE {
        return Err(JvmError::InvalidReference);
    }
    let n = arrays.length(idx).ok_or(JvmError::InvalidReference)? as usize;
    if off.saturating_add(bytes.len()) > n {
        return Err(JvmError::InvalidReference);
    }
    for (i, b) in bytes.iter().enumerate() {
        arrays
            .store(idx, off + i, *b as i8 as i32)
            .ok_or(JvmError::InvalidReference)?;
    }
    Ok(bytes.len())
}

// ── backend: the HalFs seam for sim + hardware, in-memory map for unit tests ─

// The LittleFS body that used to live here is now this family's `HalFs` impl
// in `glue.rs`. Routing through the seam means these natives move to
// picodroid-core without carrying `crate::fs` — and a future family supplies
// storage by implementing one trait rather than editing this file.
#[cfg(not(test))]
use crate::hal::fs as backend;

#[cfg(test)]
mod backend {
    use alloc::collections::BTreeMap;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;
    use std::sync::{Mutex, OnceLock};

    fn store() -> &'static Mutex<BTreeMap<String, Vec<u8>>> {
        static STORE: OnceLock<Mutex<BTreeMap<String, Vec<u8>>>> = OnceLock::new();
        STORE.get_or_init(|| Mutex::new(BTreeMap::new()))
    }

    pub fn exists(path: &str) -> bool {
        store()
            .lock()
            .expect("sim io store mutex poisoned")
            .contains_key(path)
    }
    pub fn is_file(path: &str) -> bool {
        exists(path)
    }
    pub fn is_dir(_path: &str) -> bool {
        false
    }
    pub fn length(path: &str) -> i64 {
        store()
            .lock()
            .unwrap()
            .get(path)
            .map(|v| v.len() as i64)
            .unwrap_or(0)
    }
    pub fn delete(path: &str) -> bool {
        store()
            .lock()
            .expect("sim io store mutex poisoned")
            .remove(path)
            .is_some()
    }
    pub fn mkdir(_path: &str) -> bool {
        true
    }
    pub fn rename(from: &str, to: &str) -> bool {
        let mut s = store().lock().expect("sim io store mutex poisoned");
        if let Some(data) = s.remove(from) {
            s.insert(to.to_string(), data);
            true
        } else {
            false
        }
    }
    pub fn truncate(path: &str) {
        store()
            .lock()
            .expect("sim io store mutex poisoned")
            .insert(path.to_string(), Vec::new());
    }
    pub fn read_at(path: &str, pos: u64, out: &mut Vec<u8>, len: usize) -> i32 {
        let s = store().lock().expect("sim io store mutex poisoned");
        let Some(v) = s.get(path) else {
            return -1;
        };
        let start = pos as usize;
        if start >= v.len() {
            return 0;
        }
        let end = (start + len).min(v.len());
        out.extend_from_slice(&v[start..end]);
        (end - start) as i32
    }
    pub fn write_at(path: &str, pos: u64, data: &[u8]) -> i32 {
        let mut s = store().lock().expect("sim io store mutex poisoned");
        let entry = s.entry(path.to_string()).or_default();
        let start = pos as usize;
        if entry.len() < start + data.len() {
            entry.resize(start + data.len(), 0);
        }
        entry[start..start + data.len()].copy_from_slice(data);
        data.len() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a FileInputStream/FileOutputStream object over `path` with the
    /// in-memory backend holding `content`.
    fn stream_over(
        objects: &mut ObjectHeap,
        strings: &mut StringTable,
        class: &'static str,
        path: &'static str,
        content: &[u8],
    ) -> u16 {
        backend::truncate(path);
        backend::write_at(path, 0, content);
        let this = objects.alloc(class).unwrap();
        let p = strings.intern(path.as_bytes()).unwrap();
        objects.set_field(this, fields::fis::PATH, Value::Reference(p));
        objects.set_field(this, fields::fis::POS, Value::Long(0));
        this
    }

    fn call(
        class: &str,
        method: &str,
        args: &[Value],
        objects: &mut ObjectHeap,
        strings: &mut StringTable,
        arrays: &mut ArrayHeap,
    ) -> Result<Option<Value>, JvmError> {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: "([BII)I",
            args,
            strings,
            objects,
            arrays,
            upcall: None,
        };
        dispatch(class, method, &mut ctx).expect("io method handled")
    }

    #[test]
    fn read_and_write_reject_bad_offsets_with_index_out_of_bounds() {
        // A negative len went through `as usize` into `vec![0u8; len]` — a
        // capacity-overflow panic on the host, an allocation failure on
        // device. Android's InputStream.read throws IndexOutOfBoundsException.
        let mut objects = ObjectHeap::new();
        let mut strings = StringTable::new();
        let mut arrays = ArrayHeap::new();
        let fis = stream_over(
            &mut objects,
            &mut strings,
            "picodroid/io/FileInputStream",
            "/bugbash-f6-in",
            b"hello",
        );
        let fos = stream_over(
            &mut objects,
            &mut strings,
            "picodroid/io/FileOutputStream",
            "/bugbash-f6-out",
            b"",
        );
        let buf = arrays.alloc(ATYPE_BYTE, 4).unwrap();
        for (class, this, m) in [
            ("picodroid/io/FileInputStream", fis, m::read),
            ("picodroid/io/FileOutputStream", fos, m::write),
        ] {
            for (off, len) in [(0, -1), (-1, 2), (3, 2), (0, 5), (i32::MAX, 1)] {
                let r = call(
                    class,
                    m,
                    &[
                        Value::ObjectRef(this),
                        Value::ArrayRef(buf),
                        Value::Int(off),
                        Value::Int(len),
                    ],
                    &mut objects,
                    &mut strings,
                    &mut arrays,
                );
                let Err(JvmError::Exception(idx)) = r else {
                    panic!("{m}(off={off}, len={len}) = {r:?}");
                };
                assert_eq!(
                    objects.class_name(idx),
                    Some("java/lang/IndexOutOfBoundsException"),
                    "{m}(off={off}, len={len})"
                );
            }
        }
        // A well-formed read still works: 4 bytes of "hello" into the buffer.
        let r = call(
            "picodroid/io/FileInputStream",
            m::read,
            &[
                Value::ObjectRef(fis),
                Value::ArrayRef(buf),
                Value::Int(0),
                Value::Int(4),
            ],
            &mut objects,
            &mut strings,
            &mut arrays,
        );
        assert_eq!(r, Ok(Some(Value::Int(4))));
        assert_eq!(arrays.load(buf, 0), Some(b'h' as i32));
        // len == 0 reads nothing and returns 0 (InputStream contract).
        let r = call(
            "picodroid/io/FileInputStream",
            m::read,
            &[
                Value::ObjectRef(fis),
                Value::ArrayRef(buf),
                Value::Int(0),
                Value::Int(0),
            ],
            &mut objects,
            &mut strings,
            &mut arrays,
        );
        assert_eq!(r, Ok(Some(Value::Int(0))));
    }
}
