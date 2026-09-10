// SPDX-License-Identifier: GPL-3.0-only
package kotlin.collections;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.Set;
import kotlin.Pair;
import kotlin.jvm.functions.Function1;
import kotlin.ranges.IntRange;
import picodroid.shim.ShimName;

/**
 * The non-inline part of {@code kotlin.collections} for lists and iterables. Every method here is
 * one the fixture apps reference ({@code :kotlin-shim:contractCheck}); the inline HOFs ({@code
 * map}, {@code filter}, {@code forEach}, {@code any}, …) never reach this class.
 *
 * <p>Divergences (compatibility matrix): read-only factories return plain {@code ArrayList}s;
 * {@code toSet}/{@code distinct}/{@code union} use the hash-ordered {@code HashSet}; {@code
 * emptyList()} returns a fresh instance per call (Kotlin's is an immutable singleton).
 */
public final class CollectionsKt {
  private CollectionsKt() {}

  // ── helpers ───────────────────────────────────────────────────────────────

  private static ArrayList<Object> copy(Iterable<?> source) {
    ArrayList<Object> out =
        source instanceof Collection
            ? new ArrayList<Object>(((Collection<?>) source).size())
            : new ArrayList<Object>();
    for (Object e : source) {
      out.add(e);
    }
    return out;
  }

  private static ArrayList<Object> fromArray(Object[] array) {
    ArrayList<Object> out = new ArrayList<Object>(array.length);
    for (Object e : array) {
      out.add(e);
    }
    return out;
  }

  private static boolean same(Object a, Object b) {
    return a == null ? b == null : a.equals(b);
  }

  private static NoSuchElementException empty(String what) {
    return new NoSuchElementException(what + " is empty.");
  }

  // ── factories ─────────────────────────────────────────────────────────────

  /**
   * A fresh (mutable) empty list per call. Kotlin's is an immutable singleton; this shim has no
   * unmodifiable wrapper, so a shared instance would let one {@code (it as MutableList).add(x)} —
   * or any Java interop that appends — poison every later {@code emptyList()} app-wide.
   */
  public static List emptyList() {
    return new ArrayList<Object>(0);
  }

  public static List listOf(Object[] elements) {
    return fromArray(elements);
  }

  public static List listOf(Object element) {
    ArrayList<Object> out = new ArrayList<Object>(1);
    out.add(element);
    return out;
  }

  public static List mutableListOf(Object[] elements) {
    return fromArray(elements);
  }

  public static List listOfNotNull(Object[] elements) {
    ArrayList<Object> out = new ArrayList<Object>();
    for (Object e : elements) {
      if (e != null) {
        out.add(e);
      }
    }
    return out;
  }

  public static List toList(Iterable source) {
    return copy(source);
  }

  public static List toMutableList(Collection source) {
    return copy(source);
  }

  public static Set toSet(Iterable source) {
    LinkedHashSet<Object> out = new LinkedHashSet<Object>();
    for (Object e : source) {
      out.add(e);
    }
    return out;
  }

  public static Set toMutableSet(Iterable source) {
    return toSet(source);
  }

  public static HashSet toHashSet(Iterable source) {
    HashSet<Object> out = new HashSet<Object>();
    for (Object e : source) {
      out.add(e);
    }
    return out;
  }

  public static int[] toIntArray(Collection source) {
    int[] out = new int[source.size()];
    int i = 0;
    for (Object e : source) {
      out[i++] = ((Integer) e).intValue();
    }
    return out;
  }

  public static float[] toFloatArray(Collection source) {
    float[] out = new float[source.size()];
    int i = 0;
    for (Object e : source) {
      out[i++] = ((Float) e).floatValue();
    }
    return out;
  }

  // ── inline-HOF helpers ────────────────────────────────────────────────────

  public static int collectionSizeOrDefault(Iterable source, int dflt) {
    return source instanceof Collection ? ((Collection) source).size() : dflt;
  }

  public static void throwIndexOverflow() {
    throw new ArithmeticException("Index overflow has happened.");
  }

  public static void throwCountOverflow() {
    throw new ArithmeticException("Count overflow has happened.");
  }

  // ── accessors ─────────────────────────────────────────────────────────────

  public static Object first(List list) {
    if (list.isEmpty()) {
      throw empty("List");
    }
    return list.get(0);
  }

  public static Object first(Iterable source) {
    if (source instanceof List) {
      return first((List) source);
    }
    Iterator it = source.iterator();
    if (!it.hasNext()) {
      throw empty("Collection");
    }
    return it.next();
  }

  public static Object firstOrNull(List list) {
    return list.isEmpty() ? null : list.get(0);
  }

  public static Object firstOrNull(Iterable source) {
    if (source instanceof List) {
      return firstOrNull((List) source);
    }
    Iterator it = source.iterator();
    return it.hasNext() ? it.next() : null;
  }

  public static Object last(List list) {
    if (list.isEmpty()) {
      throw empty("List");
    }
    return list.get(list.size() - 1);
  }

  public static Object lastOrNull(List list) {
    return list.isEmpty() ? null : list.get(list.size() - 1);
  }

  public static Object single(List list) {
    if (list.isEmpty()) {
      throw empty("List");
    }
    if (list.size() != 1) {
      throw new IllegalArgumentException("List has more than one element.");
    }
    return list.get(0);
  }

  public static Object getOrNull(List list, int index) {
    return index >= 0 && index < list.size() ? list.get(index) : null;
  }

  public static int getLastIndex(List list) {
    return list.size() - 1;
  }

  public static IntRange getIndices(Collection collection) {
    return new IntRange(0, collection.size() - 1);
  }

  public static int count(Iterable source) {
    if (source instanceof Collection) {
      return ((Collection) source).size();
    }
    int n = 0;
    for (Iterator it = source.iterator(); it.hasNext(); it.next()) {
      n++;
    }
    return n;
  }

  public static int indexOf(Iterable source, Object element) {
    int i = 0;
    for (Object e : source) {
      if (same(e, element)) {
        return i;
      }
      i++;
    }
    return -1;
  }

  // ── aggregation ───────────────────────────────────────────────────────────

  public static int sumOfInt(Iterable source) {
    int sum = 0;
    for (Object e : source) {
      sum += ((Integer) e).intValue();
    }
    return sum;
  }

  public static long sumOfLong(Iterable source) {
    long sum = 0;
    for (Object e : source) {
      sum += ((Long) e).longValue();
    }
    return sum;
  }

  public static float sumOfFloat(Iterable source) {
    float sum = 0;
    for (Object e : source) {
      sum += ((Float) e).floatValue();
    }
    return sum;
  }

  public static double sumOfDouble(Iterable source) {
    double sum = 0;
    for (Object e : source) {
      sum += ((Double) e).doubleValue();
    }
    return sum;
  }

  public static double averageOfInt(Iterable source) {
    double sum = 0;
    int n = 0;
    for (Object e : source) {
      sum += ((Integer) e).intValue();
      n++;
    }
    return n == 0 ? Double.NaN : sum / n;
  }

  public static double averageOfDouble(Iterable source) {
    double sum = 0;
    int n = 0;
    for (Object e : source) {
      sum += ((Double) e).doubleValue();
      n++;
    }
    return n == 0 ? Double.NaN : sum / n;
  }

  @SuppressWarnings({"unchecked", "rawtypes"})
  public static Comparable maxOrNull(Iterable source) {
    Comparable max = null;
    boolean seen = false;
    for (Object e : source) {
      if (!seen || ((Comparable) e).compareTo(max) > 0) {
        max = (Comparable) e;
        seen = true;
      }
    }
    return max;
  }

  @SuppressWarnings({"unchecked", "rawtypes"})
  public static Comparable minOrNull(Iterable source) {
    Comparable min = null;
    boolean seen = false;
    for (Object e : source) {
      if (!seen || ((Comparable) e).compareTo(min) < 0) {
        min = (Comparable) e;
        seen = true;
      }
    }
    return min;
  }

  @SuppressWarnings("rawtypes")
  public static Comparable maxOrThrow(Iterable source) {
    Iterator it = source.iterator();
    if (!it.hasNext()) {
      throw empty("Collection");
    }
    return maxOrNull(source);
  }

  @SuppressWarnings("rawtypes")
  public static Comparable minOrThrow(Iterable source) {
    Iterator it = source.iterator();
    if (!it.hasNext()) {
      throw empty("Collection");
    }
    return minOrNull(source);
  }

  /** {@code Iterable<Float>.maxOrNull()} — the return-type-only overload. */
  @ShimName("maxOrNull")
  public static Float maxOrNullFloat(Iterable source) {
    Iterator it = source.iterator();
    if (!it.hasNext()) {
      return null;
    }
    float max = ((Float) it.next()).floatValue();
    while (it.hasNext()) {
      max = Math.max(max, ((Float) it.next()).floatValue());
    }
    return Float.valueOf(max);
  }

  /** {@code Iterable<Float>.min()}. */
  @ShimName("minOrThrow")
  public static float minOrThrowFloat(Iterable source) {
    Iterator it = source.iterator();
    if (!it.hasNext()) {
      throw empty("Collection");
    }
    float min = ((Float) it.next()).floatValue();
    while (it.hasNext()) {
      min = Math.min(min, ((Float) it.next()).floatValue());
    }
    return min;
  }

  /** {@code Iterable<Double>.max()}. */
  @ShimName("maxOrThrow")
  public static double maxOrThrowDouble(Iterable source) {
    Iterator it = source.iterator();
    if (!it.hasNext()) {
      throw empty("Collection");
    }
    double max = ((Double) it.next()).doubleValue();
    while (it.hasNext()) {
      max = Math.max(max, ((Double) it.next()).doubleValue());
    }
    return max;
  }

  @SuppressWarnings("unchecked")
  public static Object maxWithOrThrow(Iterable source, Comparator comparator) {
    Iterator it = source.iterator();
    if (!it.hasNext()) {
      throw empty("Collection");
    }
    Object max = it.next();
    while (it.hasNext()) {
      Object e = it.next();
      if (comparator.compare(max, e) < 0) {
        max = e;
      }
    }
    return max;
  }

  @SuppressWarnings("unchecked")
  public static Object minWithOrNull(Iterable source, Comparator comparator) {
    Iterator it = source.iterator();
    if (!it.hasNext()) {
      return null;
    }
    Object min = it.next();
    while (it.hasNext()) {
      Object e = it.next();
      if (comparator.compare(min, e) > 0) {
        min = e;
      }
    }
    return min;
  }

  // ── slicing / reshaping ───────────────────────────────────────────────────

  public static List take(Iterable source, int n) {
    if (n < 0) {
      throw new IllegalArgumentException("Requested element count " + n + " is less than zero.");
    }
    ArrayList<Object> out = new ArrayList<Object>(n);
    if (n == 0) {
      return out;
    }
    for (Object e : source) {
      out.add(e);
      if (out.size() == n) {
        break;
      }
    }
    return out;
  }

  public static List drop(Iterable source, int n) {
    if (n < 0) {
      throw new IllegalArgumentException("Requested element count " + n + " is less than zero.");
    }
    ArrayList<Object> out = new ArrayList<Object>();
    int i = 0;
    for (Object e : source) {
      if (i++ >= n) {
        out.add(e);
      }
    }
    return out;
  }

  public static List takeLast(List list, int n) {
    if (n < 0) {
      throw new IllegalArgumentException("Requested element count " + n + " is less than zero.");
    }
    int size = list.size();
    int from = n >= size ? 0 : size - n;
    ArrayList<Object> out = new ArrayList<Object>(size - from);
    for (int i = from; i < size; i++) {
      out.add(list.get(i));
    }
    return out;
  }

  public static List dropLast(List list, int n) {
    if (n < 0) {
      throw new IllegalArgumentException("Requested element count " + n + " is less than zero.");
    }
    int keep = list.size() - n;
    return take(list, keep < 0 ? 0 : keep);
  }

  public static List reversed(Iterable source) {
    ArrayList<Object> out = copy(source);
    Collections.reverse(out);
    return out;
  }

  public static List distinct(Iterable source) {
    HashSet<Object> seen = new HashSet<Object>();
    ArrayList<Object> out = new ArrayList<Object>();
    for (Object e : source) {
      if (seen.add(e)) {
        out.add(e);
      }
    }
    return out;
  }

  public static List filterNotNull(Iterable source) {
    ArrayList<Object> out = new ArrayList<Object>();
    for (Object e : source) {
      if (e != null) {
        out.add(e);
      }
    }
    return out;
  }

  public static List flatten(Iterable source) {
    ArrayList<Object> out = new ArrayList<Object>();
    for (Object inner : source) {
      for (Object e : (Iterable) inner) {
        out.add(e);
      }
    }
    return out;
  }

  public static List zip(Iterable a, Iterable b) {
    ArrayList<Object> out = new ArrayList<Object>();
    Iterator ia = a.iterator();
    Iterator ib = b.iterator();
    while (ia.hasNext() && ib.hasNext()) {
      out.add(new Pair(ia.next(), ib.next()));
    }
    return out;
  }

  public static List chunked(Iterable source, int size) {
    if (size <= 0) {
      throw new IllegalArgumentException("size " + size + " must be greater than zero.");
    }
    ArrayList<Object> out = new ArrayList<Object>();
    ArrayList<Object> chunk = new ArrayList<Object>(size);
    for (Object e : source) {
      chunk.add(e);
      if (chunk.size() == size) {
        out.add(chunk);
        chunk = new ArrayList<Object>(size);
      }
    }
    if (!chunk.isEmpty()) {
      out.add(chunk);
    }
    return out;
  }

  public static List plus(Collection source, Object element) {
    ArrayList<Object> out = new ArrayList<Object>(source.size() + 1);
    for (Object e : source) {
      out.add(e);
    }
    out.add(element);
    return out;
  }

  public static List plus(Collection source, Iterable elements) {
    ArrayList<Object> out = copy(source);
    for (Object e : elements) {
      out.add(e);
    }
    return out;
  }

  /** Removes the <em>first</em> occurrence only, as the stdlib does. */
  public static List minus(Iterable source, Object element) {
    ArrayList<Object> out = new ArrayList<Object>();
    boolean removed = false;
    for (Object e : source) {
      if (!removed && same(e, element)) {
        removed = true;
      } else {
        out.add(e);
      }
    }
    return out;
  }

  public static Set union(Iterable a, Iterable b) {
    LinkedHashSet<Object> out = new LinkedHashSet<Object>();
    for (Object e : a) {
      out.add(e);
    }
    for (Object e : b) {
      out.add(e);
    }
    return out;
  }

  public static Set intersect(Iterable a, Iterable b) {
    HashSet<Object> other = toHashSet(b);
    LinkedHashSet<Object> out = new LinkedHashSet<Object>();
    for (Object e : a) {
      if (other.contains(e)) {
        out.add(e);
      }
    }
    return out;
  }

  public static Set subtract(Iterable a, Iterable b) {
    HashSet<Object> other = toHashSet(b);
    LinkedHashSet<Object> out = new LinkedHashSet<Object>();
    for (Object e : a) {
      if (!other.contains(e)) {
        out.add(e);
      }
    }
    return out;
  }

  // ── mutation ──────────────────────────────────────────────────────────────

  @SuppressWarnings("unchecked")
  public static boolean addAll(Collection target, Iterable elements) {
    boolean changed = false;
    for (Object e : elements) {
      if (target.add(e)) {
        changed = true;
      }
    }
    return changed;
  }

  public static void reverse(List list) {
    Collections.reverse(list);
  }

  // ── sorting ───────────────────────────────────────────────────────────────

  public static List sorted(Iterable source) {
    Object[] array = copy(source).toArray();
    Arrays.sort(array);
    return fromArray(array);
  }

  @SuppressWarnings("unchecked")
  public static List sortedWith(Iterable source, Comparator comparator) {
    Object[] array = copy(source).toArray();
    Arrays.sort(array, comparator);
    return fromArray(array);
  }

  public static List sortedDescending(Iterable source) {
    return sortedWith(source, kotlin.comparisons.ComparisonsKt.reverseOrder());
  }

  @SuppressWarnings("unchecked")
  public static void sort(List list) {
    Collections.sort(list);
  }

  @SuppressWarnings("unchecked")
  public static void sortWith(List list, Comparator comparator) {
    Collections.sort(list, comparator);
  }

  public static void sortDescending(List list) {
    sortWith(list, kotlin.comparisons.ComparisonsKt.reverseOrder());
  }

  // ── joining ───────────────────────────────────────────────────────────────

  @SuppressWarnings("unchecked")
  public static String joinToString(
      Iterable source,
      CharSequence separator,
      CharSequence prefix,
      CharSequence postfix,
      int limit,
      CharSequence truncated,
      Function1 transform) {
    StringBuilder sb = new StringBuilder();
    sb.append(prefix.toString());
    int count = 0;
    for (Object e : source) {
      if (++count > 1) {
        sb.append(separator.toString());
      }
      if (limit < 0 || count <= limit) {
        appendElement(sb, e, transform);
      } else {
        break;
      }
    }
    if (limit >= 0 && count > limit) {
      sb.append(truncated.toString());
    }
    sb.append(postfix.toString());
    return sb.toString();
  }

  /**
   * Shared with {@code ArraysKt}: the element's {@code toString()} via the append(Object)
   * trampoline.
   */
  @SuppressWarnings("unchecked")
  static void appendElement(StringBuilder sb, Object element, Function1 transform) {
    if (transform != null) {
      sb.append(transform.invoke(element));
    } else if (element instanceof Character) {
      sb.append(((Character) element).charValue());
    } else {
      sb.append(element);
    }
  }

  /** Mask bit i = the i-th parameter after the receiver takes its default. */
  public static String joinToString$default(
      Iterable source,
      CharSequence separator,
      CharSequence prefix,
      CharSequence postfix,
      int limit,
      CharSequence truncated,
      Function1 transform,
      int mask,
      Object marker) {
    if ((mask & 1) != 0) {
      separator = ", ";
    }
    if ((mask & 2) != 0) {
      prefix = "";
    }
    if ((mask & 4) != 0) {
      postfix = "";
    }
    if ((mask & 8) != 0) {
      limit = -1;
    }
    if ((mask & 16) != 0) {
      truncated = "...";
    }
    if ((mask & 32) != 0) {
      transform = null;
    }
    return joinToString(source, separator, prefix, postfix, limit, truncated, transform);
  }
}
