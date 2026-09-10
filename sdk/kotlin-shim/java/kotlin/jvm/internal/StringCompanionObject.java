// SPDX-License-Identifier: GPL-3.0-only
package kotlin.jvm.internal;

/**
 * The receiver kotlinc loads for {@code String.format(...)} and the other {@code String.Companion}
 * extensions before inlining their bodies; the object itself is never used.
 */
public final class StringCompanionObject {
  public static final StringCompanionObject INSTANCE = new StringCompanionObject();

  private StringCompanionObject() {}
}
