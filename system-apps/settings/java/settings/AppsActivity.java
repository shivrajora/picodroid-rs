// SPDX-License-Identifier: GPL-3.0-only
package settings;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import picodroid.app.Activity;
import picodroid.app.AlertDialog;
import picodroid.content.pm.ApplicationInfo;
import picodroid.content.pm.PackageInfo;
import picodroid.content.pm.PackageManager;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.LinearLayout;

/**
 * Apps: one row per installed app (system apps stay out; they cannot be uninstalled). A tap on a
 * row opens an {@code AlertDialog} — the app's label, "Remove app and data?", Uninstall / Cancel —
 * and Uninstall removes it through {@code PackageInstaller}, then the list is rebuilt. The bench
 * taps the first app at y = 60 and the dialog's positive button at the coordinates its screenshot
 * pinned.
 */
public class AppsActivity extends Activity {
  /** The installed apps in label order, and their labels, fetched once. */
  private PackageInfo[] apps;

  private String[] labels;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    render();
  }

  /** The screen from scratch: a new column, the rows, and the "apps N" line. */
  private void render() {
    LinearLayout column = Screens.screen(this, "< Apps", v -> finish());
    list();
    if (apps.length == 0) {
      Screens.info(this, column, "No apps installed");
    }
    for (int i = 0; i < apps.length; i++) {
      final String pkg = apps[i].packageName;
      final String label = labels[i];
      Screens.row(this, column, label, "  v" + apps[i].versionName, v -> confirm(pkg, label));
    }
    done(column);
  }

  /**
   * The installed, non-system packages in label order — case-insensitively, as the launcher lists
   * them — with one label fetch per package.
   */
  private void list() {
    PackageManager pm = getPackageManager();
    List<PackageInfo> installed = pm.getInstalledPackages(0);
    List<Entry> entries = new ArrayList<>();
    for (int i = 0; i < installed.size(); i++) {
      PackageInfo info = installed.get(i);
      if ((info.applicationInfo.flags & ApplicationInfo.FLAG_SYSTEM) != 0) {
        continue;
      }
      entries.add(new Entry(info, pm.getApplicationLabel(info.applicationInfo).toString()));
    }
    Collections.sort(entries, (a, b) -> a.key.compareTo(b.key));
    apps = new PackageInfo[entries.size()];
    labels = new String[entries.size()];
    for (int i = 0; i < entries.size(); i++) {
      apps[i] = entries.get(i).info;
      labels[i] = entries.get(i).label;
    }
  }

  /** A package, its label, and the label's sort key. */
  private static final class Entry {
    final PackageInfo info;
    final String label;
    final String key;

    Entry(PackageInfo info, String label) {
      this.info = info;
      this.label = label;
      this.key = label.toLowerCase();
    }
  }

  /** The rows are in: focus the first, and say how many — the harness keys on this line. */
  private void done(LinearLayout column) {
    column.getChildAt(1).requestFocus(); // child 0 is the header; an info row is focusable too
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
    getMainExecutor().execute(() -> render());
  }
}
