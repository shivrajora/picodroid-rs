// SPDX-License-Identifier: GPL-3.0-only
package kotlin.jvm.internal;

/**
 * The trailing parameter type of every synthetic default-argument constructor kotlinc emits ({@code
 * Foo(String, int mask, DefaultConstructorMarker)}). Only ever passed as {@code null}, so it is
 * descriptor-only at run time and the strip prunes it from every PAPK; it exists so the shim's own
 * default-argument constructors ({@link kotlin.NotImplementedError}) compile.
 */
public final class DefaultConstructorMarker {
  private DefaultConstructorMarker() {}
}
