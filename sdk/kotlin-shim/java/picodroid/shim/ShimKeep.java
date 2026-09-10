// SPDX-License-Identifier: GPL-3.0-only
package picodroid.shim;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Exempts a shim method from static-method shaking, or a shim class from pruning, when the
 * reference that keeps it alive is not visible in class-file constant pools (reflection-free
 * dispatch by runtime class, for instance). CLASS retention: read by the strip, never shipped.
 */
@Retention(RetentionPolicy.CLASS)
@Target({ElementType.METHOD, ElementType.TYPE})
public @interface ShimKeep {}
