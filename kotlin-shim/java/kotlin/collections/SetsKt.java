// SPDX-License-Identifier: GPL-3.0-only
package kotlin.collections;

import java.util.HashSet;
import java.util.LinkedHashSet;
import java.util.Set;

/**
 * Set factories and {@code plus}/{@code minus}; {@code mutableSetOf()} is inline ({@code new
 * LinkedHashSet}).
 */
public final class SetsKt {
  private SetsKt() {}

  private static LinkedHashSet<Object> fromArray(Object[] elements) {
    LinkedHashSet<Object> out = new LinkedHashSet<Object>();
    for (Object e : elements) {
      out.add(e);
    }
    return out;
  }

  private static LinkedHashSet<Object> copy(Set source) {
    LinkedHashSet<Object> out = new LinkedHashSet<Object>();
    for (Object e : source) {
      out.add(e);
    }
    return out;
  }

  /** A fresh (mutable) empty set per call — see {@code CollectionsKt.emptyList}. */
  public static Set emptySet() {
    return new HashSet<Object>();
  }

  public static Set setOf(Object[] elements) {
    return fromArray(elements);
  }

  public static Set setOf(Object element) {
    HashSet<Object> out = new HashSet<Object>();
    out.add(element);
    return out;
  }

  public static Set mutableSetOf(Object[] elements) {
    return fromArray(elements);
  }

  public static HashSet hashSetOf(Object[] elements) {
    HashSet<Object> out = new HashSet<Object>();
    for (Object e : elements) {
      out.add(e);
    }
    return out;
  }

  public static LinkedHashSet linkedSetOf(Object[] elements) {
    return fromArray(elements);
  }

  public static Set plus(Set source, Object element) {
    LinkedHashSet<Object> out = copy(source);
    out.add(element);
    return out;
  }

  public static Set plus(Set source, Iterable elements) {
    LinkedHashSet<Object> out = copy(source);
    for (Object e : elements) {
      out.add(e);
    }
    return out;
  }

  public static Set minus(Set source, Object element) {
    LinkedHashSet<Object> out = copy(source);
    out.remove(element);
    return out;
  }

  public static Set minus(Set source, Iterable elements) {
    HashSet<Object> other = CollectionsKt.toHashSet(elements);
    LinkedHashSet<Object> out = new LinkedHashSet<Object>();
    for (Object e : source) {
      if (!other.contains(e)) {
        out.add(e);
      }
    }
    return out;
  }
}
