package arraysdemo;

import java.util.Arrays;
import picodroid.app.Application;
import picodroid.util.Log;

public class ArraysDemo extends Application {
  private static final String TAG = "ArraysDemo";

  public void onCreate() {
    int[] xs = {5, 3, 8, 1, 9, 2, 7};
    Arrays.sort(xs);
    Log.i(TAG, "sorted ints = " + Arrays.toString(xs));

    long[] ys = {3L, -10L, 0L, 7L, -1L};
    Arrays.sort(ys);
    Log.i(TAG, "sorted longs = " + Arrays.toString(ys));

    double[] ds = {2.5, 1.1, -0.5, 4.0};
    Arrays.sort(ds);
    Log.i(TAG, "sorted doubles = " + Arrays.toString(ds));

    int[] grown = Arrays.copyOf(xs, 10);
    Log.i(TAG, "grown copyOf = " + Arrays.toString(grown));

    int[] filled = new int[5];
    Arrays.fill(filled, 42);
    Log.i(TAG, "filled = " + Arrays.toString(filled));

    byte[] bs = {-128, 0, 127, -1};
    Arrays.sort(bs);
    Log.i(TAG, "sorted bytes = " + Arrays.toString(bs));
  }
}
