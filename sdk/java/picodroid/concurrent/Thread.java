// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

public class Thread {
  public static final int MIN_PRIORITY = 1;
  public static final int NORM_PRIORITY = 5;
  public static final int MAX_PRIORITY = 10;

  private Runnable target;
  private int priority;

  public Thread(Runnable target) {
    this.target = target;
    this.priority = NORM_PRIORITY;
  }

  public void setPriority(int priority) {
    if (priority < MIN_PRIORITY || priority > MAX_PRIORITY) {
      throw new IllegalArgumentException("Priority out of range");
    }
    this.priority = priority;
  }

  public int getPriority() {
    return priority;
  }

  public native void start();
}
