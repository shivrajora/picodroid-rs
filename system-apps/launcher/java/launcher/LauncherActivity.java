// SPDX-License-Identifier: GPL-3.0-only
package launcher;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import picodroid.activity.OnBackPressedCallback;
import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.content.pm.ApplicationInfo;
import picodroid.content.pm.PackageInfo;
import picodroid.content.pm.PackageManager;
import picodroid.graphics.Color;
import picodroid.graphics.drawable.Drawable;
import picodroid.launcher.R;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.ImageView;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.TextView;

/**
 * The home screen of a multi-app board: one row per installed app, icon and label. Tap a row, or
 * move to it with the up and down buttons and press select, to start that app. When the app
 * finishes, the launcher comes back.
 *
 * <p>Installed apps come first, sorted by label, then the other system apps; the launcher never
 * lists itself. Rows are 40 pixels tall ({@code res/layout/row.xml}) from the top of the screen, so
 * the bench taps the middle of row 0 at (120, 20); more rows than the screen holds scroll — by drag
 * on a touch panel, and with the focus on a keypad.
 *
 * <p>The rows are inflated from {@code res/layout/row.xml} inside {@code onCreate}, as an Android
 * launcher builds its list; the main loop allows a screen build a slow first frame
 * (docs/designs/ui-build-budget-2026-10.md). "ready" is logged once the last row is in.
 */
public class LauncherActivity extends Activity {
  private static final String TAG = "Launcher";

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
    // Home: there is nothing to go back to, so BACK stays here.
    getOnBackPressedDispatcher()
        .addCallback(
            this,
            new OnBackPressedCallback(true) {
              @Override
              public void handleOnBackPressed() {}
            });

    // A LinearLayout does not scroll (as on Android): the column sits in a ScrollView, the
    // content view's root, which fills the window; the column takes its width and wraps its rows,
    // so a long list is reachable on every board.
    root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(0, 0, 0, 0);
    ScrollView scroller = new ScrollView(this);
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
      TextView empty = new TextView(this);
      empty.setText("No apps installed. Use pdb install.");
      empty.setTextColor(Color.WHITE);
      root.addView(empty);
    } else {
      rows[0].requestFocus();
    }
    Log.i(TAG, "ready: " + n + " apps");
  }

  /** The packages in display order: installed apps by label, then the other system apps. */
  private void list() {
    PackageManager pm = getPackageManager();
    String self = getPackageName();
    List<PackageInfo> installed = pm.getInstalledPackages(0);
    // Label order, case-insensitively as Android sorts app names; each label fetched once.
    List<Entry> sorted = new ArrayList<>();
    for (int i = 0; i < installed.size(); i++) {
      PackageInfo info = installed.get(i);
      sorted.add(new Entry(info, pm.getApplicationLabel(info.applicationInfo).toString()));
    }
    Collections.sort(sorted, (a, b) -> a.key.compareTo(b.key));
    order = new PackageInfo[sorted.size()];
    labels = new String[sorted.size()];
    int n = 0;
    // Pass 0 lists the installed apps, pass 1 the other system apps.
    for (int pass = 0; pass < 2; pass++) {
      for (int i = 0; i < sorted.size(); i++) {
        Entry e = sorted.get(i);
        boolean system = (e.info.applicationInfo.flags & ApplicationInfo.FLAG_SYSTEM) != 0;
        if (system != (pass == 1) || e.info.packageName.equals(self)) {
          continue;
        }
        order[n] = e.info;
        labels[n] = e.label;
        n++;
      }
    }
    rows = new View[n];
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
}
