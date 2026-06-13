// SPDX-License-Identifier: GPL-3.0-only
package clonedemo;

import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Exercises Object.clone(): shallow-copy semantics, identity, class preservation, and the canonical
 * override pattern ({@code (T) super.clone()} inside a Cloneable class).
 */
public class CloneDemo extends Application {
  private static final String TAG = "CloneDemo";

  static int passed = 0;
  static int failed = 0;

  static void check(String name, boolean condition) {
    if (condition) {
      Log.i(TAG, "PASS: " + name);
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  static class Point implements Cloneable {
    int x;
    int y;
    int[] tags; // reference field: clone must SHARE it (shallow copy)

    Point(int x, int y, int[] tags) {
      this.x = x;
      this.y = y;
      this.tags = tags;
    }

    @Override
    public Point clone() {
      try {
        return (Point) super.clone();
      } catch (CloneNotSupportedException e) {
        // Unreachable on picodroid (the marker is not enforced) but the
        // catch is required to compile against the JDK's Object.clone().
        throw new RuntimeException("clone failed");
      }
    }
  }

  @Override
  public void onCreate() {
    run();
  }

  public static void run() {
    Log.i(TAG, "=== Clone Tests ===");

    Point p = new Point(3, 4, new int[] {7, 8});
    Point q = p.clone();

    check("clone is a distinct object", q != p);
    check("clone copies primitive fields", q.x == 3 && q.y == 4);
    check("clone preserves class", p.getClass() == q.getClass());
    check("clone shares reference fields (shallow)", q.tags == p.tags);

    q.x = 99;
    check("original untouched after clone mutation", p.x == 3);

    q.tags[0] = 42;
    check("shared array visible through original", p.tags[0] == 42);

    String passStr = String.valueOf(passed);
    String failStr = String.valueOf(failed);
    Log.i(TAG, "Results: " + passStr + " passed, " + failStr + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== SOME FAILED ===");
    }
  }
}
