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

/**
 * Apps: one row per installed app (system apps stay out; they cannot be uninstalled). A tap on a
 * row opens an {@code AlertDialog} — the app's label, "Remove app and data?", Uninstall / Cancel —
 * and Uninstall removes it through {@code PackageInstaller}, then the list is rebuilt. The bench
 * taps the first app at y = 60 and the dialog's positive button at the coordinates its screenshot
 * pinned.
 */
public class AppsActivity extends Activity {
  private Column column;
  /** Held so the rows stay reachable while their click listeners are live. */
  private View[] rows;
  /** The installed apps in label order, and their labels, fetched once. */
  private PackageInfo[] apps;
  private String[] labels;

  @Override
  public void onCreate() {
    render();
  }

  /** The screen from scratch: a new column, then the rows one per tick. */
  private void render() {
    if (column != null) {
      column.stop();
    }
    column = new Column(this, "< Apps", v -> finish());
    column.fill(() -> list(), i -> row(i), () -> done());
  }

  /**
   * The installed, non-system packages in label order — case-insensitively, as the launcher lists
   * them (a dozen at most, so an insertion sort) — with one label fetch per package.
   */
  private void list() {
    PackageManager pm = getPackageManager();
    List<PackageInfo> installed = pm.getInstalledPackages(0);
    PackageInfo[] sorted = new PackageInfo[installed.size()];
    String[] sortedLabels = new String[installed.size()];
    String[] keys = new String[installed.size()];
    int n = 0;
    for (int i = 0; i < installed.size(); i++) {
      PackageInfo info = installed.get(i);
      if ((info.applicationInfo.flags & ApplicationInfo.FLAG_SYSTEM) != 0) {
        continue;
      }
      String label = pm.getApplicationLabel(info.applicationInfo).toString();
      String key = label.toLowerCase();
      int j = n;
      while (j > 0 && keys[j - 1].compareTo(key) > 0) {
        sorted[j] = sorted[j - 1];
        sortedLabels[j] = sortedLabels[j - 1];
        keys[j] = keys[j - 1];
        j--;
      }
      sorted[j] = info;
      sortedLabels[j] = label;
      keys[j] = key;
      n++;
    }
    apps = new PackageInfo[n];
    labels = new String[n];
    rows = new View[n];
    for (int i = 0; i < n; i++) {
      apps[i] = sorted[i];
      labels[i] = sortedLabels[i];
    }
  }

  /** One row per app; "No apps installed" when there is none. */
  private View row(int i) {
    if (apps.length == 0) {
      return i == 0 ? Screens.info(this, "No apps installed") : null;
    }
    if (i >= apps.length) {
      return null;
    }
    final String pkg = apps[i].packageName;
    final String label = labels[i];
    rows[i] = Screens.row(this, label, "  v" + apps[i].versionName, v -> confirm(pkg, label));
    return rows[i];
  }

  /** The rows are in: focus the first, and say how many — the harness keys on this line. */
  private void done() {
    if (apps.length > 0) {
      rows[0].requestFocus();
    }
    Log.i(SettingsActivity.TAG, "apps " + apps.length);
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

  @Override
  public void onDestroy() {
    column.stop();
    super.onDestroy();
  }
}
