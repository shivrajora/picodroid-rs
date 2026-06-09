// SPDX-License-Identifier: GPL-3.0-only
package collectionsdemo;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import picodroid.app.Application;
import picodroid.util.Log;

public class CollectionsDemo extends Application {
  private static final String TAG = "CollectionsDemo";

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

  static class Item implements Comparable<Item> {
    final String name;
    final int score;

    Item(String name, int score) {
      this.name = name;
      this.score = score;
    }

    @Override
    public int compareTo(Item other) {
      return this.score - other.score;
    }

    @Override
    public String toString() {
      return name + "(" + score + ")";
    }
  }

  @Override
  public void onCreate() {
    run();
  }

  public static void run() {
    Log.i(TAG, "=== Collections Tests ===");

    testArraysSortPrimitives();
    testArraysCopyAndFill();
    testArrayListBasics();
    testArrayListAutoboxing();
    testArraysSortObjects();
    testCollectionsSortAndReverse();
    testExplicitIterator();
    testForEachArrayList();
    testForEachHashMapKeys();
    testForEachHashMapValues();
    testIteratorOnEmpty();
    testNestedForEach();
    testHashMapIntegerKeys();
    testHashMapOverwrite();
    testHashMapRemove();
    testHashMapContainsKey();
    testHashMapContainsValue();
    testHashMapSizeAndEmpty();
    testHashMapClear();
    testHashMapGetOrDefault();
    testHashMapStringKeys();
    testHashSetBasic();
    testHashSetDuplicate();
    testHashSetClear();

    String passStr = String.valueOf(passed);
    String failStr = String.valueOf(failed);
    Log.i(TAG, "Results: " + passStr + " passed, " + failStr + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== SOME FAILED ===");
    }
  }

  // ── java.util.Arrays — primitive sorts ────────────────────────────────────

  static void testArraysSortPrimitives() {
    int[] xs = {5, 3, 8, 1, 9, 2, 7};
    Arrays.sort(xs);
    check("sort int ascending", Arrays.toString(xs).equals("[1, 2, 3, 5, 7, 8, 9]"));

    long[] ys = {3L, -10L, 0L, 7L, -1L};
    Arrays.sort(ys);
    check("sort long ascending", Arrays.toString(ys).equals("[-10, -1, 0, 3, 7]"));

    double[] ds = {2.5, 1.1, -0.5, 4.0};
    Arrays.sort(ds);
    check("sort double ascending", ds[0] == -0.5 && ds[3] == 4.0);

    byte[] bs = {-128, 0, 127, -1};
    Arrays.sort(bs);
    check("sort byte ascending", bs[0] == -128 && bs[1] == -1 && bs[2] == 0 && bs[3] == 127);
  }

  static void testArraysCopyAndFill() {
    int[] src = {1, 2, 3, 5, 7, 8, 9};
    int[] grown = Arrays.copyOf(src, 10);
    check("copyOf length", grown.length == 10);
    check("copyOf head intact", grown[0] == 1 && grown[6] == 9);
    check("copyOf tail zeroed", grown[7] == 0 && grown[8] == 0 && grown[9] == 0);

    int[] filled = new int[5];
    Arrays.fill(filled, 42);
    check("fill all 42", filled[0] == 42 && filled[2] == 42 && filled[4] == 42);
  }

  // ── java.util.ArrayList ──────────────────────────────────────────────────

  static void testArrayListBasics() {
    // NOTE: where check() reads list state into a local first, that's deliberate.
    // The JVM uses one shared StringBuilder buffer; capturing values before
    // string concatenation avoids buffer-aliasing artifacts in the assertion
    // names. (See JVM/string handling in stringdemo for the StringBuilder rules.)
    ArrayList list = new ArrayList();
    list.add("alpha");
    list.add("beta");
    list.add("gamma");

    check("size after 3 add", list.size() == 3);
    check("not empty", !list.isEmpty());

    String item = (String) list.get(1);
    check("get(1) = beta", item.equals("beta"));

    String alphaUpper = "ALPHA";
    String old = (String) list.set(0, alphaUpper);
    check("set(0) returns old alpha", old.equals("alpha"));
    String got = (String) list.get(0);
    check("get(0) = ALPHA", got.equals(alphaUpper));

    String removed = (String) list.remove(2);
    check("remove(2) returns gamma", removed.equals("gamma"));
    check("size after remove = 2", list.size() == 2);

    // contains() uses reference equality for non-wrapper objects on this JVM,
    // so probe with the same reference we inserted, not a literal.
    check("contains ALPHA", list.contains(alphaUpper));
    check("not contains gamma", !list.contains("gamma"));

    list.clear();
    check("size after clear = 0", list.size() == 0);
    check("isEmpty after clear", list.isEmpty());
  }

  static void testArrayListAutoboxing() {
    ArrayList<Integer> nums = new ArrayList<Integer>();
    nums.add(10);
    nums.add(20);
    nums.add(30);
    check("Integer autobox size", nums.size() == 3);

    int n = nums.get(0);
    check("Integer autobox get(0) = 10", n == 10);
    check("Integer contains 20", nums.contains(20));
    check("Integer not contains 99", !nums.contains(99));

    nums.remove(1);
    check("Integer size after remove = 2", nums.size() == 2);

    ArrayList<Boolean> flags = new ArrayList<Boolean>();
    flags.add(true);
    flags.add(false);
    flags.add(true);
    boolean f0 = flags.get(0);
    boolean f1 = flags.get(1);
    check("Boolean autobox get(0) = true", f0);
    check("Boolean autobox get(1) = false", !f1);
  }

  // ── java.util.Arrays / java.util.Collections — Object sorts ──────────────

  static void testArraysSortObjects() {
    // Arrays.sort(Object[]) — Java-side mergesort using Comparable.
    Item[] items = {
      new Item("alpha", 30), new Item("bravo", 10), new Item("charlie", 50), new Item("delta", 20),
    };
    Arrays.sort(items);
    check("Arrays.sort Object[] [0]", items[0].name.equals("bravo"));
    check("Arrays.sort Object[] [1]", items[1].name.equals("delta"));
    check("Arrays.sort Object[] [2]", items[2].name.equals("alpha"));
    check("Arrays.sort Object[] [3]", items[3].name.equals("charlie"));
  }

  static void testCollectionsSortAndReverse() {
    // Collections.sort(List) — copies into Object[], delegates to Arrays.sort.
    ArrayList<Item> list = new ArrayList<Item>();
    list.add(new Item("zeta", 90));
    list.add(new Item("yota", 5));
    list.add(new Item("xena", 60));
    Collections.sort(list);
    check("Collections.sort [0] = yota", list.get(0).name.equals("yota"));
    check("Collections.sort [1] = xena", list.get(1).name.equals("xena"));
    check("Collections.sort [2] = zeta", list.get(2).name.equals("zeta"));

    // Collections.reverse(List) — in-place swap.
    Collections.reverse(list);
    check("Collections.reverse [0] = zeta", list.get(0).name.equals("zeta"));
    check("Collections.reverse [2] = yota", list.get(2).name.equals("yota"));
  }

  // ── java.util.Iterator + enhanced for-each ───────────────────────────────

  static void testExplicitIterator() {
    ArrayList list = new ArrayList();
    list.add(Integer.valueOf(10));
    list.add(Integer.valueOf(20));
    list.add(Integer.valueOf(30));

    Iterator it = list.iterator();
    int sum = 0;
    while (it.hasNext()) {
      Integer val = (Integer) it.next();
      sum = sum + val.intValue();
    }
    check("explicit iterator sum=60", sum == 60);
  }

  static void testForEachArrayList() {
    ArrayList<Integer> list = new ArrayList<Integer>();
    list.add(1);
    list.add(2);
    list.add(3);
    list.add(4);
    list.add(5);

    int sum = 0;
    for (Object obj : list) {
      int val = ((Integer) obj).intValue();
      sum = sum + val;
    }
    check("for-each list sum=15", sum == 15);
    check("for-each list size=5", list.size() == 5);
  }

  static void testForEachHashMapKeys() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(10));
    map.put(Integer.valueOf(2), Integer.valueOf(20));
    map.put(Integer.valueOf(3), Integer.valueOf(30));

    int keySum = 0;
    for (Object key : map.keySet()) {
      keySum = keySum + ((Integer) key).intValue();
    }
    check("for-each keys sum=6", keySum == 6);
  }

  static void testForEachHashMapValues() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(10));
    map.put(Integer.valueOf(2), Integer.valueOf(20));
    map.put(Integer.valueOf(3), Integer.valueOf(30));

    int valSum = 0;
    for (Object val : map.values()) {
      valSum = valSum + ((Integer) val).intValue();
    }
    check("for-each values sum=60", valSum == 60);
  }

  static void testIteratorOnEmpty() {
    ArrayList list = new ArrayList();
    int count = 0;
    for (Object obj : list) {
      count = count + 1;
    }
    check("for-each empty count=0", count == 0);
  }

  static void testNestedForEach() {
    ArrayList outer = new ArrayList();
    for (int i = 0; i < 3; i++) {
      ArrayList inner = new ArrayList();
      inner.add(Integer.valueOf(i * 10 + 1));
      inner.add(Integer.valueOf(i * 10 + 2));
      outer.add(inner);
    }

    int sum = 0;
    for (Object row : outer) {
      ArrayList innerList = (ArrayList) row;
      for (Object val : innerList) {
        sum = sum + ((Integer) val).intValue();
      }
    }
    // (1+2) + (11+12) + (21+22) = 3 + 23 + 43 = 69
    check("nested for-each sum=69", sum == 69);
  }

  // ── java.util.HashMap ────────────────────────────────────────────────────

  static void testHashMapIntegerKeys() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(10));
    map.put(Integer.valueOf(2), Integer.valueOf(20));
    map.put(Integer.valueOf(3), Integer.valueOf(30));

    int a = ((Integer) map.get(Integer.valueOf(1))).intValue();
    int b = ((Integer) map.get(Integer.valueOf(2))).intValue();
    int c = ((Integer) map.get(Integer.valueOf(3))).intValue();
    check("put/get 1=10", a == 10);
    check("put/get 2=20", b == 20);
    check("put/get 3=30", c == 30);
    check("size=3", map.size() == 3);
  }

  static void testHashMapOverwrite() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(10));
    Object old = map.put(Integer.valueOf(1), Integer.valueOf(20));
    int oldVal = ((Integer) old).intValue();
    int newVal = ((Integer) map.get(Integer.valueOf(1))).intValue();
    check("overwrite old=10", oldVal == 10);
    check("overwrite new=20", newVal == 20);
    check("overwrite size=1", map.size() == 1);
  }

  static void testHashMapRemove() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(42));
    Object removed = map.remove(Integer.valueOf(1));
    int val = ((Integer) removed).intValue();
    check("remove val=42", val == 42);
    check("remove size=0", map.size() == 0);
    Object missing = map.remove(Integer.valueOf(1));
    check("remove missing=null", missing == null);
  }

  static void testHashMapContainsKey() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(100));
    check("containsKey present", map.containsKey(Integer.valueOf(1)));
    check("containsKey absent", !map.containsKey(Integer.valueOf(99)));
  }

  static void testHashMapContainsValue() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(99));
    check("containsValue 99", map.containsValue(Integer.valueOf(99)));
    check("containsValue 100", !map.containsValue(Integer.valueOf(100)));
  }

  static void testHashMapSizeAndEmpty() {
    HashMap map = new HashMap();
    check("empty isEmpty", map.isEmpty());
    check("empty size=0", map.size() == 0);
    map.put(Integer.valueOf(1), Integer.valueOf(2));
    check("non-empty !isEmpty", !map.isEmpty());
    check("non-empty size=1", map.size() == 1);
  }

  static void testHashMapClear() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(2));
    map.put(Integer.valueOf(3), Integer.valueOf(4));
    map.clear();
    check("clear size=0", map.size() == 0);
    check("clear isEmpty", map.isEmpty());
  }

  static void testHashMapGetOrDefault() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(10));
    int present = ((Integer) map.getOrDefault(Integer.valueOf(1), Integer.valueOf(-1))).intValue();
    int absent = ((Integer) map.getOrDefault(Integer.valueOf(99), Integer.valueOf(-1))).intValue();
    check("getOrDefault present=10", present == 10);
    check("getOrDefault absent=-1", absent == -1);
  }

  static void testHashMapStringKeys() {
    HashMap map = new HashMap();
    map.put("hello", "world");
    map.put("foo", "bar");
    String v1 = (String) map.get("hello");
    String v2 = (String) map.get("foo");
    check("string key hello", v1.equals("world"));
    check("string key foo", v2.equals("bar"));
    check("string containsKey", map.containsKey("hello"));
    check("string !containsKey", !map.containsKey("missing"));
  }

  // ── java.util.HashSet ────────────────────────────────────────────────────

  static void testHashSetBasic() {
    HashSet set = new HashSet();
    boolean added = set.add(Integer.valueOf(10));
    check("set add new=true", added);
    check("set contains 10", set.contains(Integer.valueOf(10)));
    check("set !contains 20", !set.contains(Integer.valueOf(20)));
    check("set size=1", set.size() == 1);
    boolean removed = set.remove(Integer.valueOf(10));
    check("set remove=true", removed);
    check("set size after remove=0", set.size() == 0);
  }

  static void testHashSetDuplicate() {
    HashSet set = new HashSet();
    set.add(Integer.valueOf(5));
    boolean dup = set.add(Integer.valueOf(5));
    check("set add duplicate=false", !dup);
    check("set size still 1", set.size() == 1);
  }

  static void testHashSetClear() {
    HashSet set = new HashSet();
    set.add(Integer.valueOf(1));
    set.add(Integer.valueOf(2));
    set.add(Integer.valueOf(3));
    set.clear();
    check("set clear size=0", set.size() == 0);
    check("set clear isEmpty", set.isEmpty());
  }
}
