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
    List<PackageInfo> all = pm.getInstalledPackages(0);
    rows = new View[all.size()];
    int n = 0;
    for (int i = 0; i < all.size(); i++) {
      PackageInfo info = all.get(i);
      if ((info.applicationInfo.flags & ApplicationInfo.FLAG_SYSTEM) != 0) {
        continue;
      }
      final String pkg = info.packageName;
      final String label = pm.getApplicationLabel(info.applicationInfo).toString();
      View row = Screens.row(this, label + "  v" + info.versionName, v -> confirm(pkg, label));
      root.addView(row);
      rows[n] = row;
      n++;
    }
    if (n == 0) {
      root.addView(Screens.text(this, "No apps installed"));
    } else {
      rows[0].requestFocus();
    }
    setContentView(root);
    Log.i(SettingsActivity.TAG, "apps " + n);
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
