// SPDX-License-Identifier: GPL-3.0-only
package kotlin;

/** Reading a {@code lateinit} property before it is assigned (thrown by {@code Intrinsics}). */
public final class UninitializedPropertyAccessException extends RuntimeException {
  public UninitializedPropertyAccessException(String message) {
    super(message);
  }
}
