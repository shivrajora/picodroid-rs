# Java System API

Java system APIs live under `sdk/java/picodroid/` and mirror the Android API surface. Native implementations are in `src/system/picodroid/`.

## `picodroid.util.Log`

```java
import picodroid.util.Log;

Log.i("TAG", "message");   // info log → defmt::info! over RTT
```

## `picodroid.os.SystemClock`

```java
import picodroid.os.SystemClock;

SystemClock.sleep(500);   // sleep for 500 ms
```

## `picodroid.pio.PeripheralManager`

Singleton for opening hardware peripherals.

```java
import picodroid.pio.PeripheralManager;

PeripheralManager pm = PeripheralManager.getInstance();
Gpio gpio       = pm.openGpio("GP25");
UartDevice uart = pm.openUartDevice("UART0");
I2cDevice  i2c  = pm.openI2cDevice("I2C0");
SpiDevice  spi  = pm.openSpiDevice("SPI0");
```

## `picodroid.pio.Gpio`

```java
import picodroid.pio.Gpio;

gpio.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
gpio.setValue(true);    // drive high
gpio.setValue(false);   // drive low
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

## `picodroid.concurrent.Thread`

```java
import picodroid.concurrent.Thread;

Thread t = new Thread(new MyRunnable());
t.start();   // spawns a FreeRTOS task that calls MyRunnable.run()
```

Each call to `t.start()` creates a dedicated FreeRTOS task with a 4096-word stack. When `MyRunnable.run()` returns, the task self-deletes and its stack is reclaimed automatically.

On hot-swap, any thread blocked inside `SystemClock.sleep()` is woken immediately so it can see the stop signal and exit cleanly before the new app starts.
