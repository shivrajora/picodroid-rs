// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net.wifi;

/**
 * A network to save, mirroring {@code android.net.wifi.WifiConfiguration}: set {@link #SSID} and,
 * for a secured network, {@link #preSharedKey}, then {@link WifiManager#addNetwork}. As on Android
 * both are quoted strings ({@code "\"MyAP\""}); an unquoted value is accepted too.
 *
 * <p>Not here: {@code allowedKeyManagement} and the other {@code BitSet}s — the security comes from
 * the last scan of that SSID, else from whether a password is set (see {@link WifiManager}).
 */
public class WifiConfiguration {
  /** {@link #status} values, as Android names them. */
  public static class Status {
    /** The network the device is on now — on picodroid, one compiled into the firmware. */
    public static final int CURRENT = 0;

    public static final int DISABLED = 1;

    /** Saved and joined at boot. */
    public static final int ENABLED = 2;

    private Status() {}
  }

  /** The network name, in quotes as Android has it; an unquoted name works as well. */
  public String SSID;

  /** Accepted for Android source compatibility; a join is by SSID. */
  public String BSSID;

  /**
   * The passphrase, in quotes as Android has it (or a 64-digit hex PSK); null or empty for open.
   */
  public String preSharedKey;

  /** 0 once saved, -1 before. */
  public int networkId = -1;

  /** One of {@link Status}. */
  public int status = Status.DISABLED;

  /** Accepted for Android source compatibility; hidden networks are not scanned for. */
  public boolean hiddenSSID;

  public WifiConfiguration() {}

  /** {@code "\"x\""} → {@code x}; anything else unchanged. */
  static String unquote(String s) {
    int n = s.length();
    if (n >= 2 && s.charAt(0) == '"' && s.charAt(n - 1) == '"') {
      return s.substring(1, n - 1);
    }
    return s;
  }
}
