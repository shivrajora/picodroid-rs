// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app.usage;

/** One package's storage, from {@link StorageStatsManager#queryStatsForPackage}. */
public final class StorageStats {
  private final long appBytes;
  private final long dataBytes;

  StorageStats(long appBytes, long dataBytes) {
    this.appBytes = appBytes;
    this.dataBytes = dataBytes;
  }

  /** The installed image: its run in the app region, meta sector included. */
  public long getAppBytes() {
    return appBytes;
  }

  /** The package's data directory, counted the way the storage cap counts it. */
  public long getDataBytes() {
    return dataBytes;
  }

  /** Picodroid keeps no cache directory. */
  public long getCacheBytes() {
    return 0L;
  }
}
