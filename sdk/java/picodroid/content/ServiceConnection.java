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
   * The connection to the Service {@code name} is going away (last unbind, owning Activity
   * destroyed, or app exit). Drop the binder reference; do not call back into it.
   */
  void onServiceDisconnected(ComponentName name);
}
