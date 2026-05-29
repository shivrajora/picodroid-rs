// SPDX-License-Identifier: GPL-3.0-only
package perfbench;

public class FastCounter extends Counter {
  public FastCounter() {
    super();
  }

  public int increment() {
    count = count + 2;
    return count;
  }
}
