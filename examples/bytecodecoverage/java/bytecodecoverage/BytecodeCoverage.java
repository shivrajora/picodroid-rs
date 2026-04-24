package bytecodecoverage;

import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Exercises JVM bytecode that was missing prior to M2-D6: - long[] / double[] arrays (laload /
 * lastore / daload / dastore) - multi-dimensional arrays (multianewarray, aaload of ArrayRef) -
 * stack manipulation in fluent builders (dup_x2 via StringBuilder.append(long/double))
 *
 * <p>goto_w requires >32KB of method bytecode to trigger and is covered by the hand-crafted unit
 * tests in jvm/src/interpreter/tests/wide_goto.rs instead. Prints "=== ALL PASSED ===" iff every
 * subtest produces the expected value.
 */
public class BytecodeCoverage extends Application {
  private static final String TAG = "ByteCov";

  public void onCreate() {
    int failures = 0;
    failures += longArrays();
    failures += doubleArrays();
    failures += multiDim();
    failures += fluentBuilder();
    if (failures == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED (" + failures + " subtests) ===");
    }
  }

  private static int longArrays() {
    long[] counts = new long[8];
    for (int i = 0; i < counts.length; i++) {
      counts[i] = (long) i * 1_000_000_000L;
    }
    long sum = 0;
    for (int i = 0; i < counts.length; i++) {
      sum += counts[i];
    }
    long expected = 28_000_000_000L;
    if (sum != expected) {
      Log.i(TAG, "long FAIL: got " + sum + " expected " + expected);
      return 1;
    }
    Log.i(TAG, "long PASS");
    return 0;
  }

  private static int doubleArrays() {
    double[] vals = new double[4];
    vals[0] = 1.5;
    vals[1] = 2.5;
    vals[2] = 3.5;
    vals[3] = 4.5;
    double total = 0.0;
    for (int i = 0; i < vals.length; i++) {
      total += vals[i];
    }
    if (total != 12.0) {
      Log.i(TAG, "double FAIL: got " + total);
      return 1;
    }
    Log.i(TAG, "double PASS");
    return 0;
  }

  private static int multiDim() {
    int[][] grid = new int[3][4];
    for (int r = 0; r < grid.length; r++) {
      for (int c = 0; c < grid[r].length; c++) {
        grid[r][c] = r * 10 + c;
      }
    }
    int v = grid[2][3];
    if (v != 23) {
      Log.i(TAG, "multidim FAIL: grid[2][3]=" + v);
      return 1;
    }
    Log.i(TAG, "multidim PASS");
    return 0;
  }

  private static int fluentBuilder() {
    // StringBuilder.append(long) forces javac to emit dup_x2 so the
    // builder reference stays under the pushed long category-2 value.
    StringBuilder sb = new StringBuilder();
    sb.append("n=").append((long) 42).append(" d=").append(3.14);
    String out = sb.toString();
    String want = "n=42 d=3.14";
    if (!out.equals(want)) {
      Log.i(TAG, "fluent FAIL: got " + out);
      return 1;
    }
    Log.i(TAG, "fluent PASS");
    return 0;
  }
}
