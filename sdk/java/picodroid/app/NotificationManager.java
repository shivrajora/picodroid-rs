// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

/**
 * Posts and cancels {@link Notification}s. v1 routes every notification to a single persistent
 * on-screen banner managed by the framework.
 *
 * <p>Apps don't usually call {@link #notify} or {@link #cancel} directly — {@link
 * Service#startForeground} and {@link Service#stopForeground} drive notifications during their
 * lifetimes. The methods are exposed for parity with Android.
 */
public final class NotificationManager {
  private static final NotificationManager INSTANCE = new NotificationManager();

  private NotificationManager() {}

  public static NotificationManager getInstance() {
    return INSTANCE;
  }

  public native void notify(int id, Notification notification);

  public native void cancel(int id);
}
