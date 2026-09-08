// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app.usage;

import picodroid.content.pm.PackageManager;

/**
 * Storage used per package, mirroring {@code android.app.usage.StorageStatsManager}: obtain it with
 * {@code getSystemService(Context.STORAGE_STATS_SERVICE)}. Picodroid has one volume and one user,
 * so {@link #queryStatsForPackage} takes the package name alone. Multi-app boards only.
 */
public class StorageStatsManager {
  private static final StorageStatsManager INSTANCE = new StorageStatsManager();

  private StorageStatsManager() {}

  public static StorageStatsManager getInstance() {
    return INSTANCE;
  }

  /**
   * The app's own size (its installed image) and the size of its data directory, in bytes; a
   * package that is not installed throws.
   */
  public StorageStats queryStatsForPackage(String packageName)
      throws PackageManager.NameNotFoundException {
    long app = nativeAppBytes(packageName);
    if (app < 0) {
      throw new PackageManager.NameNotFoundException(packageName);
    }
    return new StorageStats(app, nativeDataBytes(packageName));
  }

  /** The run's bytes in the app region (sectors × 4 KB), or -1 when not installed. */
  private static native long nativeAppBytes(String packageName);

  /** What the package's data directory occupies, by the storage cap's accounting. */
  private static native long nativeDataBytes(String packageName);
}
