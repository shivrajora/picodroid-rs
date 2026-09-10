// SPDX-License-Identifier: GPL-3.0-only
package kotlin.jvm.internal;

import java.util.ArrayList;

/** {@code f("w", *strs, "z")}: a spread mixed with elements on a reference vararg. */
public class SpreadBuilder {
  private final ArrayList<Object> list;

  public SpreadBuilder(int size) {
    list = new ArrayList<Object>(size);
  }

  public void addSpread(Object container) {
    if (container == null) {
      return;
    }
    Object[] array = (Object[]) container;
    for (Object element : array) {
      list.add(element);
    }
  }

  public int size() {
    return list.size();
  }

  public void add(Object element) {
    list.add(element);
  }

  /** Fills the caller's exactly-sized array (kotlinc passes {@code new T[size()]}). */
  public Object[] toArray(Object[] a) {
    int n = list.size();
    for (int i = 0; i < n; i++) {
      a[i] = list.get(i);
    }
    return a;
  }
}
