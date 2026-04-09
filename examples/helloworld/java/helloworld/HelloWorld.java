package helloworld;

import picodroid.app.Application;
import picodroid.util.Log;

public class HelloWorld extends Application {
  public void onCreate() {
    Log.i("HelloWorld", "hi " + ("Hello, World!" + " bye ") + 42 + test(123));
  }

  private static String test(int val) {
    return " test " + val;
  }
}
