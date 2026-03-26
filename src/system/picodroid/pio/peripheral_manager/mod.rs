use pico_jvm::{
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

mod gpio;
mod i2c;
mod pwm;
mod spi;
mod uart;

pub use gpio::open_gpio;
pub use i2c::open_i2c;
pub use pwm::open_pwm;
pub use spi::open_spi;
pub use uart::open_uart;

pub fn get_instance(objects: &mut ObjectHeap) -> Result<Option<Value>, JvmError> {
    let idx = objects
        .alloc("picodroid/pio/PeripheralManager")
        .ok_or(JvmError::StackOverflow)?;
    Ok(Some(Value::ObjectRef(idx)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_jvm::{object_heap::ObjectHeap, types::Value};

    #[test]
    fn get_instance_returns_object_ref() {
        let mut objects = ObjectHeap::new();
        let result = get_instance(&mut objects);
        assert_eq!(result, Ok(Some(Value::ObjectRef(0))));
    }
}
