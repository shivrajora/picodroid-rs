package exceptiondemo;

import picodroid.util.Log;

public class ExceptionDemo {

  /** Throws AppException when x is negative, logs otherwise. */
  static void riskyMethod(int x) throws AppException {
    if (x < 0) {
      throw new AppException();
    }
    Log.i("ExceptionDemo", "no exception");
  }

  public static void main(String[] args) {
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

    Log.i("ExceptionDemo", "done");
  }
}
