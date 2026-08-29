// SPDX-License-Identifier: GPL-3.0-only
package kotlin.collections;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.HashSet;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.Set;
import kotlin.Pair;
import kotlin.jvm.functions.Function1;

/**
 * The non-inline part of {@code kotlin.collections} for arrays, one Java overload per element type
 * kotlinc references ({@code int[]}, {@code float[]}, {@code long[]}, {@code double[]}, {@code
 * char[]}, {@code Object[]}). {@code copyOf}, {@code contentToString}, {@code map}, {@code filter},
 * {@code forEach}, … are inline to {@code java.util.Arrays} builtins or loops.
 */
public final class ArraysKt {
  private ArraysKt() {}

  private static NoSuchElementException empty() {
    return new NoSuchElementException("Array is empty.");
  }

  private static IllegalArgumentException negativeCount(int n) {
    return new IllegalArgumentException("Requested element count " + n + " is less than zero.");
  }

  private static List boxed(int[] array) {
    ArrayList<Object> out = new ArrayList<Object>(array.length);
    for (int v : array) {
      out.add(Integer.valueOf(v));
    }
    return out;
  }

  private static List boxed(float[] array) {
    ArrayList<Object> out = new ArrayList<Object>(array.length);
    for (float v : array) {
      out.add(Float.valueOf(v));
    }
    return out;
  }

  private static List boxed(char[] array) {
    ArrayList<Object> out = new ArrayList<Object>(array.length);
    for (char v : array) {
      out.add(Character.valueOf(v));
    }
    return out;
  }

  private static List boxed(Object[] array) {
    ArrayList<Object> out = new ArrayList<Object>(array.length);
    for (Object v : array) {
      out.add(v);
    }
    return out;
  }

  // ── int[] ─────────────────────────────────────────────────────────────────

  public static int sum(int[] array) {
    int sum = 0;
    for (int v : array) {
      sum += v;
    }
    return sum;
  }

  public static double average(int[] array) {
    if (array.length == 0) {
      return Double.NaN;
    }
    double sum = 0;
    for (int v : array) {
      sum += v;
    }
    return sum / array.length;
  }

  public static int maxOrThrow(int[] array) {
    if (array.length == 0) {
      throw empty();
    }
    int max = array[0];
    for (int i = 1; i < array.length; i++) {
      if (array[i] > max) {
        max = array[i];
      }
    }
    return max;
  }

  public static int minOrThrow(int[] array) {
    if (array.length == 0) {
      throw empty();
    }
    int min = array[0];
    for (int i = 1; i < array.length; i++) {
      if (array[i] < min) {
        min = array[i];
      }
    }
    return min;
  }

  public static Integer maxOrNull(int[] array) {
    return array.length == 0 ? null : Integer.valueOf(maxOrThrow(array));
  }

  public static Integer minOrNull(int[] array) {
    return array.length == 0 ? null : Integer.valueOf(minOrThrow(array));
  }

  public static int indexOf(int[] array, int element) {
    for (int i = 0; i < array.length; i++) {
      if (array[i] == element) {
        return i;
      }
    }
    return -1;
  }

  public static boolean contains(int[] array, int element) {
    return indexOf(array, element) >= 0;
  }

  public static int getLastIndex(int[] array) {
    return array.length - 1;
  }

  public static int first(int[] array) {
    if (array.length == 0) {
      throw empty();
    }
    return array[0];
  }

  public static int last(int[] array) {
    if (array.length == 0) {
      throw empty();
    }
    return array[array.length - 1];
  }

  public static Integer getOrNull(int[] array, int index) {
    return index >= 0 && index < array.length ? Integer.valueOf(array[index]) : null;
  }

  public static List toList(int[] array) {
    return boxed(array);
  }

  public static Set toSet(int[] array) {
    LinkedHashSet<Object> out = new LinkedHashSet<Object>();
    for (int v : array) {
      out.add(Integer.valueOf(v));
    }
    return out;
  }

  public static List distinct(int[] array) {
    HashSet<Object> seen = new HashSet<Object>();
    ArrayList<Object> out = new ArrayList<Object>();
    for (int v : array) {
      Integer boxed = Integer.valueOf(v);
      if (seen.add(boxed)) {
        out.add(boxed);
      }
    }
    return out;
  }

  public static List take(int[] array, int n) {
    if (n < 0) {
      throw negativeCount(n);
    }
    int count = n < array.length ? n : array.length;
    ArrayList<Object> out = new ArrayList<Object>(count);
    for (int i = 0; i < count; i++) {
      out.add(Integer.valueOf(array[i]));
    }
    return out;
  }

  public static List drop(int[] array, int n) {
    if (n < 0) {
      throw negativeCount(n);
    }
    ArrayList<Object> out = new ArrayList<Object>();
    for (int i = n; i < array.length; i++) {
      out.add(Integer.valueOf(array[i]));
    }
    return out;
  }

  public static List zip(int[] a, Object[] b) {
    int n = a.length < b.length ? a.length : b.length;
    ArrayList<Object> out = new ArrayList<Object>(n);
    for (int i = 0; i < n; i++) {
      out.add(new Pair(Integer.valueOf(a[i]), b[i]));
    }
    return out;
  }

  public static int[] sortedArray(int[] array) {
    int[] copy = Arrays.copyOf(array, array.length);
    Arrays.sort(copy);
    return copy;
  }

  public static List sorted(int[] array) {
    return boxed(sortedArray(array));
  }

  public static void sort(int[] array) {
    Arrays.sort(array);
  }

  public static void sortDescending(int[] array) {
    Arrays.sort(array);
    reverse(array);
  }

  public static void reverse(int[] array) {
    for (int i = 0, j = array.length - 1; i < j; i++, j--) {
      int tmp = array[i];
      array[i] = array[j];
      array[j] = tmp;
    }
  }

  public static int[] reversedArray(int[] array) {
    int[] copy = Arrays.copyOf(array, array.length);
    reverse(copy);
    return copy;
  }

  public static List reversed(int[] array) {
    return boxed(reversedArray(array));
  }

  public static int[] copyOfRange(int[] array, int from, int to) {
    if (from < 0 || to > array.length || from > to) {
      throw new IndexOutOfBoundsException(
          "copyOfRange " + from + ".." + to + " of " + array.length);
    }
    int[] out = new int[to - from];
    for (int i = from; i < to; i++) {
      out[i - from] = array[i];
    }
    return out;
  }

  public static void fill(int[] array, int element, int from, int to) {
    if (from == 0 && to == array.length) {
      Arrays.fill(array, element);
      return;
    }
    for (int i = from; i < to; i++) {
      array[i] = element;
    }
  }

  public static void fill$default(
      int[] array, int element, int from, int to, int mask, Object marker) {
    if ((mask & 2) != 0) {
      from = 0;
    }
    if ((mask & 4) != 0) {
      to = array.length;
    }
    fill(array, element, from, to);
  }

  public static String joinToString$default(
      int[] array,
      CharSequence separator,
      CharSequence prefix,
      CharSequence postfix,
      int limit,
      CharSequence truncated,
      Function1 transform,
      int mask,
      Object marker) {
    return CollectionsKt.joinToString$default(
        boxed(array), separator, prefix, postfix, limit, truncated, transform, mask, marker);
  }

  // ── float[] ───────────────────────────────────────────────────────────────

  public static float sum(float[] array) {
    float sum = 0;
    for (float v : array) {
      sum += v;
    }
    return sum;
  }

  public static double average(float[] array) {
    if (array.length == 0) {
      return Double.NaN;
    }
    double sum = 0;
    for (float v : array) {
      sum += v;
    }
    return sum / array.length;
  }

  public static float maxOrThrow(float[] array) {
    if (array.length == 0) {
      throw empty();
    }
    float max = array[0];
    for (int i = 1; i < array.length; i++) {
      max = Math.max(max, array[i]);
    }
    return max;
  }

  public static float minOrThrow(float[] array) {
    if (array.length == 0) {
      throw empty();
    }
    float min = array[0];
    for (int i = 1; i < array.length; i++) {
      min = Math.min(min, array[i]);
    }
    return min;
  }

  public static Float maxOrNull(float[] array) {
    return array.length == 0 ? null : Float.valueOf(maxOrThrow(array));
  }

  public static Float minOrNull(float[] array) {
    return array.length == 0 ? null : Float.valueOf(minOrThrow(array));
  }

  public static int getLastIndex(float[] array) {
    return array.length - 1;
  }

  public static float first(float[] array) {
    if (array.length == 0) {
      throw empty();
    }
    return array[0];
  }

  public static float last(float[] array) {
    if (array.length == 0) {
      throw empty();
    }
    return array[array.length - 1];
  }

  public static List toList(float[] array) {
    return boxed(array);
  }

  public static List sorted(float[] array) {
    float[] copy = Arrays.copyOf(array, array.length);
    Arrays.sort(copy);
    return boxed(copy);
  }

  public static void fill(float[] array, float element, int from, int to) {
    if (from == 0 && to == array.length) {
      Arrays.fill(array, element);
      return;
    }
    for (int i = from; i < to; i++) {
      array[i] = element;
    }
  }

  public static void fill$default(
      float[] array, float element, int from, int to, int mask, Object marker) {
    if ((mask & 2) != 0) {
      from = 0;
    }
    if ((mask & 4) != 0) {
      to = array.length;
    }
    fill(array, element, from, to);
  }

  // ── long[] / double[] / char[] ────────────────────────────────────────────

  public static long sum(long[] array) {
    long sum = 0;
    for (long v : array) {
      sum += v;
    }
    return sum;
  }

  public static double sum(double[] array) {
    double sum = 0;
    for (double v : array) {
      sum += v;
    }
    return sum;
  }

  public static List sorted(char[] array) {
    char[] copy = Arrays.copyOf(array, array.length);
    Arrays.sort(copy);
    return boxed(copy);
  }

  public static String joinToString$default(
      char[] array,
      CharSequence separator,
      CharSequence prefix,
      CharSequence postfix,
      int limit,
      CharSequence truncated,
      Function1 transform,
      int mask,
      Object marker) {
    return CollectionsKt.joinToString$default(
        boxed(array), separator, prefix, postfix, limit, truncated, transform, mask, marker);
  }

  // ── Object[] ──────────────────────────────────────────────────────────────

  public static int indexOf(Object[] array, Object element) {
    for (int i = 0; i < array.length; i++) {
      Object e = array[i];
      if (e == null ? element == null : e.equals(element)) {
        return i;
      }
    }
    return -1;
  }

  public static boolean contains(Object[] array, Object element) {
    return indexOf(array, element) >= 0;
  }

  public static int getLastIndex(Object[] array) {
    return array.length - 1;
  }

  public static Object first(Object[] array) {
    if (array.length == 0) {
      throw empty();
    }
    return array[0];
  }

  public static Object last(Object[] array) {
    if (array.length == 0) {
      throw empty();
    }
    return array[array.length - 1];
  }

  public static List toList(Object[] array) {
    return boxed(array);
  }

  /** A copy, not the stdlib's write-through view (documented). */
  public static List asList(Object[] array) {
    return boxed(array);
  }

  public static List filterNotNull(Object[] array) {
    ArrayList<Object> out = new ArrayList<Object>();
    for (Object e : array) {
      if (e != null) {
        out.add(e);
      }
    }
    return out;
  }

  public static List sorted(Comparable[] array) {
    Object[] copy = Arrays.copyOf(array, array.length);
    Arrays.sort(copy);
    return boxed(copy);
  }

  public static List sortedDescending(Comparable[] array) {
    return sortedWith(array, kotlin.comparisons.ComparisonsKt.reverseOrder());
  }

  @SuppressWarnings("unchecked")
  public static List sortedWith(Object[] array, Comparator comparator) {
    Object[] copy = Arrays.copyOf(array, array.length);
    Arrays.sort(copy, comparator);
    return boxed(copy);
  }

  public static void sort(Object[] array) {
    Arrays.sort(array);
  }

  @SuppressWarnings("unchecked")
  public static void sortWith(Object[] array, Comparator comparator) {
    Arrays.sort(array, comparator);
  }

  public static String joinToString$default(
      Object[] array,
      CharSequence separator,
      CharSequence prefix,
      CharSequence postfix,
      int limit,
      CharSequence truncated,
      Function1 transform,
      int mask,
      Object marker) {
    return CollectionsKt.joinToString$default(
        boxed(array), separator, prefix, postfix, limit, truncated, transform, mask, marker);
  }
}
