// SPDX-License-Identifier: GPL-3.0-only
package tracedemo;

import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Showcases Java-style stack traces: exception class + message header, plus per-frame source
 * positions from the {@code SourceFile} attribute and the {@code LineNumberTable} Code
 * sub-attribute, in Android's {@code StackTraceElement} spelling. A 3-deep call chain throws an
 * uncaught {@code RuntimeException("kaboom")}; the JVM prints
 *
 * <pre>
 * Exception in thread "main" java.lang.RuntimeException: kaboom
 *     at tracedemo.TraceDemo.deepest(TraceDemo.java:41)
 *     at tracedemo.TraceDemo.middle(TraceDemo.java:37)
 *     ...
 * </pre>
 *
 * That is the sim and debug-profile device firmware ({@code line-numbers} cargo feature). Release
 * firmware leaves the tables out of flash and prints the {@code (pc=N)} bytecode offset instead;
 * pipe its log through {@code scripts/retrace.sh --app tracedemo} to get the same frames back.
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
