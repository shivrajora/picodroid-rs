# pico-jvm

A `no_std` Java bytecode interpreter for bare-metal embedded systems.

Parses and executes Java `.class` files on `no_std + alloc` targets with no OS or hardware dependencies. It is the core of [Picodroid](https://github.com/shivrajora/picodroid-rs), a stripped-down Android-style runtime for the Raspberry Pi Pico, but can be embedded in any Rust project.

## Usage

```toml
[dependencies]
pico-jvm = "0.1"
```

```rust
use pico_jvm::{Jvm, SharedJvmHeap, NativeContext, NativeMethodHandler};
use pico_jvm::types::{JvmError, Value};

// 1. Implement NativeMethodHandler for your platform.
struct MyHandler;

impl NativeMethodHandler for MyHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        match (class_name, method_name) {
            ("com/example/Io", "println") => {
                if let Some(Value::Reference(idx)) = ctx.args.first() {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    // write s to your platform output
                }
                Some(Ok(None))
            }
            _ => None, // fall through to BuiltinHandler (java/lang/*)
        }
    }
}

// 2. Embed compiled .class bytes.
static MY_CLASS: &[u8] = include_bytes!("MyApp.class");

// 3. Run.
let mut jvm = Jvm::new();
let mut heap = SharedJvmHeap::new();
jvm.load_class(MY_CLASS).unwrap();
jvm.invoke_static("MyApp", "main", &mut heap, &mut MyHandler).unwrap();

// To call an instance method, pass the ObjectHeap index of the receiver.
jvm.invoke_instance("MyApp", "run", obj_ref, &mut heap, &mut MyHandler).unwrap();

// Reset all heap state before running a new app.
heap.reset();
```

## Native method dispatch

Your [`NativeMethodHandler`] is called for every Java `native` method and for any method not found in a loaded `.class` file. Return `None` to pass the call to the built-in [`BuiltinHandler`], which handles:

| Class | Methods |
|---|---|
| `java/lang/Object` | `<init>` |
| `java/lang/Throwable / Exception / RuntimeException` | `<init>` |
| `java/lang/StringBuilder` | `<init>`, `<init>(String)`, `append(String/int/long/float/double/char/boolean)`, `length`, `charAt`, `toString` |
| `java/lang/String` | `length`, `charAt`, `isEmpty`, `equals`, `equalsIgnoreCase`, `compareTo`, `startsWith`, `endsWith`, `contains`, `indexOf`, `lastIndexOf`, `substring`, `trim`, `toUpperCase`, `toLowerCase`, `valueOf` |
| `java/lang/Integer` | `<init>`, `valueOf`, `intValue` |
| `java/lang/Boolean` | `<init>`, `valueOf`, `booleanValue` |
| `java/lang/Long` | `<init>`, `valueOf`, `longValue` |
| `java/lang/Float` | `<init>`, `valueOf`, `floatValue` |
| `java/lang/Double` | `<init>`, `valueOf`, `doubleValue` |
| `java/util/ArrayList` | `<init>`, `add`, `get`, `size`, `isEmpty`, `set`, `remove`, `clear`, `contains` |
| `java/lang/Math` | `abs`, `min`, `max` (int/long/float/double), `sqrt`, `pow`, `floor`, `ceil`, `round`, `sin`, `cos`, `tan`, `atan2`, `toRadians`, `toDegrees`, `log`, `log10`, `exp` |

If neither handler claims the call, [`JvmError::NoSuchMethod`] is returned.

### Garbage collection

The interpreter includes a **stop-the-world mark-sweep collector**. It fires automatically every 256 heap allocations, scanning roots from all live frames (locals + operand stacks) and static fields, marking reachable objects/arrays/strings, then freeing the rest.

Your handler receives a callback after each GC cycle via `report_gc`:

```rust
impl NativeMethodHandler for MyHandler {
    fn report_gc(&mut self, elapsed_nanos: u64, freed: usize) {
        // elapsed_nanos: wall-clock time spent in this GC cycle
        // freed: number of heap entries reclaimed
    }
}
```

The default `report_gc` implementation is a no-op.

### Cooperative interruption

Override `interrupted()` on your handler to signal a clean stop (e.g. for hot-swap app deployment). The interpreter checks it once per bytecode instruction and returns [`JvmError::Interrupted`] when `true`.

```rust
impl NativeMethodHandler for MyHandler {
    fn interrupted(&self) -> bool {
        self.stop_flag.load(Ordering::Relaxed)
    }
    // ...
}
```

## Supported Java language features

| Feature | Example |
|---|---|
| Arrays | `byte[] buf = new byte[16];` |
| Inheritance | `class Dog extends Animal` — virtual dispatch, `super()` |
| Interfaces | `interface Speakable` / `implements` |
| Exceptions | `throw new AppException()`, `try`/`catch` |
| Switch | `switch`/`case` — `lookupswitch` and `tableswitch` |
| Static fields | `static` field declarations |
| Null checks | `ifnull` / `ifnonnull` |
| Threading | `new Thread(runnable).start()` (platform-provided via native handler) |
| Numeric types | `int`, `long`, `float`, `double` — arithmetic, bitwise, casts |
| Type checking | `instanceof`, `checkcast` |
| Reference arrays | `new SomeClass[n]`, `aastore`, `aaload` |

## License

GPL-3.0-only. See [LICENSE](https://github.com/shivrajora/picodroid-rs/blob/main/LICENSE) in the repo root.
