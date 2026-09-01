// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.os.IBinder;

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
 *
 * <p>A Service is a {@link Context}, as on Android: {@code getSystemService}, {@code
 * getSharedPreferences}, {@code startService} and {@code bindService} are available on {@code
 * this}. One divergence: a binding made from inside a Service is owned by the <em>foreground
 * Activity</em> at the time of the call (bindings are tracked per Activity), so it is released when
 * that Activity finishes, not when the Service is destroyed.
 */
public abstract class Service extends Context {
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

  /**
   * Returned from {@link #onStartCommand} to ask for the last Intent to be redelivered after a
   * kill. No-op on picodroid, like {@link #START_STICKY}.
   */
  public static final int START_REDELIVER_INTENT = 3;

  /**
   * {@code flags} bit: the Intent is a redelivery of one the Service was killed before finishing.
   * Never set on picodroid — a Service is never killed — so {@code flags} is always {@code 0}; the
   * constants exist so Android code that tests them compiles.
   */
  public static final int START_FLAG_REDELIVERY = 1;

  /** {@code flags} bit: the Intent is a retry after a failed earlier delivery. Never set. */
  public static final int START_FLAG_RETRY = 2;

  public void onCreate() {
    // Subclass overrides
  }

  /**
   * Called for every {@code startService} (including repeats). Mirrors {@code
   * android.app.Service#onStartCommand(Intent, int, int)}: {@code flags} is always {@code 0} here
   * (see {@link #START_FLAG_REDELIVERY}); {@code startId} increments monotonically per Service
   * instance and is the token {@link #stopSelfResult} checks.
   */
  public int onStartCommand(Intent intent, int flags, int startId) {
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
   * Last client unbound. Return {@code true} to receive {@link #onRebind} when a new client binds
   * (the Service is not destroyed in the meantime). Mirrors {@code android.app.Service#onUnbind}.
   */
  public boolean onUnbind(Intent intent) {
    return false;
  }

  /**
   * A new client bound after {@link #onUnbind} returned {@code true}. Mirrors {@code
   * android.app.Service#onRebind} — {@code onBind} is NOT called again; the cached IBinder is
   * reused. Default no-op.
   */
  public void onRebind(Intent intent) {
    // Subclass overrides
  }

  public void onDestroy() {
    // Subclass overrides
  }

  /** Stop this Service. Equivalent to {@code Context.stopService(new Intent(thisClass))}. */
  public final native void stopSelf();

  /**
   * Stop this Service only if {@code startId} matches the most recent {@link #onStartCommand} call.
   * Mirrors {@code android.app.Service#stopSelfResult(int)}: returns {@code true} and stops if it
   * was the latest start request, {@code false} (and keeps running) if a newer start arrived — the
   * safe way to stop a Service that may have been restarted.
   */
  public final native boolean stopSelfResult(int startId);

  /**
   * Promote this Service to foreground state with a persistent notification. Picodroid renders the
   * notification as a top-of-screen banner; ID {@code id} can later be passed to {@link
   * #stopForeground} or {@link NotificationManager#cancel}.
   */
  public final native void startForeground(int id, Notification notification);

  /** Demote from foreground state. If {@code removeNotification} is true the banner is cleared. */
  public final native void stopForeground(boolean removeNotification);
}
