package java.util;

public final class Collections {
  private Collections() {}

  /**
   * Sorts a list in natural (Comparable) order. Stable per Java spec.
   *
   * <p>Implementation: pulls elements into a Object[], delegates to {@link Arrays#sort(Object[])},
   * writes back via {@code List.set}.
   */
  public static void sort(List list) {
    int n = list.size();
    if (n < 2) {
      return;
    }
    Object[] arr = new Object[n];
    for (int i = 0; i < n; i++) {
      arr[i] = list.get(i);
    }
    Arrays.sort(arr);
    for (int i = 0; i < n; i++) {
      list.set(i, arr[i]);
    }
  }

  /** Reverses a list in place. */
  public static void reverse(List list) {
    int n = list.size();
    for (int i = 0; i < n / 2; i++) {
      int j = n - 1 - i;
      Object tmp = list.get(i);
      list.set(i, list.get(j));
      list.set(j, tmp);
    }
  }
}
