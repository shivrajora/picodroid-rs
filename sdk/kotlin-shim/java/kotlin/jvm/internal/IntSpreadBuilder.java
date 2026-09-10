// SPDX-License-Identifier: GPL-3.0-only
package kotlin.jvm.internal;

import java.util.Arrays;

/** {@code f(1, *ints)}: a spread mixed with elements on an {@code Int} vararg. */
public final class IntSpreadBuilder {
  private int[] buf;
  private int count;

  public IntSpreadBuilder(int size) {
    buf = new int[Math.max(size, 4)];
  }

  public void add(int value) {
    ensure(1);
    buf[count++] = value;
  }

  public void addSpread(Object spread) {
    int[] array = (int[]) spread;
    ensure(array.length);
    for (int v : array) {
      buf[count++] = v;
    }
  }

  public int[] toArray() {
    return Arrays.copyOf(buf, count);
  }

  private void ensure(int extra) {
    if (count + extra > buf.length) {
      buf = Arrays.copyOf(buf, Math.max(buf.length * 2, count + extra));
    }
  }
}
