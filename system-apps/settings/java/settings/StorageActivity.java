// SPDX-License-Identifier: GPL-3.0-only
package settings;

import java.util.List;
import picodroid.app.Activity;
import picodroid.app.usage.StorageStats;
import picodroid.app.usage.StorageStatsManager;
import picodroid.content.pm.PackageInfo;
import picodroid.content.pm.PackageManager;
import picodroid.os.StatFs;
import picodroid.util.Log;
import picodroid.widget.LinearLayout;

/** Storage: the volume, then every package's app and data bytes. The header row goes back. */
public class StorageActivity extends Activity {
  @Override
  public void onCreate() {
    LinearLayout root = Screens.column(this);
    root.addView(Screens.header(this, "< Storage", v -> finish()));
    StatFs fs = new StatFs("/");
    long total = fs.getTotalBytes();
    long free = fs.getFreeBytes();
    root.addView(
        Screens.info(
            this, "Volume  " + Screens.kb(total - free) + " / " + Screens.kb(total) + " KB used"));
    int rows = 2;
    StorageStatsManager ssm = (StorageStatsManager) getSystemService(STORAGE_STATS_SERVICE);
    PackageManager pm = getPackageManager();
    List<PackageInfo> all = pm.getInstalledPackages(0);
    for (int i = 0; i < all.size(); i++) {
      PackageInfo info = all.get(i);
      String label = pm.getApplicationLabel(info.applicationInfo).toString();
      try {
        StorageStats st = ssm.queryStatsForPackage(info.packageName);
        // The numbers always show; a long label is cut instead.
        String numbers =
            "  app " + Screens.kb(st.getAppBytes()) + " / data " + Screens.kb(st.getDataBytes())
                + " KB";
        root.addView(Screens.info(this, Screens.fit(this, label, numbers)));
        rows++;
        Log.i(
            SettingsActivity.TAG,
            "storage " + info.packageName + " " + st.getAppBytes() + " " + st.getDataBytes());
      } catch (PackageManager.NameNotFoundException e) {
        // Uninstalled between the query and the stats: skip the row.
      }
    }
    setContentView(Screens.scrollable(this, root, rows));
  }
}
