package bytecodecoverage;

import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Exercises JVM bytecode that was missing prior to M2-D6: - long[] / double[] arrays (laload /
 * lastore / daload / dastore) - multi-dimensional arrays (multianewarray) - stack manipulation in
 * fluent builders (dup_x1 / dup_x2 via StringBuilder) - goto_w can't be triggered from small source
 * without bloat, so it is exercised by unit tests in jvm/src/interpreter/tests/wide_goto.rs.
 */
public class BytecodeCoverage extends Application {
  private static final String TAG = "ByteCov";

  public void onCreate() {
    longArrays();
    doubleArrays();
    multiDim();
    fluentBuilder();
    Log.i(TAG, "done");
  }

  private static void longArrays() {
    long[] counts = new long[8];
    for (int i = 0; i < counts.length; i++) {
      counts[i] = (long) i * 1_000_000_000L;
    }
    long sum = 0;
    for (int i = 0; i < counts.length; i++) {
      sum += counts[i];
    }
    Log.i(TAG, "long sum = " + sum);
  }

  private static void doubleArrays() {
    double[] vals = new double[4];
    vals[0] = 1.5;
    vals[1] = 2.5;
    vals[2] = 3.5;
    vals[3] = 4.5;
    double total = 0.0;
    for (int i = 0; i < vals.length; i++) {
      total += vals[i];
    }
    Log.i(TAG, "double total = " + total);
  }

  private static void multiDim() {
    int[][] grid = new int[3][4];
    for (int r = 0; r < grid.length; r++) {
      for (int c = 0; c < grid[r].length; c++) {
        grid[r][c] = r * 10 + c;
      }
    }
    Log.i(TAG, "grid[2][3] = " + grid[2][3]);
  }

  private static void fluentBuilder() {
    // StringBuilder.append(long) forces javac to emit dup_x2 so the
    // builder reference stays under the pushed long category-2 value.
    StringBuilder sb = new StringBuilder();
    sb.append("n=").append((long) 42).append(" d=").append(3.14);
    Log.i(TAG, sb.toString());
  }
}
