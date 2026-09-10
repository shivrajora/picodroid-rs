// SPDX-License-Identifier: GPL-3.0-only
package kotlin;

/**
 * The value of a {@code Unit}-typed expression. Kotlin lambdas returning {@code Unit} return {@link
 * #INSTANCE}.
 */
public final class Unit {
  public static final Unit INSTANCE = new Unit();

  private Unit() {}

  @Override
  public String toString() {
    return "kotlin.Unit";
  }
}
