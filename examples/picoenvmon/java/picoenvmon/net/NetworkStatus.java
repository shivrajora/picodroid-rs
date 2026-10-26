// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.net;

/**
 * What the Network screen shows, as {@link NetworkManager#status()} publishes it: immutable, one
 * per change of the link, the clock or the weather.
 */
public final class NetworkStatus {
  /** One of {@link NetworkManager#STATE_NO_WIFI} and its siblings. */
  public final int state;

  /** Dotted-quad local address, or null while the link is down or its address unknown. */
  public final String ipAddress;

  /** Dashboard URL ("http://a.b.c.d:8080/"), or null with {@link #ipAddress}. */
  public final String url;

  /** Whether the platform's time service has anchored the wall clock this boot. */
  public final boolean timeSynced;

  /** Latest weather one-liner, or null (unavailable / not fetched yet). */
  public final String weather;

  NetworkStatus(int state, String ipAddress, String url, boolean timeSynced, String weather) {
    this.state = state;
    this.ipAddress = ipAddress;
    this.url = url;
    this.timeSynced = timeSynced;
    this.weather = weather;
  }
}
