// SPDX-License-Identifier: GPL-3.0-only
package trywithresourcesdemo;

import picodroid.app.Application;
import picodroid.pio.Adc;
import picodroid.pio.PeripheralManager;
import picodroid.util.Log;

public class TryWithResourcesDemo extends Application {
  private static final String TAG = "TWR";

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

  /** Distinguishable user exception — classfile-backed, so instanceof/catch walks work. */
  static class CloseException extends RuntimeException {
    CloseException(String msg) {
      super(msg);
    }
  }

  /** AutoCloseable whose close() always throws — drives the suppression machinery. */
  static class ThrowingResource implements AutoCloseable {
    final String name;

    ThrowingResource(String name) {
      this.name = name;
    }

    @Override
    public void close() {
      throw new CloseException("close failed: " + name);
    }
  }

  @Override
  public void onCreate() {
    run();
  }

  public static void run() {
    PeripheralManager pm = PeripheralManager.getInstance();

    try (Adc adc = pm.openAdcPin("GP26")) {
      double v = adc.readValue();
      Log.i(TAG, "voltage=" + v);
    }

    testBodyThrowsCloseThrows();
    testOnlyCloseThrows();
    testNestedResourcesBothClose();
    testManualAddSuppressed();

    String passStr = String.valueOf(passed);
    String failStr = String.valueOf(failed);
    Log.i(TAG, "Results: " + passStr + " passed, " + failStr + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== SUPPRESSED PASSED ===");
    }
    Log.i(TAG, "done");
  }

  /** Body throws, then close() throws: close's exception must be suppressed under the body's. */
  static void testBodyThrowsCloseThrows() {
    try {
      try (ThrowingResource r = new ThrowingResource("r1")) {
        throw new RuntimeException("body failed");
      }
    } catch (RuntimeException e) {
      check("primary is the body exception", "body failed".equals(e.getMessage()));
      // Churn the heap so a GC cycle runs between addSuppressed (inside the
      // compiled try-with-resources) and getSuppressed below — regression
      // for side-table entries being swept while their owner lives.
      String churn = "";
      for (int i = 0; i < 50; i++) {
        churn = churn + i;
      }
      check("churn ran", churn.length() > 0);
      Throwable[] sup = e.getSuppressed();
      check("one suppressed exception", sup.length == 1);
      check(
          "suppressed is close()'s exception",
          sup.length == 1 && "close failed: r1".equals(sup[0].getMessage()));
    }
  }

  /** Only close() throws: its exception is the primary, nothing suppressed. */
  static void testOnlyCloseThrows() {
    try {
      try (ThrowingResource r = new ThrowingResource("r2")) {
        Log.i(TAG, "body ok");
      }
      check("close exception propagated", false);
    } catch (CloseException e) {
      check("close exception is primary", "close failed: r2".equals(e.getMessage()));
      check("nothing suppressed", e.getSuppressed().length == 0);
    }
  }

  /** Two resources, body throws: BOTH closes throw and suppress in reverse order. */
  static void testNestedResourcesBothClose() {
    try {
      try (ThrowingResource a = new ThrowingResource("a");
          ThrowingResource b = new ThrowingResource("b")) {
        throw new RuntimeException("multi body");
      }
    } catch (RuntimeException e) {
      Throwable[] sup = e.getSuppressed();
      check("two suppressed exceptions", sup.length == 2);
      // Resources close in reverse declaration order: b first, then a.
      check(
          "suppressed order b then a",
          sup.length == 2
              && "close failed: b".equals(sup[0].getMessage())
              && "close failed: a".equals(sup[1].getMessage()));
    }
  }

  /** Direct API use, including the spec'd NPE / IllegalArgumentException cases. */
  static void testManualAddSuppressed() {
    RuntimeException primary = new RuntimeException("primary");
    primary.addSuppressed(new CloseException("manual"));
    check("manual suppressed recorded", primary.getSuppressed().length == 1);

    boolean npe = false;
    try {
      primary.addSuppressed(null);
    } catch (NullPointerException e) {
      npe = true;
    }
    check("addSuppressed(null) throws NPE", npe);

    boolean iae = false;
    try {
      primary.addSuppressed(primary);
    } catch (IllegalArgumentException e) {
      iae = true;
    }
    check("addSuppressed(this) throws IAE", iae);
    check("failed attempts recorded nothing", primary.getSuppressed().length == 1);
  }
}
