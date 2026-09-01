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
3. ~~**Natives cannot call back into Java synchronously.**~~ **Fixed
   2026-08-29** — see § E2. `NativeContext` now carries an `upcall` env and
   `NativeMethodHandler::invoke_java` re-enters the interpreter, for builtin
   and embedder arms alike. Deferred callbacks still reach Java through the
   lifecycle loop's append-only `dispatch_sites.rs` table, which remains the
   right mechanism whenever the native side can return before the Java runs.

   Three of the items this list originally blamed on the upcall were
   miscategorised, and are *not* fixed by it: `Collections.sort(list,
   comparator)` already worked in pure bytecode; custom `Interpolator`s and
   `Iterator.remove` need deferred-callback plumbing and native iterator
   state respectively; and **`ViewGroup.getChildAt` needs an
   `lv_obj_t* → ObjectRef` reverse map**, not an upcall — there is no
   Java-side child list to ask, since `addView` is native and the child set
   lives in LVGL. It is correctly gated on T3.2(B) below.

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
`NoSuchMethod` (`class_registry::is_excluded_on_this_board`). ~~Every board
currently excludes nothing, so behavior is unchanged everywhere~~ — the
mechanism is what unblocks new S-class work (JSON, Bundle, Resources,
pickers) on the RP2040.

**Correction 2026-08-31: exclusion is live, and the open follow-up below is no
longer speculative.** `testbench_rp2040/board.toml:12` excludes **nine
`picodroid/net/*` classes** (`HttpURLConnection`, `HttpInputStream`,
`HttpOutputStream`, `URL`, `Socket`, `ServerSocket`, `DatagramSocket`,
`DatagramPacket`, `InetAddress`). `framework_class_excludes` is read only by
`build_support/` and `picodroid-core` — **nothing in `buildSrc/` knows about
it** — so an app calling `new Socket(...)` for that board compiles cleanly and
dies at runtime with the hint, not at build time. Expect this to get worse: at
19,961 B free (97 %) the RP2040 will need more exclusions, not fewer.

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

### E2. Native → Java synchronous upcall — **session 1 DONE 2026-08-29**

`Executor::invoke_java` re-enters the interpreter from native code and
returns the callee's value. Proof consumer: `ArrayList.sort(Comparator)`.
Cost +4,860 B rp2040 / +5,692 B rp2350 (mechanism 1,756 B, sort 3,104 B);
ratchet baseline raised.

**Three corrections to this section as originally written:**

1. **The stated proof consumer was already working.**
   `Collections.sort(List, Comparator)` runs today in pure bytecode —
   `sdk/java/java/util/Collections.java:33-46` delegates to
   `sdk/java/java/util/Arrays.java:116-139`, a Java merge sort calling
   `c.compare`. The comment at `Arrays.java:65-69` says it is written in
   Java *precisely because* native could not upcall. It proved nothing.
   `ArrayList.sort` was used instead: `java/util/ArrayList` is
   classfile-less, so there is no Java body it could live in — blocked
   structurally rather than by convention — and it exercises both hard
   paths, a value-returning upcall and lambda-proxy resolution.

2. **Custom `Interpolator` does not need E2.** `animations::tick` is a
   `Display` hook called from `graphics/lvgl/mod.rs:94`, *outside*
   `execute()`, where `invoke_instance_with_args_returning` already works.
   What blocks it is the tick site not holding the heap and handler, plus
   the abstract-method no-op that needed the `Runnable` bridge — the
   deferred-callback problem, not this one. Same for `Iterator.remove`,
   whose state is native (`object_heap::iter_store`).

3. **The recorded sketch — parking `*mut Executor` in a static cell — is
   unsound and was rejected.** While `H::dispatch` runs, `&mut H` is held
   exclusively by the arm; a parked executor's `dispatch_native` calling
   `self.handler.dispatch(...)` aliases it. The static cell hides that
   rather than avoiding it. `handler: Option<&'a mut H>` + `take()` *is*
   sound but sound by amputation: during the upcall the trait defaults
   apply, so `gc_visit_roots` stops visiting the embedder's 32 root
   providers and a GC mid-upcall sweeps live Views, while `interrupted()`
   and `monitor_enter/exit` silently no-op. What shipped instead is a
   reborrow chain — `dispatch_native` → `H::dispatch(&mut self)` →
   `self.invoke_java(...)` → nested `Executor { handler: self }` — which
   needs zero `unsafe` and keeps roots, monitors and nested native dispatch
   working.

Also landed, both independently useful: `MAX_FRAME_DEPTH` (the Java frame
stack was unbounded, so runaway recursion exhausted the heap instead of
throwing a catchable `StackOverflowError`) and a `floor` on
`handle_exception`, without which an exception in an upcall would unwind
past the native arm into its caller's frames.

**Session 2 — DONE 2026-08-29.** `NativeContext` gained an `upcall` field
carrying `UpcallEnv` (the executor state minus the handler), and
`NativeMethodHandler::invoke_java` is a provided method, so embedder arms in
`picodroid-core` can upcall too. Cost +1,040 B rp2040 / +100 B rp2350 —
the mechanism itself is only 236 B of that.

Proof consumer: **`ListView.nativeBindAdapter`**. Java's
`refreshFromAdapter` used to loop and push one `addItem` per row; native now
*pulls*, calling `getCount()`, `getItem(int)` and `toString()` back into
app-authored bytecode. That covers three descriptor shapes, virtual dispatch
against the runtime class, and the no-bytecode-body fallthrough into the
String builtin. Verified end-to-end in the sim on picoenvmon, whose home
menu is an `ArrayAdapter<String>`: keypad nav selects row 1 and opens
`History`, so the rows are real, ordered and selectable.

Two design points worth keeping:

- **`UpcallEnv` deliberately excludes the handler.** The arm already holds
  `&mut H` and lends it back through `invoke_java`; carrying a handler here
  would hand the nested executor a second one.
- **The arm must live where `&mut self` is the handler.** The graphics
  sub-dispatchers only receive `&mut LvglBackend`, so `nativeBindAdapter`
  sits with the other `self`-taking arms in `native_handler/mod.rs`
  alongside `app_services::dispatch`, not with its ListView siblings.

The mass "NativeEnv" accessor refactor is **not** needed and was rejected:
`&mut self` on `invoke_java` already makes the borrow checker reject an arm
that holds a `ctx.objects`-derived reference across the call (verified —
it is an `E0499`), because direct field use is already a partial borrow of
`ctx`.

**Still open:** T3.4 `Adapter.getView` + convertView recycling, which is the
row-*views* half of what session 2 built the row-*data* half of.

### E3. Compile-time API contract — not started

**Why this is now the *only* compile-time fence (2026-08-31).** Nothing hides
the JDK: every javac in the tree runs `--release 8` with no `-bootclasspath` /
`--system` override, so `java.*` resolves from `ct.sym` and the SDK's own
`java/**` files are shadowed on the app compile classpath. Apps therefore
type-check against the JDK's *full* `String`, `List`, `Map` and friends while
the device serves a subset — `new TreeMap<>()`, `map.forEach`, `list.removeIf`
all compile and fail at run time. T2.2 confirmed an SDK stub cannot fix this
(javac ignores it); only a post-compile constant-pool check against the
runtime's own tables can.

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
| T1.9 | `EditText.setInputType` + password masking | N/S-small | **partial** (see below) |

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
LittleFS supports it). ~~`getAll()`~~ shipped with T2.2 (`Map<String, ?>`,
replacing `getAllKeys()`); `getStringSet`/`putStringSet` are still open and
need a new blob type tag, not interface plumbing.

**T1.9 — `setInputType` + password masking (PARTIAL — the setter already
shipped).** `706e14c` landed `EditText.setInputType(int)` and the full
`picodroid/text/InputType` constant set on **2026-06-05**, two and a half
months *before* this roadmap was written — the row was open at authoring, not
by drift. Only masking remains, and `InputType.java:44` already says so:
"Accepted; the field is not yet masked in v1."

## Tier 2 — medium milestones (1–2 weeks each)

- **T2.1 — compile-contract verifier (E3 phase 1).** Parallel Gradle-only
  track; makes "it compiled" mean "it will run".
- **T2.2 — collection interfaces as builtins. DONE 2026-08-31**, but not as
  written: **the premise of the "compile half" was false.** The *runtime* half
  landed via Kotlin Sessions 3/4 — `helpers.rs` `BUILTIN_INTERFACES` maps
  `ArrayList`→List/Collection/Iterable, `HashMap`→Map, `HashSet`→Set, plus
  `HashMap$KeySet`/`$Values` and `Appendable`, so `instanceof` and interface
  dispatch work. The compile half needed **nothing**: apps and the SDK compile
  with `javac --release 8` and no bootclasspath override, so `java.*` resolves
  from the JDK's `ct.sym`, which precedes the SDK on the class path.
  `Map<String,String> m = new HashMap<>()` has compiled and run since `cd7fc57`
  (2026-08-28) — `collectionsdemo` was already asserting it (`Map<String,Integer>
  asMap = lm`, `rttidemo`'s `(List<?>) o`) when this entry was written claiming
  the opposite. An SDK `Map.java` could not have helped: javac would shadow it.

  What actually shipped instead, once the premise was corrected:

  1. **The six body-less `java/**` SDK stubs are retired** — `java/util/List`
     (601 B), `Comparator` (254 B), `java/lang/Comparable` (235 B),
     `AutoCloseable` (187 B), `Runnable` (127 B), `Cloneable` (109 B). Every one
     was invisible to app javac (shadowed by ct.sym), never read by dispatch
     (which goes by the receiver's runtime class) and never read by RTTI (which
     walks `BUILTIN_INTERFACES`) — yet embedded on every board and loaded at
     boot. **≈1.5 KB of flash per board, reclaimed**; `java/lang/AutoCloseable`
     gained the one `BUILTIN_CLASS_NAMES` row it needed as a lambda SAM.
  2. **A hygiene test makes it permanent.** `no_bodiless_java_framework_classes`
     (`class_registry.rs`) fails any embedded `java/**` class with no Code
     attribute and no `ACC_NATIVE` method, so the next "let's add `Map.java` to
     document the surface" is rejected at test time with the reason.
     (`javax/**` is exempt — not in ct.sym, so `javax/inject/Provider` really
     must ship.)
  3. **Class literals on builtins stopped being fatal.** `resolve_class_literal`
     required the class to be *loaded*, so `String.class` / `Object.class` were
     an uncatchable `ClassNotFound` and `List.class` only worked by accident of
     the stub existing. It now accepts `BUILTIN_CLASS_NAMES` names, with
     `getClass() == String.class` identity preserved (`examples/classlit`).
  4. **The idioms are pinned** by `collectionsdemo`'s `testInterfaceTyped*`
     (interface-typed locals, params, returns, `Iterator.remove`, `Map.Entry`,
     a user `Iterable`, `instanceof`/checkcast) — inside langsuite, so the claim
     cannot go stale silently again.
  5. **Proof consumer:** `SharedPreferences.getAll()` returning `Map<String, ?>`
     replaces the non-Android `getAllKeys()`.

  The real compile-time gap is the *opposite* of what this entry described: the
  JDK's full interfaces are visible, so `TreeMap`, `map.forEach` and
  `list.removeIf` compile and then die at run time. Closing that is **T2.1**,
  and it is now the only thing standing between "it compiled" and "it runs".
- **T2.3 — Thread parity.** **DONE 2026-08-30** (concurrency-parity WP4/WP5:
  `Thread` API, `Object.wait`/`notify`, `ACC_SYNCHRONIZED`, monitor store
  with ownership; `setPriority` advisory — parity-audit THR-06). Original
  scope: `sleep`, `currentThread`, `join`, `interrupt`,
  `isAlive`, `setName` on `picodroid.concurrent.Thread`. Split
  `Object.wait`/`notify` into a separate follow-on — monitor-wait integration
  is the risky half.
- **T2.4 — line-number stack traces.** Parse `LineNumberTable`; the project's
  own "biggest debugging quality-of-life win remaining". Schedule early: it
  multiplies the velocity of everything after it.
  **Partial — and this was already true when the roadmap was written.**
  `fce8241` landed line numbers on 2026-05-06, three and a half months before
  this doc (`0dcd3fa`, 2026-08-18): `class_file/mod.rs` carries
  `lnt_offset`/`lnt_len`, `parse.rs` scans the Code sub-attributes, and
  `tests/exceptions.rs` pins the rendered format. Only the release half is
  open — it is all `#[cfg(debug_assertions)]`-gated — and since `flash.sh`
  defaults to debug builds, that gap bites less than the entry implies.
- **T2.5 — the upcall enabler (E2).** **DONE** — both sessions. Builtin and
  embedder arms can upcall; T3.4 is unblocked.
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
- **T3.4 — `Adapter.getView` + convertView recycling** (E2 done; unblocked),
  pooled to the ~12-row cap. Deliberately instead of `RecyclerView`.
  `ListView.nativeBindAdapter` already pulls `getCount`/`getItem` from
  native, so this adds the per-row *View* and the recycling pool.
- **T3.5 — `Canvas`/`onDraw`** — optional, RP2350-only, last. LVGL's canvas
  buffer is W×H×2 (112.5 KB at 240×240): impossible on RP2040, tight on
  RP2350. Needs E2. Only on concrete app demand.

## Ordering

1. ~~E1 gating~~, ~~T1.1~~, ~~T1.2~~ (done)
2. T2.7 shape corrections — early, before more code accretes on the wrong shapes
3. Remaining Tier 1 + T2.1 verifier (parallel Gradle track)
4. ~~T2.2 collection interfaces~~ (done — and it turned out to be a
   stub-retirement plus a hygiene test, not new SDK classes)
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
  *2026-08-30:* the core set landed as **pure Java** in `picodroid.concurrent`
  (`ExecutorService`/`Future`/`Callable`/`TimeUnit`/`FutureTask`, a fixed
  `ThreadPoolExecutor`, `AtomicInteger`/`Long`/`Boolean`/`Reference`,
  `CountDownLatch`) on top of the Thread parity work — zero natives, zero
  `.text`, class files only. What stays out, and what would change the answer,
  is enumerated below.

### Concurrency surface deliberately left out (2026-08-31)

Recorded after T2.3 + the `j.u.c.` core set merged (`a34a639`), so the
decisions outlive the session that made them. Every one of these is a *cost*
call, not a difficulty call: SDK classes are charged to **every** board's
flash, and `testbench_rp2040` already excludes the `j.u.c.` core set via
`framework_class_excludes` to stay under its gate (19.9 KB headroom). Anything
added here has to earn that budget on the smallest board or arrive E1-gated.

| Left out | Why | What would change it |
|---|---|---|
| `ThreadLocal` | Needs a per-task slot map the GC must root, and the natural users (a Looper, a per-thread `StringBuilder`) do not exist. The shared `sb_buf` aliasing hazard is a *separate* bug, tracked in `docs/followups-2026-08.md` § 2 — do not "fix" it by adding `ThreadLocal`. | A framework need for per-task state, or `sb_buf` being retired in favour of per-thread buffers after measurement. |
| `ReentrantLock`, `ReadWriteLock`, `Condition` | `synchronized` + `wait`/`notify` cover every in-tree case and are already kernel-recursive-mutex-backed. Locks add interruptible/timed acquisition and lock ordering — surface without a caller. | A real caller needing `tryLock`/timeout, or fairness work (WP3c) proving `synchronized` too coarse. |
| `Semaphore`, `CyclicBarrier`, `Exchanger`, `Phaser` | `CountDownLatch` covers the one shipped pattern (fan-in); the rest are pure class-file cost. | A shipped app needing bounded-permit or barrier semantics. |
| `ConcurrentHashMap`, `BlockingQueue`, `CopyOnWriteArrayList` | `synchronized` wrappers around the existing collections give the same guarantees at zero new `.text`. A genuinely concurrent map wants CAS, which **thumbv6m does not have** — it would be `AtomicSection`-guarded anyway, i.e. a coarse lock wearing a lock-free name. | RP2350-only (E1-gated) scope, plus a profile showing the coarse lock is the bottleneck. |
| `ScheduledExecutorService`, `Timer`/`TimerTask`, `Handler.postDelayed`, `CountDownTimer` | The same rejection as `Handler`/`Looper` above, re-confirmed 2026-08-30: delayed work stays executor-shaped, a `Thread` that `sleep`s then posts to `Executors.mainExecutor()`, or `view.animate()…withEndAction(…)`. Timers are leak-prone and temporally coupled, and a scheduled pool needs a timer thread per pool. | An internal tick-slot table (the Toast/animation pattern) growing a public face — a design conversation, not a backlog item. |
| Kotlin coroutines / `kotlinx-coroutines` | Contract-rejected in `docs/designs/kotlin-roadmap-2026-08.md`; the dispatcher machinery plus a Java SE library the JVM lacks (`ThreadLocal`, `WeakReference`, `IdentityHashMap`) dwarfs the flash budget. `suspend` over the existing executors is the shape to revisit, not the library. | Nothing on this roadmap. See the Compose entry below for the same arithmetic. |
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
