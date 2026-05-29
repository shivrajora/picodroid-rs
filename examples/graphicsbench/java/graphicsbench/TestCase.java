// SPDX-License-Identifier: GPL-3.0-only
package graphicsbench;

/** Workload closure consumed by {@link GraphicsBench#runTest}. */
public interface TestCase {
  void run();
}
