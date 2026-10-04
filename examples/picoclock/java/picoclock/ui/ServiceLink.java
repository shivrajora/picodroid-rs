// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picoclock.AlarmService;
import picodroid.content.ComponentName;
import picodroid.content.ServiceConnection;
import picodroid.os.IBinder;

/**
 * The {@link ServiceConnection} every screen binds through, a class of its own so that {@link
 * BaseActivity} hands the framework one small object rather than itself.
 *
 * <p>It used to have to be: the framework once called a connection by a flat lookup on the exact
 * class it was handed, and callbacks a base class declared were never found. They are ordinary
 * interface calls now, so an Activity may implement {@code ServiceConnection} in a base class.
 */
final class ServiceLink implements ServiceConnection {
  private final BaseActivity screen;

  ServiceLink(BaseActivity screen) {
    this.screen = screen;
  }

  @Override
  public void onServiceConnected(ComponentName name, IBinder binder) {
    screen.attach(((AlarmService.LocalBinder) binder).service);
  }

  @Override
  public void onServiceDisconnected(ComponentName name) {
    screen.attach(null);
  }
}
