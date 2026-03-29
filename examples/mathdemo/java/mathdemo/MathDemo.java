package mathdemo;

import picodroid.util.Log;

public class MathDemo {
  public static void main(String[] args) {
    // abs
    Log.i("MathDemo", "abs(-7) = " + Math.abs(-7));
    Log.i("MathDemo", "abs(-3.14f) = " + Math.abs(-3.14f));

    // min / max
    Log.i("MathDemo", "min(4, 9) = " + Math.min(4, 9));
    Log.i("MathDemo", "max(4, 9) = " + Math.max(4, 9));

    // sqrt
    double sq2 = Math.sqrt(2.0);
    Log.i("MathDemo", "sqrt(2.0) ~ 1.414: " + (int) (sq2 * 1000));

    // pow
    double p = Math.pow(2.0, 10.0);
    Log.i("MathDemo", "pow(2, 10) = " + (int) p);

    // floor / ceil
    Log.i("MathDemo", "floor(2.9) = " + (int) Math.floor(2.9));
    Log.i("MathDemo", "ceil(2.1) = " + (int) Math.ceil(2.1));

    // round
    Log.i("MathDemo", "round(2.6f) = " + Math.round(2.6f));

    // trig — sin(PI/2) = 1, cos(0) = 1
    double sinHalfPi = Math.sin(Math.PI / 2.0);
    double cos0 = Math.cos(0.0);
    Log.i("MathDemo", "sin(PI/2) ~ 1000: " + (int) (sinHalfPi * 1000));
    Log.i("MathDemo", "cos(0) ~ 1000: " + (int) (cos0 * 1000));

    // toRadians / toDegrees
    double rad = Math.toRadians(90.0);
    double deg = Math.toDegrees(Math.PI);
    Log.i("MathDemo", "toRadians(90) ~ 1570: " + (int) (rad * 1000));
    Log.i("MathDemo", "toDegrees(PI) = " + (int) deg);

    // log / exp
    double logE = Math.log(Math.E);
    double exp1 = Math.exp(1.0);
    Log.i("MathDemo", "log(E) ~ 1000: " + (int) (logE * 1000));
    Log.i("MathDemo", "exp(1) ~ 2718: " + (int) (exp1 * 1000));
  }
}
