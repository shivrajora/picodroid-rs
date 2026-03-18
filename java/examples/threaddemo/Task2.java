package threaddemo;

import picodroid.os.SystemClock;
import picodroid.util.Log;

public class Task2 implements Runnable {
  public void run() {
    while (true) {
      Log.i("T2", "tock");
      SystemClock.sleep(1000);
    }
  }
}
