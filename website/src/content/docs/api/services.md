---
title: "Services & DI"
description: "Service / ServiceConnection / Notification / IBinder, plus the manual DI components — preview surface introduced in v0.4.0."
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
import picodroid.app.IBinder;
import picodroid.content.Intent;
import picodroid.util.Log;

public class CounterService extends Service {
    private int count;

    @Override
    public void onCreate() {
        Log.i("CounterService", "onCreate");
    }

    @Override
    public int onStartCommand(Intent intent, int startId) {
        count++;
        Log.i("CounterService", "tick=" + count);
        return Service.START_STICKY;
    }

    @Override
    public IBinder onBind(Intent intent) {
        return new LocalBinder();
    }

    @Override
    public void onDestroy() {
        Log.i("CounterService", "onDestroy");
    }

    public class LocalBinder extends IBinder {
        public CounterService getService() { return CounterService.this; }
    }
}
```

### Lifecycle

| Callback | When it fires |
|---|---|
| `onCreate()` | Once, the first time the service is started or bound. |
| `onStartCommand(Intent, int startId)` | Each call to `Context.startService()`. Return `START_STICKY` to ask the framework to keep the service alive. |
| `onBind(Intent)` | First call to `Context.bindService()` for this service. Return an `IBinder` (typically a custom `LocalBinder`). Cached and reused across subsequent binds. |
| `onUnbind(Intent)` | Last bound client unbinds. Default returns `false`; returning `true` is reserved for `onRebind` (see below). |
| `onDestroy()` | Service is being torn down (last unbind + no `startService` keepalive, or explicit `stopService`). |

`Service.START_STICKY` is the default return value; the only other recognized constant is `START_NOT_STICKY` (released when `onStartCommand` returns).

> **`onRebind` is not implemented in v1.** `onUnbind` can return `true` to opt into a future
> `onRebind` callback, but the re-bind path is not yet dispatched — a subsequent bind runs
> `onBind` again. Don't rely on `onRebind` firing yet.

## `picodroid.app.IBinder`

Marker base class for the object handed back from `onBind`. Subclass it with your own `LocalBinder` exposing the methods you want clients to call (no IPC stub generation in v1 — `LocalBinder` is just a Java reference handed across `bindService`).

## `picodroid.app.Notification` and `startForeground`

Foreground services keep running across config changes and aren't first-killed by the framework's idle policy. To opt in, build a `Notification` and call `startForeground` from `onStartCommand`:

```java
import picodroid.app.Notification;

@Override
public int onStartCommand(Intent intent, int flags, int startId) {
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
`NotificationManager` singleton. Picodroid renders a notification as a brief top-of-screen banner.

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
import picodroid.app.IBinder;

Intent i = new Intent(CounterService.class);

// Fire-and-forget: invokes onStartCommand
startService(i);

// Bind: invokes onBind, then onServiceConnected
ServiceConnection conn = new ServiceConnection() {
    public void onServiceConnected(IBinder service) {
        CounterService.LocalBinder b = (CounterService.LocalBinder) service;
        CounterService s = b.getService();
        // call s.someMethod() ...
    }
    public void onServiceDisconnected() {
        // service died unexpectedly
    }
};
bindService(i, conn);   // 2-arg; binding implicitly creates the service if needed

unbindService(conn);
stopService(i);
```

`bindService` takes just `(Intent, ServiceConnection)` — there is no `flags` parameter and no
`Context.BIND_AUTO_CREATE` constant; binding always creates the service if it isn't running.

## Manual DI: `ApplicationComponent` / `ActivitySingletonComponent`

Picodroid does not ship a runtime container like Hilt — there's no annotation processing on-device. Instead, the framework gives you a tiny manual-DI shape that the `picoenvmon` example uses end-to-end:

```java
import picodroid.di.ApplicationComponent;
import picodroid.di.ActivitySingletonComponent;

public final class EnvAppComponent extends ApplicationComponent {
    private final SensorRepository repo = new SensorRepository(/* ... */);
    public SensorRepository sensorRepository() { return repo; }
}

public final class HomeActivityComponent extends ActivitySingletonComponent {
    private final HomeViewModel vm;
    public HomeActivityComponent(EnvAppComponent app) {
        super(app);
        this.vm = new HomeViewModel(app.sensorRepository());
    }
    public HomeViewModel viewModel() { return vm; }
}
```

`ApplicationComponent` is process-singleton; `ActivitySingletonComponent` is per-Activity-instance and is recreated on every `onCreate`. Wire both up in your `Application.onCreate()` and `Activity.onCreate()` respectively.

This pattern keeps the dependency graph explicit, statically typed, and visible in the source — no reflection, no codegen, no startup cost.

See [`examples/servicedemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/servicedemo) for the full Service v1 lifecycle in one non-UI run, and [`examples/picoenvmon/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/picoenvmon) for the manual DI pattern in production-shape code.
