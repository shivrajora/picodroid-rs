// SPDX-License-Identifier: GPL-3.0-only
package tracedemo;

import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Showcases stack-trace line numbers from the {@code LineNumberTable} Code sub-attribute. A 3-deep
 * call chain throws an uncaught exception; the JVM surfaces a stack trace where each frame shows
 * the source line in debug builds (e.g. {@code at tracedemo.TraceDemo.deepest(:29)}) and falls back
 * to {@code (pc=N)} in release builds.
 */
public class TraceDemo extends Application {

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
