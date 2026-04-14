package prefsdemo;

import picodroid.app.Application;
import picodroid.content.Preferences;
import picodroid.util.Log;

public class PrefsDemo extends Application {
  private static final String TAG = "PrefsDemo";

  static int passed = 0;
  static int failed = 0;

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

    Log.i(TAG, "Results: " + passed + " passed, " + failed + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== SOME FAILED ===");
    }
  }

  static void testWriteAndReadBack() {
    Preferences p = Preferences.open("demo1");
    p.edit().clear().commit();
    boolean ok =
        p.edit()
            .putString("ssid", "home-wifi")
            .putInt("channel", 6)
            .putLong("uptime", 123456789012L)
            .putBoolean("autoconnect", true)
            .commit();
    check("commit success", ok);

    Preferences q = Preferences.open("demo1");
    check("readback string", "home-wifi".equals(q.getString("ssid", "")));
    check("readback int", q.getInt("channel", -1) == 6);
    check("readback long", q.getLong("uptime", -1L) == 123456789012L);
    check("readback bool true", q.getBoolean("autoconnect", false));
    check("missing returns default", q.getInt("missing", 42) == 42);
  }

  static void testTypeSafety() {
    Preferences p = Preferences.open("demo2");
    p.edit().clear().putInt("x", 5).commit();
    Preferences q = Preferences.open("demo2");
    check("wrong-type string falls back", "def".equals(q.getString("x", "def")));
    check("correct-type int still works", q.getInt("x", -1) == 5);
  }

  static void testUpdateExisting() {
    Preferences p = Preferences.open("demo3");
    p.edit().clear().putInt("count", 1).commit();
    Preferences q = Preferences.open("demo3");
    q.edit().putInt("count", q.getInt("count", 0) + 1).commit();
    Preferences r = Preferences.open("demo3");
    check("updated value", r.getInt("count", -1) == 2);
  }

  static void testRemoveAndContains() {
    Preferences p = Preferences.open("demo4");
    p.edit().clear().putString("k1", "v1").putString("k2", "v2").commit();
    Preferences q = Preferences.open("demo4");
    check("contains before remove", q.contains("k1"));
    q.edit().remove("k1").commit();
    Preferences r = Preferences.open("demo4");
    check("missing after remove", !r.contains("k1"));
    check("sibling survives remove", r.contains("k2"));
  }

  static void testClear() {
    Preferences p = Preferences.open("demo5");
    p.edit().putString("a", "1").putString("b", "2").commit();
    p.edit().clear().commit();
    Preferences q = Preferences.open("demo5");
    check("cleared has no keys", q.getAllKeys().length == 0);
    check("cleared missing default", 99 == q.getInt("a", 99));
  }

  static void testPersistenceAcrossInstances() {
    Preferences p = Preferences.open("demo6");
    p.edit().clear().putString("greeting", "hello").putInt("n", 7).commit();
    String[] keys = Preferences.open("demo6").getAllKeys();
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
}
