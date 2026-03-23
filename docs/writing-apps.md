# Writing a Java App

1. Create a directory and Java file under `java/examples/`:

```java
// java/examples/myapp/java/myapp/MyApp.java
package myapp;

import picodroid.util.Log;

public class MyApp {
    public static void main(String[] args) {
        Log.i("MyApp", "Hello from MyApp!");
    }
}
```

2. Add a feature for it in `Cargo.toml`:

```toml
[features]
default = ["blinky", "chip-rp2040"]
helloworld = []
blinky = []
uart = []
myapp = []       # add this
```

3. Add a `cfg(feature)` block in `src/app.rs` inside `run_jvm()`:

```rust
#[cfg(feature = "myapp")]
{
    jvm.load_class(MYAPP_MYAPP_CLASS).unwrap();
    jvm.load_class(PICODROID_UTIL_LOG_CLASS).unwrap();
    jvm.invoke_static("myapp/MyApp", "main", heap, &mut handler).unwrap();
}
```

4. Build and flash:

```bash
./scripts/build.sh --app myapp
./scripts/flash.sh --app myapp

# Or for Pico 2
./scripts/build.sh --app myapp --chip rp2350
./scripts/flash.sh --app myapp --chip rp2350
```

The build system automatically detects new `.java` files, compiles them with `javac`, and embeds the resulting `.class` bytecode into firmware Flash. The constant name for a class is derived from its path: `java/examples/myapp/java/myapp/MyApp.class` → `MYAPP_MYAPP_CLASS`.

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
| Switch statements | `switch`/`case` on integer and other supported types |
| Static fields | `static` field declarations and access via `getstatic`/`putstatic` |
| Null checks | Null reference detection (`ifnull`/`ifnonnull`) |
| Threading | `new Thread(runnable).start()` — spawns a FreeRTOS task per thread |
| Arithmetic ops | subtraction, division, remainder, negation for `int`, `long`, `double` |
| Bitwise / shifts | `<<`, `>>`, `>>>`, `|`, `^` for `int` and `long` |
| Cross-type casts | `i2f`, `i2c`, `i2s`, `l2f`, `l2d`, `f2l`, `f2d`, `d2l`, `d2f` |
| Dense switch | consecutive-case `switch` compiled to `tableswitch` |
| Type checking | `instanceof`, `checkcast` |
| Reference arrays | `new SomeClass[n]`, element store and load (`aastore`, `aaload`) |
