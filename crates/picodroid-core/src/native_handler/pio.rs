// SPDX-License-Identifier: GPL-3.0-only
use crate::shrink_names::c;
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
    match (class_name, method_name) {
        (c::picodroid_pio_PeripheralManager, m::getInstance) => {
            Some(crate::pio::peripheral_manager::get_instance(ctx.objects))
        }
        (c::picodroid_pio_PeripheralManager, m::openGpio) => Some(
            crate::pio::peripheral_manager::open_gpio(ctx.args, ctx.strings, ctx.objects),
        ),
        (c::picodroid_pio_PeripheralManager, m::openUartDevice) => Some(
            crate::pio::peripheral_manager::open_uart(ctx.args, ctx.strings, ctx.objects),
        ),
        (c::picodroid_pio_PeripheralManager, m::openI2cDevice) => Some(
            crate::pio::peripheral_manager::open_i2c(ctx.args, ctx.strings, ctx.objects),
        ),
        (c::picodroid_pio_PeripheralManager, m::openAdcPin) => Some(
            crate::pio::peripheral_manager::open_adc(ctx.args, ctx.strings, ctx.objects),
        ),
        (c::picodroid_pio_Adc, m::readValue) => {
            Some(crate::pio::adc::read_value_native(ctx.args, ctx.objects))
        }
        (c::picodroid_pio_Adc, m::close) => Some(Ok(None)),
        (c::picodroid_pio_I2cDevice, m::setSpeed) => {
            Some(crate::pio::i2c::set_speed_native(ctx.args, ctx.objects))
        }
        (c::picodroid_pio_I2cDevice, m::write) => Some(crate::pio::i2c::write_native(
            ctx.args,
            ctx.objects,
            ctx.arrays,
        )),
        (c::picodroid_pio_I2cDevice, m::read) => Some(crate::pio::i2c::read_native(
            ctx.args,
            ctx.objects,
            ctx.arrays,
        )),
        (c::picodroid_pio_I2cDevice, m::close) => Some(Ok(None)),
        (c::picodroid_pio_PeripheralManager, m::openPwm) => Some(
            crate::pio::peripheral_manager::open_pwm(ctx.args, ctx.strings, ctx.objects),
        ),
        (c::picodroid_pio_Pwm, m::setEnabled) => {
            Some(crate::pio::pwm::set_enabled_native(ctx.args, ctx.objects))
        }
        (c::picodroid_pio_Pwm, m::setPwmDutyCycle) => Some(crate::pio::pwm::set_duty_cycle_native(
            ctx.args,
            ctx.objects,
        )),
        (c::picodroid_pio_Pwm, m::setPwmFrequencyHz) => {
            Some(crate::pio::pwm::set_frequency_native(ctx.args, ctx.objects))
        }
        (c::picodroid_pio_Pwm, m::close) => Some(Ok(None)),
        (c::picodroid_pio_PeripheralManager, m::openSpiDevice) => Some(
            crate::pio::peripheral_manager::open_spi(ctx.args, ctx.strings, ctx.objects),
        ),
        (c::picodroid_pio_SpiDevice, m::setFrequency) => {
            Some(crate::pio::spi::set_frequency_native(ctx.args, ctx.objects))
        }
        (c::picodroid_pio_SpiDevice, m::setMode) => {
            Some(crate::pio::spi::set_mode_native(ctx.args, ctx.objects))
        }
        (c::picodroid_pio_SpiDevice, m::transfer) => Some(crate::pio::spi::transfer_native(
            ctx.args,
            ctx.objects,
            ctx.arrays,
        )),
        (c::picodroid_pio_SpiDevice, m::write) => Some(crate::pio::spi::write_native(
            ctx.args,
            ctx.objects,
            ctx.arrays,
        )),
        (c::picodroid_pio_SpiDevice, m::close) => Some(Ok(None)),
        (c::picodroid_pio_UartDevice, m::setBaudrate) => {
            Some(crate::pio::uart::set_baudrate_native(ctx.args, ctx.objects))
        }
        (c::picodroid_pio_UartDevice, m::setDataSize) => Some(
            crate::pio::uart::set_data_size_native(ctx.args, ctx.objects),
        ),
        (c::picodroid_pio_UartDevice, m::setParity) => {
            Some(crate::pio::uart::set_parity_native(ctx.args, ctx.objects))
        }
        (c::picodroid_pio_UartDevice, m::setStopBits) => Some(
            crate::pio::uart::set_stop_bits_native(ctx.args, ctx.objects),
        ),
        (c::picodroid_pio_UartDevice, m::setHardwareFlowControl) => Some(
            crate::pio::uart::set_hw_flow_ctrl_native(ctx.args, ctx.objects),
        ),
        (c::picodroid_pio_UartDevice, m::writeByte) => {
            Some(crate::pio::uart::write_byte_native(ctx.args, ctx.objects))
        }
        (c::picodroid_pio_UartDevice, m::readByte) => {
            Some(crate::pio::uart::read_byte_native(ctx.args, ctx.objects))
        }
        (c::picodroid_pio_UartDevice, m::close) => Some(Ok(None)),
        (c::picodroid_pio_Gpio, m::setDirection) => Some(crate::pio::gpio::set_direction_native(
            ctx.args,
            ctx.objects,
        )),
        (c::picodroid_pio_Gpio, m::setValue) => {
            Some(crate::pio::gpio::set_value_native(ctx.args, ctx.objects))
        }
        (c::picodroid_pio_Gpio, m::getValue) => {
            Some(crate::pio::gpio::get_value_native(ctx.args, ctx.objects))
        }
        (c::picodroid_pio_Gpio, m::close) => Some(Ok(None)),
        _ => None,
    }
}
