// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

public final class Executors {
  private Executors() {}

  public static native Executor mainExecutor();

  public static native Executor backgroundExecutor();

  /**
   * Bridge invoked by the Rust-side drain of the main queue / background pool. Called directly via
   * {@code Jvm::invoke_static_with_args} with the queued Runnable as the single argument; routes
   * through {@code invokeinterface} bytecode so lambda proxies (which store their target method in
   * Rust-side metadata, not in a real vtable entry under {@code java/lang/Runnable}) resolve
   * correctly.
   */
  static void dispatchRunnable(Runnable r) {
    if (r != null) {
      r.run();
    }
  }
}
