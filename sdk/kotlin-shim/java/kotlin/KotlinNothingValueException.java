// SPDX-License-Identifier: GPL-3.0-only
package kotlin;

/**
 * kotlinc emits a {@code throw} of this right after every call to a non-inline function returning
 * {@code Nothing}, for the case where that function returns anyway.
 */
public final class KotlinNothingValueException extends RuntimeException {
  public KotlinNothingValueException() {}
}
