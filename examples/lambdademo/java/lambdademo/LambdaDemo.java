// SPDX-License-Identifier: GPL-3.0-only
package lambdademo;

import picodroid.app.Application;
import picodroid.util.Log;

public class LambdaDemo extends Application {
  @Override
  public void onCreate() {
    run();
  }

  public static void run() {
    // 1. Non-capturing lambda
    IntSupplier three = () -> 3;
    Log.i("Lambda", "non-capturing: " + three.get());

    // 2. Capturing lambda (captures local variable)
    int base = 100;
    IntSupplier added = () -> base + 7;
    Log.i("Lambda", "capturing: " + added.get());

    // 3. Lambda passed as callback
    logResult("callback", () -> 42);

    // 4. Method reference (static)
    IntSupplier ref = LambdaDemo::meaningOfLife;
    Log.i("Lambda", "method-ref: " + ref.get());
  }

  static void logResult(String label, IntSupplier supplier) {
    Log.i("Lambda", label + ": " + supplier.get());
  }

  static int meaningOfLife() {
    return 42;
  }
}
