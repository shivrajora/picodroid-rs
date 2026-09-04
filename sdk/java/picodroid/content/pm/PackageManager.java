// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content.pm;

/**
 * Query compile-time board capabilities. Mirrors Android's {@code
 * PackageManager.hasSystemFeature(String)} API.
 */
public class PackageManager {
  /** The board's network link is WiFi. */
  public static final String FEATURE_WIFI = "picodroid.hardware.wifi";

  /** The board's network link is wired Ethernet. */
  public static final String FEATURE_ETHERNET = "picodroid.hardware.ethernet";

  private static final PackageManager INSTANCE = new PackageManager();

  private PackageManager() {}

  public static PackageManager getInstance() {
    return INSTANCE;
  }

  /**
   * @return true if the board firmware was built with the given feature.
   */
  public native boolean hasSystemFeature(String name);
}
