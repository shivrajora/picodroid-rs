---
title: "Graphics and UI"
description: "Activity lifecycle, widgets, theming, gestures, and animations."
---

Picodroid's Android-inspired UI toolkit. Packages: `picodroid.app`, `picodroid.graphics`, `picodroid.view`, `picodroid.widget`. See [Java API overview](/api/) for the full API index.

The toolkit is backed by [LVGL](https://lvgl.io). Apps create an `Application`, start an `Activity`, and build a widget tree. On hardware, the display is driven via SPI (ST7789) with touch input (XPT2046). In the simulator, a graphical window (minifb) renders the UI with mouse-as-touch input.

## `picodroid.app.Application`

Base class for all apps. Subclass it and override `onCreate()`.

```java
import picodroid.app.Application;
import picodroid.content.Intent;

public class MyApp extends Application {
    public void onCreate() {
        // Console app: do work here
        // Display app: start an Activity
        startActivity(new Intent(MyActivity.class));
    }
}
```

| Method | Description |
|--------|-------------|
| `onCreate()` | Called by the runtime after instantiation. Override to initialize your app. |
| `startActivity(Intent intent)` | Launches the Activity named by the Intent's target class (`new Intent(MyActivity.class)`). The Activity's `onCreate()` is called after the display is ready. |

## `picodroid.app.Activity`

Base class for display screens. Subclass it, override `onCreate(Bundle)`, build a widget tree, and call `setContentView()`.

```java
import picodroid.app.Activity;
import picodroid.os.Bundle;
import picodroid.view.View;
import picodroid.debug.DisplayDebug;

public class MyActivity extends Activity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        DisplayDebug.calibrate();     // optional: run touch calibration (debug helper)
        // ... build widget tree ...
        setContentView(rootView);     // render the widget tree
    }
}
```

### Lifecycle

The full Android-style lifecycle is dispatched by the runtime. Override only the callbacks you need.

| Callback | When |
|----------|------|
| `onCreate(Bundle savedInstanceState)` | Once, after instantiation. Build the UI tree here. The argument is `null` on a fresh launch, and the Bundle filled by `onSaveInstanceState` when the Activity is being [re-created](#saved-instance-state). |
| `onStart()` | After `onCreate`, and on every return to the foreground. |
| `onResume()` | Immediately after `onStart`; the Activity is now interactive. |
| `onRestart()` | When the Activity returns to the foreground after being stopped (the Activity above it finished), before `onStart`. Not called on the first launch. |
| `onPause()` | When another Activity is being launched on top. |
| `onStop()` | After `onPause`, once the new top Activity is fully resumed. |
| `onDestroy()` | Just before this Activity is popped off the stack. |
| `onBackPressed()` | BACK-key default action — pops the [fragment back stack](#picodroidappfragment) if it has an entry, else calls `finish()`. Override and don't `super.onBackPressed()` to suppress (e.g. show a confirm dialog). |
| `onKeyDown(int keyCode, KeyEvent)` / `onKeyLongPress(int keyCode, KeyEvent)` / `onKeyUp(int keyCode, KeyEvent)` | A hardware key no focused view consumed: its press and auto-repeats, its long-press, its release; return `true` to consume it. The defaults track BACK so its release runs `onBackPressed`. `Activity` implements `KeyEvent.Callback`. See [Key events](#key-events). |

The content view installed in `onCreate` (or `onResume`) is **preserved across pause** — when this Activity returns to the foreground, the saved widget tree is restored automatically. Rebuilding the tree from `onResume` is still supported; the new root replaces the saved one.

:::caution[Migrating from `onCreate()`]
Activities used to override a no-argument `onCreate()`. It is gone: override `protected void onCreate(Bundle savedInstanceState)` and call `super.onCreate(savedInstanceState)`. The build rejects an Activity that still declares the old one (`api contract: … callback retired`), because without `@Override` it would compile and then never be called. `Application.onCreate()` and `Service.onCreate()` are unchanged — they take no argument on Android either.
:::

### Saved instance state

`recreate()` destroys the foreground Activity and starts a new instance of it in the same back-stack slot, the Android way of rebuilding a screen from scratch. State crosses over in a [`Bundle`](#picodroidosbundle):

| Callback / method | When |
|-------------------|------|
| `recreate()` | Ask for the re-creation; it happens after the current callback returns. Foreground Activity only. |
| `onSaveInstanceState(Bundle outState)` | On the old instance, after `onStop` and before `onDestroy`. Put what the next instance needs into `outState`. Not called when the Activity is finishing. |
| `onRestoreInstanceState(Bundle savedInstanceState)` | On the new instance, after `onStart` and before `onResume`, with the same Bundle `onCreate` received. Never called on a fresh launch. |

Full order: old `onPause → onStop → onSaveInstanceState → onDestroy`, then new `onCreate(saved) → onStart → onRestoreInstanceState(saved) → onResume`. `getIntent()` and a pending `startActivityForResult` launch carry over to the new instance; a `setResult` made by the old one does not.

```java
@Override
protected void onSaveInstanceState(Bundle outState) {
    outState.putInt("count", count);
}

@Override
protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    count = savedInstanceState == null ? 0 : savedInstanceState.getInt("count");
}
```

One difference from Android: the default `onSaveInstanceState` saves nothing — there are no view ids to key a view hierarchy's state by, so an `EditText`'s text is yours to save. See [`examples/bundledemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/bundledemo).

The framework can also destroy a **covered** Activity to get its memory back. A covered Activity normally keeps its instance and its hidden view tree; when a `startActivity` finds the LVGL pool or the heap nearly full, the Activities underneath are destroyed oldest first — `onSaveInstanceState → onDestroy` (they are already stopped) — and each is re-created when the user comes back to it: `onCreate(saved) → onStart → onRestoreInstanceState(saved) → onResume`, with no `onRestart`. `getIntent()` is unchanged, and a `startActivityForResult` answer still arrives, on the new instance, between `onRestoreInstanceState` and `onResume`. Fields you did not save are gone, so an Activity that can be covered should save what it cannot rebuild.

To test that, turn on the equivalent of Android's *Don't keep activities* developer option, which destroys every Activity the moment it is covered: run the simulator with `PICODROID_DONT_KEEP_ACTIVITIES=1`, or set that variable when building device firmware. See [`examples/reclaimdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/reclaimdemo).

### Back stack

| Method | Description |
|--------|-------------|
| `startActivity(Intent intent)` | Push the Activity named by `new Intent(this, TargetActivity.class)` onto the stack. Triggers this.onPause → newActivity.{onCreate,onStart,onResume} → this.onStop. |
| `finish()` | Pop this Activity. Triggers onPause → onStop → onDestroy on this Activity, and onRestart/onStart/onResume on the one below. If the stack is empty after the pop, the app exits. |
| `startActivityForResult(Intent intent, int requestCode)` | Like `startActivity`, expecting a result: when the launched Activity finishes, its result arrives in `onActivityResult` here, before `onResume`. |
| `setResult(int resultCode)` / `setResult(int resultCode, Intent data)` | In the launched Activity: the result reported to its launcher — `RESULT_OK` (-1), `RESULT_CANCELED` (0, the default when never called) or `RESULT_FIRST_USER` (1) and above. The Intent's extras are readable in the launcher's `onActivityResult`. |
| `onActivityResult(int requestCode, int resultCode, Intent data)` | Override (it is `protected`) to read the result. `data` is `null` unless the child called `setResult(int, Intent)`. |
| `getIntent()` | The Intent that launched this Activity, extras included, or `null` for the app's boot Activity. |
| `setContentView(View root)` | Sets the root of the widget tree and renders it to the display. |
| `setContentView(int layoutResID)` | Inflates `R.layout.*` and makes it the content. See [resources](/guides/resources/). |
| `<T extends View> T findViewById(int id)` | The view with that `android:id` / `setId` in the content, depth first, or `null`. Also on every `View`. |
| `getLayoutInflater()` | A `LayoutInflater` for this Activity: `inflate(R.layout.row, parent, false)`. The Activity is its `LayoutInflater.Factory`. |
| `onCreateView(String name, Context context, AttributeSet attrs)` | Override to construct the view classes of your own that a layout names; see [custom views](/guides/resources/#custom-views). The default returns `null`. |
| `setTheme(int resid)` | Make `R.style.*` the app's theme: its colours become the framework widgets' defaults. Call it before `setContentView`. On `Context`. See [styles and the theme](/guides/resources/#styles-and-the-theme). |
| `runOnUiThread(Runnable action)` | Run `action` now when called on the main thread, else post it there. `getMainExecutor()` (on `Context`) is the main thread's `Executor`. |
| `getResources()` | The app's compiled `res/` tree: `getString`, `getText`, `getColor`, `getDimension`, `getDimensionPixelSize`, `getDimensionPixelOffset`, `getInteger`, `getBoolean`, and `getDisplayMetrics()`. `getString(int)`, `getString(int, Object...)` and `getColor(int)` are also on `Context`. |
| `getSupportFragmentManager()` | The Activity's `FragmentManager`. See [Fragment](#picodroidappfragment). |
| `getLifecycle()` | The Activity's `Lifecycle`: pass `this` to `LiveData.observe`. See [lifecycle](#picodroidlifecycle). |
| `getViewModelStore()` / `getDefaultViewModelProviderFactory()` | What `new ViewModelProvider(activity)` uses; the second is `ViewModelProvider.NewInstanceFactory` (the public no-argument constructor) unless overridden for ViewModels that take arguments. See [lifecycle](#picodroidlifecycle). |
| `getDisplay()` | Returns the `Display` singleton. |

See [`examples/navdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/navdemo) for a multi-Activity back-stack demo and [`examples/dialogdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/dialogdemo) for an `onBackPressed` override pattern.

## `picodroid.app.Fragment`

A reusable portion of an Activity's UI with a lifecycle of its own, the shape of `androidx.fragment.app.Fragment`; `FragmentManager`, `FragmentTransaction` and `FragmentFactory` sit beside it in `picodroid.app`. A fragment is hosted by an Activity, whose `getSupportFragmentManager()` adds, replaces, removes, hides and shows it:

```java
import picodroid.app.Fragment;
import picodroid.os.Bundle;
import picodroid.view.View;

public class DetailFragment extends Fragment {
  public DetailFragment() {
    super(R.layout.fragment_detail); // the default onCreateView inflates it
  }

  @Override
  public void onViewCreated(View view, Bundle savedInstanceState) {
    TextView title = view.findViewById(R.id.title);
    title.setText(requireArguments().getString("title"));
  }
}

// In the Activity, after setContentView:
DetailFragment detail = new DetailFragment();
Bundle args = new Bundle();
args.putString("title", "Second");
detail.setArguments(args);
getSupportFragmentManager()
    .beginTransaction()
    .replace(R.id.container, detail, "detail")
    .addToBackStack(null)
    .commit();
```

The callbacks arrive in Android's order: `onAttach(Context)`, `onCreate(Bundle)`, `onCreateView(LayoutInflater, ViewGroup, Bundle)`, `onViewCreated(View, Bundle)`, `onViewStateRestored`, `onStart`, `onResume`; then `onPause`, `onStop`, `onSaveInstanceState(Bundle)` (when the host saves), `onDestroyView`, `onDestroy`, `onDetach`; and `onHiddenChanged(boolean)`. The host's lifecycle drives them:

| Host callback | Its fragments |
|---|---|
| `onCreate(Bundle)` | Restored (through the `FragmentFactory`, see below) and created inside `super.onCreate`; a transaction committed in the `onCreate` body runs when the body returns. |
| `onStart` | Views created and started, before the host's `onStart` body. |
| `onResume` | Resumed after the host's `onResume` body (Android's `onPostResume`). |
| `onPause` / `onStop` | Paused / stopped before the host's callback; views are kept while stopped. |
| `onSaveInstanceState` | Every fragment's `onSaveInstanceState` follows the host's, into the same Bundle. |
| `onDestroy` | Views freed, fragments destroyed and detached before the host's `onDestroy` body. |

**Transactions.** `beginTransaction()` returns a `FragmentTransaction`: `add(containerId, fragment[, tag])`, `add(fragment, tag)` (no container: the view is placed by whoever owns the fragment, or there is none), `replace(containerId, fragment[, tag])`, `remove`, `hide` / `show` (the view goes `GONE`; the fragment stays resumed), `detach` / `attach` (the view is freed, the instance kept at `CREATED`), `setMaxLifecycle(fragment, Fragment.CREATED | STARTED | RESUMED)`, `addToBackStack(name)`, `setReorderingAllowed` (accepted), `isEmpty()`. `commit()` queues the transaction for a later main-thread tick, as Android does; it also runs when the host's lifecycle moves or on `executePendingTransactions()`. `commitNow()` runs it at once (not with a back stack entry). Both refuse to run after the host saved its state (`IllegalStateException`, Android's message) unless the `*AllowingStateLoss` form is used, and from inside a fragment callback. Within a transaction the fragments moving down go first, so a replaced page's widgets are freed before the new page's are made.

**Back stack.** A transaction committed with `addToBackStack` is kept so `popBackStack()` (queued), `popBackStackImmediate()`, or the named forms with `POP_BACK_STACK_INCLUSIVE`, reverse it: a popped `replace` removes the new fragment and adds the replaced ones back, their views built again through `onCreateView`. `getBackStackEntryCount()` and `addOnBackStackChangedListener` / `removeOnBackStackChangedListener` are there. The default `onBackPressed` pops the back stack when it has an entry and finishes the Activity otherwise, as `FragmentActivity` does.

**Finding fragments.** `findFragmentById(containerId)`, `findFragmentByTag(tag)`, `getFragments()`, and `putFragment(bundle, key, fragment)` / `getFragment(bundle, key)` to keep a reference to one in a Bundle; `isStateSaved()` and `isDestroyed()` on the manager. On a fragment `getActivity()` / `requireActivity()`, `getContext()` / `requireContext()`, `getView()` / `requireView()`, `getArguments()` / `requireArguments()`, `getParentFragmentManager()`, `getTag()`, `getId()` (its container's id), `isAdded()`, `isResumed()`, `isVisible()`, `isHidden()`, `isDetached()`, `isRemoving()`, `isStateSaved()`, `getLayoutInflater()`, `getString(int)`, `getResources()`, `startActivity(Intent)`. The `require*` forms throw `IllegalStateException` where the plain ones return `null`.

**Saved state.** When the host saves its state (before a `recreate()` or a reclaim, see [saved instance state](#saved-instance-state)), every fragment's `onSaveInstanceState` Bundle is saved with it under Android's key, `android:support:fragments`, along with its arguments, tag, container, back stack membership and the back stack itself. The next instance's `super.onCreate` re-creates them through the `FragmentFactory`, whose default constructs each fragment by its saved class name through its public no-argument constructor, as Android's does. A fragment that takes constructor arguments needs a factory installed **before** `super.onCreate`:

```java
@Override
protected void onCreate(Bundle savedInstanceState) {
  getSupportFragmentManager().setFragmentFactory(new FragmentFactory() {
    @Override
    public Fragment instantiate(String className) {
      if (className.equals(DetailFragment.class.getName())) return new DetailFragment(repository);
      return super.instantiate(className);   // the default: Class.forName(className).newInstance()
    }
  });
  super.onCreate(savedInstanceState);
  setContentView(R.layout.activity_main);
  if (savedInstanceState == null) {
    getSupportFragmentManager().beginTransaction().add(R.id.container, new HomeFragment(), "home").commit();
  }
}
```

Compare with `X.class.getName()`, never a string literal: a shrunk build renames app classes and `getName()` follows the rename. A saved fragment whose class has no public no-argument constructor and no factory throws `RuntimeException` from the restore, as Android's `Fragment.InstantiationException` does. `FragmentManager.saveFragmentInstanceState(fragment)` and `Fragment.setInitialSavedState(Bundle)` carry one fragment's state by hand, as Android's `SavedState` does.

**What differs from Android.** A view a fragment gives up in `onDestroyView` is freed at once, LVGL widgets and all, so every field holding one of its children is dead afterwards and the next `onCreateView` builds a fresh tree (Android keeps detached trees; a panel cannot). Lifecycle states are `int` constants on `Fragment` rather than a `Lifecycle.State` enum. `FragmentFactory.instantiate` takes the class name alone (no `ClassLoader`). Views are appended to their container in the order fragments reach `VIEW_CREATED`. An override that skips `super` is tolerated, as on `Activity` (Android throws `SuperNotCalledException`); call it anyway. A fragment is not a `LifecycleOwner` or a `ViewModelStoreOwner` itself: observe with `getViewLifecycleOwner()` and share a `ViewModel` through `requireActivity()` (see [lifecycle](#picodroidlifecycle)). Not provided: child fragment managers, `startActivityForResult` on a fragment (use the Activity's), transitions and animations, `setRetainInstance`, menus, the Fragment Result API. Every method runs on the main thread. A second `setContentView` after fragments have views stales them like any other view of the old tree: remove the fragments first.

See [`examples/fragmentdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/fragmentdemo) for the conformance script (callback order, back stack, refused transactions, the factory under a reclaim) and [`examples/claudeusage/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/claudeusage) for four screens as fragments in a [`ViewPager2`](#picodroidwidgetviewpager2).

## `picodroid.lifecycle`

`ViewModel`, `ViewModelProvider`, `LiveData` / `MutableLiveData`, `Observer`, `Lifecycle` and `LifecycleOwner`, the shape of `androidx.lifecycle`. They are how an Activity and its fragments share data without the fragments knowing their host's class: the Activity owns a `ViewModel`, the fragments get the same instance from a `ViewModelProvider` over `requireActivity()`, and each observes the `LiveData` it shows for as long as its view lives.

```java
import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.MutableLiveData;
import picodroid.lifecycle.ViewModel;
import picodroid.lifecycle.ViewModelProvider;

final class ReadingViewModel extends ViewModel {
  private final MutableLiveData<String> reading = new MutableLiveData<>();

  LiveData<String> reading() { return reading; }
  void publish(String text) { reading.setValue(text); }
}

public class MainActivity extends Activity {
  // ReadingViewModel has a public no-argument constructor, so the default factory
  // (ViewModelProvider.NewInstanceFactory) makes it; nothing to override.
}

public class ReadingFragment extends Fragment {
  @Override
  public void onViewCreated(View view, Bundle savedInstanceState) {
    super.onViewCreated(view, savedInstanceState);
    TextView label = view.findViewById(R.id.reading);
    ReadingViewModel model = new ViewModelProvider(requireActivity()).get(ReadingViewModel.class);
    model.reading().observe(getViewLifecycleOwner(), text -> label.setText(text));
  }
}
```

**`LiveData<T>`.** `observe(owner, observer)` adds an observer that is active while `owner`'s lifecycle is at least `STARTED`: it gets the current value when it becomes active (if it has not seen it), every `setValue` while it stays active, and is removed by itself when the owner is destroyed. `observeForever(observer)` is always active until `removeObserver`. Also `removeObservers(owner)`, `getValue()` (`null` before the first value), `hasObservers()`, `hasActiveObservers()`, and the `onActive()` / `onInactive()` hooks. `setValue` and `postValue` are `protected` on `LiveData` and public on `MutableLiveData`, as on Android. `setValue` is main-thread only and delivers before it returns; `postValue` is for any thread and hands the value to the main thread, where the last of several posted values wins.

`setValue` runs every active observer inside the call. On a microcontroller that is one main-thread tick doing all of their work, so an observer with a lot to repaint should post the repaint to the next tick (`Executors.mainExecutor().execute(...)`), which is what `claudeusage`'s pages do.

**Owners.** An `Activity` is a `LifecycleOwner`: its `getLifecycle()` is `CREATED` after `onCreate` returns, `STARTED` after `onStart`, `RESUMED` after `onResume`, and steps back down before `onPause`, `onStop` and `onDestroy` run. A fragment's `getViewLifecycleOwner()` follows its view the same way and is destroyed before `onDestroyView`; each view gets a new one, and the call throws while there is no view. `Lifecycle.getCurrentState()` returns an `int` (`Lifecycle.DESTROYED`, `INITIALIZED`, `CREATED`, `STARTED`, `RESUMED`); compare with `>=` where Android says `isAtLeast`. An app can be an owner of its own: implement `LifecycleOwner`, hold a `new Lifecycle()` and move it with `setCurrentState`.

**`ViewModel` and `ViewModelProvider`.** `new ViewModelProvider(owner).get(X.class)` returns the owner's `X`, creating it on the first call; `get(key, X.class)` keeps several of one class. An `Activity` is the `ViewModelStoreOwner`. The instance is made by a `ViewModelProvider.Factory`: the one passed as the constructor's second argument, else the owner's `getDefaultViewModelProviderFactory()`, which for an Activity is `ViewModelProvider.NewInstanceFactory`, `modelClass.newInstance()` through the public no-argument constructor, as on Android. A ViewModel that takes arguments gets a factory of its own (override the default, or pass one); a class the default cannot construct throws `RuntimeException("Cannot create an instance of …")`. `onCleared()` runs when the owning Activity is destroyed.

**What differs from Android.** A `ViewModel` lives as long as its Activity *instance*: there are no configuration changes here, and after `recreate()` or a reclaim the new instance starts with a new ViewModel, as an Android app does after process death (keep what must survive in `onSaveInstanceState`). Lifecycle states are `int`s. Not provided: `Lifecycle.addObserver` with `LifecycleObserver` / `DefaultLifecycleObserver` and `Lifecycle.Event`, `Transformations`, `MediatorLiveData`, `SavedStateHandle`, `AndroidViewModel`, `viewModelScope`, and a fragment as its own `LifecycleOwner` or `ViewModelStoreOwner`.

See [`examples/claudeusage/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/claudeusage) (`UsageViewModel`, observed by four pager pages) and the lifecycle step of [`examples/fragmentdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/fragmentdemo).

## `picodroid.os.Bundle`

A String-keyed map of typed values, mirroring `android.os.Bundle`: the carrier for Intent extras and for saved instance state.

```java
import picodroid.os.Bundle;

Bundle b = new Bundle();
b.putInt("count", 3);
b.putString("name", "pico");
int count = b.getInt("count");            // 3
long missing = b.getLong("nope", -1L);    // -1: absent key gives the default
String wrong = b.getString("count");      // null: so does a value of another type

startActivity(new Intent(DetailActivity.class).putExtras(b));
// in DetailActivity:
Bundle extras = getIntent().getExtras();  // a copy, or null when there are none
```

| Method | Description |
|--------|-------------|
| `putBoolean` / `putInt` / `putLong` / `putFloat` / `putDouble` / `putString` | Store a value under a key, replacing any previous one (of any type). |
| `putIntArray` / `putByteArray` / `putStringArray` / `putBundle` | Arrays and nested Bundles, stored by reference. |
| `getInt(key)` / `getInt(key, default)` and the same pair for every scalar type | Typed read. An absent key or a value of another type returns the default (`0`, `false`, `null`, or the one given) — never throws. |
| `getIntArray` / `getByteArray` / `getStringArray` / `getBundle` / `get` | Reference reads; `null` when absent or mistyped. `get` returns primitives boxed. |
| `containsKey` / `remove` / `clear` / `size` / `isEmpty` / `keySet` | Map housekeeping. `keySet()` is a fresh set in insertion order. |
| `putAll(Bundle)` / `new Bundle(Bundle)` | Shallow copy of another Bundle's mappings. |

`Intent` exposes the same store: `putExtra(String, int/long/boolean/String/Bundle)`, `getIntExtra` / `getLongExtra` / `getBooleanExtra` / `getStringExtra` / `getBundleExtra`, `hasExtra`, `putExtras(Bundle)` and `getExtras()`. There is no `Parcelable` or `Serializable`.

## `picodroid.graphics.Display`

Singleton representing the physical display. Typically accessed via `Activity.getDisplay()`.

```java
import picodroid.graphics.Display;

Display display = Display.getInstance();
int w = display.getWidth();      // e.g. 320
int h = display.getHeight();     // e.g. 240

display.setContentView(root);    // set root widget
display.update();                // refresh the display
```

The `Display` surface is intentionally minimal and Android-shaped. Picodroid-only helpers
(touch calibration, the FPS overlay, pull-mode touch polling) live on
[`picodroid.debug.DisplayDebug`](#picodroiddebugdisplaydebug), not on `Display`.

## `picodroid.debug.DisplayDebug`

Picodroid-specific debug helpers that have no Android equivalent — kept off `Display` so its
surface stays close to `android.view.Display`. All methods are `static`.

```java
import picodroid.debug.DisplayDebug;
import picodroid.view.MotionEvent;

DisplayDebug.calibrate();    // interactive 4-point touch calibration (embedded targets; blocks)
DisplayDebug.showFps();      // toggle the live LVGL FPS overlay (call once in onCreate)
MotionEvent touch = DisplayDebug.pollTouch();  // pull one raw touch sample (null if the queue is empty)
```

| Method | Description |
|--------|-------------|
| `static void calibrate()` | Run the interactive 4-point touch calibration. Blocks until the user finishes. |
| `static void showFps()` | Show the live FPS overlay. Idempotent after the first call. |
| `static MotionEvent pollTouch()` | Poll one raw touch sample; `null` if the queue is empty. The primary touch path is a per-View `OnTouchListener` (below) — `pollTouch` is the pull-mode alternative. |

## `picodroid.graphics.Color`

Color constants and factory methods. All colors are ARGB integers.

```java
import picodroid.graphics.Color;

int white = Color.WHITE;          // 0xFFFFFFFF
int red   = Color.RED;            // 0xFFFF0000
int custom = Color.rgb(128, 0, 255);       // 0xFF8000FF
int semi   = Color.argb(128, 255, 0, 0);   // 0x80FF0000 (50% transparent red)
```

| Constant | Value |
|----------|-------|
| `Color.BLACK` | `0xFF000000` |
| `Color.WHITE` | `0xFFFFFFFF` |
| `Color.RED` | `0xFFFF0000` |
| `Color.GREEN` | `0xFF00FF00` |
| `Color.BLUE` | `0xFF0000FF` |
| `Color.YELLOW` | `0xFFFFFF00` |
| `Color.CYAN` | `0xFF00FFFF` |
| `Color.MAGENTA` | `0xFFFF00FF` |
| `Color.TRANSPARENT` | `0x00000000` |

| Method | Description |
|--------|-------------|
| `Color.rgb(int r, int g, int b)` | Returns an ARGB int with full opacity (alpha=255) |
| `Color.argb(int a, int r, int g, int b)` | Returns an ARGB int with the specified alpha |

## `picodroid.graphics.Theme`

App-wide color palette — static fields the framework's widgets read at view-construction time. The Android way to fill it is a theme: declare `<style name="AppTheme">` in `res/values` and name it in the manifest, `<application android:theme="@style/AppTheme">`, which the framework applies before the first Activity's `onCreate`; or call `setTheme(R.style.AppTheme)` in `onCreate` before any view exists ([styles and the theme](/guides/resources/#styles-and-the-theme)). An app without resources assigns the fields directly, **before any UI is built** (typically in `Application.onCreate`):

```java
import picodroid.graphics.Color;
import picodroid.graphics.Theme;

Theme.colorPrimary    = Color.argb(255,  80, 180, 120);
Theme.colorBackground = Color.argb(255,  24,  24,  28);
```

| Field | Theme item | Default | Use |
|-------|------------|---------|-----|
| `colorPrimary` | `colorPrimary` | bluish accent | button fill, focused outlines, slider track |
| `colorOnPrimary` | `colorOnPrimary` | white | text/icons on top of `colorPrimary` |
| `colorBackground` | `android:colorBackground` | near-black | page background |
| `colorSurface` | `colorSurface` | dark grey | card / surface background |
| `colorText` | `android:textColorPrimary` | near-white | primary body text |
| `colorTextSecondary` | `android:textColorSecondary` | muted grey | secondary / muted body text |
| `colorOutline` | `colorOutline` | dark grey | subtle separator / divider line |

picodroid is single-app, so the palette is process-global rather than per-Activity. Views still need to read these values explicitly (`view.setBackgroundColor(Theme.colorBackground)`); there is no automatic cascading.

See the themed-widgets section of [`examples/displaydemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/displaydemo) for a worked example.

## `picodroid.graphics.drawable.GradientDrawable`

A configurable shape drawable: solid fill (or two-color linear gradient), optional corner radius, optional stroke. Mirrors the most-used subset of Android's `GradientDrawable`.

```java
import picodroid.graphics.Color;
import picodroid.graphics.drawable.GradientDrawable;

GradientDrawable bg = new GradientDrawable()
    .setColor(Color.argb(255, 32, 32, 40))
    .setCornerRadius(16)
    .setStroke(2, Color.WHITE);
view.setBackground(bg);

// Or a vertical gradient
GradientDrawable g = new GradientDrawable()
    .setGradient(Color.BLUE, Color.MAGENTA, GradientDrawable.Orientation.TOP_BOTTOM)
    .setCornerRadius(20);
view.setBackground(g);
```

| Method | Description |
|--------|-------------|
| `setColor(int argb)` | Solid fill. Replaces any previously set gradient. |
| `setCornerRadius(int px)` | Corner radius. Half the smaller dimension renders a pill. |
| `setStroke(int width, int color)` | Border outline. `width = 0` removes. |
| `setGradient(int start, int end, int orientation)` | Two-color linear gradient. Replaces any previously set solid color. |

| Constant | Value |
|----------|-------|
| `GradientDrawable.Orientation.TOP_BOTTOM` | 1 |
| `GradientDrawable.Orientation.LEFT_RIGHT` | 2 |

Multi-stop gradients, angle-arbitrary orientations, and radial gradients are deferred.

## `picodroid.graphics.drawable.BitmapDrawable`

An image from another package's bundled assets, as `PackageManager.getApplicationIcon` returns it (multi-app boards). Show it with `ImageView.setImageDrawable`, or use it as a background with `View.setBackground`. Apps do not build these themselves; for the app's own assets use `ImageView.setImageSource`.

```java
Drawable icon = getPackageManager().getApplicationIcon(info.applicationInfo);
if (icon != null) {
  imageView.setImageDrawable(icon);
}
```

## `picodroid.view.View`

Base class for all UI widgets. Use its subclasses like `TextView` and `Button`, or subclass it
with `View(Context)` to draw your own content (see [Custom drawing](#custom-drawing-ondraw-canvas-and-paint)).

```java
import picodroid.view.View;

view.setPosition(10, 20);           // x=10, y=20
view.setSize(200, 50);              // width=200, height=50
view.setBackgroundColor(Color.BLUE);
view.setBackground(drawable);       // or apply a Drawable (see GradientDrawable)
view.setBackgroundTintList(ColorStateList.valueOf(Color.RED));  // recolour it, keep its shape
view.setVisibility(View.VISIBLE);   // VISIBLE, INVISIBLE, or GONE
view.setEnabled(false);             // grey out / disable interaction
view.setTranslationX(8f);           // also setTranslationY, setRotation, setScaleX/Y + getters
view.animate().alpha(1f).setDuration(200).start();       // see ViewPropertyAnimator
view.setOnClickListener(v -> doThing());   // View.OnClickListener (fires on tap or D-pad center)
view.setOnTouchListener(listener);  // per-View touch dispatch
ViewParent p = view.getParent();     // the ViewGroup it was added to, or null
view.close();                        // free the widget; a parented view leaves its parent first
```

`close()` is picodroid's addition (Android's views are garbage collected). On a child it is
`removeView` from the child's side: the view leaves its parent's child list, then its widget is
freed. A closed view is released like a removed one — further calls throw, `addView` refuses it,
and a second `close()` is a no-op.

| Constant | Value | Description |
|----------|-------|-------------|
| `View.VISIBLE` | 0 | Widget is visible and takes up layout space |
| `View.INVISIBLE` | 4 | Widget is invisible but still takes up layout space; touches pass through it, as on Android |
| `View.GONE` | 8 | Widget is invisible and takes no layout space |
| `View.WRAP_CONTENT` | -2 | Passed to `setSize`: size to content. `MATCH_PARENT` (-1) is on [`ViewGroup.LayoutParams`](#picodroidviewviewgroup). |
| `View.NO_ID` | -1 | What `getId()` returns for a view without an id; never matches in `findViewById`. |

The values are Android's. A setter given the value the view already has does nothing, as on
Android: `setVisibility`, `setAlpha`, `setEnabled`, `setBackground` with the same instance, an equal
tint, and on a `TextView` `setText` and `setTextColor`. A screen that writes every label on every
update pays only for the ones that changed.

The rest of the `View` surface, all mirroring `android.view.View`:

| Method | Description |
|--------|-------------|
| `getVisibility()` / `isEnabled()` / `getAlpha()` | The value the app last set. `setAlpha(float)` takes 0.0–1.0; `getAlpha()` returns the target of a started alpha animation, not the per-frame value (after `animate().cancel()`, the value the animation stopped at). |
| `getBackground()` | The `Drawable` last given to `setBackground` (an inflated `<shape>` included), or `null` for none or a plain colour. |
| `setBackgroundTintList(ColorStateList)` / `getBackgroundTintList()` | Recolour the background, keeping its shape: the way to change a rounded dot's or pill's colour without a new drawable. The tint's colour replaces the background's and its alpha is ignored; `null` puts a drawable's own colour back. |
| `onMeasure(int, int)` / `setMeasuredDimension` / `measure` / `getMeasuredWidth` / `getMeasuredHeight` / `resolveSize` / `getDefaultSize` / `View.MeasureSpec` | How a view that draws itself says what size it wants; see [custom views](/guides/resources/#custom-views). |
| `setPadding(int left, int top, int right, int bottom)` | Inner padding in pixels. |
| `setMinimumWidth(int)` / `setMinimumHeight(int)` / `getMinimumWidth()` / `getMinimumHeight()` | A floor under the laid-out size, as on Android: a `wrap_content` or weighted view never comes out smaller. `android:minWidth` / `android:minHeight` in a layout file. |
| `setKeepScreenOn(boolean)` / `getKeepScreenOn()` | Hold the display on while this view lives ([display idle sleep](/reference/limits/#display-idle-sleep)). `android:keepScreenOn` in a layout file. |
| `getLeft()` / `getTop()` / `getWidth()` / `getHeight()` | Laid-out position relative to the parent (excluding translation) and size, in pixels. |
| `getX()` / `getY()` | `getLeft() + getTranslationX()` and `getTop() + getTranslationY()`, as `float`. |
| `setId(int)` / `getId()` / `findViewById(int)` | The view's identifier (a layout's `android:id` sets it) and a depth-first search of this view and its descendants; `null` when nothing matches. |
| `setTag(Object)` / `getTag()` | An arbitrary object kept with the view. |
| `setLayoutParams(ViewGroup.LayoutParams)` / `getLayoutParams()` | The parameters the parent applies in `addView(child, params)`. |
| `setOnLongClickListener(View.OnLongClickListener)` | `boolean onLongClick(View v)` fires when a press is held past the long-press threshold (~400 ms); returning `true` consumes it and suppresses the click that would follow. Attaching one makes the view clickable, as `setOnClickListener` does. |
| `setOnKeyListener(OnKeyListener)` | See [Key events](#key-events). |
| `setOnSwipeListener(OnSwipeListener)` | See [Swipe gestures](#swipe-gestures). |
| `performClick()` / `performLongClick()` | Run the registered listener without a touch, for scripted flows and tests; `performLongClick()` returns whether the listener consumed it. |
| `invalidate()` / `postInvalidate()` | Ask for `onDraw` to run again; see below. |

## Custom drawing: `onDraw`, `Canvas` and `Paint`

Subclass `View` with the `View(Context)` constructor and override `onDraw(Canvas)`, as on Android.
Call `invalidate()` when the state `onDraw` reads changes; `onDraw` then runs on the main thread
before the next frame, and several `invalidate()` calls before it runs cost one `onDraw`.

```java
import picodroid.content.Context;
import picodroid.graphics.Canvas;
import picodroid.graphics.Color;
import picodroid.graphics.Paint;
import picodroid.view.View;

final class BarChart extends View {
  private final Paint paint = new Paint();
  private final int[] values = new int[24];

  BarChart(Context ctx) {
    super(ctx);
    setSize(264, 50);
  }

  void setValue(int i, int percent) {
    values[i] = percent;
    invalidate();
  }

  @Override
  protected void onDraw(Canvas canvas) {
    for (int i = 0; i < values.length; i++) {
      int h = 2 + values[i] * 48 / 100;
      paint.setColor(values[i] >= 80 ? Color.RED : Color.GREEN);
      canvas.drawRoundRect(i * 11, 50 - h, i * 11 + 8, 50, 2, 2, paint);
    }
  }
}
```

A drawing view starts transparent, borderless and not clickable, and is 0 by 0 until it is sized.
Coordinates are pixels from the view's top-left corner; a rectangle's right and bottom edges are
exclusive; angles are degrees clockwise from 3 o'clock; everything is clipped to the view.

| `Canvas` method | Description |
|--------|-------------|
| `drawColor(int argb)` | Fill the whole view. |
| `drawRect(l, t, r, b, paint)` | A rectangle. |
| `drawRoundRect(l, t, r, b, rx, ry, paint)` | Rounded corners; the smaller of `rx` and `ry` is the radius. |
| `drawCircle(cx, cy, radius, paint)` | A circle. |
| `drawLine(x1, y1, x2, y2, paint)` | A line in the paint's colour, stroke width and cap. |
| `drawArc(l, t, r, b, start, sweep, useCenter, paint)` | An arc of the circle inscribed in the bounds. Filled, a wedge from the centre; stroked, the arc alone. |
| `drawText(text, x, y, paint)` | One line of text with its baseline at `y`, aligned on `x` by the paint's text align. |
| `getWidth()` / `getHeight()` | The view's size, during `onDraw`. |

| `Paint` method | Description |
|--------|-------------|
| `setColor(int)` / `setAlpha(int)` / `setARGB(a, r, g, b)` | Colour; its alpha is the opacity. |
| `setStyle(Paint.Style)` | `FILL` (default), `STROKE` or `FILL_AND_STROKE`. |
| `setStrokeWidth(float)` | Stroke width; 0 is a one-pixel hairline. A stroke straddles the outline, as on Android. |
| `setStrokeCap(Paint.Cap)` | `BUTT` (default) or `ROUND`; `SQUARE` draws as `BUTT`. |
| `setTextSize(float)` / `setTextAlign(Paint.Align)` | Snaps to the nearest compiled face, like `TextView.setTextSize`; `LEFT`, `CENTER` or `RIGHT`. |
| `ascent()` / `descent()` / `measureText(String)` | The face's line top above the baseline (negative), its bottom below it, and a text's width. |
| `new Paint()` / `new Paint(int flags)` / `new Paint(Paint src)`, `set(Paint src)`, `reset()` | A new paint is opaque black, filled, hairline stroke, 12 px text, left-aligned. `Paint.ANTI_ALIAS_FLAG`, `setAntiAlias`, `isAntiAlias`, `setFlags` and `getFlags` are accepted for source compatibility: drawing is always anti-aliased. |

Every setter has its getter: `getColor()`, `getAlpha()`, `getStyle()`, `getStrokeWidth()`, `getStrokeCap()`, `getTextSize()`, `getTextAlign()`.

**How it works, and what it costs.** There is no pixel buffer: a 320x240 bitmap would be 150 KB. Each draw
call records one 32-byte op for the view, and the renderer replays the ops whenever it repaints the view.
The recording lives in LVGL's memory pool and holds 2 KB, about 60 calls, per view; calls past that are
dropped, with a `[sim] Canvas:` line in the simulator. That is still far cheaper than a view per shape,
which costs pool memory and milliseconds to create. `onDraw` must draw the whole view each time, since the
next `onDraw` replaces the recording.

**Boards.** Every RP2350 board. The RP2040 testbench leaves `Canvas` and `Paint` out (`has_canvas = false`
in its `board.toml`): they cost about 16 KB of a program region that has little left, and
`build-apk.sh --board testbench_rp2040` rejects an app that uses them.

**Divergences.** No `Bitmap`, `Path`, `Matrix`, `Shader`, `save`/`restore`, clipping or `Rect`/`RectF`
overloads. Ovals and arcs are circular: the smaller side of the bounds sets the radius. `onDraw` runs after
`invalidate()`, not on every frame, and a size change needs an `invalidate()` to redraw at the new size.
`invalidate()` on a framework widget does nothing; they redraw themselves.

### Focus navigation

On button-only devices (no touchscreen), key events route to whichever view is **focused**. Make a
view focusable and give it focus so it receives D-pad / hardware-key input. Mirrors
`android.view.View`. A focusable view also scrolls into view when it takes focus, so a column
taller than the screen can be walked with the buttons.

```java
button.setFocusable(true);          // opt this view into the focus group
button.requestFocus();              // take input focus now (returns false if not focusable)
boolean f = button.isFocused();

button.setOnFocusChangeListener(new View.OnFocusChangeListener() {
    public void onFocusChange(View v, boolean hasFocus) {
        v.setBackgroundColor(hasFocus ? Color.YELLOW : Color.GRAY);
    }
});
```

| Method | Description |
|--------|-------------|
| `setFocusable(boolean)` | Opt this view into the per-Activity LVGL focus group. |
| `requestFocus()` | Request input focus. Returns `false` if the view is not focusable. |
| `isFocusable()` / `isFocused()` / `hasFocus()` | Focus-state queries. |
| `setOnFocusChangeListener(OnFocusChangeListener)` | `onFocusChange(View v, boolean hasFocus)` fires on focus gain/loss. |

See [Key events](#key-events) for handling the keys themselves, and
[`examples/keydemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/keydemo).

## `picodroid.view.ViewGroup`

Abstract base for every container that holds child views — `LinearLayout`, `FrameLayout`,
`ScrollView`, `SwipeRefreshLayout`, and the adapter views. Extends `View`, so containers also have
position/size/background/visibility. Mirrors `android.view.ViewGroup`.

```java
group.addView(child);                 // append a child
group.addView(child, params);         // append with explicit LayoutParams
int n = group.getChildCount();
View first = group.getChildAt(0);    // null past the last child
group.removeView(child);
group.removeAllViews();
```

`getChildCount()`/`getChildAt()` track the children added through this API. One divergence from
Android: `removeView` and `removeAllViews` also **free the removed view's widget** (an embedded
panel cannot keep detached trees waiting for a re-add), so a removed view cannot be added again —
`addView` throws `IllegalStateException` for it. Build a fresh view, or hide one with
`setVisibility(View.GONE)` when it will come back. A second divergence follows from the first:
`addView` on a view that already has a parent **moves** it (Android throws), because a move is the
only way to reparent a view when `removeView` frees. The old parent's list lets go of it.

`ViewGroup.LayoutParams` carries a child's requested `width`/`height`, using `MATCH_PARENT` (-1) or
`WRAP_CONTENT` (-2):

```java
view.setLayoutParams(new ViewGroup.LayoutParams(
    ViewGroup.LayoutParams.MATCH_PARENT,
    ViewGroup.LayoutParams.WRAP_CONTENT));
```

`addView(child)` applies the `LayoutParams` the child already carries (an inflated view's, or what
`setLayoutParams` set), as on Android; a child with none keeps the size and position it was given
directly.

`ViewGroup.MarginLayoutParams` adds `leftMargin`, `topMargin`, `rightMargin`, `bottomMargin` and
`setMargins(l, t, r, b)`; `LinearLayout.LayoutParams` and `FrameLayout.LayoutParams` extend it. A
`LinearLayout` keeps the margins clear around the child; a `FrameLayout` places the child by them
([below](#picodroidwidgetframelayout)). Margins are read when the child is added. In a layout file
they are `android:layout_margin` and its per-side forms.

## `picodroid.view.MotionEvent`

Represents a touch event from the display. Delivered to per-View `OnTouchListener`s (the primary path), or pulled via `DisplayDebug.pollTouch()`.

```java
import picodroid.debug.DisplayDebug;
import picodroid.view.MotionEvent;

MotionEvent event = DisplayDebug.pollTouch();
if (event != null) {
    int action = event.getAction();   // ACTION_DOWN, ACTION_UP, ACTION_MOVE, ACTION_LONG_PRESS
    int x = event.getX();
    int y = event.getY();
    long t = event.getEventTime();    // ms timestamp (boot-elapsed)
}
```

| Constant | Value |
|----------|-------|
| `MotionEvent.ACTION_DOWN` | 0 |
| `MotionEvent.ACTION_UP` | 1 |
| `MotionEvent.ACTION_MOVE` | 2 |
| `MotionEvent.ACTION_LONG_PRESS` | 3 (picodroid extension; LVGL long-press) |

In an `OnTouchListener`, `getX()` / `getY()` are relative to the receiving view's top-left corner and `getRawX()` / `getRawY()` are screen-absolute, as on Android. All four return `int`, not `float`.

## `picodroid.view.OnTouchListener` and `GestureDetector`

Install an `OnTouchListener` to receive raw touch events on a single `View`:

```java
import picodroid.view.MotionEvent;
import picodroid.view.OnTouchListener;
import picodroid.view.View;

view.setOnTouchListener(new OnTouchListener() {
    public boolean onTouch(View v, MotionEvent e) {
        if (e.getAction() == MotionEvent.ACTION_DOWN) {
            // ...
        }
        return true;   // event consumed
    }
});
```

For tap / long-press / fling recognition, wrap an `OnGestureListener` in `GestureDetector` (which itself implements `OnTouchListener`):

```java
import picodroid.view.GestureDetector;
import picodroid.view.MotionEvent;
import picodroid.view.View;

view.setOnTouchListener(new GestureDetector(new GestureDetector.OnGestureListener() {
    public void onSingleTap(MotionEvent e) { Log.i("UI", "tap @ " + e.getX()); }
    public void onLongPress(MotionEvent e) { Log.i("UI", "long press"); }
    public void onFling(MotionEvent down, MotionEvent up, float vx, float vy) {
        Log.i("UI", "fling vx=" + vx + " vy=" + vy);
    }
}));
```

| Constant | Value | Meaning |
|----------|-------|---------|
| `GestureDetector.TAP_SLOP_PX` | 12 | Max DOWN→UP displacement to count as a tap. |
| `GestureDetector.FLING_MIN_PX` | 24 | Min DOWN→UP displacement to count as a fling. |

`GestureDetector.SimpleOnGestureListener` is an abstract base with an empty body for each of the three callbacks, so a subclass overrides only the gestures it wants. `onLongPress` fires while the finger is still down, and the release that follows does not call `onSingleTap`.

Velocities are pixels/second; positive `vx` is rightward, positive `vy` is downward. v1 caveats: no `ACTION_MOVE` / scroll callbacks; multi-touch is not supported. See [`examples/gesturedemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/gesturedemo).

## `picodroid.view.ViewPropertyAnimator`

Fluent builder for short interpolated property animations on a single `View`. Obtain via `view.animate()`. Every property method takes only the **target** value, as on Android — the animation starts from whatever value the view has when it begins.

```java
view.animate()
    .alpha(1f)            // fade in from the current alpha
    .translationX(40f)    // and slide right to a 40 px offset
    .setDuration(250)     // both in 250 ms (default 300 ms)
    .setInterpolator(new DecelerateInterpolator())
    .withEndAction(() -> Log.i("UI", "done"))
    .start();
```

| Method | Description |
|--------|-------------|
| `alpha(float)` | Animate alpha (0.0–1.0). |
| `x(float)`, `y(float)` | Animate the layout position in pixels. No effect on a child of a `LinearLayout`, which positions its children itself — use translation there. |
| `translationX(float)`, `translationY(float)` | Animate the offset from the laid-out position. Works everywhere. |
| `rotation(float)` | Animate rotation in degrees clockwise, about the view's centre. |
| `scaleX(float)`, `scaleY(float)` | Animate scale about the centre (1.0 = unscaled). |
| `setDuration(long ms)` / `getDuration()` | Total duration; applies to every queued property. |
| `setStartDelay(long ms)` / `getStartDelay()` | Wait before starting; the animation then starts from the value the view has at that moment. |
| `setInterpolator(Interpolator)` | `Linear`, `Accelerate`, `Decelerate` or `AccelerateDecelerate` from `picodroid.view.animation`; anything else falls back to linear. |
| `withEndAction(Runnable)` | Run once every queued property finishes, on the main thread. One per view — a later registration replaces an earlier one; dropped by `cancel()`. |
| `start()` | Begin every queued property animation. Explicit — nothing runs until it is called. |
| `cancel()` | Cancel every property animation targeting this view. Properties stay at the last interpolated frame. |

Starting a property that is already animating replaces the running animation for that property (Android cancels it too); a delayed start takes over when its delay expires. Multiple property calls in one chain run concurrently. The immediate setters live on `View` — `setTranslationX/Y`, `setRotation`, `setScaleX/Y` and their getters, with `getX() == getLeft() + getTranslationX()` as on Android.

**Memory:** a rotated or scaled view is rendered through an off-screen ARGB layer of its own size, allocated from LVGL's pool (64 KB by default). A 60×30 tile costs ~7 KB; a full 240×240 screen would need 225 KB and is skipped with an `Allocating layer buffer failed` log. Keep transformed views small. Up to 16 property animations run concurrently across all views. See [`examples/animdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/animdemo).

## Key events

Hardware buttons declared in [`board.toml`](/reference/porting-guide/#boardtoml-reference) are surfaced through Android-style `KeyEvent`s, and delivered the way Android delivers them: first to the **focused** widget's `OnKeyListener`, then — if there is no focused widget, or its listener returned `false` — to the foreground Activity's `onKeyDown` / `onKeyUp`.

A screen with nothing to focus (a dashboard, a game) needs no invisible focus-catcher: override the Activity callbacks.

```java
@Override
public boolean onKeyDown(int keyCode, KeyEvent event) {
    switch (keyCode) {
        case KeyEvent.KEYCODE_DPAD_UP:   previousPage(); return true;
        case KeyEvent.KEYCODE_DPAD_DOWN: nextPage();     return true;
        default: return super.onKeyDown(keyCode, event);
    }
}
```

The defaults follow Android's BACK contract: `Activity.onKeyDown` consumes `KEYCODE_BACK` and calls `event.startTracking()`, and `onKeyUp` runs `onBackPressed()` for a BACK release whose press it tracked (`event.isTracking()`) and whose long-press did not run (`!event.isCanceled()`). So an override that consumes BACK in `onKeyDown` without calling `super` suppresses the back action, and one that calls `super` for BACK keeps it — no need to override `onBackPressed` to a no-op. HOME never reaches an app, and BACK is offered to a showing soft keyboard and to a showing `AlertDialog` before either handler sees it.

### Auto-repeat and long-press

A key held past `ViewConfiguration.getKeyRepeatTimeout()` (400 ms, Android's long-press timeout) delivers further `ACTION_DOWN` events with a rising `getRepeatCount()` every `getKeyRepeatDelay()` (50 ms) until the release, as Android's input dispatcher does. The first repeat carries `FLAG_LONG_PRESS`. A handler that wants one action per physical press acts on `getRepeatCount() == 0` (or on the release); one that wants to accelerate while the key is held uses the count as a pace.

Android's way of giving one button two actions works unchanged, through `KeyEvent.dispatch` and the Activity's `KeyEvent.DispatcherState`: track the press, act on the long-press in `onKeyLongPress` (consuming it cancels the release), and act on the release only when it is tracked and not cancelled.

```java
@Override
public boolean onKeyDown(int keyCode, KeyEvent event) {
    if (keyCode == KeyEvent.KEYCODE_DPAD_CENTER) {
        if (event.getRepeatCount() == 0) event.startTracking();
        return true;
    }
    return super.onKeyDown(keyCode, event);
}

@Override
public boolean onKeyLongPress(int keyCode, KeyEvent event) {
    if (keyCode == KeyEvent.KEYCODE_DPAD_CENTER) { rediscover(); return true; }   // the hold
    return super.onKeyLongPress(keyCode, event);
}

@Override
public boolean onKeyUp(int keyCode, KeyEvent event) {
    if (keyCode == KeyEvent.KEYCODE_DPAD_CENTER && event.isTracking() && !event.isCanceled()) {
        syncNow();                                                                // the press
        return true;
    }
    return super.onKeyUp(keyCode, event);
}
```

A focused view's `OnKeyListener` sees the repeats too (`getRepeatCount()` on each), but `onKeyLongPress` is an Activity callback: the tracking state lives with the Activity, as it lives with the window on Android. [`examples/keydemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/keydemo) shows both patterns; [`examples/claudeusage/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/claudeusage) gives its four buttons eight actions with them.

A widget that wants a key for itself takes it first through an `OnKeyListener` (a `Button` is the easiest to focus):

```java
import picodroid.view.KeyEvent;
import picodroid.view.OnKeyListener;
import picodroid.view.View;
import picodroid.widget.Button;

Button focus = new Button("Focus me");
focus.setOnKeyListener(new OnKeyListener() {
    public boolean onKey(View v, KeyEvent event) {
        if (event.getAction() == KeyEvent.ACTION_DOWN
                && event.getKeyCode() == KeyEvent.KEYCODE_DPAD_CENTER) {
            // handled
            return true;
        }
        return false;  // let LVGL default nav run
    }
});
```

Return `true` from `onKey` to consume the event; `false` passes it on to the Activity's `onKeyDown` / `onKeyUp` (and lets LVGL keep processing it, e.g. for default focus navigation).

| Constant | Value |
|----------|-------|
| `KeyEvent.ACTION_DOWN` | 0 |
| `KeyEvent.ACTION_UP` | 1 |
| `KeyEvent.KEYCODE_HOME` | 3 (handled by the framework; never delivered to an app) |
| `KeyEvent.KEYCODE_BACK` | 4 |
| `KeyEvent.KEYCODE_DPAD_UP` | 19 |
| `KeyEvent.KEYCODE_DPAD_DOWN` | 20 |
| `KeyEvent.KEYCODE_DPAD_LEFT` | 21 |
| `KeyEvent.KEYCODE_DPAD_RIGHT` | 22 |
| `KeyEvent.KEYCODE_DPAD_CENTER` | 23 |
| `KeyEvent.FLAG_CANCELED` | `0x20` |
| `KeyEvent.FLAG_LONG_PRESS` | `0x80` |
| `KeyEvent.FLAG_CANCELED_LONG_PRESS` | `0x100` |
| `KeyEvent.FLAG_TRACKING` | `0x200` |
| `KeyEvent.FLAG_START_TRACKING` | `0x40000000` |

The `keycode` each pin emits is declared in `board.toml` — see [Porting Guide → board.toml reference](/reference/porting-guide/#boardtoml-reference) for the full schema. On boards with no buttons (touch-only), neither path ever fires.

| `KeyEvent` method | Description |
|---|---|
| `getAction()` / `getKeyCode()` | `ACTION_DOWN` or `ACTION_UP`, and the `KEYCODE_*` constant. |
| `getRepeatCount()` | 0 for the press, then 1, 2, 3… for each auto-repeat while the key stays held. |
| `isLongPress()` / `getFlags()` | Whether this is the first repeat after the long-press timeout (`FLAG_LONG_PRESS`); the raw flag word. |
| `startTracking()` / `isTracking()` | Mark a press in `onKeyDown` and recognise its release in `onKeyUp`, as the default BACK handling does. The flag carries from a press to its own release only. |
| `isCanceled()` | On a release: the press's long-press was handled, so the release must not act (`FLAG_CANCELED`, with `FLAG_CANCELED_LONG_PRESS`). |
| `getDownTime()` / `getEventTime()` | `SystemClock.elapsedRealtime()` of the press and of this edge. |
| `dispatch(Callback, DispatcherState, Object)` | Android's dispatch: runs `onKeyDown` / `onKeyLongPress` / `onKeyUp` on the `Callback` and keeps the tracking and cancel state in the `DispatcherState`. The Activity calls it for every event no view took. |

`KeyEvent.Callback` is the four-method interface `Activity` implements: `onKeyDown`, `onKeyLongPress`, `onKeyUp` and `onKeyMultiple(int keyCode, int count, KeyEvent event)`, which exists for Android's signature and is never called. `KeyEvent.DispatcherState` has Android's `reset()`, `reset(Object target)`, `startTracking(KeyEvent, Object)`, `isTracking(KeyEvent)`, `performedLongPress(KeyEvent)` and `handleUpEvent(KeyEvent)`, for code that calls `dispatch` itself.

`ViewConfiguration.getLongPressTimeout()`, `getKeyRepeatTimeout()` and `getKeyRepeatDelay()` give the timings (400, 400 and 50 ms).

The framework recycles one `KeyEvent` per edge: read it inside the callback, never keep it.

> **Idle wake:** if the display has gone to sleep (60 s with no input), the first button press wakes the display and is **not** delivered to listeners. Subsequent presses route normally.

See [`examples/keydemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/keydemo) for a complete example.

## Widgets

All widget classes live in `picodroid.widget.*` and extend `View` (`Button` through `TextView`, as on Android). They inherit `setPosition()`, `setSize()`, `setBackgroundColor()`, `setVisibility()`, and `close()` from `View`. Every widget has Android's `(Context)` constructor beside the no-argument one the examples here use (`new TextView(this)`).

### `picodroid.widget.TextView`

Displays a text label.

```java
import picodroid.text.TextUtils;
import picodroid.widget.TextView;

TextView label = new TextView();
label.setText("Hello, World!");                 // null is the empty string
label.setTextColor(Color.WHITE);
String current = label.getText().toString();   // CharSequence, as on Android

// One line, cut with an ellipsis when it does not fit the width the layout gives it.
label.setSingleLine();
label.setEllipsize(TextUtils.TruncateAt.END);
// Or at most two lines, the second cut.
label.setMaxLines(2);

// Where the text sits in a label wider than its text (a fixed width, match_parent or a weight).
label.setSize(120, 18);
label.setGravity(Gravity.RIGHT);                  // or CENTER_HORIZONTAL; LEFT/START is the default

// Bigger text: 28 sp, or any TypedValue unit.
label.setTextSize(28);
label.setTextSize(TypedValue.COMPLEX_UNIT_PX, 64);
int lineHeight = label.getLineHeight();   // the face actually in use
```

`setTextSize(float)`, `setTextSize(int unit, float)`, `getTextSize()` and `getLineHeight()` mirror Android, with one divergence: the faces are bitmaps, one per size the board compiles, so the text renders in the compiled face **nearest** the size asked for (a tie goes to the larger). The RP2350 boards compile Montserrat at 14, 20, 28 and 64 px; the RP2040 testbench compiles 14 alone, so every size snaps to it there. `getTextSize()` returns the size that was set, as on Android; `getLineHeight()` tells you which face you got (16 px for the default 14). There is one density — `px`, `dp` and `sp` are the same logical pixel — so `TypedValue.applyDimension` is an identity for those three, and `PT`, `IN` and `MM` convert through the panel's real pitch (`DisplayMetrics.xdpi`, the board file's `[display] dpi`); see `picodroid.util.TypedValue` and `DisplayMetrics`, which `Resources.getDisplayMetrics()` and `Display.getMetrics(DisplayMetrics)` hand out. The ladder is board configuration (`text_sizes` in the MCU or board toml, see [Advanced configuration](/reference/advanced-config/)); `android:textSize` in a layout does the same thing at inflation.

`setSingleLine()`, `setSingleLine(boolean)`, `setEllipsize(TextUtils.TruncateAt)`, `getEllipsize()`, `setMaxLines(int)` and `getMaxLines()` mirror Android over LVGL's label long modes:

- A single-line or max-lines view is **at most that many lines tall**. Give it a bounded width — a fixed `setSize` width, or a weight in a horizontal `LinearLayout` (`new LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f)`) — for the cut to happen; a view that sizes to its content never runs out of room. A taller explicit height shrinks to the limit (Android keeps the box and draws at the top).
- The ellipsis is three ASCII dots, `...` (the bundled font has no `…`). `START` and `MIDDLE` render like `END`; `MARQUEE` scrolls the text circularly whenever it is wider than the view, selected or not.
- Without an ellipsize, a single-line view clips the text at its edge (a content-sized one grows to the text's width), and a max-lines view clips the lines past the limit.
- `getText()` returns the full text while the ellipsis shows. A newline in the text still breaks the line.

`setGravity(int)` / `getGravity()` take [`picodroid.view.Gravity`](#picodroidwidgetlinearlayout) constants. The horizontal part aligns the text; the vertical part is kept and returned but not drawn, since a label is as tall as its text. `setIncludeFontPadding(false)` strips the whitespace above the glyphs so the label box hugs them (default `true`), and `setPadding` is `View`'s. `setMaxWidth(int)` / `getMaxWidth()` cap a `wrap_content` label's width (`android:maxWidth`), so a long line wraps or ellipsizes instead of pushing its neighbours off the panel.

### `picodroid.widget.Button`

A clickable button with a text label. Extends `TextView`, so `setText`, `getText`, `setTextColor`, `setTextSize` and the line-mode setters (`setSingleLine`, `setEllipsize`, `setMaxLines`, which act on the button's label) are the TextView methods and a `Button` can be passed wherever a `TextView` is expected. A content-sized button grows with its text size.

```java
import picodroid.view.View;
import picodroid.widget.Button;

Button btn = new Button("Tap Me!");
btn.setSize(200, 50);
btn.setText("New Label");

// Event-driven click handling — View.OnClickListener (onClick(View v))
btn.setOnClickListener(new View.OnClickListener() {
    public void onClick(View v) {
        Log.i("UI", "Button clicked!");
    }
});
// ...or a lambda, since OnClickListener is a single-method interface:
btn.setOnClickListener(v -> Log.i("UI", "Button clicked!"));
```

> **Typed listeners (since v0.10.0):** widget callbacks are Android-style single-method
> interfaces, not bare `Runnable`s. Each widget below names its interface — e.g.
> `View.OnClickListener`, `CompoundButton.OnCheckedChangeListener`,
> `SeekBar.OnSeekBarChangeListener`, `AdapterView.OnItemClickListener`,
> `AdapterView.OnItemSelectedListener`. Single-method interfaces accept a lambda wherever an
> anonymous class does; multi-method interfaces (like `OnItemSelectedListener`, which also
> declares `onNothingSelected`) need an anonymous class — exactly as on Android.

### `picodroid.widget.LinearLayout`

A container that arranges child widgets horizontally or vertically. Like Android's, it draws
nothing of its own: no background, border, corner radius or padding until you set one
(`setBackgroundColor`, a `GradientDrawable`, `setPadding`). Until 2026-09-24 every container came
with the LVGL theme's 2 px card border, rounded corners, fill and 13 px padding, which apps
stripped by hand; a layout that wants the card look now asks for it with a `GradientDrawable`
stroke.

```java
import picodroid.widget.LinearLayout;

LinearLayout layout = new LinearLayout();             // default: VERTICAL
layout.setOrientation(LinearLayout.HORIZONTAL);       // or VERTICAL
layout.setSize(320, 240);
layout.addView(textView);
layout.addView(button);
```

| Constant | Value |
|----------|-------|
| `LinearLayout.HORIZONTAL` | 0 |
| `LinearLayout.VERTICAL` | 1 |

| Method | Description |
|--------|-------------|
| `setOrientation(int)` | `HORIZONTAL` or `VERTICAL` (the default). |
| `setSpacing(int px)` | Gap between adjacent children; 0 by default. A picodroid addition. |
| `setGravity(int)` | Where the children go, in `picodroid.view.Gravity` constants, which may name both axes (`Gravity.BOTTOM` or-ed with `Gravity.RIGHT`). Call it after `setOrientation`. An axis the gravity does not name falls back to the start, as on Android (`START \| TOP`: a column's children sit at the left, a row's at the top). One divergence: `FILL` places at the start instead of stretching the child. |

`LinearLayout.LayoutParams(width, height, weight)` gives a child a share of the space left over, as on Android:

```java
row.addView(label, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
```

Weights are relative, with one decimal of precision (`1.5f` against `1f` is 15 : 10), and cap at 25.5. The per-child `LayoutParams.gravity` field (`android:layout_gravity` on a `LinearLayout` child) is stored but not applied: LVGL's flex layout has no per-item cross-axis alignment, and the layout compiler warns. To place one child differently, wrap it in a `FrameLayout` and give that the gravity. A `Space` with a weight is the Android way to push children apart ([below](#picodroidwidgetspace)).

`picodroid.view.Gravity` has Android's constants and values: `NO_GRAVITY`, `TOP`, `BOTTOM`, `LEFT`, `RIGHT`, `START`, `END`, `CENTER_VERTICAL`, `CENTER_HORIZONTAL`, `CENTER`, `FILL_VERTICAL`, `FILL_HORIZONTAL`, `FILL`, and the masks.

### `picodroid.widget.Space`

A view that draws nothing and takes room, as `android.widget.Space`. Given a weight in a `LinearLayout` it pushes its neighbours apart, a header's title to the left and its icon to the right whatever the panel's width:

```java
header.addView(title);
header.addView(new Space(this), new LinearLayout.LayoutParams(0, 0, 1f));
header.addView(icon);
```

In a layout file it is `<Space>`.

### `picodroid.widget.CompoundButton`

Abstract base for the two-state widgets below — `Switch`, `ToggleButton`, `CheckBox` and `RadioButton`. Mirrors
`android.widget.CompoundButton`. You don't instantiate it directly; it provides the shared checked
API and listener that each subclass inherits:

| Member | Description |
|--------|-------------|
| `boolean isChecked()` | Current checked state. |
| `void setChecked(boolean)` | Set checked state (does not fire the listener). |
| `void toggle()` | Flip the checked state. On `Switch` and `ToggleButton` only; on a `CheckBox` or `RadioButton` use `setChecked(!isChecked())`. |
| `setOnCheckedChangeListener(OnCheckedChangeListener)` | `onCheckedChanged(CompoundButton buttonView, boolean isChecked)` fires on user toggle. |

### `picodroid.widget.Switch`

An on/off toggle switch.

```java
import picodroid.widget.CompoundButton;
import picodroid.widget.Switch;

Switch sw = new Switch();
sw.setSize(60, 30);

boolean on = sw.isChecked();
sw.setChecked(true);
sw.toggle();

sw.setOnCheckedChangeListener(new CompoundButton.OnCheckedChangeListener() {
    public void onCheckedChanged(CompoundButton buttonView, boolean isChecked) {
        Log.i("UI", "Switch is now " + isChecked);
    }
});
```

`Switch` extends [`CompoundButton`](#picodroidwidgetcompoundbutton) (the shared base for
`Switch` / `ToggleButton` / `CheckBox` / `RadioButton`), which is where `isChecked()` /
`setChecked()` / `setOnCheckedChangeListener()` come from.

### `picodroid.widget.ToggleButton`

A button that toggles between two states with configurable text labels.

```java
import picodroid.widget.CompoundButton;
import picodroid.widget.ToggleButton;

ToggleButton toggle = new ToggleButton("ON", "OFF");  // or new ToggleButton()
toggle.setSize(200, 50);

boolean on = toggle.isChecked();
toggle.setChecked(true);
toggle.toggle();
toggle.setTextOn("Enabled");
toggle.setTextOff("Disabled");

toggle.setOnCheckedChangeListener(new CompoundButton.OnCheckedChangeListener() {
    public void onCheckedChanged(CompoundButton buttonView, boolean isChecked) {
        Log.i("UI", "Toggle is now " + isChecked);
    }
});
```

### `picodroid.widget.ImageView`

Displays an image bundled in the PAPK's `assets/` directory. Asset names are resolved against the PAPK ASSETS section at boot — see [Bundled image assets](/guides/assets/) for the manifest format and pipeline.

```java
import picodroid.widget.ImageView;

ImageView img = new ImageView();
img.setImageSource("icon.png");
img.setImageResource(R.drawable.logo);   // a PNG under res/drawable/, see resources
img.setImageDrawable(drawable);   // a Drawable, e.g. another app's icon from the PackageManager
```

`setImageResource` throws `Resources.NotFoundException` for an id that is not one of the app's drawables; see [resources](/guides/resources/).

Scale, tint, and aspect controls (Tier C):

```java
img.setScaleType(ImageView.SCALE_FIT_CENTER);
img.setScale(ImageView.SCALE_1X * 3 / 2);   // 256 = 1.0×, so this is 1.5× — uses LVGL transforms
img.setTint(Color.RED);     // recolours the image; the colour's alpha is the blend strength
```

| Constant | Value | Meaning |
|----------|-------|---------|
| `ImageView.SCALE_FIT_CENTER` | 0 | Fit inside the view, aspect kept, centred. |
| `ImageView.SCALE_CENTER_CROP` | 1 | Fill the view, aspect kept; the image may be cropped. |
| `ImageView.SCALE_FIT_XY` | 2 | Stretch to the view's width and height. |
| `ImageView.SCALE_TILE` | 3 | Tile the image across the view. No Android counterpart. |
| `ImageView.SCALE_CENTER` | 4 | Centre at the image's own size; clipped if larger than the view. |
| `ImageView.SCALE_1X` | 256 | The `setScale` unit for 1.0×. |

`setTint(int argb)`: alpha 0 leaves the image alone, 255 recolours it fully. There is no `FIT_START`.

Anti-aliased scale and rotation rendering depends on LVGL 9.6.0's `LV_DRAW_SW_SUPPORT_RGB565A8` (enabled in `lv_conf.h`). Without it scaled images render aliased — see [Advanced configuration → lv_conf.h](/reference/advanced-config/#lv_confh).

### `picodroid.widget.ProgressBar`

A horizontal progress bar with Android's range and tint API.

```java
import picodroid.content.res.ColorStateList;
import picodroid.widget.ProgressBar;

ProgressBar bar = new ProgressBar();
bar.setSize(200, 20);
bar.setMax(250);                  // the range is [getMin(), getMax()], 0..100 by default
bar.setProgress(75);              // instant, clamped to the range
bar.setProgress(120, true);       // animated over 80 ms, as on Android
bar.incrementProgressBy(10);
bar.setProgressTintList(ColorStateList.valueOf(Color.GREEN));            // the fill
bar.setProgressBackgroundTintList(ColorStateList.valueOf(0x40FFFFFF));  // the track; alpha honoured
bar.setProgressTintList(null);    // back to the theme colour
```

`getProgress()` returns the value last set (the target while an animation runs), and `setMax` /
`setMin` pull a progress outside the new range back into it, as on Android. `ColorStateList` is a
single colour — `valueOf(int)`, `getDefaultColor()`, `withAlpha(int)` — with no state sets
(`isStateful()` is `false` and `getColorForState` returns the one colour). Each tint setter has its
getter, and `getMax()`, `getMin()` and `isIndeterminate()` read the rest.

For an **indeterminate** spinner (no progress value, just an animation while work is happening), use the static factory:

```java
ProgressBar spinner = ProgressBar.indeterminate();
spinner.setSize(48, 48);
spinner.setIndeterminateTintList(ColorStateList.valueOf(Color.RED));  // the arc; Theme.colorPrimary by default
// Add to layout; remove or hide when work completes.
```

`indeterminate()` returns a `ProgressBar` backed by `lv_spinner` and ignores `setProgress`; the mode
is fixed at construction (there is no `setIndeterminate(boolean)`). A tint set on the flavour that is not showing is kept for its getter but
not drawn. `setTint(int)` is a deprecated alias of `setIndeterminateTintList`.

### `picodroid.widget.CircularProgressIndicator`

A determinate ring gauge: the progress is an arc over a circular track. It is Material Components'
`CircularProgressIndicator` (a `ProgressBar` subclass) with the same method names, folded into
`picodroid.widget` like `Snackbar`, and backed by LVGL's `lv_arc`.

```java
import picodroid.widget.CircularProgressIndicator;

CircularProgressIndicator ring = new CircularProgressIndicator(this);
ring.setIndicatorSize(72);          // diameter in px; the view is square
ring.setTrackThickness(8);          // stroke of both the track and the arc
ring.setIndicatorColor(Color.GREEN);
ring.setTrackColor(0xFF2E2A26);
ring.setProgress(42);               // ProgressBar's API, 0..100
```

Material's knobs: `setIndicatorColor(int)`, `setTrackColor(int)` (a colour's alpha is honoured, so
`Color.TRANSPARENT` hides the track), `setTrackThickness(int)`, `setIndicatorSize(int)`,
`setIndicatorDirection(INDICATOR_DIRECTION_CLOCKWISE | INDICATOR_DIRECTION_COUNTERCLOCKWISE)` and
`setTrackCornerRadius(int)` (any positive radius rounds the caps, `0` squares them; rounded by
default), each with its getter.

Two picodroid extensions, for gauges that are not a full circle, in `Canvas.drawArc` terms — degrees,
0 at 3 o'clock, clockwise: `setStartAngle(float)` and `setSweepAngle(float)` (0..360, clamped).
The defaults, 270 and 360, draw a full ring filling from 12 o'clock; a dashboard dial open at the
bottom is `setStartAngle(135); setSweepAngle(270)`. Angles are rounded to whole degrees.

Divergences from Material: determinate only (for a spinner use `ProgressBar.indeterminate()`);
one indicator colour, so `getIndicatorColor()` returns an `int`; no `indicatorInset`;
`indicatorSize` is the intrinsic size and explicit layout dimensions win over it. Inflatable from
XML as `<CircularProgressIndicator>` with `indicatorColor`, `trackColor`, `trackThickness`,
`indicatorSize`, `startAngle` and `sweepAngle` (see [resources](/guides/resources/)).

### `picodroid.widget.ListView`

A scrollable list. Add plain text items directly, or back it with an `Adapter` for data-driven
lists with stable item IDs and D-pad item selection.

```java
import picodroid.widget.ListView;

ListView list = new ListView();
list.setSize(200, 150);
list.addItem("Item 1");
list.addItem("Item 2");
list.addItem("Item 3");
```

**Adapter-backed (the Tier 2 `Adapter` pattern, since v0.10.0).** Bind an `ArrayAdapter` and
receive typed click callbacks. Each item renders as a `TextView` showing its `toString()`:

```java
import picodroid.widget.ArrayAdapter;
import picodroid.widget.ListView;

String[] rows = { "Live", "History", "Settings" };
ListView list = new ListView();
list.setAdapter(new ArrayAdapter<String>(rows));

// onItemClick(AdapterView<?> parent, View view, int position, long id) — usable as a lambda:
list.setOnItemClickListener((parent, view, position, id) -> open(position));
```

**Custom rows: `getView` and `convertView`.** As on Android, an adapter builds each row in
`getView(position, convertView, parent)` and re-binds the `convertView` it is handed back:

```java
class ReadingAdapter extends BaseAdapter {
  private final Reading[] data;
  ReadingAdapter(Reading[] data) { this.data = data; }

  @Override public int getCount() { return data.length; }
  @Override public Object getItem(int i) { return data[i]; }
  @Override public long getItemId(int i) { return i; }

  @Override
  public View getView(int position, View convertView, ViewGroup parent) {
    LinearLayout row = (LinearLayout) convertView;
    if (row == null) {                       // first time: build the row
      row = new LinearLayout(ctx);
      row.setOrientation(LinearLayout.VERTICAL);
      row.addView(new TextView(ctx));
      row.addView(new TextView(ctx));
    }
    Reading r = data[position];              // every time: bind the data
    ((TextView) row.getChildAt(0)).setText(r.name);
    ((TextView) row.getChildAt(1)).setText(r.value + " " + r.unit);
    return row;
  }
}
```

`ListView` asks `getView` for every position and keeps the rows as its children in position
order (`getChildAt(i)` is row *i*). `notifyDataSetChanged()` offers each existing row back as
`convertView`, so a data change re-binds rows in place instead of rebuilding them; a position
past the new count frees its row, and `setAdapter` always starts from fresh rows. Whatever view
`getView` returns is stretched to the list's width, padded, made clickable and keypad-focusable
and highlighted when focused, and `onItemClick` receives it as `view`. A row returned instead of
the `convertView` frees the `convertView`; do not add the row to `parent` yourself. A layout
resource works too: `LayoutInflater.from(ctx).inflate(R.layout.row, parent, false)` on a `null`
`convertView`, then `findViewById` into it.

> **The `Adapter` family.** `ListView` extends `AdapterView<Adapter>`. The pieces:
> - `Adapter` — interface: `getCount()`, `getItem(int)`, `getItemId(int)`,
>   `getView(int, View, ViewGroup)`.
> - `BaseAdapter` — abstract base with `notifyDataSetChanged()` (call after mutating data).
> - `ArrayAdapter<T>` — concrete `BaseAdapter` over a `T[]` or a `List<T>`, or built
>   incrementally with `add` / `addAll` / `insert` / `remove` / `clear` (`getPosition` finds an
>   item); renders each item's `toString()` in a `TextView`, or in Android's layout-resource form
>   — `new ArrayAdapter<>(ctx, R.layout.row, items)` inflates `row` (a `TextView`) per item, and
>   `new ArrayAdapter<>(ctx, R.layout.row, R.id.title, items)` puts the text into that child.
> - `AdapterView<T>.setOnItemClickListener(OnItemClickListener)` — the 4-arg `onItemClick` above.
>
> Every row is a live widget — there is no off-screen recycling — so on memory-constrained boards
> the LVGL renderer makes very long focusable lists expensive; keep adapter-backed data lists
> modest in length (a dozen rows is comfortable everywhere).

### `picodroid.widget.CheckBox`

A labelled checkable box.

```java
import picodroid.widget.CheckBox;
import picodroid.widget.CompoundButton;

CheckBox cb = new CheckBox();
cb.setText("Enable WiFi");
cb.setChecked(true);
boolean on = cb.isChecked();

cb.setOnCheckedChangeListener(new CompoundButton.OnCheckedChangeListener() {
    public void onCheckedChanged(CompoundButton buttonView, boolean isChecked) {
        Log.i("UI", "checked=" + isChecked);
    }
});
```

### `picodroid.widget.RadioButton` and `RadioGroup`

A `RadioButton` is a two-state button with a circular indicator; a `RadioGroup` is a vertical
`LinearLayout` that keeps at most one of the `RadioButton`s added to it checked. Both mirror
`android.widget`.

```java
import picodroid.widget.RadioButton;
import picodroid.widget.RadioGroup;

RadioGroup group = new RadioGroup();
RadioButton celsius = new RadioButton();
celsius.setText("Celsius");
celsius.setId(1);                     // optional: a button without an id is given one
RadioButton fahrenheit = new RadioButton();
fahrenheit.setText("Fahrenheit");
group.addView(celsius);
group.addView(fahrenheit);
group.check(celsius.getId());

group.setOnCheckedChangeListener(new RadioGroup.OnCheckedChangeListener() {
    public void onCheckedChanged(RadioGroup group, int checkedId) {
        Log.i("UI", "checked id=" + checkedId);
    }
});
```

| `RadioGroup` method | Description |
|--------|-------------|
| `check(int id)` | Check the button with that id, unchecking the previous one. |
| `clearCheck()` | Uncheck everything; the listener fires with `View.NO_ID`. |
| `getCheckedRadioButtonId()` | The checked button's id, or `View.NO_ID`. |
| `setOnCheckedChangeListener(RadioGroup.OnCheckedChangeListener)` | `onCheckedChanged(RadioGroup group, int checkedId)`, once per change of selection. |

Listen on the group, not on its buttons: the group puts its own checked-change listener on every
button it tracks. A checked radio in a group stays checked when tapped again, and
`RadioButton.setChecked(true)` moves the group's selection, as on Android. A `RadioButton` outside
a group toggles like a `CheckBox`.

### `picodroid.widget.SeekBar`

A horizontal slider (0–`max`).

```java
import picodroid.widget.SeekBar;

SeekBar bar = new SeekBar(100);   // or `new SeekBar()` for default max
bar.setMax(200);
bar.setProgress(25);
int p = bar.getProgress();

bar.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
    public void onProgressChanged(SeekBar seekBar, int progress, boolean fromUser) {
        Log.i("UI", "progress=" + progress);
    }
    // onStartTrackingTouch(SeekBar) / onStopTrackingTouch(SeekBar) have default no-op bodies
});
```

### `picodroid.widget.Spinner`

A drop-down list. Items are passed as a single newline-separated string, or come from an
adapter: `Spinner` extends `AdapterView<Adapter>` like `ListView`, so
`sp.setAdapter(new ArrayAdapter<String>(items))` works and each item renders via its `toString()`.

```java
import picodroid.widget.Spinner;

Spinner sp = new Spinner();
sp.setItems("Red\nGreen\nBlue");
int sel = sp.getSelectedItemPosition();

sp.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
    public void onItemSelected(AdapterView<?> parent, View view, int position, long id) {
        Log.i("UI", "sel=" + position);   // view is null: rows render natively
    }

    public void onNothingSelected(AdapterView<?> parent) {}   // never called: there is always a selection
});
```

### `picodroid.widget.EditText`

A single-line text input field. Tapping it pops up the system soft keyboard at the bottom of the screen by default.

```java
import picodroid.widget.EditText;

EditText input = new EditText();
input.setHint("device name");
input.setText("pico-01");
String value = input.getText();
input.setShowKeyboardOnTouch(false);   // disable system keyboard for this field
```

`getText()` returns a `String`. An `EditText` is focusable by default, as on Android, so on a
keypad board it takes the focus ring's select and ENTER opens the keyboard.

`addTextChangedListener(TextWatcher)` / `removeTextChangedListener(TextWatcher)` take a
`picodroid.text.TextWatcher`. Its three callbacks take `String` rather than `CharSequence` /
`Editable`, and only `afterTextChanged(String s)` is ever called, with the full new text;
`beforeTextChanged` and `onTextChanged` have empty default bodies and exist so ported code
compiles. As on Android, `setText` fires the watcher too.

```java
import picodroid.text.TextWatcher;

input.addTextChangedListener(new TextWatcher() {
    public void afterTextChanged(String s) { save.setEnabled(s.length() > 0); }
});
```

#### `OnEditorActionListener`

Fires when the user presses the keyboard's Done / Send key. Lets you commit the value without the user having to tap elsewhere first.

```java
import picodroid.widget.EditText;
import picodroid.widget.OnEditorActionListener;
import picodroid.view.KeyEvent;

input.setOnEditorActionListener(new OnEditorActionListener() {
    public boolean onEditorAction(EditText v, int actionId, KeyEvent event) {
        save(v.getText());   // event is null for the synthesized soft-keyboard OK
        return true;         // true = handled, the keyboard stays up; false = it hides as usual
    }
});
```

`actionId` is always `EditorInfo.IME_ACTION_DONE`. The listener fires for the system keyboard
only; an explicit [`Keyboard`](#picodroidwidgetkeyboard) instance reports through its
`OnReadyListener`.

#### `EditorInfo` hints

`picodroid.view.inputmethod.EditorInfo` holds the `IME_ACTION_*` codes an `OnEditorActionListener`
receives, with Android's values (`IME_ACTION_UNSPECIFIED` 0, `NONE` 1, `GO` 2, `SEARCH` 3, `SEND` 4,
`NEXT` 5, `DONE` 6, `PREVIOUS` 7); the soft keyboard's OK key emits `IME_ACTION_DONE` alone.

Which keyboard a field opens is its input type, set with `picodroid.text.InputType` constants as
on Android:

```java
import picodroid.text.InputType;

input.setInputType(InputType.TYPE_CLASS_NUMBER);   // the digit pad
input.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD);   // masked
```

The class picks the layout: `TYPE_CLASS_NUMBER` and `TYPE_CLASS_PHONE` open the digit pad,
everything else the text layout. The password variations mask the field with bullets, showing each
character briefly as it is typed; `getText()` still reads the real text. The other variations and
the flag bits are accepted and change nothing.

See [`picodroid.widget.Keyboard`](#picodroidwidgetkeyboard) for the soft keyboard widget.

### `picodroid.widget.ScrollView`

A vertically scrollable container with a single child (typically a `LinearLayout`). As on
Android it draws no border and has square corners; give it a `GradientDrawable` background if
you want an outline.

On a board whose panel can scroll its own frame memory (the ST7796 touch board), a
full-width `ScrollView` scrolls in hardware: a step renders only the rows that scrolled in
rather than the whole viewport. That needs the view's own paint to be the same on every
row — a flat fill, no top or bottom border line, no rounded corners — and anything drawn on
top of it (a toast, a floating child) is repainted after each step. A view that does not
qualify simply repaints in full, as every other board does.

```java
import picodroid.widget.ScrollView;
import picodroid.widget.LinearLayout;

ScrollView scroll = new ScrollView();
scroll.setSize(320, 240);
LinearLayout content = new LinearLayout();
content.setOrientation(LinearLayout.VERTICAL);
// ... addView(...) lots of children ...
scroll.addView(content);
```

### `picodroid.widget.FrameLayout`

A simple container that stacks children (last `addView` is on top). Useful for overlays such as a status badge over an `ImageView`. Flat by default, like `LinearLayout` above.

```java
import picodroid.view.Gravity;
import picodroid.widget.FrameLayout;

FrameLayout overlay = new FrameLayout();
overlay.addView(background);

// Bottom-right, 4 px in from each edge.
FrameLayout.LayoutParams lp =
    new FrameLayout.LayoutParams(24, 24, Gravity.RIGHT | Gravity.BOTTOM);
lp.setMargins(0, 0, 4, 4);
overlay.addView(badge, lp);
```

`FrameLayout.LayoutParams` places a child as Android's does: against the corner, edge or centre its
`gravity` names (`TOP | LEFT` when it names none), moved in by the margins on that side. With no
gravity, `leftMargin` and `topMargin` are the child's position. A child added without params is
positioned with `setPosition`. In a layout file these are `android:layout_gravity` and
`android:layout_margin*`.

### `picodroid.widget.ViewPager2`

Pages of [fragments](#picodroidappfragment), one on screen at a time, the shape of `androidx.viewpager2.widget.ViewPager2` with a `FragmentStateAdapter` (both in `picodroid.widget`). It goes in a layout as `<ViewPager2 android:id="@+id/pager" …/>` or is built with `new ViewPager2(context)`:

```java
ViewPager2 pager = findViewById(R.id.pager);
pager.setAdapter(new FragmentStateAdapter(this) {
  @Override public int getItemCount() { return 4; }
  @Override public Fragment createFragment(int position) { return PageFragment.newInstance(position); }
});
pager.registerOnPageChangeCallback(new ViewPager2.OnPageChangeCallback() {
  @Override public void onPageSelected(int position) { dots.select(position); }  // the dots-row idiom
});

// A board with keys rather than a touch panel:
@Override public boolean onKeyDown(int keyCode, KeyEvent event) {
  if (keyCode == KeyEvent.KEYCODE_DPAD_DOWN) { pager.setCurrentItem(pager.getCurrentItem() + 1, false); return true; }
  return super.onKeyDown(keyCode, event);
}
```

| Method | Description |
|--------|-------------|
| `setAdapter(FragmentStateAdapter)` / `getAdapter()` | The adapter's `createFragment(position)` makes each page's fragment, added with the tag `"f" + itemId` (Android's spelling; as there it is an implementation detail, so have each page observe its data through a shared [`ViewModel`](#picodroidlifecycle) rather than look the current page up by tag); `getItemCount()`, `getItemId(position)`, `containsItem(itemId)`, `notifyDataSetChanged()`. The first page appears on a later tick. |
| `setCurrentItem(int item[, boolean smoothScroll])` / `getCurrentItem()` | Turn to a page. `getCurrentItem` is the page asked for, even mid-turn. No wrap. |
| `registerOnPageChangeCallback` / `unregisterOnPageChangeCallback` | `onPageSelected(position)` once the new page is resumed; `onPageScrollStateChanged(SCROLL_STATE_SETTLING | SCROLL_STATE_IDLE)`; `onPageScrolled(position, 0f, 0)` once per turn. |
| `setUserInputEnabled(boolean)` / `isUserInputEnabled()` | Whether a swipe across the page turns it (on by default; off on a board without touch, where keys drive the pager). |
| `setOrientation(ORIENTATION_HORIZONTAL | ORIENTATION_VERTICAL)` / `getOrientation()` | Which swipes turn the page: left/right or up/down. Layout is unaffected. |
| `setOffscreenPageLimit(int)` / `getOffscreenPageLimit()` | Accepted for source compatibility and logged: one page is alive whatever the limit. |
| `getScrollState()` | `SCROLL_STATE_IDLE` or `SCROLL_STATE_SETTLING`; `SCROLL_STATE_DRAGGING` is never reported. |
| `saveState()` / `restoreState(Bundle)` | The page index and every page's saved state, for the Activity's `onSaveInstanceState` / `onCreate` (call `restoreState` before or after `setAdapter`). Android saves these through the view hierarchy; there is none here. |

An embedded panel has no room for the page beside the current one, so this pager keeps exactly one page alive: a turn removes the outgoing fragment (`onPause` … `onDestroyView` … `onDetach`; its `onSaveInstanceState` Bundle kept by the adapter under its item id, its widgets freed) and creates the incoming one (`onAttach` … `onStart`, then `onResume` once it is the page on screen), over three main-thread ticks so no single tick carries the whole turn. When that page comes back, `createFragment` makes a new instance and the Bundle returns through `setInitialSavedState`. There is no scroller: `smoothScroll` is a fade of the incoming page (180 ms), and the outgoing page is gone a tick before it appears. A page that builds its views over several ticks, as `claudeusage`'s do, turns with `setCurrentItem(i, false)` and fades its own root once painted. Page fragments are not saved with the Activity's other fragments (the manager cannot place them again); `saveState()` carries them, so call it from `onSaveInstanceState`.

Not provided: page transformers, fake drags, item decorations, `RecyclerView.Adapter`, the `(Fragment)` adapter constructor (no child fragment managers), `android:orientation` in XML. A swipe anywhere over the page turns it, one that starts on a clickable child (a `Button`) included; a child with an `OnSwipeListener` of its own keeps the swipe (see [Swipe gestures](#swipe-gestures)).

See [`examples/pagerdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/pagerdemo) (three pages, a dots row, the states across `recreate()` and a reclaim) and [`examples/claudeusage/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/claudeusage) (four screens on a four-button board).

### `picodroid.widget.Toast`

Brief, non-modal, auto-dismissing message bubble — Android-style.

```java
import picodroid.widget.Toast;

// First arg is a Context — `this` inside an Activity.
Toast.makeText(this, "Saved.", Toast.LENGTH_SHORT).show();
```

| Constant | Value | Default duration |
|----------|-------|-----------------|
| `Toast.LENGTH_SHORT` | 0 | ~2 s |
| `Toast.LENGTH_LONG` | 1 | ~3.5 s |

| Method | Description |
|--------|-------------|
| `Toast.makeText(Context ctx, String text, int duration)` | Static factory. |
| `show()` | Display the toast. |
| `cancel()` | Dismiss before the timeout expires. |
| `setDuration(int)` / `getDuration()` | `LENGTH_SHORT` or `LENGTH_LONG`. A change takes effect at the next `show()`; a toast already on screen keeps its deadline. |

### `picodroid.app.AlertDialog`

Modal dialog with a title, a message or a list, and up to three buttons. Built via the nested `Builder` (`new AlertDialog.Builder()` or `new AlertDialog.Builder(context)`).

```java
import picodroid.content.DialogInterface;
import picodroid.app.AlertDialog;

new AlertDialog.Builder()
    .setTitle("Erase data?")
    .setMessage("This cannot be undone.")
    .setPositiveButton("Erase", new DialogInterface.OnClickListener() {
        public void onClick(DialogInterface dialog, int which) { eraseAll(); }
    })
    .setNegativeButton("Cancel", null)
    .show();
```

| `Builder` method | Description |
|-----|-------------|
| `setTitle(String)` | Dialog title (top bar). |
| `setMessage(String)` | Body text. |
| `setPositiveButton(String text, DialogInterface.OnClickListener listener)` | Confirm button. `onClick(DialogInterface, int which)`; `listener` may be null. `which` is `DialogInterface.BUTTON_POSITIVE`. |
| `setNegativeButton(String text, DialogInterface.OnClickListener listener)` | Dismiss button. `listener` may be null. `which` is `DialogInterface.BUTTON_NEGATIVE`. |
| `setNeutralButton(String text, DialogInterface.OnClickListener listener)` | Third button, placed leftmost. `which` is `DialogInterface.BUTTON_NEUTRAL`. |
| `setItems(String[] items, DialogInterface.OnClickListener listener)` | A tappable list: a row dismisses the dialog and reports its index as `which`. |
| `setSingleChoiceItems(String[] items, int checkedItem, DialogInterface.OnClickListener listener)` | A radio-style list with `checkedItem` selected (`-1` for none). A tap selects the row and reports its index; the dialog stays open. |
| `setMultiChoiceItems(String[] items, boolean[] checkedItems, DialogInterface.OnMultiChoiceClickListener listener)` | A checkbox list. `checkedItems` seeds the state (`null` for all unchecked) and is updated in place as rows toggle; `onClick(DialogInterface dialog, int which, boolean isChecked)` fires per toggle. |
| `create()` | Returns an `AlertDialog` without showing it. |
| `show()` | Convenience: `create()` + `show()`. |

A list holds at most 12 rows (`IllegalArgumentException` past that), and a message set beside a list wins: the list is dropped, as on Android.

```java
String[] units = {"Celsius", "Fahrenheit", "Kelvin"};
new AlertDialog.Builder()
    .setTitle("Units")
    .setSingleChoiceItems(units, 0, (dialog, which) -> selected = which)
    .setPositiveButton("OK", null)
    .show();
```

A button click runs its listener (if any) and then dismisses the dialog. The button constants are Android's: `DialogInterface.BUTTON_POSITIVE` (-1), `BUTTON_NEGATIVE` (-2), `BUTTON_NEUTRAL` (-3). BACK dismisses the topmost showing dialog. Call `dialog.dismiss()` (or `cancel()`) to close programmatically; a second `dismiss()` is a no-op, as on Android. Unlike Android, dismissing **frees the dialog's widgets**, so a dismissed dialog cannot be shown again — `show()` throws `IllegalStateException`; build a new one. See [`examples/dialogdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/dialogdemo).

### `picodroid.widget.Keyboard`

On-screen soft keyboard, wrapping LVGL's `lv_keyboard`. Two ways to use:

**System keyboard (default).** Any `EditText` pops up a singleton system keyboard at the screen bottom on touch — no setup needed. Dismissed by BACK or the keyboard's OK key.

**Explicit instance** for custom placement or styling:

```java
import picodroid.widget.EditText;
import picodroid.widget.Keyboard;

EditText input = new EditText();
input.setShowKeyboardOnTouch(false);   // disable system keyboard for this field

Keyboard kb = new Keyboard();
kb.setEditText(input);
kb.setMode(Keyboard.MODE_TEXT_LOWER);
kb.setPosition(0, 120);
kb.setSize(320, 120);
kb.setOnReadyListener(new Keyboard.OnReadyListener() {
    public void onReady(Keyboard keyboard) { keyboard.hide(); /* validate input here */ }
});
kb.show();
```

| Constant | Value |
|----------|-------|
| `Keyboard.MODE_TEXT_LOWER` | 0 |
| `Keyboard.MODE_TEXT_UPPER` | 1 |
| `Keyboard.MODE_SPECIAL` | 2 |
| `Keyboard.MODE_NUMBER` | 3 |

LVGL switches modes internally as the user taps the keyboard's "abc"/"ABC"/"123"/"!@#" toggle keys. v1 caveats: US-English layout only; explicit instances do not auto-hide on the OK key (the listener decides).

**Polish pass (since v0.5.0).** The system keyboard:

- Slides up from the bottom edge over ~150 ms when an `EditText` gains focus, and slides back down on dismiss — no instant jump.
- Forwards the OK key to any `OnEditorActionListener` registered on the focused `EditText` before the default close behavior runs.
- Dismisses on tap-outside: tapping anywhere outside the keyboard rectangle (and outside the focused `EditText`) hides it.

See [`examples/keyboarddemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/keyboarddemo).

### `picodroid.widget.Snackbar`

Toast-with-an-action. Brief, auto-dismissing message bubble that optionally carries a clickable lozenge ("Undo", "Retry", etc.).

```java
import picodroid.view.View;
import picodroid.widget.Snackbar;

// make(View parent, String text, int duration) — duration is passed here, not via setDuration
Snackbar.make(rootView, "Item deleted", Snackbar.LENGTH_LONG)
    .setAction("Undo", new View.OnClickListener() {
        public void onClick(View v) { restoreItem(); }
    })
    .show();
```

| Constant | Value | Default duration |
|----------|-------|-----------------|
| `Snackbar.LENGTH_SHORT` | 0 | ~2 s |
| `Snackbar.LENGTH_LONG` | 1 | ~3.5 s |
| `Snackbar.LENGTH_INDEFINITE` | -1 | until manually dismissed |

If the user taps the action lozenge, the listener runs and the Snackbar dismisses immediately; the `View` passed to `onClick` is `null`, since the lozenge is not a `View`. Otherwise the Snackbar fades out after `duration`. `dismiss()` closes it from code, which is how a `LENGTH_INDEFINITE` one without an action goes away. Every Snackbar sits at the bottom of the screen, whichever `parent` is passed.

See [`examples/snackbardemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/snackbardemo).

### `picodroid.widget.DatePicker`

Calendar widget for picking a calendar date, backed by `lv_calendar`.

```java
import picodroid.widget.DatePicker;

DatePicker dp = new DatePicker();
dp.setSize(280, 220);
dp.setDate(2026, 5, 7);   // year, month (1–12), day-of-month

dp.setOnDateChangedListener(new DatePicker.OnDateChangedListener() {
    public void onDateChanged(DatePicker view, int year, int monthOfYear, int dayOfMonth) {
        Log.i("UI", "picked " + year + "-" + monthOfYear + "-" + dayOfMonth);
    }
});
```

`OnDateChangedListener` fires only on user interaction; `setDate` programmatically does not re-trigger the listener. `getYear()`, `getMonth()` (1–12) and `getDay()` return the date the user last tapped, and 0 until a day has been tapped.

### `picodroid.widget.NumberPicker`

Picks a number from a range, mirroring a subset of `android.widget.NumberPicker`. Android draws a
scroll wheel; picodroid shows the current value in a focusable box. On a keypad board ENTER on the
focused picker enters edit mode, PREV/NEXT step the value while focus navigation is suspended, and
ENTER or BACK leaves edit mode.

```java
import picodroid.widget.NumberPicker;

NumberPicker np = new NumberPicker();
np.setMinValue(0);
np.setMaxValue(10000);
np.setStep(100);          // picodroid extension: the change per step
np.setValue(500);

np.setOnValueChangedListener(new NumberPicker.OnValueChangeListener() {
    public void onValueChange(NumberPicker picker, int oldVal, int newVal) {
        Log.i("UI", oldVal + " -> " + newVal);
    }
});
```

| Method | Description |
|--------|-------------|
| `setMinValue(int)` / `getMinValue()`, `setMaxValue(int)` / `getMaxValue()` | The range; the current value is pulled into it. |
| `setValue(int)` / `getValue()` | The current value, clamped to the range. `setValue` does not notify the listener, as on Android. |
| `setStep(int)` / `getStep()` | How much one step changes the value (1 by default; values below 1 are treated as 1). No Android counterpart. |
| `setOnValueChangedListener(OnValueChangeListener)` | `onValueChange(NumberPicker picker, int oldVal, int newVal)`, once per change; a step at the edge of the range changes nothing and fires nothing. |

### `picodroid.widget.TimePicker`

Roller widget for picking a wall-clock time, backed by `lv_roller`. Defaults to 24-hour mode.

```java
import picodroid.widget.TimePicker;

TimePicker tp = new TimePicker();
tp.setSize(220, 180);
tp.setTime(14, 30);   // hour is 0..23, whatever the display mode
int hour = tp.getHour();      // 14
int minute = tp.getMinute();  // 30

tp.setOnTimeChangedListener(new TimePicker.OnTimeChangedListener() {
    public void onTimeChanged(TimePicker view, int hourOfDay, int minute) {
        Log.i("UI", "picked " + hourOfDay + ":" + minute);
    }
});
```

12-hour / AM-PM mode (since v0.7.0):

```java
tp.setIs24HourView(false);   // adds an AM/PM column; 14:30 shows as 2:30 PM
boolean is24 = tp.is24HourView();
```

The display mode changes what the rollers show, not the API: `setTime` and `getHour()` always use
0..23, as Android's `TimePicker` does since API 23, and switching modes keeps the time.

See [`examples/pickerdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/pickerdemo) for both pickers in one screen.

## Swipe gestures

### `picodroid.view.OnSwipeListener`

Per-View swipe primitive — sits beside `OnTouchListener` and fires on a recognized swipe direction.

```java
import picodroid.view.OnSwipeListener;
import picodroid.view.View;

view.setOnSwipeListener(new OnSwipeListener() {
    public void onSwipe(View v, int direction) {
        switch (direction) {
            case View.SWIPE_UP:    /* ... */ break;
            case View.SWIPE_DOWN:  /* ... */ break;
            case View.SWIPE_LEFT:  /* ... */ break;
            case View.SWIPE_RIGHT: /* ... */ break;
        }
    }
});
```

The `SWIPE_*` direction constants live on `View` (`View.SWIPE_LEFT` = 1, `SWIPE_RIGHT` = 2,
`SWIPE_UP` = 4, `SWIPE_DOWN` = 8). Direction is decided from the largest dominant axis with a
configurable minimum delta. Diagonal-only swipes do not fire. The listener fires once per gesture.

As on Android, the nearest view with a listener takes the swipe: a listener hears the swipes that
start on its view or on any descendant without a listener of its own, a clickable child such as a
`Button` included. A scrollable ancestor that can scroll in the swipe's direction takes the drag as
a scroll first.

### `picodroid.widget.SwipeRefreshLayout`

Container that triggers a refresh action when the user pulls down from the top of its child.

```java
import picodroid.widget.SwipeRefreshLayout;

SwipeRefreshLayout pull = new SwipeRefreshLayout();
pull.addView(scrollableContent);
pull.setOnRefreshListener(new SwipeRefreshLayout.OnRefreshListener() {
    public void onRefresh() {
        reload();
        pull.setRefreshing(false);   // dismiss the spinner when done
    }
});
```

A pull-down turns refreshing on and then calls `onRefresh()`; the spinner stays until
`setRefreshing(false)`, and a pull while it shows is ignored. `setRefreshing(true)` shows the spinner programmatically without firing the
listener. The layout wraps a single child and does not scroll itself. A pull-down that starts on a
descendant with an `OnSwipeListener` of its own goes to the layout first, which is Android's
`onInterceptTouchEvent`; swipes in every other direction stay with that descendant. See [`examples/swipedemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/swipedemo).

## Complete display app example

A minimal app that creates a button and updates a label when tapped:

```java
// CounterApp.java
package counter;

import picodroid.app.Application;
import picodroid.content.Intent;

public class CounterApp extends Application {
    public void onCreate() {
        startActivity(new Intent(CounterActivity.class));
    }
}
```

```java
// CounterActivity.java
package counter;

import picodroid.app.Activity;
import picodroid.debug.DisplayDebug;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class CounterActivity extends Activity {
    private int count = 0;

    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        DisplayDebug.calibrate();

        LinearLayout root = new LinearLayout();
        root.setOrientation(LinearLayout.VERTICAL);
        root.setSize(320, 240);

        TextView label = new TextView();
        label.setText("Count: 0");
        label.setTextColor(Color.WHITE);
        root.addView(label);

        Button btn = new Button("Increment");
        btn.setSize(200, 50);
        btn.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) {
                count = count + 1;
                label.setText("Count: " + count);
            }
        });
        root.addView(btn);

        setContentView(root);
    }
}
```

---

**See also:** [core.md](/api/core/) (Java language) · [system.md](/api/system/) (logging, clock, threads) · [peripherals.md](/api/peripherals/) (GPIO, UART, I2C, SPI, PWM, ADC) · [storage.md](/api/storage/) (files, preferences) · [networking.md](/api/networking/) (sockets)
