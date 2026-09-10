// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.os.Build;
import picodroid.os.Runtime;
import picodroid.os.StatFs;
import picodroid.util.Log;
import picodroid.view.View;

/** About: the board, its MCU, the release, the volume and the heap. The header row goes back. */
public class AboutActivity extends Activity {
  private Column column;

  @Override
  public void onCreate() {
    column = new Column(this, "< About", v -> finish());
    column.fill(
        null,
        i -> row(i),
        () -> Log.i(SettingsActivity.TAG, "about " + Build.BOARD + " " + Build.VERSION.RELEASE));
  }

  private View row(int i) {
    switch (i) {
      case 0:
        return Screens.info(this, "Board  " + Build.BOARD);
      case 1:
        return Screens.info(this, "MCU  " + Build.HARDWARE);
      case 2:
        return Screens.info(this, "Release  " + Build.VERSION.RELEASE);
      case 3:
        StatFs fs = new StatFs("/");
        long total = fs.getTotalBytes();
        long free = fs.getFreeBytes();
        return Screens.info(
            this, "Storage  " + Screens.kb(total - free) + " / " + Screens.kb(total) + " KB used");
      case 4:
        return Screens.info(this, "Heap  " + Screens.kb(Runtime.usedMemory()) + " KB used");
      default:
        return null;
    }
  }

  @Override
  public void onDestroy() {
    column.stop();
    super.onDestroy();
  }
}
