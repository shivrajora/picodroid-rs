// SPDX-License-Identifier: GPL-3.0-only
package bugbash;

import java.util.ArrayList;
import java.util.Arrays;
import picodroid.app.Application;
import picodroid.content.SharedPreferences;
import picodroid.io.FileInputStream;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * End-to-end regression checks for the 2026-08-30 bug bash (docs/bugbash-2026-08-30.md). Each check
 * names the bash id it pins; the unit tests next to each fix are the primary guard, this app proves
 * the same behaviour through real bytecode on the sim and on hardware.
 */
public class BugBash extends Application {
  private static final String TAG = "BugBash";

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

  static class Item {
    final String name;

    Item(String name) {
      this.name = name;
    }

    @Override
    public String toString() {
      return "Item(" + name + ")";
    }
  }

  @Override
  public void onCreate() {
    Log.i(TAG, "=== BugBash start ===");
    numbers();
    strings();
    collections();
    arrays();
    io();
    prefs();
    Log.i(TAG, "passed=" + passed + " failed=" + failed);
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED: " + failed + " ===");
    }
  }

  static void numbers() {
    // J1: decimal formatting of MIN_VALUE.
    check("J1 int MIN toString", ("" + Integer.MIN_VALUE).equals("-2147483648"));
    check("J1 long MIN toString", Long.toString(Long.MIN_VALUE).equals("-9223372036854775808"));
    // J3: Math edge cases.
    check("J3 abs(MIN)", Math.abs(Integer.MIN_VALUE) == Integer.MIN_VALUE);
    check("J3 abs(long MIN)", Math.abs(Long.MIN_VALUE) == Long.MIN_VALUE);
    check("J3 round(-2.5)", Math.round(-2.5) == -2L);
    check("J3 round(-2.5f)", Math.round(-2.5f) == -2);
    check("J3 round(-0.5)", Math.round(-0.5) == 0L);
    double mn = Math.min(Double.NaN, 1.0);
    check("J3 min(NaN, 1)", String.valueOf(mn).equals("NaN"));
    check("J3 max(-0.0, 0.0)", 1.0 / Math.max(-0.0, 0.0) > 0);
    // J6 / J14: double stringification keeps double precision and Java layout.
    check("J6 Double.toString(1.5)", Double.toString(1.5).equals("1.5"));
    check("J6 concat 1e10", ("" + 1e10).equals("1.0E10"));
    check("J6 valueOf(1/3)", String.valueOf(1.0 / 3.0).equals("0.3333333333333333"));
    Double boxed = 2.5;
    check("J6 boxed Double toString", boxed.toString().equals("2.5"));
    check("J9 Float.toString(1e10f)", Float.toString(1e10f).equals("1.0E10"));
    check("J9 float 1/3", String.valueOf(1.0f / 3.0f).equals("0.33333334"));
    check("J14 %f Infinity", String.format("%f", Double.POSITIVE_INFINITY).equals("Infinity"));
    check("J14 %s double", String.format("%s", 1.0 / 3.0).equals("0.3333333333333333"));
    check("J14 %.2f", String.format("%.2f", 3.14159).equals("3.14"));
  }

  static void strings() {
    // J7: control characters survive String.valueOf(char).
    String nl = String.valueOf('\n');
    check("J7 valueOf('\\n')", nl.length() == 1 && nl.charAt(0) == '\n');
    // J8: trailing empties dropped by split.
    check("J8 split trailing", "a,b,,".split(",").length == 2);
    check("J8 split all-empty", ",,".split(",").length == 0);
    check("J8 split interior", "a,,b".split(",").length == 3);
    // J10: fromIndex overloads and charAt bounds.
    check("J10 indexOf from", "abcabc".indexOf("a", 1) == 3);
    check("J10 indexOf char from", "abcabc".indexOf('c', 3) == 5);
    check("J10 lastIndexOf from", "abcabc".lastIndexOf('a', 2) == 0);
    check("J10 startsWith offset", "hello".startsWith("ll", 2));
    int hits = 0;
    int i = -1;
    while ((i = "a.b.c".indexOf('.', i + 1)) >= 0) {
      hits++;
      if (hits > 5) {
        break;
      }
    }
    check("J10 tokenizer loop terminates", hits == 2);
    boolean threw = false;
    char sink = ' ';
    try {
      sink = "abc".charAt(5);
    } catch (StringIndexOutOfBoundsException e) {
      threw = true;
    }
    check("J10 charAt OOB throws", threw && sink == ' ');
    threw = false;
    try {
      sink = new StringBuilder("x").charAt(-1);
    } catch (StringIndexOutOfBoundsException e) {
      threw = true;
    }
    check("J10 StringBuilder.charAt OOB throws", threw);
    // J11: equals against non-strings is false, not fatal.
    Object o = new Object();
    check("J11 equals(Object)", !"x".equals(o));
    Object none = null;
    check("J11 equals(null)", !"x".equals(none));
    check("J11 equals(int[])", !"x".equals(new int[1]));
  }

  static void collections() {
    // J2: the first-allocated object (this Application lives in slot 0)
    // survives a round trip through an Object[].
    Object[] slot0 = new Object[] {BugBash.current};
    check("J2 slot-0 object in Object[]", slot0[0] != null && slot0[0] == BugBash.current);
    ArrayList<Object> holder = new ArrayList<Object>();
    holder.add(BugBash.current);
    check("J2 toArray slot-0", holder.toArray()[0] == BugBash.current);
    // S1: remove(Object).
    ArrayList<String> l = new ArrayList<String>();
    l.add("a");
    l.add("b");
    check("S1 remove(Object) true", l.remove("a") && l.size() == 1);
    check("S1 remove(Object) false", !l.remove("zzz") && l.size() == 1);
    ArrayList<Integer> li = new ArrayList<Integer>();
    li.add(5);
    li.add(6);
    check("S1 remove(Integer)", li.remove(Integer.valueOf(5)) && li.size() == 1 && li.get(0) == 6);
    check("S1 remove(int) index", li.remove(0) == 6 && li.isEmpty());
    // S2: contains sees content, not string identity.
    String b = String.valueOf('b');
    String dyn = "a" + b;
    l.add("ab");
    check("S2 contains dynamic string", l.contains(dyn));
    check("S2 remove dynamic string", l.remove(dyn));
    // S3: index bounds throw IndexOutOfBoundsException.
    boolean threw = false;
    try {
      l.add(-1, "x");
    } catch (IndexOutOfBoundsException e) {
      threw = true;
    }
    check("S3 add(-1) throws", threw && l.size() == 1);
    threw = false;
    try {
      l.get(99);
    } catch (IndexOutOfBoundsException e) {
      threw = true;
    }
    check("S3 get(99) throws", threw);
    threw = false;
    try {
      l.set(7, "y");
    } catch (IndexOutOfBoundsException e) {
      threw = true;
    }
    check("S3 set(7) throws", threw && l.size() == 1);
    l.add(1, "z");
    check("S3 add(size) appends", l.size() == 2 && l.get(1).equals("z"));
  }

  static void arrays() {
    boolean[] flags = new boolean[3];
    Arrays.fill(flags, true);
    check("J13 fill(boolean[])", flags[0] && flags[1] && flags[2]);
    flags[1] = false;
    check("J13 toString(boolean[])", Arrays.toString(flags).equals("[true, false, true]"));
    check("S5 toString(char[])", Arrays.toString(new char[] {'a', 'b'}).equals("[a, b]"));
    int[] a = new int[5];
    Arrays.fill(a, 1, 3, 9);
    check("S5 fill range", a[0] == 0 && a[1] == 9 && a[2] == 9 && a[3] == 0 && a[4] == 0);
    int[] s = {5, 4, 3, 2, 1};
    Arrays.sort(s, 1, 4);
    check("S5 sort range", s[0] == 5 && s[1] == 2 && s[2] == 3 && s[3] == 4 && s[4] == 1);
    boolean threw = false;
    try {
      Arrays.fill(a, 3, 1, 0);
    } catch (IllegalArgumentException e) {
      threw = true;
    }
    check("S5 fill from>to throws IAE", threw);
    threw = false;
    try {
      Arrays.fill(a, 0, 9, 0);
    } catch (ArrayIndexOutOfBoundsException e) {
      threw = true;
    }
    check("S5 fill past end throws AIOOBE", threw);
    Object[] objs = new Object[2];
    Arrays.fill(objs, "q");
    check("S5 fill(Object[])", "q".equals(objs[1]));
    double[] d = {1.0 / 3.0};
    check("J14 toString(double[])", Arrays.toString(d).equals("[0.3333333333333333]"));
  }

  static void io() {
    // F5: a negative sleep returns immediately.
    long t0 = SystemClock.elapsedRealtimeNanos();
    SystemClock.sleep(-1);
    SystemClock.sleep(0);
    long dt = SystemClock.elapsedRealtimeNanos() - t0;
    check("F5 sleep(-1) returns at once", dt < 500_000_000L);
    // F6: bad read window throws instead of panicking.
    boolean threw = false;
    try {
      new FileInputStream("/bugbash-none").read(new byte[4], 0, -1);
    } catch (IndexOutOfBoundsException e) {
      threw = true;
    }
    check("F6 read(len=-1) throws", threw);
    threw = false;
    try {
      new FileInputStream("/bugbash-none").read(new byte[4], 3, 2);
    } catch (IndexOutOfBoundsException e) {
      threw = true;
    }
    check("F6 read(off+len>buf) throws", threw);
  }

  static void prefs() {
    SharedPreferences p = SharedPreferences.open("bugbash");
    p.edit().clear().commit();
    // F9: a reused Editor must not alias the committed state.
    SharedPreferences.Editor e = p.edit();
    e.putInt("n", 1);
    e.commit();
    e.putInt("n", 999);
    check("F9 reused editor does not leak before commit", p.getInt("n", 0) == 1);
    e.commit();
    check("F9 reused editor commits on request", p.getInt("n", 0) == 999);
    // S7: clear() is applied before the pending puts at commit (Android).
    p.edit().putString("a", "1").clear().commit();
    check("S7 put-then-clear keeps put", "1".equals(p.getString("a", null)));
    p.edit().clear().putString("b", "2").commit();
    check("S7 clear-then-put", p.getString("b", null) != null && !p.contains("a"));
    p.edit().clear().commit();
  }

  /** The Application instance — object slot 0 in a fresh heap (J2). */
  static Object current;

  public BugBash() {
    current = this;
  }
}
