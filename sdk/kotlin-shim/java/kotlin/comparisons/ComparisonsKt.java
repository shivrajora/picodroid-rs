// SPDX-License-Identifier: GPL-3.0-only
package kotlin.comparisons;

import java.util.Comparator;
import kotlin.jvm.functions.Function1;

/**
 * {@code compareValues} (the tail of every inlined {@code sortedBy} / {@code compareBy} lambda),
 * the vararg {@code compareBy}, {@code naturalOrder} / {@code reverseOrder}, {@code minOf} on
 * {@code Comparable}s ({@code maxOf}/{@code minOf} on primitives inline to {@code Math}).
 */
public final class ComparisonsKt {
  private ComparisonsKt() {}

  @SuppressWarnings({"unchecked", "rawtypes"})
  public static int compareValues(Comparable a, Comparable b) {
    if (a == b) {
      return 0;
    }
    if (a == null) {
      return -1;
    }
    if (b == null) {
      return 1;
    }
    return a.compareTo(b);
  }

  @SuppressWarnings({"unchecked", "rawtypes"})
  public static Comparable minOf(Comparable a, Comparable b) {
    return a.compareTo(b) <= 0 ? a : b;
  }

  @SuppressWarnings({"unchecked", "rawtypes"})
  public static Comparable maxOf(Comparable a, Comparable b) {
    return a.compareTo(b) >= 0 ? a : b;
  }

  @SuppressWarnings("rawtypes")
  public static Comparator compareBy(Function1[] selectors) {
    if (selectors.length == 0) {
      throw new IllegalArgumentException("Failed requirement.");
    }
    return new SelectorsComparator(selectors);
  }

  @SuppressWarnings("rawtypes")
  public static Comparator naturalOrder() {
    return NaturalOrderComparator.INSTANCE;
  }

  @SuppressWarnings("rawtypes")
  public static Comparator reverseOrder() {
    return ReverseOrderComparator.INSTANCE;
  }

  @SuppressWarnings({"unchecked", "rawtypes"})
  private static final class SelectorsComparator implements Comparator<Object> {
    private final Function1[] selectors;

    SelectorsComparator(Function1[] selectors) {
      this.selectors = selectors;
    }

    @Override
    public int compare(Object a, Object b) {
      for (Function1 selector : selectors) {
        int diff = compareValues((Comparable) selector.invoke(a), (Comparable) selector.invoke(b));
        if (diff != 0) {
          return diff;
        }
      }
      return 0;
    }
  }

  @SuppressWarnings({"unchecked", "rawtypes"})
  private static final class NaturalOrderComparator implements Comparator<Object> {
    static final NaturalOrderComparator INSTANCE = new NaturalOrderComparator();

    @Override
    public int compare(Object a, Object b) {
      return ((Comparable) a).compareTo(b);
    }
  }

  @SuppressWarnings({"unchecked", "rawtypes"})
  private static final class ReverseOrderComparator implements Comparator<Object> {
    static final ReverseOrderComparator INSTANCE = new ReverseOrderComparator();

    @Override
    public int compare(Object a, Object b) {
      return ((Comparable) b).compareTo(a);
    }
  }
}
