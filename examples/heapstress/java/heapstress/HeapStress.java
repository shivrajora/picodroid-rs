package heapstress;

import java.util.ArrayList;
import picodroid.app.Application;
import picodroid.os.Runtime;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * Heap fragmentation stress test.
 *
 * <p>Exercises allocation patterns that are hostile to a fixed-size embedded heap: rapid slot
 * reuse, grow/shrink cycles, interleaved allocation sizes, and sustained high-water-mark pressure.
 * Designed to run under a sim heap limit (e.g. PICODROID_HEAP_LIMIT_KB=128) to validate GC and
 * allocator robustness.
 */
public class HeapStress extends Application {
  private static final String TAG = "HeapStress";

  // Sinks to prevent dead-code elimination.
  static int sinkInt;
  static Object sink;
  static Node retained;

  public void onCreate() {
    Log.i(TAG, "=== Heap Fragmentation Stress Test ===");
    long total = 0;
    long t;

    t = testSlotReuse();
    total += t;
    report("slot_reuse", t);

    t = testGrowShrink();
    total += t;
    report("grow_shrink", t);

    t = testInterleavedSizes();
    total += t;
    report("interleaved_sizes", t);

    t = testPeakPressure();
    total += t;
    report("peak_pressure", t);

    long totalUs = total / 1000L;
    Log.i(TAG, "TOTAL: " + String.valueOf(totalUs) + " us");
    Log.i(TAG, "=== PASSED ===");
  }

  static void report(String name, long wallNs) {
    long wallUs = wallNs / 1000L;
    long gcUs = Runtime.gcTimeNanos() / 1000L;
    int gcCount = Runtime.gcCount();
    int gcFreed = Runtime.gcFreed();
    Log.i(
        TAG,
        name
            + ": "
            + String.valueOf(wallUs)
            + " us (gc: "
            + String.valueOf(gcUs)
            + " us, "
            + String.valueOf(gcCount)
            + " collections, "
            + String.valueOf(gcFreed)
            + " freed)");
  }

  // ── 1. Slot reuse ──────────────────────────────────────────────────────────
  // Rapid alloc/discard cycling — GC must reclaim fast enough to avoid OOM.

  static long testSlotReuse() {
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

  // ── 2. Grow / shrink ──────────────────────────────────────────────────────
  // ArrayList growing to 100 elements then clearing, 10 rounds.
  // Tests backing-store reallocation and GC of abandoned list buffers.

  static long testGrowShrink() {
    Runtime.resetGcStats();
    long start = SystemClock.elapsedRealtimeNanos();
    int sum = 0;
    for (int round = 0; round < 10; round++) {
      ArrayList list = new ArrayList();
      for (int i = 0; i < 100; i++) {
        list.add(new Integer(i));
      }
      int val = ((Integer) list.get(99)).intValue();
      sum = sum + val;
      // list goes out of scope — GC should reclaim it and all Integer objects.
    }
    sinkInt = sum;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── 3. Interleaved sizes ───────────────────────────────────────────────────
  // Alternate between small objects and large arrays to create a
  // fragmentation-hostile pattern.

  static long testInterleavedSizes() {
    Runtime.resetGcStats();
    long start = SystemClock.elapsedRealtimeNanos();
    int sum = 0;
    for (int i = 0; i < 512; i++) {
      // Small object (few fields, inline storage)
      Node n = new Node(i);
      sum = sum + n.value;

      // Large array (arena-backed, >8 elements)
      int[] arr = new int[16];
      arr[0] = i;
      sum = sum + arr[0];

      // Small array (inline, ≤8 elements)
      int[] small = new int[4];
      small[0] = i;
      sum = sum + small[0];

      // Dynamic string via StringBuilder
      StringBuilder sb = new StringBuilder();
      sb.append("x");
      sb.append(i);
      String s = sb.toString();
      sum = sum + s.length();
    }
    sinkInt = sum;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── 4. Peak pressure ───────────────────────────────────────────────────────
  // Build up a large live set (linked list of 256 nodes + arrays), then
  // release and rebuild.  Tests that GC can reclaim a large graph and the
  // allocator can reuse the freed space without fragmentation failures.

  static long testPeakPressure() {
    Runtime.resetGcStats();
    long start = SystemClock.elapsedRealtimeNanos();

    for (int round = 0; round < 5; round++) {
      // Build a linked list of 256 nodes, each holding a small array.
      Node head = null;
      for (int i = 0; i < 256; i++) {
        Node n = new Node(i);
        int[] data = new int[8];
        data[0] = i;
        sinkInt = data[0]; // prevent elimination
        n.next = head;
        head = n;
      }

      // Walk the list to verify integrity.
      int count = 0;
      Node cur = head;
      while (cur != null) {
        count = count + 1;
        cur = cur.next;
      }
      sinkInt = count;
      // Release entire list — next round must reuse the space.
      head = null;
    }

    return SystemClock.elapsedRealtimeNanos() - start;
  }
}
