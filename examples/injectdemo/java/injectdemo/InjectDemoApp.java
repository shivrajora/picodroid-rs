// SPDX-License-Identifier: GPL-3.0-only
package injectdemo;

import javax.inject.Inject;
import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.di.ApplicationComponent;
import picodroid.util.Log;

/**
 * {@code @Inject} / {@code @Singleton} end to end: the framework injects this Application's fields
 * before {@code onCreate}, then every Activity and Service it starts gets the same treatment.
 * Assertions ride on the log lines (see scripts/hil-tests.conf).
 */
public class InjectDemoApp extends Application {
  static final String TAG = "InjectDemo";

  /** Kept so HomeActivity can prove its own Greeter is a fresh (unscoped) instance. */
  static Greeter appGreeter;

  @Inject Clock clock;
  @Inject Greeter greeter;
  @Inject LegacyComponent legacy;

  @Override
  public void onCreate() {
    appGreeter = greeter;
    Log.i(TAG, "app clock#" + clock.id() + " legacy=" + (ApplicationComponent.current() == legacy));

    Message m = Message_Factory.get();
    Log.i(
        TAG,
        "Message fields="
            + (m.fieldsOk() ? "ok" : "BAD")
            + " method="
            + (m.methodOk() ? "ok" : "BAD"));

    startService(new Intent(PingService.class));
    startActivity(new Intent(HomeActivity.class));
  }
}
