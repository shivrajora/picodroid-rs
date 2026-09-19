---
title: "Resources, R and XML layouts"
description: "Put strings, colours, dimensions, layouts and images under res/, reference them through the generated R class, and inflate XML layouts with setContentView(R.layout.main)."
---

An app may carry an Android-style `res/` directory. Gradle compiles it into a binary table inside
the PAPK and generates an `R` class next to your sources, so this works as it does on Android:

```java
public class MainActivity extends Activity {
  @Override
  public void onCreate() {
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
complete app that checks every call on this page.

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
```

`res/` is opt-in: an app without one builds exactly as before, and its PAPK is byte-identical.
`R` is generated into the manifest's `package`, the same package as the app's own classes, so no
import is needed.

There are **no resource configurations**. A picodroid device has one display, one density and one
locale, so `values-night/`, `drawable-hdpi/`, `layout-land/` and the like are a build error rather
than something silently ignored. File-based names (`layout/`, `drawable/`) must be `[a-z0-9_]`, as
on Android.

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
| `<string>` | `getResources().getString(id)`, `Context.getString(id)`, `getText(id)` | Android's whitespace and escape rules: `\n`, `\t`, `\'`, `\"`, `\\`, `\uXXXX`, and `"double quotes"` to keep spacing. Plain text only — no `<b>`, no plurals, no string arrays, no format-argument overload. |
| `<color>` | `getResources().getColor(id)`, `Context.getColor(id)` | `#RGB`, `#ARGB`, `#RRGGBB`, `#AARRGGBB`. |
| `<dimen>` | `getDimension(id)` (float px), `getDimensionPixelSize(id)`, `getDimensionPixelOffset(id)` | `px`, `dp` and `sp` are accepted and are all **one pixel**: there is one density. `pt`, `mm`, `in` are refused. |
| `<integer>` | `getInteger(id)` | Decimal or `0x…`. |
| `<bool>` | `getBoolean(id)` | |

A value may be a reference to another of the same type (`@color/accent`); chains are followed and
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

**Elements:** `LinearLayout`, `FrameLayout`, `ScrollView`, `RadioGroup`, `TextView`, `Button`,
`ImageView`, `EditText`, `CheckBox`, `Switch`, `ToggleButton`, `RadioButton`, `ProgressBar`,
`SeekBar`, `Spinner`, `ListView`. A custom view class cannot be inflated — there is no reflection
to construct it with — and `<include>` / `<merge>` are not supported yet; both are build errors.
Create those views in Java and `addView` them into an inflated container.

**Attributes** (the `android:` prefix is what Android Studio writes; any prefix is accepted, and
`tools:` attributes are dropped):

| Group | Attributes |
|---|---|
| Any view | `id`, `layout_width`, `layout_height` (`match_parent`, `wrap_content`, a dimension), `layout_weight`, `layout_gravity`, `padding`, `paddingLeft/Top/Right/Bottom`, `paddingStart/End`, `paddingHorizontal/Vertical`, `background` (a colour), `visibility`, `enabled`, `focusable`, `alpha` |
| `LinearLayout`, `RadioGroup` | `orientation`, `gravity` |
| `TextView`, `Button` | `text`, `textColor`, `singleLine`, `maxLines`, `ellipsize` |
| `EditText` | `text`, `hint`, `inputType` (`text`, `number`, `phone`, `datetime`, `textUri`, `textEmailAddress`, `textPassword`, `numberSigned`, `numberDecimal`) |
| `CheckBox`, `RadioButton` | `text`, `checked` |
| `Switch`, `ToggleButton` | `checked`; `textOn`, `textOff` on `ToggleButton` |
| `ImageView` | `src` (`@drawable/…`), `scaleType` (`fitCenter`, `centerCrop`, `fitXY`, `center`), `tint` |
| `ProgressBar`, `SeekBar` | `progress`; `max` on `SeekBar`; `tint` on `ProgressBar` |

An attribute the framework has no setter for — `textSize` (there is [one font
size](/guides/embedded-gotchas/)), `layout_margin`, `onClick` — is **reported as a build warning
and dropped**, so a layout pasted from an Android project builds and tells you what it lost. A
value that does not parse, an undefined reference, or an unknown element is a build error that names
the file and the attribute.

## Drawables

A PNG under `res/drawable/` becomes `R.drawable.<name>` and is shown with
`ImageView.setImageResource(R.drawable.logo)` or `android:src="@drawable/logo"`. It is decoded to
RGB565 at build time and stored in the PAPK's ASSETS section exactly like a file under
[`assets/`](/guides/assets/), so the same rules apply: PNG only, alpha is discarded, and each image
costs `width × height × 2` bytes of flash. XML drawables (shapes, selectors, vectors) do not exist;
use `GradientDrawable` from Java.

## What is not there

Styles and themes (`style=`, `?attr/…`), `AttributeSet` and custom view constructors, string
arrays and plurals, `getString(int, Object...)`, `getIdentifier`, `<include>` / `<merge>` /
`ViewStub`, menus, animations and any qualified resource directory.

## Inspecting a package

`./scripts/papk-info.sh build/apks/resdemo.papk` prints the RESOURCES section — how many entries
each type has and the id range they occupy — next to the classes and assets.
