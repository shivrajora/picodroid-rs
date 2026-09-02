// SPDX-License-Identifier: GPL-3.0-only
use crate::shrink_names::m;
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let class_name = crate::shrink_names::unshrink_class(class_name);
    match (class_name, method_name) {
        ("picodroid/hardware/SensorManager", m::getDefaultSensor) => Some(
            crate::hardware::sensors::get_default_sensor(ctx.args, ctx.objects, ctx.strings),
        ),
        ("picodroid/hardware/SensorManager", m::registerListener) => Some(
            crate::hardware::sensors::register_listener(ctx.args, ctx.objects, ctx.arrays),
        ),
        ("picodroid/hardware/SensorManager", m::unregisterListener) => {
            Some(crate::hardware::sensors::unregister_listener(ctx.args))
        }
        _ => None,
    }
}
