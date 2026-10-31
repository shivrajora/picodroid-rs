// SPDX-License-Identifier: GPL-3.0-only
package javax.inject;

import java.lang.annotation.Documented;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Meta-annotation identifying qualifier annotations (JSR-330 {@code javax.inject.Qualifier}).
 *
 * <p>The picodroid processor knows one qualifier, {@link picodroid.di.ApplicationContext}, which
 * the framework binds; any other {@code @Qualifier} annotation on an injection site or a
 * {@code @Provides} method is a compile error until keyed bindings land. Present so the annotation
 * surface matches JSR-330 and so custom qualifiers fail loudly instead of being silently ignored.
 *
 * <p>Retention is {@link RetentionPolicy#SOURCE} (JSR-330 specifies {@code RUNTIME}); see {@link
 * Inject}.
 */
@Documented
@Retention(RetentionPolicy.SOURCE)
@Target(ElementType.ANNOTATION_TYPE)
public @interface Qualifier {}
