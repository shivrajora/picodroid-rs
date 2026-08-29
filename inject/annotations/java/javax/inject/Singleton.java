// SPDX-License-Identifier: GPL-3.0-only
package javax.inject;

import java.lang.annotation.Documented;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Scopes an {@code @Inject}-constructed class to one instance per process (JSR-330 {@code
 * javax.inject.Singleton}).
 *
 * <p>The generated {@code Foo_Factory.get()} creates the instance on first use and caches it in a
 * static field (a GC root shared by every thread), so every injection site and every {@code
 * Foo_Factory.get()} call observes the same object — the picodroid equivalent of an
 * application-scoped binding. Unscoped classes get a fresh instance per injection.
 *
 * <p>Retention is {@link RetentionPolicy#SOURCE} (JSR-330 specifies {@code RUNTIME}); see {@link
 * Inject}. {@code METHOD} is in the target list for the future {@code @Provides} shape; today the
 * processor rejects {@code @Singleton} on methods.
 */
@Documented
@Retention(RetentionPolicy.SOURCE)
@Target({ElementType.TYPE, ElementType.METHOD})
@Scope
public @interface Singleton {}
