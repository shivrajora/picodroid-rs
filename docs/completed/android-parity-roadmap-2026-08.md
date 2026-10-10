# Completed: Roadmap: Android API parity — 2026-08-18

Items closed out of [android-parity-roadmap-2026-08.md](../designs/android-parity-roadmap-2026-08.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

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

**Open follow-up — closed 2026-08-31 by E3:** `verifyApiContract` with
`-Ppicodroid.board=<name>` (forwarded by `build-apk.sh --board`, which every
board-aware script now passes) fails an *app* build that references a class
the board excludes, naming the class, the call sites and the board.toml.

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

## Tier 1 — quick wins (days each, all N/B)

| # | Item | Cost | Status |
|---|---|---|---|
| T1.1 | StringBuilder per-instance buffers | N | **DONE** |
| T1.2 | `HttpURLConnection` request/response headers | N | **DONE** |
| T1.7 | `java.util.Objects`, `String.join` | B/S-small | **DONE** 2026-09-02 |

**High priority — next (deferred 2026-09-02, in this order):**

1. ~~`File.list()` / `listFiles()`~~ — **shipped 2026-09-08** with multi-app
   M3a (`c50422d1`).
2. ~~`Context.getFilesDir()` / `openFileInput` / `openFileOutput` / `deleteFile`
   / `fileList()`~~ — **shipped 2026-09-08** (`c50422d1`), sandboxed under
   `/data/<package>`.

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

**T1.7 — cheap builtins.** `java.util.Objects` (`equals`, `hashCode`,
`requireNonNull`, `toString`) and `String.join`. No `.class` cost, both
boards benefit.

## Tier 2 — medium milestones (1–2 weeks each)

- **T2.1 — compile-contract verifier (E3 phase 1).** **DONE 2026-08-31** —
  see E3. "It compiled" now means "it will run" for the `java/**` surface.
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
  which landed the same day — see E3.
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
  **DONE 2026-09-02.** `fce8241` had landed line numbers on 2026-05-06 —
  `parse.rs` scans the Code sub-attributes, `tests/exceptions.rs` pins the
  format — but all of it was `#[cfg(debug_assertions)]`-gated, and the
  earlier claim here that `flash.sh`'s debug default therefore kept lines
  was wrong: `lib.sh build_firmware` forces
  `--config profile.dev.debug-assertions=false` in *both* profiles (the
  RP2040 flash gate), so no device image ever had the parser, and the
  2026-09-01 strip removed the tables from the bytes as well. Now: a
  `line-numbers` cargo feature is the only gate (JVM, `build.rs` tree
  choice, `framework_classes.rs` invariant); the sim and debug-profile
  `flash.sh` firmware have it and print Android's
  `at pkg.Class.method(File.java:39)` (a `SourceFile` reader came with it);
  release firmware prints `(pc=N)` and `scripts/retrace.sh [--app <app>]`
  resolves those on the host from the unstripped class trees, composed with
  the shrink-map un-shrinking it already did. Zero RAM (`lnt_offset` is a
  `u16` in `MethodInfo`'s padding); ~15 KB of flash for the SDK tables,
  debug-profile images only (flash-string-budget §4).
- **T2.5 — the upcall enabler (E2).** **DONE** — both sessions. Builtin and
  embedder arms can upcall; T3.4 is unblocked.
- **T3.3 — `java.io` streams. DONE 2026-10-09**, with T3.4 in
  `designs/java-io-streams-2026-10.md`. `InputStream`, `OutputStream`,
  `Reader`, `Writer` (abstract), `ByteArrayInputStream`/`OutputStream`,
  `InputStreamReader`, `BufferedReader.readLine`, `OutputStreamWriter`,
  `PrintWriter` as pure-Java SDK classes; `picodroid.io.File*` and
  `picodroid.net.Http*` streams re-parented; `Socket.getInputStream()`/
  `getOutputStream()`. *What differed from the plan:* no "abstract builtins"
  — bodied `java/**` class files needed no JVM table rows at all, only two
  name-only classes (`Closeable`, `UnsupportedEncodingException`); the real
  work was the compile-time contract learning SDK class-file `@extends`
  edges and the "chars are bytes" decision (no charset decoding; a UTF-8
  line survives `readLine` because it is built with `append(char)`).
- **T3.4 — `getView` + convertView. DONE 2026-10-09.** Rows are Java views
  built by `getView`, kept as the list's children, re-bound through
  `convertView` on every `notifyDataSetChanged`; `ArrayAdapter` gained the
  layout-resource constructors; `ViewGroup.addView(child, index)`. *What
  differed:* no ~12-row pool — rows never scroll out of existence here, so
  the pool is the row set and `setAdapter` starts fresh (Android clears its
  recycler then too); and the Java loop replaced the `nativeBindAdapter`
  upcall loop rather than extending it.
- **T2.6 — JSON. DONE 2026-09-04.** `picodroid.json.JSONObject`/`JSONArray`/
  `JSONException` with Android's full `org.json` surface for the two classes.
  Native node pool (`picodroid-core/src/json/`: pool, strict RFC 8259 parser,
  `JSONStringer`-style serializer) plus `native_handler/json.rs`; wrappers hold
  an `int` node index, values materialize on `get`, child wrappers share the
  parent's node (identity semantics). No native code holds a JVM reference —
  what it needed instead was to learn when a wrapper dies: a new
  `NativeMethodHandler::native_state_prune` hook, called from the same funnel
  as `monitors_prune`, drops dead wrappers' bindings and sweeps unreachable
  nodes. *What differed from the plan:* there was no hand-rolled parser to
  retire — picoenvmon was on wttr.in's plain-text one-liner — so the consumer
  is an open-meteo JSON fetch (both twins); the Java side owns every boxing
  and coercion decision so the natives are one typed primitive each and need
  no descriptor rows; the pool charges its nodes to the GC pacer
  (`ObjectHeap::charge_alloc_events`) because native `Vec`s are invisible to
  it; and E1 gating became a board switch, `has_json = true` in board.toml,
  which derives both the class exclusion (firmware and `verifyApiContract`)
  and the `cfg(has_json)` for the Rust side — on for the four RP2350 boards,
  off for `testbench_rp2040`. `examples/jsondemo` is the conformance app.
  *Namespace note:* Android ships this as `org.json`; the picodroid-namespace
  rule makes it `picodroid.json`.
- **T2.7 — shape corrections. DONE 2026-08-31.** `Service extends Context`
  and `onStartCommand(Intent, int flags, int startId)` (`flags` is always 0:
  redelivery after a process kill has no MCU analogue); `Button extends
  TextView`; `ViewPropertyAnimator` to-only (`alpha`/`x`/`y`/`translationX/Y`/
  `rotation`/`scaleX/Y(float)`, `setDuration(long)`, `setStartDelay(long)`)
  with the `from,to` variants deleted, plus `View.set/getTranslationX/Y`,
  `set/getRotation`, `set/getScaleX/Y` over two generic natives
  (`nativeSetProperty`/`nativeGetProperty`) so the setters and the animator
  share one unit conversion. What the implementation found, worth keeping:
  - `Service extends Context` and the 3-arg callback were Java-only changes —
    `app_services.rs` routes the Context natives by method name, the callback
    is invoked by name with an explicit arg array, and shrink maps are
    class-name-only. The one behavioural wrinkle: a `bindService` from inside
    a Service is owned by the foreground Activity (bindings are per Activity).
  - `Button` re-declares `native setText`: its LVGL object is a button with a
    child label, so TextView's label arm must not run on it. `setTextColor`
    is inherited and reaches TextView's arm through the native superclass
    walk (`ops_invoke.rs`), which works because LVGL's `text_color` cascades
    to the child. A *private* native declared on `TextView`
    (`nativeSetLineMode`) is an `invokespecial` and always dispatches on
    `TextView`, so its impl resolves a `Button` receiver to the child label
    itself (`label_of` in `graphics/lvgl/widgets/text_view.rs`). `TextView`
    now declares one field (`mLineMode`), which `Button` inherits.
  - LVGL has no linkable `lv_obj_get_style_<prop>` getters (they are `static
    inline`); readback goes through `lv_obj_get_style_prop`, with the
    `LV_STYLE_*` ids pinned by a drift guard. LVGL also folds `translate_*`
    into the laid-out coords, so `View.getLeft/getTop` subtract it back out —
    otherwise `getX() = getLeft() + getTranslationX()` double-counts.
  - A *delayed* start captures its `from` lazily when the delay expires and
    only then retires a running animation of the same property; an immediate
    start replaces it at once (Android's rule). Without the lazy capture the
    pulse idiom (`alpha(0.35f)` then `alpha(1f).setStartDelay(180)`) reads
    `from = 1.0` and never dips.
  - Rotation/scale switch the object to an ARGB8888 transform layer from the
    LVGL pool (64 KB default): a 60×30 tile is 7 KB, a full screen 225 KB and
    impossible. Documented, not guarded.
  - `getAlpha()` stays field-backed (exact floats, no per-View heap); the
    animator writes the alpha *target* into it on `start()`. The other
    getters read LVGL (exact in the units written: 0.1°, 1/256).
  `Activity.onCreate(Bundle)` joined with T3.1 (2026-09-19). Not done,
  same pattern available: `EditText extends TextView`, `CompoundButton extends
  Button`, hoisting `startActivity` onto `Context`.

## Tier 3 — large milestones

- **T3.1 — Bundle + instance state. DONE 2026-09-19 (A, B, and C through
  `recreate()`).** (A) `picodroid.os.Bundle` — boolean/int/long/float/double/
  String, `int[]`/`byte[]`/`String[]`, nested Bundles, Android's
  mismatch-gives-the-default rule, `keySet`/`putAll`/copy constructor — and
  `Intent.putExtras`/`getExtras` (a copy, as on Android) plus `putExtra(long)`/
  `putExtra(Bundle)`/`getLongExtra`/`getBundleExtra`. (B) `protected void
  onCreate(Bundle)`. (C) `onSaveInstanceState`/`onRestoreInstanceState` and
  `Activity.recreate()`: old `onPause → onStop → onSaveInstanceState →
  onDestroy`, new `onCreate(saved) → onStart → onRestoreInstanceState →
  onResume`, in the same stack entry, so the launch Intent and a pending
  for-result launch carry over at any stack depth. `examples/bundledemo` is
  the conformance app (nightly row, all three shrink modes).
  *What differed from the plan:*
  - Bundle is **Java-side, not native-backed**: a lazily allocated
    `String[]`/`Object[]` pair with boxed primitives, so the getters
    type-check with `instanceof` and need no tag array. A native store would
    have needed the JSON pool's machinery (wrapper-death hook, GC-pacer
    charging) *and* GC roots for nested Bundles and arrays, to save a class
    file. `Intent` lost its own five-field extras table to it (which paid
    for its five new methods: `Intent.class` stayed at 3.0 KB),
    `fields::intent::PACKAGE` moved from slot 6 to 2, and `PendingIntent`
    reads extras through `getExtras()`/`keySet()` instead of the four index
    accessors Intent used to expose for it. RP2040 release flash, measured:
    836,968 → 843,504 (+6,536 B; 73.7 KB of the program region left).
  - The three Bundle callbacks are entered through `final` trampolines on
    Activity (`performCreate`, `performSaveInstanceState`,
    `performRestoreInstanceState`; `DISPATCH_SITES` rows), not by name on
    the app's class. The native lookup is flat and descriptor-blind: it
    could not tell `onCreate()` from `onCreate(Bundle)`, and it never found
    an override declared on an app's *base* Activity class. The
    trampoline's `invokevirtual` does both. The other lifecycle callbacks
    still go by flat name and still have the base-class blind spot.
  - `onCreate()` stayed at first as a deprecated bridge (the default
    `onCreate(Bundle)` called it); T3.1-E below migrated every Activity and
    removed it.
  - `cut-app` had a latent bug this exposed: it renamed any app member
    missing from the release map, including an override of an SDK method
    declared *since* that map was cut (`onSaveInstanceState` → `EZ`, while
    the framework still called the verbatim name). Names in
    `member-names.tsv`/`api-contract.tsv` are no longer candidates. The
    Gradle `cutAppShrinkMap` task does not list the shrinker as an input, so
    a stale per-app map survives a tool change until the app's `build/` goes.
  *Not done:* the framework never destroys a *covered* Activity, so
  `recreate()` is the only producer of a non-null Bundle. Reclaiming parked
  Activities under LVGL-pool pressure (and an Android-style "don't keep
  activities" switch to test it) is the natural follow-up; it needs the
  stack to identify a for-result caller by entry rather than by `obj_ref`,
  which a destroyed entry no longer has. The default `onSaveInstanceState`
  saves no view state (no view ids until T3.2), and nothing is carried
  across a cross-package re-entry (app-store roadmap S7), where the heap is
  reset.
- **T3.1 follow-ups (pending).**
  - **T3.1-D — reclaim covered Activities. DONE 2026-09-19.** A covered
    Activity is destroyed (`onSaveInstanceState` → `onDestroy`, parked view
    tree freed, Service bindings dropped) and its stack entry kept — launch
    Intent, for-result metadata, the Bundle. Uncovering it starts a new
    instance: `onCreate(saved)` → `onStart` → `onRestoreInstanceState` →
    (`onActivityResult`) → `onResume`, no `onRestart`. Checked at both ends
    of every push, oldest entry first, while the LVGL pool has under 1/8
    free or the native heap under 1/16 — late on purpose, since most apps
    save nothing and a re-created screen loses what was not saved. "Don't
    keep activities" reclaims every Activity as soon as it is covered:
    `PICODROID_DONT_KEEP_ACTIVITIES=1`, read at start by the simulator and
    baked in at build time on a device (`lifecycle::set_dont_keep_activities`
    is the runtime setter a pdb verb would call; no verb yet).
    `examples/reclaimdemo` is the conformance app; its `test.env` turns the
    switch on for its nightly row (`lib.sh::app_test_env`, new: sim-run
    applies it at run time, hil-run at firmware build time). Each stack
    entry has a token that outlives its instance; a for-result launch
    records the caller's token when it is queued, `getIntent`/`setResult`/
    `finish`/`recreate` and the GC's instance roots skip a destroyed entry,
    and a result Intent on its way to a re-created caller is rooted on the
    stack meanwhile. *Not done:* pressure is only looked at on a push, so an
    allocation failing mid-screen does not trigger a reclaim; the pressure
    path has no test of its own (the switch drives the same code).
  - ~~**T3.1-E — migrate the Activities** from `onCreate()` to
    `onCreate(Bundle)`, then decide whether the bridge goes.~~ **DONE
    2026-09-19; the bridge went.** The "~150" was every `onCreate()` in the
    tree: 83 of those are `Application`s and 9 are `Service`s, whose no-arg
    `onCreate()` *is* the Android shape and stays. 63 Activities moved (54
    Java, 9 Kotlin, the two `tools/kotlin-survey` fixtures among them) to
    `protected void onCreate(Bundle)` + `super.onCreate(savedInstanceState)`,
    and 13 website snippets with them. The bridge was removed rather than
    kept: it had no Android counterpart, and keeping it meant two spellings
    in every tutorial. The cost of removing it is one silent failure mode —
    an out-of-tree Activity that declares `onCreate()` *without* `@Override`
    still compiles and is then never called — so `verifyApiContract` gained
    a retired-callbacks table (`ApiContract.RETIRED_CALLBACKS`): an app class
    below `picodroid/app/Activity` declaring `onCreate()V` fails the build
    with the replacement spelled out. Also fixed here: `qa_life` still used
    the four `Intent` index accessors T3.1 deleted (`extraCount`/`extraKey`/
    `isIntExtra`/`extraInt`), which broke the every-APK CI build on main;
    it now reads `getExtras()`.
  - ~~**T3.1-F — trampolines for the remaining lifecycle callbacks**
    (`onStart`/`onResume`/`onPause`/`onStop`/`onDestroy`/`onRestart`/
    `onActivityResult`/`onBackPressed`): they still dispatch by flat name and
    miss an override declared on an app's base Activity class.~~ **DONE
    2026-09-19.** Eight more `final` `perform*` trampolines on `Activity`,
    and their `DISPATCH_SITES` rows now name them; `lifecycle.rs` makes one
    call on `picodroid/app/Activity` instead of trying the app's class by
    name and falling back. `bundledemo` moved `onStart`/`onPause`/`onStop`/
    `onDestroy` and the launcher's `onActivityResult`/`onRestart` onto base
    classes; against the old dispatch it fails. `onBackPressed` has no
    conformance row (it needs an injected BACK key) but takes the same path.
    The eight names are unmapped until the next shrink-map cut. `Service`
    callbacks still dispatch by flat name — same blind spot, not in scope
    here.
- **T3.5 — `Canvas`/`onDraw`** — optional, RP2350-only, last. LVGL's canvas
  buffer is W×H×2 (112.5 KB at 240×240): impossible on RP2040, tight on
  RP2350. Needs E2. Only on concrete app demand.
