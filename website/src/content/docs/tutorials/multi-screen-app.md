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
wherever the snippets below say `tutorial_screens`. The manifest names the `Application` class that
the framework instantiates at boot — this is the committed app's:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest package="tutorial_screens" version="1.0">
    <application application="tutorial_screens/TutorialScreensApp" />
</manifest>
```

The `application` attribute is the entry point the framework reads, and `package` is the app's
identity on a device that holds several apps — see the
[manifest reference](/reference/manifest/) for every supported element, and
[your first app](/get-started/first-app/) for the basics of the project layout.

## The Application entry point

`Application.onCreate` runs first, once, when the app starts — before any Activity exists. Seed the back stack by
launching the root screen from it:

```java
public class TutorialScreensApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(HomeActivity.class));
  }
}
```

An `Intent` names its target by class: `new Intent(HomeActivity.class)`. There is no
`(Context, Class)` constructor — pass the target class directly. `startActivity` instantiates the
target through its public no-arg constructor and pushes it as the first stack entry, which then runs
`onCreate → onStart → onResume`.

## The Home hub

Home is the root of the back stack. It builds a vertical `LinearLayout` with a title and two buttons;
each button pushes another Activity:

```java
public class HomeActivity extends Activity {
  private static final String TAG = "HomeActivity";

  // Views with listeners are held as fields so the GC always sees them rooted through this
  // Activity, not only through the native listener registry — best practice for callback views.
  private Button counterButton;
  private Button aboutButton;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "onCreate");
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Tutorial: Screens");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    counterButton = new Button("Counter");
    counterButton.setSize(200, 40);
    counterButton.setOnClickListener(v -> startActivity(new Intent(CounterActivity.class)));
    root.addView(counterButton);

    aboutButton = new Button("About");
    aboutButton.setSize(200, 40);
    aboutButton.setOnClickListener(v -> startActivity(new Intent(AboutActivity.class)));
    root.addView(aboutButton);

    setContentView(root);
  }
```

Two things here matter on embedded:

- **Field-held listener buttons.** `counterButton` and `aboutButton` are instance fields, not locals.
  A View that only the native listener registry references can be swept by the garbage collector,
  killing its callback mid-session. Holding it in a field roots it through the Activity. See
  [embedded gotchas](/guides/embedded-gotchas/) for the full pattern.

- **`setContentView(root)` is mandatory.** Until you call it the Activity has no visible tree. It
  delegates to the `Display`, replacing whatever the previous screen showed.

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

  // Field-held views: the label so the click handler can update it, the button so the GC sees the
  // listener-bearing view rooted through this Activity.
  private TextView countLabel;
  private Button incrementButton;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "onCreate");
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Counter");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    countLabel = new TextView();
    countLabel.setText("Count: 0");
    countLabel.setTextColor(Color.CYAN);
    root.addView(countLabel);

    incrementButton = new Button("Increment");
    incrementButton.setSize(200, 40);
    incrementButton.setOnClickListener(
        v -> {
          count++;
          Log.i(TAG, "count=" + count);
          countLabel.setText("Count: " + count);
        });
    root.addView(incrementButton);

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

  // Field-held so the GC roots the listener-bearing button through this Activity.
  private Button backButton;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "onCreate");
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("About");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    TextView body = new TextView();
    body.setText("Back-stack tutorial app.\nEach screen is an Activity.");
    body.setTextColor(Color.WHITE);
    root.addView(body);

    backButton = new Button("Back");
    backButton.setSize(200, 40);
    backButton.setOnClickListener(v -> finish());
    root.addView(backButton);

    setContentView(root);
  }
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
startActivity(new Intent(CounterActivity.class).putExtra("start", 10));
// in CounterActivity.onCreate():
int start = getIntent().getIntExtra("start", 0);
```

For *shared state* — a repository, settings, a sensor cache — the idiomatic way is an app-scoped
singleton injected where it is needed: mark the class `@Singleton` with an `@Inject` constructor and
declare `@Inject` fields in each Activity; the framework populates them before `onCreate()`. The
hand-written `ApplicationComponent` / `ApplicationComponent.current()` shape still works if you
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
