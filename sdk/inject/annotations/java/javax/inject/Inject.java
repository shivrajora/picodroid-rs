// SPDX-License-Identifier: GPL-3.0-only
package javax.inject;

import java.lang.annotation.Documented;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks an injectable constructor, field, or method (JSR-330 {@code javax.inject.Inject}).
 *
 * <p>Picodroid resolves injection entirely at compile time: the {@code :inject:compiler} annotation
 * processor generates a {@code Foo_Factory} for every {@code @Inject} constructor and a {@code
 * Foo_MembersInjector} for every class with {@code @Inject} fields or methods. Framework-owned
 * components ({@code Application}, {@code Activity}, {@code Service}) have their members injected
 * automatically before {@code onCreate()}; everything else is built through its factory.
 *
 * <p>Divergence from JSR-330: retention is {@link RetentionPolicy#SOURCE}, not {@code RUNTIME}.
 * pico-jvm has no reflection and drops annotation attributes, so nothing about {@code @Inject} ever
 * reaches the device — only the generated classes do.
 */
@Documented
@Retention(RetentionPolicy.SOURCE)
@Target({ElementType.CONSTRUCTOR, ElementType.FIELD, ElementType.METHOD})
public @interface Inject {}
