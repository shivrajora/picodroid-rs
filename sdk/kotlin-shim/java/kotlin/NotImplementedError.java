// SPDX-License-Identifier: GPL-3.0-only
package kotlin;

import kotlin.jvm.internal.DefaultConstructorMarker;

/**
 * {@code TODO()} / {@code TODO("reason")}. An {@code Error}, as in the real stdlib, whose single
 * constructor has a default argument — so {@code TODO()} calls the synthetic {@code (String, int,
 * DefaultConstructorMarker)} form with the "argument omitted" bit set.
 */
public final class NotImplementedError extends Error {
  private static final String DEFAULT_MESSAGE = "An operation is not implemented.";

  public NotImplementedError(String message) {
    super(message);
  }

  /**
   * kotlinc's default-argument bridge: bit 0 of {@code mask} set means {@code message} was omitted.
   */
  public NotImplementedError(String message, int mask, DefaultConstructorMarker marker) {
    super((mask & 1) != 0 ? DEFAULT_MESSAGE : message);
  }
}
