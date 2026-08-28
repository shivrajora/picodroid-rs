// SPDX-License-Identifier: GPL-3.0-only
package picodroid.shim;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Renames a shim method at strip time. kotlinc emits return-type-only overloads Java cannot declare
 * (e.g. {@code maxOrNull(Iterable)Float} next to {@code maxOrNull(Iterable)Comparable}); write them
 * under distinct Java names and annotate each with the name kotlinc calls. Shim-internal call sites
 * are rewritten too. CLASS retention: read by the strip, never shipped.
 */
@Retention(RetentionPolicy.CLASS)
@Target(ElementType.METHOD)
public @interface ShimName {
  String value();
}
