// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.ListView`.

use crate::shrink_names::m;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::list_view as lvgl_list_view;
use super::super::view::{extract_native_handle, extract_string_at};

pub use lvgl_list_view::reset_list_view_state;
// `visit_item_click_listener_roots` is reached directly via the lvgl path in
// `gc_visit_roots` (mirroring `button::visit_click_listener_roots`), so it is
// not re-exported here.
pub use lvgl_list_view::{drain_item_click_queue, lookup_item_click};

pub fn list_view_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_list_view::create())))
}

pub fn list_view_add_item(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let text = extract_string_at(args, 1, strings)?;
    lvgl_list_view::add_item(id, text);
    Ok(None)
}

pub fn list_view_register_item_click_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    lvgl_list_view::register_item_click_listener(id, obj_ref);
    Ok(None)
}

/// Bind an `Adapter` by *pulling* its rows from native, instead of having Java
/// loop and push one `addItem` per row.
///
/// This is the embedder-side proof of the native→Java upcall. It resolves
/// three different descriptor shapes — `()I`, `(I)Ljava/lang/Object;` and
/// `()Ljava/lang/String;` — against the *runtime* class of app-authored
/// bytecode, and the `toString` case falls through to the String builtin
/// because `java/lang/String` has no class file.
///
/// Takes the handler rather than the graphics backend on purpose:
/// [`NativeMethodHandler::invoke_java`] needs the arm to lend back the very
/// `&mut H` it already holds, so the nested executor reborrows one handler
/// instead of acquiring a second. The graphics sub-dispatchers only ever
/// receive `&mut LvglBackend`, which is why this arm lives beside the other
/// `self`-taking arms in `native_handler/mod.rs` rather than with its
/// siblings in `graphics/`.
///
/// The caller (`ListView.refreshFromAdapter`) has already emptied the list and
/// holds the `ListView` as `this` in its own frame, so the receiver stays
/// GC-rooted for the whole loop even though a collection can fire inside any
/// of these upcalls. `add_item` re-validates the generational handle on every
/// call, so even a stale one degrades to a no-op rather than a dangle.
pub fn list_view_bind_adapter<H: pico_jvm::native::NativeMethodHandler>(
    handler: &mut H,
    ctx: &mut pico_jvm::native::NativeContext<'_>,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(ctx.args, ctx.objects)?;
    let adapter = ctx.args.get(1).copied().unwrap_or(Value::Null);
    if matches!(adapter, Value::Null) {
        return Ok(None);
    }

    let count = match handler.invoke_java(ctx, adapter, m::getCount, "()I", &[])? {
        Some(Value::Int(n)) => n,
        _ => return Err(JvmError::InvalidReference),
    };

    for i in 0..count {
        let item = handler
            .invoke_java(
                ctx,
                adapter,
                m::getItem,
                "(I)Ljava/lang/Object;",
                &[Value::Int(i)],
            )?
            .unwrap_or(Value::Null);
        // Mirrors `item == null ? "" : item.toString()`. A `Reference` is
        // already a String, and `String.toString()` returns `this`, so the
        // upcall is skipped for the common `ArrayAdapter<String>` case.
        let text = match item {
            Value::Null => None,
            Value::Reference(idx) => Some(idx),
            _ => match handler.invoke_java(ctx, item, m::toString, "()Ljava/lang/String;", &[])? {
                Some(Value::Reference(idx)) => Some(idx),
                _ => None,
            },
        };
        // Resolved after every upcall has returned: a `&str` borrowed from
        // `ctx.strings` must not be held across one, and the borrow checker
        // enforces that here.
        match text.and_then(|idx| ctx.strings.resolve(idx)) {
            Some(s) => lvgl_list_view::add_item(id, s),
            None => lvgl_list_view::add_item(id, ""),
        }
    }
    Ok(None)
}
