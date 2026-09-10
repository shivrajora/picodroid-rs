// SPDX-License-Identifier: GPL-3.0-only
package kotlin.jvm.internal;

/**
 * The compiler-inserted intrinsics that survive the {@code -Xno-*-assertions} flags: {@code !!}
 * ({@link #checkNotNull(Object)}), the assertions copied out of inlined stdlib bodies ({@link
 * #checkNotNullExpressionValue}), {@code ==} on nullable references ({@link #areEqual}), {@code
 * lateinit} reads ({@link #throwUninitializedPropertyAccessException}). {@code
 * checkNotNullParameter} is deliberately absent — the flags remove every call to it, and its
 * absence is what keeps public methods free of a per-invocation intrinsic call. {@code compare} and
 * {@code stringPlus} are absent too: kotlinc 2.1 intrinsifies primitive {@code compareTo} and
 * compiles {@code String? + x} to a {@code StringBuilder} chain (the contract check proved both
 * unreferenced).
 */
public final class Intrinsics {
  private Intrinsics() {}

  public static void checkNotNull(Object object) {
    if (object == null) {
      throw new NullPointerException();
    }
  }

  public static void checkNotNull(Object object, String message) {
    if (object == null) {
      throw new NullPointerException(message);
    }
  }

  public static void checkNotNullExpressionValue(Object value, String expression) {
    if (value == null) {
      throw new NullPointerException(expression + " must not be null");
    }
  }

  /** {@code a == b} on references: null-safe {@code equals}. */
  public static boolean areEqual(Object first, Object second) {
    return first == null ? second == null : first.equals(second);
  }

  /** {@code Float? == Float}: null-safe IEEE equality (NaN != NaN, -0.0 == 0.0). */
  public static boolean areEqual(Float first, float second) {
    return first != null && first.floatValue() == second;
  }

  /** {@code Double? == Double}: null-safe IEEE equality. */
  public static boolean areEqual(Double first, double second) {
    return first != null && first.doubleValue() == second;
  }

  /** {@code a.compareTo(b)} on two {@code Int}s when kotlinc cannot fuse it into a branch. */
  public static int compare(int first, int second) {
    return first < second ? -1 : first == second ? 0 : 1;
  }

  public static void throwUninitializedPropertyAccessException(String propertyName) {
    throw new kotlin.UninitializedPropertyAccessException(
        "lateinit property " + propertyName + " has not been initialized");
  }
}
