// SPDX-License-Identifier: GPL-3.0-only
package settings;

import java.util.List;
import picodroid.app.Activity;
import picodroid.app.usage.StorageStats;
import picodroid.app.usage.StorageStatsManager;
import picodroid.concurrent.Executors;
import picodroid.content.pm.PackageInfo;
import picodroid.content.pm.PackageManager;
import picodroid.os.StatFs;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.TextView;

/**
 * Storage: the volume, then every package's app and data bytes. The header row goes back.
 *
 * <p>The rows land one per tick with their numbers pending. The numbers come from a pool worker —
 * a package's data walk blocks that worker, not the tick — and are posted back one row at a time,
 * so every row is touched on the UI thread only.
 */
public class StorageActivity extends Activity {
  private static final String PENDING = "  app -- / data -- KB";

  private Column column;
  private StorageStatsManager stats;
  private String[] pkgs;
  private String[] labels;
  /** Each package row's numbers, held so the rows stay reachable until they are filled in. */
  private TextView[] tails;
  private long[] appBytes;
  private long[] dataBytes;
  private boolean alive;

  @Override
  public void onCreate() {
    alive = true;
    stats = (StorageStatsManager) getSystemService(STORAGE_STATS_SERVICE);
    column = new Column(this, "< Storage", v -> finish());
    column.fill(() -> listPackages(), i -> row(i), () -> fetch());
  }

  /** The packages and their labels, one query each, on the tick before the first row. */
  private void listPackages() {
    PackageManager pm = getPackageManager();
    List<PackageInfo> all = pm.getInstalledPackages(0);
    int n = all.size();
    pkgs = new String[n];
    labels = new String[n];
    tails = new TextView[n];
    appBytes = new long[n];
    dataBytes = new long[n];
    for (int i = 0; i < n; i++) {
      PackageInfo info = all.get(i);
      pkgs[i] = info.packageName;
      labels[i] = pm.getApplicationLabel(info.applicationInfo).toString();
    }
  }

  /** Row 0 is the volume; then one row per package, its numbers pending. */
  private View row(int i) {
    if (i == 0) {
      StatFs fs = new StatFs("/");
      long total = fs.getTotalBytes();
      long free = fs.getFreeBytes();
      return Screens.info(
          this, "Volume  " + Screens.kb(total - free) + " / " + Screens.kb(total) + " KB used");
    }
    int p = i - 1;
    if (p >= pkgs.length) {
      return null;
    }
    tails[p] = Screens.tail(this, PENDING);
    return Screens.info(this, labels[p], tails[p]);
  }

  /** The numbers, off the UI thread: the data walks block a pool worker, not the tick. */
  private void fetch() {
    Executors.backgroundExecutor().execute(() -> fetchAll());
  }

  private void fetchAll() {
    for (int i = 0; i < pkgs.length && alive; i++) {
      final int row = i;
      try {
        StorageStats st = stats.queryStatsForPackage(pkgs[i]);
        appBytes[i] = st.getAppBytes();
        dataBytes[i] = st.getDataBytes();
      } catch (PackageManager.NameNotFoundException e) {
        appBytes[i] = -1; // uninstalled between the list and the query
      }
      Executors.mainExecutor().execute(() -> show(row));
    }
  }

  /** On the UI thread: the only place a row is touched. */
  private void show(int i) {
    if (!alive) {
      return;
    }
    if (appBytes[i] < 0) {
      tails[i].setText("  not installed");
      return;
    }
    tails[i].setText(
        "  app " + Screens.kb(appBytes[i]) + " / data " + Screens.kb(dataBytes[i]) + " KB");
    Log.i(SettingsActivity.TAG, "storage " + pkgs[i] + " " + appBytes[i] + " " + dataBytes[i]);
  }

  @Override
  public void onDestroy() {
    alive = false;
    column.stop();
    super.onDestroy();
  }
}
