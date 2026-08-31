---
title: "Kotlin apps"
description: "Writing Picodroid apps in Kotlin: toolchain, the supported subset, JVM divergences, class-metadata frugality, DI via kapt, and what to do when the shim contract check fails."
---

Picodroid apps can be written in Kotlin. Kotlin compiles to the same bytecode
the JVM interpreter already runs; a small hand-written stdlib shim
(`kotlin-shim/` in the repo) rides inside each Kotlin app's PAPK and supplies
the `kotlin/**` entry points the compiler emits. Only the shim classes your app
actually reaches ship — the strip step prunes the rest — so a minimal Kotlin
app costs a few hundred bytes over its Java twin.

## Toolchain

```bash
./gradlew newApp -Pname=myapp -Plang=kotlin   # scaffold examples/myapp
./scripts/sim.sh --app myapp                  # run it — same as a Java app
```

- Sources live in `kotlin/<package>/` and the app's `build.gradle.kts` applies
  `picodroid-papk-kotlin` (Kotlin 2.1). Everything downstream —
  `build-apk.sh`, the simulator, `flash.sh`, `pdb`, class-name shrinking — is
  identical to the Java pipeline.
- Formatting is `bash scripts/format_kotlin.sh format` (ktfmt, the Kotlin twin
  of the repo's google-java-format hook); the pre-commit suite enforces it.
- Compile-time DI works unchanged: the annotation processor runs through kapt
  automatically, so `@Inject`, `@Singleton`, `@Module` / `@Provides`,
  `Provider<T>` and `picodroid.di.Lazy<T>` are all available. The
  Kotlin-specific shapes and traps are covered in
  [Services & DI](/api/services/).

## What is supported

The full tables live in the
[Android compatibility matrix](/reference/compatibility-matrix/); the short
version:

| Works | Notes |
|---|---|
| Lambdas & SAM conversion | `invokedynamic`, capturing and non-capturing; `fun interface` |
| Null safety | `?.`, `?:`, `!!`, smart casts (`!!` calls the shim's `Intrinsics`) |
| String templates | compile to `StringBuilder` chains |
| Data classes, sealed classes + `when`, enums | `Enum.valueOf(String)` is the one gap — use `values()` + `name()` |
| Objects, companions, interface defaults | every `companion object` is its own class — see frugality below |
| Extension / infix / default & named args / varargs | including mixed spread calls (`f(1, *xs)`) |
| Scope functions, `lazy`, `Pair`, destructuring | |
| Ranges & progressions | `for` over ranges is intrinsified (free) |
| Collections, maps, sets, arrays, sorting, strings, math | via the shim over the JVM's builtin `ArrayList`/`HashMap`/`HashSet` |
| `synchronized {}` blocks, `picodroid.concurrent.Thread` | `@Synchronized` *methods* are not synchronized — the flag is ignored |

Not supported — each fails **loudly** (a build-time contract error or a
runtime `NoSuchMethod`), never silently:

- **Coroutines** and `suspend` functions (no `kotlin.coroutines` shim).
- **Reflection** beyond `Foo::class.java`: property references
  (`::prop`, `::prop.isInitialized`), `KClass` members, `is (A) -> B`
  function-type checks.
- `kotlin.io` (`println` → use `picodroid.util.Log`).
- `String.toByteArray()` and `String(bytes, off, len)` — they inline to
  `Charset` overloads the JVM does not serve. Cast through the platform type
  instead: `(s as java.lang.String).bytes` (plain `getBytes()`).
- `java.lang.Thread` — Kotlin default-imports `java.lang.*`; import
  `picodroid.concurrent.Thread` explicitly.
- A handful of stdlib members that walk unsupported JDK surface:
  `list.last {}` / `indexOfLast` / `findLast` (need `listIterator`),
  `map += otherMap` (`putAll`), `contentEquals` (`Arrays.equals`),
  `isNaN()` / `isInfinite()` / `isFinite()`, `String(chars)`,
  `toTypedArray()`.

## JVM divergences to know

| Divergence | Consequence |
|---|---|
| `mutableMapOf` / `mutableSetOf` / `mapOf` / `toSet` are **unordered** | `LinkedHashMap` / `LinkedHashSet` alias to `HashMap` / `HashSet`; don't rely on insertion order |
| Identity `hashCode()` is the heap slot index | stable for any live object, but **reused after GC** — don't key long-lived maps on identity hashes of short-lived objects |
| `toString()` on builtin collections is identity-formatted | build your own string for display |
| `@Synchronized` methods are not synchronized | `synchronized(lock) {}` blocks work |
| Arrays are capped at 65,535 elements | allocation beyond that fails |
| `toArray` returns a fresh array | never the backing store |
| Boxed accessors do not convert | `(n as java.lang.Integer).toFloat()`-style cross-conversions return the raw value |
| `Character` predicates are ASCII-only | `isDigit`, `isLetter`, case mapping |

The repo's `examples/gcstress_kt` exercises the collector under exactly the
churn Kotlin adds (lambda proxies, `Ref` boxes, autoboxing, `Pair`, entry
views) and asserts the slot-stability and capture-rooting behaviour above.

## Class-metadata frugality

On device, the binding constraint is usually **class metadata**, not the
object heap: every class in the PAPK costs ~20 B registered at boot, every
*parsed* class ~0.8 KB, every method 32 B — and Kotlin codegen mints classes
freely unless told not to. For a long-running app on a small board:

- **Avoid `companion object`** — each is a parsed class. Use top-level
  `const val` (inlined at the use site) and top-level functions with
  `@file:JvmName("...")` (plain `invokestatic`, no `INSTANCE`, no `<clinit>`).
- **`@JvmField`** on fields shared across classes; `private val`/`var` inside
  a class emits no accessors; `var x … private set` keeps only a getter.
- On frugal apps, skip stdlib collections, sequences, `Pair`, `lazy`, data
  classes and enums — each pulls shim classes into the PAPK. (Throwaway and
  test apps can use them freely; that is what the shim is for.)
- Prefer `?.` over `!!` (a branch instead of an `Intrinsics` call) and `when`
  over constant tables.

`examples/picoenvmon_kt` is the reference: the Kotlin twin of a real
multi-Activity + Service + HTTP-dashboard app, written under these rules. Its
like-for-like cost over the Java app is **+5 % PAPK and +3.8 % parsed class
metadata (+2.5 KB)** with an identical idle allocation signature — the full
measurement table is in the repo at
`docs/designs/kotlin-shim-inventory.md` § 10, and the RAM figures are
summarized in [Runtime limits](/reference/limits/).

## When `contractCheck` fails

Every `kotlin/**` and `java/**` reference a Kotlin app makes is verified at
build time by `:kotlin-shim:contractCheck` (your app's compiled classes are
its input if it is registered as a fixture; the langsuite fixtures cover the
stdlib surface either way). When a new idiom misses:

- **`MISSING` (Direction A)** — the shim has no such member. The report
  prints a paste-ready Java signature. First ask whether the idiom has a
  cheaper spelling (this page's tables); if the member is genuinely worth
  serving, add a demo check to the langsuite apps *first* (unused shim
  members are a build **error**), then the shim member.
- **`UNLISTED` (Direction C)** — the idiom reaches a `java/**` member not in
  `kotlin-shim/jdk-allowlist.tsv`. If pico-jvm serves it, add the printed TSV
  row (a test cross-checks that every owner is actually served); if not, the
  idiom is unsupported — rewrite it.
- A runtime `NoSuchMethod` on a `kotlin/**` or `java/**` name that compiled
  fine means the class fell outside the fixture set — register the app in
  `kotlin-shim/build.gradle.kts`'s `shimFixtures` so the contract check sees
  it.
