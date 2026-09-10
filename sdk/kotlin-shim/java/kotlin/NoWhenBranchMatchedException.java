// SPDX-License-Identifier: GPL-3.0-only
package kotlin;

/** Thrown by the default branch kotlinc adds to an exhaustive {@code when} over a sealed type. */
public final class NoWhenBranchMatchedException extends RuntimeException {
  public NoWhenBranchMatchedException() {}
}
