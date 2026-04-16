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
        ("picodroid/hardware/SensorManager", "getDefaultSensor") => Some(
            crate::system::picodroid::hardware::sensors::get_default_sensor(
                ctx.args,
                ctx.objects,
                ctx.strings,
            ),
        ),
        ("picodroid/hardware/SensorManager", "registerListener") => Some(
            crate::system::picodroid::hardware::sensors::register_listener(ctx.args, ctx.objects),
        ),
        ("picodroid/hardware/SensorManager", "unregisterListener") => {
            Some(crate::system::picodroid::hardware::sensors::unregister_listener(ctx.args))
        }
        _ => None,
    }
}
