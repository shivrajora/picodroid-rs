// SPDX-License-Identifier: GPL-3.0-only
package javax.inject;

import java.lang.annotation.Documented;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Meta-annotation identifying scope annotations (JSR-330 {@code javax.inject.Scope}).
 *
 * <p>Only {@link Singleton} is supported by the picodroid processor today; any other {@code @Scope}
 * annotation applied to an injectable class is a compile error. Present so the annotation surface
 * matches JSR-330 and so custom scopes fail loudly instead of silently unscoped.
 *
 * <p>Retention is {@link RetentionPolicy#SOURCE} (JSR-330 specifies {@code RUNTIME}); see {@link
 * Inject}.
 */
@Documented
@Retention(RetentionPolicy.SOURCE)
@Target(ElementType.ANNOTATION_TYPE)
public @interface Scope {}
