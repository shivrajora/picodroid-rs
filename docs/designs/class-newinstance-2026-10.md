# Reflection-lite: `Class.forName` and `Class.newInstance`

**Status: built 2026-10-09** (decision F2 of
[claudeusage-decisions-2026-10.md](claudeusage-decisions-2026-10.md)). Amendments at the bottom
record where execution diverged from the plan.

## 0. Why

`java.lang.Class` exposed `getName()` and nothing else, on the grounds that reflection is out of
scope on a 160 KB heap. One decision, three overrides in every app that is more than one
Activity, each of them something Android does by reflection:

- `Activity.getDefaultViewModelProviderFactory()`, a factory that `new`s the one ViewModel class;
- `FragmentManager.setFragmentFactory(…)`, a factory that compares the saved class name against
  string literals and calls the matching constructor, without which re-created fragments are
  silently dropped;
- `Activity.onCreateView(String, Context, AttributeSet)`, the `LayoutInflater.Factory` that does
  the same for the custom views a layout names.

`claudeusage`, `fragmentdemo` and `layoutdemo` carried all three. The decision doc's estimate was
"the cheapest item here with the widest reach".

## 1. What changes for an app

Nothing an Android app would not already do; three things it no longer has to:

| Before | After |
|---|---|
| Override `getDefaultViewModelProviderFactory()` | A ViewModel with a public no-argument constructor is made by `ViewModelProvider.NewInstanceFactory`, the default, as on Android. |
| Install a `FragmentFactory` before `super.onCreate` | Fragments with public no-argument constructors are re-created by name through the default factory. |
| Override `onCreateView(String, Context, AttributeSet)` | A custom view class with a public `(Context, AttributeSet)` constructor is inflated by name; the override is still consulted first, as on Android. |

`Class.forName(String)` and `Class.newInstance()` are public with their JDK signatures (and the
JDK's checked exceptions: `ClassNotFoundException`, `InstantiationException`,
`IllegalAccessException`, under `ReflectiveOperationException`, all new builtins). The example
apps keep their classes `public` with public constructors, because that is what Android's
reflection would require; this runtime does not check access.

## 2. What it is not

- No `Constructor`, `Method` or `Field`; no `getConstructor(…)`, `getDeclaredMethods()`,
  annotations at run time. A `Class` is a name; a constructor is found by descriptor and run as a
  frame.
- No class loading. `forName` finds a class packed with the app or the framework's, or a
  classfile-less builtin (`String.class`); nothing is read from anywhere.
- No access checks: `newInstance()` on a class with a package-private constructor constructs it,
  where Android throws `IllegalAccessException`. `IllegalAccessException` exists so the JDK's
  `throws` clause compiles and a `catch` written for Android resolves.

## 3. How it works

### 3.1 Prechecks, not handler arms

`crates/jvm/src/interpreter/ops_reflect.rs`. A native handler arm receives a `NativeContext` and
cannot run Java; `newInstance` must run `<init>`, and `<clinit>` first when the class was never
touched. The interpreter already serves `Object.getClass`, `Enum.valueOf` and `ArrayList.sort`
the same way: `helpers::precheck_flag` marks the site, `native_prechecks` in `ops_invoke.rs`
routes `(java/lang/Class, forName)`, `(java/lang/Class, newInstance)` and
`(picodroid/view/LayoutInflater, nativeNewView)` to it before any handler sees the call.

- **`forName`**: dotted name to slashes, `find_class`, else the `BUILTIN_CLASS_NAMES` list, then
  `class_object_for_name`, the same cache `ldc` and `getClass()` use, so `forName(X.class.getName())
  == X.class`. Not found: `ClassNotFoundException` with the name as its message.
- **`newInstance`** and **`nativeNewView`** share `construct`: the class index from the Class
  object's name, refuse an interface or abstract class or a builtin, `find_method_in` for the
  exact `<init>` descriptor (`()V`, or `(Context, AttributeSet)V`), then initialise the class if
  it never was, allocate (`alloc_instance`), build the frame with the receiver and the arguments,
  and run it to completion the way an upcall runs a callee (`run_frames`, with the upcall-depth
  and frame-depth limits). The object is the result.
- **`<clinit>` from a precheck.** `op_new` rewinds its instruction and lets the main loop run the
  queued `<clinit>` frames; a precheck has no instruction to re-execute, since its arguments are
  already off the operand stack. `ensure_class_initialized_at` queues the frames and marks the
  class; `construct` takes the queue and runs it synchronously, root class first. A constructor
  reference from a native upcall (`ops_indy.rs`) refuses an uninitialised class for the same
  reason; it could now do this instead.
- **Allocation failure** is the hard `StackOverflow`, which `finalize_native` turns into a
  collection and a re-execution of the invoke; the class is initialised by then, so the second
  pass only allocates.
- **Refusals**: `newInstance` throws `InstantiationException` (the class name as message);
  `nativeNewView` returns `null` and `LayoutInflater` throws its own `InflateException` naming the
  layout element and the missing constructor, as Android's message does.

### 3.2 The tables

`BUILTIN_SDK_HANDLED` has the two `Class` rows (they are exact contract rows, since `Class.java`
declares the natives), `CLASS_METHODS` lists them as interpreter-served, and the builtin method
contract test allows them, as it does `getClass`. The inflater native is a picodroid class, so its
row is in picodroid-core's `CORE_HANDLED`, with an anchor arm in `native_handler/res.rs` that the
member-const guard sees and the precheck keeps from ever running. Four exception classes join
`BUILTIN_CLASS_NAMES` and `BUILTIN_SUPER`; three descriptors join `sdk/descriptors.tsv`.

### 3.3 The shrinker

Under plain `--shrink` (the nightly's second mode) app classes keep their names, so a layout's
class string and `forName` agree. Under `--shrink-app` the app's classes are renamed to `c/X`;
`getName()` already returns the renamed form, so saved fragment state round-trips, and
`papk-pack` now writes a layout's custom class name through the shrink map
(`res::compile_with`, from `compile_resources`), the way `shrink_entry_point` maps the manifest's
class. No link roots are involved: the packer packs every class under `--classes-dir`, so a class
only a layout names was never at risk of being dropped (the decision doc assumed otherwise).

### 3.4 The SDK

- `ViewModelProvider.NewInstanceFactory` (Android's name): `modelClass.newInstance()`, a
  `RuntimeException("Cannot create an instance of …")` when that fails. The owner constructor
  falls back to it; a null factory means it too; the `IllegalStateException("No
  ViewModelProvider.Factory…")` path is gone. `Activity.getDefaultViewModelProviderFactory()`
  returns it.
- `FragmentFactory.instantiate(String)`: `Class.forName(className).newInstance()`, a
  `RuntimeException` with Android's wording on failure. `FragmentManager.restoreSaveState` no
  longer warns and skips when no factory was set; `getFragmentFactory()` always has one.
- `LayoutInflater.custom(name)`: the Factory first (an Activity's `onCreateView`, null by
  default), then `Class.forName` and the private static native `nativeNewView`.
- `Class.java` documents the surface; the "reflection is out of scope" sentence is gone from it
  and from the guides.

## 4. Verification

- `examples/classlit` (the `term` row in `hil-tests.conf`): `forName` by `getName()` (so it holds
  under `--shrink-app`), `forName` of a builtin, a miss, `newInstance` twice with the static
  initialiser observed, and the abstract, no-constructor and builtin refusals.
- `examples/layoutdemo` on `pico_touch_kit` (`=== ALL PASSED ===`): the `Gauge` views are made by
  the inflater, with an empty `AttributeSet`, and the Activity no longer overrides
  `onCreateView`.
- `examples/fragmentdemo`: `HomeFragment` and `DetailFragment` re-created after `recreate()`
  through the default factory; `DemoViewModel` made by the default factory, counted in its
  constructor.
- `examples/claudeusage`: its three custom views and the ViewModel are made by reflection; both
  overrides are gone (tracker row 57 closes).
- `cargo test -p pico-jvm` (the builtin method contract, the hierarchy registration) and the
  picodroid-core native-table guards; `cargo test -p papk-pack`.

## 5. Amendments

- 2026-10-09: the plan said "no `forName`". Two of the three consumers (`LayoutInflater`,
  `FragmentFactory`) only ever hold a class *name*, so a name-to-class lookup is the whole
  point; `forName` is the JDK's spelling of it and costs one precheck. Still no member discovery.
- 2026-10-09: the two-argument view form is a framework-private native on `LayoutInflater`
  rather than a second public `Class` method, so `java.lang.Class` keeps exactly the JDK's
  surface.
- 2026-10-09: the inflater native's table row needs a dispatch anchor in picodroid-core
  (`handled_rows_use_member_consts` scans only that crate's handlers), hence the never-reached arm
  in `native_handler/res.rs`.
