package hashmaptest;

import java.util.HashMap;
import java.util.HashSet;
import picodroid.app.Application;
import picodroid.util.Log;

public class HashMapTest extends Application {
  private static final String TAG = "HashMapTest";

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
    Log.i(TAG, "=== HashMap/HashSet Tests ===");

    testIntegerKeys();
    testOverwrite();
    testRemove();
    testContainsKey();
    testContainsValue();
    testSizeAndEmpty();
    testClear();
    testGetOrDefault();
    testStringKeys();
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

  static void testIntegerKeys() {
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

  static void testOverwrite() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(10));
    Object old = map.put(Integer.valueOf(1), Integer.valueOf(20));
    int oldVal = ((Integer) old).intValue();
    int newVal = ((Integer) map.get(Integer.valueOf(1))).intValue();
    check("overwrite old=10", oldVal == 10);
    check("overwrite new=20", newVal == 20);
    check("overwrite size=1", map.size() == 1);
  }

  static void testRemove() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(42));
    Object removed = map.remove(Integer.valueOf(1));
    int val = ((Integer) removed).intValue();
    check("remove val=42", val == 42);
    check("remove size=0", map.size() == 0);
    Object missing = map.remove(Integer.valueOf(1));
    check("remove missing=null", missing == null);
  }

  static void testContainsKey() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(100));
    check("containsKey present", map.containsKey(Integer.valueOf(1)));
    check("containsKey absent", !map.containsKey(Integer.valueOf(99)));
  }

  static void testContainsValue() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(99));
    check("containsValue 99", map.containsValue(Integer.valueOf(99)));
    check("containsValue 100", !map.containsValue(Integer.valueOf(100)));
  }

  static void testSizeAndEmpty() {
    HashMap map = new HashMap();
    check("empty isEmpty", map.isEmpty());
    check("empty size=0", map.size() == 0);
    map.put(Integer.valueOf(1), Integer.valueOf(2));
    check("non-empty !isEmpty", !map.isEmpty());
    check("non-empty size=1", map.size() == 1);
  }

  static void testClear() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(2));
    map.put(Integer.valueOf(3), Integer.valueOf(4));
    map.clear();
    check("clear size=0", map.size() == 0);
    check("clear isEmpty", map.isEmpty());
  }

  static void testGetOrDefault() {
    HashMap map = new HashMap();
    map.put(Integer.valueOf(1), Integer.valueOf(10));
    int present = ((Integer) map.getOrDefault(Integer.valueOf(1), Integer.valueOf(-1))).intValue();
    int absent = ((Integer) map.getOrDefault(Integer.valueOf(99), Integer.valueOf(-1))).intValue();
    check("getOrDefault present=10", present == 10);
    check("getOrDefault absent=-1", absent == -1);
  }

  static void testStringKeys() {
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
