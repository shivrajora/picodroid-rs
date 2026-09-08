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
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.ImageView;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * The home screen of a multi-app board: one row per installed app, icon and label. Tap a row, or
 * move to it with the up and down buttons and press select, to start that app. When the app
 * finishes, the launcher comes back.
 *
 * <p>Installed apps come first, sorted by label, then the other system apps; the launcher never
 * lists itself. Rows are {@link #ROW_HEIGHT} pixels tall from the top of the screen, so a test can
 * tap row 0 at a known point.
 */
public class LauncherActivity extends Activity {
  private static final String TAG = "Launcher";

  /** Row height in pixels: the bench taps the middle of row 0 at (120, 20). */
  private static final int ROW_HEIGHT = 40;

  private static final int ICON_SIZE = 32;

  /** Tile color behind the first letter of an app that has no icon. */
  private static final int TILE_COLOR = 0xFF1F8A8A;

  /** Held here so the rows stay reachable while their click listeners are live. */
  private View[] rows;

  @Override
  public void onCreate() {
    Log.i(TAG, "onCreate");
    int width = getDisplay().getWidth();
    int height = getDisplay().getHeight();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(width, height);
    root.setPadding(0, 0, 0, 0);
    root.setSpacing(0);

    PackageManager pm = getPackageManager();
    String self = getPackageName();
    List<PackageInfo> installed = pm.getInstalledPackages(0);
    PackageInfo[] sorted = sortedByLabel(pm, installed);
    rows = new View[sorted.length];
    int n = 0;
    // Pass 0 lists the installed apps, pass 1 the other system apps.
    for (int pass = 0; pass < 2; pass++) {
      for (int i = 0; i < sorted.length; i++) {
        PackageInfo info = sorted[i];
        boolean system = (info.applicationInfo.flags & ApplicationInfo.FLAG_SYSTEM) != 0;
        if (system != (pass == 1) || info.packageName.equals(self)) {
          continue;
        }
        View row = makeRow(pm, info, width);
        root.addView(row);
        rows[n] = row;
        n++;
      }
    }
    if (n == 0) {
      TextView empty = new TextView();
      empty.setText("No apps installed. Use pdb install.");
      empty.setTextColor(Color.WHITE);
      root.addView(empty);
    } else {
      rows[0].requestFocus();
    }
    Log.i(TAG, "ready: " + n + " apps");
    setContentView(root);
  }

  /** The packages in label order (a dozen at most, so an insertion sort). */
  private static PackageInfo[] sortedByLabel(PackageManager pm, List<PackageInfo> installed) {
    PackageInfo[] out = new PackageInfo[installed.size()];
    String[] labels = new String[installed.size()];
    for (int i = 0; i < installed.size(); i++) {
      PackageInfo info = installed.get(i);
      String label = pm.getApplicationLabel(info.applicationInfo).toString();
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

  private View makeRow(PackageManager pm, PackageInfo info, int width) {
    final String pkg = info.packageName;
    String label = pm.getApplicationLabel(info.applicationInfo).toString();

    LinearLayout row = new LinearLayout();
    row.setOrientation(LinearLayout.HORIZONTAL);
    row.setSize(width, ROW_HEIGHT);
    row.setPadding(8, 4, 8, 4);
    row.setSpacing(8);

    Drawable icon = pm.getApplicationIcon(info.applicationInfo);
    if (icon != null) {
      ImageView image = new ImageView();
      image.setSize(ICON_SIZE, ICON_SIZE);
      image.setScaleType(ImageView.SCALE_FIT_CENTER);
      image.setImageDrawable(icon);
      row.addView(image);
    } else {
      TextView tile = new TextView();
      tile.setSize(ICON_SIZE, ICON_SIZE);
      tile.setText(label.length() > 0 ? label.substring(0, 1) : "?");
      tile.setTextColor(Color.WHITE);
      tile.setPadding(10, 6, 0, 0);
      tile.setBackground(new GradientDrawable().setColor(TILE_COLOR).setCornerRadius(6));
      row.addView(tile);
    }

    TextView text = new TextView();
    text.setText(label);
    text.setTextColor(Color.WHITE);
    row.addView(text);

    row.setFocusable(true);
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
