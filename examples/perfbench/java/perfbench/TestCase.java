// SPDX-License-Identifier: GPL-3.0-only
package perfbench;

/** Workload closure consumed by {@link PerfBench#runTest}. */
public interface TestCase {
  void run();
}
