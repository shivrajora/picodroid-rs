// SPDX-License-Identifier: GPL-3.0-only
package benchmark;

import picodroid.app.Application;
import picodroid.os.SystemClock;
import picodroid.util.Log;

public class Benchmark extends Application {
  private static final String TAG = "Benchmark";
  private static final int ITER_INT = 500000;
  private static final int ITER_LONG = 200000;
  private static final int ITER_FLOAT = 300000;
  private static final int ITER_DOUBLE = 300000;
  private static final int ITER_DISPATCH = 200000;
  private static final int ITER_ALLOC = 50000;
  private static final int ITER_ARRAY = 10000;
  private static final int ITER_STRING = 20000;
  private static final int ITER_CONTROL = 300000;

  // Sinks to prevent dead-code elimination.
  static int sinkInt;
  static long sinkLong;
  static float sinkFloat;
  static double sinkDouble;
  static Object sinkObj;

  @Override
  public void onCreate() {
    Log.i(TAG, "--- picodroid JVM benchmark ---");
    long total = 0;
    long t;

    t = benchIntArithmetic();
    total += t;
    report("int_arithmetic", t);

    t = benchLongArithmetic();
    total += t;
    report("long_arithmetic", t);

    t = benchFloatArithmetic();
    total += t;
    report("float_arithmetic", t);

    t = benchDoubleArithmetic();
    total += t;
    report("double_arithmetic", t);

    t = benchMethodDispatch();
    total += t;
    report("method_dispatch", t);

    t = benchInterfaceDispatch();
    total += t;
    report("interface_dispatch", t);

    t = benchObjectAllocation();
    total += t;
    report("object_allocation", t);

    t = benchArrayOperations();
    total += t;
    report("array_operations", t);

    t = benchStringOperations();
    total += t;
    report("string_operations", t);

    t = benchControlFlow();
    total += t;
    report("control_flow", t);

    report("TOTAL", total);
  }

  static void report(String name, long nanos) {
    long ms = nanos / 1000000L;
    Log.i(TAG, name + ": " + String.valueOf(ms) + " ms");
  }

  // ── int arithmetic ──────────────────────────────────────────────────────────

  static long benchIntArithmetic() {
    long start = SystemClock.elapsedRealtimeNanos();
    int sum = 0;
    for (int i = 0; i < ITER_INT; i++) {
      sum = sum + i;
      sum = sum * 3;
      sum = sum - i;
      sum = sum / (i + 1);
      sum = sum % 1000;
    }
    sinkInt = sum;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── long arithmetic ─────────────────────────────────────────────────────────

  static long benchLongArithmetic() {
    long start = SystemClock.elapsedRealtimeNanos();
    long sum = 0L;
    for (int i = 0; i < ITER_LONG; i++) {
      sum = sum + (long) i;
      sum = sum * 3L;
      sum = sum - (long) i;
      sum = sum / ((long) i + 1L);
    }
    sinkLong = sum;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── float arithmetic ────────────────────────────────────────────────────────

  static long benchFloatArithmetic() {
    long start = SystemClock.elapsedRealtimeNanos();
    float sum = 0.0f;
    for (int i = 0; i < ITER_FLOAT; i++) {
      sum = sum + 1.5f;
      sum = sum * 1.01f;
      sum = sum - 0.5f;
      sum = sum / 1.02f;
    }
    sinkFloat = sum;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── double arithmetic ───────────────────────────────────────────────────────

  static long benchDoubleArithmetic() {
    long start = SystemClock.elapsedRealtimeNanos();
    double sum = 0.0;
    for (int i = 0; i < ITER_DOUBLE; i++) {
      sum = sum + 1.5;
      sum = sum * 1.01;
      sum = sum - 0.5;
      sum = sum / 1.02;
    }
    sinkDouble = sum;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── virtual method dispatch ─────────────────────────────────────────────────

  static long benchMethodDispatch() {
    Counter fast = new FastCounter();
    Counter slow = new SlowCounter();
    Counter[] counters = new Counter[2];
    counters[0] = fast;
    counters[1] = slow;

    long start = SystemClock.elapsedRealtimeNanos();
    int sum = 0;
    for (int i = 0; i < ITER_DISPATCH; i++) {
      sum += counters[i % 2].increment();
    }
    sinkInt = sum;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── interface dispatch ──────────────────────────────────────────────────────

  static long benchInterfaceDispatch() {
    Countable fast = new FastCounter();
    Countable slow = new SlowCounter();
    Countable[] items = new Countable[2];
    items[0] = fast;
    items[1] = slow;

    long start = SystemClock.elapsedRealtimeNanos();
    int sum = 0;
    for (int i = 0; i < ITER_DISPATCH; i++) {
      sum += items[i % 2].count();
    }
    sinkInt = sum;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── object allocation ───────────────────────────────────────────────────────

  static long benchObjectAllocation() {
    long start = SystemClock.elapsedRealtimeNanos();
    Counter last = null;
    for (int i = 0; i < ITER_ALLOC; i++) {
      Counter c = new Counter();
      c.count = i;
      last = c;
    }
    sinkObj = last;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── array operations ────────────────────────────────────────────────────────

  static long benchArrayOperations() {
    long start = SystemClock.elapsedRealtimeNanos();
    int totalSum = 0;
    for (int iter = 0; iter < ITER_ARRAY; iter++) {
      int[] arr = new int[100];
      for (int i = 0; i < 100; i++) {
        arr[i] = i;
      }
      int sum = 0;
      for (int i = 0; i < 100; i++) {
        sum += arr[i];
      }
      totalSum += sum;
    }
    sinkInt = totalSum;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── string operations ───────────────────────────────────────────────────────

  static long benchStringOperations() {
    long start = SystemClock.elapsedRealtimeNanos();
    String last = "";
    for (int i = 0; i < ITER_STRING; i++) {
      StringBuilder sb = new StringBuilder();
      sb.append("item");
      sb.append(i);
      last = sb.toString();
    }
    sinkObj = last;
    return SystemClock.elapsedRealtimeNanos() - start;
  }

  // ── control flow ────────────────────────────────────────────────────────────

  static long benchControlFlow() {
    long start = SystemClock.elapsedRealtimeNanos();
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
    return SystemClock.elapsedRealtimeNanos() - start;
  }
}
