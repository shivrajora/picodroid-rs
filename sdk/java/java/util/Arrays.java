// SPDX-License-Identifier: GPL-3.0-only
package java.util;

public final class Arrays {
  private Arrays() {}

  // ── Primitive overloads — implemented natively in jvm/src/native/arrays.rs.

  public static native void sort(int[] a);

  public static native void sort(long[] a);

  public static native void sort(double[] a);

  public static native void sort(float[] a);

  public static native void sort(short[] a);

  public static native void sort(byte[] a);

  public static native void sort(char[] a);

  public static native void fill(int[] a, int val);

  public static native void fill(long[] a, long val);

  public static native void fill(double[] a, double val);

  public static native void fill(float[] a, float val);

  public static native void fill(short[] a, short val);

  public static native void fill(byte[] a, byte val);

  public static native void fill(char[] a, char val);

  public static native int[] copyOf(int[] a, int newLength);

  public static native long[] copyOf(long[] a, int newLength);

  public static native double[] copyOf(double[] a, int newLength);

  public static native float[] copyOf(float[] a, int newLength);

  public static native short[] copyOf(short[] a, int newLength);

  public static native byte[] copyOf(byte[] a, int newLength);

  public static native char[] copyOf(char[] a, int newLength);

  public static native String toString(int[] a);

  public static native String toString(long[] a);

  public static native String toString(double[] a);

  public static native String toString(float[] a);

  public static native String toString(short[] a);

  public static native String toString(byte[] a);

  public static native String toString(char[] a);

  // ── Object[] sort — Java implementation using Comparable.compareTo.
  //
  // Bottom-up iterative merge sort; stable per Java spec. Cannot be native
  // because native code in this JVM cannot upcall into Java to invoke
  // Comparable.compareTo on user objects.

  public static void sort(Object[] a) {
    if (a == null || a.length < 2) {
      return;
    }
    Object[] aux = new Object[a.length];
    int n = a.length;
    for (int width = 1; width < n; width *= 2) {
      for (int i = 0; i < n; i += 2 * width) {
        int mid = i + width;
        if (mid > n) {
          mid = n;
        }
        int hi = i + 2 * width;
        if (hi > n) {
          hi = n;
        }
        merge(a, aux, i, mid, hi);
      }
    }
  }

  @SuppressWarnings({"rawtypes", "unchecked"})
  private static void merge(Object[] a, Object[] aux, int lo, int mid, int hi) {
    for (int k = lo; k < hi; k++) {
      aux[k] = a[k];
    }
    int i = lo;
    int j = mid;
    for (int k = lo; k < hi; k++) {
      if (i >= mid) {
        a[k] = aux[j++];
      } else if (j >= hi) {
        a[k] = aux[i++];
      } else if (((Comparable) aux[i]).compareTo(aux[j]) <= 0) {
        a[k] = aux[i++];
      } else {
        a[k] = aux[j++];
      }
    }
  }

  /**
   * Sorts using the supplied comparator. Stable per Java spec. A {@code null} comparator means
   * natural (Comparable) ordering, mirroring the JDK.
   */
  public static <T> void sort(T[] a, Comparator<? super T> c) {
    if (c == null) {
      sort((Object[]) a);
      return;
    }
    if (a == null || a.length < 2) {
      return;
    }
    Object[] aux = new Object[a.length];
    int n = a.length;
    for (int width = 1; width < n; width *= 2) {
      for (int i = 0; i < n; i += 2 * width) {
        int mid = i + width;
        if (mid > n) {
          mid = n;
        }
        int hi = i + 2 * width;
        if (hi > n) {
          hi = n;
        }
        merge(a, aux, i, mid, hi, c);
      }
    }
  }

  @SuppressWarnings("unchecked")
  private static <T> void merge(
      T[] a, Object[] aux, int lo, int mid, int hi, Comparator<? super T> c) {
    for (int k = lo; k < hi; k++) {
      aux[k] = a[k];
    }
    int i = lo;
    int j = mid;
    for (int k = lo; k < hi; k++) {
      if (i >= mid) {
        a[k] = (T) aux[j++];
      } else if (j >= hi) {
        a[k] = (T) aux[i++];
      } else if (c.compare((T) aux[i], (T) aux[j]) <= 0) {
        a[k] = (T) aux[i++];
      } else {
        a[k] = (T) aux[j++];
      }
    }
  }

  public static String toString(Object[] a) {
    if (a == null) {
      return "null";
    }
    if (a.length == 0) {
      return "[]";
    }
    StringBuilder sb = new StringBuilder("[");
    for (int i = 0; i < a.length; i++) {
      if (i > 0) {
        sb.append(", ");
      }
      sb.append(a[i] == null ? "null" : a[i].toString());
    }
    sb.append(']');
    return sb.toString();
  }
}
