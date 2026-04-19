package picodroid.content.pm;

/**
 * Query compile-time board capabilities. Mirrors Android's {@code
 * PackageManager.hasSystemFeature(String)} API.
 */
public class PackageManager {
  /** Board has WiFi networking (FreeRTOS+TCP + a wireless driver). */
  public static final String FEATURE_WIFI = "picodroid.hardware.wifi";

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
