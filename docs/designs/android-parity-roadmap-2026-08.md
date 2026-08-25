# Roadmap: Android API parity — 2026-08-18

**Goal:** grow the `picodroid.*` Java API toward its `android.*` counterpart —
same class names, same method signatures, same semantics — so an Android
developer's code and intuition transfer directly. Apps always import
`picodroid.*`; the `android.*` alias layer was built and reverted on
instruction (`6663c4c`, and `CLAUDE.md`), and is not coming back. This doc is
the standing tracker for that work, in the shape of
`docs/networking-followups-2026-08.md`.

Written against the tree at `1a014ec`. The surface it audits: 100 classes
across 19 `picodroid.*` packages, 11 `java.*` stubs, ~25 JVM-native builtins,
~308 native methods, 57 example apps.

## Why now

Three structural findings dominate everything below.

1. **There is no compile-time API contract.** Apps compile against the host
   JDK's full `java.*` — only `picodroid.*` comes from `:sdk`
   (`buildSrc/…/PicodroidPapkPlugin.kt:51`, `--release 8`). So
   `new LinkedList<>()`, `str.matches(…)`, and `System.out.println` all
   compile cleanly and die at runtime with `NoSuchMethod`. Android's
   `android.jar` *is* this contract. Closing it costs zero device bytes.
2. **RP2040 flash is the binding constraint.** The release image sits at
   915,663 / 917,248 bytes — **1,585 bytes free**. Every compiled SDK class
   is embedded on every board and loaded at boot
   (`build_support/papk.rs`, `boot.rs`), so a new SDK class costs its full
   `.class` size in flash everywhere, used or not. One mid-sized class does
   not fit. JVM *builtins* (native-backed `java.*` classes with no `.class`
   file) are far cheaper and shared across boards.
3. **Natives cannot call back into Java synchronously.** `NativeContext`
   holds no interpreter handle, so `Collections.sort(list, comparator)`,
   forEach-with-lambda, `Iterator.remove`, custom `Interpolator`s,
   `Adapter.getView`, and `ViewGroup.getChildAt` are all blocked on the same
   missing mechanism. Callbacks reach Java only through the lifecycle loop's
   append-only `dispatch_sites.rs` table.

**Flash cost classes** used throughout:
**G** = Gradle/host only (0 device bytes) ·
**N** = native method on an existing class (~0.1–1 KB shared `.text`) ·
**B** = JVM builtin (no `.class` file) ·
**S** = new SDK class (~1.5–3 KB `.rodata`, on every board).

Breaking changes are free — picodroid has no external users. Shape
corrections land as ordinary commits with the in-repo examples migrated
alongside; no deprecation shims, no compatibility windows.

## Enablers

### E1. Per-board framework-class gating — **DONE 2026-08-18**

`board.toml` gained an optional top-level `framework_class_excludes` key (a
`;`/`,`-separated list of JVM internal names). `embed_framework_classes`
drops those classes from the embedded set for that board; excluding a class
also excludes its inner classes. An exclude matching no compiled class fails
the build, so a typo cannot silently keep shipping the class. A native miss
on an excluded class says so rather than surfacing as a bare
`NoSuchMethod` (`class_registry::is_excluded_on_this_board`). Every board
currently excludes nothing, so behavior is unchanged everywhere — the
mechanism is what unblocks new S-class work (JSON, Bundle, Resources,
pickers) on the RP2040.

Verified end-to-end: excluding `picodroid/widget/NumberPicker` took the
embedded set from 137 to 135 classes (the class and its
`$OnValueChangeListener`), the app still booted, and a deliberately
misspelled exclude failed the build with the intended message.

**First real use.** T1.2 grew `HttpURLConnection.class` from 3,400 to 6,427
bytes, which — since every SDK class ships on every board — pushed the RP2040
release image 1,911 bytes past its program region. Networking will never be
supported on `testbench_rp2040`, so the whole `picodroid.net` stack is dead
weight there; excluding all of it except `NetworkInfo` took the image from
915,663 (the pre-tranche baseline) to **906,231** — 11,017 bytes free
instead of 1,585, and 128 embedded classes instead of 137.

`NetworkInfo` deliberately stays: the native stub keeps
`isConnected()`/`getIpAddress()` answerable
(`native_handler/net_stub.rs`) precisely so an app targeting several boards
can probe and degrade, which needs the class to resolve. It is 267 bytes and
references nothing else, so keeping it costs almost nothing. Note the
behavioral difference this introduces: on a board that merely lacks
networking, a socket call throws `UnsupportedOperationException`; on one that
also excludes the classes, it fails to resolve. Probe-and-degrade is the
portable pattern.

This is worth internalizing as the standing rule: **an SDK class that a board
cannot use is pure flash cost on that board**, and Tier 1's cheap-looking
Java additions are only cheap on RP2350.

**Open follow-up:** a Gradle-side check that fails an *app* build which
references a class excluded on its target board, instead of letting it
surface at runtime. Worth building before any board actually excludes
something.

### E2. Native → Java synchronous upcall — not started

A standalone milestone (1–2 weeks) with exactly one proof consumer:
`Collections.sort(List, Comparator)`. The sketch (op_invoke refactor +
`Executor::invoke_inline` + an `Upcaller` trait) is recorded in project
memory. Touches `jvm/src/interpreter/`, `jvm/src/native/mod.rs`,
`picodroid-core/src/native_handler/mod.rs`. Risk is high: reentrancy, and GC
roots that must survive across the upcall boundary
(`gc_root_registration.rs`). Everything in Tier 1 deliberately avoids
needing it.

### E3. Compile-time API contract — not started

*Phase 1 (the valuable half).* A post-compile bytecode verifier in
`PicodroidPapkPlugin.kt`: scan each app's constant pool against an allowlist
**generated from the runtime's own tables** (`method_tables.rs`,
`class_registry.rs`, the SDK class list) so the contract cannot drift from
what the device actually implements. Fail the Gradle build with an
actionable message, reusing the `API_HINTS` text. Cost **G**; risk low. This
achieves `android.jar`'s purpose without the `android` namespace.

*Phase 2 (optional, later).* A restricted compile classpath — a stub jar of
only the supported `java.*` subset — so IDE autocomplete matches reality
too.

## Tier 1 — quick wins (days each, all N/B)

| # | Item | Cost | Status |
|---|---|---|---|
| T1.1 | StringBuilder per-instance buffers | N | **DONE** |
| T1.2 | `HttpURLConnection` request/response headers | N | **DONE** |
| T1.3 | Widget fidelity fills | N | open |
| T1.4 | `TextView`/`EditText` text surface | N | open |
| T1.5 | Input & sensor fills | N | open |
| T1.6 | `Gpio` input | N | open |
| T1.7 | `java.util.Objects`, `String.join` | B | open |
| T1.8 | Persistence fills | N | open |
| T1.9 | `EditText.setInputType` + password masking | N/S-small | open |

**T1.1 — StringBuilder per-instance buffers (DONE).** Every builder shared
one global LIFO buffer, so two concurrently-alive builders interleaved
(`a.append(x); b.append(y)` both landed in `b`) and aliased across threads;
`alloc` additionally handed every `new StringBuilder()` the same heap slot.
Each instance now owns a buffer in a side store addressed by a slot index in
field 0 — the `list_bufs`/ArrayList pattern — freed on GC sweep.
`toString()` is now non-destructive, as on Android. Peak heap and GC count
on `benchmark` are unchanged (277 KB, 403 collections).

**T1.2 — HTTP headers (DONE).** `setRequestProperty` / `addRequestProperty` /
`getRequestProperty`, `getHeaderField(String|int)`, `getHeaderFieldKey(int)`,
`getResponseMessage()`, `getErrorStream()`, and the `HTTP_*` status
constants. Request headers are assembled Java-side (which owns ordering,
replace-vs-add, the 16-header cap, and CR/LF injection rejection) and passed
to `nativeConnect` as preformatted lines; `Host`, `Connection`, and
`Content-Length` stay connection-managed. Response headers are read by
re-scanning the retained head in `rx_buf` — no parsed table, so no extra
heap. Index 0 is the status line with a null key, per Android. Head parsing
moved to `net/http_head.rs` with a `#[path]` test shim in `lib.rs`, because
`net` is `cfg(not(test))` and its six existing tests had never run.

**T1.3 — widget fidelity fills.** `View.getParent()`/`getContext()`;
`ViewGroup.removeViewAt`/`indexOfChild`/`addView(View,int)`;
`ListView.setSelection`; `ArrayAdapter.remove`/`insert`/`addAll`/
`getPosition`; `Toast.setGravity`; `AlertDialog.setCancelable` and
`setOnDismissListener` (one appended `dispatch_sites.rs` row);
`LinearLayout.setGravity`; `Notification` icon/priority.

**T1.4 — text surface.** `TextView.getText()`, `setTextSize(float)` and
`(int unit, float)`, `setGravity`, `append`; `EditText.getText`/`setText`/
`setSelection`. `getText()` is the single most-typed widget call in Android
code. Decide `CharSequence` vs `String` here: declaring `CharSequence`
matches Android exactly and keeps the universal `getText().toString()` idiom
working either way.

**T1.5 — input & sensors.** `MotionEvent.ACTION_CANCEL`;
`GestureDetector.onDown`/`onScroll`/`onDoubleTap`;
`SensorManager.getSensorList(int)`.

**T1.6 — Gpio input.** The cheapest hardware gap: `HalGpio` already has
`read`/`set_input`/`enable_edge_irq`, but the Java `Gpio` is output-only.
Needs `getValue()`, `DIRECTION_IN`, and an edge callback — plus a GC-root
visitor for the retained callback, historically this repo's #1 bug class.

**T1.7 — cheap builtins.** `java.util.Objects` (`equals`, `hashCode`,
`requireNonNull`, `toString`) and `String.join`. No `.class` cost, both
boards benefit.

**T1.8 — persistence fills.** `Intent` long/float/double extras;
`SharedPreferences.getFloat`/`putFloat`; `File.getName`/`getParent`/
`mkdirs`/`createNewFile`/`list()` (`list()` needs a `HalFs` readdir —
LittleFS supports it). `getAll()`/`getStringSet` wait for T2.2.

## Tier 2 — medium milestones (1–2 weeks each)

- **T2.1 — compile-contract verifier (E3 phase 1).** Parallel Gradle-only
  track; makes "it compiled" mean "it will run".
- **T2.2 — collection interfaces as builtins.** `java.util.Map`/`Set`/`List`/
  `Collection`/`Iterable`, `java.lang.CharSequence`/`Comparable`, implemented
  by the existing builtins, so `Map<String,String> m = new HashMap<>()` — the
  most basic Java idiom there is — compiles. Pathfinder for builtin-interface
  plumbing, and a prerequisite for JSON `keys()`, `getAll()`, and T3.3.
- **T2.3 — Thread parity.** `sleep`, `currentThread`, `join`, `interrupt`,
  `isAlive`, `setName` on `picodroid.concurrent.Thread`. Split
  `Object.wait`/`notify` into a separate follow-on — monitor-wait integration
  is the risky half.
- **T2.4 — line-number stack traces.** Parse `LineNumberTable`; the project's
  own "biggest debugging quality-of-life win remaining". Schedule early: it
  multiplies the velocity of everything after it.
- **T2.5 — the upcall enabler (E2).**
- **T2.6 — JSON.** `picodroid.json.JSONObject`/`JSONArray`/`JSONException`
  with exact `org.json` signatures. Native handle-backed parse tree with
  strings materialized on `get`, so no native code holds `ObjectRef`s. Three
  S-classes (E1-gated) plus `native_handler/json.rs`. Retires picoenvmon's
  hand-rolled parser. *Namespace note:* Android ships this as `org.json`; the
  picodroid-namespace rule makes it `picodroid.json`.
- **T2.7 — shape corrections.** `Service extends Context`;
  `onStartCommand(Intent, int flags, int startId)` (fixing the 2-arg form at
  `Service.java:57`); `Button extends TextView` (fixing `Button.java:7`);
  `ViewPropertyAnimator` to-only signatures (`alpha(float)`,
  `translationX`, `rotation`, `scaleX/Y`, `setStartDelay`) with the
  `from,to` variants deleted outright. `Activity.onCreate(Bundle)` joins once
  Bundle exists. Land early — breaking is free, and later work should build
  on the corrected shapes.
- **T2.8 — `DatePickerDialog`/`TimePickerDialog`.** Thin S-classes over
  `AlertDialog` plus the existing picker widgets.

## Tier 3 — large milestones

- **T3.1 — Bundle + instance state.** (A) `picodroid.os.Bundle`, a
  native-backed typed map, plus `Intent.putExtras`/`getExtras`. (B)
  `onCreate(Bundle)`. (C) `onSaveInstanceState`/restore across the 8-deep
  activity stack.
- **T3.2 — resource system + `R.*` + XML layouts.** The largest remaining
  Android-parity gap, and the one that supersedes the `API_HINTS` entries
  steering people away from `findViewById`/`getResources`/
  `getLayoutInflater`. (A) `res/values/` → Gradle-generated app-side
  `R.java` (static finals, so zero framework flash) + a PAPK resource chunk +
  `Resources.getString`/`getColor`/`getDimension` and `Context.getResources()`.
  (B) `res/layout/*.xml` precompiled by Gradle into a compact binary format —
  never ship an XML parser to the device, mirroring Android's binary XML —
  plus `setContentView(int)`,
  `LayoutInflater.inflate(int, ViewGroup, boolean)`, the generic
  `<T extends View> T findViewById(int)`, and `ViewGroup.getChildAt` (which
  today throws, gated on exactly this milestone). (C) `res/drawable/` →
  `ImageView.setImageResource(int)`. (D) AttributeSet and styles later.
- **T3.3 — `java.io` stream hierarchy.** `InputStream`/`OutputStream` as
  abstract builtins; re-parent the `File*` and `Http*` streams;
  `Socket.getInputStream()`/`getOutputStream()`; `InputStreamReader` and
  `BufferedReader.readLine()`. The biggest "code from the internet just
  works" enabler. (The typed-exceptions design excluded socket streams from
  *its own* scope, not permanently.)
- **T3.4 — `Adapter.getView` + convertView recycling** (needs E2), pooled to
  the ~12-row cap. Deliberately instead of `RecyclerView`.
- **T3.5 — `Canvas`/`onDraw`** — optional, RP2350-only, last. LVGL's canvas
  buffer is W×H×2 (112.5 KB at 240×240): impossible on RP2040, tight on
  RP2350. Needs E2. Only on concrete app demand.

## Ordering

1. ~~E1 gating~~, ~~T1.1~~, ~~T1.2~~ (done)
2. T2.7 shape corrections — early, before more code accretes on the wrong shapes
3. Remaining Tier 1 + T2.1 verifier (parallel Gradle track)
4. T2.2 collection interfaces
5. T2.4 line-number stack traces
6. T2.6 JSON + T2.3 Thread parity
7. T3.1 Bundle → `onCreate(Bundle)` → save/restore
8. T2.5 upcall → T3.4 convertView recycling
9. T3.2 resource system (A → B → C)
10. T3.3 `java.io`

## Not doing, and why

- **`android.*` imports / stub jar / alias rewriting.** Reverted on
  instruction; E3 achieves the underlying goal without the namespace.
- **`Handler` / `Looper` / `postDelayed` / `CountDownTimer`.** Explicitly
  rejected: leak-prone on Android (Handlers keep Activities alive), easy to
  forget cancellation, temporally coupled. Delayed work stays
  executor-shaped, `Thread` + `SystemClock.sleep`, or an internal tick-slot
  table (the Toast/animation pattern). A coroutine/flow-style alternative is
  a someday conversation, not this roadmap.
- **`RecyclerView`.** Needs upcalls, recycling, and layout managers for a
  ~12-row screen; `ListView` + convertView gets the semantics far cheaper.
- **HTTPS/TLS.** No stack is vendored and the flash budget rules it out on
  RP2040. `connect()` keeps throwing. Revisit as RP2350-only, behind E1, if
  a real case appears.
- **Full `java.util.concurrent`, `LinkedList`, `TreeMap`, `ArrayDeque`.** No
  demonstrated need; every builtin still costs shared `.text` and table rows.
  Add `AtomicInteger` alone if thread-parity work surfaces demand.
- **`Fragment` before the resource system.** Fragments without layouts and
  ids are shape without substance.
- **Jetpack Compose proper.** Even runtime-only Compose (custom `Applier`,
  no `compose-ui`) is ~2k classes / ~600 KB of tree-shaken class files plus
  kotlinx-coroutines plus a Java SE library the JVM does not have
  (`ThreadLocal`, atomics, `WeakReference`, `IdentityHashMap`); the class
  table alone would be ~40 KB and a first composition is ~10⁶ bytecodes at
  ~1 M bytecodes/s. A Compose-*like* declarative layer over the retained
  `View` tree is feasible and is deferred in `docs/quality-roadmap.md`
  § Framework direction, behind `docs/designs/kotlin-roadmap-2026-08.md`.
