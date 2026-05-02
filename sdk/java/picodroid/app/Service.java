// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.Intent;

/**
 * Background component analogous to {@code android.app.Service}. A Service has a lifecycle
 * independent of any Activity and runs on the main thread by default — spawn a {@link
 * picodroid.concurrent.Thread} for blocking work.
 *
 * <h2>Started services</h2>
 *
 * <pre>{@code
 * startService(new Intent(SyncService.class));   // → onCreate, onStartCommand
 * stopService(new Intent(SyncService.class));    // → onDestroy
 * }</pre>
 *
 * <h2>Bound services</h2>
 *
 * <pre>{@code
 * ServiceConnection conn = new ServiceConnection() {
 *   public void onServiceConnected(IBinder b) { ... }
 *   public void onServiceDisconnected()       { ... }
 * };
 * bindService(new Intent(SyncService.class), conn);   // → onCreate, onBind, onServiceConnected
 * unbindService(conn);                                // → onUnbind, onDestroy (if no other refs)
 * }</pre>
 *
 * <p>onCreate runs on the first start <em>or</em> first bind; onDestroy runs when the Service is
 * neither started nor bound. The framework owns instantiation: subclasses must have a public no-arg
 * constructor.
 */
public abstract class Service {
  /**
   * Returned from {@link #onStartCommand} to indicate the system should re-create the Service after
   * a kill. On picodroid the OS never kills a running Service, so the constant has no runtime
   * effect — it exists for source-level Android compatibility.
   */
  public static final int START_STICKY = 1;

  /**
   * Returned from {@link #onStartCommand} to indicate no-restart-on-kill semantics. No-op on
   * picodroid.
   */
  public static final int START_NOT_STICKY = 2;

  public void onCreate() {
    // Subclass overrides
  }

  /**
   * Called for every {@code startService} (including repeats). {@code startId} increments
   * monotonically per Service instance and is supplied to {@code stopSelfResult} (not yet
   * implemented).
   */
  public int onStartCommand(Intent intent, int startId) {
    return START_STICKY;
  }

  /**
   * Return the IBinder clients receive via {@link ServiceConnection#onServiceConnected}, or null to
   * refuse binding.
   */
  public IBinder onBind(Intent intent) {
    return null;
  }

  /**
   * Last client unbound. Return {@code true} to receive {@link #onRebind} on the next bind (not yet
   * implemented).
   */
  public boolean onUnbind(Intent intent) {
    return false;
  }

  public void onDestroy() {
    // Subclass overrides
  }

  /** Stop this Service. Equivalent to {@code Context.stopService(new Intent(thisClass))}. */
  public final native void stopSelf();

  /**
   * Promote this Service to foreground state with a persistent notification. Picodroid renders the
   * notification as a top-of-screen banner; ID {@code id} can later be passed to {@link
   * #stopForeground} or {@link NotificationManager#cancel}.
   */
  public final native void startForeground(int id, Notification notification);

  /** Demote from foreground state. If {@code removeNotification} is true the banner is cleared. */
  public final native void stopForeground(boolean removeNotification);
}
