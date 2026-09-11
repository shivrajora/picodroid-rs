// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picoclock.Alarm;
import picoclock.AlarmService;
import picodroid.content.Intent;

/**
 * The binding every screen shares: a connection to {@link AlarmService}, and the rule that an alarm
 * coming due interrupts whatever screen is up. Only the resumed screen listens — the listener is
 * claimed in {@code onResume} and released in {@code onPause} — so exactly one screen reacts to a
 * ring, however deep the stack is. A Service here cannot start an Activity of its own ({@code
 * startActivity} is on Activity, not Context), so routing a ring to a screen is a screen's job, and
 * this is where it lives rather than in five copies.
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

  /** Set by {@link RingActivity}: the screen that *is* the ring does not launch another. */
  protected boolean handlesRingItself;

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
  public void onAlarmRing(Alarm alarm) {
    if (!handlesRingItself) {
      startActivity(new Intent(RingActivity.class).putExtra(RingActivity.EXTRA_ALARM_ID, alarm.id));
    }
  }

  @Override
  public void onAlarmStopped() {}

  @Override
  public void onTick() {}
}
