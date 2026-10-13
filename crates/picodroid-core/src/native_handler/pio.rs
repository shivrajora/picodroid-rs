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
        (ch::picodroid_pio_PeripheralManager, mh::getInstance) => {
            Some(crate::pio::peripheral_manager::get_instance(ctx.objects))
        }
        (ch::picodroid_pio_PeripheralManager, mh::openGpio) => Some(
            crate::pio::peripheral_manager::open_gpio(ctx.args, ctx.strings, ctx.objects),
        ),
        (ch::picodroid_pio_PeripheralManager, mh::openUartDevice) => Some(
            crate::pio::peripheral_manager::open_uart(ctx.args, ctx.strings, ctx.objects),
        ),
        (ch::picodroid_pio_PeripheralManager, mh::openI2cDevice) => Some(
            crate::pio::peripheral_manager::open_i2c(ctx.args, ctx.strings, ctx.objects),
        ),
        (ch::picodroid_pio_PeripheralManager, mh::openAdcPin) => Some(
            crate::pio::peripheral_manager::open_adc(ctx.args, ctx.strings, ctx.objects),
        ),
        (ch::picodroid_pio_Adc, mh::readValue) => {
            Some(crate::pio::adc::read_value_native(ctx.args, ctx.objects))
        }
        (ch::picodroid_pio_Adc, mh::close) => Some(Ok(None)),
        (ch::picodroid_pio_I2cDevice, mh::setSpeed) => {
            Some(crate::pio::i2c::set_speed_native(ctx.args, ctx.objects))
        }
        (ch::picodroid_pio_I2cDevice, mh::write) => Some(crate::pio::i2c::write_native(
            ctx.args,
            ctx.objects,
            ctx.arrays,
        )),
        (ch::picodroid_pio_I2cDevice, mh::read) => Some(crate::pio::i2c::read_native(
            ctx.args,
            ctx.objects,
            ctx.arrays,
        )),
        (ch::picodroid_pio_I2cDevice, mh::close) => Some(Ok(None)),
        (ch::picodroid_pio_PeripheralManager, mh::openPwm) => Some(
            crate::pio::peripheral_manager::open_pwm(ctx.args, ctx.strings, ctx.objects),
        ),
        (ch::picodroid_pio_Pwm, mh::setEnabled) => {
            Some(crate::pio::pwm::set_enabled_native(ctx.args, ctx.objects))
        }
        (ch::picodroid_pio_Pwm, mh::setPwmDutyCycle) => Some(
            crate::pio::pwm::set_duty_cycle_native(ctx.args, ctx.objects),
        ),
        (ch::picodroid_pio_Pwm, mh::setPwmFrequencyHz) => {
            Some(crate::pio::pwm::set_frequency_native(ctx.args, ctx.objects))
        }
        (ch::picodroid_pio_Pwm, mh::close) => Some(Ok(None)),
        (ch::picodroid_pio_PeripheralManager, mh::openSpiDevice) => Some(
            crate::pio::peripheral_manager::open_spi(ctx.args, ctx.strings, ctx.objects),
        ),
        (ch::picodroid_pio_SpiDevice, mh::setFrequency) => {
            Some(crate::pio::spi::set_frequency_native(ctx.args, ctx.objects))
        }
        (ch::picodroid_pio_SpiDevice, mh::setMode) => {
            Some(crate::pio::spi::set_mode_native(ctx.args, ctx.objects))
        }
        (ch::picodroid_pio_SpiDevice, mh::transfer) => Some(crate::pio::spi::transfer_native(
            ctx.args,
            ctx.objects,
            ctx.arrays,
        )),
        (ch::picodroid_pio_SpiDevice, mh::write) => Some(crate::pio::spi::write_native(
            ctx.args,
            ctx.objects,
            ctx.arrays,
        )),
        (ch::picodroid_pio_SpiDevice, mh::close) => Some(Ok(None)),
        (ch::picodroid_pio_UartDevice, mh::setBaudrate) => {
            Some(crate::pio::uart::set_baudrate_native(ctx.args, ctx.objects))
        }
        (ch::picodroid_pio_UartDevice, mh::setDataSize) => Some(
            crate::pio::uart::set_data_size_native(ctx.args, ctx.objects),
        ),
        (ch::picodroid_pio_UartDevice, mh::setParity) => {
            Some(crate::pio::uart::set_parity_native(ctx.args, ctx.objects))
        }
        (ch::picodroid_pio_UartDevice, mh::setStopBits) => Some(
            crate::pio::uart::set_stop_bits_native(ctx.args, ctx.objects),
        ),
        (ch::picodroid_pio_UartDevice, mh::setHardwareFlowControl) => Some(
            crate::pio::uart::set_hw_flow_ctrl_native(ctx.args, ctx.objects),
        ),
        (ch::picodroid_pio_UartDevice, mh::writeByte) => {
            Some(crate::pio::uart::write_byte_native(ctx.args, ctx.objects))
        }
        (ch::picodroid_pio_UartDevice, mh::readByte) => {
            Some(crate::pio::uart::read_byte_native(ctx.args, ctx.objects))
        }
        (ch::picodroid_pio_UartDevice, mh::close) => Some(Ok(None)),
        (ch::picodroid_pio_Gpio, mh::setDirection) => Some(crate::pio::gpio::set_direction_native(
            ctx.args,
            ctx.objects,
        )),
        (ch::picodroid_pio_Gpio, mh::setValue) => {
            Some(crate::pio::gpio::set_value_native(ctx.args, ctx.objects))
        }
        (ch::picodroid_pio_Gpio, mh::getValue) => {
            Some(crate::pio::gpio::get_value_native(ctx.args, ctx.objects))
        }
        (ch::picodroid_pio_Gpio, mh::close) => Some(Ok(None)),
        _ => None,
    }
}
