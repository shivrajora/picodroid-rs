// SPDX-License-Identifier: GPL-3.0-only
package benchmark;

public class FastCounter extends Counter {
  public FastCounter() {
    super();
  }

  @Override
  public int increment() {
    count = count + 2;
    return count;
  }
}
