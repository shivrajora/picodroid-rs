// SPDX-License-Identifier: GPL-3.0-only
package enumdemo;

import picodroid.app.Application;
import picodroid.util.Log;

public class EnumDemo extends Application {
  private static final String TAG = "EnumDemo";

  static int passed = 0;
  static int failed = 0;

  static void check(String name, boolean condition) {
    if (condition) {
      Log.i(TAG, "PASS: " + name);
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  public void onCreate() {
    run();
  }

  public static void run() {
    Log.i(TAG, "=== Enum Tests ===");

    // name and ordinal
    String rName = Color.RED.name();
    check("RED name", rName.equals("RED"));
    String gName = Color.GREEN.name();
    check("GREEN name", gName.equals("GREEN"));
    String bName = Color.BLUE.name();
    check("BLUE name", bName.equals("BLUE"));
    check("RED ordinal=0", Color.RED.ordinal() == 0);
    check("GREEN ordinal=1", Color.GREEN.ordinal() == 1);
    check("BLUE ordinal=2", Color.BLUE.ordinal() == 2);

    // equality
    Color a = Color.RED;
    Color b = Color.RED;
    check("== equality", a == b);
    check("!= different", a != Color.BLUE);

    // toString
    String s = Color.BLUE.toString();
    check("toString BLUE", s.equals("BLUE"));

    // switch — uses ordinal() via a synthetic $SwitchMap class
    check("switch RED", describe(Color.RED).equals("red"));
    check("switch GREEN", describe(Color.GREEN).equals("green"));
    check("switch BLUE", describe(Color.BLUE).equals("blue"));

    String passStr = String.valueOf(passed);
    String failStr = String.valueOf(failed);
    Log.i(TAG, "Results: " + passStr + " passed, " + failStr + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== SOME FAILED ===");
    }
  }

  static String describe(Color c) {
    switch (c) {
      case RED:
        return "red";
      case GREEN:
        return "green";
      case BLUE:
        return "blue";
      default:
        return "unknown";
    }
  }
}
