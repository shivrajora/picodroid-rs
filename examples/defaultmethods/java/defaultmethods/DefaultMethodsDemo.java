// SPDX-License-Identifier: GPL-3.0-only
package defaultmethods;

import java.util.Iterator;
import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Interface default methods (JVMS §5.4.3.3 superinterface resolution): a class without an override,
 * a class override, a sub-interface override (whatever the {@code implements} order), a diamond
 * resolved with {@code I.super.f()}, defaults reached through abstract and builtin superclasses, a
 * default calling an abstract method on {@code this}, an interface static method, and a user {@code
 * Iterable} driven by the enhanced {@code for} loop. This is the shape kotlinc emits under {@code
 * -Xjvm-default=all}.
 */
public class DefaultMethodsDemo extends Application {
  private static final String TAG = "DefaultMethods";

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

  interface Describable {
    default String describe() {
      return "describable";
    }
  }

  interface Tagged {
    String tag();

    default String describe() {
      return "tagged:" + tag();
    }
  }

  /** Diamond: both defaults conflict, so the class must override; it calls both supers. */
  static class Both implements Describable, Tagged {
    @Override
    public String tag() {
      return "b";
    }

    @Override
    public String describe() {
      return Describable.super.describe() + "+" + Tagged.super.describe();
    }
  }

  static class OnlyDefault implements Describable {}

  static class Overrider implements Describable {
    @Override
    public String describe() {
      return "overridden";
    }
  }

  interface Sub extends Describable {
    @Override
    default String describe() {
      return "sub";
    }
  }

  static class ViaSub implements Sub {}

  /**
   * {@code Sub.describe} is the maximally-specific method even though {@code Describable} is listed
   * first.
   */
  static class ViaBoth implements Describable, Sub {}

  interface Untouched extends Describable {}

  /** A diamond with a single default (the other branch inherits it) is not a conflict. */
  static class SingleDefaultDiamond implements Describable, Untouched {}

  abstract static class Base implements Describable {}

  static class Leaf extends Base {}

  static class Deeper extends Leaf {}

  /** The superclass chain leaves the loaded set (a builtin parent); defaults still resolve. */
  static class DescribedException extends RuntimeException implements Describable {
    DescribedException(String message) {
      super(message);
    }
  }

  interface Counter {
    int next();

    /** A default calling the abstract method on {@code this}, and another default. */
    default int skip(int n) {
      int last = 0;
      for (int i = 0; i < n; i++) {
        last = next();
      }
      return last + bonus();
    }

    default int bonus() {
      return 100;
    }
  }

  static class Up implements Counter {
    int c = 0;

    @Override
    public int next() {
      c = c + 1;
      return c;
    }
  }

  interface WithStatic {
    static int twice(int x) {
      return x * 2;
    }

    default int plusTwice(int x) {
      return x + twice(x);
    }
  }

  static class UsesStatic implements WithStatic {}

  /** A user Iterable: the enhanced for loop calls {@code Iterable.iterator()} on it. */
  static class Range implements Iterable<Integer> {
    final int lo;
    final int hi;

    Range(int lo, int hi) {
      this.lo = lo;
      this.hi = hi;
    }

    @Override
    public Iterator<Integer> iterator() {
      return new RangeIterator(lo, hi);
    }
  }

  static class RangeIterator implements Iterator<Integer> {
    int cur;
    final int end;

    RangeIterator(int lo, int hi) {
      this.cur = lo;
      this.end = hi;
    }

    @Override
    public boolean hasNext() {
      return cur < end;
    }

    @Override
    public Integer next() {
      int v = cur;
      cur = cur + 1;
      return Integer.valueOf(v);
    }
  }

  @Override
  public void onCreate() {
    run();
  }

  public static void run() {
    Log.i(TAG, "=== Default Method Tests ===");

    testResolution();
    testSuperCalls();
    testThroughSuperclasses();
    testDefaultsCallingThis();
    testUserIterable();

    String passStr = String.valueOf(passed);
    String failStr = String.valueOf(failed);
    Log.i(TAG, "Results: " + passStr + " passed, " + failStr + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== SOME FAILED ===");
    }
  }

  static void testResolution() {
    check("default without override", new OnlyDefault().describe().equals("describable"));
    Describable viaIface = new OnlyDefault();
    check("default via interface ref", viaIface.describe().equals("describable"));
    check("class override wins", new Overrider().describe().equals("overridden"));
    check("sub-interface default", new ViaSub().describe().equals("sub"));
    check("most specific wins over order", new ViaBoth().describe().equals("sub"));
    check("single-default diamond", new SingleDefaultDiamond().describe().equals("describable"));
    check("instanceof through default iface", viaIface instanceof Describable);
  }

  static void testSuperCalls() {
    check("I.super.f() in a diamond", new Both().describe().equals("describable+tagged:b"));
    Tagged t = new Both();
    check("diamond via other iface", t.describe().equals("describable+tagged:b"));
  }

  static void testThroughSuperclasses() {
    check("through abstract class", new Leaf().describe().equals("describable"));
    check("two levels down", new Deeper().describe().equals("describable"));
    DescribedException ex = new DescribedException("boom");
    check("through builtin superclass", ex.describe().equals("describable"));
    check("builtin superclass still works", ex.getMessage().equals("boom"));
    boolean caught = false;
    try {
      throw ex;
    } catch (RuntimeException e) {
      caught = ((Describable) e).describe().equals("describable");
    }
    check("caught + default on catch var", caught);
  }

  static void testDefaultsCallingThis() {
    check("default calls abstract on this", new Up().skip(3) == 103);
    Counter c = new Up();
    c.next();
    check("default sees state", c.skip(2) == 103);
    check("interface static", WithStatic.twice(21) == 42);
    check("default calls interface static", new UsesStatic().plusTwice(5) == 15);
  }

  static void testUserIterable() {
    int sum = 0;
    int n = 0;
    for (int v : new Range(1, 5)) {
      sum = sum + v;
      n = n + 1;
    }
    check("for-each over user Iterable", n == 4 && sum == 10);
    Iterable<Integer> it = new Range(0, 0);
    check("empty user Iterable", !it.iterator().hasNext());
  }
}
