# Roadmap: Android API parity — 2026-08-18

**Goal:** grow the `picodroid.*` Java API toward its `android.*` counterpart —
same class names, same method signatures, same semantics — so an Android
developer's code and intuition transfer directly. Apps always import
`picodroid.*`; the `android.*` alias layer was built and reverted on
instruction (`6663c4c`, and `CLAUDE.md`), and is not coming back. This doc is
the standing tracker for that work, in the shape of
`docs/networking-followups-2026-08.md`.

Completed items: [completed/android-parity-roadmap-2026-08.md](../completed/android-parity-roadmap-2026-08.md) — E1, E2, T1.1, T1.2, T1.7, the shipped "High priority — next" items 1–2, T2.1, T2.2, T2.3, T2.4, T2.5, T2.6, T2.7, T3.1, T3.1-D, T3.1-E, T3.1-F, T3.5.

Written against the tree at `1a014ec`. The surface it audits: 100 classes
across 19 `picodroid.*` packages, 11 `java.*` stubs, ~25 JVM-native builtins,
~308 native methods, 57 example apps.

## Why now

Three structural findings dominate everything below.

1. **There is no compile-time API contract.** Apps compile against the host
   JDK's full `java.*` — only `picodroid.*` comes from `:sdk`
   (`build.gradle.kts` sets `--release 8`; `PicodroidPapkPlugin.kt` adds only
   `:sdk`). So `new LinkedList<>()`, `str.matches(…)`, and
   `System.out.println` all compile cleanly and die at runtime with
   `NoSuchMethod`. Android's `android.jar` *is* this contract. Closing it
   costs zero device bytes. **Closed 2026-08-31 — see E3.**
2. **RP2040 flash is the binding constraint.** The release image sits at
   915,663 / 917,248 bytes — **1,585 bytes free**. Every compiled SDK class
   is embedded on every board and loaded at boot
   (`build_support/papk.rs`, `boot.rs`), so a new SDK class costs its full
   `.class` size in flash everywhere, used or not. One mid-sized class does
   not fit. JVM *builtins* (native-backed `java.*` classes with no `.class`
   file) are far cheaper and shared across boards.
3. ~~**Natives cannot call back into Java synchronously.**~~ **Fixed
   2026-08-29** — see § E2 in
   [completed/android-parity-roadmap-2026-08.md](../completed/android-parity-roadmap-2026-08.md). `NativeContext` now carries an `upcall` env and
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

### E3. Compile-time API contract — **phase 1 DONE 2026-08-31** (= T2.1)

**Why this is now the *only* compile-time fence (2026-08-31).** Nothing hides
the JDK: every javac in the tree runs `--release 8` with no `-bootclasspath` /
`--system` override, so `java.*` resolves from `ct.sym` and the SDK's own
`java/**` files are shadowed on the app compile classpath. Apps therefore
type-check against the JDK's *full* `String`, `List`, `Map` and friends while
the device serves a subset — `new TreeMap<>()`, `map.forEach`, `list.removeIf`
all compile and fail at run time. T2.2 confirmed an SDK stub cannot fix this
(javac ignores it); only a post-compile constant-pool check against the
runtime's own tables can.

*Phase 1.* `verifyApiContract`, a post-compile bytecode check in the
`picodroid-papk` pipeline (`buildSrc/src/main/kotlin/picodroid/classfile/ApiContract.kt`
+ `ApiContractTask.kt`, between compile and `packPapk`, also under `check`),
rejects every load-bearing `java/**` / `javax/**` reference an app's classes
make that pico-jvm does not serve: `new LinkedList<>()`, `str.matches(…)`,
`System.out`, `new String(char[])` and `class E extends RuntimeException` +
`e.printStackTrace()` all fail the build with the reason, the call sites and
a hint. The allowlist is **generated**: `sdk/api-contract.tsv` is written by
picodroid-core's `api_contract_is_current` test
(`native_handler/api_contract.rs`; regenerate with
`scripts/gen-api-contract.sh`) from the SDK's `java/**` class files
(descriptor-exact), `BUILTIN_CLASS_NAMES` and the new **`BUILTIN_METHODS`**
table (`jvm/src/native/mod.rs` — the machine-readable form of the builtin
rustdoc table: per-class method names, with descriptor lists only where an
arm is descriptor-guarded and would mis-serve other overloads silently),
`BUILTIN_SUPER` / `BUILTIN_INTERFACES` as hierarchy edges,
`PLATFORM_BUILTIN_METHODS` (`Object.wait/notify/notifyAll`) and
`BUILTIN_INTERFACE_METHODS` (the lambda-targetable SAMs `Runnable.run`,
`Comparator.compare`, `Comparable.compareTo`, `AutoCloseable.close`,
`Iterable.iterator` — T2.2 retired the body-less stubs that declared them).
The committed
file is checked for staleness in both `scripts/test.sh` lanes, so the
contract cannot drift from the runtime; the roadmap's original sources
(`method_tables.rs`, `class_registry.rs`) turned out to be `picodroid/*`-only
— the class files plus the builtin tables are the real `java/**` truth. The
verifier models `dispatch_native`: owner → `@extends` chain → `Object`;
interface owners via any builtin or app implementor; app-typed owners walked
through the app's classes to their `java/**` supertypes.
`-Ppicodroid.apiContract=warn|off` is the escape hatch. Cost **G**: every
new table is test-only or unreferenced at runtime.

Triage of the 72 examples found no latent app bug — the three first-run
failures were verifier gaps (`catch CloneNotSupportedException` is a static
shape every `clone()` override emits, now a `@nameonly` row; `Appendable.append`
arrives with the interface-typed descriptor; `EnumEntries.indexOf` is served
by the shim subtype). It also closes E1's follow-up (E1 is in
[completed/android-parity-roadmap-2026-08.md](../completed/android-parity-roadmap-2026-08.md)) and retires the
hand-maintained `kotlin-shim/jdk-allowlist.tsv` / `ShimContract` Direction C:
the Kotlin fixtures run `verifyApiContract` on their staged shim classes.

Residuals: an arm without a `BUILTIN_METHODS` row is not machine-checked
(it surfaces as a contract failure naming the row to add; the X-macro of
`method-level-native-registry.md` Phase 2 is the structural fix); descriptor
lists are trusted, not cross-checked; an interface member is accepted when
*any* implementor serves it; `Enum.valueOf(Class,String)` (served by the
interpreter since 2026-09-14, `711226ef`) and
`Locale.ROOT/US` are tolerated static shapes; boardless builds (pre-commit,
CI `assemblePapk`) check only the `java/**` contract.

*Phase 2 (optional, later).* A restricted compile classpath — a stub jar of
only the supported `java.*` subset — so IDE autocomplete matches reality
too.

## Tier 1 — quick wins (days each, all N/B)

| # | Item | Cost | Status |
|---|---|---|---|
| T1.3 | Widget fidelity fills | N | open |
| T1.4 | `TextView`/`EditText` text surface | N | **partial** (`getText` shipped 2026-09-02; `setSingleLine`/`setEllipsize`/`setMaxLines` shipped 2026-09-09) |
| T1.5 | Input & sensor fills | N | open |
| T1.6 | `Gpio` input | N | **partial** (`getValue`/`DIRECTION_IN` shipped 2026-09-02; edge callback open) |
| T1.8 | Persistence fills | N | **partial** (`File` name/parent/mkdirs/createNewFile, prefs float shipped 2026-09-02; `File.list`/`listFiles` and the `Context` file API shipped 2026-09-08; `Intent` long/float/double extras and `getStringSet` open) |
| T1.9 | `EditText.setInputType` + password masking | N/S-small | **partial** (see below) |

**Shipped 2026-09-02 — the Tier 1 core set.** `TextView.getText()` (and
`Button.getText()`) returning `CharSequence`; `Gpio.DIRECTION_IN` +
`getValue()` over the existing `HalGpio::set_input`/`read`; `java.util.Objects`
as a body-ful `sdk/java/java/util/Objects.java` (not the "B" the table
promised: `equals`/`hashCode`/`toString` must virtual-dispatch to user
overrides, which only bytecode can do — so it is an S-small class on every
board); `String.join` (varargs and `ArrayList`) and `Float.intBitsToFloat` as
builtins; `File.getName`/`getParent`/`getParentFile`/`getAbsolutePath`/
`mkdirs`/`createNewFile`; `SharedPreferences.getFloat`/`Editor.putFloat`
(blob tag 6, still `VERSION` 1). **Until the next release map is cut** the
`java/util/Objects` name is un-shrunk and `stage_shrink_image` in
`pre-commit --full` reports exactly that one leak; the v0.18.0 cut on `main`
clears it.

**High priority — next (deferred 2026-09-02, in this order):**

3. ~~`TextView.setTextSize(float)` / `(int unit, float)`~~ — **shipped 2026-09-23** over a
   per-board ladder of generated Montserrat faces (`text_sizes`; nearest-face snapping);
   `append`, `setGravity` — `append` is pure Java, `setGravity` an LVGL-side native.
4. `Gpio` edge callback — a `dispatch_sites.rs` row plus a GC-root provider
   for the retained listener (`EXPECTED_PROVIDERS` bump).

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
working either way. Shipped 2026-09-09: `setSingleLine`,
`setEllipsize(TextUtils.TruncateAt)` and `setMaxLines` with their getters,
over LVGL's label long modes and a `max_height` cap of N lines
(`docs/designs/multi-app-2026-09.md`, A5). Shipped 2026-09-23: `setTextSize`
(both overloads), `getTextSize`, `getLineHeight`, with `TypedValue` and
`DisplayMetrics`, over a per-board ladder of generated Montserrat faces
(`docs/designs/claudeusage-gaps-roadmap-2026-09.md`, G1); `setGravity` and
`append` remain.

**T1.5 — input & sensors.** `MotionEvent.ACTION_CANCEL`;
`GestureDetector.onDown`/`onScroll`/`onDoubleTap`;
`SensorManager.getSensorList(int)`.

**T1.6 — Gpio input.** The cheapest hardware gap: `HalGpio` already has
`read`/`set_input`/`enable_edge_irq`, but the Java `Gpio` is output-only.
Needs `getValue()`, `DIRECTION_IN`, and an edge callback — plus a GC-root
visitor for the retained callback, historically this repo's #1 bug class.

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

- **T2.7 residual (open).** T2.7 is done (see the completed doc); its "Not done, same pattern
  available" tail stays open here: `EditText extends TextView`, `CompoundButton extends
  Button`, hoisting `startActivity` onto `Context`.
- **T2.8 — `DatePickerDialog`/`TimePickerDialog`.** Thin S-classes over
  `AlertDialog` plus the existing picker widgets.

## Tier 3 — large milestones

- **T3.1 follow-ups (pending).** D, E and F are done (completed doc). Residuals of those, still
  open: T3.1-D looks at pressure only on a push, so an allocation failing mid-screen does not
  trigger a reclaim, and the pressure path has no test of its own; after T3.1-F, `Service`
  callbacks still dispatch by flat name (the base-class blind spot).
  - **T3.1-G — default view-state save/restore** by view id, once T3.2 gives
    views ids.
  - **T3.1-H — state across a cross-package re-entry** (app-store S7): needs
    a Bundle serialized outside the JVM heap, which is reset there.
  - **Tooling:** make `cutAppShrinkMap` (and `packPapk`) track the
    `class-shrink`/`papk-pack` sources as inputs so a tool change re-cuts.
- **T3.2 — resource system + `R.*` + XML layouts.** **(A)–(C) done
  2026-09-19** — see the amendment at the bottom for what was built and
  where it diverges from this paragraph. The largest remaining
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
  ~~today throws, gated on exactly this milestone~~ works since 2026-09-14,
  `a97321ee`, from a Java-side child list rather than a reverse map). (C) `res/drawable/` →
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

## Ordering

1. ~~E1 gating~~, ~~T1.1~~, ~~T1.2~~ (done)
2. ~~T2.7 shape corrections~~ (done 2026-08-31)
3. Remaining Tier 1 (~~T2.1 verifier~~ done 2026-08-31)
4. ~~T2.2 collection interfaces~~ (done — and it turned out to be a
   stub-retirement plus a hygiene test, not new SDK classes)
5. ~~T2.4 line-number stack traces~~ (done 2026-09-02)
6. ~~T2.6 JSON~~ (done 2026-09-04) + ~~T2.3 Thread parity~~ (done)
7. ~~T3.1 Bundle → `onCreate(Bundle)` → save/restore~~ (done 2026-09-19;
   follow-ups D, E, F done; G, H pending)
8. ~~T2.5 upcall~~ (done) → T3.4 convertView recycling
9. ~~T3.2 resource system (A → B → C)~~ (done 2026-09-19; D — styles and
   `AttributeSet` — remains)
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
- **HTTPS/TLS.** *2026-09-27:* landed RP2350-only as
  `docs/designs/tls-2026-09.md` — `HttpsURLConnection`, `SntpClient`, the
  `javax.net.ssl` exceptions; the RP2040 keeps throwing (10.9 KB of flash
  left). Not mirrored: `SSLSocket`, `setSSLSocketFactory`, `HostnameVerifier`.
- **Full `java.util.concurrent`, `LinkedList`, `TreeMap`, `ArrayDeque`.** No
  demonstrated need; every builtin still costs shared `.text` and table rows.
  *2026-08-30:* the core set landed as **pure Java** in `picodroid.concurrent`
  (`ExecutorService`/`Future`/`Callable`/`TimeUnit`/`FutureTask`, a fixed
  `ThreadPoolExecutor`, `AtomicInteger`/`Long`/`Boolean`/`Reference`,
  `CountDownLatch`) on top of the Thread parity work — zero natives, zero
  `.text`, class files only. What stays out, and what would change the answer,
  is enumerated below.
- **The rest of `ViewGroup`'s child-list API** (*2026-09-25*):
  `addView(child, index)`, `removeViewAt`, `removeViews`, `indexOfChild`, and
  the `onAttachedToWindow` / `onDetachedFromWindow` callbacks. None has ever
  existed here and no app has asked. Since the D3 fix (`2581a272`) the Java
  child list and `View.mParent` are kept consistent by `addView`, `removeView`
  and `close`, so each is a small Java-only addition over `mChildren` (the
  index forms need `lv_obj_move_to_index` for the widget order; the callbacks
  fire from `addView` / `detachChild`). Backlog only: implement one when a
  real app needs it, not as a set.

### Concurrency surface deliberately left out (2026-08-31)

Recorded after T2.3 + the `j.u.c.` core set merged (`a34a639`), so the
decisions outlive the session that made them. Every one of these is a *cost*
call, not a difficulty call: SDK classes are charged to **every** board's
flash, and `testbench_rp2040` already excludes the `j.u.c.` core set via
`framework_class_excludes` to stay under its gate (19.9 KB headroom). Anything
added here has to earn that budget on the smallest board or arrive E1-gated.

| Left out | Why | What would change it |
|---|---|---|
| `ThreadLocal` | Needs a per-task slot map the GC must root, and the natural users (a Looper, a per-thread `StringBuilder`) do not exist. The shared `sb_buf` aliasing hazard is a *separate* bug, fixed in `896f691` (`docs/completed/followups-2026-08.md` § 2) — do not "fix" it by adding `ThreadLocal`. | A framework need for per-task state, or `sb_buf` being retired in favour of per-thread buffers after measurement. |
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

## Amendments

### 2026-09-16 — status sweep against `09e7a8b3`

What reached the Java surface since the table was last touched (2026-09-09),
checked against `sdk/java` and the commits named:

- **T1.8:** `File.list()`/`listFiles()` and `Context.getFilesDir`/
  `openFileInput`/`openFileOutput`/`deleteFile`/`fileList` shipped with multi-app
  M3a (`c50422d1`, 2026-09-08), so items 1 and 2 of the "High priority — next"
  list are done. Still open in T1.8: `Intent` long/float/double extras and
  `getStringSet`/`putStringSet`.
- **`ViewGroup.getChildAt`/`getChildCount`** work (`a97321ee`, 2026-09-14): a
  Java-side child list, not the `lv_obj_t* → ObjectRef` map §"Why now" said it
  needed, and `addView` of a released view throws. T3.2(B) is no longer its gate.
- **Language/runtime, from the 2026-09-13 QA round** (`qa-2026-09-13.md`):
  `Enum.valueOf(Class, String)` (`711226ef`); method references to builtin,
  virtual and constructor targets (`eba21165` — `Foo::new` used to be rejected
  as `REF_newInvokeSpecial`); heap exhaustion is a catchable `OutOfMemoryError`
  in natives, collections and builders (`58f16fc5`, `6da931fc`, `49ed4b3e`);
  the four `IllegalFormat*` subclasses (`e5d5b88c`); `Integer.valueOf` box
  caching, user `equals` in `HashMap`/`HashSet`/`ArrayList`, `list.sort(null)`,
  and ~20 smaller `String`/`String.format`/boxing fixes.
- **Widget semantics from the same round:** `RadioButton.setChecked` keeps its
  group in sync, `SeekBar.setMax` clamps, `setText`/`getText`/`setHint` are no
  longer cut at 127 bytes, `AlertDialog.dismiss()` is idempotent.
- **New API outside the T-table:** `picodroid.app.AlarmManager` +
  `PendingIntent` (`32a271a8`, `designs/alarm-manager-2026-09.md`) and
  `picodroid.media.ToneGenerator` (`90947047`), both 2026-09-11.

Unchanged and still open: T1.3 (none of its list exists — no
`View.getParent`/`getContext`, `removeViewAt`, `indexOfChild`,
`ListView.setSelection`, `ArrayAdapter.remove/insert/getPosition`,
`AlertDialog.setCancelable`/`setOnDismissListener`, `Toast.setGravity`);
T1.4's `TextView.setGravity`, `append` and `EditText.setSelection`
(`setTextSize` shipped 2026-09-23); T1.5 in full; T1.6's edge callback; T1.9's
password masking; T2.8 pickers-as-dialogs; T3.1 Bundle; T3.2 resources/XML
layouts; T3.3 `java.io` streams; T3.4 `getView`/convertView; T3.5 Canvas; E3
phase 2 (restricted compile classpath).

## Amendment 2026-09-19 — T3.2 (A)–(C) landed

`res/values`, `res/layout` and `res/drawable` compile into the PAPK; apps get
a generated `R`, `Context.getResources()`, `setContentView(int)`,
`LayoutInflater`, `findViewById` and `ImageView.setImageResource(int)`.
App-developer documentation is `website/src/content/docs/guides/resources.md`;
`examples/resdemo` checks every call and logs `ResDemo PASS`.

Where the pieces live:

- **Container.** PAPK v1.2: a fourth section, `RESR`, found through a 4-byte
  file-header extension that is written — with `version_minor = 2` — only when
  an app has resources. Every other PAPK stays byte-identical to v1.1, and a
  v1.1 reader handed a v1.2 file reads its three sections from the same slots.
  The table and the layout word stream are specified in
  `crates/papk-format/src/res.rs`, which also holds the zero-copy reader and
  the writer.
- **Compiler.** `tools/papk-pack/src/res.rs`, in Rust rather than in the
  Gradle plugin as the paragraph above assumed: the format's reader, writer
  and compiler then share one crate and round-trip in unit tests. Gradle runs
  it twice over the same `res/` — `papk-pack gen-r` before `compileJava`
  (`GenerateRTask`), `papk-pack --res-dir` at `packPapk` — and ids come from
  sorted names, so `R.java` and the table agree by construction. Both tasks
  declare the compiler's sources as inputs, which closes the "stale PAPK after
  a packer change" gap for apps with resources.
- **Zero-cost `R`.** `R`'s fields are `static final int`, inlined by javac;
  `compileJava` deletes `R*.class` from its output, so `R` is never packed.
- **Runtime.** `crates/picodroid-core/src/resources.rs` is one slice into the
  package in flash. `Resources` is five natives over it
  (`native_handler/res.rs`). `LayoutInflater` is Java: it walks the word
  stream through a single native, `nativeWord(layout, index)`, and builds
  views with their ordinary constructors, so Java-side state (`id`,
  `visibility`, the `ViewGroup` child list, `LayoutParams`) is correct by the
  same code paths as a hand-built tree. A native inflater was rejected for
  that reason: `ObjectHeap::alloc` does not run `<init>`. Only strings are
  resolved at inflation; colours, dimensions and references are numbers by
  then.
- **Flash.** Four SDK classes on every board — `Resources` (747 B stripped),
  `Resources$NotFoundException` (240 B), `InflateException` (175 B) and
  `LayoutInflater`. The inflater's element/attribute codes are literal `case`
  labels named in comments, not `static final` fields, which saves ~2 KB; a
  papk-pack test (`codes_match_the_java_sdk`) reads those comments and holds
  them to `papk_format::res::layout`, along with the `Gravity`, `InputType`
  and `ImageView.SCALE_*` numbers the compiler bakes into layouts.
- **Deliberate limits.** No configurations (qualified directories are a build
  error); the framework widgets and a fixed attribute set only; unknown
  attributes are a build *warning* and dropped, so pasted Android layouts
  build; custom views, `<include>`/`<merge>`, styles, `AttributeSet`, string
  arrays, plurals and `getString(int, Object...)` are not there (D).
  `inflate(id, null)` keeps the root's explicit `layout_width/height` via
  `setSize`, where Android drops them, so `setContentView(R.layout.x)` honours
  a sized root.
- **`API_HINTS`** for `findViewById`, `getResources` and `getLayoutInflater`
  are deleted, with a test that they stay deleted.

## Amendment 2026-09-26 — T3.5 `Canvas` / `onDraw` landed, without a canvas buffer

T3.5 (now in [completed/android-parity-roadmap-2026-08.md](../completed/android-parity-roadmap-2026-08.md)) assumed LVGL's `lv_canvas`, whose W×H×2 buffer is what made it "RP2350-only, last".
It was built instead as a retained display list: `Canvas.drawX` records a 32-byte op per call in
LVGL's pool and an `LV_EVENT_DRAW_MAIN` hook replays them whenever the view is painted, so there
is no buffer and it ships on every board. It does not need E2 either: `onDraw` runs from a
main-executor task that `invalidate()` posts, not as an upcall from the renderer. The concrete
demand was `claudeusage`'s two charts (G4). Design, costs and what is left out:
[`canvas-2026-09.md`](canvas-2026-09.md).

## Amendment 2026-09-27 — `Fragment` and `ViewPager2` landed

"`Fragment` before the resource system" (Not doing, above) is retired: T3.2 (A)–(C) gave
fragments layouts and ids on 2026-09-19, and the first app that needed them was `claudeusage`
(row 1 of `claudeusage-android-shape-2026-09.md`, four hand-rolled pages in a `FrameLayout`).
Landed as pure Java in `picodroid.app` (`Fragment`, `FragmentManager`, `FragmentTransaction`,
`FragmentFactory`) driven from `Activity`'s `perform*` trampolines, and `picodroid.widget`
(`ViewPager2`, `FragmentStateAdapter`) over them, with `<ViewPager2>` as layout class 18. Cost
class **S**, RP2350 boards only (`testbench_rp2040` excludes the six classes; `Activity` resolves
the manager lazily). Design, deviations and measurements:
[`fragments-2026-09.md`](fragments-2026-09.md).
