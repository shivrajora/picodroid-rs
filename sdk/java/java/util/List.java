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

  Iterator<E> iterator();

  /**
   * Sorts this list under {@code c}. Unlike {@link Collections#sort}, which is implemented in Java
   * on top of {@link Arrays}, this resolves to a native arm that calls {@code c.compare} back into
   * the interpreter, so it works on the classfile-less builtin {@code ArrayList} where no Java body
   * could live. {@code c} must not be null — natural ordering is not supported here.
   */
  void sort(Comparator<? super E> c);
}
