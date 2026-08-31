// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

/**
 * A one-shot gate that opens when {@link #countDown()} has been called {@code count} times,
 * mirroring {@code java.util.concurrent.CountDownLatch}.
 */
public class CountDownLatch {
  private int count;

  public CountDownLatch(int count) {
    if (count < 0) {
      throw new IllegalArgumentException("count < 0");
    }
    this.count = count;
  }

  /** Blocks until the count reaches zero. */
  public synchronized void await() throws InterruptedException {
    while (count > 0) {
      wait();
    }
  }

  /** Blocks until the count reaches zero or the timeout elapses; true if it reached zero. */
  public synchronized boolean await(long timeout, TimeUnit unit) throws InterruptedException {
    long deadline = System.currentTimeMillis() + unit.toMillis(timeout);
    while (count > 0) {
      long left = deadline - System.currentTimeMillis();
      if (left <= 0) {
        return false;
      }
      wait(left);
    }
    return true;
  }

  public synchronized void countDown() {
    if (count == 0) {
      return;
    }
    count = count - 1;
    if (count == 0) {
      notifyAll();
    }
  }

  public synchronized long getCount() {
    return count;
  }

  @Override
  public String toString() {
    return "CountDownLatch[Count = " + getCount() + "]";
  }
}
