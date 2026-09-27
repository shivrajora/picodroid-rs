// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import javax.inject.Inject
import org.gradle.api.provider.MapProperty
import org.gradle.api.provider.Provider
import org.gradle.api.provider.ProviderFactory

/**
 * `picodroidBuildConfig { … }` — build-time string constants for an app, the
 * shape of Android's `buildConfigField`: the plugin generates
 * `<package>.BuildConfig` with one `public static final String` per field
 * ([GenerateBuildConfigTask]). Nothing is generated when no field is declared,
 * so ordinary apps carry no dead class.
 *
 * ```
 * picodroidBuildConfig {
 *     field("MODEL", "claude-opus-5")
 *     fieldFromProperty("ANTHROPIC_API_KEY", "picodroidAnthropicApiKey", "PICODROID_ANTHROPIC_API_KEY", "")
 * }
 * ```
 *
 * A secret passed this way is baked into the papk — keep it to keys that a
 * spend cap or a scope bounds, and never commit one.
 */
abstract class BuildConfigExtension @Inject constructor(private val providers: ProviderFactory) {
    abstract val fields: MapProperty<String, String>

    /** A constant with a literal value. */
    fun field(name: String, value: String) {
        fields.put(name, value)
    }

    /** A constant from a provider (a Gradle property, an environment variable, …). */
    fun field(name: String, value: Provider<String>) {
        fields.put(name, value)
    }

    /**
     * A constant from the Gradle property `property` (`-P<property>=…`), else the
     * `picodroid.env.<env>` property scripts/build-apk.sh sets from the variable
     * `env` per invocation, else the environment variable itself, else `default`.
     * The properties come first because a warm Gradle daemon keeps its original
     * environment (the PICODROID_SHRINK lesson, 81b6b7c): a value exported for one
     * build would otherwise be missed or go stale.
     */
    fun fieldFromProperty(name: String, property: String, env: String, default: String) {
        fields.put(
            name,
            providers.gradleProperty(property)
                .orElse(providers.gradleProperty("picodroid.env.$env"))
                .orElse(providers.environmentVariable(env))
                .orElse(default),
        )
    }
}
