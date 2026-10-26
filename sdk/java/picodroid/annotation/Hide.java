// SPDX-License-Identifier: GPL-3.0-only
package picodroid.annotation;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks a member the framework reaches across its own packages but an app may not call: Android's
 * {@code @hide}. Java has no visibility between "package" and "public", so these stay {@code
 * public} in the SDK, and {@code verifyApiContract} fails an app build that references one (or an
 * override of one), printing {@link #value} as the hint: the Android spelling to use instead.
 *
 * <p>Compile-only: the class is never embedded ({@code COMPILE_ONLY_CLASSES} in {@code
 * build_support/papk.rs}), and the firmware's class strip drops the annotation from every member.
 */
@Retention(RetentionPolicy.CLASS)
@Target({ElementType.METHOD, ElementType.FIELD, ElementType.CONSTRUCTOR})
public @interface Hide {
  /** What an app writes instead, for the build error. */
  String value() default "";
}
