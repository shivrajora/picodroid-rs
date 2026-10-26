// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import picodroid.annotation.Hide;

/**
 * The link's state as the framework reads it ({@link ConnectivityManager}, {@code WifiManager}, the
 * connectivity broadcasts). Every member is {@link Hide}: an app asks {@link ConnectivityManager}
 * ({@code getActiveNetwork}, a {@code NetworkCallback}), as on Android.
 */
public class NetworkInfo {
  private NetworkInfo() {}

  /** Returns true if the IP stack is up and an address has been assigned. */
  @Hide("connectivityManager.getActiveNetwork() != null, or registerDefaultNetworkCallback")
  public static native boolean isConnected();

  /** Returns the local IPv4 address as a packed 32-bit integer. */
  @Hide("wifiManager.getConnectionInfo().getIpAddress()")
  public static native int getIpAddress();

  /**
   * The board's link kind as a {@link ConnectivityManager}{@code .TYPE_*} value: {@code TYPE_WIFI},
   * {@code TYPE_ETHERNET}, or {@code TYPE_NONE} on a board without networking. A build-time fact
   * about the board, so it never changes while the app runs.
   */
  @Hide(
      "connectivityManager.getNetworkCapabilities(network).hasTransport(NetworkCapabilities"
          + ".TRANSPORT_WIFI)")
  public static native int getType();
}
