---
title: "Services & DI"
description: "Service / ServiceConnection / Notification / IBinder, plus dependency injection: compile-time @Inject / @Singleton and the manual DI components."
---

:::caution[Preview]
The Services & DI surface was introduced in v0.4.0. APIs documented on this page are reasonably stable but may change between releases — check [Release notes](/project/release-notes/) before relying on a specific signature in long-lived code.
:::

Picodroid mirrors the Android `Service` shape closely enough that an Android developer can pick it up without re-learning the pattern, but pares the lifecycle down to what fits on a Pico: no Binder IPC, no remote services, no system-process dispatch.

## `picodroid.app.Service`

A long-running background component with a lifecycle independent of any `Activity`. Subclass it in your app:

```java
package myapp;

import picodroid.app.Service;
import picodroid.os.IBinder;
import picodroid.content.Intent;
import picodroid.util.Log;

public class CounterService extends Service {
    private int count;
    private final LocalBinder binder = new LocalBinder();

    @Override
    public void onCreate() {
        binder.service = this;          // wire the handle up front
        Log.i("CounterService", "onCreate");
    }

    @Override
    public int onStartCommand(Intent intent, int startId) {
        count++;
        Log.i("CounterService", "tick=" + count);
        return Service.START_STICKY;    // return value is ignored on picodroid (see below)
    }

    @Override
    public IBinder onBind(Intent intent) {
        return binder;
    }

    @Override
    public void onDestroy() {
        Log.i("CounterService", "onDestroy");
    }

    // IBinder is an interface; LocalBinder just carries a direct reference to the
    // service (single-process — there is no IPC stub generation).
    public static class LocalBinder implements IBinder {
        public CounterService service;
    }
}
```

### Lifecycle

| Callback | When it fires |
|---|---|
| `onCreate()` | Once, the first time the service is started or bound. |
| `onStartCommand(Intent, int startId)` | Each call to `Context.startService()` (including repeats). The return value is **ignored** on picodroid — see the note below. |
| `onBind(Intent)` | First call to `Context.bindService()` for this service. Return an `IBinder` (typically a custom `LocalBinder`). Cached and reused across subsequent binds. |
| `onUnbind(Intent)` | Last bound client unbinds. Default returns `false`; return `true` to receive `onRebind` when a new client binds later. |
| `onRebind(Intent)` | A client binds again after `onUnbind` returned `true` (and the service was not destroyed in between). `onBind` is **not** called again — the cached `IBinder` is reused, matching Android's contract. |
| `onDestroy()` | Service is being torn down (last unbind + no `startService` keepalive, or explicit `stopService`). |

On picodroid the OS never kills a running service, so `onStartCommand`'s return value has no runtime effect — `START_STICKY` and `START_NOT_STICKY` exist only for source-level Android compatibility. Return `START_STICKY` by convention.

## `picodroid.os.IBinder`

Marker **interface** for the object handed back from `onBind`. Implement it with your own `LocalBinder` that carries a reference to the service (no IPC stub generation in v1 — `LocalBinder` is just a Java reference handed across `bindService`):

```java
public static class LocalBinder implements IBinder {
    public CounterService service;
}
```

Picodroid is single-process, so there is no AIDL / Messenger / true Binder IPC. Clients cast the `IBinder` they receive back to your `LocalBinder` type and read the field.

## `picodroid.app.Notification` and `startForeground`

A foreground service shows a persistent banner while it runs. There is no idle or low-memory kill policy on an MCU, so "foreground" here is about the banner, not about survival. To opt in, build a `Notification` and call `startForeground` from `onStartCommand`:

```java
import picodroid.app.Notification;

@Override
public int onStartCommand(Intent intent, int startId) {
    Notification n = new Notification.Builder()
        .setContentTitle("Logging sensors")
        .setContentText("ring buffer 0/256")
        .build();
    startForeground(NOTIFICATION_ID, n);
    return Service.START_STICKY;
}
```

`stopForeground(true)` removes the notification; `onDestroy` cancels it automatically.

### `picodroid.app.NotificationManager`

For notifications outside the foreground-service flow, post or cancel by ID through the
`NotificationManager` singleton. Picodroid renders every notification as a single persistent top-of-screen banner.

```java
import picodroid.app.Notification;
import picodroid.app.NotificationManager;

Notification n = new Notification.Builder()
    .setContentTitle("Upload complete")
    .build();

NotificationManager nm = NotificationManager.getInstance();
nm.notify(1, n);   // post under id 1
nm.cancel(1);      // dismiss it
```

## `picodroid.content.Context` — start / bind / stop

The `Context` (your `Application` or `Activity`) drives the service lifecycle:

```java
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.os.IBinder;

Intent i = new Intent(CounterService.class);

// Fire-and-forget: invokes onStartCommand
startService(i);

// Bind: invokes onBind, then onServiceConnected
ServiceConnection conn = new ServiceConnection() {
    public void onServiceConnected(IBinder binder) {
        CounterService s = ((CounterService.LocalBinder) binder).service;
        // call s.someMethod() ...
    }
    public void onServiceDisconnected() {
        // last unbind, owning Activity destroyed, or app exit — drop the reference
    }
};
bindService(i, conn);   // 2-arg; binding implicitly creates the service if needed

unbindService(conn);
stopService(i);
```

`bindService` takes just `(Intent, ServiceConnection)` — there is no `flags` parameter and no
`Context.BIND_AUTO_CREATE` constant; binding always creates the service if it isn't running.

## Dependency injection: `@Inject` / `@Singleton`

Picodroid ships a compile-time DI framework in the Dagger/Hilt shape. Annotate constructors, fields and methods with JSR-330's `javax.inject.Inject`, scope app-wide objects with `javax.inject.Singleton`, and the build generates the wiring. Nothing is resolved at runtime — pico-jvm has no reflection and drops annotations from class files — so the only thing that reaches the device is ordinary generated Java.

```java
import javax.inject.Inject;
import javax.inject.Singleton;

@Singleton
public class SensorRepository {
  @Inject
  public SensorRepository(Formatter formatter) { /* ... */ }   // constructor injection
}

public class Formatter {
  @Inject
  public Formatter() {}
}

public class HomeActivity extends Activity {
  @Inject SensorRepository repo;      // field injection — set before onCreate()
  @Inject Formatter formatter;

  @Override
  public void onCreate() {
    // repo and formatter are already populated
  }
}
```

What the build does:

- Every class with an `@Inject` constructor gets a generated `Foo_Factory` with `public static Foo get()`. Unscoped classes yield a fresh instance per injection; a `@Singleton` class yields one instance per process, created lazily on first use and held in a static field (a GC root shared by every thread).
- Every class with `@Inject` fields or methods gets a generated `Foo_MembersInjector`. Superclass members are injected first, then fields, then methods, each in declaration order.
- **Framework-owned components — `Application`, `Activity`, `Service` — are injected automatically** right after construction and before `onCreate()`, like Hilt's `@AndroidEntryPoint`. They keep their no-arg constructor; use field or method injection there.
- Anything else is pulled from the graph with `Foo_Factory.get()`, the equivalent of a Dagger component accessor (see `Message_Factory.get()` in `injectdemo`).
- Types that cannot carry an `@Inject` constructor — SDK classes, interfaces, abstract types — are bound by `@Provides` methods on a `@Module` class (`picodroid.di.Module` / `picodroid.di.Provides`, the `dagger.Module` / `dagger.Provides` counterparts). Every `@Module` in the app is installed automatically; there is no `@Component` to declare. Methods may be `static` (preferred) or instance methods on a module with a no-arg constructor (the module is then created once, lazily); parameters are injected like constructor parameters; `@Singleton` on the method scopes the value. Each method becomes a generated `Mod_ProvideFooFactory`.

  ```java
  @Module
  public final class AppModule {
    @Provides @Singleton
    static SharedPreferences providePrefs() { return SharedPreferences.open("app"); }

    @Provides
    static Greeter provideGreeter(Clock clock) { return new FriendlyGreeter(clock); }
  }
  ```

- `javax.inject.Provider<T>` and `picodroid.di.Lazy<T>` (the `dagger.Lazy` counterpart) can be injected anywhere a `T` can. A `Provider` hands out a fresh instance per `get()` for unscoped types (the one instance for a `@Singleton`) and constructs nothing until called; a `Lazy` calls the factory once and memoizes. Both break dependency cycles, since neither constructs anything at injection time. They are generated on demand as `T_Provider` / `T_Lazy`.

Rules the compiler enforces — each violation is a compile error that says why:

- One `@Inject` constructor per class, never on an abstract class or on an `Application` / `Activity` / `Service` subclass (the framework constructs those with the no-arg constructor).
- `@Inject` fields must be non-private, non-final and non-static; the injector lives in the same package, so package-private is the idiom. `@Inject` methods must be non-private, non-static, non-abstract and non-generic.
- Every dependency must have exactly one binding in the app — an `@Inject` constructor on a concrete, non-generic class, or one `@Provides` method — optionally wrapped in one `Provider<T>` / `Lazy<T>`. A type bound twice is an error. Parameterized types (other than the two wrappers), qualifiers and `@Binds` are not supported yet.
- No dependency cycles through direct constructor or member edges; break one with `Provider<T>` or `Lazy<T>`.
- An `@Inject` field's name must not be reused anywhere in its superclass or subclass chain: pico-jvm resolves instance fields by name only.
- `@Singleton` is the only scope; it goes on a class with an `@Inject` constructor or on a `@Provides` method. `@Provides` methods must live in a `@Module`, be non-private and non-abstract, return a class or interface type, and have unique names within the module; a module cannot itself have an `@Inject` constructor or members.

Divergences worth knowing: the annotations have `SOURCE` retention (JSR-330 says `RUNTIME`), so using them adds nothing to the PAPK; the generated classes ship inside the PAPK and cost about 20 B of RAM each at boot plus their metadata when first touched, and no firmware flash at all. Design, generated-code contract and roadmap: `docs/designs/inject-annotations-2026-08.md`. End-to-end example: [`examples/injectdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/injectdemo); `picoenvmon` uses it throughout.

Kotlin apps get the same processor through kapt (`picodroid-papk-kotlin` applies it; nothing to configure). The shapes that work: `@Inject lateinit var` for fields (a `lateinit` property has a public backing field; a plain `var`/`val` has a private one and is rejected), `@Inject constructor(...)`, `@Singleton` on the class, a `@Module object` whose `@Provides` methods are `@JvmStatic` (the static path), and a `@Module class` for instance `@Provides` methods. Avoid `@Provides` in a `companion object` (kotlinc emits the method twice, so one copy is always a stray outside the module), `@Module object` methods without `@JvmStatic` (the object's constructor is private, which the instance path rejects), `inner class`, and `kotlin.Lazy` — import `picodroid.di.Lazy`. Kotlin twin of the example: [`examples/injectdemo_kt/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/injectdemo_kt).

## Manual DI: `ApplicationComponent` / `ActivitySingletonComponent`

The hand-written shape predates `@Inject` and remains supported for apps that want an explicit graph with no generated code:

```java
import picodroid.di.ApplicationComponent;
import picodroid.di.ActivitySingletonComponent;

public final class EnvAppComponent extends ApplicationComponent {
    private final SensorRepository repo = new SensorRepository(/* ... */);
    public SensorRepository sensorRepository() { return repo; }
}

public final class HomeActivityComponent extends ActivitySingletonComponent {
    private final HomeViewModel vm;
    public HomeActivityComponent() {                       // no-arg constructor
        EnvAppComponent app = (EnvAppComponent) app();    // app() resolves the ApplicationComponent
        this.vm = new HomeViewModel(app.sensorRepository());
    }
    public HomeViewModel viewModel() { return vm; }
}
```

`ApplicationComponent` is process-singleton — its constructor stores itself, and `ApplicationComponent.current()` (or the protected `app()` accessor inside an `ActivitySingletonComponent`) returns it. `ActivitySingletonComponent` has a no-arg constructor that grabs `current()` for you; it is per-Activity-instance and recreated on every `onCreate`. Construct your `ApplicationComponent` once in `Application.onCreate()` and your `ActivitySingletonComponent` in each `Activity.onCreate()`.

This pattern keeps the dependency graph explicit, statically typed, and visible in the source — no reflection, no codegen, no startup cost.

The two styles coexist: an `ApplicationComponent` subclass can itself be a `@Singleton` with an `@Inject` constructor, which makes it injectable while `ApplicationComponent.current()` keeps working for legacy call sites (`LegacyComponent` in `injectdemo`).

See [`examples/servicedemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/servicedemo) for the full Service v1 lifecycle in one non-UI run, and [`examples/picoenvmon/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/picoenvmon) for `@Inject` / `@Singleton` in production-shape code.
