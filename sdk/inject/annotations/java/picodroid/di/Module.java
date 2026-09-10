// SPDX-License-Identifier: GPL-3.0-only
package picodroid.di;

import java.lang.annotation.Documented;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks a class whose {@link Provides @Provides} methods contribute bindings to the app's object
 * graph — the picodroid counterpart of {@code dagger.Module}.
 *
 * <p>There is a single implicit application component, so every {@code @Module} in the app is
 * installed automatically; there is no {@code @Component(modules = ...)} to write.
 * {@code @Provides} methods may be {@code static} (preferred — no module instance is ever created)
 * or instance methods, in which case the module needs a non-private no-arg constructor and is
 * instantiated once, lazily, as a singleton. A module is not itself an injection target: it cannot
 * have an {@code @Inject} constructor or {@code @Inject} members.
 *
 * <p>Compile-time only ({@link RetentionPolicy#SOURCE}); see {@code
 * docs/designs/inject-annotations-2026-08.md}.
 */
@Documented
@Retention(RetentionPolicy.SOURCE)
@Target(ElementType.TYPE)
public @interface Module {}
