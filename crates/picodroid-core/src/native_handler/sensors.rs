// SPDX-License-Identifier: GPL-3.0-only
use crate::shrink_names::{ch, mh};
use pico_jvm::NativeKey;
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

pub fn dispatch(
    key: NativeKey,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match (key.class, key.method) {
        (ch::picodroid_hardware_SensorManager, mh::getDefaultSensor) => Some(
            crate::hardware::sensors::get_default_sensor(ctx.args, ctx.objects, ctx.strings),
        ),
        (ch::picodroid_hardware_SensorManager, mh::registerListener) => Some(
            crate::hardware::sensors::register_listener(ctx.args, ctx.objects, ctx.arrays),
        ),
        (ch::picodroid_hardware_SensorManager, mh::unregisterListener) => {
            Some(crate::hardware::sensors::unregister_listener(ctx.args))
        }
        _ => None,
    }
}
