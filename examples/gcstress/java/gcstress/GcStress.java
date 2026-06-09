// SPDX-License-Identifier: GPL-3.0-only
package gcstress;

import java.util.ArrayList;
import picodroid.app.Application;
import picodroid.os.Runtime;
import picodroid.os.SystemClock;
import picodroid.util.Log;

public class GcStress extends Application {
  private static final String TAG = "GcStress";

  // Sinks to prevent dead-code elimination.
  static int sinkInt;
  static Object sink;
  static Node retained;

  @Override
  public void onCreate() {
    Log.i(TAG, "=== GC Stress Test ===");
    long total = 0;
    long t;

    t = stressObjectChurn();
    total += t;
    report("object_churn", t);

    t = stressLinkedChain();
    total += t;
    report("linked_chain", t);

    t = stressCircularRefs();
    total += t;
    report("circular_refs", t);

    t = stressStringChurn();
    total += t;
    report("string_churn", t);

    t = stressArrayChurn();
    total += t;
    report("array_churn", t);

    t = stressArrayListChurn();
    total += t;
    report("arraylist_churn", t);

    t = stressMixedTypes();
    total += t;
    report("mixed_types", t);

    t = stressRetention();
    total += t;
    report("retention", t);

    long totalUs = total / 1000L;
    Log.i(TAG, "TOTAL: " + String.valueOf(totalUs) + " us");
    Log.i(TAG, "=== PASSED ===");
  }

  static void report(String name, long wallNs) {
    long wallUs = wallNs / 1000L;
    long gcUs = Runtime.gcTimeNanos() / 1000L;
    int gcCount = Runtime.gcCount();
    int gcFreed = Runtime.gcFreed();
    String wallStr = String.valueOf(wallUs);
    String gcStr = String.valueOf(gcUs);
    String countStr = String.valueOf(gcCount);
    String freedStr = String.valueOf(gcFreed);
    Log.i(
        TAG,
        name
            + ": "
            + wallStr
            + " us (gc: "
            + gcStr
            + " us, "
            + countStr
            + " collections, "
            + freedStr
            + " freed)");
  }

  // ── 1. Object churn ──────────────────────────────────────────────────────────

  static long stressObjectChurn() {
    Runtime.resetGcStats();
    long start = SystemClock.elapsedRealtimeNanos();
    Node last = null;
    for (int i = 0; i < 2048; i++) {
      last = new Node(i);
    }
    sinkInt = last.value;
    sink = last;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── 2. Linked chain ──────────────────────────────────────────────────────────

  static long stressLinkedChain() {
    Runtime.resetGcStats();
    long start = SystemClock.elapsedRealtimeNanos();
    Node head = null;
    for (int round = 0; round < 3; round++) {
      head = null;
      for (int i = 0; i < 512; i++) {
        Node n = new Node(i);
        n.next = head;
        head = n;
      }
      // Walk the chain to verify integrity.
      int count = 0;
      Node cur = head;
      while (cur != null) {
        count = count + 1;
        cur = cur.next;
      }
      sinkInt = count;
    }
    sink = head;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── 3. Circular references ────────────────────────────────────────────────────

  static long stressCircularRefs() {
    Runtime.resetGcStats();
    long start = SystemClock.elapsedRealtimeNanos();
    for (int i = 0; i < 1024; i++) {
      Node a = new Node(i);
      Node b = new Node(i + 1);
      a.next = b;
      b.next = a;
    }
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── 4. String churn ──────────────────────────────────────────────────────────

  static long stressStringChurn() {
    Runtime.resetGcStats();
    long start = SystemClock.elapsedRealtimeNanos();
    String last = "";
    for (int i = 0; i < 1024; i++) {
      StringBuilder sb = new StringBuilder();
      sb.append("gc");
      sb.append(i);
      last = sb.toString();
    }
    sinkInt = last.length();
    sink = last;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── 5. Array churn ───────────────────────────────────────────────────────────

  static long stressArrayChurn() {
    Runtime.resetGcStats();
    long start = SystemClock.elapsedRealtimeNanos();
    int sum = 0;
    for (int i = 0; i < 1024; i++) {
      int[] arr = new int[16];
      arr[0] = i;
      sum = sum + arr[0];
      Node[] refs = new Node[4];
      refs[0] = new Node(i);
      sum = sum + refs[0].value;
    }
    sinkInt = sum;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── 6. ArrayList churn ───────────────────────────────────────────────────────

  static long stressArrayListChurn() {
    Runtime.resetGcStats();
    long start = SystemClock.elapsedRealtimeNanos();
    int sum = 0;
    for (int i = 0; i < 512; i++) {
      ArrayList list = new ArrayList();
      list.add(new Integer(i));
      list.add(new Integer(i + 1));
      list.add(new Integer(i + 2));
      list.add(new Integer(i + 3));
      int val = ((Integer) list.get(0)).intValue();
      sum = sum + val;
    }
    sinkInt = sum;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── 7. Mixed types ───────────────────────────────────────────────────────────

  static long stressMixedTypes() {
    Runtime.resetGcStats();
    long start = SystemClock.elapsedRealtimeNanos();
    Node lastNode = null;
    String lastStr = "";
    int arraySum = 0;
    int listSum = 0;
    for (int i = 0; i < 512; i++) {
      lastNode = new Node(i);
      int[] arr = new int[8];
      arr[0] = i;
      arraySum = arraySum + arr[0];
      StringBuilder sb = new StringBuilder();
      sb.append("mix");
      sb.append(i);
      lastStr = sb.toString();
      ArrayList list = new ArrayList();
      list.add(new Integer(i));
      int val = ((Integer) list.get(0)).intValue();
      listSum = listSum + val;
    }
    sinkInt = arraySum + listSum + lastStr.length();
    sink = lastNode;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── 8. Retention pattern ─────────────────────────────────────────────────────

  static long stressRetention() {
    Runtime.resetGcStats();
    long start = SystemClock.elapsedRealtimeNanos();
    for (int i = 0; i < 1024; i++) {
      Node n = new Node(i);
      if (i % 100 == 0) {
        retained = n;
      }
    }
    sinkInt = retained.value;
    retained = null;
    return SystemClock.elapsedRealtimeNanos() - start;
  }
}
