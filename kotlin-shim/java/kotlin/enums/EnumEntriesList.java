// SPDX-License-Identifier: GPL-3.0-only
package kotlin.enums;

import java.util.AbstractList;
import java.util.Iterator;
import java.util.NoSuchElementException;

/**
 * {@code entries} over the enum's {@code values()} array. Extends {@code AbstractList} so javac
 * sees a complete {@code List}; on pico-jvm that superclass has no class file, so only the methods
 * declared here exist at run time — {@code get}, {@code size}, {@code isEmpty}, {@code iterator},
 * {@code contains}, {@code indexOf} — which is the surface {@code for}, {@code entries[i]}, {@code
 * entries.size} and {@code x in entries} use. Anything else is a {@code NoSuchMethod} at the call.
 */
final class EnumEntriesList<E extends Enum<E>> extends AbstractList<E> implements EnumEntries<E> {
  private final E[] entries;

  EnumEntriesList(E[] entries) {
    this.entries = entries;
  }

  @Override
  public E get(int index) {
    if (index < 0 || index >= entries.length) {
      throw new IndexOutOfBoundsException(
          "index " + index + " out of bounds for " + entries.length);
    }
    return entries[index];
  }

  @Override
  public int size() {
    return entries.length;
  }

  @Override
  public boolean isEmpty() {
    return entries.length == 0;
  }

  @Override
  public boolean contains(Object o) {
    return indexOf(o) >= 0;
  }

  @Override
  public int indexOf(Object o) {
    for (int i = 0; i < entries.length; i++) {
      if (entries[i].equals(o)) {
        return i;
      }
    }
    return -1;
  }

  @Override
  public Iterator<E> iterator() {
    return new Itr<>(entries);
  }

  private static final class Itr<E> implements Iterator<E> {
    private final E[] entries;
    private int next = 0;

    Itr(E[] entries) {
      this.entries = entries;
    }

    @Override
    public boolean hasNext() {
      return next < entries.length;
    }

    @Override
    public E next() {
      if (next >= entries.length) {
        throw new NoSuchElementException();
      }
      E e = entries[next];
      next = next + 1;
      return e;
    }
  }
}
