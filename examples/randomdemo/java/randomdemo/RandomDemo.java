package randomdemo;

import java.util.Random;
import picodroid.app.Application;
import picodroid.util.Log;

public class RandomDemo extends Application {
  private static final String TAG = "RandomDemo";

  public void onCreate() {
    // Deterministic sequence: same seed must give the same draws every run.
    Random r = new Random(42L);
    StringBuilder sb = new StringBuilder("seeded ints:");
    for (int i = 0; i < 5; i++) {
      sb.append(' ');
      sb.append(r.nextInt(100));
    }
    Log.i(TAG, sb.toString());

    Random r2 = new Random(42L);
    sb = new StringBuilder("longs:");
    for (int i = 0; i < 3; i++) {
      sb.append(' ');
      sb.append(r2.nextLong());
    }
    Log.i(TAG, sb.toString());

    Random r3 = new Random(7L);
    sb = new StringBuilder("doubles:");
    for (int i = 0; i < 3; i++) {
      sb.append(' ');
      sb.append(r3.nextDouble());
    }
    Log.i(TAG, sb.toString());

    Random r4 = new Random(99L);
    sb = new StringBuilder("gaussians:");
    for (int i = 0; i < 3; i++) {
      sb.append(' ');
      sb.append(r4.nextGaussian());
    }
    Log.i(TAG, sb.toString());

    Random r5 = new Random(17L);
    byte[] bytes = new byte[8];
    r5.nextBytes(bytes);
    sb = new StringBuilder("bytes:");
    for (int i = 0; i < bytes.length; i++) {
      sb.append(' ');
      sb.append(bytes[i]);
    }
    Log.i(TAG, sb.toString());
  }
}
