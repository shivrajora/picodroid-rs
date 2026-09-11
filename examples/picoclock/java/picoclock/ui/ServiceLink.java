// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picoclock.AlarmService;
import picodroid.content.ServiceConnection;
import picodroid.os.IBinder;

/**
 * The {@link ServiceConnection} every screen binds through.
 *
 * <p>It exists as a class of its own rather than as something {@link BaseActivity} implements
 * because of how the framework calls back into Java: the lookup behind a native upcall is flat, so
 * it finds {@code onServiceConnected} only on the exact class of the object it was handed. A
 * connection inherited from a base class is never found, and the bind fails silently — no
 * exception, no log, just a screen whose service reference stays null. Declaring both methods on
 * the concrete class that is actually passed to {@code bindService} is what makes them reachable.
 */
final class ServiceLink implements ServiceConnection {
  private final BaseActivity screen;

  ServiceLink(BaseActivity screen) {
    this.screen = screen;
  }

  @Override
  public void onServiceConnected(IBinder binder) {
    screen.attach(((AlarmService.LocalBinder) binder).service);
  }

  @Override
  public void onServiceDisconnected() {
    screen.attach(null);
  }
}
