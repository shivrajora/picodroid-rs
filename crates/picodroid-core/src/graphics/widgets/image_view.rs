// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.ImageView`.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::super::assets;
use super::super::lvgl::widgets::image_view as lvgl_image_view;
use super::super::view::{extract_native_handle, extract_string_at};

pub fn image_view_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_image_view::create())))
}

/// `ImageView.setImageSource(String name)` — resolves `name` against the
/// papk's bundled-asset registry and hands the descriptor to LVGL.
///
/// `name` matches the file name as bundled by `papk-pack` (e.g. `"logo.png"`).
/// A miss is silently ignored — the widget keeps whatever it was showing —
/// because the alternative (throw) would force every Java caller to
/// try/catch around what is conceptually a static resource lookup.
pub fn image_view_set_src(
    args: &[Value],
    strings: &StringTable,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let name = extract_string_at(args, 1, strings)?;
    if let Some(dsc) = assets::lookup(name) {
        lvgl_image_view::set_src(id, dsc);
    }
    Ok(None)
}

/// `ImageView.setImageResource(int resId)` — `R.drawable.*` names an ASSETS
/// entry (`res/drawable/<file>`) through the resource table. Unlike the
/// by-name setter a miss throws, as on Android: an id comes from `R`, so a
/// miss is a package built wrong, not a typo to shrug at.
pub fn image_view_set_resource(
    ctx: &mut pico_jvm::NativeContext<'_>,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(ctx.args, ctx.objects)?;
    let res_id = match ctx.args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    match crate::resources::drawable_name(res_id).and_then(assets::lookup) {
        Some(dsc) => {
            lvgl_image_view::set_src(id, dsc);
            Ok(None)
        }
        None => Err(crate::native_handler::res::not_found(
            ctx, "Drawable", res_id,
        )),
    }
}

/// `ImageView.setScaleType(int scaleType)`
pub fn image_view_set_scale_type(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let scale_type = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_image_view::set_scale_type(id, scale_type);
    Ok(None)
}

/// `ImageView.setTint(int argbColor)`
pub fn image_view_set_tint(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let argb = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_image_view::set_tint(id, argb);
    Ok(None)
}

/// `ImageView.setScale(int zoom)`
pub fn image_view_set_scale(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let zoom = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    lvgl_image_view::set_scale(id, zoom);
    Ok(None)
}
