// SPDX-License-Identifier: GPL-3.0-only
package bundledemo;

import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.util.Log;

/**
 * Conformance app for {@code picodroid.os.Bundle}, {@code Intent.putExtras}/{@code getExtras}, and
 * the saved-instance-state round trip through {@code Activity.recreate()}. Runs without input and
 * ends in {@code === PASSED ===} or a {@code FAIL} line.
 */
public class BundleDemoApp extends Application {
  static final String TAG = "BundleDemo";
  static int failures;

  static void check(String what, boolean ok) {
    if (!ok) {
      failures++;
      Log.e(TAG, "FAIL " + what);
    }
  }

  @Override
  public void onCreate() {
    bundleBasics();
    intentExtras();
    Log.i(TAG, "bundle checks done, failures=" + failures);
    startActivity(new Intent(HomeActivity.class));
  }

  private static void bundleBasics() {
    Bundle b = new Bundle();
    check("new is empty", b.isEmpty() && b.size() == 0);
    b.putInt("i", 7);
    b.putLong("l", 1L << 40);
    b.putFloat("f", 1.5f);
    b.putDouble("d", 2.25);
    b.putBoolean("z", true);
    b.putString("s", "pico");
    b.putIntArray("ia", new int[] {1, 2, 3});
    b.putByteArray("ba", new byte[] {9});
    b.putStringArray("sa", new String[] {"x", "y"});
    Bundle inner = new Bundle();
    inner.putInt("depth", 2);
    b.putBundle("b", inner);
    check("size", b.size() == 10);
    check("getInt", b.getInt("i") == 7);
    check("getLong", b.getLong("l") == 1L << 40);
    check("getFloat", b.getFloat("f") == 1.5f);
    check("getDouble", b.getDouble("d") == 2.25);
    check("getBoolean", b.getBoolean("z"));
    check("getString", "pico".equals(b.getString("s")));
    check("getIntArray", b.getIntArray("ia") != null && b.getIntArray("ia")[2] == 3);
    check("getByteArray", b.getByteArray("ba") != null && b.getByteArray("ba")[0] == 9);
    check(
        "getStringArray", b.getStringArray("sa") != null && "y".equals(b.getStringArray("sa")[1]));
    check("getBundle", b.getBundle("b") != null && b.getBundle("b").getInt("depth") == 2);

    // Absent key, and a key holding another type, both give the default.
    check("absent int", b.getInt("nope") == 0 && b.getInt("nope", -1) == -1);
    check("absent string", b.getString("nope") == null && "d".equals(b.getString("nope", "d")));
    check("mismatch int", b.getInt("s", -2) == -2);
    check("mismatch string", b.getString("i") == null);
    check("mismatch long is not int", b.getLong("i", -3L) == -3L);
    check("mismatch array", b.getIntArray("sa") == null && b.getStringArray("ia") == null);
    check("mismatch bundle", b.getBundle("s") == null);
    check("get boxed", b.get("i") instanceof Integer && b.get("nope") == null);

    // Replace keeps the size and may change the type.
    b.putString("i", "seven");
    check("replace", b.size() == 10 && "seven".equals(b.getString("i")) && b.getInt("i", -1) == -1);

    // Insertion order, through keySet.
    StringBuilder order = new StringBuilder();
    for (String k : b.keySet()) {
      order.append(k).append(',');
    }
    check("keySet order " + order, "i,l,f,d,z,s,ia,ba,sa,b,".equals(order.toString()));

    b.remove("l");
    b.remove("nope");
    check("remove", b.size() == 9 && !b.containsKey("l") && b.containsKey("f"));
    check("remove shifts", b.getFloat("f") == 1.5f && b.getBundle("b") == inner);

    // Copy constructor and putAll are shallow copies that do not alias the table.
    Bundle copy = new Bundle(b);
    copy.putInt("extra", 1);
    check("copy", copy.size() == 10 && b.size() == 9 && copy.getBundle("b") == inner);
    Bundle merged = new Bundle();
    merged.putString("s", "old");
    merged.putAll(b);
    check("putAll", merged.size() == 9 && "pico".equals(merged.getString("s")));

    // Growth past the first table, then clear.
    Bundle big = new Bundle();
    for (int i = 0; i < 40; i++) {
      big.putInt("k" + i, i * i);
    }
    boolean all = big.size() == 40;
    for (int i = 0; i < 40; i++) {
      all &= big.getInt("k" + i, -1) == i * i;
    }
    check("growth", all);
    big.clear();
    check("clear", big.isEmpty() && !big.containsKey("k3"));
    big.putInt("again", 1);
    check("put after clear", big.getInt("again") == 1);
  }

  private static void intentExtras() {
    Intent plain = new Intent(HomeActivity.class);
    check("no extras is null", plain.getExtras() == null && !plain.hasExtra("x"));
    check(
        "no extras defaults", plain.getIntExtra("x", 5) == 5 && plain.getStringExtra("x") == null);

    Intent in = new Intent(HomeActivity.class).putExtra("n", 3).putExtra("s", "str");
    in.putExtra("z", true).putExtra("l", 99L);
    Bundle more = new Bundle();
    more.putInt("n", 4);
    more.putDouble("d", 0.5);
    in.putExtras(more);
    check("typed extras", in.getIntExtra("n", 0) == 4 && "str".equals(in.getStringExtra("s")));
    check("bool/long extras", in.getBooleanExtra("z", false) && in.getLongExtra("l", 0L) == 99L);
    check("extra mismatch", in.getIntExtra("s", -1) == -1 && in.getStringExtra("n") == null);

    Bundle out = in.getExtras();
    check("getExtras", out != null && out.size() == 5 && out.getDouble("d") == 0.5);
    // getExtras is a copy, as on Android: writing to it leaves the Intent alone.
    out.putInt("n", 100);
    check("getExtras copies", in.getIntExtra("n", 0) == 4);

    in.putExtra("nested", more);
    check("bundle extra", in.getBundleExtra("nested") == more && in.getBundleExtra("n") == null);
  }
}
