// SPDX-License-Identifier: GPL-3.0-only
//! `picodroid/io/*` native methods — File / FileInputStream / FileOutputStream
//! over the `HalFs` seam (LittleFS on the device and in the simulator,
//! `TestHal`'s in-memory map under test), behind the storage sandbox: every
//! path an app names is mapped under its own directory first
//! (`crate::storage::sandbox`).
//!
//! Failures on the read side fold into the Java return value the way
//! `java.io.File`'s predicates report them — `false`, `0`, `-1`. Failures on
//! the write side — a refused path, a full volume, a rejected write — throw
//! `IOException`, as `java.io` does, so an app can catch them.

// The LittleFS body that used to live here is this family's `HalFs` impl
// in `glue.rs`. Routing through the seam means these natives carry no
// `crate::fs` — a future family supplies storage by implementing one trait
// rather than editing this file — and the test build reaches the in-memory
// `TestHal` through the same seam.
use crate::hal::fs as backend;
use crate::shrink_names::{c, m};
use crate::storage::sandbox::{self, BUF};
use alloc::vec::Vec;
use pico_jvm::{
    array_heap::{encode_ref, ArrayHeap, ATYPE_BYTE, ATYPE_REF},
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
    match (class_name, method_name) {
        (c::picodroid_io_File, m::exists) => Some(file_bool(ctx, backend::exists)),
        (c::picodroid_io_File, m::isFile) => Some(file_bool(ctx, backend::is_file)),
        (c::picodroid_io_File, m::isDirectory) => Some(file_bool(ctx, backend::is_dir)),
        (c::picodroid_io_File, m::length) => Some(file_length(ctx)),
        (c::picodroid_io_File, m::createNewFile) => Some(file_create_new(ctx)),
        (c::picodroid_io_File, m::delete) => Some(file_bool(ctx, backend::delete)),
        (c::picodroid_io_File, m::mkdir) => Some(file_creating_bool(ctx, backend::mkdir)),
        (c::picodroid_io_File, m::renameTo) => Some(file_rename_to(ctx)),
        (c::picodroid_io_File, m::list) => Some(file_list(ctx)),
        (c::picodroid_io_FileInputStream, m::read) => Some(fis_read(ctx)),
        (c::picodroid_io_FileInputStream, m::available) => Some(fis_available(ctx)),
        (c::picodroid_io_FileOutputStream, m::initStream) => Some(fos_init_stream(ctx)),
        (c::picodroid_io_FileOutputStream, m::write) => Some(fos_write(ctx)),
        (c::picodroid_io_FileOutputStream, m::flush) => Some(Ok(None)),
        _ => None,
    }
}

// ── the sandbox at the seam ────────────────────────────────────────────────

/// The volume path for the app path `path`, built in `buf`; `None` when the
/// sandbox refuses it.
fn mapped<'b>(path: &str, buf: &'b mut [u8; BUF]) -> Option<&'b str> {
    sandbox::resolve(crate::packages::running(), path, buf).ok()
}

/// `IOException` carrying `msg`. Local rather than `super::throw_exception`
/// because the test build reaches this module through a `#[path]` shim at
/// the crate root, where `super` is not `native_handler`.
fn throw_io(ctx: &mut NativeContext<'_>, msg: &str) -> JvmError {
    match ctx.objects.alloc(c::java_io_IOException) {
        Some(idx) => {
            if let Some(midx) = ctx.strings.intern_dyn(msg.as_bytes()) {
                ctx.objects.register_exception_message(idx, midx);
            }
            JvmError::Exception(idx)
        }
        None => JvmError::StackOverflow,
    }
}

// ── File helpers ───────────────────────────────────────────────────────────

fn file_bool(
    ctx: &mut NativeContext<'_>,
    op: impl FnOnce(&str) -> bool,
) -> Result<Option<Value>, JvmError> {
    let path = resolve_path_field(ctx.args, ctx.objects, ctx.strings, fields::file::PATH)?;
    let mut buf = [0u8; BUF];
    let ok = mapped(path, &mut buf).is_some_and(op);
    Ok(Some(Value::Int(ok as i32)))
}

/// [`file_bool`] for an operation that creates something: the package
/// directory is made first.
fn file_creating_bool(
    ctx: &mut NativeContext<'_>,
    op: impl FnOnce(&str) -> bool,
) -> Result<Option<Value>, JvmError> {
    let path = resolve_path_field(ctx.args, ctx.objects, ctx.strings, fields::file::PATH)?;
    let mut buf = [0u8; BUF];
    let ok = match mapped(path, &mut buf) {
        Some(volume) => {
            sandbox::ensure_package_dir();
            op(volume)
        }
        None => false,
    };
    Ok(Some(Value::Int(ok as i32)))
}

fn file_length(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let path = resolve_path_field(ctx.args, ctx.objects, ctx.strings, fields::file::PATH)?;
    let mut buf = [0u8; BUF];
    let len = mapped(path, &mut buf).map_or(0, backend::length);
    Ok(Some(Value::Long(len)))
}

/// `File.createNewFile()`: an empty write creates the file (`HalFs::write_at`
/// opens with CREATE); a path that is already there gives Android's
/// `false`, and one that cannot be created throws.
fn file_create_new(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let path = resolve_path_field(ctx.args, ctx.objects, ctx.strings, fields::file::PATH)?;
    let mut buf = [0u8; BUF];
    let Some(volume) = mapped(path, &mut buf) else {
        let msg = alloc::format!("path refused: {path}");
        return Err(throw_io(ctx, &msg));
    };
    if backend::exists(volume) {
        return Ok(Some(Value::Int(0)));
    }
    sandbox::ensure_package_dir();
    if backend::write_at(volume, 0, &[]) < 0 {
        let msg = alloc::format!("cannot create {path}");
        return Err(throw_io(ctx, &msg));
    }
    Ok(Some(Value::Int(1)))
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
    let mut from_buf = [0u8; BUF];
    let mut to_buf = [0u8; BUF];
    let ok = match (mapped(from, &mut from_buf), mapped(to, &mut to_buf)) {
        (Some(from), Some(to)) => {
            sandbox::ensure_package_dir();
            backend::rename(from, to)
        }
        _ => false,
    };
    Ok(Some(Value::Int(ok as i32)))
}

/// `File.list()`: the directory's entry names as a `String[]`, or `null`
/// when the path is not a directory the app can read.
fn file_list(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let path = resolve_path_field(ctx.args, ctx.objects, ctx.strings, fields::file::PATH)?;
    let mut buf = [0u8; BUF];
    let Some(volume) = mapped(path, &mut buf) else {
        return Ok(Some(Value::Null));
    };
    let mut entries = Vec::new();
    if !backend::list_dir(volume, &mut entries) {
        return Ok(Some(Value::Null));
    }
    let count = entries.len().min(u16::MAX as usize);
    let array = ctx
        .arrays
        .alloc(ATYPE_REF, count as u16)
        .ok_or(JvmError::StackOverflow)?;
    for (i, entry) in entries.iter().take(count).enumerate() {
        let name = ctx
            .strings
            .intern_dyn(entry.name.as_bytes())
            .ok_or(JvmError::StackOverflow)?;
        let slot = encode_ref(Value::Reference(name)).ok_or(JvmError::InvalidReference)?;
        ctx.arrays
            .store(array, i, slot)
            .ok_or(JvmError::InvalidReference)?;
    }
    Ok(Some(Value::ArrayRef(array)))
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

    let path = resolve_path_field(ctx.args, ctx.objects, ctx.strings, fields::fis::PATH)?;
    let pos = get_long_field(ctx.objects, this, fields::fis::POS);
    let mut path_buf = [0u8; BUF];
    let Some(volume) = mapped(path, &mut path_buf) else {
        return Ok(Some(Value::Int(-1)));
    };

    let mut buf: Vec<u8> = Vec::new();
    let n = backend::read_at(volume, pos as u64, &mut buf, len);
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
    let mut buf = [0u8; BUF];
    let size = mapped(path, &mut buf).map_or(0, backend::length);
    let remaining = (size - pos).max(0);
    Ok(Some(Value::Int(remaining.min(i32::MAX as i64) as i32)))
}

// ── FileOutputStream.initStream(String, boolean) — static ──────────────────

/// Truncates (or, appending, measures) the file; a refused path yields 0
/// and the first `write` throws.
fn fos_init_stream(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let path_idx = as_string_ref(ctx.args.first().ok_or(JvmError::InvalidReference)?)?;
    let path = ctx
        .strings
        .resolve(path_idx)
        .ok_or(JvmError::InvalidReference)?;
    let append = as_int(ctx.args.get(1))? != 0;
    let mut buf = [0u8; BUF];
    let Some(volume) = mapped(path, &mut buf) else {
        return Ok(Some(Value::Long(0)));
    };
    if append {
        Ok(Some(Value::Long(backend::length(volume))))
    } else {
        sandbox::ensure_package_dir();
        backend::truncate(volume);
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
    let mut path_buf = [0u8; BUF];
    let Some(volume) = mapped(path, &mut path_buf) else {
        let msg = alloc::format!("path refused: {path}");
        return Err(throw_io(ctx, &msg));
    };

    let bytes = load_bytes_from_array(ctx.arrays, arr_idx, off, len)?;
    sandbox::ensure_package_dir();
    let n = backend::write_at(volume, pos as u64, &bytes);
    if n < 0 {
        let msg = alloc::format!("cannot write {path}");
        return Err(throw_io(ctx, &msg));
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
            match ctx.objects.alloc(c::java_lang_IndexOutOfBoundsException) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use pico_jvm::array_heap::decode_ref;

    /// A File / FileInputStream / FileOutputStream object over `path` (the
    /// three share the field layout: the path first, then the stream's
    /// position).
    fn object_over(
        objects: &mut ObjectHeap,
        strings: &mut StringTable,
        class: &'static str,
        path: &'static str,
    ) -> u16 {
        let this = objects.alloc(class).unwrap();
        let p = strings.intern(path.as_bytes()).unwrap();
        objects.set_field(this, fields::fis::PATH, Value::Reference(p));
        if class != c::picodroid_io_File {
            objects.set_field(this, fields::fis::POS, Value::Long(0));
        }
        this
    }

    /// A stream object over `path` with the in-memory backend holding
    /// `content` at that very path (no sandbox: no package runs).
    fn stream_over(
        objects: &mut ObjectHeap,
        strings: &mut StringTable,
        class: &'static str,
        path: &'static str,
        content: &[u8],
    ) -> u16 {
        backend::truncate(path);
        backend::write_at(path, 0, content);
        object_over(objects, strings, class, path)
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
        // The directory is a process-wide static: hold it so no other test's
        // running package maps these raw paths somewhere else.
        let _g = crate::packages::test_support::lock();
        crate::packages::set_running(None);
        // A negative len went through `as usize` into `vec![0u8; len]` — a
        // capacity-overflow panic on the host, an allocation failure on
        // device. Android's InputStream.read throws IndexOutOfBoundsException.
        let mut objects = ObjectHeap::new();
        let mut strings = StringTable::new();
        let mut arrays = ArrayHeap::new();
        let fis = stream_over(
            &mut objects,
            &mut strings,
            c::picodroid_io_FileInputStream,
            "/bugbash-f6-in",
            b"hello",
        );
        let fos = stream_over(
            &mut objects,
            &mut strings,
            c::picodroid_io_FileOutputStream,
            "/bugbash-f6-out",
            b"",
        );
        let buf = arrays.alloc(ATYPE_BYTE, 4).unwrap();
        for (class, this, m) in [
            (c::picodroid_io_FileInputStream, fis, m::read),
            (c::picodroid_io_FileOutputStream, fos, m::write),
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
                    Some(c::java_lang_IndexOutOfBoundsException),
                    "{m}(off={off}, len={len})"
                );
            }
        }
        // A well-formed read still works: 4 bytes of "hello" into the buffer.
        let r = call(
            c::picodroid_io_FileInputStream,
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
            c::picodroid_io_FileInputStream,
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

    /// With a package running, every app path lands under `/data/<package>`,
    /// a climb is refused (`false` on the read side, `IOException` on the
    /// write side), and `File.list()` reads the mapped directory back.
    #[test]
    fn app_paths_land_under_the_running_package() {
        let _g = crate::packages::test_support::lock();
        crate::packages::set_running(Some("com.sandbox"));
        let mut objects = ObjectHeap::new();
        let mut strings = StringTable::new();
        let mut arrays = ArrayHeap::new();

        // FileOutputStream("/note") → the volume's /data/com.sandbox/note.
        let fos = object_over(
            &mut objects,
            &mut strings,
            c::picodroid_io_FileOutputStream,
            "/note",
        );
        let payload = arrays.alloc(ATYPE_BYTE, 2).unwrap();
        arrays.store(payload, 0, b'h' as i32);
        arrays.store(payload, 1, b'i' as i32);
        let r = call(
            c::picodroid_io_FileOutputStream,
            m::write,
            &[
                Value::ObjectRef(fos),
                Value::ArrayRef(payload),
                Value::Int(0),
                Value::Int(2),
            ],
            &mut objects,
            &mut strings,
            &mut arrays,
        );
        assert_eq!(r, Ok(None));
        assert!(backend::exists("/data/com.sandbox/note"));
        assert!(!backend::exists("/note"));

        // File("/note").exists() and length() see it through the mapping.
        let file = object_over(&mut objects, &mut strings, c::picodroid_io_File, "/note");
        let exists = call(
            c::picodroid_io_File,
            m::exists,
            &[Value::ObjectRef(file)],
            &mut objects,
            &mut strings,
            &mut arrays,
        );
        assert_eq!(exists, Ok(Some(Value::Int(1))));
        let length = call(
            c::picodroid_io_File,
            m::length,
            &[Value::ObjectRef(file)],
            &mut objects,
            &mut strings,
            &mut arrays,
        );
        assert_eq!(length, Ok(Some(Value::Long(2))));

        // File("/").list() names it.
        let root = object_over(&mut objects, &mut strings, c::picodroid_io_File, "/");
        let listed = call(
            c::picodroid_io_File,
            m::list,
            &[Value::ObjectRef(root)],
            &mut objects,
            &mut strings,
            &mut arrays,
        );
        let Ok(Some(Value::ArrayRef(names))) = listed else {
            panic!("list() = {listed:?}");
        };
        assert_eq!(arrays.length(names), Some(1));
        let Value::Reference(name) = decode_ref(arrays.load(names, 0).unwrap()) else {
            panic!("not a string");
        };
        assert_eq!(strings.resolve(name), Some("note"));

        // A climb: false from a predicate, IOException from a write.
        let climb = object_over(&mut objects, &mut strings, c::picodroid_io_File, "/../x");
        let exists = call(
            c::picodroid_io_File,
            m::exists,
            &[Value::ObjectRef(climb)],
            &mut objects,
            &mut strings,
            &mut arrays,
        );
        assert_eq!(exists, Ok(Some(Value::Int(0))));
        let created = call(
            c::picodroid_io_File,
            m::createNewFile,
            &[Value::ObjectRef(climb)],
            &mut objects,
            &mut strings,
            &mut arrays,
        );
        let Err(JvmError::Exception(idx)) = created else {
            panic!("createNewFile past the root = {created:?}");
        };
        assert_eq!(objects.class_name(idx), Some(c::java_io_IOException));

        crate::packages::set_running(None);
    }
}
