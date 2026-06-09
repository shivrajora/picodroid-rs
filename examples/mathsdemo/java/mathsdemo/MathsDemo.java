// SPDX-License-Identifier: GPL-3.0-only
package mathsdemo;

import picodroid.app.Application;
import picodroid.util.Log;

public class MathsDemo extends Application {
  private static final String TAG = "MathsDemo";

  @Override
  public void onCreate() {
    run();
  }

  public static void run() {
    // ── Integer arithmetic ─────────────────────────────────────────────────
    int a = 10, b = 3;
    Log.i(TAG, "10 - 3 = " + (a - b)); // isub
    Log.i(TAG, "10 / 3 = " + (a / b)); // idiv
    Log.i(TAG, "10 % 3 = " + (a % b)); // irem
    Log.i(TAG, "-10 = " + (-a)); // ineg

    // ── Integer bitwise / shifts ───────────────────────────────────────────
    Log.i(TAG, "1 << 3 = " + (1 << 3)); // ishl  → 8
    Log.i(TAG, "16 >> 2 = " + (16 >> 2)); // ishr  → 4
    Log.i(TAG, "-1 >>> 28 = " + (-1 >>> 28)); // iushr → 15
    Log.i(TAG, "5 | 3 = " + (5 | 3)); // ior   → 7
    Log.i(TAG, "5 ^ 3 = " + (5 ^ 3)); // ixor  → 6

    // ── Cross-type conversions ─────────────────────────────────────────────
    float fi = (float) a; // i2f
    int ci = (char) a; // i2c  (10 = LF)
    int si = (short) a; // i2s
    float lf = (float) 100L; // l2f
    double ld = (double) 100L; // l2d
    long fl = (long) 3.14f; // f2l
    double fd = (double) 3.14f; // f2d
    long dl = (long) 3.14; // d2l
    float df = (float) 3.14; // d2f
    Log.i(
        TAG,
        "i2f(10)="
            + (int) fi
            + " l2f(100L)="
            + (int) lf
            + " f2l(3.14f)="
            + fl
            + " d2l(3.14)="
            + dl);

    // ── Long arithmetic ────────────────────────────────────────────────────
    long la = 100L, lb = 30L;
    Log.i(TAG, "100L - 30L = " + (la - lb)); // lsub
    Log.i(TAG, "100L / 30L = " + (la / lb)); // ldiv
    Log.i(TAG, "100L % 30L = " + (la % lb)); // lrem
    Log.i(TAG, "-100L = " + (-la)); // lneg

    // ── Long bitwise / shifts ──────────────────────────────────────────────
    Log.i(TAG, "1L << 3 = " + (1L << 3)); // lshl  → 8
    Log.i(TAG, "16L >> 2 = " + (16L >> 2)); // lshr  → 4
    Log.i(TAG, "-1L >>> 60 = " + (-1L >>> 60)); // lushr → 15
    Log.i(TAG, "5L | 3L = " + (5L | 3L)); // lor   → 7
    Log.i(TAG, "5L ^ 3L = " + (5L ^ 3L)); // lxor  → 6

    // ── Double arithmetic ──────────────────────────────────────────────────
    double da = 10.0, db = 3.0;
    Log.i(TAG, "10.0 - 3.0 = " + (int) (da - db)); // dsub
    Log.i(TAG, "10.0 % 3.0 = " + (int) (da % db)); // drem
    Log.i(TAG, "-10.0 = " + (int) (-da)); // dneg

    // ── Float comparisons (fcmpl / fcmpg) ─────────────────────────────────
    float x = 1.5f, y = 2.5f;
    if (x < y) Log.i(TAG, "1.5 < 2.5"); // fcmpl
    if (y > x) Log.i(TAG, "2.5 > 1.5"); // fcmpg

    // ── Double comparison (dcmpl) ──────────────────────────────────────────
    if (da < db) Log.i(TAG, "10.0 < 3.0"); // dcmpl (won't print)
    if (da > db) Log.i(TAG, "10.0 > 3.0"); // dcmpl

    // ── Dense tableswitch ──────────────────────────────────────────────────
    for (int i = 0; i <= 3; i++) {
      switch (i) { // tableswitch (cases 0-3 are dense)
        case 0:
          Log.i(TAG, "case 0");
          break;
        case 1:
          Log.i(TAG, "case 1");
          break;
        case 2:
          Log.i(TAG, "case 2");
          break;
        case 3:
          Log.i(TAG, "case 3");
          break;
        default:
          Log.i(TAG, "default");
          break;
      }
    }

    // ── Comparison branches ────────────────────────────────────────────────
    if (a >= 5) Log.i(TAG, "10 >= 5"); // ifge
    if (a > 5) Log.i(TAG, "10 > 5"); // ifgt
    if (a <= 15) Log.i(TAG, "10 <= 15"); // ifle

    // ── ifnonnull ──────────────────────────────────────────────────────────
    Object obj = new Circle();
    if (obj != null) Log.i(TAG, "obj != null"); // ifnonnull

    // ── instanceof / checkcast ─────────────────────────────────────────────
    Shape shape = new Circle();
    if (shape instanceof Circle) Log.i(TAG, "shape instanceof Circle"); // instanceof
    Circle c = (Circle) shape; // checkcast
    Log.i(TAG, "sides=" + c.sides);

    // ── Reference array ────────────────────────────────────────────────────
    Shape[] arr = new Shape[2]; // anewarray
    arr[0] = new Circle(); // aastore
    arr[1] = new Circle(); // aastore
    Shape first = arr[0]; // aaload
    Log.i(TAG, "arr[0].sides=" + first.sides);

    // ── java.lang.Math library ───────────────────────────────────────────
    Log.i(TAG, "abs(-7) = " + Math.abs(-7));
    Log.i(TAG, "abs(-3.14f) = " + Math.abs(-3.14f));
    Log.i(TAG, "min(4, 9) = " + Math.min(4, 9));
    Log.i(TAG, "max(4, 9) = " + Math.max(4, 9));

    double sq2 = Math.sqrt(2.0);
    Log.i(TAG, "sqrt(2.0) ~ 1.414: " + (int) (sq2 * 1000));

    double p = Math.pow(2.0, 10.0);
    Log.i(TAG, "pow(2, 10) = " + (int) p);

    Log.i(TAG, "floor(2.9) = " + (int) Math.floor(2.9));
    Log.i(TAG, "ceil(2.1) = " + (int) Math.ceil(2.1));
    Log.i(TAG, "round(2.6f) = " + Math.round(2.6f));

    double sinHalfPi = Math.sin(Math.PI / 2.0);
    double cos0 = Math.cos(0.0);
    Log.i(TAG, "sin(PI/2) ~ 1000: " + (int) (sinHalfPi * 1000));
    Log.i(TAG, "cos(0) ~ 1000: " + (int) (cos0 * 1000));

    double rad = Math.toRadians(90.0);
    double deg = Math.toDegrees(Math.PI);
    Log.i(TAG, "toRadians(90) ~ 1570: " + (int) (rad * 1000));
    Log.i(TAG, "toDegrees(PI) = " + (int) deg);

    double logE = Math.log(Math.E);
    double exp1 = Math.exp(1.0);
    Log.i(TAG, "log(E) ~ 1000: " + (int) (logE * 1000));
    Log.i(TAG, "exp(1) ~ 2718: " + (int) (exp1 * 1000));
  }
}
