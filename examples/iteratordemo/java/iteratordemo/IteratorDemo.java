package iteratordemo;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import picodroid.app.Application;
import picodroid.util.Log;

public class IteratorDemo extends Application {
  private static final String TAG = "IteratorDemo";

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
    Log.i(TAG, "=== Iterator Tests ===");

    testExplicitIterator();
    testForEachArrayList();
    testForEachHashMapKeys();
    testForEachHashMapValues();
    testIteratorOnEmpty();
    testNestedForEach();

    String passStr = String.valueOf(passed);
    String failStr = String.valueOf(failed);
    Log.i(TAG, "Results: " + passStr + " passed, " + failStr + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== SOME FAILED ===");
    }
  }

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
    // List of lists pattern
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
}
