// SPDX-License-Identifier: GPL-3.0-only
package kotlin.jvm.internal;

/**
 * The compiler-inserted null checks that survive the {@code -Xno-*-assertions} flags: {@code !!}
 * ({@link #checkNotNull(Object)}) and the assertions copied out of inlined stdlib bodies ({@link
 * #checkNotNullExpressionValue}). {@code checkNotNullParameter} is deliberately absent — the flags
 * remove every call to it, and its absence is what keeps public methods free of a per-invocation
 * intrinsic call.
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
}
