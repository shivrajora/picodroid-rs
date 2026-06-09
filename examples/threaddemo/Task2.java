// SPDX-License-Identifier: GPL-3.0-only
package threaddemo;

import picodroid.os.SystemClock;
import picodroid.util.Log;

public class Task2 implements Runnable {
  @Override
  public void run() {
    while (true) {
      Log.i("T2", "tock");
      SystemClock.sleep(1000);
    }
  }
}
