// SPDX-License-Identifier: GPL-3.0-only
package rttidemo;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;
import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Runtime type information conformance: {@code instanceof}/{@code checkcast} on strings, arrays,
 * builtin collections under their interfaces and boxes under {@code Number}; transitive
 * superinterfaces; a catchable {@code ClassCastException}; the boxed {@code equals}/{@code
 * hashCode}/{@code compare} family; identity {@code equals}/{@code hashCode}/{@code toString};
 * {@code Comparable} on strings through {@code Arrays.sort(Object[])}; enum identity.
 */
public class RttiDemo extends Application {
  private static final String TAG = "RttiDemo";

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

  interface Shape {}

  interface Named extends Shape {
    String name();
  }

  static class Circle implements Named {
    @Override
    public String name() {
      return "circle";
    }
  }

  static class Plain {}

  /** Overrides win over the identity fallbacks. */
  static class Point {
    final int x;
    final int y;

    Point(int x, int y) {
      this.x = x;
      this.y = y;
    }

    @Override
    public boolean equals(Object o) {
      if (!(o instanceof Point)) {
        return false;
      }
      Point p = (Point) o;
      return x == p.x && y == p.y;
    }

    @Override
    public int hashCode() {
      return 31 * x + y;
    }

    @Override
    public String toString() {
      return "(" + x + "," + y + ")";
    }
  }

  enum Color {
    RED,
    GREEN,
    BLUE
  }

  @Override
  public void onCreate() {
    run();
  }

  public static void run() {
    Log.i(TAG, "=== RTTI Tests ===");

    testStrings();
    testArrays();
    testCollections();
    testBoxed();
    testIdentity();
    testInterfaces();
    testClassCast();
    testEnum();

    String passStr = String.valueOf(passed);
    String failStr = String.valueOf(failed);
    Log.i(TAG, "Results: " + passStr + " passed, " + failStr + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILURES ===");
    }
  }

  static void testStrings() {
    Object s = "hello";
    check("String instanceof String", s instanceof String);
    check("String instanceof CharSequence", s instanceof CharSequence);
    check("String instanceof Comparable", s instanceof Comparable);
    check("String not instanceof List", !(s instanceof List));
    CharSequence cs = (CharSequence) s;
    check("CharSequence.length on a String", cs.length() == 5);
    check("Object.equals on a String", s.equals("hello"));
    check("Object.hashCode on a String", s.hashCode() == "hello".hashCode());
    check("Object.toString on a String", s.toString().equals("hello"));

    // Arrays.sort(Object[]) compares through Comparable.compareTo.
    String[] words = {"pear", "apple", "fig"};
    Arrays.sort(words);
    check("Comparable on strings via Arrays.sort", words[0].equals("apple"));
    check("Comparable on strings via Arrays.sort (last)", words[2].equals("pear"));
  }

  @SuppressWarnings("BadInstanceof") // the always-true cast is the point
  static void testArrays() {
    Object ints = new int[3];
    check("int[] instanceof int[]", ints instanceof int[]);
    check("int[] not instanceof float[]", !(ints instanceof float[]));
    check("int[] instanceof Object", ints instanceof Object);
    int[] back = (int[]) ints;
    check("checkcast int[]", back.length == 3);

    String[] names = {"b", "a"};
    Object refs = names;
    check("String[] instanceof String[]", refs instanceof String[]);
    check("String[] instanceof Object[]", refs instanceof Object[]);
    check("String[] not instanceof int[]", !(refs instanceof int[]));
    Object[] objs = (Object[]) refs;
    check("checkcast Object[]", objs.length == 2);
  }

  static void testCollections() {
    ArrayList<String> al = new ArrayList<>();
    al.add("x");
    Object o = al;
    check("ArrayList instanceof List", o instanceof List);
    check("ArrayList instanceof Collection", o instanceof Collection);
    check("ArrayList instanceof Iterable", o instanceof Iterable);
    check("ArrayList not instanceof Map", !(o instanceof Map));
    List<?> l = (List<?>) o;
    check("List.size through the cast", l.size() == 1);
    Collection<?> c = (Collection<?>) o;
    check("Collection.isEmpty through the cast", !c.isEmpty());
    Iterable<?> it = (Iterable<?>) o;
    Iterator<?> iter = it.iterator();
    check("Iterable.iterator through the cast", iter.hasNext() && iter.next().equals("x"));
    Object io = iter;
    check("Iterator instanceof Iterator", io instanceof Iterator);

    HashMap<String, Integer> hm = new HashMap<>();
    hm.put("k", 1);
    Object mo = hm;
    check("HashMap instanceof Map", mo instanceof Map);
    check("HashMap not instanceof Collection", !(mo instanceof Collection));
    Map<?, ?> m = (Map<?, ?>) mo;
    check("Map.size through the cast", m.size() == 1);
    Object ks = hm.keySet();
    check("keySet instanceof Set", ks instanceof Set);
    check("keySet instanceof Collection", ks instanceof Collection);
    Object vs = hm.values();
    check("values instanceof Collection", vs instanceof Collection);
    check("values not instanceof Set", !(vs instanceof Set));

    HashSet<String> hs = new HashSet<>();
    Object so = hs;
    check("HashSet instanceof Set", so instanceof Set);
    check("HashSet instanceof Iterable", so instanceof Iterable);
    check("HashSet not instanceof List", !(so instanceof List));
  }

  @SuppressWarnings("EqualsIncompatibleType") // Integer.equals(Long) must be false
  static void testBoxed() {
    Object i = Integer.valueOf(5);
    check("Integer instanceof Number", i instanceof Number);
    check("Integer instanceof Integer", i instanceof Integer);
    check("Integer instanceof Comparable", i instanceof Comparable);
    check("Integer not instanceof Long", !(i instanceof Long));
    Number n = (Number) i;
    check("Number.intValue", n.intValue() == 5);
    Object f = Float.valueOf(2.75f);
    check("Float instanceof Number", f instanceof Number);
    check("Number.floatValue on a Float", ((Number) f).floatValue() == 2.75f);

    check("Integer.equals same value", Integer.valueOf(7).equals(Integer.valueOf(7)));
    check("Integer.equals other value", !Integer.valueOf(7).equals(Integer.valueOf(8)));
    check("Integer.equals other class", !Integer.valueOf(7).equals(Long.valueOf(7)));
    check("Integer.hashCode()", Integer.valueOf(7).hashCode() == 7);
    check("Integer.hashCode(int)", Integer.hashCode(-3) == -3);
    check("Integer.compare", Integer.compare(3, 3) == 0 && Integer.compare(2, 3) < 0);
    check("Integer.compareTo", Integer.valueOf(9).compareTo(Integer.valueOf(4)) > 0);
    check("Long.hashCode(long)", Long.hashCode((1L << 32) | 5L) == 4);
    check("Long.compare", Long.compare(-1L, 1L) < 0);
    check("Float.compare", Float.compare(1.0f, 2.0f) < 0 && Float.compare(2.0f, 2.0f) == 0);
    check("Float.compare NaN is greatest", Float.compare(Float.NaN, 1.0e30f) > 0);
    check("Float.compare -0.0 < 0.0", Float.compare(-0.0f, 0.0f) < 0);
    check("Float.hashCode(float)", Float.hashCode(1.0f) == Float.floatToIntBits(1.0f));
    check("Float.floatToIntBits", Float.floatToIntBits(1.0f) == 0x3f800000);
    check("Float.equals", Float.valueOf(1.5f).equals(Float.valueOf(1.5f)));
    check("Double.compare", Double.compare(0.5, 0.25) > 0);
    check("Double.hashCode(double)", Double.hashCode(1.0) == 0x3ff00000);
    check("Boolean.hashCode", Boolean.hashCode(true) == 1231 && Boolean.hashCode(false) == 1237);
    check("Boolean.equals", Boolean.valueOf(true).equals(Boolean.valueOf(true)));
    check("Character.isDigit", Character.isDigit('7') && !Character.isDigit('x'));
    check("Character.isLetter", Character.isLetter('q') && !Character.isLetter('1'));
    check("Character.toUpperCase", Character.toUpperCase('a') == 'A');
    check("Character.toLowerCase", Character.toLowerCase('Q') == 'q');

    HashMap<Float, String> byFloat = new HashMap<>();
    byFloat.put(1.5f, "one-and-a-half");
    check("boxed Float map key", "one-and-a-half".equals(byFloat.get(1.5f)));
  }

  static void testIdentity() {
    Plain p = new Plain();
    Object q = p;
    check("identity equals self", p.equals(q));
    check("identity equals other", !p.equals(new Plain()));
    check("identity hashCode stable", p.hashCode() == q.hashCode());
    check("identity hashCode differs", p.hashCode() != new Plain().hashCode());
    String s = p.toString();
    check("identity toString shape", s.startsWith("rttidemo.RttiDemo$Plain@"));
    Object arr = new int[2];
    Object same = arr;
    Object other = new int[2];
    check("array identity equals", arr.equals(same) && !arr.equals(other));
    check("array identity hashCode", arr.hashCode() == same.hashCode());

    Point a = new Point(1, 2);
    Point b = new Point(1, 2);
    check("override equals wins", a.equals(b));
    check("override hashCode wins", a.hashCode() == 33 && b.hashCode() == 33);
    check("override toString wins", a.toString().equals("(1,2)"));
  }

  @SuppressWarnings("BadInstanceof")
  static void testInterfaces() {
    Object c = new Circle();
    check("class instanceof interface", c instanceof Named);
    check("class instanceof superinterface", c instanceof Shape);
    check("class not instanceof Runnable", !(c instanceof Runnable));
    Shape sh = (Shape) c;
    Named nm = (Named) sh;
    check("checkcast through superinterface", nm.name().equals("circle"));

    Runnable r = () -> Log.i(TAG, "lambda ran");
    Object ro = r;
    check("lambda instanceof its interface", ro instanceof Runnable);
    check("lambda instanceof Object", ro instanceof Object);
    check("lambda not instanceof Shape", !(ro instanceof Shape));
    ((Runnable) ro).run();
  }

  static List<?> asList(Object o) {
    return (List<?>) o;
  }

  static Integer asInteger(Object o) {
    return (Integer) o;
  }

  static Runnable asRunnable(Object o) {
    return (Runnable) o;
  }

  static void testClassCast() {
    boolean threw = false;
    String message = null;
    try {
      List<?> l = asList("str");
      threw = l == null;
    } catch (ClassCastException e) {
      threw = true;
      message = e.getMessage();
    }
    check("String -> List throws ClassCastException", threw);
    check("ClassCastException message is null", message == null);

    threw = false;
    try {
      Integer i = asInteger(Long.valueOf(1));
      threw = i == null;
    } catch (RuntimeException e) {
      threw = e instanceof ClassCastException;
    }
    check("Long -> Integer caught as RuntimeException", threw);

    threw = false;
    try {
      Runnable r = asRunnable(new Plain());
      threw = r == null;
    } catch (ClassCastException e) {
      threw = true;
    }
    check("Plain -> Runnable throws", threw);

    Object nothing = null;
    List<?> ln = asList(nothing);
    check("null passes any checkcast", ln == null);
    Integer stillNull = asInteger(nothing);
    check("null passes boxed checkcast", stillNull == null);
  }

  static void testEnum() {
    check("Enum.hashCode differs per constant", Color.RED.hashCode() != Color.BLUE.hashCode());
    Object e = Color.BLUE;
    check("enum instanceof Enum", e instanceof Enum);
    check("enum instanceof Comparable", e instanceof Comparable);
    check("enum instanceof its class", e instanceof Color);
    HashMap<Color, String> byColor = new HashMap<>();
    byColor.put(Color.RED, "r");
    check("enum map key", "r".equals(byColor.get(Color.RED)));
  }
}
