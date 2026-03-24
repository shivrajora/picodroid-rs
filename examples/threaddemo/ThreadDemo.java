package threaddemo;

import picodroid.concurrent.Thread;
import picodroid.util.Log;

public class ThreadDemo {
  public static void main() {
    Log.i("Main", "Starting threads");
    Thread t1 = new Thread(new Task1());
    Thread t2 = new Thread(new Task2());
    t1.start();
    t2.start();
  }
}
