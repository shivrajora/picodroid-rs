// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net.wifi;

/**
 * One access point a scan found, mirroring {@code android.net.wifi.ScanResult}: the network's name,
 * its BSSID, its signal strength in dBm and its security as Android's {@link #capabilities} string.
 * Sorted strongest first by {@link WifiManager#getScanResults}, one per SSID.
 */
public class ScanResult {
  /** The network name, unquoted (Android quotes only {@code WifiConfiguration.SSID}). */
  public String SSID;

  /** The access point's address, {@code aa:bb:cc:dd:ee:ff}. */
  public String BSSID;

  /**
   * The security, in Android's spelling: {@code [ESS]} for an open network, {@code
   * [WPA2-PSK-CCMP][ESS]}, {@code [WPA3-SAE-CCMP][ESS]}, both for a mixed-mode access point.
   */
  public String capabilities;

  /** Signal strength in dBm. */
  public int level;

  /** The channel's centre frequency in MHz. */
  public int frequency;

  /** Accepted for Android source compatibility; always 0 (no clock stamps a scan). */
  public long timestamp;

  public ScanResult() {}

  /** Whether {@link #capabilities} says the network needs a password. */
  public boolean isSecured() {
    return capabilities != null && !"[ESS]".equals(capabilities);
  }

  /** The 2.4 GHz channel's frequency; 5 GHz channels by their formula, though the chip has none. */
  static int frequencyOf(int channel) {
    if (channel >= 1 && channel <= 13) {
      return 2407 + 5 * channel;
    }
    if (channel == 14) {
      return 2484;
    }
    return channel > 14 ? 5000 + 5 * channel : 0;
  }

  /** The capabilities string for the native security value (hal/wifi.rs {@code Security}). */
  static String capabilitiesOf(int security) {
    switch (security) {
      case 1:
        return "[WEP][ESS]";
      case 2:
        return "[WPA-PSK-TKIP][ESS]";
      case 3:
        return "[WPA2-PSK-CCMP][ESS]";
      case 4:
        return "[WPA3-SAE-CCMP][ESS]";
      case 5:
        return "[WPA2-PSK-CCMP][WPA3-SAE-CCMP][ESS]";
      default:
        return "[ESS]";
    }
  }
}
