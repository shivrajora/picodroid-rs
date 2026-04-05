package floatdemo;

import picodroid.app.Application;
import picodroid.util.Log;

public class FloatDemo extends Application {
  public void onCreate() {
    // float arithmetic (fmul, f2i)
    float x = 3.0f;
    float y = 2.0f;
    float z = x * y; // 6.0
    int fi = (int) z; // 6 — showcases f2i

    // long arithmetic — showcases ladd, lmul, lcmp, i2l, l2i opcodes
    long a = 1_000_000_000L; // ldc2_w / sipush + i2l
    long b = 3L;
    long product = a * b; // 3_000_000_000 — lmul (exceeds int range)
    int li = (int) product; // -1_294_967_296 — l2i (truncated)

    // double arithmetic — showcases dadd, dmul, dcmpg, i2d, d2i opcodes
    double p = 355.0;
    double q = 113.0;
    double pi = p / q; // ≈ 3.1415929 — ddiv
    int di = (int) pi; // 3 — d2i

    Log.i("FloatDemo", "3.0f * 2.0f = " + fi);
    Log.i("FloatDemo", "1_000_000_000L * 3L (overflows int) = " + li);
    Log.i("FloatDemo", "355.0 / 113.0 (as int) = " + di);
  }
}
