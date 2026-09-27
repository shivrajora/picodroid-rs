// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net.wifi;

/**
 * How far a join got, mirroring the {@code android.net.wifi.SupplicantState} values picodroid can
 * tell apart. {@link #COMPLETED} means associated and keyed; the address comes a moment later and
 * is {@code ConnectivityManager}'s business. A join that failed reads {@link #DISCONNECTED} and
 * {@link WifiManager#getLastError()} says why.
 */
public enum SupplicantState {
  /** Not associated: never joined, left, or the last join failed. */
  DISCONNECTED,
  /** A join is in progress. */
  ASSOCIATING,
  /** Associated and authenticated. */
  COMPLETED;

  /** The state for the native status value (hal/wifi.rs {@code Status}). */
  static SupplicantState of(int status) {
    switch (status) {
      case WifiManager.STATUS_JOINING:
        return ASSOCIATING;
      case WifiManager.STATUS_JOINED:
        return COMPLETED;
      default:
        return DISCONNECTED;
    }
  }
}
