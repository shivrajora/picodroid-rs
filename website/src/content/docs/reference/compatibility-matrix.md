---
title: "Android compatibility matrix"
description: "Which android.* classes and idioms Picodroid mirrors, where it diverges, and the picodroid alternative for each gap."
---

Picodroid's goal is that Android code and intuition transfer directly: class
names, method signatures, and semantics track `android.*`. Embedded constraints
force some divergences, and a few Android subsystems are intentionally absent.
This page is the authoritative list of what's full, partial, renamed-only, or
unsupported — and the picodroid alternative for every gap.

The SDK is imported as `picodroid.*` — every class below mirrors its `android.*`
counterpart's name, so the API reads the same; you just import `picodroid.*`
(e.g. `import picodroid.view.View;`). Apps always use `picodroid.*` imports.

## Status legend

| Status | Meaning |
|---|---|
| **Full** | API surface and semantics match Android closely enough to port unchanged. |
| **Partial** | Present, but a subset of methods/overloads or a documented behavior difference. |
| **Renamed** | Same shape, but only reachable as `picodroid.*` (or via the android alias) — not a real Android class. |
| **Unsupported** | No equivalent; use the listed alternative. |

## By package

### android.app

| API | Status | Notes / alternative |
|---|---|---|
| `Activity` | Full | Lifecycle (`onCreate`/`onStart`/`onResume`/`onPause`/`onStop`/`onRestart`/`onDestroy`), `startActivity`, `startActivityForResult` + `onActivityResult`, `setResult`, `getIntent`, `finish`. |
| `Application` | Full | `onCreate` entry point. |
| `Service` | Partial | Started + bound services, `onRebind`, `stopSelfResult`. No `IntentService`, no foreground-service notification contract. |
| `AlertDialog` / `AlertDialog.Builder` | Partial | Positive/negative/neutral buttons, `setItems`, single- and multi-choice. **List variants cap at ~12 rows** (LVGL renderer limit) and **a message set alongside items wins** (items are dropped, with a `Log.w`) — matching Android's message-vs-items precedence. |
| `Notification` / `NotificationManager` | Partial | Basic post/cancel. No channels, styles, or actions. |
| `Fragment`, `Loader`, `PendingIntent` | Unsupported | No Fragment system — compose with Activities + Views. |

### android.view

| API | Status | Notes / alternative |
|---|---|---|
| `View` | Partial | Geometry/visibility/enabled/tag/id, `OnClickListener`, `OnLongClickListener` + `performLongClick`, `OnTouchListener`, `OnKeyListener`. No `findViewById` (no resource IDs — keep references or use `setTag`/`getTag`); no `post`/`postDelayed` (use `Executors.mainExecutor()` or animation timers). |
| `ViewGroup` / `ViewPropertyAnimator` | Partial | `animate()` with `translationX/Y`, `alpha`, `scaleX/Y`, `setInterpolator`, `withEndAction`. |
| `MotionEvent` | Partial | `getX`/`getY` are **view-relative**, `getRawX`/`getRawY` are screen-absolute, matching Android. **Coordinates are `int`, not `float`** (no FPU). |
| `GestureDetector` | Partial | `OnGestureListener` + `SimpleOnGestureListener`; slop/fling use raw coordinates. |
| `KeyEvent` | Partial | D-pad / button codes for button-only boards. |
| `LayoutInflater`, XML layouts, `Menu` | Unsupported | No resource/XML layout system — build View trees programmatically. |

### android.widget

| API | Status | Notes / alternative |
|---|---|---|
| `TextView`, `Button`, `LinearLayout`, `ImageView`, `Switch`, `CheckBox`, `ToggleButton`, `RadioButton`/`RadioGroup`, `ProgressBar`, `SeekBar`, `Toast`, `Spinner`, `NumberPicker`, `EditText`, `ListView` | Partial–Full | Core widgets present. See specific divergences below. |
| `ProgressBar` | Partial | `indeterminate()` is **creation-time only** — `setIndeterminate(boolean)` after construction is unsupported (LVGL can't morph bar↔spinner). |
| `Spinner.OnItemSelectedListener` | Partial | Full 4-arg `onItemSelected(parent, view, position, id)`; **`view` is always null** (LVGL rows have no Java wrapper) and `parent` is the `Spinner` (no `AdapterView`). |
| `ImageView` | Partial | `SCALE_FIT_CENTER`, `SCALE_CENTER`. Source is a bundled asset name (see [assets](/guides/assets/)). |
| `EditText` + `TextWatcher` | Partial | `TextWatcher` takes **`String`** (no `CharSequence`/`Editable`); **only `afterTextChanged` fires** in v1. |

### android.util

| API | Status | Notes / alternative |
|---|---|---|
| `Log` (`v`/`d`/`i`/`w`/`e`) | Full | Maps to defmt levels on device; the simulator prints every level as `[Tag] msg`. Filter by tag/level with `pdb logcat --stdin`. |

### android.graphics

| API | Status | Notes / alternative |
|---|---|---|
| `Color` | Full | Named constants + ARGB ints. |
| `drawable.GradientDrawable` | Partial | Solid/gradient fills, corner radius. |
| `Canvas`, `Paint`, `Bitmap` | Unsupported | Drawing is via LVGL widgets, not an immediate-mode `Canvas`. |

### android.content

| API | Status | Notes / alternative |
|---|---|---|
| `Intent` | Partial | Explicit (class-targeted) intents + extras. No implicit intents / `IntentFilter` resolution. |
| `Context` | Partial | `getMainExecutor`, `getDisplay`, service access. No `getSystemService` (services are exposed directly), no `getResources` (bundle files under `assets/` → generated `AssetConstants`), no `registerReceiver` (no `BroadcastReceiver`). |
| `SharedPreferences` / `Editor` | Full | Backed by LittleFS. |
| `DialogInterface` | Full | `OnClickListener`, `OnDismissListener`, `OnMultiChoiceClickListener`, button constants. |

### android.os

| API | Status | Notes / alternative |
|---|---|---|
| `SystemClock` | Full | `uptimeMillis`/`elapsedRealtime`. |
| `Handler` / `Looper` / `Message` | Unsupported | Use `Executors.mainExecutor().execute(Runnable)` for "post to UI"; for delayed work, a `Thread` that sleeps then posts, or the animation engine's `withEndAction`. There is no `postDelayed`. |
| `Bundle` | Partial | Intent extras only. |

### android.hardware

| API | Status | Notes / alternative |
|---|---|---|
| `Sensor` / `SensorManager` / `SensorEvent` / `SensorEventListener` | Partial | Board-dependent sensors; registration + event callbacks. |

### Concurrency

| API | Status | Notes / alternative |
|---|---|---|
| `Thread` (`picodroid.concurrent.Thread`) | Full | `start()` spawns a real FreeRTOS task on device and in the simulator (the sim runs the real kernel). `run()` override or `Runnable` target, `sleep`, `join`/`join(ms)`, `interrupt`/`isInterrupted`/`interrupted`, `isAlive`, `currentThread`, `get`/`setName`, `getId`, `yield`, `UncaughtExceptionHandler` (per-thread and default), `IllegalThreadStateException` on a second `start()`. `synchronized` blocks **and methods** are real kernel mutexes; `Object.wait`/`notify`/`notifyAll` work (timed, interruptible, `IllegalMonitorStateException` when not owner). Divergences: `setPriority` and `setDaemon` are advisory (every Java task runs at one RTOS priority — see the system reference), and a compute-bound thread holds the core until it blocks (no time slicing). |
| `Executor` / `Executors` (`mainExecutor` / `backgroundExecutor`) | Full | The recommended concurrency primitive — this is how you "post to the UI thread". |
| `ExecutorService` / `Future` / `Callable` / `TimeUnit` (`picodroid.concurrent`) | Partial | `Executors.newFixedThreadPool(n)` and `newSingleThreadExecutor()`: `execute`, `submit(Runnable/Callable)`, `Future.get`/`get(timeout, unit)`/`cancel`/`isDone`/`isCancelled`, `shutdown`/`shutdownNow`/`isShutdown`/`isTerminated`/`awaitTermination`; `ExecutionException` (with `getCause()`), `CancellationException`, `TimeoutException`, `RejectedExecutionException` under their `java.util.concurrent` names. Pure Java over `Thread` + `wait`/`notify`; each worker costs a 16 KiB task stack. Not built into `testbench_rp2040` (`framework_class_excludes`, flash headroom) — nor are the atomics and `CountDownLatch` below. No `invokeAll`/`invokeAny`, no scheduled or cached pools, no `CompletableFuture`. |
| `AtomicInteger` / `AtomicLong` / `AtomicBoolean` / `AtomicReference` (`picodroid.concurrent`) | Partial | `get`/`set`/`getAndSet`/`compareAndSet`/`incrementAndGet`/`getAndIncrement`/`decrementAndGet`/`addAndGet`/`getAndAdd`. `synchronized` underneath (one core, one JVM priority). No `updateAndGet`/lambdas, no `AtomicIntegerArray`. |
| `CountDownLatch` (`picodroid.concurrent`) | Full | `countDown`, `await`, `await(timeout, unit)`, `getCount`. No `Semaphore`, `CyclicBarrier`, `ReentrantLock`, `ConcurrentHashMap`, `BlockingQueue`. |

### java.* standard library

| API | Status | Notes / alternative |
|---|---|---|
| `Object.clone()` / `Cloneable` | Partial | Shallow copy works, but **the `Cloneable` check is skipped** — `clone()` never throws `CloneNotSupportedException`. |
| Interface `default` methods | Full | Resolved per JVMS §5.4.3.3 (sub-interface overrides win whatever the `implements` order; `I.super.f()` works; defaults are found through abstract and builtin superclasses). **Exception: a lambda's own interface defaults** — calling a default method *on a lambda object* runs the lambda body instead, so keep lambda-typed interfaces to their single abstract method. |
| `StringBuilder.append(Object)` / `String.valueOf(Object)` / `"" + obj` | Full | The argument's `toString()` runs first — a Java override, else the builtin one for boxes/enums, else the identity form `pkg.Cls@hhhh`; `null` prints `null`. |
| `Object.equals`/`hashCode`/`toString` defaults | Partial | Identity semantics as in Java, but **`hashCode()` is the object's heap slot index** (stable for the object's lifetime, reused after GC) and builtin collections use identity too (`new ArrayList<>().hashCode()` is not content-based; `list.toString()` prints `java.util.ArrayList@…`, not the elements). |
| `instanceof` / `checkcast` | Partial | Strings, arrays, builtin collections under `List`/`Collection`/`Iterable`/`Set`/`Map`, boxes under `Number`/`Comparable`, lambdas under their interface, and transitive superinterfaces all work; a failed cast throws a catchable `ClassCastException` (with a `null` message). **A reference array's element class is not recorded**, so `(String[]) someObjectArray` succeeds where Java would throw. |
| Boxed wrappers (`Integer`, `Float`, …) | Partial | `equals`/`hashCode`/`compareTo`, the `compare`/`hashCode(x)` statics and `Float.floatToIntBits` follow Java 8. **The `xxxValue()` accessors return the box's own value unconverted** (`Float.valueOf(2.5f).intValue()` yields a float; call the matching accessor, or unbox and convert). `Character` has `isDigit`/`isLetter`/`toUpperCase`/`toLowerCase` only, **ASCII-only** (strings are byte-backed). |
| `Enum.valueOf(Class, String)` | Unsupported | The static every enum's synthetic `valueOf(String)` delegates to has no builtin (a heap scan cost ~800 B of RP2040 flash). Look the constant up yourself: `for (Color c : Color.values()) if (c.name().equals(s)) …`. `Enum.hashCode()` is the ordinal. |
| `Throwable` | Partial | `addSuppressed`/`getSuppressed`/`getCause` stored; `ExceptionInInitializerError` wraps `<clinit>` throws. **A failed `<clinit>` does not poison the class** (no `NoClassDefFoundError` on re-access). |
| `Comparator` + `Collections.sort(List, Comparator)` | Full | Lambda comparators supported. |
| `HashMap.entrySet()` / `Map.Entry` | Partial | `entrySet()`, `keySet()` and `values()` views answer `iterator()` and `size()`; the key and value views also `contains()` (`k in map.keys`). Entries answer `getKey()`/`getValue()` (no `setValue`). Iteration order is the map's internal order. |
| `HashSet.iterator()` | Full | `for (x : set)` and every iterating idiom on a set; hash order, like the map views. Iterators and map views keep a **temporary** collection alive for as long as they do (`for (String w : text.split(" "))`, `for (e : makeMap().entrySet())`) — the GC pins the iterator's source. |
| `Map.putAll` / `List.listIterator` / `Arrays.equals` | Unsupported | No builtin arm: copy with an `entrySet()` loop, iterate by index, compare arrays element-wise. Kotlin: `map += otherMap`, `list.last { }` / `indexOfLast { }` / `findLast { }` (they walk a `listIterator`) and `contentEquals` hit these. |
| `Float.isNaN(f)` / `Double.isNaN(d)` / `isInfinite` statics | Unsupported | Use `f != f` for NaN and a magnitude compare for infinity (Kotlin's `isNaN()`/`isInfinite()`/`isFinite()` inline to these statics). |
| `String(char[])` | Unsupported | Only the `byte[]` constructors exist; build with `StringBuilder.append(char)` (Kotlin: `chars.concatToString()` does this). |
| `Math.round` | Partial | Rounds half **away from zero** (`Math.round(-2.5f)` is `-3`; Java gives `-2`). Kotlin's `roundToInt()`/`roundToLong()` shim spells out Java's `floor(x + 0.5)` and is exact. |
| `String.replace(target, replacement)` | Partial | An empty `target` leaves the string unchanged (Java interleaves the replacement between every char). |
| `LinkedHashMap` / `LinkedHashSet` | Partial | **Aliases of `HashMap` / `HashSet`: insertion order is not preserved.** `instanceof HashMap`/`Map`/`Set` hold. |
| `Collection.toArray()` / `toArray(T[])` | Partial | Always returns a **fresh** `Object[]` of exactly the collection's size — the array argument is neither filled nor returned (`list.toArray(new String[0])` is the idiom to use; a cast to `String[]` succeeds because reference arrays carry no element class). |
| `java.util.Locale` | Partial | Name-only: `Locale.ROOT`/`US`/… read as `null`, and `toUpperCase(Locale)`/`toLowerCase(Locale)` ignore the argument (ASCII only). This is what Kotlin's `uppercase()`/`lowercase()` compile to. No `Locale` instances. |
| `Class.getName()` | Full | Returns dot-form (`pkg.Class`) per the Java spec. |
| `javax.inject.Inject` / `Singleton` / `Scope` | Partial | Compile-time only: an annotation processor generates `Foo_Factory` / `Foo_MembersInjector`, and `Application` / `Activity` / `Service` are injected automatically before `onCreate`. `SOURCE` retention (JSR-330 says `RUNTIME`) — no runtime annotations, no reflection. `javax.inject.Provider<T>` / `picodroid.di.Lazy<T>` (≙ `dagger.Lazy`) inject anywhere a `T` can; `picodroid.di.Module` / `picodroid.di.Provides` (≙ `dagger.Module` / `dagger.Provides`) bind SDK types and interfaces, every module auto-installed. No `@Component`, `@Binds`, `@Named`/qualifiers, custom scopes. Java and Kotlin apps (the Kotlin plugin runs the same processor through kapt; Kotlin shapes: `@Inject lateinit var`, `@Inject constructor`, `@Module object` + `@JvmStatic @Provides`, `@Module class` for instance `@Provides`). See [Services & DI](/api/services/). |
| `String.split` | Partial | Literal delimiters only — **no regex**. |
| `BufferedReader` / `InputStreamReader` | Unsupported | Use the byte-oriented `picodroid.io` streams (`FileInputStream`/`FileOutputStream`); there is no char-stream reader layer. |

## Cross-cutting divergences

- **Coordinates and sizes are `int` px.** There is no `float` `MotionEvent`
  coordinate, and no density-independent units — no `dp`/`sp`, no
  `getResources().getDisplayMetrics()`. Lay out in pixels.
- **No resources system.** No `R` class, no `res/` directory, no XML layouts,
  drawables, or strings. Bundle binary assets under `assets/` and reference
  them through the generated `AssetConstants` (see [assets](/guides/assets/)).
- **No `Handler`/`Looper`.** The main loop is an executor-driven dispatcher;
  use `Executors.mainExecutor()` and animation timers.
- **Custom `Interpolator`s fall back to linear.** Standard interpolators
  (linear/accelerate/decelerate/accelerate-decelerate) map to native easing; an
  app-defined `Interpolator` can't be up-called from the native tick, so it
  falls back to linear with a `Log.w`.

## Imports

Always import the `picodroid.*` classes directly — e.g.
`import picodroid.view.View;`, `import picodroid.widget.TextView;`. There is no
`android.*` import compatibility layer: `import android.view.View;` will not
compile or load. The package names below mirror Android's only so the API reads
the same and intuition transfers; the namespace you import is `picodroid`.
