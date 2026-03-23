use crate::{
    array_heap::ArrayHeap,
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

pub struct NativeContext<'a> {
    pub descriptor: &'a str,
    pub args: &'a [Value],
    pub strings: &'a mut StringTable,
    pub objects: &'a mut ObjectHeap,
    pub arrays: &'a mut ArrayHeap,
}

pub trait NativeMethodHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>>;
}

/// Built-in handler for `java/lang/*` methods provided by every JVM.
/// The interpreter tries the user-supplied handler first, then falls back to
/// this one.  Returning `None` means "not handled here".
pub struct BuiltinHandler;

impl NativeMethodHandler for BuiltinHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        match (class_name, method_name) {
            ("java/lang/Object", "<init>")
            | ("java/lang/Throwable", "<init>")
            | ("java/lang/Exception", "<init>")
            | ("java/lang/RuntimeException", "<init>") => Some(Ok(None)),

            ("java/lang/StringBuilder", "<init>") => {
                ctx.objects.sb_clear();
                Some(Ok(None))
            }
            ("java/lang/StringBuilder", "append") => {
                match ctx.args.get(1) {
                    Some(Value::Reference(idx)) => {
                        let s = ctx.strings.resolve(*idx).unwrap_or("");
                        ctx.objects.sb_append_bytes(s.as_bytes());
                    }
                    Some(Value::Int(n)) => {
                        if ctx.descriptor.starts_with("(C)") {
                            // append(char): emit the character, not its decimal value
                            let ch = (*n as u8).max(0x20); // replace non-printable with space
                            ctx.objects.sb_append_bytes(&[ch]);
                        } else {
                            ctx.objects.sb_append_int(*n);
                        }
                    }
                    _ => {}
                }
                Some(Ok(ctx.args.first().copied().map(Some).unwrap_or(None)))
            }
            ("java/lang/StringBuilder", "toString") => {
                let (ptr, len) = ctx.objects.sb_contents();
                // SAFETY: ptr points into ObjectHeap::sb_buf which lives for the
                // duration of the JVM. The dyn_slot in StringTable is overwritten on
                // the next toString() call, which is safe because Log.i (the only
                // consumer) copies the string before that happens.
                let str_ref =
                    unsafe { ctx.strings.intern_dyn(ptr, len) }.ok_or(JvmError::StackOverflow);
                Some(str_ref.map(|r| Some(Value::Reference(r))))
            }

            ("java/lang/String", "length") => {
                if let Some(Value::Reference(idx)) = ctx.args.first() {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    Some(Ok(Some(Value::Int(s.len() as i32))))
                } else {
                    Some(Err(JvmError::InvalidReference))
                }
            }
            ("java/lang/String", "charAt") => {
                if let (Some(Value::Reference(idx)), Some(Value::Int(i))) =
                    (ctx.args.first(), ctx.args.get(1))
                {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    let ch = s.as_bytes().get(*i as usize).copied().unwrap_or(0);
                    Some(Ok(Some(Value::Int(ch as i32))))
                } else {
                    Some(Err(JvmError::InvalidReference))
                }
            }

            _ => None,
        }
    }
}
