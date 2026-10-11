// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

import picodroid.os.IBinder;

/**
 * Callback interface passed to {@link Context#bindService} and notified when the bound Service is
 * created and torn down. Mirrors {@code android.content.ServiceConnection}. Both methods run on the
 * main thread, between frames.
 */
public interface ServiceConnection {
  /**
   * The Service {@code name} has been instantiated and its {@code onBind} returned {@code service}.
   * Cast {@code service} to the Service's LocalBinder type to reach the Service.
   */
  void onServiceConnected(ComponentName name, IBinder service);

  /**
   * The connection to the Service {@code name} was lost: on Android, the Service's process died.
   * Not called for the app's own {@code unbindService}, nor for a connection an Activity left bound
   * when it finished (the framework unbinds it silently, as Android does). Picodroid runs every
   * Service in the app's own process, so no path delivers it today; drop the binder reference here
   * all the same, as portable code does.
   */
  void onServiceDisconnected(ComponentName name);
}
