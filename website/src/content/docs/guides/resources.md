---
title: "Resources, R and XML layouts"
description: "Put strings, colours, dimensions, styles, layouts, shapes and images under res/, reference them through the generated R class, and inflate XML layouts with setContentView(R.layout.main)."
---

An app may carry an Android-style `res/` directory. Gradle compiles it into a binary table inside
the PAPK and generates an `R` class next to your sources, so this works as it does on Android:

```java
public class MainActivity extends Activity {
  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    setContentView(R.layout.activity_main);

    TextView title = findViewById(R.id.title);
    title.setText(getString(R.string.greeting));
    title.setTextColor(getResources().getColor(R.color.accent));
  }
}
```

No XML parser runs on the device. Layouts are compiled to a stream of integers, every
`@color/…`, `@dimen/…` and `12dp` is resolved to a number at build time, and the table is read in
place out of flash. `R`'s fields are compile-time constants that `javac` inlines, so the `R` classes
themselves are left out of the PAPK: resources cost an app its table and nothing else.
[`examples/resdemo`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/resdemo) is a
complete app that checks the values, the inflater and `findViewById`, and
[`examples/layoutdemo`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/layoutdemo)
one that checks margins, gravity, shapes, includes, styles, the theme, custom views and
`AsyncLayoutInflater`.

## Directory layout

```
examples/resdemo/
  PicodroidManifest.xml
  build.gradle.kts
  java/resdemo/ResDemoActivity.java
  res/
    values/strings.xml        any number of *.xml files, any names
    values/values.xml
    layout/activity_main.xml  R.layout.activity_main
    layout/row.xml
    drawable/logo.png         R.drawable.logo
    drawable/card.xml         a <shape>, for android:background="@drawable/card"
```

`res/` is opt-in: an app without one builds exactly as before, and its PAPK is byte-identical.
`R` is generated into the manifest's `package`, the same package as the app's own classes, so no
import is needed.

There is one density and one locale, so `values-night/`, `drawable-hdpi/` and the like are a build
error rather than something silently ignored. What may vary is the window and the input:
[configuration variants](#configuration-variants) of `values/` and `layout/`. File-based names
(`layout/`, `drawable/`) must be `[a-z0-9_]`, as on Android.

## Values

```xml
<resources>
    <string name="app_name">Res Demo</string>
    <color name="accent">#FFB300</color>
    <color name="title">@color/accent</color>
    <dimen name="gap">8dp</dimen>
    <integer name="max_taps">10</integer>
    <bool name="show_logo">true</bool>
</resources>
```

| Element | Java | Notes |
|---|---|---|
| `<string>` | `getString(id)` and `getString(id, Object... formatArgs)` on `Resources`, `Context` and `Fragment`; `getText(id)` | Android's whitespace and escape rules: `\n`, `\t`, `\'`, `\"`, `\\`, `\uXXXX`, and `"double quotes"` to keep spacing. The format-argument overload is `String.format` over the string. Plain text only — no `<b>`, no plurals, no string arrays. |
| `<color>` | `getResources().getColor(id)`, `Context.getColor(id)` | `#RGB`, `#ARGB`, `#RRGGBB`, `#AARRGGBB`. |
| `<dimen>` | `getDimension(id)` (float px), `getDimensionPixelSize(id)`, `getDimensionPixelOffset(id)` | `px`, `dp` and `sp` are accepted and are all **one pixel**: there is one density. `pt`, `mm`, `in` are refused. |
| `<integer>` | `getInteger(id)` | Decimal or `0x…`. |
| `<bool>` | `getBoolean(id)` | |

A value may be a reference to another of the same type (`@color/accent`) or to the theme
(`?attr/colorPrimary`, [below](#styles-and-the-theme)); chains are followed and
cycles are a build error. An id of the wrong type, or one that does not exist, throws
`Resources.NotFoundException`, as on Android.

## Layouts

```xml
<LinearLayout xmlns:android="http://schemas.android.com/apk/res/android"
    android:layout_width="match_parent"
    android:layout_height="match_parent"
    android:orientation="vertical"
    android:padding="@dimen/gap">

    <TextView
        android:id="@+id/title"
        android:layout_width="wrap_content"
        android:layout_height="wrap_content"
        android:text="@string/app_name"
        android:textColor="@color/title" />

    <Button
        android:id="@+id/tap"
        android:layout_width="0dp"
        android:layout_height="wrap_content"
        android:layout_weight="1"
        android:text="Tap" />
</LinearLayout>
```

- `Activity.setContentView(int)`, `Activity.getLayoutInflater()`, `LayoutInflater.from(context)`
- `inflate(int, ViewGroup)` and `inflate(int, ViewGroup, boolean attachToRoot)` with Android's
  semantics: given a `root`, the layout's top-level `layout_*` attributes become the
  `LayoutParams` that root wants (`LinearLayout.LayoutParams`, `FrameLayout.LayoutParams`); with
  `attachToRoot` the tree is added and `root` is returned.
- `<T extends View> T findViewById(int)` on `Activity` and on any `View`, depth first.
  `@+id/name` declares `R.id.name`.

**Elements:** `LinearLayout`, `FrameLayout`, `ScrollView`, `RadioGroup`, `View`, `Space`,
`TextView`, `Button`, `ImageView`, `EditText`, `CheckBox`, `Switch`, `ToggleButton`,
`RadioButton`, `ProgressBar`, `CircularProgressIndicator`, `SeekBar`, `Spinner`, `ListView`,
`ViewPager2`. Each may also be written fully qualified (`<picodroid.widget.ViewPager2>`), the way
Android requires for a view outside `android.widget`. `<include layout="@layout/row"/>` puts
another layout in place of the element; `<merge>` is not supported and is a build error. Any
other dotted name is [a view class of your own](#custom-views).

**Writing a layout once for every board.** Every board is at least 240×240 logical pixels, and a
`dp` is one of them; lay out against that floor and let the rest stretch: `match_parent` and
`layout_weight` take the room a wider panel adds, a weighted `<Space>` pushes neighbours to the
edges, `android:minWidth` / `android:minHeight` floor a `wrap_content` view, `android:maxWidth`
caps a label so a long line wraps or ellipsizes instead of pushing the row off the panel, and
`android:visibility="invisible"` keeps a view's room while it is hidden (`gone` gives it up).
`android:keepScreenOn="true"` on a root holds the display on while that screen shows.
`android:layout_gravity` on a `LinearLayout` child is not applied (the compiler warns): wrap that
child in a `FrameLayout`, or set the parent's `android:gravity`. A root that is still larger than
the panel is not clipped: the screen pans to it (a drag on a touch board, the focus on a four-key
one), which keeps the app usable and is the sign to fix the layout. The simulator and a debug build
say which it was after each `setContentView` — `[layout] fit ok 320x240 in 320x240`, or
`[layout] overflow 320x480 in 320x240: the screen pans 0 right, 240 down` — and the nightly runs
each flagship app on a 240×240, a 320×240 and a 320×480 board expecting `fit ok`
(`./scripts/sim.sh --app yourapp --board pico_enviro_mon` tries the smallest). The design is
[app portability](https://github.com/shivrajora/picodroid-rs/blob/main/docs/designs/app-portability-2026-10.md).

### Margins and `FrameLayout` placement

`layout_margin`, `layout_marginLeft/Top/Right/Bottom`, `layout_marginStart/End` and
`layout_marginHorizontal/Vertical` become the child's `ViewGroup.MarginLayoutParams`, which both
`LinearLayout.LayoutParams` and `FrameLayout.LayoutParams` extend:

- In a `LinearLayout` a margin is space kept clear around the child, so
  `android:layout_marginLeft="6dp"` on the second of two children is the gap between them.
- In a `FrameLayout` a child is placed by its `layout_gravity` and its margins, as on Android:
  against the corner, edge or centre the gravity names (`top|left` when it names none), moved in by
  the margins on that side. `layout_marginLeft` and `layout_marginTop` alone are therefore the
  child's x and y, which is how a pixel-exact panel is written in XML:

```xml
<FrameLayout
    android:layout_width="304dp"
    android:layout_height="92dp"
    android:background="@drawable/card">

    <TextView
        android:layout_width="wrap_content"
        android:layout_height="wrap_content"
        android:layout_marginLeft="12dp"
        android:layout_marginTop="7dp"
        android:text="@string/caption" />

    <TextView
        android:id="@+id/note"
        android:layout_width="wrap_content"
        android:layout_height="wrap_content"
        android:layout_gravity="right|bottom"
        android:layout_marginRight="12dp"
        android:layout_marginBottom="7dp" />
</FrameLayout>
```

The same params work from code (`new FrameLayout.LayoutParams(w, h, Gravity.CENTER)`,
`lp.setMargins(l, t, r, b)`, `parent.addView(child, lp)`). They are read when the child is added.
`ViewGroup.addView(child)` applies the `LayoutParams` the child already carries, so an inflated
view needs no second argument. One divergence: a background with a stroke insets its children by
the stroke's width (the renderer's border is inside the box), where Android draws the stroke under
them.

### `<include>`

`<include layout="@layout/meter_row" android:id="@+id/cap_0" android:layout_marginTop="27dp"/>`
compiles to the named layout's tree at that place. The include's `android:id`,
`android:visibility` and `layout_*` attributes replace the included root's; any other attribute on
an `<include>` is a build error. Find a view inside one instance with
`findViewById(R.id.cap_0).findViewById(R.id.meter_name)`. The inclusion happens at build time, so
it costs nothing on the device beyond the views themselves.

### Custom views

A view class of your own is named by its fully qualified class name, as on Android:

```xml
<com.example.ui.GaugeView
    android:id="@+id/gauge"
    android:layout_width="wrap_content"
    android:layout_height="wrap_content"
    android:layout_marginLeft="40dp" />
```

There is no reflection to construct it by, so the inflater asks a `LayoutInflater.Factory`, and
every `Activity` is one. Override `Activity.onCreateView(String name, Context context,
AttributeSet attrs)` (the same method Android calls) and construct your views there:

```java
@Override
public View onCreateView(String name, Context context, AttributeSet attrs) {
  if (name.equals("com.example.ui.GaugeView")) {
    return new GaugeView(context, attrs);
  }
  return super.onCreateView(name, context, attrs);
}
```

The framework then applies the element's common attributes (`id`, `layout_*`, padding,
`background`, `visibility`, `alpha`) to what you return. `attrs` is always empty: a layout is
numbers by the time it reaches the device, and an attribute of the view's own (`app:…`) is
reported and dropped at build time. A custom element cannot have children. Inflating one with no
factory answering for it throws `InflateException`. `LayoutInflater.setFactory` installs a factory
other than the Activity.

A view that draws itself (`extends View`, `onDraw(Canvas)`) says how large it wants to be in
`onMeasure`, which the framework calls when the view is added with a `wrap_content` dimension:

```java
@Override
protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
  setMeasuredDimension(
      resolveSize(bars * BAR_PITCH, widthMeasureSpec), resolveSize(HEIGHT, heightMeasureSpec));
}
```

`View.MeasureSpec`, `measure`, `setMeasuredDimension`, `getMeasuredWidth/Height`, `resolveSize` and
`getDefaultSize` are Android's. A `wrap_content` dimension is measured `UNSPECIFIED` and a fixed
one `EXACTLY`; a view with two fixed (or `match_parent`) dimensions is not asked at all, and the
framework's own widgets are sized by the renderer, which knows their content. Drawing views need a
board with `Canvas`, which is every RP2350 board.

### Inflating without stalling the UI

On a board where every view costs milliseconds, a screen of thirty views inflated in one go is one
long main-thread tick. `picodroid.view.AsyncLayoutInflater` (androidx's class) spreads it out:

```java
new AsyncLayoutInflater(context)
    .inflate(R.layout.page_limits, container, (view, resid, parent) -> {
      parent.addView(view);
      // findViewById, first paint ...
    });
```

Android inflates on a worker thread; views are made on the main thread here, so the work is cut
into slices of a few milliseconds, one per tick of the main loop, and the callback runs on a tick
of its own once the tree is whole. As on Android the view is not added to `parent`, which only
supplies the root's `LayoutParams`. An `OutOfMemoryError` part-way drops what was built and starts
again on a later tick. A callback that finds its screen gone (a fragment whose view was destroyed
meanwhile) should call `view.close()`, because picodroid frees a view's widget when it is removed
and this one was never added.
[`examples/claudeusage`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/claudeusage)
builds each of its pages this way.

A Fragment needs no element of its own: give a `FrameLayout` an id and hand it to the
transaction, `getSupportFragmentManager().beginTransaction().replace(R.id.container, fragment)`,
as [`examples/fragmentdemo`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/fragmentdemo)
does; there is no `<fragment>` or `FragmentContainerView` element. `testbench_rp2040` leaves
`ViewPager2` and Fragments out of its framework, so a layout with a `<ViewPager2>` is for the
RP2350 boards.

**Attributes** (the `android:` prefix is what Android Studio writes; any prefix is accepted, and
`tools:` attributes are dropped):

| Group | Attributes |
|---|---|
| Any view | `id`, `style`, `layout_width`, `layout_height` (`match_parent`, `wrap_content`, a dimension), `layout_weight`, `layout_gravity` (applied in a `FrameLayout`), `layout_margin` and its per-side forms, `padding`, `paddingLeft/Top/Right/Bottom`, `paddingStart/End`, `paddingHorizontal/Vertical`, `background` (a colour, `#AARRGGBB` with the alpha honoured; `@android:color/transparent`, `black`, `white`; or a `@drawable/` [shape](#drawables)), `visibility`, `enabled`, `focusable`, `alpha` |
| `LinearLayout`, `RadioGroup` | `orientation`, `gravity` (where the children go) |
| `TextView`, `Button` | `text`, `textColor`, `textSize` (a dimension, `sp` = `px`; snaps to the board's nearest compiled face), `gravity` (where the text sits in a view wider than it: `left`/`start`, `center_horizontal`/`center`, `right`/`end`; the vertical half is recorded, not drawn), `singleLine`, `maxLines`, `ellipsize`, `includeFontPadding` |
| `EditText` | `text`, `hint`, `inputType` (`text`, `number`, `phone`, `datetime`, `textUri`, `textEmailAddress`, `textPassword`, `numberSigned`, `numberDecimal`) |
| `CheckBox`, `RadioButton` | `text`, `checked` |
| `Switch`, `ToggleButton` | `checked`; `textOn`, `textOff` on `ToggleButton` |
| `ImageView` | `src` (`@drawable/…`), `scaleType` (`fitCenter`, `centerCrop`, `fitXY`, `center`), `tint` |
| `ProgressBar`, `SeekBar` | `progress`, `max`; `min`, `progressTint`, `progressBackgroundTint`, `indeterminateTint` on `ProgressBar` (`tint` is the older spelling of `indeterminateTint`). `min` and `max` are applied before `progress` whatever the XML order, as on Android. |
| `CircularProgressIndicator` | `ProgressBar`'s `progress`, `min`, `max`, `progressTint` (the indicator) and `progressBackgroundTint` (the track), plus Material's `indicatorColor`, `trackColor`, `trackThickness`, `indicatorSize` (Android Studio writes them with the `app:` prefix) and picodroid's `startAngle`, `sweepAngle` (degrees) |

An attribute the framework has no setter for — `textStyle`, `fontFamily`, `elevation`,
`onClick` — is **reported as a build warning
and dropped**, so a layout pasted from an Android project builds and tells you what it lost. A
value that does not parse, an undefined reference, or an unknown element is a build error that names
the file and the attribute.

## Drawables

A PNG under `res/drawable/` becomes `R.drawable.<name>` and is shown with
`ImageView.setImageResource(R.drawable.logo)` or `android:src="@drawable/logo"`. It is decoded to
RGB565 at build time and stored in the PAPK's ASSETS section exactly like a file under
[`assets/`](/guides/assets/), so the same rules apply: PNG only, alpha is discarded, and each image
costs `width × height × 2` bytes of flash.

An XML file under `res/drawable/` is a `<shape>`: a filled rectangle with optional round corners
and a stroke, for `android:background="@drawable/card"`.

```xml
<shape xmlns:android="http://schemas.android.com/apk/res/android" android:shape="rectangle">
    <solid android:color="?attr/colorSurface" />
    <corners android:radius="12dp" />
    <stroke android:width="1dp" android:color="@color/outline" />
</shape>
```

A shape is flattened at build time into the layouts that use it: the inflater gives the view a
`GradientDrawable` with those numbers, so `getBackground()` returns one. It has no `R.drawable`
entry and costs no flash of its own. Only `rectangle` (the default), `<solid>`, `<corners
android:radius>` and `<stroke>` are supported; selectors, vectors, gradients and per-corner radii
are build errors. To change such a background's colour at run time, tint it:
`view.setBackgroundTintList(ColorStateList.valueOf(color))` recolours it and keeps the shape.

## Styles and the theme

```xml
<resources>
    <style name="AppTheme">
        <item name="colorPrimary">@color/clay</item>
        <item name="android:colorBackground">@color/background</item>
        <item name="android:textColorPrimary">@color/text</item>
    </style>

    <style name="Line">
        <item name="android:layout_width">wrap_content</item>
        <item name="android:layout_height">wrap_content</item>
        <item name="android:singleLine">true</item>
        <item name="android:textColor">?android:attr/textColorPrimary</item>
    </style>

    <!-- Inherits Line by its name; parent="..." names a parent explicitly. -->
    <style name="Line.Caption">
        <item name="android:textColor">?android:attr/textColorSecondary</item>
    </style>
</resources>
```

- `style="@style/Line.Caption"` on a layout element gives it the style's items, parents first; an
  attribute written on the element itself wins. The expansion happens at build time, so a style
  costs nothing on the device.
- `?attr/name` (also `?android:attr/name` and `?name`) in a layout, a shape or a value is the item
  `name` of the app's theme, which is the style called **`AppTheme`**. It too is resolved at build
  time. A theme item may be anything a reference can be, including one of your own
  (`<item name="captionSize">20sp</item>`).
- `Context.setTheme(R.style.AppTheme)`, called in `onCreate` before `setContentView`, hands the
  theme's colours to the framework's own widgets, which is what the device keeps of a style:
  `colorPrimary`, `colorOnPrimary`, `android:colorBackground`, `colorSurface`,
  `android:textColorPrimary`, `android:textColorSecondary` and `colorOutline` become
  `picodroid.graphics.Theme`'s defaults. Every `<style>` has an `R.style` id (dots as underscores).

Divergences: one theme per app rather than one per context; the theme `?attr/` reads is chosen by
name, not by the manifest; no `TypedArray`, `obtainStyledAttributes` or `Theme.resolveAttribute`;
a parent that is not one of the app's own styles (a framework theme) contributes nothing.

## Configuration variants

A board's window is 240×240, 320×240 or 320×480 and it has a touch panel or four keys; a layout that
needs more than `match_parent` and weights to look right on all of them says so with Android's
qualified directories, in the subset that can differ here:

| Qualifier | Matches when | Example |
|---|---|---|
| `sw<N>dp` | the window's shorter side is at least N | `values-sw320dp/` |
| `w<N>dp` | the window is at least N wide | `layout-w320dp/` |
| `h<N>dp` | the window is at least N tall | `values-h480dp/` |
| `land` / `port` | the window is wider than tall / at least as tall as wide | `layout-land/` |
| `notouch` / `finger` | the board has no touch panel / has one | `values-notouch/` |

They combine in that order, each at most once (`layout-w320dp-land/`), as `aapt` requires. A
variant directory **overrides**: every name in `values-land/values.xml` must exist in
`res/values/`, and every `layout-finger/row.xml` must have a `res/layout/row.xml` — `R` is the
base's, so the app compiles against one set of ids and every id resolves on every board. Where
several variants match, Android's precedence picks: a `sw` bound beats a `w` bound beats an `h`
bound beats orientation beats touch, and a larger bound beats a smaller one. The choice is made
once, when the app's resources open, against the same window
`Resources.getConfiguration()` reports (an app with a `<supports-screens>` design size sees its
design size); nothing changes while the app runs. A variant value's `@` references resolve against
the base with the variant laid over it; a variant layout's resolve against the base values.

Drawables and styles do not vary (`drawable-land/`, or a `<style>` in a variant, is a build error),
and neither do density, locale, night or screen-size buckets (`values-night/`, `layout-xlarge/`).
`papk-info` lists the variants a package carries; the simulator logs which applied:
`[res] 320x240dp land notouch: 2 of 3 variants apply`. `examples/resdemo` carries one of each kind
and checks them on four boards.

## What is not there

String arrays and plurals, `getIdentifier`, `<merge>` / `ViewStub`, custom view attributes
(`app:…`, `declare-styleable`), selector / vector / layer drawables, menus, animations, and the
resource configurations outside the subset above (density, locale, night, screen-size buckets).

## Inspecting a package

`./scripts/papk-info.sh build/apks/resdemo.papk` prints the RESOURCES section — how many entries
each type has and the id range they occupy — next to the classes and assets.
