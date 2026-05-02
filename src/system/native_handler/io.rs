// SPDX-License-Identifier: GPL-3.0-only
//! `java/io/*` native methods — minimal File / FileInputStream / FileOutputStream
//! backed by LittleFS on hardware and by an in-process map in sim.

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
        ("picodroid/io/File", "exists") => Some(file_bool(ctx, backend::exists)),
        ("picodroid/io/File", "isFile") => Some(file_bool(ctx, backend::is_file)),
        ("picodroid/io/File", "isDirectory") => Some(file_bool(ctx, backend::is_dir)),
        ("picodroid/io/File", "length") => Some(file_length(ctx)),
        ("picodroid/io/File", "delete") => Some(file_bool(ctx, backend::delete)),
        ("picodroid/io/File", "mkdir") => Some(file_bool(ctx, backend::mkdir)),
        ("picodroid/io/File", "renameTo") => Some(file_rename_to(ctx)),
        ("picodroid/io/FileInputStream", "read") => Some(fis_read(ctx)),
        ("picodroid/io/FileInputStream", "available") => Some(fis_available(ctx)),
        ("picodroid/io/FileOutputStream", "initStream") => Some(fos_init_stream(ctx)),
        ("picodroid/io/FileOutputStream", "write") => Some(fos_write(ctx)),
        ("picodroid/io/FileOutputStream", "flush") => Some(Ok(None)),
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
    Ok(Some(Value::Long(backend::length(path) as i64)))
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
    let off = as_int(ctx.args.get(2))? as usize;
    let len = as_int(ctx.args.get(3))? as usize;

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
    let off = as_int(ctx.args.get(2))? as usize;
    let len = as_int(ctx.args.get(3))? as usize;

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

// ── backend: real LittleFS for sim + hardware, in-memory map for unit tests ─

#[cfg(not(test))]
mod backend {
    use alloc::vec::Vec;
    use littlefs_rust::{OpenFlags, SeekFrom};

    use crate::fs::with_fs;

    pub fn exists(path: &str) -> bool {
        with_fs(|fs| fs.exists(path)).unwrap_or(false)
    }

    pub fn is_file(path: &str) -> bool {
        with_fs(|fs| {
            matches!(
                fs.stat(path).map(|m| m.file_type),
                Ok(littlefs_rust::FileType::File)
            )
        })
        .unwrap_or(false)
    }

    pub fn is_dir(path: &str) -> bool {
        with_fs(|fs| {
            matches!(
                fs.stat(path).map(|m| m.file_type),
                Ok(littlefs_rust::FileType::Dir)
            )
        })
        .unwrap_or(false)
    }

    pub fn length(path: &str) -> i64 {
        with_fs(|fs| fs.stat(path).map(|m| m.size as i64).unwrap_or(0)).unwrap_or(0)
    }

    pub fn delete(path: &str) -> bool {
        with_fs(|fs| fs.remove(path).is_ok()).unwrap_or(false)
    }

    pub fn mkdir(path: &str) -> bool {
        with_fs(|fs| fs.mkdir(path).is_ok()).unwrap_or(false)
    }

    pub fn rename(from: &str, to: &str) -> bool {
        with_fs(|fs| fs.rename(from, to).is_ok()).unwrap_or(false)
    }

    pub fn truncate(path: &str) {
        let _ = with_fs(|fs| fs.write_file(path, &[]));
    }

    pub fn read_at(path: &str, pos: u64, out: &mut Vec<u8>, len: usize) -> i32 {
        with_fs(|fs| {
            let file = match fs.open(path, OpenFlags::READ) {
                Ok(f) => f,
                Err(_) => return -1i32,
            };
            if file.seek(SeekFrom::Start(pos as u32)).is_err() {
                return -1;
            }
            let mut tmp = alloc::vec![0u8; len];
            match file.read(&mut tmp) {
                Ok(n) => {
                    out.extend_from_slice(&tmp[..n as usize]);
                    n as i32
                }
                Err(_) => -1,
            }
        })
        .unwrap_or(-1)
    }

    pub fn write_at(path: &str, pos: u64, data: &[u8]) -> i32 {
        with_fs(|fs| {
            let file = match fs.open(path, OpenFlags::WRITE | OpenFlags::CREATE) {
                Ok(f) => f,
                Err(_) => return -1i32,
            };
            if file.seek(SeekFrom::Start(pos as u32)).is_err() {
                return -1;
            }
            match file.write(data) {
                Ok(n) => {
                    let _ = file.sync();
                    n as i32
                }
                Err(_) => -1,
            }
        })
        .unwrap_or(-1)
    }
}

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
        store().lock().unwrap().contains_key(path)
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
        store().lock().unwrap().remove(path).is_some()
    }
    pub fn mkdir(_path: &str) -> bool {
        true
    }
    pub fn rename(from: &str, to: &str) -> bool {
        let mut s = store().lock().unwrap();
        if let Some(data) = s.remove(from) {
            s.insert(to.to_string(), data);
            true
        } else {
            false
        }
    }
    pub fn truncate(path: &str) {
        store().lock().unwrap().insert(path.to_string(), Vec::new());
    }
    pub fn read_at(path: &str, pos: u64, out: &mut Vec<u8>, len: usize) -> i32 {
        let s = store().lock().unwrap();
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
        let mut s = store().lock().unwrap();
        let entry = s.entry(path.to_string()).or_default();
        let start = pos as usize;
        if entry.len() < start + data.len() {
            entry.resize(start + data.len(), 0);
        }
        entry[start..start + data.len()].copy_from_slice(data);
        data.len() as i32
    }
}
