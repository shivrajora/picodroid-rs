// SPDX-License-Identifier: GPL-3.0-only
package settings;

import java.util.List;
import picodroid.app.Activity;
import picodroid.app.AlertDialog;
import picodroid.concurrent.Executors;
import picodroid.content.pm.ApplicationInfo;
import picodroid.content.pm.PackageInfo;
import picodroid.content.pm.PackageManager;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.LinearLayout;

/**
 * Apps: one row per installed app (system apps stay out; they cannot be uninstalled). A tap on a
 * row opens an {@code AlertDialog} — the app's label, "Remove app and data?", Uninstall / Cancel —
 * and Uninstall removes it through {@code PackageInstaller}, then the list is rebuilt. The bench
 * taps the first app at y = 60 and the dialog's positive button at the coordinates its screenshot
 * pinned.
 */
public class AppsActivity extends Activity {
  /** Held so the rows stay reachable while their click listeners are live. */
  private View[] rows;

  @Override
  public void onCreate() {
    render();
  }

  private void render() {
    LinearLayout root = Screens.column(this);
    root.addView(Screens.header(this, "< Apps", v -> finish()));
    PackageManager pm = getPackageManager();
    PackageInfo[] all = sortedByLabel(pm, pm.getInstalledPackages(0));
    rows = new View[all.length];
    int n = 0;
    for (int i = 0; i < all.length; i++) {
      PackageInfo info = all[i];
      if ((info.applicationInfo.flags & ApplicationInfo.FLAG_SYSTEM) != 0) {
        continue;
      }
      final String pkg = info.packageName;
      final String label = pm.getApplicationLabel(info.applicationInfo).toString();
      View row =
          Screens.row(
              this, Screens.fit(this, label, "  v" + info.versionName), v -> confirm(pkg, label));
      root.addView(row);
      rows[n] = row;
      n++;
    }
    if (n == 0) {
      root.addView(Screens.info(this, "No apps installed"));
    } else {
      rows[0].requestFocus();
    }
    setContentView(Screens.scrollable(this, root, 1 + (n == 0 ? 1 : n)));
    Log.i(SettingsActivity.TAG, "apps " + n);
  }

  /** The packages in label order, case-insensitively, as the launcher lists them. */
  private static PackageInfo[] sortedByLabel(PackageManager pm, List<PackageInfo> installed) {
    PackageInfo[] out = new PackageInfo[installed.size()];
    String[] labels = new String[installed.size()];
    for (int i = 0; i < installed.size(); i++) {
      PackageInfo info = installed.get(i);
      String label = pm.getApplicationLabel(info.applicationInfo).toString().toLowerCase();
      int j = i;
      while (j > 0 && labels[j - 1].compareTo(label) > 0) {
        out[j] = out[j - 1];
        labels[j] = labels[j - 1];
        j--;
      }
      out[j] = info;
      labels[j] = label;
    }
    return out;
  }

  private void confirm(final String pkg, String label) {
    new AlertDialog.Builder(this)
        .setTitle(label)
        .setMessage("Remove app and data?")
        .setPositiveButton("Uninstall", (dialog, which) -> uninstall(pkg))
        .setNegativeButton("Cancel", null)
        .show();
  }

  private void uninstall(String pkg) {
    try {
      getPackageManager().getPackageInstaller().uninstall(pkg);
      Log.i(SettingsActivity.TAG, "uninstalled " + pkg);
    } catch (IllegalArgumentException e) {
      Log.i(SettingsActivity.TAG, "uninstall refused: " + e.getMessage());
    } catch (IllegalStateException e) {
      Log.i(SettingsActivity.TAG, "uninstall failed: " + e.getMessage());
    }
    // Rebuild once the dialog has closed, not from inside its click.
    Executors.mainExecutor().execute(() -> render());
  }
}
