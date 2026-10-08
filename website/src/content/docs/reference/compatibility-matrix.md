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
| **Renamed** | Same shape, but only reachable as `picodroid.*` — not a real Android class. |
| **Unsupported** | No equivalent; use the listed alternative. |

## By package

### android.app

| API | Status | Notes / alternative |
|---|---|---|
| `Activity` | Full | Lifecycle (`onCreate`/`onStart`/`onResume`/`onPause`/`onStop`/`onRestart`/`onDestroy`), `onKeyDown`/`onKeyLongPress`/`onKeyUp` (`KeyEvent.Callback`; the fallback after the focused view's `OnKeyListener`, with Android's BACK tracking and long-press cancel), `onBackPressed`, `startActivity`, `startActivityForResult` + `onActivityResult`, `setResult`, `getIntent`, `finish`, `recreate` with `onSaveInstanceState` / `onRestoreInstanceState`, `setContentView(View)` / `setContentView(int)`, `findViewById`, `getSupportFragmentManager` (`FragmentActivity`'s, on `Activity` itself), `getLifecycle` / `getViewModelStore` / `getDefaultViewModelProviderFactory` (`ComponentActivity`'s), `runOnUiThread`. `onKeyMultiple` is declared and never called. No `dispatchKeyEvent`. |
| `Application` | Full | `onCreate` entry point. |
| `Service` | Partial | Started + bound services, `onRebind`, `stopSelf` / `stopSelfResult`, `startForeground` / `stopForeground` (the notification is a top-of-screen banner; nothing kills a background service, so "foreground" is about the banner). No `IntentService`. |
| `AlertDialog` / `AlertDialog.Builder` | Partial | Positive/negative/neutral buttons, `setItems`, single- and multi-choice. Titles, messages and labels are `String`. A dismissed dialog is freed and cannot be shown again (`show()` throws `IllegalStateException`). No `setView`, `setCancelable`, `setOnDismissListener` or `setIcon`. **List variants cap at ~12 rows** (LVGL renderer limit) and **a message set alongside items wins** (items are dropped, with a `Log.w`) — matching Android's message-vs-items precedence. |
| `Notification` / `NotificationManager` | Partial | Basic post/cancel. No channels, styles, or actions. |
| `AlarmManager` | Partial | Multi-app boards only. `set` / `setExact` (both exact) and `cancel`, on all four `RTC*` / `ELAPSED_REALTIME*` clocks. An alarm outlives the app that set it and starts it again to deliver it; alarms live in RAM and are lost at a reset, so re-register them at startup as an Android app does after `BOOT_COMPLETED`. No `setRepeating`, `setWindow`, `setAlarmClock` or `OnAlarmListener`. |
| `PendingIntent` | Partial | Multi-app boards only. `getActivity` alone, carrying at most two `int` extras under keys of at most 15 characters; `FLAG_NO_CREATE` is honoured, the other flags are accepted and ignored (a set always replaces by identity). No `getBroadcast` / `getService`. |
| `Fragment` / `FragmentManager` / `FragmentTransaction` (`androidx.fragment.app`, in `picodroid.app`) | Partial | Android's lifecycle callbacks in Android's order, driven by the host Activity; `add` / `replace` / `remove` / `hide` / `show` / `detach` / `attach`, `commit` (deferred) and `commitNow`, `addToBackStack` / `popBackStack` (BACK pops before `finish()`), `findFragmentById` / `ByTag`, `setArguments`, `onSaveInstanceState` saved with the Activity's Bundle. Deviations: a view given up in `onDestroyView` is freed at once (never re-added); lifecycle states are `int`s (`setMaxLifecycle(Fragment, int)`, no `Lifecycle.State`); `setInitialSavedState(Bundle)` in place of `SavedState`; after a re-creation fragments come back only through a `FragmentFactory` the app sets (`instantiate(String)`, no `ClassLoader`, no reflection — without one nothing is restored, with a warning). `getViewLifecycleOwner()` for `LiveData`; the fragment itself is not a `LifecycleOwner` or `ViewModelStoreOwner`. A skipped `super` in a lifecycle override is tolerated (no `SuperNotCalledException`). No child fragment managers, animations, `startActivityForResult`, `setRetainInstance`, menus. |
| `ViewModel` / `ViewModelProvider` (`androidx.lifecycle`, in `picodroid.lifecycle`) | Partial | `new ViewModelProvider(owner[, factory]).get(X.class)` / `get(key, X.class)`, `ViewModelProvider.Factory.create(Class)`, `ViewModel.onCleared()`, `ViewModelStore`, `ViewModelStoreOwner` and `HasDefaultViewModelProviderFactory` (both implemented by `Activity`). No reflection, so no default factory: override `Activity.getDefaultViewModelProviderFactory()` or pass one. A ViewModel lives as long as its Activity instance and is cleared on destroy, `recreate()` included (there are no configuration changes to keep it across). No `SavedStateHandle`, `AndroidViewModel`, `viewModelScope`; a fragment is not an owner. |
| `LiveData` / `MutableLiveData` / `Observer` / `Lifecycle` / `LifecycleOwner` (`androidx.lifecycle`, in `picodroid.lifecycle`) | Partial | `observe(owner, observer)` (active from `STARTED`, removed at `DESTROYED`), `observeForever`, `removeObserver(s)`, `setValue` (synchronous, main thread), `postValue` (any thread, last value wins), `getValue`, `hasObservers` / `hasActiveObservers`, `onActive` / `onInactive`. Owners: `Activity`, and `Fragment.getViewLifecycleOwner()`. `Lifecycle.getCurrentState()` is an `int` (no `Lifecycle.State` enum) and `Lifecycle` has `setCurrentState` (it is its own `LifecycleRegistry`). No `LifecycleObserver` / `DefaultLifecycleObserver`, `Lifecycle.Event`, `Transformations`, `MediatorLiveData`. |
| `Loader` | Unsupported | Load on a `Thread` or `ExecutorService` and post to the main executor. |

### android.view

| API | Status | Notes / alternative |
|---|---|---|
| `View` | Partial | Geometry/visibility/enabled/tag/id, `setTranslationX/Y`, `setRotation`, `setScaleX/Y` + getters (`getX() == getLeft() + getTranslationX()`), `OnClickListener` + `performClick`, `OnLongClickListener` + `performLongClick`, `OnTouchListener`, `OnKeyListener`, `OnFocusChangeListener` with `setFocusable` / `requestFocus`, `setAlpha`, `setPadding`, `setLayoutParams`, `getParent`, `setBackground` / `getBackground`, `setBackgroundColor`, `setBackgroundTintList` / `getBackgroundTintList` (the tint replaces the background's colour and keeps its shape; its alpha is ignored). A setter given the value the view already has (`setVisibility`, `setAlpha`, `setEnabled`, the same `Drawable` or an equal tint; `TextView.setText` and `setTextColor`) does nothing. `findViewById` (depth first, over `setId` / `android:id`). `VISIBLE` / `INVISIBLE` / `GONE` have Android's values (0, 4, 8). Additions with **no Android counterpart**: `setPosition`, `setSize`, `close()` (frees the widget) and `setOnSwipeListener` (`OnSwipeListener`, `View.SWIPE_*`). `onMeasure` / `setMeasuredDimension` / `measure` / `getMeasuredWidth` / `getMeasuredHeight` / `resolveSize` / `getDefaultSize` and `View.MeasureSpec`: asked of a view that draws itself, for a `wrap_content` dimension. No `post`/`postDelayed` (use `Executors.mainExecutor()` or animation timers), no `onLayout` / `onTouchEvent` / `getContext`. |
| `ViewGroup` | Partial | `addView(child)`, `addView(child, LayoutParams)`, `removeView`, `removeAllViews`, `getChildCount`, `getChildAt`, `LayoutParams` (`MATCH_PARENT`, `WRAP_CONTENT`) and `MarginLayoutParams` (`setMargins`; read when the child is added). `addView(child)` applies the `LayoutParams` the child carries. **`removeView` frees the child's widget**, so a removed view cannot be added again (`IllegalStateException`), and `addView` of a view that has a parent **moves** it (Android throws). No `addView(child, index)`, `indexOfChild` or `onInterceptTouchEvent`. |
| `ViewPropertyAnimator` | Partial | `animate()` with `alpha`, `x/y`, `translationX/Y`, `rotation`, `scaleX/Y`, `setDuration`, `setStartDelay`, `setInterpolator` (the four built-in curves), `withEndAction` — to-only, as on Android. Rotation/scale render through an off-screen layer of the view's size; keep transformed views small. |
| `MotionEvent` | Partial | `getX`/`getY` are **view-relative**, `getRawX`/`getRawY` are screen-absolute, matching Android. **Coordinates are `int`, not `float`** (no FPU). `ACTION_DOWN` / `ACTION_UP` / `ACTION_MOVE`, plus `ACTION_LONG_PRESS` (3), which has **no Android counterpart**. Single touch: no pointer ids or `getPointerCount`. |
| `GestureDetector` | Partial | `OnGestureListener` + `SimpleOnGestureListener` with three callbacks, **named and typed differently from Android's**: `void onSingleTap(MotionEvent)`, `void onLongPress(MotionEvent)`, `void onFling(MotionEvent down, MotionEvent up, float velocityX, float velocityY)`. The constructor takes the listener alone, and the detector is itself an `OnTouchListener` (no `onTouchEvent`). No `onDown`, `onScroll`, `onShowPress` or double tap; slop/fling use raw coordinates. |
| `KeyEvent` | Partial | D-pad codes, `KEYCODE_BACK` and `KEYCODE_HOME`, and the `KEYCODE_BUTTON_A` / `_B` / `_X` / `_Y` constants (no board maps a button to those yet: a board's buttons arrive as the D-pad / BACK codes its `board.toml` names); `getAction`, `getKeyCode`, `getRepeatCount`, `getFlags`, `isLongPress`, `isCanceled`, `startTracking`/`isTracking`, `getDownTime`/`getEventTime`, `dispatch` with `KeyEvent.Callback` and `KeyEvent.DispatcherState`. A held key auto-repeats after `ViewConfiguration.getKeyRepeatTimeout()` (400 ms) every `getKeyRepeatDelay()` (50 ms); the first repeat is the long-press. Both special keys are handled by the framework before an app sees them: BACK falls through the soft keyboard, a showing dialog, the focused View, then `Activity.onKeyDown`/`onKeyUp`, whose defaults run `onBackPressed`; HOME goes straight to the launcher and cannot be intercepted, as on Android. No `ACTION_MULTIPLE`, meta state or characters. Which physical buttons exist is `board.toml`'s business. |
| `ViewConfiguration` | Partial | `getLongPressTimeout`, `getKeyRepeatTimeout`, `getKeyRepeatDelay` (static, fixed per build). No touch slop, fling velocities or `get(Context)`. |
| `LayoutInflater` | Partial | `from`, `inflate(int, ViewGroup)`, `inflate(int, ViewGroup, boolean)`, and `Activity.setContentView(int)` / `getLayoutInflater()`. Layouts are compiled to binary at build time over a fixed attribute set. `<include>`, `style=` and `?attr/` are resolved at build time. A view class of the app's own is made by a `LayoutInflater.Factory` (`setFactory` / `getFactory`; every `Activity` is one, through `onCreateView(String, Context, AttributeSet)`), since there is no reflection; its `AttributeSet` is always empty. No `<merge>`, `Factory2` or custom attributes. See [resources](/guides/resources/). |
| `AsyncLayoutInflater` (`picodroid.view`) | Partial | androidx's `inflate(int, ViewGroup, OnInflateFinishedListener)`. Inflates on the main thread in slices of a few milliseconds per tick (Android: on a worker thread) and calls back on a tick of its own; retries after an `OutOfMemoryError`. A callback that no longer wants the view should `close()` it. |
| `Menu` | Unsupported | No menu resources or options menu. |

### android.widget

| API | Status | Notes / alternative |
|---|---|---|
| `TextView`, `Button`, `LinearLayout`, `FrameLayout`, `ScrollView`, `ImageView`, `Switch`, `CheckBox`, `ToggleButton`, `RadioButton`/`RadioGroup`, `ProgressBar`, `SeekBar`, `Toast`, `Spinner`, `NumberPicker`, `DatePicker`, `TimePicker`, `EditText`, `ListView` | Partial–Full | Core widgets present, each with the `(Context)` constructor and a no-argument one. Text setters take `String`. See specific divergences below. |
| `LinearLayout` | Partial | `setOrientation`, `setGravity` (an axis the gravity does not name keeps centring; `FILL` places at the start), `LayoutParams` with `weight` and margins (space kept clear around the child). `LayoutParams.gravity` is stored, not applied. Adds `setSpacing(int)`, which has **no Android counterpart** (a margin on the children is the Android spelling). No `setWeightSum`, dividers or baseline alignment. |
| `CompoundButton` | Partial | `isChecked`, `setChecked`, `setOnCheckedChangeListener`. `toggle()` is on `Switch` and `ToggleButton` only, not on `CheckBox` or `RadioButton`. `setChecked` does not fire the listener (Android's does). |
| `RadioGroup` | Partial | `check`, `clearCheck`, `getCheckedRadioButtonId`, `OnCheckedChangeListener`; buttons without an id are given one. |
| `NumberPicker` | Partial | `setMinValue` / `setMaxValue` / `setValue` and getters, `setOnValueChangedListener`. Drawn as a focusable box that the keys step, not a scroll wheel. Adds `setStep(int)`, which has **no Android counterpart**. No `setDisplayedValues`, `setWrapSelectorWheel`, `setFormatter`. |
| `DatePicker` | Partial | A calendar (`lv_calendar`). **Not Android's method names**: `setDate(year, month, day)` with **months 1–12** (Android's are 0–11), `getYear` / `getMonth` / `getDay` (0 until a day is tapped), `setOnDateChangedListener`. No `init`, `updateDate`, `getDayOfMonth`, min/max date. |
| `TimePicker` | Partial | Rollers. `getHour` / `getMinute` (hour always 0..23), `setIs24HourView` / `is24HourView`, `setOnTimeChangedListener`; `setTime(hour, minute)` in place of `setHour` / `setMinute`. |
| `SeekBar` | Partial | `setMax`, `setProgress`, `getProgress`, `OnSeekBarChangeListener` (the two tracking callbacks have default bodies, so a lambda works). A class of its own, not a `ProgressBar` subclass: no `setMin`, tints or `getMax`. |
| `Toast` | Partial | `makeText(Context, String, int)`, `show`, `cancel`, `setDuration` / `getDuration`. No `setGravity`, `setView`, or the resource-id `makeText`. |
| `Snackbar` (Material Components, in `picodroid.widget`) | Partial | `make(View, String, int)`, `setAction(String, View.OnClickListener)`, `show`, `dismiss`, the three `LENGTH_*` constants. Always at the bottom of the screen; the action's `onClick` receives a `null` view. No callbacks, `setActionTextColor`, `setAnchorView`. |
| `SwipeRefreshLayout` (`androidx.swiperefreshlayout.widget`, in `picodroid.widget`) | Partial | `setOnRefreshListener`, `setRefreshing`. A pull-down anywhere over the child refreshes (no check that the child is scrolled to its top). No `isRefreshing`, colour schemes or progress offsets. |
| `TextView.setTextSize` / `getTextSize` / `getLineHeight` | Partial | Both `setTextSize` overloads and every `TypedValue.COMPLEX_UNIT_*`. The faces are bitmaps, so a size snaps to the **nearest compiled face** (14, 20, 28, 64 px on the RP2350 boards; 14 alone on the RP2040 testbench) — `getLineHeight()` reports the face in use, `getTextSize()` the size set. One density: `px` = `dp` = `sp`. No `setTypeface`, `setTextScaleX` or autosizing. |
| `TextView.setGravity` / `getGravity` | Partial | The horizontal field (`LEFT`/`START`, `CENTER_HORIZONTAL`, `RIGHT`/`END`) aligns the text inside a view wider than it, as on Android; the vertical field is stored and returned but **not drawn** — an LVGL label is its text's height — so centre a label in a taller row through the parent `LinearLayout`'s gravity. `android:gravity` on `TextView` in layouts. |
| `LinearLayout` / `FrameLayout` / `RadioGroup` defaults | Full | Flat like Android's: no background, border, corner radius or padding until set. (Until 2026-09-24 they carried the LVGL theme's card border, fill and padding.) `setBackgroundColor` honours the colour's alpha, so `Color.TRANSPARENT` clears a background. |
| `TextView.setSingleLine` / `setEllipsize` / `setMaxLines` | Partial | Over LVGL's label long modes: the ellipsis is ASCII `...`, `START`/`MIDDLE` render like `END`, `MARQUEE` scrolls circularly whether or not the view is selected, and a single-line / max-lines view is **at most that many lines tall** — a taller explicit height shrinks to the limit. |
| `ProgressBar` | Partial | `setMax` / `setMin`, `setProgress(int, boolean animate)`, `incrementProgressBy` and the progress, progress-background and indeterminate tint lists are present. A `ColorStateList` is one colour (no state sets); `setProgressBackgroundTintList` and `View.setBackgroundColor` colour the same LVGL part; no `setSecondaryProgress`, `setProgressDrawable`, `setInterpolator` or tint mode. `indeterminate()` is **creation-time only** — there is no `setIndeterminate(boolean)` (LVGL can't morph bar↔spinner). |
| `CircularProgressIndicator` (Material Components, in `picodroid.widget`) | Partial | Determinate only; one indicator colour (`getIndicatorColor()` returns `int`); no `indicatorInset`. Adds `setStartAngle` / `setSweepAngle` (`Canvas.drawArc` convention), which have **no Material counterpart**. |
| `ViewPager2` (`androidx.viewpager2.widget`, in `picodroid.widget`) | Partial | `setAdapter(FragmentStateAdapter)`, `setCurrentItem(item[, smoothScroll])`, `getCurrentItem`, `registerOnPageChangeCallback` (`onPageSelected`, `onPageScrollStateChanged`, `onPageScrolled`), `setUserInputEnabled`, `setOrientation` / `getOrientation`, `getScrollState`, `<ViewPager2>` in XML. One page alive at a time: the outgoing page's fragment is removed (state kept) and its widgets freed before the incoming page's are made. No scroller: `smoothScroll` is a fade of the incoming page, the state goes SETTLING → IDLE (never DRAGGING), `onPageScrolled` fires once per turn with a zero offset; `setOffscreenPageLimit` is accepted and logged. A swipe over the page turns it, one that starts on a clickable child included. `saveState()` / `restoreState(Bundle)` are explicit (no view-state hierarchy). No page transformers, fake drags, item decorations, `RecyclerView.Adapter`. |
| `FragmentStateAdapter` (`androidx.viewpager2.adapter`, in `picodroid.widget`) | Partial | `createFragment`, `getItemCount`, `getItemId`, `containsItem`, `notifyDataSetChanged`; pages tagged `"f" + itemId` (Android's spelling, and as there an implementation detail: let a page observe its data rather than look it up by tag). Constructors take the `Activity` or the `FragmentManager` (no `Lifecycle`, no `(Fragment)` host). Page states are Bundles, saved through `ViewPager2.saveState()`, not with the Activity's other fragments. |
| `AdapterView` / `Adapter` / `BaseAdapter` / `ArrayAdapter` | Partial | `ListView` and `Spinner` extend `AdapterView<Adapter>`. `Adapter` is `getCount` / `getItem` / `getItemId`: **no `getView`**, a row is the item's `toString()`. `ArrayAdapter` is built from a `T[]` (with or without a `Context`) or filled with `add` / `clear`; no layout-resource constructors. `ListView.addItem(String)` and `Spinner.setItems(String)` are additions with **no Android counterpart**. |
| `AdapterView.OnItemClickListener` / `OnItemSelectedListener` | Partial | Full 4-arg `onItemClick` / `onItemSelected(parent, view, position, id)`; **`view` is always null** (LVGL rows have no Java wrapper). `onNothingSelected` is declared and never called. |
| `ImageView` | Partial | `setImageResource(R.drawable.*)`, `setImageDrawable`, and `setImageSource(String)` for a bundled asset name (see [assets](/guides/assets/)), which has **no Android counterpart**. Scale types are `int` constants, not the `ScaleType` enum: `SCALE_FIT_CENTER`, `SCALE_CENTER_CROP`, `SCALE_FIT_XY`, `SCALE_CENTER`, plus `SCALE_TILE`; no `FIT_START`. `setTint(int)` and `setScale(int)` (256 = 1.0×) in place of `setColorFilter` / `setImageMatrix`. |
| `EditText` + `TextWatcher` | Partial | Single-line. `getText()` returns `String`. `TextWatcher` takes **`String`** (no `CharSequence`/`Editable`); **only `afterTextChanged` fires** in v1. `setInputType` with `picodroid.text.InputType`: the class picks the keyboard (number and phone open the digit pad), the password variations mask the field, the rest is accepted and ignored. `OnEditorActionListener` is a top-level `picodroid.widget` interface taking the `EditText`; `actionId` is always `EditorInfo.IME_ACTION_DONE` and `event` always null. |

### android.util

| API | Status | Notes / alternative |
|---|---|---|
| `Log` (`v`/`d`/`i`/`w`/`e`) | Full | Maps to defmt levels on device; the simulator prints every level as `[Tag] msg`. Filter by tag/level with `pdb logcat --stdin`. |
| `TypedValue.COMPLEX_UNIT_*` / `applyDimension` | Partial | The six units and the conversion; nothing else of `TypedValue` (no `TYPE_*`, no `getDimension`). One density, so `PX`, `DIP` and `SP` are identities; `PT` / `IN` / `MM` use the panel's real pitch (`xdpi`), so they are ruler measurements. |
| `DisplayMetrics` | Partial | `widthPixels`, `heightPixels`, `density` (1), `densityDpi` (160), `scaledDensity` (1), `xdpi` / `ydpi` (the panel's physical pixels per inch, from the board file), `setToDefaults()`; from `Resources.getDisplayMetrics()` or `Display.getMetrics(DisplayMetrics)`. |

### android.graphics

| API | Status | Notes / alternative |
|---|---|---|
| `Color` | Full | Named constants + ARGB ints. |
| `drawable.GradientDrawable` | Partial | `setColor`, `setCornerRadius(int)`, `setStroke(width, color)`, each returning the drawable for chaining (Android's return `void`). A two-colour linear gradient through `setGradient(start, end, orientation)`, which has **no Android counterpart**, with `Orientation.TOP_BOTTOM` / `LEFT_RIGHT` as `int` constants. No shapes other than the rectangle, per-corner radii, radial or sweep gradients. |
| `drawable.Drawable` / `drawable.BitmapDrawable` | Partial | What `View.setBackground` and `ImageView.setImageDrawable` accept. `BitmapDrawable` is the icon `PackageManager.getApplicationIcon` returns; apps do not build one. A `res/drawable/*.xml` `<shape>` used as an `android:background` arrives as a `GradientDrawable`. No `draw(Canvas)`, bounds, state or `Drawable.Callback`. |
| `View.onDraw(Canvas)`, `View.invalidate()` / `postInvalidate()` | Partial | A view made with `new View(Context)` (or a subclass) draws what its `onDraw` draws. `onDraw` runs on the main thread after `invalidate()`, not every frame; `invalidate()` on a framework widget does nothing. Every RP2350 board; not the RP2040 testbench (`has_canvas = false`, flash). |
| `Canvas` | Partial | `drawColor`, `drawRect`, `drawRoundRect`, `drawCircle`, `drawLine`, `drawArc`, `drawText`, `getWidth`/`getHeight`. Recorded, not rasterised: about 60 calls per view. No `Path`, `Bitmap`, `save`/`restore`, clipping, transforms or `Rect`/`RectF` overloads; ovals and arcs are circular. |
| `Paint` | Partial | Colour and alpha, `Style`, stroke width, `Cap` (`SQUARE` draws as `BUTT`), text size (snaps to a compiled face) and align, `ascent`, `descent`, `measureText`. No shaders, path effects or typefaces; always anti-aliased. |
| `Bitmap` | Unsupported | No pixel buffers: a full-screen bitmap is 150 KB of RAM. |

### android.content

| API | Status | Notes / alternative |
|---|---|---|
| `Intent` | Partial | Explicit (class-targeted) intents, `new Intent(Context, Class)` or picodroid's `new Intent(Class)`, + extras (`int`, `long`, `boolean`, `String`, `Bundle`; `putExtras` / `getExtras`), `setClassName`, and on a multi-app board `setPackage`, which launches another installed app (extras do not cross over). No implicit intents / `IntentFilter` resolution, no `Parcelable` / `Serializable` extras. |
| `Context` | Partial | `getSystemService` (`SENSOR_SERVICE`, `NOTIFICATION_SERVICE`, `ALARM_SERVICE`, `STORAGE_STATS_SERVICE`, `CONNECTIVITY_SERVICE`, `WIFI_SERVICE`), `getPackageManager`, `getPackageName`, `getResources`, `getString(int)` / `getString(int, Object...)`, `getColor(int)`, `getSharedPreferences`, the private-file API (`getFilesDir`, `getDataDir`, `openFileInput` / `openFileOutput`, `fileList`, `deleteFile`), `startService` / `stopService`, `bindService(Intent, ServiceConnection, int)` with `BIND_AUTO_CREATE` / `unbindService`, `getMainExecutor`. `ServiceConnection` has Android's callbacks (`ComponentName`, `IBinder`); `os.Binder` is the class a LocalBinder extends. No `registerReceiver` (no `BroadcastReceiver`). |
| `pm.PackageManager` | Partial | `hasSystemFeature` (`FEATURE_WIFI`, `FEATURE_ETHERNET`), `getInstalledPackages`, `getPackageInfo`, `getLaunchIntentForPackage`, `getApplicationLabel`, `getApplicationIcon`, `getPackageInstaller`; `flags` arguments are ignored. No permissions, `resolveActivity` or `queryIntentActivities`. |
| `res.Resources` | Partial | `getString(int)` / `getString(int, Object...)`, `getText`, `getColor`, `getDimension`, `getDimensionPixelSize`, `getDimensionPixelOffset`, `getInteger`, `getBoolean`, `getDisplayMetrics`, `Resources.NotFoundException`. No `getDrawable`, arrays, plurals, `getConfiguration` or `getIdentifier`. See [resources](/guides/resources/). |
| `res.ColorStateList` | Partial | One colour: `valueOf`, `getDefaultColor`, `withAlpha`, `isStateful` (false), `getColorForState`. No state sets. |
| `SharedPreferences` / `Editor` | Partial | Backed by LittleFS. `getString`/`getInt`/`getLong`/`getFloat`/`getBoolean`, `getAll`, `contains`, and the matching `put*`, `remove`, `clear`, `commit`, `apply`. `Context.getSharedPreferences` returns one instance per file, safe from any thread; `apply()` updates memory at once and writes the file on a background thread (finished when the Activity stops), `commit()` writes before returning. A failed write leaves the new values in memory, as on Android. `SharedPreferences.open(name)` is picodroid's own: a separate instance read from the file. No `getStringSet`/`putStringSet`, no `OnSharedPreferenceChangeListener`. |
| `DialogInterface` | Partial | `dismiss`, `cancel`, `OnClickListener`, `OnMultiChoiceClickListener`, the three button constants. `OnDismissListener` is declared, but nothing takes one yet; no `OnCancelListener` or `OnShowListener`. |

### android.net

| API | Status | Notes / alternative |
|---|---|---|
| `ConnectivityManager` | Partial | `getSystemService(CONNECTIVITY_SERVICE)`; `registerDefaultNetworkCallback`, `registerNetworkCallback(NetworkRequest, cb)`, `requestNetwork`, `unregisterNetworkCallback`, `getActiveNetwork`, `getNetworkCapabilities`, the `TYPE_*` constants. No `getActiveNetworkInfo` (`NetworkInfo`'s methods are static), `getLinkProperties`, `bindProcessToNetwork`, `isActiveNetworkMetered`. |
| `ConnectivityManager.NetworkCallback` | Partial | `onAvailable`, `onCapabilitiesChanged`, `onLost` delivered on the main thread by the Activity event loop; `onLosing` / `onUnavailable` declared, never called. No `onLinkPropertiesChanged`, `onBlockedStatusChanged`. |
| `Network` | Partial | One per link-up; `getNetworkHandle`, `openConnection(URL)`, `equals`/`hashCode`/`toString`. No `getSocketFactory`, `bindSocket`, `getAllByName`. |
| `NetworkCapabilities` | Partial | `hasTransport`, `hasCapability`; the `TRANSPORT_*` / `NET_CAPABILITY_*` constants a board can show. `VALIDATED` means up with an address (no internet probe). No `Builder`, `getLinkDownstreamBandwidthKbps`, `getTransportInfo`. |
| `NetworkRequest` / `Builder` | Partial | `addTransportType`, `removeTransportType`, `addCapability`, `removeCapability`, `clearCapabilities`, `build`; `hasTransport`, `hasCapability`. No `setNetworkSpecifier`. |
| `NetworkInfo` | Partial | Static `isConnected()`, `getIpAddress()` (packed int), `getType()`; no instances. |
| `SntpClient` | Full | `requestTime(String host, int timeoutMs)`, `getNtpTime`, `getNtpTimeReference`, `getRoundTripTime`. Public here (hidden API on Android). Nothing sets the clock for you: anchor it with `SystemClock.setCurrentTimeMillis`. |
| `wifi.WifiManager` | Partial | `getSystemService(WIFI_SERVICE)`; `startScan`, `getScanResults`, `registerScanResultsCallback` / `unregisterScanResultsCallback`, `getConnectionInfo`, `getConfiguredNetworks`, `addNetwork` / `updateNetwork` / `enableNetwork` / `removeNetwork`, `disconnect` / `reconnect` / `reassociate`, `isWifiEnabled`, `getWifiState`, `calculateSignalLevel`, `compareSignalLevel`. **One saved network** (its `networkId` is 0); `setWifiEnabled` is accepted and ignored. Adds `getLastError()`, which has **no Android counterpart** (there are no broadcasts to carry the supplicant error). No `WifiNetworkSuggestion` / `WifiNetworkSpecifier`, no locks. |
| `wifi.ScanResult` / `wifi.WifiInfo` / `wifi.WifiConfiguration` / `wifi.SupplicantState` | Partial | `ScanResult`'s public fields (`SSID`, `BSSID`, `level`, `frequency`, `capabilities`, `timestamp` always 0). `WifiInfo`: `getSSID`, `getBSSID` (always `02:00:00:00:00:00`), `getRssi`, `getIpAddress`, `getNetworkId`, `getSupplicantState`. `WifiConfiguration`: `SSID`, `BSSID`, `preSharedKey`, `networkId`, `status`, `hiddenSSID`; no `allowedKeyManagement`. `SupplicantState` has `DISCONNECTED`, `ASSOCIATING`, `COMPLETED` only. |

### java.net and javax.net.ssl

The networking classes live in `picodroid.net`; the exceptions are the real `java.net` and
`javax.net.ssl` ones. See [Networking](/api/networking/).

| API | Status | Notes / alternative |
|---|---|---|
| `URL` (`picodroid.net`) | Partial | `http` and `https`; `getProtocol`, `getHost`, `getPort` (the scheme's default when omitted), `getPath` (**includes the query string**), `openConnection`. No `getQuery`, `getFile`, `openStream`, `URI`. |
| `HttpURLConnection` (`picodroid.net`) | Partial | `GET` / `POST` / `PUT`; `setConnectTimeout` / `setReadTimeout`, `setRequestProperty` / `addRequestProperty` / `getRequestProperty` (16 headers at most), `setDoOutput`, `setFixedLengthStreamingMode` (**required** for a body; no chunked upload), `getResponseCode`, `getResponseMessage`, `getHeaderField` / `getHeaderFieldKey`, `getContentLength`, `getInputStream` / `getErrorStream` / `getOutputStream`, `disconnect`, the `HTTP_*` constants. The streams are `HttpInputStream` / `HttpOutputStream`, not `java.io` streams. `Connection: close` always; no redirects followed for you, caching or cookies. Adds `AutoCloseable`. |
| `HttpsURLConnection` (`javax.net.ssl`, in `picodroid.net.ssl`) | Partial | TLS 1.3 against a compiled-in root store, on boards with `has_tls = true`; `getCipherSuite`. No `setSSLSocketFactory`, `setHostnameVerifier`, `getServerCertificates`. |
| `Socket` / `ServerSocket` (`picodroid.net`) | Renamed | **Not `java.net`'s method surface**: `connect(int addr, int port)` takes a packed IPv4 address, I/O is `send(byte[], int, int)` / `recv(byte[], int, int)` rather than streams, and the receive timeout is `setTimeout(int)`. `ServerSocket(int port)`, `accept`, `setSoTimeout`, `close`. |
| `DatagramSocket` / `DatagramPacket` (`picodroid.net`) | Partial | `send`, `receive`, `setSoTimeout`, `setBroadcast` / `getBroadcast` (recorded, not enforced), `close`. `DatagramPacket.getAddress()` returns an `InetAddress`. No `connect`, multicast or `getLocalPort`. |
| `InetAddress` (`picodroid.net`) | Partial | IPv4 only. `getByName`, `getByAddress(byte[])`, `getAddress()`, `getHostAddress`; also picodroid's `getByAddress(int a, int b, int c, int d)`, `new InetAddress(int)` and `getRawAddress()` for the packed form the sockets take. No `getAllByName`, `getHostName`, `isReachable`. |
| `java.net` / `javax.net.ssl` exceptions | Full | `ConnectException`, `BindException`, `NoRouteToHostException`, `SocketException`, `SocketTimeoutException`, `UnknownHostException`, `ProtocolException`, `SSLException`, `SSLHandshakeException`, `SSLPeerUnverifiedException`, with Java's hierarchy. |

### android.os

| API | Status | Notes / alternative |
|---|---|---|
| `SystemClock` | Partial | `elapsedRealtime`, `elapsedRealtimeNanos`, `sleep(int)`, `setCurrentTimeMillis` (always succeeds). No `uptimeMillis` or `currentThreadTimeMillis`. |
| `Handler` / `Looper` / `Message` | Unsupported | Use `Executors.mainExecutor().execute(Runnable)` for "post to UI"; for delayed or repeating work on the main thread, `Executors.mainScheduledExecutor()` (`schedule`, `scheduleAtFixedRate`). `Context.getMainExecutor()` and `Activity.runOnUiThread(Runnable)` exist. There is no `postDelayed`. |
| `Bundle` | Partial | Intent extras, saved instance state and fragment arguments. `put` / `get` for `boolean`, `int`, `long`, `float`, `double`, `String`, `int[]`, `byte[]`, `String[]` and nested `Bundle`, with the default-taking getters; `get`, `containsKey`, `remove`, `clear`, `size`, `isEmpty`, `keySet`, `putAll`, the copy constructor. No `Parcelable` / `Serializable`, no `short` / `char` / `byte` scalars or `ArrayList` values. |

### android.hardware

| API | Status | Notes / alternative |
|---|---|---|
| `Sensor` / `SensorManager` / `SensorEvent` / `SensorEventListener` | Partial | Board-dependent sensors; registration + event callbacks. |

### Concurrency

| API | Status | Notes / alternative |
|---|---|---|
| `Thread` (`picodroid.concurrent.Thread`) | Full | `start()` spawns a real FreeRTOS task on device and in the simulator (the sim runs the real kernel). `run()` override or `Runnable` target, `sleep`, `join`/`join(ms)`, `interrupt`/`isInterrupted`/`interrupted`, `isAlive`, `currentThread`, `get`/`setName`, `getId`, `yield`, `UncaughtExceptionHandler` (per-thread and default), `IllegalThreadStateException` on a second `start()`. `synchronized` blocks **and methods** are real kernel mutexes; `Object.wait`/`notify`/`notifyAll` work (timed, interruptible, `IllegalMonitorStateException` when not owner). Divergences: `setPriority` and `setDaemon` are advisory (every Java task runs at one RTOS priority — see the system reference), and a compute-bound thread holds the core until it blocks (no time slicing). |
| `Executor` / `Executors` (`mainExecutor` / `backgroundExecutor`) | Full | The recommended concurrency primitive — this is how you "post to the UI thread". |
| `ExecutorService` / `Future` / `Callable` / `TimeUnit` (`picodroid.concurrent`) | Partial | `Executors.newFixedThreadPool(n)` and `newSingleThreadExecutor()`: `execute`, `submit(Runnable/Callable)`, `Future.get`/`get(timeout, unit)`/`cancel`/`isDone`/`isCancelled`, `shutdown`/`shutdownNow`/`isShutdown`/`isTerminated`/`awaitTermination`; `ExecutionException` (with `getCause()`), `CancellationException`, `TimeoutException`, `RejectedExecutionException` under their `java.util.concurrent` names. Pure Java over `Thread` + `wait`/`notify`; each worker costs a 16 KiB task stack. Not built into `testbench_rp2040` (`framework_class_excludes`, flash headroom) — nor are the atomics and `CountDownLatch` below. No `invokeAll`/`invokeAny`, no cached pools, no `CompletableFuture`; scheduling is `Executors.newSingleThreadScheduledExecutor()` (its own thread, as in the JDK) or `Executors.mainScheduledExecutor()` (the main thread; picodroid's own). |
| `AtomicInteger` / `AtomicLong` / `AtomicBoolean` / `AtomicReference` (`picodroid.concurrent`) | Partial | `get`/`set`/`getAndSet`/`compareAndSet`/`incrementAndGet`/`getAndIncrement`/`decrementAndGet`/`addAndGet`/`getAndAdd`. `synchronized` underneath (one core, one JVM priority). No `updateAndGet`/lambdas, no `AtomicIntegerArray`. |
| `CountDownLatch` (`picodroid.concurrent`) | Full | `countDown`, `await`, `await(timeout, unit)`, `getCount`. No `Semaphore`, `CyclicBarrier`, `ReentrantLock`, `ConcurrentHashMap`, `BlockingQueue`. |

### org.json

| API | Status | Notes / alternative |
|---|---|---|
| `JSONObject` / `JSONArray` / `JSONException` (`picodroid.json`) | Partial | Android's `org.json` method surface for the two classes: constructors from text, `Map`, `Collection` and arrays; `get`/`opt` with Android's coercions; `put`/`putOpt`/`accumulate`/`append`/`remove`; `keys`/`names`/`keySet`; `toString`/`toString(indent)`; `quote`/`numberToString`/`wrap`; `NULL`. Documents live in a native node pool (2048 nodes, 16 KiB of strings, 32 levels) reclaimed with the GC; child wrappers share nodes but are not `==`. Strict RFC 8259 parser (no lenient `JSONTokener` extras), no `JSONTokener`/`JSONStringer`, `keySet()` unordered. Only on boards with `has_json = true` in `board.toml` — every RP2350 board; not `testbench_rp2040`, where the API contract rejects the classes at build time. See [JSON](/api/json/). |

### com.google.protobuf

| API | Status | Notes / alternative |
|---|---|---|
| `CodedInputStream` / `CodedOutputStream` / `MessageLite` / `WireFormat` / `InvalidProtocolBufferException` (`picodroid.protobuf`) | Partial | protobuf-javalite's stream surface: `readTag`, every `readXxx`, `skipField`, `pushLimit`/`popLimit`, `readString`/`readBytes` (a `byte[]`, no `ByteString`); every `writeXxx`/`writeXxxNoTag` and `computeXxxSize`. Varints, fixed-width values and skipping are native (`micropb`); strings, byte arrays, zigzag and limits are Java. Message classes come from `protoc-gen-picodroid`: mutable, setters instead of Builders, enums as `int` constants, arrays instead of `List`s, proto3 only, no `map`/`oneof`/groups/extensions/reflection. Only on boards with `has_protobuf = true` in `board.toml` — every RP2350 board; not `testbench_rp2040`, where the API contract rejects the classes at build time. See [Protocol Buffers](/api/protobuf/). |

### android.media

| API | Status | Notes / alternative |
|---|---|---|
| `ToneGenerator` (`picodroid.media`) | Partial | Android's constant names, values and CEPT cadences for the DTMF, `TONE_SUP_*` and `TONE_PROP_*` families (0-28, plus `TONE_SUP_CONFIRM` and `TONE_SUP_PIP`); `startTone(int)`, `startTone(int, int)`, `stopTone`, `release`. The output is one piezo on a PWM pad, so tones are monophonic square waves: each plays the lowest component of Android's multi-frequency tone, and DTMF therefore will not decode. No CDMA tone range, no intercept tones, no `getAudioSessionId`. Adds `startToneSequence(int[], int[])`, which has **no Android counterpart**. Segments advance on the UI frame tick. Needs an `[audio]` section in `board.toml`; without one the class is still present and every method is a safe no-op. See [Audio](/api/media/). |
| `AudioManager` (`picodroid.media`) | Partial | The `STREAM_*` constants only, with Android's values, so a `ToneGenerator` reads as it does on Android. No mixer, no volume control, no `playSoundEffect`, no focus API. |
| `MediaPlayer` / `AudioTrack` / `SoundPool` / `MediaRecorder` | None | No hardware path: a piezo buzzer has no DAC, no I2S and no amplifier behind it. There is no alternative for sampled audio; use `ToneGenerator` for tones. |

### java.* standard library

Enforced at build time: every app's `verifyApiContract` Gradle task (part of
`assemblePapk`, so `build-apk.sh`, `sim.sh`, `build.sh` and `flash.sh` all run
it) rejects a `java.*` class or member pico-jvm does not serve, naming the
call site and an alternative. `sdk/api-contract.tsv`, generated from the
runtime's own tables (`scripts/gen-api-contract.sh`), is the machine-readable
form of this section. With `--board <name>` (or `-Ppicodroid.board=<name>`)
the same task also rejects classes that board drops from its framework
(`framework_class_excludes`). `-Ppicodroid.apiContract=warn` downgrades the
failure to a report while experimenting.

| API | Status | Notes / alternative |
|---|---|---|
| `Object.clone()` / `Cloneable` | Partial | Shallow copy works, but **the `Cloneable` check is skipped** — `clone()` never throws `CloneNotSupportedException`. |
| Interface `default` methods | Full | Resolved per JVMS §5.4.3.3 (sub-interface overrides win whatever the `implements` order; `I.super.f()` works; defaults are found through abstract and builtin superclasses). A default method called *on a lambda object* runs the interface's body, which reaches the lambda through its single abstract method (until 2026-09-13 it ran the lambda body instead). |
| `StringBuilder.append(Object)` / `String.valueOf(Object)` / `"" + obj` | Full | The argument's `toString()` runs first — a Java override, else the builtin one for boxes/enums, else the identity form `pkg.Cls@hhhh`; `null` prints `null`. |
| `Object.equals`/`hashCode`/`toString` defaults | Partial | Identity semantics as in Java, but **`hashCode()` is the object's heap slot index** (stable for the object's lifetime, reused after GC). `HashMap`/`HashSet`/`ArrayList.contains`/`remove(Object)` compare keys with your `equals(Object)` override when the class has one (identity otherwise; `hashCode()` is never consulted — the buffers are linear), and boxes by class and value (`Integer(1)` is not `Short(1)`; `Double` keys follow `Double.equals`). The builtin collections' own `hashCode()`/`toString()` are the identity forms (`list.toString()` prints `java.util.ArrayList@…`, not the elements). |
| `instanceof` / `checkcast` | Partial | Strings, arrays, builtin collections under `List`/`Collection`/`Iterable`/`Set`/`Map`, boxes under `Number`/`Comparable`, lambdas under their interface, and transitive superinterfaces all work; a failed cast throws a catchable `ClassCastException` (with a `null` message). **A reference array's element class is not recorded**, so `(String[]) someObjectArray` succeeds where Java would throw. |
| Boxed wrappers (`Integer`, `Float`, …) | Partial | `equals`/`hashCode`/`compareTo`, the `compare`/`hashCode(x)` statics and `Float.floatToIntBits` follow Java 8. The `xxxValue()` accessors convert as Java does (`Float.valueOf(2.5f).intValue()` is 2, `Integer.valueOf(300).byteValue()` is 44), and `valueOf` shares one box per value across the JLS §5.1.7 range, so `Integer a = 127, b = 127; a == b` holds as on Android (`new Integer(5)` is always distinct). `Character` has `isDigit`/`isLetter`/`toUpperCase`/`toLowerCase` only, **ASCII-only** (strings are byte-backed). |
| `Enum.valueOf(Class, String)` | Full | Served by the interpreter, so every enum's own `valueOf(String)` works: the constants are the enum class's static fields of its own type, matched by name; an unknown name throws `IllegalArgumentException("No enum constant …")`, a null name `NullPointerException`. `Enum.hashCode()` is the ordinal. |
| `Throwable` | Partial | `addSuppressed`/`getSuppressed`/`getCause` stored; `ExceptionInInitializerError` wraps `<clinit>` throws. **A failed `<clinit>` does not poison the class** (no `NoClassDefFoundError` on re-access). |
| `Comparator` + `Collections.sort(List, Comparator)` | Full | Lambda comparators supported; `list.sort(null)` and `Collections.sort(list)` are natural ordering (`compareTo`). |
| `OutOfMemoryError` | Full | A failed allocation — `new`, an array, a growing collection or builder, a box — collects first and throws a catchable `OutOfMemoryError` only when that frees nothing; one error object is kept in reserve for the moment the heap cannot even hold the exception. `clear()` on a builtin collection releases its buffer, so it is the way to give memory back from a handler; the arena is small and fragments, so hold no more than a few hundred boxed entries per collection. |
| Interface-typed collections (`List`/`Set`/`Collection`/`Map`/`Iterable`) | Full | `Map<String,String> m = new HashMap<>();` and interface-typed fields, parameters, returns, casts, `instanceof` and enhanced-`for` all work — the JVM dispatches on the receiver's runtime class, so the interfaces need no class file. **Caveat:** your app compiles against the JDK's *full* interfaces, so members picodroid does not implement (`map.forEach`, `list.removeIf`, `Map.putAll`, `new TreeMap<>()`) compile and then fail at run time. The rows in this table are the served surface. |
| `HashMap.entrySet()` / `Map.Entry` | Partial | `entrySet()`, `keySet()` and `values()` views answer `iterator()` and `size()`; the key and value views also `contains()` (`k in map.keys`). Entries answer `getKey()`/`getValue()` (no `setValue`). Iteration order is the map's internal order. |
| `HashSet.iterator()` | Full | `for (x : set)` and every iterating idiom on a set; hash order, like the map views. Iterators and map views keep a **temporary** collection alive for as long as they do (`for (String w : text.split(" "))`, `for (e : makeMap().entrySet())`) — the GC pins the iterator's source. |
| `Map.putAll` / `List.listIterator` / `Arrays.equals` | Unsupported | No builtin arm: copy with an `entrySet()` loop, iterate by index, compare arrays element-wise. Kotlin: `map += otherMap`, `list.last { }` / `indexOfLast { }` / `findLast { }` (they walk a `listIterator`) and `contentEquals` hit these. |
| `Float.isNaN(f)` / `Double.isNaN(d)` / `isInfinite` statics | Unsupported | Use `f != f` for NaN and a magnitude compare for infinity (Kotlin's `isNaN()`/`isInfinite()`/`isFinite()` inline to these statics). |
| `String.join` | Partial | `join(delim, a, b, …)` / `join(delim, String[])` and `join(delim, ArrayList)`; elements must be `String` or `null` (a `StringBuilder` element throws). Any other `Iterable` is rejected. |
| `java.util.Objects` | Full | `equals`, `hashCode`, `hash`, `toString(o)`, `toString(o, default)`, `requireNonNull` (both forms), `isNull`, `nonNull`. A real class, so `equals`/`hashCode`/`toString` reach your overrides. |
| `String(char[])` | Unsupported | Only the `byte[]` constructors exist; build with `StringBuilder.append(char)` (Kotlin: `chars.concatToString()` does this). |
| `Math.round` | Partial | Rounds half **away from zero** (`Math.round(-2.5f)` is `-3`; Java gives `-2`). Kotlin's `roundToInt()`/`roundToLong()` shim spells out Java's `floor(x + 0.5)` and is exact. |
| `Math.floorDiv` / `floorMod` / `addExact` / `subtractExact` / `multiplyExact` / `toIntExact` | Full | The `int` and `long` forms, as bodied Java in the SDK; the `*Exact` ones throw `ArithmeticException` on overflow. |
| `java.time` — `LocalDate`, `LocalTime`, `LocalDateTime`, `Instant`, `Duration`, `ZoneOffset`, `ZoneId`, `Month`, `DayOfWeek`, `Year`, `DateTimeFormatter`, `ChronoUnit` | Partial | An SDK port of the JDK classes with their JDK descriptors, so `Instant.now().plus(Duration.ofMinutes(5))`, `LocalDate.of(…).plusMonths(1)`, `ChronoUnit.DAYS.between(a, b)`, `LocalDateTime.ofInstant(instant, zone)`, `date.format(DateTimeFormatter.ofPattern("EEE d MMM"))` and ISO `parse`/`toString` all behave as on Android. **No tz database:** every `ZoneId` is a fixed offset (`ZoneId.of("UTC+05:30")`, `ZoneOffset.ofHours(-8)`; a region id throws `DateTimeException`), `ZoneId.systemDefault()` is UTC until `TimeZone.setDefault(TimeZone.getTimeZone("GMT+05:30"))`, and there is no `ZonedDateTime`, `OffsetDateTime`, `Period`, `TemporalField`/`ChronoField`, `TemporalAdjusters` or locale-aware formatting (`DateTimeFormatter.ofPattern` covers `y u M L d D E a H h m s S` and `'…'` literals, with English month and day names; `parse` reads ISO-8601 only). `now()` reads `System.currentTimeMillis()`, which counts from boot until `SystemClock.setCurrentTimeMillis`. Not on `testbench_rp2040` (`framework_class_excludes`; ~67 KB of class files). See [java.time](/api/core/#javatime). |
| `java.util.TimeZone` | Partial | Fixed-offset zones only: `getTimeZone("GMT+05:30")`, `getTimeZone(ZoneId)`, `getDefault`/`setDefault` (`null` restores UTC), `getID`, `getRawOffset`, `getOffset(long)`, `toZoneId`, `useDaylightTime` (false). An unknown id is GMT, as in the JDK. |
| `String.replace(target, replacement)` | Full | An empty `target` interleaves the replacement between every char and at both ends, as in Java (`"ab".replace("", "-")` is `-a-b-`). |
| `LinkedHashMap` / `LinkedHashSet` | Partial | **Aliases of `HashMap` / `HashSet`: insertion order is not preserved.** `instanceof HashMap`/`Map`/`Set` hold. |
| `Collection.toArray()` / `toArray(T[])` | Partial | Always returns a **fresh** `Object[]` of exactly the collection's size — the array argument is neither filled nor returned (`list.toArray(new String[0])` is the idiom to use; a cast to `String[]` succeeds because reference arrays carry no element class). |
| `java.util.Locale` | Partial | Name-only: `Locale.ROOT`/`US`/… read as `null`, and `toUpperCase(Locale)`/`toLowerCase(Locale)` ignore the argument (ASCII only). This is what Kotlin's `uppercase()`/`lowercase()` compile to. No `Locale` instances. |
| `Class.getName()` | Full | Returns dot-form (`pkg.Class`) per the Java spec. |
| `javax.inject.Inject` / `Singleton` / `Scope` | Partial | Compile-time only: an annotation processor generates `Foo_Factory` / `Foo_MembersInjector`, and `Application` / `Activity` / `Service` are injected automatically before `onCreate`. `SOURCE` retention (JSR-330 says `RUNTIME`) — no runtime annotations, no reflection. `javax.inject.Provider<T>` / `picodroid.di.Lazy<T>` (≙ `dagger.Lazy`) inject anywhere a `T` can; `picodroid.di.Module` / `picodroid.di.Provides` (≙ `dagger.Module` / `dagger.Provides`) bind SDK types and interfaces, every module auto-installed. No `@Component`, `@Binds`, `@Named`/qualifiers, custom scopes. Java and Kotlin apps (the Kotlin plugin runs the same processor through kapt; Kotlin shapes: `@Inject lateinit var`, `@Inject constructor`, `@Module object` + `@JvmStatic @Provides`, `@Module class` for instance `@Provides`). See [Services & DI](/api/services/). |
| `String.split` | Partial | Literal delimiters only — **no regex**. |
| `BufferedReader` / `InputStreamReader` | Unsupported | Use the byte-oriented `picodroid.io` streams (`FileInputStream`/`FileOutputStream`); there is no char-stream reader layer. |

## Cross-cutting divergences

- **Coordinates and sizes are `int` px.** There is no `float` `MotionEvent`
  coordinate and one display density: `dp` and `sp` in a resource file are
  accepted and mean one logical pixel each, and `getResources().getDisplayMetrics()`
  reports `density` 1 at 160 dpi, with `xdpi`/`ydpi` the panel's real pitch.
  Every board is at least 240×240 logical pixels; lay out against that floor
  and let `match_parent`, weights and margins take up the rest
  (`docs/designs/app-portability-2026-10.md`).
- **Resources have no configurations.** `res/values`, `res/layout` and
  `res/drawable` compile into the PAPK with a generated `R` class (see
  [resources](/guides/resources/)), but there is one display, density and
  locale: qualified directories (`values-night/`, `drawable-hdpi/`) are a
  build error, and plurals and string arrays do not exist. Styles and the
  theme are resolved at build time: there is one theme per app (the style
  named `AppTheme`), and `Context.setTheme` hands its colours to the widgets.
- **No `Handler`/`Looper`.** The main loop is an executor-driven dispatcher;
  use `Executors.mainExecutor()`, and the scheduled executor for delayed work.
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
