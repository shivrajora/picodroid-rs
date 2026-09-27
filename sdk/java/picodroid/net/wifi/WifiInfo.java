// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net.wifi;

/**
 * The station's current network, mirroring {@code android.net.wifi.WifiInfo}: the SSID it is on (or
 * was last asked to join), how far the join got, and the signal strength the last scan saw for it.
 * Obtained from {@link WifiManager#getConnectionInfo()}; a snapshot, never updated.
 */
public class WifiInfo {
  /** {@link #getRssi} when the network was never seen in a scan. */
  public static final int UNKNOWN_RSSI = -127;

  String ssid;
  int rssi = UNKNOWN_RSSI;
  int networkId = -1;
  int ipAddress;
  SupplicantState state = SupplicantState.DISCONNECTED;

  WifiInfo() {}

  /**
   * The SSID in quotes, as Android returns it ({@code "MyAP"} with the quote characters), or {@link
   * WifiManager#UNKNOWN_SSID} while no network is current.
   */
  public String getSSID() {
    return ssid == null ? WifiManager.UNKNOWN_SSID : "\"" + ssid + "\"";
  }

  /** The access point's address, from the last scan; {@code 02:00:00:00:00:00} when unknown. */
  public String getBSSID() {
    return "02:00:00:00:00:00";
  }

  /** The last scan's signal strength for this network in dBm, or {@link #UNKNOWN_RSSI}. */
  public int getRssi() {
    return rssi;
  }

  /** The address, packed as {@code NetworkInfo.getIpAddress()} packs it; 0 until the link is up. */
  public int getIpAddress() {
    return ipAddress;
  }

  /** 0 when the current network is the saved one, else -1. */
  public int getNetworkId() {
    return networkId;
  }

  /** How far the join got. */
  public SupplicantState getSupplicantState() {
    return state;
  }

  /** Mirrors Android: the link type this network is on. */
  public int getNetworkType() {
    return picodroid.net.ConnectivityManager.TYPE_WIFI;
  }
}
