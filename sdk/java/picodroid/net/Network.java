// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/**
 * A network the board is on, mirroring {@code android.net.Network}. picodroid has one link, so
 * there is one of these at a time; {@link ConnectivityManager#getActiveNetwork()} returns it while
 * the link is up with an address. Each time the link comes back it is a new {@code Network}, as on
 * Android, so a callback's {@code onLost} names the one its {@code onAvailable} did.
 */
public final class Network {
  /** Android's network id; the first is 100. */
  final int netId;

  Network(int netId) {
    this.netId = netId;
  }

  /**
   * The handle Android's NDK names this network by: the id in the high word over Android's marker
   * in the low one.
   */
  public long getNetworkHandle() {
    return (((long) netId) << 32) | 0xcafed00dL;
  }

  /**
   * Open a connection over this network: the board's only one, so this is {@code
   * url.openConnection()}.
   */
  public HttpURLConnection openConnection(URL url) {
    return url.openConnection();
  }

  @Override
  public boolean equals(Object o) {
    return o instanceof Network && ((Network) o).netId == netId;
  }

  @Override
  public int hashCode() {
    return netId;
  }

  @Override
  public String toString() {
    return Integer.toString(netId);
  }
}
