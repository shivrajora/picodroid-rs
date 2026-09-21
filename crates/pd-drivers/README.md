# pd-drivers

`no_std` device drivers, generic over `embedded-hal` 1.0 traits and free of
any HAL, RTOS or allocator dependency:

| Module | Part | Bus |
|---|---|---|
| `st7789`, `st7796` | TFT panel controllers (init, window, RGB565 flush, hardware vertical scroll on ST7796) | `SpiBus` + `OutputPin` |
| `xpt2046` | resistive touch controller | `SpiBus` |
| `gt911` | capacitive touch controller | `I2cBus` |
| `bme688` | temperature / humidity / pressure / gas, with Bosch compensation math | `I2cBus` |
| `ltr559` | ambient light + proximity | `I2cBus` |

Three small extension traits cover what `embedded-hal` leaves out:
`I2cBus` (blocking write/read returning a negative value on error),
`SpiFreqSwitch` (a shared display + touch bus changes clock per device) and
`SpiAsyncWrite` (start a DMA write, render the next band, collect it later).

Every driver carries host unit tests against a fake bus.

```sh
cargo test -p pd-drivers
```
