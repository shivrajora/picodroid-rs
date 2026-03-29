# Examples

Fifteen examples are included under `examples/`:

| Example | Class | Description |
|---------|-------|-------------|
| `blinky` | `blinky.LedBlink` | Blinks the onboard LED on GP25 every 500 ms |
| `uart` | `uart.UartEcho` | Configures UART0 at 115200 baud and echoes received bytes |
| `helloworld` | `helloworld.HelloWorld` | Prints "Hello, World!" via `Log.i()` |
| `arraydemo` | `arraydemo.ArrayDemo` | Demonstrates byte array allocation, `.length`, and iteration via UART |
| `inherit` | `inherit.InheritDemo` | Demonstrates class inheritance, field inheritance, method overriding, and `super()` |
| `interfacedemo` | `interfacedemo.InterfaceDemo` | Demonstrates interface dispatch (`invokeinterface`) with `Dog` and `Cat` implementing `Speakable` |
| `floatdemo` | `floatdemo.FloatDemo` | Demonstrates `float`, `long`, and `double` arithmetic and type conversions (`f2i`, `i2l`, `i2d`, etc.) |
| `exceptiondemo` | `exceptiondemo.ExceptionDemo` | Demonstrates `throw`, `try`/`catch`, and custom exception classes |
| `threaddemo` | `threaddemo.ThreadDemo` | Demonstrates spawning concurrent FreeRTOS tasks via `picodroid.concurrent.Thread` |
| `mathsdemo` | `mathsdemo.MathsDemo` | Demonstrates integer/long/double arithmetic (`sub`, `div`, `rem`, `neg`), bitwise/shift ops, cross-type conversions, `tableswitch`, `instanceof`, `checkcast`, reference arrays, and `java.lang.Math` (`abs`, `min`, `max`, `sqrt`, `pow`, `floor`, `ceil`, `round`, trig, `log`, `exp`) |
| `i2cdemo` | `i2cdemo.I2cDemo` | Scans the I2C0 bus (SDA=GP4, SCL=GP5) and logs the 7-bit address of every ACKing device |
| `spidemo` | `spidemo.SpiDemo` | Full-duplex loopback over SPI0 (SCK=GP2, MOSI=GP3, MISO=GP0): sends 0x00–0x0F and logs received bytes |
| `stringdemo` | `stringdemo.StringDemo` | Demonstrates `java.lang.String` and `StringBuilder` APIs: predicates, search, transforms (`substring`, `trim`, `toUpperCase`, `toLowerCase`), `String.valueOf`, and StringBuilder building |
| `listdemo` | `listdemo.ListDemo` | Demonstrates `java.util.ArrayList`: add, get, set, remove, contains, clear, and autoboxing with `Integer` and `Boolean` |
| `trywithresourcesdemo` | `trywithresourcesdemo.TryWithResourcesDemo` | Demonstrates `try`-with-resources (`AutoCloseable`) — opens an ADC pin in a `try` block and confirms `close()` is called on exit |

To run an example:

```bash
./scripts/build.sh --app <name>
./scripts/flash.sh --app <name>
```

See [getting-started.md](getting-started.md) for full build and flash options.
