// SPDX-License-Identifier: GPL-3.0-only
package exceptiondemo;

import picodroid.app.Application;
import picodroid.util.Log;

public class ExceptionDemo extends Application {

  /** Throws AppException when x is negative, logs otherwise. */
  static void riskyMethod(int x) throws AppException {
    if (x < 0) {
      throw new AppException();
    }
    Log.i("ExceptionDemo", "no exception");
  }

  @Override
  public void onCreate() {
    run();
  }

  public static void run() {
    // 1. Exception thrown and caught in the same frame.
    try {
      riskyMethod(-1);
    } catch (AppException e) {
      Log.i("ExceptionDemo", "caught AppException");
    }

    // 2. No exception — normal path still works after a try/catch.
    try {
      riskyMethod(1);
    } catch (AppException e) {
      Log.i("ExceptionDemo", "should not reach here");
    }

    // 3. Throwable.getMessage: message-carrying ctor surfaces it, no-arg ctor yields null.
    try {
      throw new RuntimeException("boom with detail");
    } catch (RuntimeException e) {
      Log.i("ExceptionDemo", "getMessage=" + e.getMessage());
      // 4. Log severity ladder + Throwable overload (appends ": <message>").
      Log.v("ExceptionDemo", "verbose level");
      Log.d("ExceptionDemo", "debug level");
      Log.w("ExceptionDemo", "warn level");
      Log.e("ExceptionDemo", "error level", e);
      Log.wtf("ExceptionDemo", "wtf level");
    }
    try {
      throw new AppException();
    } catch (AppException e) {
      String m = e.getMessage();
      Log.i("ExceptionDemo", "no-arg getMessage null=" + (m == null));
    }

    Log.i("ExceptionDemo", "done");
  }
}
