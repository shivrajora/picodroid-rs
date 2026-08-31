# Kotlin shim inventory — 2026-08-27

**Goal:** the empirical source of truth for the `kotlin/**` shim and the interpreter work in `docs/designs/kotlin-roadmap-2026-08.md` (Sessions 2–6): what kotlinc 2.1.21 actually emits for a picoenvmon-shaped app, one row per referenced `(owner, name, desc)`, with the fixture file that caused it — plus the answers to the roadmap's go/no-go questions (a)–(m), the strip-prototype numbers, and the `hello-kotlin-on-sim` result.

**Provenance.** Written against the tree at `dd8f5f8`. Produced by `tools/kotlin-survey/` (see its README): Kotlin/KGP **2.1.21** (kotlin-stdlib 2.1.21, KGP's own default, never pinned separately), Gradle 8.10 (root wrapper), JDK 21.0.11, ASM 9.7. Every table below is copied from `tools/kotlin-survey/out/**` — nothing is transcribed by hand; `grep -F` the cited tuple in `out/fixture/tuples.tsv` to find its rows. Regenerate with:

```bash
./gradlew :sdk:compileJava :examples:picoenvmon:compileJava
./gradlew -p tools/kotlin-survey survey && tools/kotlin-survey/hello-sim.sh && tools/kotlin-survey/hello-sim.sh --stripped
```

**Frozen flag string**, as kotlinc received it (`--debug` log of `:fixture:compileKotlin`):

```text
-jvm-target 1.8 -Werror -Xjvm-default=all -Xno-param-assertions -Xno-call-assertions
-Xno-receiver-assertions -Xstring-concat=inline -Xno-source-debug-extension -Xjdk-release=1.8
```

(`-Werror` is `allWarningsAsErrors = true`; `-Xjdk-release=1.8` was added in Session 1 — roadmap AMENDMENT 4; lambdas and SAM conversions are `indy`, the 2.x default, deliberately not set so the census proves it.) `kotlin.jvm.target.validation.mode=error`, `kotlin.incremental=false`.

**Fixture.** `tools/kotlin-survey/fixture/src/main/kotlin/survey/` — `SurveyApp.kt` (Application), `HomeActivity.kt`, `HistoryActivity.kt`, `SensorService.kt`, `Model.kt` (data/enum/sealed/Comparable/default-method interfaces), `RingBuffer.kt`, `Collections.kt`, `Text.kt`, `Registry.kt` (object/companion), and `Probes.kt` — the only file allowed to use the deliberately out-of-scope shapes (`::`, `::class`, `uppercase()`, no-arg `mutableMapOf()`, `toTypedArray()`, `println`, `is MutableList`). The tier tables in § 4 are computed with `source_file != Probes.kt`; Probes rows are in § 4.5. Attribution is by the `SourceFile` attribute, so `Comparisons.kt` in a `source files` column is the stdlib's own file name carried by the inlined `compareBy` lambda class.

## 1. Go/no-go answers

| # | Question | Answer | Evidence (`out/fixture/`) | Consequence |
|---|---|---|---|---|
| (a) | `::foo` → indy or `FunctionReferenceImpl`? | **`FunctionReferenceImpl` subclass** when the reference is a value; **inlined to a direct call** when it is the argument of an inline HOF | `classes.tsv`: `survey/Probes$probeA_functionReference$f$1` super `kotlin/jvm/internal/FunctionReferenceImpl`, interface `kotlin/jvm/functions/Function1`; `refs-all.tsv`: `invokespecial kotlin/jvm/internal/FunctionReferenceImpl.<init>(ILjava/lang/Object;Ljava/lang/Class;Ljava/lang/String;Ljava/lang/String;I)V`. `listOf(1, 2).map(::twice)` leaves only `invokestatic survey/Probes.twice(I)I` | Callable references are supported **only as inline-HOF arguments**; as values they are reflection → out of scope. Contract test rejects `kotlin/jvm/internal/FunctionReferenceImpl`, `PropertyReference*`, `Reflection`, `kotlin/reflect/**` |
| (b) | `String::length`-style refs: impl handle owner `java/**`? | **No indy at all.** As values: `String::length` and `Reading::value` → `PropertyReference1Impl` subclasses (`Probes$probeB_builtinReferences$lenRef$1`, `$valRef$1`, with `ldc_class java/lang/String`); `::Threshold` → `FunctionReferenceImpl` (`$ctorRef$1`). Inside inline `map`: `invokevirtual java/lang/String.length()I`, `new survey/Threshold` | `indy.tsv`: all 16 sites have `impl_owner` = `survey/*`; zero `java/**` impl owners | `try_lambda_dispatch`'s native-target `NoSuchMethod` path (`ops_invoke.rs:170-172`) is unreachable from kotlinc output; no interpreter work. Same rule as (a) |
| (c) | `altMetafactory` / `REF_newInvokeSpecial`? | **`altMetafactory` only for a `Serializable` SAM** (extra arg `Integer(1)` = FLAG_SERIALIZABLE) plus a synthetic `$deserializeLambda$` method referencing `java/lang/invoke/SerializedLambda` (8 tuples). **No `REF_newInvokeSpecial`** and no `REF_invokeSpecial` anywhere: all 16 sites are `6:invokeStatic`, `itf=false` — `this`-capturing lambdas compile to a static `name$lambda$N(Lsurvey/HomeActivity;…)` taking the receiver as capture 0 | `indy.tsv` rows `survey/Probes.probeC_altMetafactory()I` and `$deserializeLambda$`; `summary.md` § Red flags | Session 3 rejects non-`metafactory` BSMs and `REF_newInvokeSpecial` with a named error as planned; rule for apps: **no `java.io.Serializable` SAM interfaces**. javac, by contrast, uses `7:invokeSpecial` for 7 of picoenvmon's 8 lambdas (`out/picoenvmon-java/indy.tsv`) — pico-jvm already handles both |
| (d) | `$WhenMappings` shape | **Emitted, javac-shaped**: `survey/ModelKt$WhenMappings` (783 B, one `<clinit>`), `putstatic $EnumSwitchMapping$0 [I` filled from `invokestatic SensorKind.values()` + `getstatic SensorKind.X` + `invokevirtual ordinal()I`, each inside a `catch_type java/lang/NoSuchFieldError` block — K2 did **not** emit a direct `ordinal()` tableswitch | `refs-all.tsv` rows with `from_class = survey/ModelKt$WhenMappings` | Works today (identical to javac's `$SwitchMap`); `NoSuchFieldError` is only a catch type, never thrown. Costs one registered class (+20 B) per file with an enum `when` |
| (e) | `uppercase()` → `Locale.ROOT`? | **Yes**: `getstatic java/util/Locale.ROOT`, `invokevirtual java/lang/String.toUpperCase(Ljava/util/Locale;)Ljava/lang/String;` (and `toLowerCase`), wrapped in `Intrinsics.checkNotNullExpressionValue` | `tuples.tsv` rows `java/util/Locale ROOT`, `java/lang/String toUpperCase (Ljava/util/Locale;)…` (Probes.kt) | Session 4 Locale item stands: name-only `java/util/Locale` yielding `Null` for `ROOT`, and `toUpperCase/toLowerCase(Locale)` arms that ignore the argument |
| (f) | `mutableMapOf()` → `new LinkedHashMap`? | **Yes** for the no-arg forms: `new java/util/LinkedHashMap` + `<init>()V`; `mutableSetOf()` → `LinkedHashSet`; `hashMapOf()` → `new java/util/HashMap` + `invokevirtual HashMap.size()I`. The vararg forms are calls: `MapsKt.mutableMapOf([Lkotlin/Pair;)Ljava/util/Map;`, `MapsKt.mapOf([Lkotlin/Pair;)…`, `SetsKt.setOf([Ljava/lang/Object;)Ljava/util/Set;` | `tuples.tsv` `java/util/LinkedHashMap <init> ()V` (Probes.kt); `kotlin/collections/MapsKt mutableMapOf ([Lkotlin/Pair;)…` (Collections.kt) | Session 4 aliases confirmed (`LinkedHashMap`/`LinkedHashSet` → HashMap/HashSet dispatchers); the shim's vararg builders may return `HashMap`/`HashSet` directly |
| (g) | `checkcast java/lang/Number` + `intValue()`? | **Yes**: `checkcast java/lang/Number` then `invokevirtual java/lang/Number.intValue()I` for `ints[0] + 1` and `sumOf { it }` over `List<Int>`; `Number.floatValue()F` for `List<Float>` | `tuples.tsv` `java/lang/Number intValue ()I` (Collections.kt, Probes.kt) | Session 3: boxed → `java/lang/Number` in `builtin_super`, plus `Number.intValue/floatValue` (add `longValue/doubleValue` with them) dispatching on boxed receivers. This is on every generic-collection hot path |
| (h) | Data-class `hashCode`/`equals` statics | **Yes**: `Integer.hashCode(I)I`, `Float.hashCode(F)I`, `Long.hashCode(J)I`; `equals` uses `Float.compare(FF)I` for the Float field and `Intrinsics.areEqual` for the String field; a String field's `hashCode` is `invokevirtual java/lang/String.hashCode()I` | `tuples.tsv` rows from Model.kt | Session 3 list trimmed to what is observed: `Integer.hashCode(I)`, `Float.hashCode(F)`, `Long.hashCode(J)`, `Float.compare(FF)`. `Double.*`/`Boolean.hashCode(Z)` only if the proof app has such fields |
| (i) | Which `Intrinsics.*` survive the `-Xno-*` flags? | `checkNotNull(Ljava/lang/Object;)V` (1 site, `!!`), `areEqual(Ljava/lang/Object;Ljava/lang/Object;)Z` (7 sites: `==`, `when` on a String, data-class `equals`), `throwUninitializedPropertyAccessException(Ljava/lang/String;)V` (5 sites — one per `lateinit` read), and **`checkNotNullExpressionValue(Ljava/lang/Object;Ljava/lang/String;)V` (6 sites)**. `checkNotNullParameter` is absent, as predicted; `stringPlus`, `checkNotNull(Object,String)`, `compare`, `throwNpe` were not emitted | `refs-all.tsv` filtered on owner `kotlin/jvm/internal/Intrinsics` | **Refutes the roadmap's "must be absent" rule for `checkNotNullExpressionValue`**: every one of its sites is inside an *inlined stdlib body* (`String.format` → `fmt1`/`fmt`/`HistoryActivity.onCreate`, `FloatArray.copyOf` → `RingBuffer.copy`, `uppercase`/`lowercase` → `Probes.probeE_case`). `-Xno-call-assertions` governs code generated for *our* sources; inline bodies are copied verbatim from the stdlib jar, which was compiled with assertions on. Tier 0 gains it; the contract rule becomes "`checkNotNullParameter` must be absent" |
| (j) | `DefaultConstructorMarker` descriptor-only? | **Yes**: `descriptor_only` from six files (`$default` constructors/bridges), never `new`/`checkcast`/`ldc` | `tuples.tsv` `kotlin/jvm/internal/DefaultConstructorMarker … descriptor_only … load_bearing=false` | No class file needed — drop from tier 0 (shipping it would only cost 20 B of registration) |
| (k) | `kotlin/jvm/internal/Lambda` subclasses? | **Never referenced.** Plain `() -> Unit` values → indy with SAM `kotlin/jvm/functions/Function0`, impl returning `Lkotlin/Unit;`; the `crossinline` object expression → an ordinary class `Probes$probeK_lambdaClasses$$inlined$runLater$1` (super `java/lang/Object`, interface `java/lang/Runnable`) | `indy.tsv` rows `survey/Probes.probeK_lambdaClasses()V`; `classes.tsv` | Tier 0 drops `Lambda` and `FunctionBase`; keeps `Function0..N` interfaces and `Unit.INSTANCE` |
| (l) | `EnumEntriesKt.enumEntries` in every enum `<clinit>`? | **Yes**: `invokestatic kotlin/enums/EnumEntriesKt.enumEntries([Ljava/lang/Enum;)Lkotlin/enums/EnumEntries;` from `survey/SensorKind.<clinit>`; `kotlin/enums/EnumEntries` itself is descriptor/signature-only, but `entries.firstOrNull { }` iterates it through `invokeinterface java/lang/Iterable.iterator()`. `valueOf(String)` → `invokestatic java/lang/Enum.valueOf(Ljava/lang/Class;Ljava/lang/String;)Ljava/lang/Enum;`; `values()` → `invokevirtual java/lang/Object.clone()` on `$VALUES` (javac-identical) | `tuples.tsv` rows from Model.kt | Tier 0 trio stands; the shim's `EnumEntriesList` must implement `java/util/List` **and** `iterator()`, and `checkcast java/util/List` on it needs Session 3's transitive superinterface walk. `Enum.valueOf(Class,String)` is roadmap risk 26 — decide in Session 3 |
| (m) | `toTypedArray()` → `Collection.toArray`? | **Yes**: `anewarray java/lang/String` (length 0) + `invokeinterface java/util/Collection.toArray([Ljava/lang/Object;)[Ljava/lang/Object;` + `checkcast [Ljava/lang/String;` | `tuples.tsv` `java/util/Collection toArray …` (Probes.kt) | Session 4 optional item; otherwise a documented limitation ("build the array with `Array(n) { }`") |

## 2. `hello-kotlin-on-sim` — PASSED, zero JVM changes

`tools/kotlin-survey/hello/src/main/kotlin/hellokt/HelloKt.kt` (one class extending `picodroid.app.Application`, `Log.i("HelloKt", "hi from kotlin ${21 * 2}")`), packed by `cargo run -p papk-pack -- --application hellokt/HelloKt --package-name hellokt --version 1.0 --framework-map-version 0.0.0 --classes-dir … --output out/hellokt.papk`, run with `PICODROID_SIM_HEADLESS=1 ./scripts/sim.sh --apk out/hellokt.papk` (`sim.sh --apk` is roadmap AMENDMENT 2).

| Variant | Class bytes | CP entries | PAPK | Sim | Log |
|---|---|---|---|---|---|
| raw kotlinc output | 723 | 42 | 901 B | exit 0, `lazy-load: 2/138 classes parsed` | `[HelloKt] hi from kotlin 42` |
| ASM-stripped (§ 6) | 380 | 24 | 558 B | exit 0, `lazy-load: 2/138 classes parsed` | `[HelloKt] hi from kotlin 42` |

`out/hello/refs.tsv` has exactly one `kotlin/**` row — `annotation_type kotlin/Metadata` (a Utf8 inside `RuntimeVisibleAnnotations`, never a `CONSTANT_Class`), so "zero stdlib" is a property of the class file, not of the compile classpath. Class version 52.0; CP tags 17/19/20 absent. The stripped variant proves `ClassWriter(0)` output without `StackMapTable` loads and runs on pico-jvm — Session 2's pipeline assumption.

## 3. Class census digest (`out/fixture/classes.tsv`, `summary.md`)

| Metric | Fixture (Kotlin, 10 files) | picoenvmon (Java, 22 files, baseline) |
|---|---|---|
| classes / bytes / CP entries / `CONSTANT_Class` | 46 / 115,005 / 5,322 / 408 | 24 / — / — / — |
| methods | 221 (22 synthetic, 3 bridge, 5 `$default`, 1 `ACC_SYNCHRONIZED`, 2 interface defaults) | — |
| classes with `@Metadata` | 46 | 0 |
| `*$Companion` / `*$WhenMappings` / `*$DefaultImpls` / anonymous-or-local (`$<n>`) | 5 / 1 / **0** / 11 | 0 / 0 / 0 / 4 |
| distinct external tuples (load-bearing) / owners / indy sites | 200 (189) / 69 / 16 | 35 (34) / 13 / 8 |
| CP tags 15 / 16 / 17 / 18 / 19 / 20 | present / present / **0** / 16 / **0** / **0** | — |
| bytes in attributes pico-jvm skips: RVA / RIA / RPA / Signature / StackMapTable / LVT / InnerClasses / EnclosingMethod / SDE | 3,909 / 1,144 / 529 / 160 / 2,830 / 7,066 / 704 / 110 / **2,695** | — |

Observations that shape Sessions 2–7:

- **`-Xjvm-default=all` works**: `Describable`/`Tagged` carry one non-abstract method each, no `$DefaultImpls` class exists, `Both.describe()` calls `super<I>.f()` via `invokespecial survey/Describable.describe()…` with `itf=true`, and `OnlyDefault` (no override) leaves the resolution to the runtime walk — Session 4's `find_method_walking` item.
- **`-Xno-source-debug-extension` does not remove the `SourceDebugExtension` attribute** (2,695 B across 9 classes with inlined stdlib bodies); it only suppresses the `@kotlin.jvm.internal.SourceDebugExtension` annotation copy of the SMAP (confirmed absent). Dropping the attribute is the strip's job (§ 6).
- **Every `for`-range form is intrinsified**: `0 until n`, `indices`, `n - 1 downTo 0 step 2`, `1..10`, `withIndex()`, and `idx in 0..lastIndex` outside a loop emit no `IntRange`/`IntProgression`/`IntProgressionIterator`/`RangesKt.step`/`IndexedValue` at all — the only survivor is `kotlin/internal/ProgressionUtilKt.getProgressionLastElement(III)I` for the stepped loop, plus `kotlin/ranges/RangesKt.coerceIn(III)I`. (Non-loop range *values* — `val r = 0..n` — were not exercised and would need the classes.)
- **`when (s: String)` with two branches** compiles to an `Intrinsics.areEqual` chain (2 sites in `HomeActivity.onCreate`), not a `String.hashCode()` switch; javac-style hash switches presumably appear with more branches.
- **`String.format(...)`** (inline `String.Companion.format`) emits `getstatic kotlin/jvm/internal/StringCompanionObject.INSTANCE` + `pop` before `invokestatic java/lang/String.format`, plus a `checkNotNullExpressionValue` — a one-field shim class is required by two fixture files.
- **`FloatArray.fill()` is not inline** (`ArraysKt.fill$default([FFIIILjava/lang/Object;)V`) while `copyOf()` is (`java/util/Arrays.copyOf([FI)[F`); `lastIndex` is a real call (`ArraysKt.getLastIndex([F)I`).
- **`println` is inline to `System.out`**: `getstatic java/lang/System.out` + `invokevirtual java/io/PrintStream.println` — `kotlin/io/ConsoleKt` never appears, so the tier-3 `ConsoleKt` shim is moot; apps must use `Log` (documented, and `System.out` is absent from pico-jvm anyway).
- **Codegen growth is real** (Session 7 frugality rules): ~700 LOC of Kotlin → 46 classes / 221 methods, versus 24 classes for picoenvmon's 2.5 kLOC of Java. A `data class` is 10–12 methods (`Named` 10, `Reading` 12); each `companion object` is a class plus a `<init>(DefaultConstructorMarker)`; an enum with a companion and a `when` is three classes; `lazy` costs a `Function0` indy plus an interface call per read. Parsed-metadata projection with the roadmap's model (88 B/class + 32 B/method + 5 B/CP entry): **~37.7 KB before strip, ~30.4 KB after** (CP 5,322 → 3,859) if every class were parsed.

## 4. Referenced tuples by tier (fixture minus `Probes.kt`)

`load` = load-bearing (needs a class file or dispatch arm in pico-jvm). "Roadmap" says whether `kotlin-roadmap-2026-08.md` § Shim inventory listed the row: ✓ present with the same descriptor, ✗ absent, ≠ descriptor differs.

### 4.1 Tier 0 — core

| owner | name | desc | kinds | fixture files | Roadmap |
|---|---|---|---|---|---|
| `kotlin/jvm/internal/Intrinsics` | `checkNotNull` | `(Ljava/lang/Object;)V` | invokestatic | HomeActivity | ✓ |
| `kotlin/jvm/internal/Intrinsics` | `checkNotNullExpressionValue` | `(Ljava/lang/Object;Ljava/lang/String;)V` | invokestatic | HistoryActivity, RingBuffer, Text (+Probes) | ✗ **(listed as must-be-absent; see (i))** |
| `kotlin/jvm/internal/Intrinsics` | `areEqual` | `(Ljava/lang/Object;Ljava/lang/Object;)Z` | invokestatic | HomeActivity, Model | ✓ |
| `kotlin/jvm/internal/Intrinsics` | `throwUninitializedPropertyAccessException` | `(Ljava/lang/String;)V` | invokestatic | HomeActivity | ✓ |
| `kotlin/jvm/internal/StringCompanionObject` | `INSTANCE` | `Lkotlin/jvm/internal/StringCompanionObject;` | getstatic | HistoryActivity, Text | ✗ |
| `kotlin/Unit` | `INSTANCE` | `Lkotlin/Unit;` | getstatic | HistoryActivity (+Probes) | ✓ |
| `kotlin/jvm/functions/Function0` | — | (SAM of `lazy { }` indy; `invoke()Ljava/lang/Object;` invokeinterface from Probes) | indy_sam, invokeinterface | HomeActivity | ✓ |
| `kotlin/jvm/functions/Function1` | — | (SAM of the `joinToString` transform indy; `invoke(Ljava/lang/Object;)Ljava/lang/Object;` from Probes) | indy_sam, invokeinterface, checkcast | Collections | ✓ |
| `kotlin/Lazy` | `getValue` | `()Ljava/lang/Object;` | invokeinterface | HomeActivity | ✓ |
| `kotlin/LazyKt` | `lazy` | `(Lkotlin/jvm/functions/Function0;)Lkotlin/Lazy;` | invokestatic | HomeActivity | ✓ |
| `kotlin/Pair` | — | `anewarray` for `mapOf(a to b, …)` | anewarray | Collections | ✓ |
| `kotlin/Pair` | `component1` / `component2` | `()Ljava/lang/Object;` | invokevirtual | Collections | ✓ |
| `kotlin/TuplesKt` | `to` | `(Ljava/lang/Object;Ljava/lang/Object;)Lkotlin/Pair;` | invokestatic | Collections | ✓ |
| `kotlin/enums/EnumEntriesKt` | `enumEntries` | `([Ljava/lang/Enum;)Lkotlin/enums/EnumEntries;` | invokestatic | Model | ✓ |
| `kotlin/enums/EnumEntries` | — | descriptor/signature only, but iterated via `Iterable.iterator()` | descriptor_only | Model | ✓ (must implement `java/util/List` + `iterator()`) |
| `kotlin/NoWhenBranchMatchedException` | `<init>` | `()V` | new, invokespecial | Model | ✓ |
| `kotlin/NotImplementedError` | `<init>` | `(Ljava/lang/String;)V` | new, invokespecial | Text | ✓ |
| `kotlin/jvm/internal/DefaultConstructorMarker` | — | descriptor only | descriptor_only | 6 files | ✓ but **no class file needed** |

Roadmap tier-0 rows **not observed** in the fixture (keep as "expected, unproven" until the proof app runs the contract test): `Intrinsics.checkNotNull(Object,String)`, `compare(II/JJ)`, `stringPlus` (`null-String + "x"` compiled to a `StringBuilder` chain), `throwNpe`/`throwJavaNpe`; `kotlin/Function` marker (no `interface` kind seen); `FunctionBase`; `Lambda` (see (k)); `Ref$ObjectRef/IntRef/…` (the fixture captures no mutated local — the proof app will); `Pair.<init>`/`getFirst`/`getSecond`; `Triple`; `LazyThreadSafetyMode`; `UninitializedPropertyAccessException`, `KotlinNothingValueException`, `TypeCastException`.

### 4.2 Tier 1 — collections

| owner | name | desc | fixture files | Roadmap |
|---|---|---|---|---|
| `kotlin/collections/CollectionsKt` | `collectionSizeOrDefault` | `(Ljava/lang/Iterable;I)I` | Collections (+Probes) | ✓ |
| `kotlin/collections/CollectionsKt` | `distinct` | `(Ljava/lang/Iterable;)Ljava/util/List;` | Collections | ✓ |
| `kotlin/collections/CollectionsKt` | `first` | `(Ljava/util/List;)Ljava/lang/Object;` | Collections | ✓ |
| `kotlin/collections/CollectionsKt` | `joinToString$default` | `(Ljava/lang/Iterable;Ljava/lang/CharSequence;Ljava/lang/CharSequence;Ljava/lang/CharSequence;ILjava/lang/CharSequence;Lkotlin/jvm/functions/Function1;ILjava/lang/Object;)Ljava/lang/String;` | Collections | ✓ (only the `$default` bridge is called — both sites omit some defaults) |
| `kotlin/collections/CollectionsKt` | `listOf` | `(Ljava/lang/Object;)Ljava/util/List;` | Collections (+Probes) | ✓ |
| `kotlin/collections/CollectionsKt` | `listOf` | `([Ljava/lang/Object;)Ljava/util/List;` | Collections (+Probes) | ✓ |
| `kotlin/collections/CollectionsKt` | `maxOrNull` | `(Ljava/lang/Iterable;)Ljava/lang/Float;` | Collections | ✓ (the return-type-only overload: `@ShimName` confirmed necessary) |
| `kotlin/collections/CollectionsKt` | `sortedWith` | `(Ljava/lang/Iterable;Ljava/util/Comparator;)Ljava/util/List;` | Collections | ✓ |
| `kotlin/collections/CollectionsKt` | `take` | `(Ljava/lang/Iterable;I)Ljava/util/List;` | Collections | ✓ |
| `kotlin/collections/CollectionsKt` | `throwCountOverflow` / `throwIndexOverflow` | `()V` | Collections | ✓ |
| `kotlin/collections/CollectionsKt` | `toIntArray` | `(Ljava/util/Collection;)[I` | Collections | ✓ (listed under ArraysKt; it is on CollectionsKt) |
| `kotlin/collections/CollectionsKt` | `zip` | `(Ljava/lang/Iterable;Ljava/lang/Iterable;)Ljava/util/List;` | Collections | ✓ |
| `kotlin/collections/CollectionsKt` | `sumOfInt` | `(Ljava/lang/Iterable;)I` | Probes only (`.sum()` on `List<Int>`) | ✓ |
| `kotlin/collections/MapsKt` | `mapOf` | `([Lkotlin/Pair;)Ljava/util/Map;` | Collections | ✓ |
| `kotlin/collections/MapsKt` | `mutableMapOf` | `([Lkotlin/Pair;)Ljava/util/Map;` | Collections | ✓ |
| `kotlin/collections/SetsKt` | `setOf` | `([Ljava/lang/Object;)Ljava/util/Set;` | Collections | ✓ |
| `kotlin/collections/ArraysKt` | `average` | `([F)D` | RingBuffer | ✓ |
| `kotlin/collections/ArraysKt` | `fill$default` | `([FFIIILjava/lang/Object;)V` and `([IIIIILjava/lang/Object;)V` | RingBuffer | ✗ (roadmap: "inline → Arrays builtin" — **refuted**) |
| `kotlin/collections/ArraysKt` | `getLastIndex` | `([F)I` | HistoryActivity, RingBuffer | ✓ (`([I)I` listed; `[F` seen) |
| `kotlin/collections/ArraysKt` | `maxOrNull` | `([F)Ljava/lang/Float;` | RingBuffer | ✓ |
| `kotlin/collections/ArraysKt` | `sum` | `([F)F` and `([I)I` | RingBuffer, SensorService | ✓ |

Inline HOFs confirmed to leave **no** `kotlin/**` call except the helpers above: `map`, `filter`, `forEach`, `forEachIndexed`, `any`, `count`, `sumOf`, `sortedBy` (→ `sortedWith` + an inlined `compareBy` Comparator class), `firstOrNull { }`, `withIndex()` in `for`, `getOrPut`, `isEmpty`, `x in set`/`k in map`, `map.forEach { (k, v) -> }`, `for ((k, v) in map)`, `Array(n) { }`, `copyOf`, `minOf`. What they do emit is `java/**` (§ 5).

### 4.3 Tier 2 — strings, chars, ranges, math, comparisons

| owner | name | desc | fixture files | Roadmap |
|---|---|---|---|---|
| `kotlin/text/StringsKt` | `contains` | `(Ljava/lang/CharSequence;Ljava/lang/CharSequence;Z)Z` | Text | ✓ |
| `kotlin/text/StringsKt` | `firstOrNull` | `(Ljava/lang/CharSequence;)Ljava/lang/Character;` | Text | ✗ (then `Character.charValue()C`) |
| `kotlin/text/StringsKt` | `isBlank` | `(Ljava/lang/CharSequence;)Z` | Text | ✓ |
| `kotlin/text/StringsKt` | `padStart` | `(Ljava/lang/String;IC)Ljava/lang/String;` | Text | ✓ |
| `kotlin/text/StringsKt` | `split$default` | `(Ljava/lang/CharSequence;[Ljava/lang/String;ZIILjava/lang/Object;)Ljava/util/List;` | Text | ✓ |
| `kotlin/text/StringsKt` | `startsWith$default` | `(Ljava/lang/String;Ljava/lang/String;ZILjava/lang/Object;)Z` | Text | ✓ |
| `kotlin/text/StringsKt` | `substringBefore$default` | `(Ljava/lang/String;CLjava/lang/String;ILjava/lang/Object;)Ljava/lang/String;` | Text | ≠ (`char` delimiter overload, not `String`) |
| `kotlin/text/StringsKt` | `toIntOrNull` | `(Ljava/lang/String;)Ljava/lang/Integer;` | Text | ✓ |
| `kotlin/text/StringsKt` | `trim` | `(Ljava/lang/CharSequence;)Ljava/lang/CharSequence;` | Text | ✓ (followed by `checkcast java/lang/String`) |
| `kotlin/text/CharsKt` | — | **nothing**: `isDigit()` → `Character.isDigit(C)Z`, `uppercaseChar()` → `Character.toUpperCase(C)C`, `code`/`c - '0'`/`in 'a'..'z'` intrinsic | Text | ✓ (roadmap had `isWhitespace`/`digitToInt` — unexercised) |
| `kotlin/ranges/RangesKt` | `coerceIn` | `(III)I` | HistoryActivity | ✓ |
| `kotlin/internal/ProgressionUtilKt` | `getProgressionLastElement` | `(III)I` | HistoryActivity (`downTo … step 2`) | ✗ (replaces `IntProgression`/`RangesKt.step`, never referenced) |
| `kotlin/math/MathKt` | `roundToInt` | `(F)I` | Text | ✓ |
| `kotlin/comparisons/ComparisonsKt` | `compareValues` | `(Ljava/lang/Comparable;Ljava/lang/Comparable;)I` | `Comparisons.kt` (the inlined `compareBy` lambda class from Collections.kt) | ✓ |

Inline to `java/lang/Math`: `abs(F)F`, `sqrt(D)D`, `max(FF)F`, `pow(DD)D`, `floor(D)D`, `min(II)I`. Inline to `java/lang/String`: `format`, `toInt()` → `Integer.parseInt`, `isNotEmpty()` → `CharSequence.length()`.

### 4.4 Tier 3 — nothing observed

`kotlin/io/ConsoleKt` is never referenced (`println` is inline to `System.out`); `kotlin/random`, `Triple`, `IndexedValue`, `kotlin/jvm/internal/markers/*` did not appear.

### 4.5 `Probes.kt` — out-of-scope shapes (never shimmed; contract test rejects with a docs pointer)

| owner | name / desc | Probe | Verdict |
|---|---|---|---|
| `kotlin/jvm/internal/FunctionReferenceImpl` | `<init>(ILjava/lang/Class;Ljava/lang/String;Ljava/lang/String;I)V`, `(ILjava/lang/Object;…)V`; `super` of `$f$1`, `$ctorRef$1` | (a), (b) | reflection — reject |
| `kotlin/jvm/internal/PropertyReference1Impl` | `<init>(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/String;I)V`; `super` of `$lenRef$1`, `$valRef$1` | (b) | reflection — reject |
| `kotlin/jvm/internal/Reflection` | `getOrCreateKotlinClass(Ljava/lang/Class;)Lkotlin/reflect/KClass;` | `Reading::class` | reject |
| `kotlin/reflect/KClass` | `getSimpleName()Ljava/lang/String;` | `::class.simpleName` | reject |
| `kotlin/jvm/internal/TypeIntrinsics` | `isMutableList(Ljava/lang/Object;)Z` | `x is MutableList<*>` | reject (`is List<*>` is a plain `instanceof`) |
| `java/lang/invoke/LambdaMetafactory` | `altMetafactory` + `java/lang/invoke/SerializedLambda.*` | (c) | Session 3 named error; no `Serializable` SAMs |
| `java/util/Locale` / `java/lang/String` | `ROOT`; `toUpperCase/toLowerCase(Ljava/util/Locale;)` | (e) | Session 4 |
| `java/util/LinkedHashMap`, `LinkedHashSet`, `HashMap.size()I` | `<init>()V` | (f) | Session 4 aliases |
| `java/util/Collection` | `toArray([Ljava/lang/Object;)[Ljava/lang/Object;` | (m) | Session 4 optional |
| `java/lang/System` / `java/io/PrintStream` | `out`; `println(I)V`, `println(Ljava/lang/Object;)V` | `println` | use `Log` |

## 5. `java/**` references

### 5.1 Java picoenvmon baseline (`out/picoenvmon-java/tuples.tsv`, 24 classes) — the Session 5 allowlist seed

| owner | members (kinds) |
|---|---|
| `java/io/IOException` | `<init>(Ljava/lang/String;)V`, `getMessage()Ljava/lang/String;`; `new`, `catch_type`, `exceptions_attr` |
| `java/lang/Class` | `anewarray`, descriptor/signature only |
| `java/lang/Integer` | `toString(I)Ljava/lang/String;`, `valueOf(I)Ljava/lang/Integer;` |
| `java/lang/Math` | `min(II)I` |
| `java/lang/Object` | `<init>()V`; `super`, `anewarray` |
| `java/lang/Runnable` | `interface`; indy SAM (`run(Lpicoenvmon/net/NetworkManager;)Ljava/lang/Runnable;`) |
| `java/lang/RuntimeException`, `java/lang/Throwable`, `java/net/SocketTimeoutException` | `catch_type` |
| `java/lang/String` | `<init>([BII)V`, `equals(Ljava/lang/Object;)Z`, `format(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;`, `getBytes()[B`, `isEmpty()Z`, `trim()Ljava/lang/String;`; `new`, `anewarray` |
| `java/lang/StringBuilder` | `<init>()V`, `append(C/F/I/J/Ljava/lang/Object;/Ljava/lang/String;/Z)`, `toString()` |
| `java/lang/System` | `arraycopy(Ljava/lang/Object;ILjava/lang/Object;II)V`, `currentTimeMillis()J` |
| `java/lang/invoke/LambdaMetafactory` | `metafactory` (8 sites: 7 × `REF_invokeSpecial` this-capturing, 1 × `REF_invokeStatic`) |
| `java/lang/invoke/MethodHandles$Lookup` | `inner_class` only (stripped by the InnerClasses drop) |

### 5.2 Load-bearing `java/**` tuples the Kotlin fixture needs (non-Probes) that the Java baseline never touches — the Sessions 3/4 backlog

Grouped; each line is verbatim from `comm -13` over the two `tuples.tsv` files. Whether an existing builtin arm already covers a row is for the Session 5 allowlist ↔ `BUILTIN_CLASS_NAMES`/`builtin_super`/`builtin_interfaces` test to prove; rows marked **gap** are known missing from `jvm/src/native` today.

- **Interface/abstract types as `checkcast`/`instanceof`/`interface` targets** (Session 3 `builtin_interfaces`, the verified-fatal `checkcast` path): `java/lang/Iterable`, `java/util/Collection` (also `instanceof`), `java/util/List`, `java/util/Set`, `java/util/Map`, `java/util/Map$Entry`, `java/lang/CharSequence`, `java/lang/Comparable`, `java/util/Comparator`, `java/lang/Number`, `java/lang/Enum` (`super`) — **gap**.
- **Interface calls dispatched by runtime class**: `java/lang/Iterable.iterator()Ljava/util/Iterator;`; `java/util/Iterator.hasNext()Z`/`next()Ljava/lang/Object;`; `java/util/Collection.add/isEmpty/size`; `java/util/List.get(I)/isEmpty/size`; `java/util/Set.contains/isEmpty/iterator/size`; `java/util/Map.containsKey/entrySet/get/keySet/put/size/values`; `java/util/Map$Entry.getKey/getValue` (**gap**: `entrySet`/`Map$Entry`, Session 4); `java/lang/CharSequence.length()I` on a String (**gap**: String receivers dispatch on the CP class, Session 3).
- **Boxed / numeric statics and instance methods**: `java/lang/Float.compare(FF)I`, `Float.hashCode(F)I`, `Float.valueOf(F)`; `java/lang/Integer.hashCode(I)I`, `Integer.intValue()I`, `Integer.parseInt(Ljava/lang/String;)I`; `java/lang/Long.hashCode(J)I`, `Long.valueOf(J)`; `java/lang/Number.intValue()I` (**gap**); `java/lang/Character.charValue()C`, `Character.isDigit(C)Z`, `Character.toUpperCase(C)C`; `java/lang/String.hashCode()I`, `String.length()I`, `String.valueOf(F)Ljava/lang/String;`; `java/lang/StringBuilder.append(D)`, `StringBuilder.length()I`. The `hashCode(F/I/J)`/`compare(FF)` statics are **gaps** (Session 3 data-class set).
- **`java/lang/Object`**: `clone()Ljava/lang/Object;` (enum `values()` — already exercised by Java enums), `toString()Ljava/lang/String;` on an `ObjectRef` (data-class `toString` templates; **gap** = Session 4 trampoline / identity fallback).
- **`java/lang/Math`**: `abs(F)F`, `floor(D)D`, `max(FF)F`, `pow(DD)D`, `sqrt(D)D` (all in the SDK's `Math.java` surface; confirm the `F` overloads are dispatched).
- **Collections/arrays**: `java/util/ArrayList.<init>()V` and `<init>(I)V`; `java/util/Arrays.copyOf([FI)[F`, `copyOf([Ljava/lang/Object;I)[Ljava/lang/Object;` (spread operator); `java/util/Comparator` as an indy SAM (`compare()Ljava/util/Comparator;`).
- **Exceptions**: `java/lang/IllegalArgumentException.<init>(Ljava/lang/String;)V` (`require`), `java/lang/IllegalStateException.<init>(Ljava/lang/String;)V` (`check`/`error`), `java/lang/NumberFormatException.getMessage()`, `java/lang/NoSuchFieldError` (`catch_type` in `$WhenMappings` — never thrown), `java/lang/Enum.<init>(Ljava/lang/String;I)V`, `Enum.valueOf(Ljava/lang/Class;Ljava/lang/String;)Ljava/lang/Enum;` (**gap**, risk 26).
- **Runnable SAM shapes**: `run()Ljava/lang/Runnable;` (no capture) and `run(Lsurvey/HomeActivity;)Ljava/lang/Runnable;` (receiver capture) — both `REF_invokeStatic`, unlike javac's `REF_invokeSpecial`.

## 6. Strip prototype (`out/strip/fixture/`, `out/fixture-stripped/`)

ASM `ClassReader(SKIP_FRAMES)` → filtering visitor → `ClassWriter(0)` with no reader argument (constant pool rebuilt). Dropped: `Runtime{Visible,Invisible}{,Parameter,Type}Annotations`, `AnnotationDefault`, `Signature`, `InnerClasses`, `EnclosingMethod`, `NestHost/Members`, `PermittedSubclasses`, `MethodParameters`, `LocalVariable{,Type}Table`, `StackMapTable`, `SourceDebugExtension`, unknown attributes. Kept: `Code`, `LineNumberTable`, `SourceFile`, `Exceptions`, `BootstrapMethods`, `ConstantValue`.

| Metric | before | after | saved |
|---|---|---|---|
| bytes (46 classes) | 115,005 | 69,278 | **45,727 (39.8 %)** |
| constant-pool entries | 5,322 | 3,859 | **1,463 (27.5 %)** |
| hello class | 723 | 380 | 343 (47.4 %) |

Where the 45,727 bytes came from: attributes 19,147 B (LVT 7,066; RVA 3,909 — the `@Metadata` protobuf; StackMapTable 2,830; SDE 2,695; RIA 1,144; InnerClasses 704; RPA 529; Signature 160; EnclosingMethod 110) and **26,580 B of constant-pool entries** referenced only by those attributes (the `@Metadata` `d1`/`d2` strings, LVT names/descriptors, `Signature` strings, `MethodHandles$Lookup`-style `InnerClasses` residue).

**Equivalence proof**: re-dumping the stripped classes and diffing every reference row (kind, owner, name, desc, from_class, from_member) against the original leaves the load-bearing set **byte-identical**; the only rows that disappear are `annotation_type` (186), `inner_class` (48), `signature_only` (35) and `enclosing_method` (8) — all non-load-bearing by construction. The stripped hello class runs on the sim (§ 2). Two `--rerun-tasks` runs of the whole survey produce 81 byte-identical output files.

Notes for Session 2: the output has no `StackMapTable`, so a HotSpot JVM would refuse to load it — irrelevant for pico-jvm (never verifies; `parse.rs` skips the attribute by length) and the reason the strip must never be "fixed" with `COMPUTE_FRAMES` (which would need a class hierarchy the host JVM does not have for `picodroid/**`). Projected `picoenvmon_kt` effect: ~40 % of PAPK class bytes and ~27 % of CP entries (≈ 5 B each of parsed metadata) before any pruning/shaking.

## 7. Roadmap risks 1–29, status after the survey

| # | Status | Evidence |
|---|---|---|
| 1 | confirmed | `!!` → `Intrinsics.checkNotNull(Object)`; `as?` on `IBinder` needs no NPE |
| 2 | confirmed (static) | `checkcast java/util/List`/`Collection`/`Map`/`Map$Entry`/`Iterable`/`Number`/`CharSequence`/`Comparable`/`Comparator` all present in non-Probes code |
| 3 | confirmed (static) | `invokeinterface java/lang/CharSequence.length()I` on a String (Text.kt); `Intrinsics.areEqual` on Strings (HomeActivity `when`) |
| 4 | confirmed | (f) |
| 5 | confirmed | (e) |
| 6 | confirmed | `Map.entrySet()` + `Map$Entry.getKey/getValue` from `for ((k, v) in map)` and `map.forEach { (k, v) -> }` |
| 7 | confirmed | `StringBuilder.append(Ljava/lang/Object;)` in 5 files (data-class, `Int?`, `Float?` template parts); `Any?.toString()` not exercised separately |
| 8 | confirmed | (l) |
| 9 | **refuted** for small `when`s | two-branch `when(String)` → `areEqual` chain; `$WhenMappings` uses javac's try/catch shape |
| 10 | confirmed | 5 `$Companion` classes; `Config.<clinit>` creates `DEFAULT` after the companion |
| 11 | confirmed | Unit lambdas' impl methods return `Lkotlin/Unit;`; void SAMs return `V` |
| 12 | **partly refuted** | no `REF_invokeSpecial`/`REF_invokeVirtual` impl handles — kotlinc always emits a static `$lambda$N` with the receiver as capture 0; no `REF_newInvokeSpecial`; builtin method refs never reach indy (b) |
| 13 | confirmed | `Integer.valueOf(I)`, `Float.valueOf(F)`, `Long.valueOf(J)` boxing |
| 14 | confirmed | `arrayOf` → `anewarray`; `toIntArray` → `CollectionsKt.toIntArray(Collection)[I`; `toTypedArray` → `Collection.toArray` (m) |
| 15 | confirmed | (h) |
| 16 | confirmed | `sortedBy` → `sortedWith` + inlined `compareBy` class calling `ComparisonsKt.compareValues` |
| 17 | untested | no `first()` on an empty list is executed by a survey; shim-side |
| 18 | confirmed (shape) | `OnlyDefault` relies on runtime default-method resolution; no marker interfaces appeared |
| 19 | confirmed | `-Xjvm-default=all` accepted by 2.1.21; no `$DefaultImpls` |
| 20 | confirmed | class version 52, `StringBuilder` concat, no condy (CP tag 17 = 0) |
| 21 | confirmed | `SensorService.latest()` is `ACC_SYNCHRONIZED`; `synchronized(lock) { }` is inline |
| 22 | untested | shrink lane not exercised in Session 1 |
| 23 | confirmed | no HIL row in Session 1 |
| 24 | confirmed | § 3 codegen growth |
| 25 | untested | runtime behaviour |
| 26 | confirmed | `Enum.valueOf(Class,String)` emitted for `SensorKind.valueOf(s)` |
| 27 | not applicable | standalone build; buildSrc untouched (Session 2) |
| 28 | confirmed | fixture compiles against the JDK's `java.util.List` with the SDK classes as `compileOnly` files |
| 29 | confirmed | first survey run downloaded KGP/compiler/stdlib/ASM; README documents `--offline` for later runs |

## 8. Deltas vs the roadmap's best-effort inventory

- **Add**: `Intrinsics.checkNotNullExpressionValue(Ljava/lang/Object;Ljava/lang/String;)V` (tier 0, from inlined stdlib bodies); `kotlin/jvm/internal/StringCompanionObject.INSTANCE` (tier 0, `String.format`); `kotlin/internal/ProgressionUtilKt.getProgressionLastElement(III)I` (tier 2); `ArraysKt.fill$default([FFIIILjava/lang/Object;)V` / `([IIIIILjava/lang/Object;)V` (tier 1 — `fill` is not inline); `StringsKt.firstOrNull(Ljava/lang/CharSequence;)Ljava/lang/Character;` and `substringBefore$default` with a `C` delimiter (tier 2).
- **Drop**: `kotlin/jvm/internal/Lambda` and `FunctionBase` (never referenced with indy lambdas); `DefaultConstructorMarker` as a class file (descriptor-only); `IntRange`, `IntRange$Companion`, `IntProgression`, `IntProgression$Companion`, `IntProgressionIterator`, `RangesKt.step/until/downTo/reversed` (every `for`-range form is intrinsified — keep only if the proof app uses a range as a value); `kotlin/io/ConsoleKt` (`println` is inline to `System.out`).
- **Descriptor corrections**: `toIntArray` lives on `CollectionsKt` with `(Ljava/util/Collection;)[I`; `joinToString` is reached only through `joinToString$default(…ILjava/lang/Object;)…`; `maxOrNull(Ljava/lang/Iterable;)Ljava/lang/Float;` is the Float return-type overload (so `@ShimName` is needed exactly as planned); `getLastIndex([F)I` (not only `[I`).
- **Contract-rule correction**: only `checkNotNullParameter` must be absent; `checkNotNullExpressionValue` is legitimate.
- **Shape corrections**: `$WhenMappings` is javac-shaped (try/catch `NoSuchFieldError`); small `when(String)` is an `areEqual` chain; kotlinc lambdas are always `REF_invokeStatic` (javac's are mostly `REF_invokeSpecial`) — both already handled by `op_invokedynamic`, which ignores `ref_kind`.

## 9. Open questions for Sessions 2 and 3

1. `Ref$IntRef`/`ObjectRef` (captured mutated locals), `Double`/`Boolean` data-class fields, `Triple`, range *values* (`val r = 0..n`, `r.contains`), `when(String)` with many branches, `String?.plus` → `stringPlus`, and `first()`/`Iterator.next()` past the end were not exercised; the `picoenvmon_kt` port (Session 7) is the second contract-test fixture and will settle them. **Session 5 (2026-08-28) settled most of these with `examples/langsuite_kt`:** `Ref$IntRef/ObjectRef/LongRef/FloatRef/BooleanRef` are exercised (the shim ships all nine boxes; unused ones are pruned per app); `Double`/`Boolean`/`Char`/nullable data-class fields work (`Character.hashCode(C)` is served); `when(String)` with 8 branches compiles to a `hashCode` switch and works; `String? + x` compiles to a `StringBuilder` chain — **`Intrinsics.stringPlus` is never emitted by 2.1 and was dropped from the shim**, as were `Intrinsics.compare(II/JJ)` (primitive `compareTo` is intrinsified) and `throwNpe`/`throwJavaNpe`. Two shapes the fixture could not show (every lambda was inline or a primitive-parameter SAM) turned out to need the JVM: `LambdaMetafactory`'s boxing adaptation for `(Int) -> Int`-style lambda values, and `Object.clone()` on `$VALUES` in `values()` — roadmap AMENDMENT 12. `TODO()` calls `NotImplementedError`'s default-argument constructor `(String,int,DefaultConstructorMarker)`, so the marker class must exist as shim *source*. `Triple`, range values and `Iterator.next()` past the end stay with Session 7. **Session 6 (2026-08-28):** range *values* are settled (`IntRange`/`IntProgression`/`IntIterator` shim, `RangesDemo` 47 checks), `first()`/`last()`/`single()`/`max()` on empty throw `NoSuchElementException` from the shim, and mixed spread calls need `SpreadBuilder`/`IntSpreadBuilder` (tier 3, shipped). `Triple` and `Iterator.next()` past the end (the builtin iterator, not the shim's) stay with Session 7. The tier-1/2 rows above are a strict subset of what shipped: the contract check's Direction A/B listing (`kotlin-shim/build/reports/shim-contract.txt`) is now the inventory of record.
2. `Enum.valueOf(Class,String)` (risk 26) and `Collection.toArray` (m): cheap-or-documented, decided against the RP2040 gate in Sessions 3/4. **Session 3 (2026-08-28): `Enum.valueOf` is documented, not implemented** — a heap-scan implementation measured ~800 B of RP2040 flash against a 4 KB two-session budget; `values()` + `name()` is the workaround (compatibility matrix). `Collection.toArray` stays with Session 4. **Session 4 (2026-08-28): `Collection.toArray` is implemented** on `ArrayList` — always a fresh `Object[]` of the list's length (the array argument is ignored), which is exactly the `toTypedArray()` shape in (m); `Map.entrySet()` + `Map$Entry`, `LinkedHashMap`/`LinkedHashSet` aliases and interface default resolution landed too, and `Locale.ROOT` + `toUpperCase(Locale)` (e) needed no JVM change.
3. `SourceDebugExtension` is 2.7 KB in the fixture; the strip removes it, but `LineNumberTable` (kept for stack traces) still maps inlined stdlib lines to the app's `SourceFile` — acceptable, documented.
4. The dump tool's `refs-all.tsv` also lists every `picodroid/**` member the fixture touches — Session 2's Direction-C allowlist can be seeded from it as well as from § 5.1.
5. **Class-metadata budget (Session 6, 2026-08-28).** The 24-demo `langsuite_kt` parses 155 classes and OOMs the 416 KB sim/RP2350 arena with only 3 KB of live objects: `census classmeta main: 155/295 parsedB=236105 devB~=127909` over a 96 KB framework baseline — ~1.5 KB per parsed class on the 64-bit sim, ~0.8 KB on device. kotlinc's inlined comparator classes (`$$inlined$sortedBy$N`, 25 for `SortingDemo` alone), `$WhenMappings` and lambda-capturing objects all count. The suite was split into `langsuite_kt` (language, 15 demos) and `langsuite_kt_stdlib` (8 demos); Session 7's port must report `classmeta` for `picoenvmon_kt` against `picoenvmon` as its first number.

## 10. Session 7 — `examples/picoenvmon_kt` vs `examples/picoenvmon` (2026-08-30)

Same protocol for both apps, same commit, sim `-b pico_enviro_mon_w -m` (416 KB
arena, headless, control FIFO): census at boot with the dashboard serving, one
A/B/X/Y nav cycle over Live → History → Network → Settings, then 100 dashboard
requests at 1 req/s. PAPKs from `build-apk.sh` in both modes. `devB~` is the
32-bit device re-derivation of the sim's `parsedB` (docs/memory-diagnostics.md).

| Metric | picoenvmon (Java) | picoenvmon_kt | Δ |
|---|---|---|---|
| PAPK, no-shrink | 75,170 B | 79,095 B | +3,925 B (+5.2 %) |
| PAPK, shrink | 68,896 B | 72,908 B | +4,012 B (+5.8 %) |
| Classes in the PAPK | 35 (23 hand-written + 12 generated DI) | 45 (30 Kotlin + 12 generated DI + 3 shim) | +10 |
| … of which never parsed | 0 | 7 (`ConstantsKt` + 6 file-private `const` facades) | — |
| Strip (Kotlin only) | — | 42 app + 53 shim classes in → 3 shim kept, 50 pruned; 187,236 → 77,001 B, 7,567 → 3,850 CP entries | — |
| Shim survivors | — | `Intrinsics`, `UninitializedPropertyAccessException`, `StringCompanionObject` (`String.format`) | — |
| Registered classes (`classmeta` total) | 174 | 184 | +10 |
| Parsed at boot (serving) | 65 · `devB~` 47,608 | 66 · `devB~` 47,495 | +1 · −113 B |
| Parsed after the nav cycle | 84 · `devB~` 64,311 | 88 · `devB~` 66,776 | +4 · +2,465 B (+3.8 %) |
| Parsed after 100 serves | 85 · `devB~` 64,871 | 89 · `devB~` 67,336 | +4 · +2,465 B |
| `tableB~` | 3,500 | 3,700 | +200 B |
| Live floor after serves (`floor`) | 13,024 B | 13,580 B | +556 B |
| Live at snapshot (`live` obj/arr/str) | 13,263 (3,596 / 8,801 / 866) | 15,667 (5,436 / 8,805 / 1,426) | snapshot phase differs (Kotlin's shows 34 pending `Socket` + 37 `SocketTimeoutException` awaiting the next GC — the clean-bind idle signature, not retention: floors are 556 B apart) |
| Native min-ever-free (`nmin`) | 66,864 B | 69,088 B | +2,224 B (noise-level; Kotlin ahead) |
| Largest free block (`lblk`) | 65,728 B | 66,488 B | — |
| Idle serve signature | `alloc=+2 stri=+1` / s | `alloc=+2 stri=+1` / s | identical |
| LEAK? / GC-PRESSURE / OOM | 0 / 0 / 0 | 0 / 0 / 0 | — |
| Sim smoke, both boards × both shrink modes | 4/4 PASS | 4/4 PASS (dashboard curl incl.) | — |
| `-l 360` gate (boot + 100 serves, no nav): `nmin` / `lblk` / `floor` | 44,328 / 40,608 / 12,816 B | 49,696 / 49,248 / 13,352 B | Kotlin +5.4 KB headroom (heap layout; not worse) |
| `-l 360` gate: curl non-200 / OOM / GC-PRESSURE | 0 / 0 / 0 | 0 / 0 / 0 | — |
| `-l 360` gate: `LEAK?` | 1 (native floor +6,656 B at w≈10) | 1 (native floor +6,672 B at w=11) | identical boot-warm-up trip in both apps — sensor registration, PWM, first-touch class parsing — `nused` is a flat sawtooth and `nmin` constant from w≈40 to the end; a protocol artifact of arming the native sentinel two windows after `Home.onCreate`, not retention |

Against the Cross-cutting budget (roadmap § Cross-cutting decisions): PAPK
≤ 160 KB ✓ (79 KB); parsed metadata ≤ 61 KB — the Java app itself now sits at
64 KB after a nav cycle because the 12 generated DI classes joined it since the
41 KB figure was taken, and the Kotlin twin is +2.5 KB over that; zero `.bss`
growth ✓ (no firmware change). Device min-ever-free (`pdb.sh sysmon`,
`pico_enviro_mon_w`, debug + `mem-diag` firmware, main `f98a3da`) was measured
in Session 8: **155.7 KB at boot, 124.3 KB after a 7 h 31 m soak** (11,677
dashboard requests, hourly nav bursts; budget ≥ 120 KB ✓), JVM live floor
13,558 B on device after the soak, native footprint 237 → 272 KB plateau after
first-visit parsing, growth sentinel trips only at warm-up and transiently on the 3-way HTTP
and hourly nav bursts (~4.7 KB of socket/screen buffers, back within a minute); native use flat at
286–288 KB from 20 minutes to 7.5 hours.

Why the deltas are this small: the port follows the frugality rules in
`examples/picoenvmon_kt/README.md` — no `companion object` anywhere (top-level
`const val`s instead; the seven facade classes are registered but never
parsed), top-level functions with `@file:JvmName` for the four stateless
utilities, `@JvmField` on cross-class fields, `private val` (no accessors)
inside classes, no stdlib collections/`lazy`/`Pair`/data classes, byte
constants through `Formatter.ascii()` (`String.getBytes()`, not the `Charset`
overload `toByteArray()` inlines to), written-out scans instead of
`IntArray.indexOf`/`trim()`. What Kotlin still adds: `lateinit` accessors on
the 12 injected fields, `Intrinsics` null checks at `as` casts, the three shim
classes, `LineNumberTable`s, and one extra parsed class per `@file:JvmName`
facade that is actually called (`TimeFormat`, `SntpClient`, `WeatherFetcher`).
