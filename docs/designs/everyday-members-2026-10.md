# Everyday members: the small Android idioms every port trips on

**Status: built 2026-10-19 — all nine stages (§3); verification in §4.**

## 1. The finding

The 2026-10 review of picodroid against Android, after the broadcasts and layouts rounds, listed
a dozen members that are each small and each break a straight port:

> `View.getContext()` (every custom view on Android calls it; `RingView` has to be handed the
> context), `View.post(Runnable)`, `View.onTouchEvent`, `ViewGroup.onLayout` and `requestLayout`
> (custom ViewGroups are impossible today), `Double.isNaN`/`isInfinite`/`MAX_VALUE`,
> `StringBuilder.insert`/`setLength`/`deleteCharAt`, the `java.util.function` SAM interfaces,
> `OnKeyListener.onKey` with its keyCode, `Lifecycle.State` as an enum with
> `DefaultLifecycleObserver`, `MediatorLiveData`/`Transformations.map` (`UsageViewModel` derives
> state by hand), `HttpURLConnection.getInputStream()` declared as `InputStream` now that the
> hierarchy exists, and `EditText extends TextView`.

Decisions taken with the owner before the work: the three signature changes are **hard breaks**
with the in-repo apps fixed in the same commit (no compatibility shims in flash); touch delivery
is **always attached** to a view made with `View(Context)`, not only when `onTouchEvent` is
overridden; the cheap siblings are in (`StringBuilder.delete/setCharAt/reverse`,
`Float.isNaN/isInfinite`, `Double.isFinite`, the `Int*`/`Bi*` SAMs, `Lifecycle.Event` and
`LifecycleEventObserver`, `Transformations.switchMap`, `ViewGroup.removeViewAt/indexOfChild`).
The standing rule holds: no `Handler`/`Looper`/`postDelayed`; `View.post` is the main executor's
queue and `postDelayed` keeps its runtime hint.

## 2. What was true when the work started

- `View` stored no `Context`; every widget's `(Context)` constructor discarded it.
- Touch reached Java only through `OnTouchListener`; a view had to register one to be hit-tested.
- Layout was LVGL's alone: no `onLayout`, `requestLayout` or `layout()`, and `ViewGroup`'s only
  constructor took a native handle, so a custom group could not be written.
- `Double`/`Float` served no predicates (two `CONTRACT_HINTS` rows told apps to compare a NaN with
  itself); `StringBuilder` served `append`, `length`, `charAt`, `toString`.
- Nothing in `java.util.function` existed (a hint pointed apps at their own interface).
- `OnKeyListener.onKey(View, KeyEvent)` had two arguments; `Lifecycle` states were `int`s with no
  observers but `LiveData`'s wrapper; `HttpURLConnection` returned `HttpInputStream`;
  `EditText extends View`.

## 3. What landed, stage by stage

Commits on `main`, one per stage, in this order.

1. **`java.lang` builtins** (`jvm`): `Double`/`Float.isNaN`, `isInfinite` (static and instance),
   `isFinite`; `StringBuilder.insert` (every `append` kind; an `Object`/`CharSequence` argument is
   stringified by the interpreter first, as for `append` — `site_flags` and `stringify_object_arg`
   name `insert` too), `setLength` (NUL-padded when growing), `delete` (end clamped),
   `deleteCharAt`, `setCharAt`, `reverse`, over byte-buffer editing ops in `sb_store.rs`. The
   compile-time constants (`MAX_VALUE`, `NaN`, …) need nothing: javac inlines them
   (`qa_lang` proves it).
2. **`java.util.function`** (`jvm`): sixteen name-only rows (`BUILTIN_CLASS_NAMES` +
   `BUILTIN_INTERFACE_METHODS`; `UnaryOperator`/`BinaryOperator` also as `BUILTIN_INTERFACES`
   edges for `instanceof`). No class file, no dispatcher: the default methods (`andThen`,
   `negate`, …) do not exist. The compile-time contract wants the inherited `apply` row on the
   operator interfaces themselves, as `Closeable.close` sits beside `AutoCloseable.close`.
3. **`OnKeyListener.onKey(View, int keyCode, KeyEvent)`**: `View.fireKey` passes
   `event.getKeyCode()`; keydemo and qa_ui moved with it. Rust unchanged.
4. **`Lifecycle.State` / `Event` enums, `LifecycleObserver`, `DefaultLifecycleObserver`,
   `LifecycleEventObserver`**: `setCurrentState` walks the intermediate states one event at a
   time (`INITIALIZED → DESTROYED` passes through `CREATED`, as `LifecycleRegistry` does),
   `handleLifecycleEvent`, `addObserver` replays the events up to the current state, a
   `Lifecycle(LifecycleOwner)` hands that owner to its observers (the Activity's does).
   `LiveData.ObserverWrapper` is a `LifecycleEventObserver`. `Activity`, `Fragment`,
   `FragmentManager` and fragmentdemo moved to the enum; the int constants are gone.
5. **`MediatorLiveData` + `Transformations.map` / `switchMap`**: sources are `observeForever`'d
   while the mediator is active and unplugged after, with Android's once-per-version delivery
   across a replug (`LiveData.getVersion()`, package-private). `claudeusage`'s `UsageViewModel`
   is a `MediatorLiveData` with one source.
6. **`HttpURLConnection` streams as `java.io` types**: `getInputStream()`/`getErrorStream()`
   return `InputStream`, `getOutputStream()` `OutputStream`; the concrete classes stay behind
   them. Six apps that named the concrete type were fixed.
7. **View**: `Context mContext` is View's 25th instance field (`fields.rs::view::CONTEXT = 24`;
   `text_view`'s slots moved to 25..30); `protected View(Context, int)` threads it through every
   widget's `(Context)` constructor and the inflater stamps the views its native path makes;
   `getContext()` answers it or the parent chain's. `post(Runnable)`. `onTouchEvent` /
   `dispatchTouchEvent` / `MotionEvent.obtain`: `View(Context)` calls `nativeEnableTouch()`
   (the touch binding without a listener object), so a custom view is hit-tested from
   construction. **Custom ViewGroups**: `ViewGroup(Context)` creates a flat container
   (`frame_layout::create`), sets `mCustomGroup` and `nativeWatchSize()`s it; `onLayout` runs on
   the main thread after `addView`/`removeView`, after a size change (an `LV_EVENT_SIZE_CHANGED`
   trampoline queues the handle in `events/layout.rs`; `dispatch_layout_requests` fires the new
   `VIEW_GROUP_LAYOUT` dispatch site, `ViewGroup.fireLayout`) and on `requestLayout()`, which
   bubbles to the nearest such ancestor. `View.layout(l, t, r, b)` places a child
   (`setPosition` + `setSize`, then the child's own `onLayout` if it is a custom group);
   `measureChild`/`measureChildren`/`getChildMeasureSpec`; `View.measure` on a framework widget
   sizes it as the spec says and reports what the renderer settled on, within the spec.
   `getRight`/`getBottom`, `removeViewAt`, `indexOfChild`.
8. **`EditText extends TextView`**, through TextView's handle constructor. TextView's private
   natives are `invokespecial`, so they reach `text_view.rs` with a textarea handle:
   `textarea_of(id)` routes `setText` to `lv_textarea_set_text`, puts the font and alignment on
   the text area itself (the cursor and placeholder measure with them), maps `setSingleLine` to
   `lv_textarea_set_one_line`, and leaves a field's padding alone. `EditText.setText`/`getText`
   stay its own natives (`getText` narrows to `String`; the javac bridge is ordinary bytecode).
   The inflater's `android:textSize/textColor/gravity/singleLine` now apply to an `<EditText>`.
9. **Docs and the ratchet**: `api/ui.md`, `api/core.md`, the compatibility matrix, the roadmap
   amendment, this document; the size ratchet accepted for both testbench boards.

## 4. Verification

- `./scripts/test.sh` green (both shrink lanes), including the new `string_builder`/`boxed`
  tests, the field-table rows (`View.mContext`, `EditText.mText`/`mLineMode`), the method-table
  and dispatch-site resolution tests.
- Sim rows, both shrink modes: `qa_lang`, `qa_coll`, `qa_oop` (new `functional` section),
  `qa_ui` (272 checks; the custom-group, context, post and touch checks live in
  `CustomGroupChecks.java` because `QaUiActivity.class` sits at the PAPK's 64 KB class limit),
  `keydemo`, `fragmentdemo` (observer kinds, replay, event order, `map`, mediator, `switchMap`),
  `claudeusage` (three boards), `https_get`, `picoenvmon`, `callbacktest`, `layoutdemo` (10 rows),
  `calculator` (10), `picoclock`, `keyboarddemo` (new row), `ellipsizedemo`.
- The whole `hil-tests.conf` matrix in the sim and on the bench: see the handover notes in the
  merge commit.

## 5. Traps met

- **A member name is a Rust literal's enemy**: `Lifecycle`'s private `up()`/`down()` helpers
  failed `no_original_name_literals` on the sim display driver's `"up"`/`"down"` strings; renamed
  to `stepUp`/`stepDown`. `Predicate.test` was safe only because `test_platform.rs` is excluded
  from the scan by its file name.
- **A bound view that nothing reaches is reported**: with `View(Context)` bound from
  construction, qa_ui's throwaway `new View(this).isInTouchMode()` probe tripped the handle
  sanitizer on the next GC. A probe is `close()`d now; an app's throwaway custom view would be
  reported the same way (its widget leaks either way).
- **The 64 KB class limit**: `QaUiActivity.class` went to 67 KB with the new sections;
  `papk-pack` refuses a class over 65535 bytes. Split into a second class.
- **`InflaterSupport.adopt(cls)` returns null** for the classes the Java path makes; stamping the
  context on its result needs the null check (layoutdemo's inflate hit it).
- **A deferred check must outlive the fixture**: `root.removeView(flow)` right after
  `requestLayout()` released the group before the queued layout ran; the removal moved into the
  deferred check.
- **The ratchet lanes**: two one-block unsafe rises (`Core0` static of the layout ring,
  `lv_textarea_set_one_line`) each needed `check-source-guards.sh --unsafe-accept` in the commit.
