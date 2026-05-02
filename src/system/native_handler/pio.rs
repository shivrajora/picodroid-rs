// SPDX-License-Identifier: GPL-3.0-only
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
        ("picodroid/pio/PeripheralManager", "getInstance") => {
            Some(crate::system::picodroid::pio::peripheral_manager::get_instance(ctx.objects))
        }
        ("picodroid/pio/PeripheralManager", "openGpio") => Some(
            crate::system::picodroid::pio::peripheral_manager::open_gpio(
                ctx.args,
                ctx.strings,
                ctx.objects,
            ),
        ),
        ("picodroid/pio/PeripheralManager", "openUartDevice") => Some(
            crate::system::picodroid::pio::peripheral_manager::open_uart(
                ctx.args,
                ctx.strings,
                ctx.objects,
            ),
        ),
        ("picodroid/pio/PeripheralManager", "openI2cDevice") => {
            Some(crate::system::picodroid::pio::peripheral_manager::open_i2c(
                ctx.args,
                ctx.strings,
                ctx.objects,
            ))
        }
        ("picodroid/pio/PeripheralManager", "openAdcPin") => {
            Some(crate::system::picodroid::pio::peripheral_manager::open_adc(
                ctx.args,
                ctx.strings,
                ctx.objects,
            ))
        }
        ("picodroid/pio/Adc", "readValue") => Some(
            crate::system::picodroid::pio::adc::read_value_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/Adc", "close") => Some(Ok(None)),
        ("picodroid/pio/I2cDevice", "setSpeed") => Some(
            crate::system::picodroid::pio::i2c::set_speed_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/I2cDevice", "write") => Some(
            crate::system::picodroid::pio::i2c::write_native(ctx.args, ctx.objects, ctx.arrays),
        ),
        ("picodroid/pio/I2cDevice", "read") => Some(
            crate::system::picodroid::pio::i2c::read_native(ctx.args, ctx.objects, ctx.arrays),
        ),
        ("picodroid/pio/I2cDevice", "close") => Some(Ok(None)),
        ("picodroid/pio/PeripheralManager", "openPwm") => {
            Some(crate::system::picodroid::pio::peripheral_manager::open_pwm(
                ctx.args,
                ctx.strings,
                ctx.objects,
            ))
        }
        ("picodroid/pio/Pwm", "setEnabled") => Some(
            crate::system::picodroid::pio::pwm::set_enabled_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/Pwm", "setPwmDutyCycle") => Some(
            crate::system::picodroid::pio::pwm::set_duty_cycle_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/Pwm", "setPwmFrequencyHz") => Some(
            crate::system::picodroid::pio::pwm::set_frequency_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/Pwm", "close") => Some(Ok(None)),
        ("picodroid/pio/PeripheralManager", "openSpiDevice") => {
            Some(crate::system::picodroid::pio::peripheral_manager::open_spi(
                ctx.args,
                ctx.strings,
                ctx.objects,
            ))
        }
        ("picodroid/pio/SpiDevice", "setFrequency") => Some(
            crate::system::picodroid::pio::spi::set_frequency_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/SpiDevice", "setMode") => Some(
            crate::system::picodroid::pio::spi::set_mode_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/SpiDevice", "transfer") => Some(
            crate::system::picodroid::pio::spi::transfer_native(ctx.args, ctx.objects, ctx.arrays),
        ),
        ("picodroid/pio/SpiDevice", "write") => Some(
            crate::system::picodroid::pio::spi::write_native(ctx.args, ctx.objects, ctx.arrays),
        ),
        ("picodroid/pio/SpiDevice", "close") => Some(Ok(None)),
        ("picodroid/pio/UartDevice", "setBaudrate") => Some(
            crate::system::picodroid::pio::uart::set_baudrate_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/UartDevice", "setDataSize") => Some(
            crate::system::picodroid::pio::uart::set_data_size_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/UartDevice", "setParity") => Some(
            crate::system::picodroid::pio::uart::set_parity_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/UartDevice", "setStopBits") => Some(
            crate::system::picodroid::pio::uart::set_stop_bits_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/UartDevice", "setHardwareFlowControl") => Some(
            crate::system::picodroid::pio::uart::set_hw_flow_ctrl_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/UartDevice", "writeByte") => Some(
            crate::system::picodroid::pio::uart::write_byte_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/UartDevice", "readByte") => Some(
            crate::system::picodroid::pio::uart::read_byte_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/UartDevice", "close") => Some(Ok(None)),
        ("picodroid/pio/Gpio", "setDirection") => Some(
            crate::system::picodroid::pio::gpio::set_direction_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/Gpio", "setValue") => Some(
            crate::system::picodroid::pio::gpio::set_value_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/Gpio", "close") => Some(Ok(None)),
        _ => None,
    }
}
