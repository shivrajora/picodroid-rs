// SPDX-License-Identifier: GPL-3.0-only
package threaddemo;

import picodroid.os.SystemClock;
import picodroid.util.Log;

public class Task1 implements Runnable {
  public void run() {
    while (true) {
      Log.i("T1", "tick");
      SystemClock.sleep(500);
    }
  }
}
