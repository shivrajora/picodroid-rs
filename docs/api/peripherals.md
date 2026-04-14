# Peripherals (PIO)

Hardware peripherals: GPIO, UART, I2C, SPI, PWM, ADC. All under `picodroid.pio.*`. See [docs/README.md](../README.md) for the full API index.

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

## Resource management (`AutoCloseable`)

All peripheral classes implement `java.lang.AutoCloseable`, so they can be used in try-with-resources blocks. `close()` releases the hardware resource and is guaranteed to be called even if the body throws.

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
```

## `picodroid.pio.UartDevice`

```java
import picodroid.pio.UartDevice;

uart.setBaudrate(115200);
uart.setDataSize(8);
uart.setParity(UartDevice.PARITY_NONE);
uart.setStopBits(1);
int b = uart.readByte();    // non-blocking; returns -1 if RX FIFO empty
uart.writeByte(0x41);       // blocking write of single byte
```

## `picodroid.pio.I2cDevice`

Default pins: I2C0 → SDA=GP4, SCL=GP5; I2C1 → SDA=GP2, SCL=GP3.

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

## `picodroid.pio.Adc`

```java
import picodroid.pio.Adc;

Adc adc = pm.openAdcPin("GP26");

double voltage = adc.readValue();       // single blocking read, returns volts
adc.close();                            // or use try-with-resources
```

Pins are GPIO numbers (e.g. GP26–GP29 on RP2040). `readValue()` performs a single ADC conversion and returns the voltage.

---

**See also:** [core.md](core.md) (Java language) · [system.md](system.md) (logging, clock, threads) · [storage.md](storage.md) (files, preferences) · [networking.md](networking.md) (sockets) · [ui.md](ui.md) (display, widgets)
