// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

import picodroid.app.NotificationManager;
import picodroid.hardware.SensorManager;

/**
 * Common base for {@code Application} and {@code Activity}: provides component-launch APIs ({@code
 * startActivity}, {@code startService}, {@code bindService}) and system-service lookup ({@code
 * getSystemService}).
 *
 * <p>Picodroid is single-process, so cross-process Intents and the {@code android:process}
 * attribute don't exist.
 */
public class Context {
  public static final String SENSOR_SERVICE = "sensor";

  /** Name for {@link #getSystemService}: retrieves the {@link NotificationManager}. */
  public static final String NOTIFICATION_SERVICE = "notification";

  /**
   * Resolve a system service by name. Subclasses may extend; the base handles the well-known set.
   */
  public Object getSystemService(String name) {
    if (SENSOR_SERVICE.equals(name)) {
      return SensorManager.getInstance();
    }
    if (NOTIFICATION_SERVICE.equals(name)) {
      return NotificationManager.getInstance();
    }
    return null;
  }

  /**
   * Start a Service. Calls {@code onCreate} on first launch, then {@code onStartCommand} for every
   * call (including repeats). The framework owns instantiation; the target Service must have a
   * public no-arg constructor.
   */
  public final native void startService(Intent intent);

  /**
   * Stop a Service started via {@link #startService}. If the Service is also bound, it lives until
   * the last client unbinds; if neither, {@code onDestroy} runs immediately.
   */
  public final native void stopService(Intent intent);

  /**
   * Bind to a Service. Calls {@code onCreate} (first time) and {@code onBind}, then delivers the
   * returned IBinder to {@code conn.onServiceConnected}. The binding is scoped to the calling
   * Activity (or to the Application when called outside an Activity).
   */
  public final native void bindService(Intent intent, ServiceConnection conn);

  /** Drop a connection previously established via {@link #bindService}. */
  public final native void unbindService(ServiceConnection conn);
}
