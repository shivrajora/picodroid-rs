---
title: "Peripherals (PIO)"
description: "GPIO, UART, I2C, SPI, PWM, and ADC drivers exposed through PeripheralManager."
---

Hardware peripherals: GPIO, UART, I2C, SPI, PWM, ADC. All under `picodroid.pio.*`. See [Java API overview](/api/) for the full API index.

## `picodroid.pio.PeripheralManager`

Singleton for opening hardware peripherals.

```java
import picodroid.pio.PeripheralManager;

PeripheralManager pm = PeripheralManager.getInstance();
Gpio gpio       = pm.openGpio("GP25");
UartDevice uart = pm.openUartDevice("UART0");
I2cDevice  i2c  = pm.openI2cDevice("I2C0");
SpiDevice  spi  = pm.openSpiDevice("SPI0");
Pwm pwm         = pm.openPwm("GP25");
Adc adc         = pm.openAdcPin("GP26");
```

| Method | Returns | Names |
|--------|---------|-------|
| `static PeripheralManager getInstance()` | the manager | |
| `Gpio openGpio(String name)` | a GPIO handle | `GP` and the pin number: `GP0`, `GP25` |
| `UartDevice openUartDevice(String name)` | a UART handle | `UART0`, `UART1` |
| `I2cDevice openI2cDevice(String name)` | an I2C handle | `I2C0`, `I2C1` |
| `SpiDevice openSpiDevice(String name)` | an SPI handle | `SPI0`, `SPI1` |
| `Pwm openPwm(String name)` | a PWM handle | `GP0` – `GP29` |
| `Adc openAdcPin(String name)` | an ADC handle | `GP26` – `GP29` |

A name outside these forms is rejected: the call fails instead of returning a handle. Opening a bus, a PWM pin or an ADC pin also initialises the hardware with its defaults, given with each class below.

Every handle implements `AutoCloseable` — see below.

## Resource management (`AutoCloseable`)

All peripheral classes implement `java.lang.AutoCloseable`, so they can be used in try-with-resources blocks, and `close()` is guaranteed to be called even if the body throws.

`close()` ends the handle's use and changes nothing on the hardware: the pin or bus keeps its last configuration, an output keeps its level, and a running PWM keeps running. There is no exclusive ownership either — opening the same name twice gives two handles on the same pin. Put a pin into the state you want to leave it in (`setValue(false)`, `pwm.setEnabled(false)`) before the block ends.

```java
try (Gpio gpio = pm.openGpio("GP25")) {
    gpio.setDirection(Gpio.DIRECTION_OUT_INITIALLY_HIGH);
    // gpio.close() is called automatically here
}

// Multiple resources (closed in reverse order)
try (Adc adc = pm.openAdcPin("GP26");
     Gpio cs  = pm.openGpio("GP17")) {
    double v = adc.readValue();
    cs.setValue(false);
}
```

## `picodroid.pio.Gpio`

```java
import picodroid.pio.Gpio;

gpio.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
gpio.setValue(true);    // drive high
gpio.setValue(false);   // drive low
gpio.close();           // or use try-with-resources

Gpio sense = pm.openGpio("GP16");
sense.setDirection(Gpio.DIRECTION_IN);
boolean high = sense.getValue();   // read the pin
```

| Member | Description |
|--------|-------------|
| `DIRECTION_IN` = 0 | `setDirection` constant: input, no pull (as Android Things). Read it with `getValue()`; the sim reads such a pin `LOW`, hardware reads whatever drives the pad. |
| `DIRECTION_OUT_INITIALLY_HIGH` = 1 | `setDirection` constant: output, start high. |
| `DIRECTION_OUT_INITIALLY_LOW` = 2 | `setDirection` constant: output, start low. |
| `void setDirection(int)` / `void setValue(boolean)` / `boolean getValue()` / `void close()` | Configure direction, drive the pin, read its level, end the handle's use. On an output, `getValue()` is the driven level. |

`getValue()` is a poll; there is no `registerGpioCallback` or edge trigger. For the board's own buttons use [key events](/api/ui/#key-events), which the framework debounces and delivers on the main thread.

## `picodroid.pio.UartDevice`

Default pins: UART0 → TX=GP0, RX=GP1; UART1 → TX=GP4, RX=GP5. A UART opens at 9600 baud, 8 data bits, no parity, 1 stop bit, no flow control.

```java
import picodroid.pio.UartDevice;

uart.setBaudrate(115200);
uart.setDataSize(8);
uart.setParity(UartDevice.PARITY_NONE);
uart.setStopBits(1);
uart.setHardwareFlowControl(UartDevice.HW_FLOW_CONTROL_NONE);  // or HW_FLOW_CONTROL_AUTO_RTSCTS
int b = uart.readByte();    // non-blocking; returns -1 if RX FIFO empty
uart.writeByte(0x41);       // blocking write of single byte
```

| Member | Description |
|--------|-------------|
| `PARITY_NONE` = 0 / `PARITY_EVEN` = 1 / `PARITY_ODD` = 2 | `setParity` modes. |
| `HW_FLOW_CONTROL_NONE` = 0 / `HW_FLOW_CONTROL_AUTO_RTSCTS` = 1 | `setHardwareFlowControl` modes. |
| `setBaudrate(int)`, `setDataSize(int)`, `setParity(int)`, `setStopBits(int)`, `setHardwareFlowControl(int)` | Line configuration. |
| `int writeByte(int b)` | Blocking single-byte write; returns `1`. |
| `int readByte()` | Non-blocking read; `-1` if the RX FIFO is empty. |
| `void close()` | End the handle's use (see [above](#resource-management-autocloseable)). |

## `picodroid.pio.I2cDevice`

Default pins: I2C0 → SDA=GP4, SCL=GP5; I2C1 → SDA=GP2, SCL=GP3. A board whose own wiring claims a bus may have routed it to other pads (the first user of a bus decides its pins), so check the board's `board.toml` before sharing a bus with its touch controller or sensors. Addresses are 7-bit, and every write ends with a STOP condition.

```java
import picodroid.pio.I2cDevice;

i2c.setSpeed(I2cDevice.SPEED_FAST);      // 400 kHz (default: 100 kHz)

// Write 2 bytes to device at address 0x3C
byte[] cmd = new byte[]{ (byte)0x00, (byte)0xAF };
int written = i2c.write(0x3C, cmd, 2);  // returns bytes written, or -1 on NACK

// Read 2 bytes from device at address 0x48
byte[] buf = new byte[2];
int read = i2c.read(0x48, buf, 2);      // returns bytes read, or -1 on NACK

// Zero-byte write: probe for device presence (returns 0 if ACK, -1 if NACK)
byte[] empty = new byte[0];
int ack = i2c.write(0x48, empty, 0);
```

| Member | Description |
|--------|-------------|
| `SPEED_STANDARD` = 100000 / `SPEED_FAST` = 400000 | `setSpeed` presets (Hz). |
| `void setSpeed(int hz)` | Bus clock; 100 kHz until set. |
| `int write(int address, byte[] data, int len)` | Write `len` bytes; returns bytes written, or `-1` on NACK. |
| `int read(int address, byte[] buf, int len)` | Read `len` bytes; returns bytes read, or `-1` on NACK. |
| `void close()` | End the handle's use. |

### I2C bus scan example

Probe every 7-bit address to discover connected devices:

```java
PeripheralManager pm = PeripheralManager.getInstance();
try (I2cDevice i2c = pm.openI2cDevice("I2C0")) {
    byte[] empty = new byte[0];
    for (int addr = 0x08; addr < 0x78; addr++) {
        if (i2c.write(addr, empty, 0) == 0) {
            Log.i("I2C", "Found device at 0x" + String.valueOf(addr));
        }
    }
}
```

## `picodroid.pio.SpiDevice`

Default pins (CS not driven by peripheral — use `Gpio` if needed):
SPI0 → SCK=GP2, MOSI=GP3, MISO=GP0; SPI1 → SCK=GP10, MOSI=GP11, MISO=GP8.

```java
import picodroid.pio.SpiDevice;

spi.setFrequency(4_000_000);           // 4 MHz (default: 1 MHz)
spi.setMode(SpiDevice.MODE_0);         // CPOL=0, CPHA=0 (default)

// Full-duplex: write tx, read back rx
byte[] tx = new byte[]{ (byte)0x9F, 0x00, 0x00 };
byte[] rx = new byte[3];
spi.transfer(tx, rx, 3);

// Write-only (RX discarded)
byte[] cmd = new byte[]{ (byte)0x02, (byte)0x00, (byte)0x00, (byte)0x00, (byte)0xAB };
spi.write(cmd, 5);
```

| Member | Description |
|--------|-------------|
| `MODE_0` = 0 / `MODE_1` = 1 / `MODE_2` = 2 / `MODE_3` = 3 | CPOL/CPHA combinations for `setMode`. |
| `void setFrequency(int hz)` / `void setMode(int)` | Clock (1 MHz until set) and mode (`MODE_0` until set). |
| `int transfer(byte[] tx, byte[] rx, int len)` | Full-duplex transfer; returns `len`. |
| `int write(byte[] data, int len)` | Write-only (RX discarded); returns `len`. |
| `void close()` | End the handle's use. |

## `picodroid.pio.Pwm`

```java
import picodroid.pio.Pwm;

Pwm pwm = pm.openPwm("GP25");

pwm.setPwmFrequencyHz(1000.0);          // 1 kHz
pwm.setPwmDutyCycle(50.0);              // 50% duty cycle (0.0–100.0)
pwm.setEnabled(true);                   // start PWM output

pwm.setEnabled(false);                  // stop PWM output
pwm.close();                            // or use try-with-resources
```

| Method | Description |
|--------|-------------|
| `void setPwmFrequencyHz(double)` | Carrier frequency in Hz. Set it before `setEnabled(true)`. |
| `void setPwmDutyCycle(double)` | Duty cycle, `0.0`–`100.0`. |
| `void setEnabled(boolean)` | Start / stop output. When disabled, the pin holds its last state. |
| `void close()` | End the handle's use; it does not stop the output. |

A PWM opens at 1 kHz, 0 % duty, disabled. For tones on a board's buzzer use [`ToneGenerator`](/api/media/), which drives the buzzer's PWM pad for you.

## `picodroid.pio.Adc`

```java
import picodroid.pio.Adc;

Adc adc = pm.openAdcPin("GP26");

double voltage = adc.readValue();       // single blocking read, returns volts
adc.close();                            // or use try-with-resources
```

| Method | Description |
|--------|-------------|
| `double readValue()` | Single blocking ADC conversion; returns volts, `0.0`–`3.3`. |
| `void close()` | End the handle's use. |

The ADC pins are GP26–GP29. `readValue()` performs a single 12-bit conversion and scales it to the 3.3 V reference. The simulator has no ADC and always reads 1.65 V.

## Examples

[`blinky`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/blinky) (GPIO output and input), [`uart`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/uart), [`i2cdemo`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/i2cdemo), [`spidemo`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/spidemo), [`pwmdemo`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/pwmdemo) and [`adcdemo`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/adcdemo).

---

**See also:** [core.md](/api/core/) (Java language) · [system.md](/api/system/) (logging, clock, threads) · [storage.md](/api/storage/) (files, preferences) · [networking.md](/api/networking/) (sockets) · [ui.md](/api/ui/) (display, widgets)
