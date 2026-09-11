// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picoclock.AlarmService;
import picodroid.content.Intent;

/**
 * The binding every screen shares: a connection to {@link AlarmService}, claimed in {@code
 * onResume} and released in {@code onPause}, so exactly one screen hears the service however deep
 * the stack is.
 *
 * <p>Nothing here routes a ring. The framework's {@code AlarmManager} starts {@link RingActivity}
 * on top of whatever is showing — and starts this app first if the user has gone elsewhere — which
 * is why an alarm still rings with the app shut down.
 *
 * <h2>Why every subclass re-declares its lifecycle methods</h2>
 *
 * The framework resolves a native-to-Java callback with a flat lookup on the receiver's exact
 * class, falling back to the SDK's own {@code Activity} — never to a class in between. A subclass
 * of this one that does not itself declare {@code onResume} therefore gets {@code Activity}'s empty
 * one, and the {@code super.onResume()} below never runs: the screen binds, and then quietly never
 * hears an alarm. The one-line overrides in each screen are what make these reachable, so they are
 * load-bearing rather than ceremony. {@link ServiceLink} exists for the same reason.
 */
public abstract class BaseActivity extends picodroid.app.Activity implements AlarmService.Listener {

  /** The bound service, or null before the connection lands and after a disconnect. */
  protected AlarmService alarms;

  private final ServiceLink link = new ServiceLink(this);
  private boolean resumed;

  @Override
  public void onCreate() {
    bindService(new Intent(AlarmService.class), link);
  }

  /** Called by {@link ServiceLink} when the connection lands, and with null when it drops. */
  void attach(AlarmService service) {
    alarms = service;
    if (service == null) {
      return;
    }
    if (resumed) {
      service.setListener(this);
    }
    onAlarmsReady();
  }

  /** The service is bound and usable. Screens fill in the parts that need it here. */
  protected void onAlarmsReady() {}

  @Override
  public void onResume() {
    super.onResume();
    resumed = true;
    if (alarms != null) {
      // setListener replays a ring already in progress, so a screen coming
      // forward mid-alarm shows it rather than a clock over a sounding buzzer.
      alarms.setListener(this);
    }
  }

  @Override
  public void onPause() {
    resumed = false;
    if (alarms != null) {
      alarms.setListener(null);
    }
    super.onPause();
  }

  @Override
  public void onDestroy() {
    if (alarms != null) {
      alarms.setListener(null);
    }
    unbindService(link);
    alarms = null;
    super.onDestroy();
  }

  @Override
  public void onAlarmStopped() {}

  @Override
  public void onTick() {}
}
