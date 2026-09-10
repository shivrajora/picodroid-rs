// SPDX-License-Identifier: GPL-3.0-only
package launcher;

import java.util.List;
import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.content.Intent;
import picodroid.content.pm.ApplicationInfo;
import picodroid.content.pm.PackageInfo;
import picodroid.content.pm.PackageManager;
import picodroid.graphics.Color;
import picodroid.graphics.drawable.Drawable;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.text.TextUtils;
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
 * lists itself. Rows are {@link #ROW_HEIGHT} pixels tall from the top of the screen, so a test can
 * tap row 0 at a known point; more rows than the screen holds scroll — by drag on a touch panel,
 * and with the focus on a keypad.
 *
 * <p>The rows are built one per UI tick, not all inside {@code onCreate}: a row is about 20 ms of
 * LVGL work on the device, and a whole list at once would hold the tick for that long times the
 * number of apps. "ready" is logged once the last row is in.
 */
public class LauncherActivity extends Activity {
  private static final String TAG = "Launcher";

  /** Row height in pixels: the bench taps the middle of row 0 at (120, 20). */
  private static final int ROW_HEIGHT = 40;

  private static final int ICON_SIZE = 32;

  /** Tile color behind the first letter of an app that has no icon. */
  private static final int TILE_COLOR = 0xFF1F8A8A;

  private LinearLayout root;

  /** Held here so the rows stay reachable while their click listeners are live. */
  private View[] rows;

  /** The packages in display order, and their labels, fetched once. */
  private PackageInfo[] order;

  private String[] labels;
  private int next;
  private boolean stopped;

  @Override
  public void onCreate() {
    Log.i(TAG, "onCreate");
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
    Executors.mainExecutor().execute(() -> list());
  }

  /** The packages in display order: installed apps by label, then the other system apps. */
  private void list() {
    if (stopped) {
      return;
    }
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
    next = 0;
    Executors.mainExecutor().execute(() -> addRow());
  }

  /** One row per tick; once the last is in, the list is ready. */
  private void addRow() {
    if (stopped) {
      return;
    }
    int width = getDisplay().getWidth();
    if (next < rows.length) {
      rows[next] = makeRow(getPackageManager(), order[next], labels[next], width);
      root.addView(rows[next]);
      next++;
      Executors.mainExecutor().execute(() -> addRow());
      return;
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
    int height = getDisplay().getHeight();
    int contentHeight = (n == 0 ? 1 : n) * ROW_HEIGHT;
    root.setSize(width, contentHeight > height ? contentHeight : height);
    Log.i(TAG, "ready: " + n + " apps");
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

  private View makeRow(PackageManager pm, PackageInfo info, String label, int width) {
    final String pkg = info.packageName;

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
    // One line per row: a label longer than what the icon leaves is cut with an ellipsis, not
    // wrapped over the next row.
    text.setSingleLine();
    text.setEllipsize(TextUtils.TruncateAt.END);
    row.addView(text, new LinearLayout.LayoutParams(0, View.WRAP_CONTENT, 1f));

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

  @Override
  public void onDestroy() {
    stopped = true;
    super.onDestroy();
  }
}
