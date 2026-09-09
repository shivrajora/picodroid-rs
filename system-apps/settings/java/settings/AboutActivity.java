// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.os.Build;
import picodroid.os.Runtime;
import picodroid.os.StatFs;
import picodroid.util.Log;
import picodroid.widget.LinearLayout;

/** About: the board, its MCU, the release, the volume and the heap. The header row goes back. */
public class AboutActivity extends Activity {
  @Override
  public void onCreate() {
    LinearLayout root = Screens.column(this);
    root.addView(Screens.header(this, "< About", v -> finish()));
    StatFs fs = new StatFs("/");
    long total = fs.getTotalBytes();
    long free = fs.getFreeBytes();
    root.addView(Screens.text(this, "Board  " + Build.BOARD));
    root.addView(Screens.text(this, "MCU  " + Build.HARDWARE));
    root.addView(Screens.text(this, "Release  " + Build.VERSION.RELEASE));
    root.addView(
        Screens.text(
            this, "Storage  " + Screens.kb(total - free) + " / " + Screens.kb(total) + " KB used"));
    root.addView(Screens.text(this, "Heap  " + Screens.kb(Runtime.usedMemory()) + " KB used"));
    setContentView(root);
    Log.i(SettingsActivity.TAG, "about " + Build.BOARD + " " + Build.VERSION.RELEASE);
  }
}
