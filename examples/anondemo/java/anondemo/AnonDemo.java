// SPDX-License-Identifier: GPL-3.0-only
package anondemo;

import picodroid.app.Application;
import picodroid.util.Log;

public class AnonDemo extends Application {
  public void onCreate() {
    run();
  }

  public static void run() {
    // 1. Anonymous class implementing an interface
    Greeter hello =
        new Greeter() {
          public String greet() {
            return "Hello from anonymous class!";
          }
        };
    Log.i("AnonDemo", hello.greet());

    // 2. Capturing a local variable
    int year = 2026;
    Greeter withCapture =
        new Greeter() {
          public String greet() {
            return "Year=" + year;
          }
        };
    Log.i("AnonDemo", withCapture.greet());

    // 3. Multiple anonymous classes
    Greeter a =
        new Greeter() {
          public String greet() {
            return "I am A";
          }
        };
    Greeter b =
        new Greeter() {
          public String greet() {
            return "I am B";
          }
        };
    Log.i("AnonDemo", a.greet());
    Log.i("AnonDemo", b.greet());
  }
}
