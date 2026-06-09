// SPDX-License-Identifier: GPL-3.0-only
package perfbench;

import java.util.ArrayList;
import picodroid.app.Application;
import picodroid.os.Runtime;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * Unified speed + memory benchmark.
 *
 * <p>Runs three workload groups (speed-only arithmetic and dispatch, GC/fragmentation pressure, and
 * realistic mixed workloads), then prints one composite SCORE (lower is better) computed from wall
 * time, GC cycle count, and peak heap delta. The single number is intended for tracking
 * optimisation work across commits — grep for {@code "^.PerfBench. SCORE"} in sim output.
 *
 * <p>Score formula per test:
 *
 * <pre>{@code
 * score = wall_ms + W_GC * gc_cycles + (peak_kb / W_PEAK_DIV)
 * }</pre>
 *
 * <p>Wall time already includes GC pause time, so {@code gcTimeNanos} is reported but not added to
 * the score (would double-count). The cycle penalty captures GC frequency, the peak penalty
 * captures memory high-water mark — both are signals wall time alone misses.
 */
public class PerfBench extends Application {
  private static final String TAG = "PerfBench";

  // ── speed-test iteration counts ────────────────────────────────────────────
  private static final int ITER_INT = 500000;
  private static final int ITER_LONG = 200000;
  private static final int ITER_FLOAT = 300000;
  private static final int ITER_DOUBLE = 300000;
  private static final int ITER_DISPATCH = 200000;
  private static final int ITER_CONTROL = 300000;

  // ── scoring weights (tune to taste; bigger = harsher) ─────────────────────
  private static final long W_GC = 1L; // each GC cycle adds 1 ms-equivalent
  private static final long W_PEAK_DIV = 10L; // 10 KB of peak heap ≈ 1 ms-equivalent

  // Sinks to defeat dead-code elimination.
  static int sinkInt;
  static long sinkLong;
  static float sinkFloat;
  static double sinkDouble;
  static Object sinkObj;

  @Override
  public void onCreate() {
    Log.i(TAG, "=== perfbench (speed + memory composite score) ===");

    Log.i(TAG, "--- speed group ---");
    long speedScore = 0L;
    speedScore += runTest("int_arith", () -> benchIntArith());
    speedScore += runTest("long_arith", () -> benchLongArith());
    speedScore += runTest("float_arith", () -> benchFloatArith());
    speedScore += runTest("double_arith", () -> benchDoubleArith());
    speedScore += runTest("method_dispatch", () -> benchMethodDispatch());
    speedScore += runTest("interface_dispatch", () -> benchInterfaceDispatch());
    speedScore += runTest("control_flow", () -> benchControlFlow());

    Log.i(TAG, "--- memory group ---");
    long memScore = 0L;
    memScore += runTest("object_churn", () -> memObjectChurn());
    memScore += runTest("linked_chain", () -> memLinkedChain());
    memScore += runTest("string_churn", () -> memStringChurn());
    memScore += runTest("slot_reuse", () -> memSlotReuse());
    memScore += runTest("grow_shrink", () -> memGrowShrink());

    Log.i(TAG, "--- mixed group ---");
    long mixScore = 0L;
    mixScore += runTest("mix_tokenize", () -> mixTokenize());
    mixScore += runTest("mix_graph_walk", () -> mixGraphWalk());
    mixScore += runTest("mix_array_compute", () -> mixArrayCompute());

    long total = speedScore + memScore + mixScore;
    Log.i(TAG, "SUBSCORE speed=" + speedScore + " memory=" + memScore + " mixed=" + mixScore);
    Log.i(TAG, "SCORE " + total);
    Log.i(TAG, "=== PASSED ===");
  }

  /**
   * Instruments {@code t.run()} with timing, GC counters, and peak-heap delta, prints a one-line
   * per-test report, and returns the composite per-test score.
   */
  static long runTest(String name, TestCase t) {
    Runtime.resetGcStats();
    long usedBefore = Runtime.usedMemory();
    Runtime.resetPeakMemory();

    long startNs = SystemClock.elapsedRealtimeNanos();
    t.run();
    long wallNs = SystemClock.elapsedRealtimeNanos() - startNs;

    int gcCount = Runtime.gcCount();
    long gcTimeNs = Runtime.gcTimeNanos();
    long peakBytes = Runtime.peakMemory() - usedBefore;
    if (peakBytes < 0L) {
      peakBytes = 0L;
    }
    long peakKb = peakBytes / 1024L;
    long wallMs = wallNs / 1000000L;
    long gcTimeMs = gcTimeNs / 1000000L;

    long score = wallMs + (W_GC * (long) gcCount) + (peakKb / W_PEAK_DIV);
    Log.i(
        TAG,
        name
            + ": wall "
            + String.valueOf(wallMs)
            + " ms (gc "
            + String.valueOf(gcTimeMs)
            + " ms / "
            + String.valueOf(gcCount)
            + " cyc), peak +"
            + String.valueOf(peakKb)
            + " KB -> score "
            + String.valueOf(score));
    return score;
  }

  // ════════════════════════════════════════════════════════════════════════════
  // SPEED group
  // ════════════════════════════════════════════════════════════════════════════

  static void benchIntArith() {
    int sum = 0;
    for (int i = 0; i < ITER_INT; i++) {
      sum = sum + i;
      sum = sum * 3;
      sum = sum - i;
      sum = sum / (i + 1);
      sum = sum % 1000;
    }
    sinkInt = sum;
  }

  static void benchLongArith() {
    long sum = 0L;
    for (int i = 0; i < ITER_LONG; i++) {
      sum = sum + (long) i;
      sum = sum * 3L;
      sum = sum - (long) i;
      sum = sum / ((long) i + 1L);
    }
    sinkLong = sum;
  }

  static void benchFloatArith() {
    float sum = 0.0f;
    for (int i = 0; i < ITER_FLOAT; i++) {
      sum = sum + 1.5f;
      sum = sum * 1.01f;
      sum = sum - 0.5f;
      sum = sum / 1.02f;
    }
    sinkFloat = sum;
  }

  static void benchDoubleArith() {
    double sum = 0.0;
    for (int i = 0; i < ITER_DOUBLE; i++) {
      sum = sum + 1.5;
      sum = sum * 1.01;
      sum = sum - 0.5;
      sum = sum / 1.02;
    }
    sinkDouble = sum;
  }

  static void benchMethodDispatch() {
    Counter fast = new FastCounter();
    Counter slow = new SlowCounter();
    Counter[] counters = new Counter[2];
    counters[0] = fast;
    counters[1] = slow;

    int sum = 0;
    for (int i = 0; i < ITER_DISPATCH; i++) {
      sum += counters[i % 2].increment();
    }
    sinkInt = sum;
  }

  static void benchInterfaceDispatch() {
    Countable fast = new FastCounter();
    Countable slow = new SlowCounter();
    Countable[] items = new Countable[2];
    items[0] = fast;
    items[1] = slow;

    int sum = 0;
    for (int i = 0; i < ITER_DISPATCH; i++) {
      sum += items[i % 2].count();
    }
    sinkInt = sum;
  }

  static void benchControlFlow() {
    int sum = 0;
    for (int i = 0; i < ITER_CONTROL; i++) {
      switch (i % 4) {
        case 0:
          sum += 1;
          break;
        case 1:
          sum += 2;
          break;
        case 2:
          sum += 3;
          break;
        case 3:
          sum += 4;
          break;
      }
      if (sum > 1000000) {
        sum = sum - 1000000;
      } else if (sum > 500000) {
        sum = sum - 500000;
      }
    }
    sinkInt = sum;
  }

  // ════════════════════════════════════════════════════════════════════════════
  // MEMORY group — adapted from gcstress / heapstress.
  // ════════════════════════════════════════════════════════════════════════════

  static void memObjectChurn() {
    Node last = null;
    for (int i = 0; i < 2048; i++) {
      last = new Node(i);
    }
    sinkInt = last.value;
    sinkObj = last;
  }

  static void memLinkedChain() {
    Node head = null;
    for (int round = 0; round < 3; round++) {
      head = null;
      for (int i = 0; i < 512; i++) {
        Node n = new Node(i);
        n.next = head;
        head = n;
      }
      int count = 0;
      Node cur = head;
      while (cur != null) {
        count = count + 1;
        cur = cur.next;
      }
      sinkInt = count;
    }
    sinkObj = head;
  }

  static void memStringChurn() {
    String last = "";
    for (int i = 0; i < 1024; i++) {
      StringBuilder sb = new StringBuilder();
      sb.append("gc");
      sb.append(i);
      last = sb.toString();
    }
    sinkInt = last.length();
    sinkObj = last;
  }

  static void memSlotReuse() {
    Node last = null;
    for (int i = 0; i < 2048; i++) {
      last = new Node(i);
    }
    sinkInt = last.value;
    sinkObj = last;
  }

  static void memGrowShrink() {
    int sum = 0;
    for (int round = 0; round < 10; round++) {
      ArrayList list = new ArrayList();
      for (int i = 0; i < 100; i++) {
        list.add(new Integer(i));
      }
      int val = ((Integer) list.get(99)).intValue();
      sum = sum + val;
    }
    sinkInt = sum;
  }

  // ════════════════════════════════════════════════════════════════════════════
  // MIXED group — realistic compute + allocation patterns. These are the most
  // representative optimisation targets: shaving wall time on these without
  // increasing peak heap is the goal.
  // ════════════════════════════════════════════════════════════════════════════

  private static final String SENTENCE = "the quick brown fox jumps over the lazy dog";

  /** Split a sentence by spaces 256 times, summing token lengths. */
  static void mixTokenize() {
    int total = 0;
    int senLen = SENTENCE.length();
    for (int iter = 0; iter < 256; iter++) {
      ArrayList tokens = new ArrayList();
      StringBuilder buf = new StringBuilder();
      for (int i = 0; i < senLen; i++) {
        int c = SENTENCE.charAt(i);
        if (c == ' ') {
          tokens.add(buf.toString());
          buf = new StringBuilder();
        } else {
          buf.append((char) c);
        }
      }
      tokens.add(buf.toString());
      for (int i = 0; i < tokens.size(); i++) {
        String s = (String) tokens.get(i);
        total = total + s.length();
      }
    }
    sinkInt = total;
  }

  /** Build a linked Node graph each round, traverse summing values, drop, repeat. */
  static void mixGraphWalk() {
    int total = 0;
    for (int round = 0; round < 4; round++) {
      Node head = null;
      for (int i = 0; i < 256; i++) {
        Node n = new Node(i);
        n.next = head;
        head = n;
      }
      Node cur = head;
      while (cur != null) {
        total = total + cur.value;
        cur = cur.next;
      }
    }
    sinkInt = total;
  }

  /** Allocate a 32-element int[] per iteration, fill + sum it. */
  static void mixArrayCompute() {
    int total = 0;
    for (int iter = 0; iter < 512; iter++) {
      int[] arr = new int[32];
      for (int i = 0; i < 32; i++) {
        arr[i] = (iter ^ i) + 1;
      }
      int sum = 0;
      for (int i = 0; i < 32; i++) {
        sum = sum + (arr[i] * (i + 1));
      }
      total = total + sum;
    }
    sinkInt = total;
  }
}
