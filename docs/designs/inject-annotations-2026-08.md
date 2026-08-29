# Design: compile-time dependency injection — `@Inject` / `@Singleton` — 2026-08-28

**Goal:** Dagger/Hilt-shaped DI for picodroid apps. Developers write JSR-330
`@javax.inject.Inject` constructors / fields / methods and
`@javax.inject.Singleton` classes; the build generates the wiring; the
framework injects `Application`, `Activity` and `Service` instances
automatically before `onCreate()`. Nothing is resolved at runtime and nothing
about the annotations reaches the device.

This doc records what was decided, why, and the contract between the three
pieces (annotation processor, generated code, runtime probe) so the follow-ups
at the bottom can be executed cold. Verified against the tree at `ad79a98`
(file:line references are from that commit unless noted).

## Why (context)

- The existing DI is two hand-written SDK classes,
  `sdk/java/picodroid/di/ApplicationComponent.java` (process singleton via a
  static `INSTANCE` + `current()`) and `ActivitySingletonComponent.java`,
  from `22a5462`. They work, but every consumer writes
  `(EnvAppComponent) EnvAppComponent.current()` and each Activity component
  hand-plumbs pass-through accessors. Android developers expect `@Inject`.
- pico-jvm cannot do Hilt's runtime half. The class-file parser keeps only
  `Code`, `LineNumberTable` and `BootstrapMethods`
  (`jvm/src/class_file/parse.rs`); `FieldInfo` is `{name, descriptor}`;
  `java.lang.Class` exposes only `getName()`. So the whole thing must be
  host-side codegen producing ordinary classes.

## Constraints that shaped the design (all verified)

| Constraint | Where | Consequence |
|---|---|---|
| No reflection, annotations dropped at parse | `jvm/src/class_file/parse.rs`, `sdk/java/java/lang/Class.java` | `SOURCE` retention; generated Java only |
| Generated classes ship in the PAPK for free | `tools/papk-pack` packs every `.class` in the compile output; no keep-list | Cost class **P**: 0 firmware flash, ~20 B RAM/class registered, ~88 B + 5 B/CP entry + 32 B/method when first touched |
| New SDK classes cost flash on every board | `build_support/papk.rs` embeds `sdk/java/**` into firmware; RP2040 gate ~58 KB headroom | **Zero new SDK classes** this cut (no `Provider<T>`/`Lazy<T>`) |
| Statics are shared + GC-rooted | `SharedJvmHeap.statics` (`jvm/src/lib.rs:107-121`), `jvm/src/gc/mod.rs:313` | A `static` holder is a correct process singleton across threads |
| `synchronized` blocks work, methods don't | `monitorenter` → handler; `ACC_SYNCHRONIZED` never read | Singleton = double-checked locking on `synchronized (Foo_Factory.class)` |
| `Foo::new` rejected; lambda proxies capture every call | `jvm/src/interpreter/ops_invoke.rs:398`, `try_lambda_dispatch` | Generated code uses no lambdas or method refs |
| Fields resolve by **name only** on the runtime class | `jvm/src/interpreter/ops_fields.rs:85-126` | Shadowed `@Inject` field names are a compile error |
| Framework components are constructed by name with null-padded `<init>` | `lifecycle.rs::instantiate_component`, `jvm/src/frame.rs:41` | `@Inject` constructors on `Application`/`Activity`/`Service` are a compile error |
| `invoke_lifecycle` looks up leaf-only, falls back to the framework default | `picodroid-core/src/lifecycle.rs:581-611` (pre-change numbering) | A generated `Hilt_MyActivity` base class would be skipped → inject from Rust instead |
| `find_method_by_name` ignores descriptors | `jvm/src/lib.rs:505-528` | Never two overloads of `get`/`injectMembers` in one generated class |
| Entry-path invocation does not run `<clinit>` for the entry class | `Jvm::invoke_static_with_args` → `interpreter::execute` | Injectors are stateless; factories are only reached via bytecode `invokestatic` |

## Decisions (settled with the project owner, 2026-08-28)

- **`javax.inject.*`**, JSR-330 names (`Inject`, `Singleton`, `Scope`), in a
  compile-only Gradle project `inject/annotations/`. `SOURCE` retention —
  documented divergence from JSR-330's `RUNTIME`. Nothing ships on-device.
- **Automatic, Hilt-style injection** of framework-owned components via a
  runtime probe, not an explicit `Injector.inject(this)` call and not a
  generated intermediate base class.
- **Java only** this cut. Kotlin apps get neither the annotations nor the
  processor, so a Kotlin `@Inject` fails at compile time instead of silently
  doing nothing. kapt/KSP is a follow-up.
- **Examples:** new `examples/injectdemo`; `examples/picoenvmon` migrated off
  its hand-written `di/` package. `dialogdemo`/`gesturedemo` and the manual
  SDK classes stay; the two styles coexist (a hand-written
  `ApplicationComponent` subclass can be `@Singleton @Inject`-constructed).

## The three pieces

### 1. `inject/annotations` (`:inject:annotations`)

`java/javax/inject/{Inject,Singleton,Scope}.java`. `Singleton` is
`@Scope`-meta-annotated for JSR-330 shape and targets `{TYPE, METHOD}`
(Error Prone's `InjectInvalidTargetingOnScopingAnnotation`; the processor
rejects it on methods until `@Provides` exists). Wired `compileOnly` into every
Java app by `buildSrc/src/main/kotlin/picodroid/PicodroidPapkPlugin.kt`;
override path with `-Ppicodroid.injectAnnotationsProjectPath` for out-of-tree
apps.

### 2. `inject/compiler` (`:inject:compiler`)

`picodroid.inject.compiler.InjectProcessor`, a javac `AbstractProcessor`
(declared `aggregating`), zero runtime dependencies, `StringBuilder` writers.
Wired `annotationProcessor` into every Java app by the same plugin
(`-Ppicodroid.injectCompilerProjectPath`). Runs once (round 1 holds every user
source; generated sources carry no annotations), builds an `InjectionGraph`
from the root elements (nested types included), validates, then writes. Error
Prone is kept off `build/generated/**` (root `build.gradle.kts`).

**Generated-code contract** (same package as the annotated class, Java 8,
fully-qualified names, no imports, no lambdas, header
`// Generated by picodroid inject (...). Do not edit.`):

- `Foo_Factory` for every class with an `@Inject` constructor:
  `public static Foo get()`. Unscoped: `new Foo(A_Factory.get(), …)` then
  `X_MembersInjector.injectMembers(instance)` where `X` is `Foo` if it has
  its own `@Inject` members, else its nearest ancestor that does. `@Singleton`:
  DCL on `private static Foo instance` inside
  `synchronized (Foo_Factory.class)`; members are injected before publication.
- `Foo_MembersInjector` with `public static void injectMembers(Foo instance)`
  for every class with its own `@Inject` fields/methods: nearest injectable
  ancestor's injector first, then fields, then methods, in declaration order.
  **Also generated for every concrete `Application`/`Activity`/`Service`
  subclass whose chain has injectable members even if the leaf declares
  none**, so the runtime probes exactly one name.
- Nested classes: `Outer.Inner` → `Outer_Inner_Factory` (Dagger convention);
  the runtime maps `$` → `_` to match.

**Validation rules** (each a `Messager` error at the element; the golden and
diagnostic tests in `inject/compiler/src/test/java` pin them):
one `@Inject` constructor; not on abstract classes, enums, interfaces or
framework components; not private; `@Inject` fields non-private/final/static;
`@Inject` methods non-private/static/abstract/generic; dependencies must be
concrete, non-generic classes with an `@Inject` constructor in the same
compilation (no primitives, arrays, parameterized types, interfaces, abstract
classes); no cycles over constructor + member edges; `@Singleton` only on
classes with an `@Inject` constructor, no other `@Scope`; no shadowed
`@Inject` field names up or down the class chain; no inner (non-static
nested), local or anonymous classes; no generic classes.

### 3. Runtime probe (`picodroid-core/src/lifecycle.rs`)

`instantiate_component` (Activity push, Service create, and — new — the
`Application` and manifest-`activity=` boot paths) now does: `alloc_with_defaults`
→ `<init>` → `inject_members`. `inject_members` builds
`"<class>_MembersInjector"` (`$`→`_`), calls
`jvm.invoke_static_with_args(name, "injectMembers", [obj])`; `MethodNotFound`
means "no injector" (one linear class-table scan, the common case), a fault is
logged like a faulted `<init>`, `Interrupted` propagates as `None`.

**Behaviour change shipped with this:** `Application` (and a boot Activity)
previously got a bare `alloc()` with no `<init>` — instance initializers and
constructors on an `Application` subclass never ran. They do now
(Android-faithful). No in-tree `Application` had any, so nothing regressed.

## Cost (measured 2026-08-28)

| Item | Value |
|---|---|
| Firmware flash for the probe + Application/boot refactor | testbench_rp2040 +928 B, testbench_rp2350 +880 B (`bench/parity/ratchet.toml`) |
| `injectdemo` PAPK | 19 classes (9 user + 10 generated), 13.8 KB |
| `picoenvmon` PAPK | 63,222 → 69,170 B (+5.9 KB, 0 firmware flash): 12 generated classes (6 factories, 6 injectors) + `EnvPrefs` replace the 2 hand-written components |
| RAM per generated class | ~20 B registered at boot; ~88 B + CP + 32 B/method when first touched (each factory: 2 methods; each injector: 2 methods) |

## Follow-ups (not started)

1. **`Provider<T>` / `Lazy<T>`** — JSR-330 `Provider` is an interface with
   one method; cheapest home is a tiny SDK interface (cost class S, ~300 B) or
   generation into each PAPK. Would also let cycles be broken the Dagger way.
2. **`@Provides` / `@Module` / `@Binds`** — the missing piece for SDK types
   (`SharedPreferences`, `SensorManager`) and interfaces; today apps wrap them
   (`picoenvmon`'s `EnvPrefs`). Natural design: a `@Module` class with static
   `@Provides` methods, called from factories exactly like `_Factory.get()`.
3. **Qualifiers** (`@Named`, custom `@Qualifier`) — keyed bindings.
4. **Activity scope** (`@ActivityScoped`) — per-Activity instances shared by
   that Activity's graph; needs an activity-lifetime holder.
5. **One aggregate graph class per app** instead of a factory per class —
   trades Dagger fidelity for fewer registered classes (~20 B + parse cost
   each) on RAM-tight boards.
6. **Kotlin** — kapt or KSP in `PicodroidPapkKotlinPlugin`; generated Java must
   flow through `stageClasses` → `stripClassMetadata` untouched (it is not
   under `kotlin/**` or `picodroid/shim/**`, so it will).
7. **`sdk/keep.toml`'s `picodroid/annotation/KeepName`** is a stale,
   never-implemented hook; unrelated to this work, left alone.
