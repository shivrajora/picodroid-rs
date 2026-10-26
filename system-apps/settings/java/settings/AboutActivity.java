// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.os.Build;
import picodroid.os.Bundle;
import picodroid.os.Runtime;
import picodroid.os.StatFs;
import picodroid.util.Log;
import picodroid.widget.LinearLayout;

/**
 * About: the board, its MCU, the release, the serial number, the volume and the heap. The header
 * row goes back.
 */
public class AboutActivity extends Activity {
  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    LinearLayout column = Screens.screen(this, "< About", v -> finish());
    Screens.info(this, column, "Board  " + Build.BOARD);
    Screens.info(this, column, "MCU  " + Build.HARDWARE);
    Screens.info(this, column, "Release  " + Build.VERSION.RELEASE);
    Screens.info(this, column, "Serial  " + Build.getSerial());
    StatFs fs = new StatFs("/");
    long total = fs.getTotalBytes();
    long free = fs.getFreeBytes();
    Screens.info(
        this,
        column,
        "Storage  " + Screens.kb(total - free) + " / " + Screens.kb(total) + " KB used");
    Screens.info(this, column, "Heap  " + Screens.kb(Runtime.usedMemory()) + " KB used");
    Log.i(SettingsActivity.TAG, "about " + Build.BOARD + " " + Build.VERSION.RELEASE);
  }
}
