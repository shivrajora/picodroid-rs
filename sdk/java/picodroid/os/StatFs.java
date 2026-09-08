// SPDX-License-Identifier: GPL-3.0-only
package picodroid.os;

/**
 * Space on the storage volume, mirroring {@code android.os.StatFs}. Picodroid has one volume, so
 * the path is accepted and ignored. {@link #getAvailableBytes} is what this app may still write: on
 * a multi-app board the free space above the system reserve, capped by the app's storage cap
 * ({@code app_data_cap_kb}); on a single-app board simply the free space. {@link #getFreeBytes} is
 * the volume's raw free space either way.
 */
public class StatFs {
  public StatFs(String path) {}

  /**
   * Re-read the numbers; every getter reads them fresh, so this is a no-op kept for source
   * compatibility.
   */
  public void restat(String path) {}

  /** Size of the volume. */
  public long getTotalBytes() {
    return nativeTotalBytes();
  }

  /** Bytes not yet allocated on the volume, the reserve included. */
  public long getFreeBytes() {
    return nativeFreeBytes();
  }

  /** Bytes this app may still write. */
  public long getAvailableBytes() {
    return nativeAvailableBytes();
  }

  /** LittleFS block size: 4 KB. */
  public long getBlockSizeLong() {
    return 4096L;
  }

  public long getBlockCountLong() {
    return getTotalBytes() / 4096L;
  }

  public long getFreeBlocksLong() {
    return getFreeBytes() / 4096L;
  }

  public long getAvailableBlocksLong() {
    return getAvailableBytes() / 4096L;
  }

  private static native long nativeTotalBytes();

  private static native long nativeFreeBytes();

  private static native long nativeAvailableBytes();
}
