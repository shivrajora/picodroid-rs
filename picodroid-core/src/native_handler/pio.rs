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
        ("picodroid/pio/PeripheralManager", m::getInstance) => {
            Some(crate::pio::peripheral_manager::get_instance(ctx.objects))
        }
        ("picodroid/pio/PeripheralManager", m::openGpio) => Some(
            crate::pio::peripheral_manager::open_gpio(ctx.args, ctx.strings, ctx.objects),
        ),
        ("picodroid/pio/PeripheralManager", m::openUartDevice) => Some(
            crate::pio::peripheral_manager::open_uart(ctx.args, ctx.strings, ctx.objects),
        ),
        ("picodroid/pio/PeripheralManager", m::openI2cDevice) => Some(
            crate::pio::peripheral_manager::open_i2c(ctx.args, ctx.strings, ctx.objects),
        ),
        ("picodroid/pio/PeripheralManager", m::openAdcPin) => Some(
            crate::pio::peripheral_manager::open_adc(ctx.args, ctx.strings, ctx.objects),
        ),
        ("picodroid/pio/Adc", m::readValue) => {
            Some(crate::pio::adc::read_value_native(ctx.args, ctx.objects))
        }
        ("picodroid/pio/Adc", m::close) => Some(Ok(None)),
        ("picodroid/pio/I2cDevice", m::setSpeed) => {
            Some(crate::pio::i2c::set_speed_native(ctx.args, ctx.objects))
        }
        ("picodroid/pio/I2cDevice", m::write) => Some(crate::pio::i2c::write_native(
            ctx.args,
            ctx.objects,
            ctx.arrays,
        )),
        ("picodroid/pio/I2cDevice", m::read) => Some(crate::pio::i2c::read_native(
            ctx.args,
            ctx.objects,
            ctx.arrays,
        )),
        ("picodroid/pio/I2cDevice", m::close) => Some(Ok(None)),
        ("picodroid/pio/PeripheralManager", m::openPwm) => Some(
            crate::pio::peripheral_manager::open_pwm(ctx.args, ctx.strings, ctx.objects),
        ),
        ("picodroid/pio/Pwm", m::setEnabled) => {
            Some(crate::pio::pwm::set_enabled_native(ctx.args, ctx.objects))
        }
        ("picodroid/pio/Pwm", m::setPwmDutyCycle) => Some(crate::pio::pwm::set_duty_cycle_native(
            ctx.args,
            ctx.objects,
        )),
        ("picodroid/pio/Pwm", m::setPwmFrequencyHz) => {
            Some(crate::pio::pwm::set_frequency_native(ctx.args, ctx.objects))
        }
        ("picodroid/pio/Pwm", m::close) => Some(Ok(None)),
        ("picodroid/pio/PeripheralManager", m::openSpiDevice) => Some(
            crate::pio::peripheral_manager::open_spi(ctx.args, ctx.strings, ctx.objects),
        ),
        ("picodroid/pio/SpiDevice", m::setFrequency) => {
            Some(crate::pio::spi::set_frequency_native(ctx.args, ctx.objects))
        }
        ("picodroid/pio/SpiDevice", m::setMode) => {
            Some(crate::pio::spi::set_mode_native(ctx.args, ctx.objects))
        }
        ("picodroid/pio/SpiDevice", m::transfer) => Some(crate::pio::spi::transfer_native(
            ctx.args,
            ctx.objects,
            ctx.arrays,
        )),
        ("picodroid/pio/SpiDevice", m::write) => Some(crate::pio::spi::write_native(
            ctx.args,
            ctx.objects,
            ctx.arrays,
        )),
        ("picodroid/pio/SpiDevice", m::close) => Some(Ok(None)),
        ("picodroid/pio/UartDevice", m::setBaudrate) => {
            Some(crate::pio::uart::set_baudrate_native(ctx.args, ctx.objects))
        }
        ("picodroid/pio/UartDevice", m::setDataSize) => Some(
            crate::pio::uart::set_data_size_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/UartDevice", m::setParity) => {
            Some(crate::pio::uart::set_parity_native(ctx.args, ctx.objects))
        }
        ("picodroid/pio/UartDevice", m::setStopBits) => Some(
            crate::pio::uart::set_stop_bits_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/UartDevice", m::setHardwareFlowControl) => Some(
            crate::pio::uart::set_hw_flow_ctrl_native(ctx.args, ctx.objects),
        ),
        ("picodroid/pio/UartDevice", m::writeByte) => {
            Some(crate::pio::uart::write_byte_native(ctx.args, ctx.objects))
        }
        ("picodroid/pio/UartDevice", m::readByte) => {
            Some(crate::pio::uart::read_byte_native(ctx.args, ctx.objects))
        }
        ("picodroid/pio/UartDevice", m::close) => Some(Ok(None)),
        ("picodroid/pio/Gpio", m::setDirection) => Some(crate::pio::gpio::set_direction_native(
            ctx.args,
            ctx.objects,
        )),
        ("picodroid/pio/Gpio", m::setValue) => {
            Some(crate::pio::gpio::set_value_native(ctx.args, ctx.objects))
        }
        ("picodroid/pio/Gpio", m::close) => Some(Ok(None)),
        _ => None,
    }
}
