// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/** Network status queries. */
public class NetworkInfo {
  private NetworkInfo() {}

  /** Returns true if the IP stack is up and an address has been assigned. */
  public static native boolean isConnected();

  /** Returns the local IPv4 address as a packed 32-bit integer. */
  public static native int getIpAddress();

  /**
   * The board's link kind as a {@link ConnectivityManager}{@code .TYPE_*} value: {@code TYPE_WIFI},
   * {@code TYPE_ETHERNET}, or {@code TYPE_NONE} on a board without networking. A build-time fact
   * about the board, so it never changes while the app runs.
   */
  public static native int getType();
}
