// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

import picodroid.os.IBinder;

/**
 * Callback interface passed to {@link Context#bindService} and notified when the bound Service is
 * created and torn down. Both methods run on the main thread, between frames.
 */
public interface ServiceConnection {
  /**
   * The Service has been instantiated and {@code onBind} returned {@code binder}. Cast {@code
   * binder} to the Service's LocalBinder type to access the Service.
   */
  void onServiceConnected(IBinder binder);

  /**
   * The Service is going away (last unbind, owning Activity destroyed, or app exit). Drop the
   * binder reference; do not call back into it.
   */
  void onServiceDisconnected();
}
