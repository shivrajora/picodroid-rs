// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net.wifi;

import java.util.ArrayList;
import java.util.List;
import picodroid.concurrent.Executor;
import picodroid.net.NetworkInfo;

/**
 * The board's WiFi, mirroring {@code android.net.wifi.WifiManager}: obtain it with {@code
 * getSystemService(Context.WIFI_SERVICE)}. Scan for networks, join one with a password, and keep it
 * so the device rejoins at every boot — what Settings → Wi-Fi does.
 *
 * <pre>{@code
 * WifiManager wm = (WifiManager) getSystemService(Context.WIFI_SERVICE);
 * wm.registerScanResultsCallback(Executors.mainExecutor(), new WifiManager.ScanResultsCallback() {
 *   @Override public void onScanResultsAvailable() { show(wm.getScanResults()); }
 * });
 * wm.startScan();
 * ...
 * WifiConfiguration c = new WifiConfiguration();
 * c.SSID = "\"MyAP\"";
 * c.preSharedKey = "\"secret\"";
 * wm.enableNetwork(wm.addNetwork(c), true);
 * }</pre>
 *
 * <p>What differs from Android, and why:
 *
 * <ul>
 *   <li><b>One saved network.</b> An embedded board keeps the network it is on, not a list: {@link
 *       #addNetwork} replaces the saved network and returns {@code 0}, its {@code networkId};
 *       {@link #getConfiguredNetworks} has at most one entry; {@link #removeNetwork
 *       removeNetwork(0)} forgets it. A firmware built with {@code PICODROID_WIFI_SSID} joins that
 *       network at boot whatever is saved — the configuration's {@link WifiConfiguration#status} is
 *       then {@link WifiConfiguration.Status#CURRENT} and {@link #removeNetwork} returns false.
 *   <li><b>No key-management set.</b> {@link WifiConfiguration} has no {@code
 *       allowedKeyManagement}: the join uses the security the last scan reported for that SSID —
 *       open, WPA2, WPA3 or mixed — and, for a network never scanned, WPA2/WPA3 with a password and
 *       open without one.
 *   <li><b>No broadcasts.</b> picodroid has no {@code BroadcastReceiver}, so the join's outcome is
 *       read, not pushed: {@link WifiInfo#getSupplicantState()} and {@link #getLastError()} ({@link
 *       #ERROR_AUTHENTICATING} is Android's value for the {@code EXTRA_SUPPLICANT_ERROR} extra).
 *       The link coming up with an address still arrives through {@code ConnectivityManager}. Scan
 *       completion is pushed, through {@link #registerScanResultsCallback} (Android 11).
 *   <li><b>WiFi is always on</b> where it exists: {@link #setWifiEnabled} is accepted and ignored,
 *       and {@link #isWifiEnabled} says whether this board has a WiFi link at all.
 * </ul>
 *
 * <p>Callbacks run on the executor they were registered with; the framework delivers them between
 * frames, so an app with no Activity does not receive them.
 */
public class WifiManager {
  /** {@link #getWifiState}: this board has no WiFi. */
  public static final int WIFI_STATE_DISABLED = 1;

  /** {@link #getWifiState}: WiFi is up; it always is on a board that has it. */
  public static final int WIFI_STATE_ENABLED = 3;

  /** {@link #getLastError}: the last join was rejected — a wrong password. Android's value. */
  public static final int ERROR_AUTHENTICATING = 1;

  /** {@link #getLastError}: no access point with that SSID answered. */
  public static final int ERROR_NETWORK_NOT_FOUND = 2;

  /** {@link #getLastError}: the last join failed for a reason the driver did not classify. */
  public static final int ERROR_GENERIC = 3;

  /** {@link WifiInfo#getSSID()} while not associated. Android's spelling. */
  public static final String UNKNOWN_SSID = "<unknown ssid>";

  /** The most scan callbacks an app may hold registered at once. */
  private static final int MAX_CALLBACKS = 4;

  /** A scan finished: {@link #getScanResults} has the new list. Register with an executor. */
  public abstract static class ScanResultsCallback {
    Executor executor;

    public ScanResultsCallback() {}

    public abstract void onScanResultsAvailable();
  }

  private static final WifiManager INSTANCE = new WifiManager();

  private final ScanResultsCallback[] callbacks = new ScanResultsCallback[MAX_CALLBACKS];
  private int count;

  /** The scan generation the last {@link #fireEvent} saw. */
  private int scanGenerationSeen;

  private WifiManager() {
    scanGenerationSeen = nativeScanGeneration();
  }

  public static WifiManager getInstance() {
    return INSTANCE;
  }

  /** Whether this board has a WiFi link. */
  public boolean isWifiEnabled() {
    return nativeAvailable();
  }

  /** Accepted for Android source compatibility; the radio is not switchable. Returns false. */
  public boolean setWifiEnabled(boolean enabled) {
    return false;
  }

  public int getWifiState() {
    return nativeAvailable() ? WIFI_STATE_ENABLED : WIFI_STATE_DISABLED;
  }

  /**
   * Start a scan. Returns false when the board has no WiFi or a request is already waiting for the
   * link driver; a scan already running just keeps running. The results arrive through the {@link
   * ScanResultsCallback}s a few seconds later.
   */
  public boolean startScan() {
    return nativeAvailable() && nativeStartScan();
  }

  /** The networks the last scan found, strongest first; empty before the first scan. */
  public List<ScanResult> getScanResults() {
    List<ScanResult> list = new ArrayList<ScanResult>();
    int n = nativeScanCount();
    for (int i = 0; i < n; i++) {
      ScanResult r = new ScanResult();
      r.SSID = nativeScanSsid(i);
      r.BSSID = nativeScanBssid(i);
      r.level = nativeScanRssi(i);
      r.frequency = ScanResult.frequencyOf(nativeScanChannel(i));
      r.capabilities = ScanResult.capabilitiesOf(nativeScanSecurity(i));
      list.add(r);
    }
    return list;
  }

  /**
   * Hear every scan's completion on {@code executor}. Throws {@code IllegalArgumentException} for a
   * callback already registered and {@code IllegalStateException} past {@value #MAX_CALLBACKS}.
   */
  public void registerScanResultsCallback(Executor executor, ScanResultsCallback callback) {
    if (executor == null || callback == null) {
      throw new NullPointerException();
    }
    synchronized (this) {
      for (int i = 0; i < count; i++) {
        if (callbacks[i] == callback) {
          throw new IllegalArgumentException("callback already registered");
        }
      }
      if (count == MAX_CALLBACKS) {
        throw new IllegalStateException("too many scan callbacks");
      }
      callback.executor = executor;
      callbacks[count++] = callback;
    }
  }

  public void unregisterScanResultsCallback(ScanResultsCallback callback) {
    synchronized (this) {
      for (int i = 0; i < count; i++) {
        if (callbacks[i] == callback) {
          for (int j = i; j < count - 1; j++) {
            callbacks[j] = callbacks[j + 1];
          }
          callbacks[--count] = null;
          callback.executor = null;
          return;
        }
      }
    }
  }

  /** The network the station is on, or was last asked to join. Never null. */
  public WifiInfo getConnectionInfo() {
    WifiInfo info = new WifiInfo();
    String ssid = nativeCurrentSsid();
    int status = nativeStatus();
    info.ssid = ssid.length() == 0 ? null : ssid;
    info.state = SupplicantState.of(status);
    info.rssi = ssid.length() == 0 ? WifiInfo.UNKNOWN_RSSI : rssiOf(ssid);
    info.networkId = ssid.length() > 0 && ssid.equals(nativeSavedSsid()) ? 0 : -1;
    info.ipAddress = status == STATUS_JOINED ? NetworkInfo.getIpAddress() : 0;
    return info;
  }

  /**
   * Why the last join did not complete: {@link #ERROR_AUTHENTICATING}, {@link
   * #ERROR_NETWORK_NOT_FOUND}, {@link #ERROR_GENERIC}, or 0 while it is in progress, succeeded, or
   * none was made. Read it when {@link WifiInfo#getSupplicantState()} is {@link
   * SupplicantState#DISCONNECTED} after a join.
   */
  public int getLastError() {
    switch (nativeStatus()) {
      case STATUS_BAD_AUTH:
        return ERROR_AUTHENTICATING;
      case STATUS_NO_NET:
        return ERROR_NETWORK_NOT_FOUND;
      case STATUS_FAIL:
        return ERROR_GENERIC;
      default:
        return 0;
    }
  }

  /** The saved network as a one-entry list, or an empty list when none is saved. */
  public List<WifiConfiguration> getConfiguredNetworks() {
    List<WifiConfiguration> list = new ArrayList<WifiConfiguration>();
    String ssid = nativeSavedSsid();
    if (ssid.length() > 0) {
      WifiConfiguration c = new WifiConfiguration();
      c.SSID = "\"" + ssid + "\"";
      c.networkId = 0;
      c.status =
          nativeSavedSource() == SOURCE_BUILD
              ? WifiConfiguration.Status.CURRENT
              : WifiConfiguration.Status.ENABLED;
      list.add(c);
    }
    return list;
  }

  /**
   * Save {@code config} as the device's network — it replaces whatever was saved — and return its
   * {@code networkId}, always 0; -1 when the SSID is empty or the fields are too long, or the board
   * has no WiFi. Does not connect: {@link #enableNetwork} does.
   */
  public int addNetwork(WifiConfiguration config) {
    if (config == null || config.SSID == null || !nativeAvailable()) {
      return -1;
    }
    String ssid = WifiConfiguration.unquote(config.SSID);
    String pass = config.preSharedKey == null ? "" : WifiConfiguration.unquote(config.preSharedKey);
    return nativeSave(ssid, pass) ? 0 : -1;
  }

  /** Mirrors Android: {@link #addNetwork} for an existing {@code networkId}. */
  public int updateNetwork(WifiConfiguration config) {
    return addNetwork(config);
  }

  /**
   * With {@code attemptConnect}, join the saved network now (leaving the current one first). The
   * join runs on the link driver's task; watch {@link #getConnectionInfo} or {@code
   * ConnectivityManager} for the outcome. Returns false when nothing is saved, {@code netId} is not
   * 0, or a request is already waiting.
   */
  public boolean enableNetwork(int netId, boolean attemptConnect) {
    if (netId != 0 || nativeSavedSsid().length() == 0) {
      return false;
    }
    return !attemptConnect || nativeReconnect();
  }

  /** Forget the saved network and leave it. False when it was compiled into the firmware. */
  public boolean removeNetwork(int netId) {
    if (netId != 0 || nativeSavedSource() == SOURCE_BUILD) {
      return false;
    }
    boolean forgotten = nativeForget();
    nativeDisconnect();
    return forgotten;
  }

  /** Leave the current network; the saved one is kept and rejoined at the next boot. */
  public boolean disconnect() {
    return nativeAvailable() && nativeDisconnect();
  }

  /** Join the saved network again. */
  public boolean reconnect() {
    return nativeAvailable() && nativeReconnect();
  }

  /** Mirrors Android: the same as {@link #reconnect} — there is nothing to reassociate with. */
  public boolean reassociate() {
    return reconnect();
  }

  /**
   * Mirrors Android: {@code rssi} in dBm as a level in {@code 0..numLevels-1}, linear between -100
   * and -55 dBm.
   */
  public static int calculateSignalLevel(int rssi, int numLevels) {
    if (rssi <= -100) {
      return 0;
    }
    if (rssi >= -55) {
      return numLevels - 1;
    }
    return (rssi + 100) * (numLevels - 1) / 45;
  }

  /** Mirrors Android 11: {@link #calculateSignalLevel} over five levels. */
  public int calculateSignalLevel(int rssi) {
    return calculateSignalLevel(rssi, 5);
  }

  /** Mirrors Android: whether {@code signalLevelA} is stronger than {@code signalLevelB}. */
  public static int compareSignalLevel(int rssiA, int rssiB) {
    return rssiA - rssiB;
  }

  /** The last scan's RSSI for {@code ssid}, or {@link WifiInfo#UNKNOWN_RSSI}. */
  private static int rssiOf(String ssid) {
    int n = nativeScanCount();
    for (int i = 0; i < n; i++) {
      if (ssid.equals(nativeScanSsid(i))) {
        return nativeScanRssi(i);
      }
    }
    return WifiInfo.UNKNOWN_RSSI;
  }

  /**
   * Invoked from the framework's event loop when a scan finished or the station's state changed.
   * Fans scan completion out to the registered callbacks; state is read on demand.
   */
  static void fireEvent() {
    WifiManager wm = INSTANCE;
    int generation = nativeScanGeneration();
    if (generation == wm.scanGenerationSeen) {
      return;
    }
    wm.scanGenerationSeen = generation;
    ScanResultsCallback[] snapshot;
    int n;
    synchronized (wm) {
      n = wm.count;
      snapshot = new ScanResultsCallback[n];
      System.arraycopy(wm.callbacks, 0, snapshot, 0, n);
    }
    for (int i = 0; i < n; i++) {
      final ScanResultsCallback cb = snapshot[i];
      Executor executor = cb.executor;
      if (executor != null) {
        executor.execute(() -> cb.onScanResultsAvailable());
      }
    }
  }

  // The station's state as the link driver reports it (hal/wifi.rs Status).
  static final int STATUS_DOWN = 0;
  static final int STATUS_JOINING = 1;
  static final int STATUS_JOINED = 2;
  static final int STATUS_FAIL = 3;
  static final int STATUS_NO_NET = 4;
  static final int STATUS_BAD_AUTH = 5;

  // Where the configured network came from (hal/wifi.rs Source).
  static final int SOURCE_NONE = 0;
  static final int SOURCE_BUILD = 1;
  static final int SOURCE_STORED = 2;

  private static native boolean nativeAvailable();

  private static native boolean nativeStartScan();

  private static native int nativeScanGeneration();

  private static native int nativeScanCount();

  private static native String nativeScanSsid(int index);

  private static native String nativeScanBssid(int index);

  private static native int nativeScanRssi(int index);

  private static native int nativeScanChannel(int index);

  private static native int nativeScanSecurity(int index);

  /** Save as the device's network; false when the fields are out of range or the write failed. */
  private static native boolean nativeSave(String ssid, String password);

  private static native boolean nativeForget();

  /** Join the configured network now; false when none is configured or a request is waiting. */
  private static native boolean nativeReconnect();

  private static native boolean nativeDisconnect();

  private static native int nativeStatus();

  /** The SSID the station is on or was last asked to join; empty when none. */
  private static native String nativeCurrentSsid();

  /** The configured network's SSID (build-time first, else saved); empty when none. */
  private static native String nativeSavedSsid();

  private static native int nativeSavedSource();
}
