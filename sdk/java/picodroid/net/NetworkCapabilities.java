// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/**
 * What a {@link Network} can do, mirroring {@code android.net.NetworkCapabilities}: which transport
 * carries it and which capabilities it has. The constants are Android's values.
 *
 * <p>The board's link has one transport — {@link #TRANSPORT_WIFI} or {@link #TRANSPORT_ETHERNET},
 * as {@link NetworkInfo#getType()} says — and, once up with an address, the capabilities a home
 * network shows on Android: {@link #NET_CAPABILITY_INTERNET}, {@link #NET_CAPABILITY_VALIDATED},
 * {@link #NET_CAPABILITY_NOT_METERED} and the {@code NOT_*} set. picodroid does not probe the
 * internet: {@code VALIDATED} means the link is up with an address, not that a server answered.
 */
public final class NetworkCapabilities {
  public static final int TRANSPORT_CELLULAR = 0;
  public static final int TRANSPORT_WIFI = 1;
  public static final int TRANSPORT_BLUETOOTH = 2;
  public static final int TRANSPORT_ETHERNET = 3;
  public static final int TRANSPORT_VPN = 4;

  public static final int NET_CAPABILITY_NOT_METERED = 11;
  public static final int NET_CAPABILITY_INTERNET = 12;
  public static final int NET_CAPABILITY_NOT_RESTRICTED = 13;
  public static final int NET_CAPABILITY_TRUSTED = 14;
  public static final int NET_CAPABILITY_NOT_VPN = 15;
  public static final int NET_CAPABILITY_VALIDATED = 16;
  public static final int NET_CAPABILITY_NOT_ROAMING = 18;
  public static final int NET_CAPABILITY_NOT_CONGESTED = 20;
  public static final int NET_CAPABILITY_NOT_SUSPENDED = 21;

  /** Bit {@code TRANSPORT_*} set for each transport. */
  final int transports;

  /** Bit {@code NET_CAPABILITY_*} set for each capability. */
  final int capabilities;

  NetworkCapabilities(int transports, int capabilities) {
    this.transports = transports;
    this.capabilities = capabilities;
  }

  /** The board's link, up with an address. */
  static NetworkCapabilities ofLink() {
    int transports = 0;
    int type = NetworkInfo.getType();
    if (type == ConnectivityManager.TYPE_WIFI) {
      transports = 1 << TRANSPORT_WIFI;
    } else if (type == ConnectivityManager.TYPE_ETHERNET) {
      transports = 1 << TRANSPORT_ETHERNET;
    }
    int capabilities =
        (1 << NET_CAPABILITY_NOT_METERED)
            | (1 << NET_CAPABILITY_INTERNET)
            | (1 << NET_CAPABILITY_NOT_RESTRICTED)
            | (1 << NET_CAPABILITY_TRUSTED)
            | (1 << NET_CAPABILITY_NOT_VPN)
            | (1 << NET_CAPABILITY_VALIDATED)
            | (1 << NET_CAPABILITY_NOT_ROAMING)
            | (1 << NET_CAPABILITY_NOT_CONGESTED)
            | (1 << NET_CAPABILITY_NOT_SUSPENDED);
    return new NetworkCapabilities(transports, capabilities);
  }

  /** Whether {@code transportType} (a {@code TRANSPORT_*} value) carries this network. */
  public boolean hasTransport(int transportType) {
    return transportType >= 0 && transportType < 32 && (transports & (1 << transportType)) != 0;
  }

  /** Whether this network has {@code capability} (a {@code NET_CAPABILITY_*} value). */
  public boolean hasCapability(int capability) {
    return capability >= 0 && capability < 32 && (capabilities & (1 << capability)) != 0;
  }
}
