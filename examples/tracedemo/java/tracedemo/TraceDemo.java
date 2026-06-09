// SPDX-License-Identifier: GPL-3.0-only
package tracedemo;

import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Showcases Java-style stack traces: exception class + message header, plus per-frame source line
 * numbers from the {@code LineNumberTable} Code sub-attribute. A 3-deep call chain throws an
 * uncaught {@code RuntimeException("kaboom")}; the JVM prints
 *
 * <pre>
 * Exception in thread "main" java.lang.RuntimeException: kaboom
 *     at tracedemo.TraceDemo.deepest(:29)
 *     at tracedemo.TraceDemo.middle(:25)
 *     ...
 * </pre>
 *
 * In release builds, line numbers are omitted (zero RAM/parse cost) and frames fall back to the
 * {@code (pc=N)} bytecode-offset format.
 */
public class TraceDemo extends Application {

  @Override
  public void onCreate() {
    Log.i("TraceDemo", "starting — about to throw an uncaught exception");
    outer();
  }

  static void outer() {
    middle();
  }

  static void middle() {
    deepest();
  }

  static void deepest() {
    throw new RuntimeException("kaboom");
  }
}
