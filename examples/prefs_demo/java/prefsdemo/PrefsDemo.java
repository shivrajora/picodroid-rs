// SPDX-License-Identifier: GPL-3.0-only
package prefsdemo;

import java.io.IOException;
import java.util.Map;
import picodroid.app.Application;
import picodroid.content.SharedPreferences;
import picodroid.io.File;
import picodroid.util.Log;

public class PrefsDemo extends Application {
  private static final String TAG = "PrefsDemo";

  // Intentionally no explicit = 0 initializers here — exercises the JVMS §5.5
  // step 2 static-field preparation path that sets primitives to typed zeros
  // before <clinit> runs.  A regression would manifest as an ArrayIOB /
  // InvalidBytecode on the first check() call (iadd on Null).
  static int passed;
  static int failed;

  /** {@code createNewFile} as a boolean: a refused or failed create counts as false here. */
  static boolean create(File f) {
    try {
      return f.createNewFile();
    } catch (IOException e) {
      Log.i(TAG, "createNewFile threw: " + e.getMessage());
      return false;
    }
  }

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
    testGetAll();
    testContextIdiomAndApply();
    testFloat();
    testFileHelpers();

    Log.i(TAG, "Results: " + passed + " passed, " + failed + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== SOME FAILED ===");
    }
  }

  /**
   * The app's one instance over {@code name}: {@code getSharedPreferences} hands every caller the
   * same object, as on Android, so a "second open" is that object again, and what these checks read
   * back is the instance's memory; the file is the cross-boot half (prefs_demo has none; qa_store's
   * "persist" section runs the app twice).
   */
  SharedPreferences prefs(String name) {
    return getSharedPreferences(name, MODE_PRIVATE);
  }

  void testWriteAndReadBack() {
    SharedPreferences p = prefs("demo1");
    p.edit().clear().commit();
    boolean ok =
        p.edit()
            .putString("ssid", "home-wifi")
            .putInt("channel", 6)
            .putLong("uptime", 123456789012L)
            .putBoolean("autoconnect", true)
            .commit();
    check("commit success", ok);
    check("commit wrote the file", new File("/prefs/demo1").length() > 0);

    SharedPreferences q = prefs("demo1");
    check("one instance per file", q == p);
    check("readback string", "home-wifi".equals(q.getString("ssid", "")));
    check("readback int", q.getInt("channel", -1) == 6);
    check("readback long", q.getLong("uptime", -1L) == 123456789012L);
    check("readback bool true", q.getBoolean("autoconnect", false));
    check("missing returns default", q.getInt("missing", 42) == 42);
  }

  void testTypeSafety() {
    SharedPreferences p = prefs("demo2");
    p.edit().clear().putInt("x", 5).commit();
    check("wrong-type string falls back", "def".equals(p.getString("x", "def")));
    check("correct-type int still works", p.getInt("x", -1) == 5);
  }

  void testUpdateExisting() {
    SharedPreferences p = prefs("demo3");
    p.edit().clear().putInt("count", 1).commit();
    boolean ok = p.edit().putInt("count", p.getInt("count", 0) + 1).commit();
    check("updated value", ok && prefs("demo3").getInt("count", -1) == 2);
  }

  void testRemoveAndContains() {
    SharedPreferences p = prefs("demo4");
    p.edit().clear().putString("k1", "v1").putString("k2", "v2").commit();
    check("contains before remove", p.contains("k1"));
    check("remove commits", p.edit().remove("k1").commit());
    check("missing after remove", !p.contains("k1"));
    check("sibling survives remove", p.contains("k2"));
    // Android: putString(key, null) is the same as remove(key).
    p.edit().putString("k2", null).commit();
    check("putString(null) removes", !p.contains("k2") && !p.getAll().containsKey("k2"));
  }

  void testClear() {
    SharedPreferences p = prefs("demo5");
    p.edit().putString("a", "1").putString("b", "2").commit();
    check("clear commits", p.edit().clear().commit());
    check("cleared has no keys", p.getAll().isEmpty());
    check("cleared missing default", 99 == p.getInt("a", 99));
  }

  void testGetAll() {
    SharedPreferences p = prefs("demo6");
    p.edit().clear().putString("greeting", "hello").putInt("n", 7).commit();
    // getAll() returns java.util.Map — an interface with no class file,
    // served by the HashMap builtin. Consume it through the interface.
    Map<String, ?> all = prefs("demo6").getAll();
    check("getAll size", all.size() == 2);
    check("getAll has greeting", all.containsKey("greeting"));
    check("getAll has n", all.containsKey("n"));
    check("getAll string value", "hello".equals(all.get("greeting")));
    check("getAll boxed int value", Integer.valueOf(7).equals(all.get("n")));
    boolean sawGreeting = false;
    boolean sawN = false;
    for (String k : all.keySet()) {
      if ("greeting".equals(k)) {
        sawGreeting = true;
      }
      if ("n".equals(k)) {
        sawN = true;
      }
    }
    check("getAll keySet has greeting", sawGreeting);
    check("getAll keySet has n", sawN);
  }

  /** The Android idiom end-to-end: context.getSharedPreferences(...).edit()...apply(). */
  void testContextIdiomAndApply() {
    SharedPreferences p = getSharedPreferences("demo7", MODE_PRIVATE);
    p.edit().clear().putString("source", "context").putInt("mode", MODE_PRIVATE).apply();
    SharedPreferences q = getSharedPreferences("demo7", MODE_PRIVATE);
    check("getSharedPreferences + apply persists", "context".equals(q.getString("source", "")));
    check("apply persisted int", q.getInt("mode", -1) == 0);
  }

  void testFloat() {
    SharedPreferences p = prefs("demo8");
    p.edit().clear().putFloat("ratio", 0.75f).putFloat("neg", -1.5e-3f).putInt("i", 3).commit();
    check("readback float", p.getFloat("ratio", 0f) == 0.75f);
    check("readback negative float", p.getFloat("neg", 0f) == -1.5e-3f);
    check("float wrong-type falls back", p.getFloat("i", 9f) == 9f);
    check("int does not read as float", p.getInt("ratio", -1) == -1);
    check("float missing default", p.getFloat("nope", 2.5f) == 2.5f);
    check("getAll boxes Float", Float.valueOf(0.75f).equals(p.getAll().get("ratio")));
    boolean ok = p.edit().putFloat("ratio", 1.25f).commit();
    check("float overwrite", ok && prefs("demo8").getFloat("ratio", 0f) == 1.25f);
  }

  static void testFileHelpers() {
    File f = new File("/pd_demo/sub/leaf.txt");
    check("getName", "leaf.txt".equals(f.getName()));
    check("getParent", "/pd_demo/sub".equals(f.getParent()));
    check("getParentFile path", "/pd_demo/sub".equals(f.getParentFile().getPath()));
    check("getAbsolutePath keeps rooted", "/pd_demo/sub/leaf.txt".equals(f.getAbsolutePath()));
    check("getAbsolutePath roots relative", "/rel".equals(new File("rel").getAbsolutePath()));
    check("getParent of root child", "/".equals(new File("/x").getParent()));
    check("getParent of bare name is null", new File("x").getParent() == null);
    check("getParentFile of bare name is null", new File("x").getParentFile() == null);

    File dir = f.getParentFile();
    new File("/pd_demo/sub/leaf.txt").delete();
    new File("/pd_demo/sub").delete();
    new File("/pd_demo").delete();
    check("mkdirs creates chain", dir.mkdirs());
    check("mkdirs result isDirectory", dir.isDirectory());
    check("mkdirs on existing is false", !dir.mkdirs());
    check("createNewFile creates", create(f));
    check("created file exists", f.exists() && f.isFile());
    check("created file is empty", f.length() == 0L);
    check("createNewFile on existing is false", !create(f));
    check("delete file", f.delete());
    check("delete dirs", new File("/pd_demo/sub").delete() && new File("/pd_demo").delete());
  }
}
