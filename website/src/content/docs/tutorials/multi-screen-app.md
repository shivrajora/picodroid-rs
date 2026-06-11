---
title: "Tutorial: a multi-screen app with a back stack"
description: "Build a Home hub that pushes Counter and About screens, learning Activities, the back stack, lifecycle, and view preservation."
---

This tutorial builds a small three-screen app: a **Home** hub with two buttons that push a
**Counter** screen and an **About** screen onto the back stack. Pressing BACK (or a Back button)
pops the top screen and returns to Home.

Along the way you'll learn how Picodroid models screens as `Activity` objects, how `startActivity`
and `finish()` drive the back stack, the order lifecycle callbacks fire, why a paused Activity's
widget tree is preserved across a push, and how to keep BACK from exiting the app at the root.

The finished code is the committed
[`examples/tutorial_screens/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/tutorial_screens)
app — every snippet below is copied from it, so it builds and runs as-is.

## Scaffold

Generate a new app skeleton with the `newApp` Gradle task:

```bash
./gradlew newApp -Pname=tutorial_screens
```

This creates `examples/tutorial_screens/` with a `PicodroidManifest.xml`, a `build.gradle.kts`, and a
`java/tutorial_screens/` source root. The manifest names the `Application` class that the framework
instantiates at boot:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest package="tutorial_screens" version="1.0">
    <application application="tutorial_screens/TutorialScreensApp" />
</manifest>
```

The `application` attribute is the only entry point the framework reads — see the
[manifest reference](/reference/manifest/) for every supported element, and
[your first app](/get-started/first-app/) for the basics of the project layout.

## The Application entry point

`Application.onCreate` runs first, once, at boot — before any Activity exists. Seed the back stack by
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
  public void onCreate() {
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

Three things here matter on embedded:

- **Field-held listener buttons.** `counterButton` and `aboutButton` are instance fields, not locals.
  A View that only the native listener registry references can be swept by the garbage collector,
  killing its callback mid-session. Holding it in a field roots it through the Activity. See
  [embedded gotchas](/guides/embedded-gotchas/) for the full pattern.

- **`setContentView(root)` is mandatory.** Until you call it the Activity has no visible tree. It
  delegates to the `Display`, replacing whatever the previous screen showed.

The second thing Home needs is a root-Back override. BACK's default behaviour is to `finish()` the
top Activity — and Home *is* the only Activity in the stack when it's showing, so finishing it pops
the last entry and exits the whole app. Swallow BACK at the root instead:

```java
  // This is the root Activity: the default onBackPressed would finish() it, popping the last stack
  // entry and exiting the whole app. Swallow BACK instead (deliberately no super call).
  @Override
  public void onBackPressed() {
    Log.i(TAG, "onBackPressed (ignored at root)");
  }
}
```

The key detail is the absence of a `super.onBackPressed()` call. The inherited `onBackPressed`
finishes the Activity; by overriding it without calling super, Home consumes BACK and stays put. See
[button navigation](/guides/button-navigation/) for how BACK is routed on hardware.

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
  public void onCreate() {
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
  public void onCreate() {
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
HomeActivity    onStart
HomeActivity    onResume
```

Two things to notice:

- **Home's `onCreate` does not run again.** When Counter was pushed, Home's widget tree was hidden
  and snapshotted into its stack entry; on the pop it's restored as-is before `onStart`/`onResume`.
  You build the UI once in `onCreate` and never rebuild it on return — the tree survives the round
  trip. (Rebuilding from `onResume` is still allowed if you want it.)

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

`Intent` has extras (`putExtra` / `getIntExtra` / `getStringExtra` / `getBooleanExtra`), **but an
Activity cannot read them.** There is no `getIntent()` on `Activity`. Extras are delivered only to
**Services**, in `onStartCommand` / `onBind`. So this does *not* work for screen-to-screen data:

```java
// The extra is set, but CounterActivity has no way to read it back — there is no getIntent().
startActivity(new Intent(CounterActivity.class).putExtra("start", 10));
```

The idiomatic way to share state between Activities is an **app-scoped DI singleton**. Construct an
`ApplicationComponent` subclass in `Application.onCreate`, then reach it from any Activity via
`ApplicationComponent.current()` and pull shared state through it. Reserve Intent extras for the
Service case — see the [background service tutorial](/tutorials/background-service/) and the
[Services & DI reference](/api/services/).

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

Tap **Counter** or **About** to push a screen; press BACK (or About's Back button) to pop it. Tap
**Increment** a few times, go BACK to Home, then re-enter Counter — the count is `0` again, because
the Activity was destroyed and rebuilt.

See [the simulator guide](/get-started/simulator/) for input and scripting details, and
[hot-swap](/get-started/hot-swap/) to push changes to a running build without a full reflash. For the
complete API surface used here — `Activity`, `Intent`, `LinearLayout`, `Button`, `TextView` — see the
[UI reference](/api/ui/) and [core reference](/api/core/).
