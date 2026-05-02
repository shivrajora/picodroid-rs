// SPDX-License-Identifier: GPL-3.0-only
package clinitdemo;

import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Demonstrates Java static class initializers ({@code <clinit>}).
 *
 * <p>Exercises field initializers, static blocks, string constants, cross-class clinit chaining,
 * and verifies clinit runs only once.
 */
public class ClinitDemo extends Application {
  private static final String TAG = "ClinitDemo";
  private static int X = 42;
  private static int Y;

  static {
    Y = 100;
  }

  public void onCreate() {
    Log.i(TAG, "Field init: X = " + X);
    Log.i(TAG, "Static block: Y = " + Y);
    Log.i(TAG, "String constant: TAG = " + TAG);
    Log.i(TAG, "Cross-class: Constants.MAGIC = " + Constants.MAGIC);
    Log.i(TAG, "Cross-class: Constants.DERIVED = " + Constants.DERIVED);

    // Access X again — clinit must not re-run, value must be the same.
    int x2 = X;
    Log.i(TAG, "Second access: X = " + x2);

    Log.i(TAG, "All clinit tests passed!");
  }
}
