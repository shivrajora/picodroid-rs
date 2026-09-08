// SPDX-License-Identifier: GPL-3.0-only
package quotademo;

import java.io.IOException;
import picodroid.app.Application;
import picodroid.app.usage.StorageStats;
import picodroid.app.usage.StorageStatsManager;
import picodroid.content.pm.PackageManager;
import picodroid.io.File;
import picodroid.io.FileOutputStream;
import picodroid.os.Build;
import picodroid.os.StatFs;
import picodroid.util.Log;

/**
 * Conformance checks for the storage quota (multi-app M3b): {@code StatFs} reports the volume and
 * what this app may still write, seven 16 KB files fit under the 128 KB cap beside the 8 KB package
 * directory and the eighth is refused with {@code IOException}, a delete and a {@code mkdir} move
 * the number by exactly what they cost, and {@code StorageStatsManager} agrees with the accounting.
 * {@code Build} names the board. Multi-app boards only (the cap is theirs).
 */
public class QuotaDemo extends Application {
  private static final String TAG = "QuotaDemo";

  static final long BLOCK = 4096L;
  static final long DIR = 8192L;
  static final long BLOB = 16384L;
  static final long CAP = 131072L;

  static int passed;
  static int failed;

  static void check(String name, boolean cond) {
    if (cond) {
      Log.i(TAG, "PASS: " + name);
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  @Override
  public void onCreate() {
    Log.i(TAG, "=== Quota tests ===");
    cleanup();
    testBuild();
    testStatFs();
    testCapAndStats();
    cleanup();
    Log.i(TAG, "Results: " + passed + " passed, " + failed + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== SOME FAILED ===");
    }
  }

  void testBuild() {
    Log.i(
        TAG,
        "board " + Build.BOARD + " mcu " + Build.HARDWARE + " release " + Build.VERSION.RELEASE);
    check("Build.BOARD", Build.BOARD != null && Build.BOARD.length() > 0);
    check("Build.HARDWARE", Build.HARDWARE != null && Build.HARDWARE.length() > 0);
    check(
        "Build.VERSION.RELEASE",
        Build.VERSION.RELEASE != null && Build.VERSION.RELEASE.length() > 0);
  }

  void testStatFs() {
    StatFs s = new StatFs("/");
    long total = s.getTotalBytes();
    long free = s.getFreeBytes();
    long avail = s.getAvailableBytes();
    Log.i(TAG, "volume " + total + " free " + free + " available " + avail);
    check("total is the volume", total > 0 && total % BLOCK == 0);
    check("free within total", free > 0 && free <= total);
    check("available within free", avail > 0 && avail <= free);
    check("available is the cap for an empty app", avail == CAP);
    check("block size", s.getBlockSizeLong() == BLOCK);
    check("block count", s.getBlockCountLong() == total / BLOCK);
  }

  void testCapAndStats() {
    StatFs s = new StatFs("/");
    int written = 0;
    boolean refused = false;
    byte[] blob = new byte[(int) BLOB];
    for (int i = 0; i < 16 && !refused; i++) {
      try {
        FileOutputStream out = new FileOutputStream("/blob" + i);
        out.write(blob);
        out.close();
        written++;
      } catch (IOException e) {
        refused = true;
        Log.i(TAG, "refused: " + e.getMessage());
      }
    }
    check("the cap refuses a write", refused);
    check("seven 16 KB blobs fit beside the 8 KB directory", written == 7);
    long avail = s.getAvailableBytes();
    check("available is the remainder under the cap", avail == CAP - DIR - 7 * BLOB);
    File eighth = new File("/blob7");
    check("a refused write stores nothing", !eighth.exists() || eighth.length() == 0L);

    StorageStatsManager ssm = (StorageStatsManager) getSystemService(STORAGE_STATS_SERVICE);
    try {
      StorageStats st = ssm.queryStatsForPackage(getPackageName());
      check("data bytes match the accounting", st.getDataBytes() == DIR + 7 * BLOB);
      check("app bytes are the run", st.getAppBytes() > 0 && st.getAppBytes() % BLOCK == 0);
      check("cache is empty", st.getCacheBytes() == 0L);
    } catch (PackageManager.NameNotFoundException e) {
      check("own package is installed", false);
    }
    boolean unknown = false;
    try {
      ssm.queryStatsForPackage("com.nobody");
    } catch (PackageManager.NameNotFoundException e) {
      unknown = true;
    }
    check("an unknown package throws", unknown);

    check(
        "delete frees a blob",
        new File("/blob0").delete() && s.getAvailableBytes() == avail + BLOB);
    check(
        "mkdir costs a directory pair",
        new File("/dir").mkdir() && s.getAvailableBytes() == avail + BLOB - DIR);
  }

  void cleanup() {
    for (int i = 0; i < 16; i++) {
      new File("/blob" + i).delete();
    }
    new File("/dir").delete();
  }
}
