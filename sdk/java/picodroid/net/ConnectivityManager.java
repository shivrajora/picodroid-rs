// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/**
 * Network type constants, mirroring {@code android.net.ConnectivityManager}. The values are
 * Android's. picodroid has one link per board, so there is no instance and no {@code
 * getActiveNetworkInfo()}: ask {@link NetworkInfo#getType()} directly.
 */
public final class ConnectivityManager {
  /** No network on this board ({@link NetworkInfo#getType()} on a board without one). */
  public static final int TYPE_NONE = -1;

  /** A WiFi link. */
  public static final int TYPE_WIFI = 1;

  /** A wired Ethernet link. */
  public static final int TYPE_ETHERNET = 9;

  private ConnectivityManager() {}
}
