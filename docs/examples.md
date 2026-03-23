# Examples

Nine examples are included under `java/examples/`:

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

To run an example:

```bash
./scripts/build.sh --app <name>
./scripts/flash.sh --app <name>
```

See [getting-started.md](getting-started.md) for full build and flash options.
