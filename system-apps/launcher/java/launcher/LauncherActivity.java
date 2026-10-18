// SPDX-License-Identifier: GPL-3.0-only
package launcher;

import java.util.List;
import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.content.pm.ApplicationInfo;
import picodroid.content.pm.PackageInfo;
import picodroid.content.pm.PackageManager;
import picodroid.graphics.Color;
import picodroid.graphics.drawable.Drawable;
import picodroid.launcher.R;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.ImageView;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.TextView;
import picodroid.os.Bundle;

/**
 * The home screen of a multi-app board: one row per installed app, icon and label. Tap a row, or
 * move to it with the up and down buttons and press select, to start that app. When the app
 * finishes, the launcher comes back.
 *
 * <p>Installed apps come first, sorted by label, then the other system apps; the launcher never
 * lists itself. Rows are {@link #ROW_HEIGHT} pixels tall from the top of the screen, so a test can
 * tap row 0 at a known point; more rows than the screen holds scroll — by drag on a touch panel,
 * and with the focus on a keypad.
 *
 * <p>The rows are inflated from {@code res/layout/row.xml} inside {@code onCreate}, as an Android
 * launcher builds its list; the main loop allows a screen build a slow first frame
 * (docs/designs/ui-build-budget-2026-10.md). "ready" is logged once the last row is in.
 */
public class LauncherActivity extends Activity {
  private static final String TAG = "Launcher";

  /** Row height in pixels: the bench taps the middle of row 0 at (120, 20). */
  private static final int ROW_HEIGHT = 40;

  private LinearLayout root;

  /** The rows in display order; row 0 takes the focus once they are in. */
  private View[] rows;

  /** The packages in display order, and their labels, fetched once. */
  private PackageInfo[] order;

  private String[] labels;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    // Not "onCreate": a --shrink image must not spell a served member name, and
    // check-shrunk-image.sh cannot tell a log literal from a leaked one.
    Log.i(TAG, "creating");
    int width = getDisplay().getWidth();
    int height = getDisplay().getHeight();

    // A LinearLayout does not scroll (as on Android): the column sits in a ScrollView and is
    // sized to its rows once they are all in, so a long list is reachable on every board.
    root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(0, 0, 0, 0);
    root.setSpacing(0);
    root.setSize(width, height);
    ScrollView scroller = new ScrollView();
    scroller.setSize(width, height);
    scroller.setPadding(0, 0, 0, 0);
    scroller.addView(root);
    setContentView(scroller);
    list();
    PackageManager pm = getPackageManager();
    for (int i = 0; i < rows.length; i++) {
      rows[i] = makeRow(pm, order[i], labels[i]);
      root.addView(rows[i]);
    }
    int n = rows.length;
    if (n == 0) {
      TextView empty = new TextView();
      empty.setText("No apps installed. Use pdb install.");
      empty.setTextColor(Color.WHITE);
      root.addView(empty);
    } else {
      rows[0].requestFocus();
    }
    int contentHeight = (n == 0 ? 1 : n) * ROW_HEIGHT;
    root.setSize(width, contentHeight > height ? contentHeight : height);
    Log.i(TAG, "ready: " + n + " apps");
  }

  /** The packages in display order: installed apps by label, then the other system apps. */
  private void list() {
    PackageManager pm = getPackageManager();
    String self = getPackageName();
    List<PackageInfo> installed = pm.getInstalledPackages(0);
    String[] sortedLabels = new String[installed.size()];
    PackageInfo[] sorted = sortedByLabel(pm, installed, sortedLabels);
    order = new PackageInfo[sorted.length];
    labels = new String[sorted.length];
    int n = 0;
    // Pass 0 lists the installed apps, pass 1 the other system apps.
    for (int pass = 0; pass < 2; pass++) {
      for (int i = 0; i < sorted.length; i++) {
        PackageInfo info = sorted[i];
        boolean system = (info.applicationInfo.flags & ApplicationInfo.FLAG_SYSTEM) != 0;
        if (system != (pass == 1) || info.packageName.equals(self)) {
          continue;
        }
        order[n] = info;
        labels[n] = sortedLabels[i];
        n++;
      }
    }
    rows = new View[n];
  }

  /**
   * The packages in label order, case-insensitively as Android sorts app names (a dozen at most, so
   * an insertion sort); {@code labelsOut} gets the labels in the same order, fetched once each.
   */
  private static PackageInfo[] sortedByLabel(
      PackageManager pm, List<PackageInfo> installed, String[] labelsOut) {
    PackageInfo[] out = new PackageInfo[installed.size()];
    String[] keys = new String[installed.size()];
    for (int i = 0; i < installed.size(); i++) {
      PackageInfo info = installed.get(i);
      String label = pm.getApplicationLabel(info.applicationInfo).toString();
      String key = label.toLowerCase();
      int j = i;
      while (j > 0 && keys[j - 1].compareTo(key) > 0) {
        out[j] = out[j - 1];
        labelsOut[j] = labelsOut[j - 1];
        keys[j] = keys[j - 1];
        j--;
      }
      out[j] = info;
      labelsOut[j] = label;
      keys[j] = key;
    }
    return out;
  }

  /**
   * A row: the app's icon (or its first letter on a tile) and its label, from {@code row.xml} or
   * {@code row_tile.xml}; the label is cut with an ellipsis rather than wrapped over the next row.
   */
  private View makeRow(PackageManager pm, PackageInfo info, String label) {
    final String pkg = info.packageName;
    Drawable icon = pm.getApplicationIcon(info.applicationInfo);
    View row = getLayoutInflater().inflate(icon != null ? R.layout.row : R.layout.row_tile, null);
    if (icon != null) {
      ImageView image = row.findViewById(R.id.icon);
      image.setImageDrawable(icon);
    } else {
      TextView tile = row.findViewById(R.id.tile);
      tile.setText(label.length() > 0 ? label.substring(0, 1) : "?");
    }
    TextView text = row.findViewById(R.id.label);
    text.setText(label);
    row.setOnClickListener(
        v -> {
          Log.i(TAG, "launch " + pkg);
          startActivity(new Intent().setPackage(pkg));
        });
    return row;
  }

  @Override
  public void onBackPressed() {
    // Home: there is nothing to go back to.
  }
}
