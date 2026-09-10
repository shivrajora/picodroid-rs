// SPDX-License-Identifier: GPL-3.0-only
package picodroid.di;

import java.lang.annotation.Documented;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks a method of a {@link Module @Module} class as the binding for its return type — the
 * picodroid counterpart of {@code dagger.Provides}.
 *
 * <p>This is how SDK types ({@code SharedPreferences}, {@code SensorManager}, ...), interfaces and
 * abstract types enter the graph, since none of them can carry an {@code @Inject} constructor:
 *
 * <pre>{@code
 * @Module
 * public final class AppModule {
 *   @Provides @Singleton
 *   static SharedPreferences providePrefs() { return SharedPreferences.open("app"); }
 *
 *   @Provides
 *   static Greeter provideGreeter(Clock clock) { return new FriendlyGreeter(clock); }
 * }
 * }</pre>
 *
 * <p>Parameters are injected like constructor parameters ({@code T}, {@code Provider<T>}, {@code
 * Lazy<T>}). A type may have exactly one binding in the app — one {@code @Provides} method, or one
 * {@code @Inject} constructor, never both. {@code @Singleton} on the method scopes the provided
 * value; otherwise the method is called on every injection. Returning {@code null} is not checked.
 *
 * <p>Compile-time only ({@link RetentionPolicy#SOURCE}); see {@code
 * docs/designs/inject-annotations-2026-08.md}.
 */
@Documented
@Retention(RetentionPolicy.SOURCE)
@Target(ElementType.METHOD)
public @interface Provides {}
