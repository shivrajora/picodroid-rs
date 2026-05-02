// SPDX-License-Identifier: GPL-3.0-only
package java.util;

public interface List<E> {
  int size();

  boolean isEmpty();

  E get(int i);

  E set(int i, E e);

  boolean add(E e);

  boolean contains(Object o);

  void clear();
}
