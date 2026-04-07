# Examples

Twenty-four examples are included under `examples/`, organized by category.

## Getting Started

Simple apps to verify your setup and get familiar with the build/flash workflow.

| Example | Class | Description |
|---------|-------|-------------|
| `helloworld` | `helloworld.HelloWorld` | Prints "Hello, World!" via `Log.i()` |
| `blinky` | `blinky.LedBlink` | Blinks the onboard LED on GP25 every 500 ms |

## Peripherals

Hardware interaction through the `picodroid.pio.PeripheralManager` API.

| Example | Class | Description |
|---------|-------|-------------|
| `uart` | `uart.UartEcho` | Configures UART0 at 115200 baud and echoes received bytes |
| `i2cdemo` | `i2cdemo.I2cDemo` | Scans the I2C0 bus (SDA=GP4, SCL=GP5) and logs the 7-bit address of every ACKing device |
| `spidemo` | `spidemo.SpiDemo` | Full-duplex loopback over SPI0 (SCK=GP2, MOSI=GP3, MISO=GP0): sends 0x00-0x0F and logs received bytes |
| `adcdemo` | `adcdemo.AdcDemo` | Opens the ADC on GP26 and takes 5 voltage readings, logging each value |
| `pwmdemo` | `pwmdemo.PwmDemo` | Fades the onboard LED on GP25 using PWM at 1 kHz -- duty cycle sweeps 0%->100%->0% three times |

## Language Features

Demonstrate Java language features supported by the JVM interpreter.

| Example | Class | Description |
|---------|-------|-------------|
| `arraydemo` | `arraydemo.ArrayDemo` | Demonstrates byte array allocation, `.length`, and iteration via UART |
| `inherit` | `inherit.InheritDemo` | Demonstrates class inheritance, field inheritance, method overriding, and `super()` |
| `interfacedemo` | `interfacedemo.InterfaceDemo` | Demonstrates interface dispatch (`invokeinterface`) with `Dog` and `Cat` implementing `Speakable` |
| `floatdemo` | `floatdemo.FloatDemo` | Demonstrates `float`, `long`, and `double` arithmetic and type conversions (`f2i`, `i2l`, `i2d`, etc.) |
| `exceptiondemo` | `exceptiondemo.ExceptionDemo` | Demonstrates `throw`, `try`/`catch`, and custom exception classes |
| `threaddemo` | `threaddemo.ThreadDemo` | Demonstrates spawning concurrent FreeRTOS tasks via `picodroid.concurrent.Thread` |
| `mathsdemo` | `mathsdemo.MathsDemo` | Demonstrates integer/long/double arithmetic, bitwise/shift ops, cross-type conversions, `tableswitch`, `instanceof`, `checkcast`, reference arrays, and `java.lang.Math` |
| `stringdemo` | `stringdemo.StringDemo` | Demonstrates `java.lang.String` and `StringBuilder` APIs: predicates, search, transforms, `String.valueOf`, and StringBuilder building |
| `listdemo` | `listdemo.ListDemo` | Demonstrates `java.util.ArrayList`: add, get, set, remove, contains, clear, and autoboxing with `Integer` and `Boolean` |
| `trywithresourcesdemo` | `trywithresourcesdemo.TryWithResourcesDemo` | Demonstrates `try`-with-resources (`AutoCloseable`) -- opens an ADC pin in a `try` block and confirms `close()` is called on exit |
| `lambdademo` | `lambdademo.LambdaDemo` | Demonstrates Java lambdas via `invokedynamic`: non-capturing, capturing, callbacks, and static method references |
| `anondemo` | `anondemo.AnonDemo` | Demonstrates anonymous classes implementing interfaces, with local variable capture |
| `clinitdemo` | `clinitdemo.ClinitDemo` | Demonstrates static class initializers (`<clinit>`): field initializers, `static {}` blocks, and cross-class chaining |
| `syncdemo` | `syncdemo.SyncDemo` | Demonstrates `synchronized` blocks (`monitorenter`/`monitorexit`) and reentrant locking |

## Graphics and Display

Full graphical UI with touch input, demonstrating the Activity lifecycle and LVGL widget system.

| Example | Class | Description |
|---------|-------|-------------|
| `displaydemo` | `displaydemo.DisplayDemoApp` | Full UI demo: Activity lifecycle, LinearLayout, TextView, Button, ToggleButton, Switch, touch input, and event handlers on a 320x240 display |

## Performance and Testing

Benchmarks and stress tests for the JVM runtime.

| Example | Class | Description |
|---------|-------|-------------|
| `benchmark` | `benchmark.Benchmark` | JVM performance benchmark: times int/long/float/double arithmetic, method dispatch, interface dispatch, object allocation, array ops, string ops, and control flow; logs per-category and total elapsed time |
| `gcstress` | `gcstress.GcStress` | GC stress test: exercises the mark-sweep collector under object churn, linked chains, circular references, string churn, and array churn; reports cycle count, freed entries, and GC time via `picodroid.os.Runtime` |

## Running an Example

```bash
./scripts/build.sh --app <name>
./scripts/flash.sh --app <name>
```

Or test on the host simulator without hardware:

```bash
./scripts/sim.sh --app <name>
```

See [getting-started.md](getting-started.md) for full build and flash options.
