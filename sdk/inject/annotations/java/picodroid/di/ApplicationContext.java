// SPDX-License-Identifier: GPL-3.0-only
package picodroid.di;

import java.lang.annotation.Documented;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
import javax.inject.Qualifier;

/**
 * Qualifies an injected {@code picodroid.content.Context} as the application's — the picodroid
 * counterpart of Hilt's {@code dagger.hilt.android.qualifiers.ApplicationContext}. The framework
 * binds it; an app declares nothing:
 *
 * <pre>{@code
 * @Module
 * public final class AppModule {
 *   @Provides @Singleton
 *   static SharedPreferences providePrefs(@ApplicationContext Context context) {
 *     return context.getSharedPreferences("app", Context.MODE_PRIVATE);
 *   }
 * }
 *
 * public class Repo {
 *   @Inject Repo(@ApplicationContext Context context) { ... }
 * }
 * }</pre>
 *
 * <p>Usable on {@code @Inject} constructor and method parameters, {@code @Inject} fields and
 * {@code @Provides} parameters. An unqualified {@code Context} is not bound (a compile error, as in
 * Hilt); {@code picodroid.app.Application} is, unqualified. Both resolve to the app's {@code
 * Application}, which lives as long as the graph's singletons do. An app whose manifest names no
 * Application class gets a bare {@code picodroid.app.Application}, made on first use: Android
 * always has one.
 *
 * <p>Compile-time only ({@link RetentionPolicy#SOURCE}); see {@code
 * docs/designs/inject-annotations-2026-08.md}.
 */
@Documented
@Retention(RetentionPolicy.SOURCE)
@Target({ElementType.PARAMETER, ElementType.FIELD, ElementType.METHOD})
@Qualifier
public @interface ApplicationContext {}
