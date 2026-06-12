// SPDX-License-Identifier: GPL-3.0-only
package prefsdemo;

import picodroid.app.Application;
import picodroid.content.SharedPreferences;
import picodroid.util.Log;

public class PrefsDemo extends Application {
  private static final String TAG = "PrefsDemo";

  // Intentionally no explicit = 0 initializers here — exercises the JVMS §5.5
  // step 2 static-field preparation path that sets primitives to typed zeros
  // before <clinit> runs.  A regression would manifest as an ArrayIOB /
  // InvalidBytecode on the first check() call (iadd on Null).
  static int passed;
  static int failed;

  static void check(String name, boolean cond) {
    if (cond) {
      Log.i(TAG, "PASS: " + name);
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  @Override
  public void onCreate() {
    Log.i(TAG, "=== Preferences tests ===");

    testWriteAndReadBack();
    testTypeSafety();
    testUpdateExisting();
    testRemoveAndContains();
    testClear();
    testPersistenceAcrossInstances();
    testContextIdiomAndApply();

    Log.i(TAG, "Results: " + passed + " passed, " + failed + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== SOME FAILED ===");
    }
  }

  static void testWriteAndReadBack() {
    SharedPreferences p = SharedPreferences.open("demo1");
    p.edit().clear().commit();
    boolean ok =
        p.edit()
            .putString("ssid", "home-wifi")
            .putInt("channel", 6)
            .putLong("uptime", 123456789012L)
            .putBoolean("autoconnect", true)
            .commit();
    check("commit success", ok);

    SharedPreferences q = SharedPreferences.open("demo1");
    check("readback string", "home-wifi".equals(q.getString("ssid", "")));
    check("readback int", q.getInt("channel", -1) == 6);
    check("readback long", q.getLong("uptime", -1L) == 123456789012L);
    check("readback bool true", q.getBoolean("autoconnect", false));
    check("missing returns default", q.getInt("missing", 42) == 42);
  }

  static void testTypeSafety() {
    SharedPreferences p = SharedPreferences.open("demo2");
    p.edit().clear().putInt("x", 5).commit();
    SharedPreferences q = SharedPreferences.open("demo2");
    check("wrong-type string falls back", "def".equals(q.getString("x", "def")));
    check("correct-type int still works", q.getInt("x", -1) == 5);
  }

  static void testUpdateExisting() {
    SharedPreferences p = SharedPreferences.open("demo3");
    p.edit().clear().putInt("count", 1).commit();
    SharedPreferences q = SharedPreferences.open("demo3");
    q.edit().putInt("count", q.getInt("count", 0) + 1).commit();
    SharedPreferences r = SharedPreferences.open("demo3");
    check("updated value", r.getInt("count", -1) == 2);
  }

  static void testRemoveAndContains() {
    SharedPreferences p = SharedPreferences.open("demo4");
    p.edit().clear().putString("k1", "v1").putString("k2", "v2").commit();
    SharedPreferences q = SharedPreferences.open("demo4");
    check("contains before remove", q.contains("k1"));
    q.edit().remove("k1").commit();
    SharedPreferences r = SharedPreferences.open("demo4");
    check("missing after remove", !r.contains("k1"));
    check("sibling survives remove", r.contains("k2"));
  }

  static void testClear() {
    SharedPreferences p = SharedPreferences.open("demo5");
    p.edit().putString("a", "1").putString("b", "2").commit();
    p.edit().clear().commit();
    SharedPreferences q = SharedPreferences.open("demo5");
    check("cleared has no keys", q.getAllKeys().length == 0);
    check("cleared missing default", 99 == q.getInt("a", 99));
  }

  static void testPersistenceAcrossInstances() {
    SharedPreferences p = SharedPreferences.open("demo6");
    p.edit().clear().putString("greeting", "hello").putInt("n", 7).commit();
    String[] keys = SharedPreferences.open("demo6").getAllKeys();
    check("getAllKeys size", keys.length == 2);
    boolean sawGreeting = false;
    boolean sawN = false;
    for (int i = 0; i < keys.length; i++) {
      if ("greeting".equals(keys[i])) {
        sawGreeting = true;
      }
      if ("n".equals(keys[i])) {
        sawN = true;
      }
    }
    check("getAllKeys has greeting", sawGreeting);
    check("getAllKeys has n", sawN);
  }

  /** The Android idiom end-to-end: context.getSharedPreferences(...).edit()...apply(). */
  void testContextIdiomAndApply() {
    SharedPreferences p = getSharedPreferences("demo7", MODE_PRIVATE);
    p.edit().clear().putString("source", "context").putInt("mode", MODE_PRIVATE).apply();
    SharedPreferences q = getSharedPreferences("demo7", MODE_PRIVATE);
    check("getSharedPreferences + apply persists", "context".equals(q.getString("source", "")));
    check("apply persisted int", q.getInt("mode", -1) == 0);
  }
}
