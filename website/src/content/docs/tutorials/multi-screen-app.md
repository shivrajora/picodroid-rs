---
title: "Tutorial: a multi-screen app with a back stack"
description: "Build a Home hub that pushes Counter and About screens, learning Activities, the back stack, lifecycle, and view preservation."
---

This tutorial builds a small three-screen app: a **Home** hub with two buttons that push a
**Counter** screen and an **About** screen onto the back stack. Pressing BACK (or a Back button)
pops the top screen and returns to Home.

Along the way you'll learn how Picodroid models screens as `Activity` objects, how `startActivity`
and `finish()` drive the back stack, the order lifecycle callbacks fire, why a paused Activity's
widget tree is preserved across a push, and what BACK does at the root.

The finished code is the committed
[`examples/tutorial_screens/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/tutorial_screens)
app — every snippet below is copied from it, so it builds and runs as-is.

## Scaffold

The finished app is already in the checkout, so to read along there is nothing to create. To type it
in yourself, generate a skeleton under a name of your own with the `newApp` Gradle task (it refuses a
name whose directory exists):

```bash
./gradlew newApp -Pname=myscreens
```

This creates `examples/myscreens/` with a `PicodroidManifest.xml`, a `build.gradle.kts`, and a
`java/myscreens/` source root holding a starter `Myscreens` Application class; use your name
wherever the snippets below say `tutorial_screens`. This app needs no `Application` subclass, so
delete the starter class and replace the generated one-line manifest with Android's long form, one
`<activity>` per screen — this is the committed app's:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest package="tutorial_screens" version="1.0">
    <!-- Every screen is an <activity>. The one with the MAIN / LAUNCHER intent filter is what the
         launcher opens; the others are reached by startActivity(new Intent(this, ...)). -->
    <application>
        <activity android:name=".HomeActivity">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
        <activity android:name=".CounterActivity" />
        <activity android:name=".AboutActivity" />
    </application>
</manifest>
```

The activity with the `MAIN` / `LAUNCHER` intent filter is the app's entry: the launcher opens it,
and it is the first entry on the back stack. `package` is the app's identity on a device that holds
several apps. Declaring every screen also lets the build check each `startActivity`: the Gradle
`verifyManifest` task refuses an `Intent` that names an undeclared Activity. See the
[manifest reference](/reference/manifest/#the-android-form) for every supported element, and
[your first app](/get-started/first-app/) for the basics of the project layout.

## Strings

The screens' texts live in `res/values/strings.xml`, as on Android, and the code reads them through
the generated `R` class with `getString(R.string.…)`; `getString(int, Object...)` formats, so
`Count: %d` takes the number ([resources](/guides/resources/)):

```xml
<?xml version="1.0" encoding="utf-8"?>
<resources>
    <string name="home_title">Tutorial: Screens</string>
    <string name="counter_title">Counter</string>
    <string name="about_title">About</string>
    <string name="about_body">Back-stack tutorial app.\nEach screen is an Activity.</string>

    <string name="button_counter">Counter</string>
    <string name="button_about">About</string>
    <string name="button_increment">Increment</string>
    <string name="button_back">Back</string>

    <!-- getString(R.string.count, count) fills in the number. -->
    <string name="count">Count: %d</string>
</resources>
```

An `Intent` names its target by class, with Android's `(Context, Class)` constructor:
`new Intent(this, CounterActivity.class)`. `startActivity` instantiates the target through its
public no-arg constructor and pushes it on the back stack, where it runs
`onCreate → onStart → onResume`. The entry Activity gets the same treatment at boot.

## The Home hub

Home is the root of the back stack. It builds a vertical `LinearLayout` with a title and two buttons;
each button pushes another Activity. `buttonParams()` is the one `LinearLayout.LayoutParams` every
button in the app uses — full width, 8 px below the view above — and the other screens call it as
`HomeActivity.buttonParams()`:

```java
public class HomeActivity extends Activity {
  private static final String TAG = "HomeActivity";

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "onCreate");

    // The content view fills the window; no size needed.
    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText(getString(R.string.home_title));
    title.setTextColor(Color.WHITE);
    root.addView(title);

    // Views are locals, as on Android: the content view keeps them, listeners included, for as
    // long as this Activity lives. A view becomes a field only when later code needs it.
    Button counterButton = new Button(this);
    counterButton.setText(getString(R.string.button_counter));
    counterButton.setOnClickListener(v -> startActivity(new Intent(this, CounterActivity.class)));
    root.addView(counterButton, buttonParams());

    Button aboutButton = new Button(this);
    aboutButton.setText(getString(R.string.button_about));
    aboutButton.setOnClickListener(v -> startActivity(new Intent(this, AboutActivity.class)));
    root.addView(aboutButton, buttonParams());

    setContentView(root);
  }

  /** A full-width button, a little below the view above it. */
  static LinearLayout.LayoutParams buttonParams() {
    LinearLayout.LayoutParams lp =
        new LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
    lp.topMargin = 8;
    return lp;
  }
```

Two things here matter on embedded:

- **Views are locals, as on Android.** `counterButton` and `aboutButton` are locals in `onCreate`:
  the content view keeps them, listeners included, for as long as this Activity lives. A view
  becomes a field only when later code needs it. See [embedded gotchas](/guides/embedded-gotchas/)
  for the one case that differs from Android (a view never added to a parent).

- **`setContentView(root)` is mandatory.** Until you call it the Activity has no visible tree. It
  replaces whatever the previous screen showed, and the root fills the window, as on Android, so
  it needs no size of its own; each button's size comes from its `LinearLayout.LayoutParams`.

The third thing to notice is what Home does *not* have: a BACK override. BACK's default behaviour
is to `finish()` the top Activity — and Home *is* the only Activity in the stack when it's showing,
so finishing it pops the last entry, which ends the app and returns to the launcher. That is the
Android home-screen behaviour and the only way off a button board back to the launcher, so leave
it alone:

```java
  // This is the root Activity, so no onBackPressed override: BACK runs the inherited finish(),
  // popping the last stack entry, which ends the app and returns to the launcher, as on Android.
}
```

You will see `onPause`, `onStop` and `onDestroy` in the log when you press BACK here, and then the
launcher's own screen — on a board whose firmware carries the launcher, which is every RP2350 board.
Where there is none (the RP2040 testbench, or a simulator started without `--system-apps`) the app
just ends. See [button navigation](/guides/button-navigation/) for how BACK is routed on hardware,
and the [launcher guide](/guides/launcher/#coming-back) for the ways back.

## A stateful screen

Counter keeps a running count in a plain `int` field and a label View. The increment button mutates
both:

```java
public class CounterActivity extends Activity {
  private static final String TAG = "CounterActivity";

  private int count = 0;

  // countLabel is a field because the click handler updates it; the button is a local — the
  // tree keeps it and its listener alive, as on Android.
  private TextView countLabel;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "onCreate");

    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText(getString(R.string.counter_title));
    title.setTextColor(Color.WHITE);
    root.addView(title);

    countLabel = new TextView(this);
    countLabel.setText(getString(R.string.count, count));
    countLabel.setTextColor(Color.CYAN);
    root.addView(countLabel);

    Button incrementButton = new Button(this);
    incrementButton.setText(getString(R.string.button_increment));
    incrementButton.setOnClickListener(
        v -> {
          count++;
          Log.i(TAG, "count=" + count);
          countLabel.setText(getString(R.string.count, count));
        });
    root.addView(incrementButton, HomeActivity.buttonParams());

    setContentView(root);
    // No Back button here: the BACK key's default onBackPressed() calls finish() for us.
  }
```

The state lives in the **Activity instance** — `count` is a field on this object. Counter doesn't
override `onBackPressed`, so BACK runs the default `finish()`, which pops Counter off the stack and
destroys the instance. The next time you open Counter from Home, the framework constructs a **fresh
instance** through its no-arg constructor, so `count` starts at `0` again. There is no automatic
state restoration across a finish. (For state that should outlive a screen, see
[Passing data between screens](#passing-data-between-screens) below.)

## A screen with an explicit Back button

About is a static screen that adds an explicit Back button. Its click handler calls `finish()`:

```java
public class AboutActivity extends Activity {
  private static final String TAG = "AboutActivity";

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "onCreate");

    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText(getString(R.string.about_title));
    title.setTextColor(Color.WHITE);
    root.addView(title);

    TextView body = new TextView(this);
    body.setText(getString(R.string.about_body));
    body.setTextColor(Color.WHITE);
    root.addView(body);

    // A local, as on Android: the content view keeps the button and its listener alive.
    Button backButton = new Button(this);
    backButton.setText(getString(R.string.button_back));
    backButton.setOnClickListener(v -> finish());
    root.addView(backButton, HomeActivity.buttonParams());

    setContentView(root);
  }
```

`finish()` is exactly what the BACK key does by default (`Activity.onBackPressed → finish()`), so the
on-screen Back button and the hardware BACK key follow the same path: pop About off the stack,
destroy it, and reveal Home underneath. `finish()` pops *this* Activity; if it were the last entry in
the stack the app would exit.

## Lifecycle: read the logs

Picodroid fires the same lifecycle callbacks as Android, in the same interleaved order. When you tap
**Counter** on Home, the framework pushes Counter on top of Home:

```text
HomeActivity    onPause
CounterActivity onCreate
CounterActivity onStart
CounterActivity onResume
HomeActivity    onStop
```

The new top is fully resumed *before* the covered Activity is stopped — `Home.onStop` lands after
`Counter.onResume`, matching Android. (The bare `onStart`/`onStop` lines above aren't logged by this
app, which only overrides `onCreate`/`onResume`/`onPause`/`onDestroy`, but they fire in this order.)

Now press BACK in Counter. The default `onBackPressed` calls `finish()`, popping Counter and
restoring Home:

```text
CounterActivity onPause
CounterActivity onStop
CounterActivity onDestroy
HomeActivity    onRestart
HomeActivity    onStart
HomeActivity    onResume
```

Two things to notice:

- **Home's `onCreate` does not run again.** When Counter was pushed, Home's widget tree was hidden
  and snapshotted into its stack entry; on the pop it's restored as-is before
  `onRestart`/`onStart`/`onResume`. You build the UI once in `onCreate` and never rebuild it on
  return — the tree survives the round trip. (Rebuilding from `onResume` is still allowed if you
  want it.) The exception is memory pressure: when a `startActivity` finds the heap or the LVGL pool
  nearly full, the framework destroys the covered Activities and re-creates each one when the user
  comes back to it, with `onCreate(savedInstanceState)` carrying what its `onSaveInstanceState`
  saved. A small app like this one never gets there; see
  [saved instance state](/api/ui/#saved-instance-state) for the screens that might.

- **Counter's `onDestroy` runs** because `finish()` truly destroys it — which is why its `count`
  resets next time, as covered above.

Run the app in the simulator and watch the `[HomeActivity]` and `[CounterActivity]` `Log.i` lines
scroll by as you navigate:

```bash
./scripts/sim.sh --app tutorial_screens
```

See [debugging](/guides/debugging/) for more on reading lifecycle traces.

## Passing data between screens

The tutorial app shares no data between screens, but you'll want to eventually — and there's one
correctness note worth internalising first.

`Intent` has extras (`putExtra` / `getIntExtra` / `getStringExtra` / `getBooleanExtra`), and an
Activity reads the Intent that launched it with `getIntent()` (null only for a manifest `activity=`
boot with no app-side launch). That covers one-shot arguments — an id, a mode, a title:

```java
startActivity(new Intent(this, CounterActivity.class).putExtra("start", 10));
// in CounterActivity.onCreate():
int start = getIntent().getIntExtra("start", 0);
```

For *shared state* — a repository, settings, a sensor cache — the idiomatic way is an app-scoped
singleton injected where it is needed: mark the class `@Singleton` with an `@Inject` constructor and
declare `@Inject` fields in each Activity; the framework populates them before `onCreate()`. The
hand-written `ApplicationComponent` / `ApplicationComponent.current()` shape is there if you
prefer no generated code. See the [Services & DI reference](/api/services/) for both, and the
[background service tutorial](/tutorials/background-service/) for extras delivered to Services.

## Run it

Build and launch the app in the host simulator:

```bash
./scripts/sim.sh --app tutorial_screens
```

The first log lines you should see, as the Application boots and pushes Home:

```text
[HomeActivity] onCreate
[HomeActivity] onResume
```

Tap **Counter** or **About** to push a screen, and About's Back button to pop it. The simulator's
default board, `testbench_rp2350`, has a touch panel and no buttons, so it has no BACK key. For one,
simulate a board that has both, and add the launcher so that BACK on Home has somewhere to go:

```bash
./scripts/sim.sh --board pico_touch_kit --app tutorial_screens --system-apps
```

Backspace on the host keyboard is that board's BACK button. Tap **Increment** a few times, go BACK
to Home, then re-enter Counter — the count is `0` again, because the Activity was destroyed and
rebuilt. BACK on Home ends the app and brings up the launcher, which lists it to start again.

See [the simulator guide](/get-started/simulator/) for input and scripting details, and
[hot-swap](/get-started/hot-swap/) to push changes to a running build without a full reflash. For the
complete API surface used here — `Activity`, `Intent`, `LinearLayout`, `Button`, `TextView` — see the
[UI reference](/api/ui/) and [core reference](/api/core/).
