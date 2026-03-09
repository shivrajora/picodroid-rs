use crate::framework::{
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

pub fn set_direction_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let pin = extract_pin(args, objects)?;
    let direction = match args.get(1) {
        Some(Value::Int(d)) => *d,
        _ => return Err(JvmError::InvalidReference),
    };
    set_direction(pin, direction);
    Ok(None)
}

pub fn set_value_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let pin = extract_pin(args, objects)?;
    let high = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    set_value(pin, high);
    Ok(None)
}

fn extract_pin(args: &[Value], objects: &ObjectHeap) -> Result<u8, JvmError> {
    match args.get(0) {
        Some(Value::ObjectRef(idx)) => match objects.get_field(*idx, 0) {
            Some(Value::Int(pin)) => Ok(pin as u8),
            _ => Err(JvmError::InvalidReference),
        },
        _ => Err(JvmError::InvalidReference),
    }
}

fn set_direction(pin: u8, direction: i32) {
    use rp_pico::hal::pac;
    // SAFETY: We are the OS — we own all hardware.
    let p = unsafe { pac::Peripherals::steal() };

    // Set GPIO function to SIO (function select 5)
    p.IO_BANK0
        .gpio(pin as usize)
        .gpio_ctrl()
        .write(|w| unsafe { w.funcsel().bits(5) });

    // Configure pad: disable input buffer, not open-drain
    p.PADS_BANK0
        .gpio(pin as usize)
        .write(|w| w.ie().clear_bit().od().clear_bit());

    // Enable output driver for this pin
    p.SIO
        .gpio_oe_set()
        .write(|w| unsafe { w.bits(1u32 << pin) });

    // Set initial output level based on direction constant
    // DIRECTION_OUT_INITIALLY_HIGH = 1, DIRECTION_OUT_INITIALLY_LOW = 2
    if direction == 1 {
        p.SIO
            .gpio_out_set()
            .write(|w| unsafe { w.bits(1u32 << pin) });
    } else {
        p.SIO
            .gpio_out_clr()
            .write(|w| unsafe { w.bits(1u32 << pin) });
    }
}

pub fn set_value(pin: u8, high: bool) {
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    if high {
        p.SIO
            .gpio_out_set()
            .write(|w| unsafe { w.bits(1u32 << pin) });
    } else {
        p.SIO
            .gpio_out_clr()
            .write(|w| unsafe { w.bits(1u32 << pin) });
    }
}
