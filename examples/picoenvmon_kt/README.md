# picoenvmon_kt — the Kotlin twin of `examples/picoenvmon`

Same app, same screens, same Service, same dashboard HTTP server, same
`@Inject`/`@Singleton`/`@Module`/`@Provides` object graph — written in Kotlin
(roadmap Session 7, `docs/designs/kotlin-roadmap-2026-08.md`). It exists so the
RAM and PAPK cost of Kotlin on pico-jvm can be measured like-for-like against
the Java app; the numbers live in `docs/designs/kotlin-shim-inventory.md` § 10.

Run it the way you run the Java app: `./scripts/sim.sh -b pico_enviro_mon_w -a picoenvmon_kt`
(add `-m` for the memory census, then `./scripts/sim-ctrl.sh heapcensus`), or
`./scripts/flash.sh --board pico_enviro_mon_w --app picoenvmon_kt --release`
with `.wifi-creds.env` sourced. Log tags are `PicoEnvMonKt`, `SensorLoggerKt`
and `RgbLedKt` so a device or sim log never confuses the twins; the Activity
class names are unchanged, so `scripts/soak/*` drives both.

## Frugality rules (why the code looks the way it does)

pico-jvm's binding constraint is **class metadata**, not the object heap: every
class in the PAPK costs 20 B at boot, every *parsed* class ~0.8 KB on device,
and every method 32 B. Kotlin codegen adds classes and methods freely unless
told not to, so:

- **No `companion object`, anywhere.** Each one is a parsed class (`<clinit>`
  news it). Cross-file constants are top-level `const val` (inlined at the use
  site; the `*Kt` facade is registered but never parsed); file-private ones are
  top-level `private const val`.
- **Stateless utilities are top-level functions** with `@file:JvmName(...)`
  (`TimeFormat`, `SntpClient`, `WeatherFetcher`, `ButtonHintBar`): plain
  `invokestatic`, no `INSTANCE`, no `<clinit>`.
- **`@JvmField`** on every field read or written across classes
  (`ThresholdConfig`'s thresholds, `LocalBinder.service`); `private val/var`
  inside a class emits no accessors at all. `var x … private set` keeps a
  getter only. Injected fields are `@Inject lateinit var` (a public backing
  field is what the generated injector writes).
- **No stdlib collections, sequences, `Pair`, `lazy`, data classes, enums.**
  The Java app uses none of them either; every one would pull shim classes
  into the PAPK.
- **Bytes, not `toByteArray()`.** Kotlin's `String.toByteArray()` and
  `String(bytes, off, len)` inline to the `Charset` overloads, which pico-jvm
  does not serve. `Formatter.ascii(s)` (`(s as java.lang.String).bytes`, i.e.
  `getBytes()`) makes the dashboard's constant fragments, and the 404 log line
  is built with `StringBuilder.append(Char)`.
- **No `trim()`, `x in intArray`, `indexOf`, `contentEquals`** — each is a
  `kotlin/collections` or `kotlin/text` shim call on a hot path; the loops are
  written out.
- `import picodroid.concurrent.Thread` — Kotlin default-imports `java.lang.*`,
  and `java.lang.Thread` does not exist on pico-jvm.
- `?.` over `!!` (a branch instead of an `Intrinsics.checkNotNull` call);
  `when` over `Sensor.TYPE_*` (Java `static final int`s inline to a
  `lookupswitch`, no `$WhenMappings`); `Foo::class.java` for `Intent`.
- `fun interface` for the two app-defined callbacks; the two-method SDK
  interfaces (`ServiceConnection`, `SensorEventListener`) are class
  supertypes, as in Java.

Every `kotlin/**` and `java/**` reference the app makes is checked at build
time by `:kotlin-shim:contractCheck` (this app is a `shimFixtures` input);
a new idiom that needs the shim fails the build with a paste-ready signature.
