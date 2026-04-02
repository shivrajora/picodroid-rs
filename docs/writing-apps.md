# Writing a Java App

1. Create a directory and Java file under `examples/`:

```java
// examples/myapp/java/myapp/MyApp.java
package myapp;

import picodroid.util.Log;

public class MyApp {
    public static void main(String[] args) {
        Log.i("MyApp", "Hello from MyApp!");
    }
}
```

2. Create a `PicodroidManifest.xml` in the app directory:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest package="myapp" version="1.0">
    <application main-class="myapp/MyApp" />
</manifest>
```

3. Build and flash — no changes to `Cargo.toml` or `src/app.rs` needed:

```bash
./scripts/build.sh --app myapp
./scripts/flash.sh --app myapp

# Or for Pico 2
./scripts/build.sh --app myapp --chip rp2350
./scripts/flash.sh --app myapp --chip rp2350
```

`build-apk.sh` automatically discovers all `.class` files under the compiled output directory and packages them into `build/apks/myapp.papk`. The firmware embeds this `.papk` at build time; the JVM reads the manifest at startup to find the entry point.

## Porting to a New Platform

The `pico-jvm` crate is hardware-independent (`no_std + alloc` only). To use it on a different platform, implement the HAL modules and `NativeMethodHandler` trait. See [porting-guide.md](porting-guide.md) for the full guide, including HAL function signatures, FreeRTOS configuration, and build system setup.

## Supported Language Features

| Feature | Example |
|---------|---------|
| Arrays | `byte[] buf = new byte[16];` — allocation, `.length`, index read/write |
| Inheritance | `class Dog extends Animal` — field inheritance, `@Override`, `super()` constructor chaining, virtual dispatch |
| Interfaces | `interface Speakable` / `implements` — `invokeinterface` polymorphic dispatch |
| Floating-point | `float`/`double` arithmetic, `f2i`/`f2d` casts |
| Long integers | `long` arithmetic, `i2l`/`l2i` type conversions |
| Double precision | `double` arithmetic, `i2d`/`d2i` type conversions |
| Exceptions | `throw new AppException()`, `try`/`catch`, custom exception classes |
| Try-with-resources | `try (Gpio gpio = pm.openGpio("GP25")) { ... }` — `AutoCloseable` peripherals; `close()` called on normal exit and on exception |
| Switch statements | `switch`/`case` on integer and other supported types |
| Static fields | `static` field declarations and access via `getstatic`/`putstatic` |
| Null checks | Null reference detection (`ifnull`/`ifnonnull`) |
| Threading | `new Thread(runnable).start()` — spawns a FreeRTOS task per thread, pinned to core 0; stack reclaimed when `run()` returns; priority set via `setPriority(1–10)` before `start()` |
| Arithmetic ops | subtraction, division, remainder, negation for `int`, `long`, `double` |
| Bitwise / shifts | `<<`, `>>`, `>>>`, `|`, `^` for `int` and `long` |
| Cross-type casts | `i2f`, `i2c`, `i2s`, `l2f`, `l2d`, `f2l`, `f2d`, `d2l`, `d2f` |
| Dense switch | consecutive-case `switch` compiled to `tableswitch` |
| Type checking | `instanceof`, `checkcast` |
| Reference arrays | `new SomeClass[n]`, element store and load (`aastore`, `aaload`) |
| String predicates | `equals`, `equalsIgnoreCase`, `startsWith`, `endsWith`, `contains`, `isEmpty`, `compareTo` |
| String search | `indexOf(char/String)`, `lastIndexOf(char/String)` |
| String transforms | `substring(int)`, `substring(int,int)`, `trim()`, `toUpperCase()`, `toLowerCase()` |
| String factory | `String.valueOf(int/long/boolean/char/float/double)` |
| StringBuilder | `new StringBuilder("seed")`, `append(String/int/long/float/boolean/char)`, `length()`, `charAt(int)`, `toString()` |
| ArrayList | `new ArrayList()`, `add`, `get`, `size`, `isEmpty`, `set`, `remove(int)`, `clear`, `contains` — dynamic list backed by heap |
| Autoboxing | `Integer`, `Boolean`, `Long`, `Float`, `Double` — `valueOf` / `intValue` etc.; enables storing primitives in `ArrayList<Integer>` etc. |

## Garbage Collection

The JVM runs a **stop-the-world mark-sweep collector** automatically. After every 256 heap allocations it scans all roots (frame locals, operand stacks, static fields), marks every reachable object, array, and string, then frees everything unreachable. There is no action needed from app code — GC fires transparently between bytecode instructions.

To introspect GC behavior from Java code, use `picodroid.os.Runtime`:

```java
import picodroid.os.Runtime;

int  count = Runtime.gcCount();      // cycles run since boot (or last reset)
int  freed = Runtime.gcFreed();      // entries freed across all cycles
long ns    = Runtime.gcTimeNanos();  // cumulative time spent in GC
Runtime.resetGcStats();              // zero all counters
```
