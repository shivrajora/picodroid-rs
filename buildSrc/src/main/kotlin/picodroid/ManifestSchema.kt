// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.GradleException
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.w3c.dom.Element

/**
 * Parsed PicodroidManifest.xml. Exactly one of [mainClass], [activity], or
 * [application] is non-null — matches the shell script's runtime check in
 * scripts/build-apk.sh, but fails here with a clear error instead of at
 * papk-pack time.
 *
 * Two spellings of the entry point are read (docs/designs/manifest-components-2026-10.md).
 * The short form names it as an attribute of `<application>`
 * (`activity="pkg/Main"`), declares nothing else, and is what every example used until
 * 2026-10. The Android form declares the app's components as children:
 *
 * ```xml
 * <application android:theme="@style/AppTheme" label="Clock">
 *   <activity android:name=".ui.MainActivity">
 *     <intent-filter>
 *       <action android:name="android.intent.action.MAIN"/>
 *       <category android:name="android.intent.category.LAUNCHER"/>
 *     </intent-filter>
 *   </activity>
 *   <service android:name=".data.UsageService"/>
 * </application>
 * ```
 *
 * The entry Activity is the one with the `MAIN` intent filter, else the first declared.
 * Declared classes go to the packer, which checks that each exists and extends the right
 * base, and to [ManifestComponentsTask], which checks that every `Intent(X.class)` the
 * code starts is declared. Names may be dotted (`pkg.Main`), slash-form (`pkg/Main`) or
 * relative to the manifest package (`.Main`), as Android reads them; [activities] and
 * [services] hold the slash form. `<uses-permission>` is read and ignored: nothing here
 * enforces a permission, and the reference page says so.
 */
data class PicodroidManifest(
    val packageName: String,
    val version: String,
    /** `<manifest version-code>`: a positive integer, 1 when absent. */
    val versionCode: Int,
    /** `<application label>`: the display name; null means "the package". */
    val label: String?,
    /** `<application icon>`: a file name under `assets/`; null means none. */
    val icon: String?,
    val mainClass: String?,
    val activity: String?,
    val application: String?,
    /**
     * `<supports-screens design-width design-height>`: the logical size the app lays out
     * against, which the runtime shows at exactly that size on every panel. Null: the app
     * resizes to the panel.
     */
    val designWidth: Int? = null,
    val designHeight: Int? = null,
    /**
     * The `<uses-feature name required="true">` names, in document order. A feature not
     * marked required is informational and dropped here: nothing acts on it.
     */
    val requiredFeatures: List<String> = emptyList(),
    /** The `<activity android:name>` classes, slash form, document order. */
    val activities: List<String> = emptyList(),
    /** The `<service android:name>` classes, slash form, document order. */
    val services: List<String> = emptyList(),
    /**
     * `<application android:theme="@style/Name">`: the `<style>` in `res/values` that is the
     * app's theme — what `?attr/…` reads at build time and what the framework applies before
     * the first Activity's `onCreate`. Null: the style called `AppTheme`, when there is one.
     */
    val theme: String? = null,
) {
    companion object {
        private const val ACTION_MAIN = "android.intent.action.MAIN"

        fun parse(file: File): PicodroidManifest {
            if (!file.isFile) {
                throw GradleException("PicodroidManifest.xml not found: ${file.absolutePath}")
            }
            val doc = DocumentBuilderFactory.newInstance().apply {
                isNamespaceAware = false
                isValidating = false
                setFeature("http://apache.org/xml/features/disallow-doctype-decl", true)
            }.newDocumentBuilder().parse(file)
            val root = doc.documentElement
            if (root.tagName != "manifest") {
                throw GradleException("${file.name}: expected <manifest> root, got <${root.tagName}>")
            }
            val pkg = root.getAttribute("package").ifBlank {
                throw GradleException("${file.name}: <manifest> missing 'package' attribute")
            }
            val version = root.getAttribute("version").ifBlank { "1.0" }
            val versionCodeText = root.getAttribute("version-code").ifBlank { "1" }
            val versionCode = versionCodeText.toIntOrNull()?.takeIf { it >= 1 }
                ?: throw GradleException(
                    "${file.name}: <manifest> version-code must be a positive integer, got '$versionCodeText'"
                )

            val appNodes = root.getElementsByTagName("application")
            if (appNodes.length == 0) {
                throw GradleException("${file.name}: missing <application> element")
            }
            val app = appNodes.item(0) as Element
            val mainClass = app.getAttribute("main-class").ifBlank { null }
            var activity = app.getAttribute("activity").ifBlank { null }
            val application = app.getAttribute("application").ifBlank { null }
            val label = app.getAttribute("label").ifBlank { null }
            val icon = app.getAttribute("icon").ifBlank { null }

            // Android's attributes carry the `android:` prefix; the bare spelling is
            // accepted too, like the attributes above. Namespaces are off, so the
            // prefix is part of the attribute's name.
            fun Element.androidAttr(name: String): String? =
                getAttribute("android:$name").ifBlank { null } ?: getAttribute(name).ifBlank { null }

            // A component class as Android spells it, to the slash form the packer and
            // the runtime use: `.Main` is relative to the manifest package.
            fun componentClass(el: Element, tag: String): String {
                val raw = el.androidAttr("name")
                    ?: throw GradleException("${file.name}: <$tag> missing 'android:name' attribute")
                val absolute = if (raw.startsWith(".")) pkg + raw else raw
                return absolute.replace('.', '/')
            }

            val activities = ArrayList<String>()
            val services = ArrayList<String>()
            var launcher: String? = null
            val children = app.childNodes
            for (i in 0 until children.length) {
                val el = children.item(i) as? Element ?: continue
                when (el.tagName) {
                    "activity" -> {
                        val cls = componentClass(el, "activity")
                        if (el.androidAttr("theme") != null) {
                            throw GradleException(
                                "${file.name}: <activity> android:theme is not supported — there is one theme per app; put android:theme on <application>"
                            )
                        }
                        if (cls in activities) {
                            throw GradleException("${file.name}: <activity> '${cls}' is declared twice")
                        }
                        activities += cls
                        val filters = el.getElementsByTagName("intent-filter")
                        for (f in 0 until filters.length) {
                            val actions = (filters.item(f) as Element).getElementsByTagName("action")
                            for (a in 0 until actions.length) {
                                if ((actions.item(a) as Element).androidAttr("name") == ACTION_MAIN) {
                                    if (launcher != null && launcher != cls) {
                                        throw GradleException(
                                            "${file.name}: two activities carry the MAIN intent filter ('$launcher', '$cls'); one app has one entry"
                                        )
                                    }
                                    launcher = cls
                                }
                            }
                        }
                    }
                    "service" -> {
                        val cls = componentClass(el, "service")
                        if (cls in services) {
                            throw GradleException("${file.name}: <service> '${cls}' is declared twice")
                        }
                        services += cls
                    }
                    "uses-permission" -> {} // declared, never enforced (reference/manifest.md)
                    else -> throw GradleException(
                        "${file.name}: <application> does not take <${el.tagName}> (supported: <activity>, <service>, <uses-permission>)"
                    )
                }
            }
            val theme = app.androidAttr("theme")?.let { t ->
                t.removePrefix("@style/").ifBlank {
                    throw GradleException("${file.name}: <application> android:theme must name a <style>, got '$t'")
                }
            }

            if (activity != null && activities.isNotEmpty() && activity !in activities) {
                throw GradleException(
                    "${file.name}: <application activity=\"$activity\"> is not among the declared <activity> elements — declare it, or drop the attribute (the MAIN intent filter, else the first <activity>, is the entry)"
                )
            }
            if (mainClass == null && activity == null && application == null && activities.isNotEmpty()) {
                activity = launcher ?: activities.first()
            }

            val screens = root.getElementsByTagName("supports-screens")
            var designWidth: Int? = null
            var designHeight: Int? = null
            if (screens.length > 0) {
                val el = screens.item(0) as Element
                fun side(attr: String): Int {
                    val text = el.getAttribute(attr)
                    return text.toIntOrNull()?.takeIf { it >= 1 }
                        ?: throw GradleException(
                            "${file.name}: <supports-screens> $attr must be a positive integer, got '$text'"
                        )
                }
                designWidth = side("design-width")
                designHeight = side("design-height")
            }

            val features = root.getElementsByTagName("uses-feature")
            val requiredFeatures = (0 until features.length).mapNotNull { i ->
                val el = features.item(i) as Element
                val name = el.getAttribute("name").ifBlank {
                    throw GradleException("${file.name}: <uses-feature> missing 'name' attribute")
                }
                if (name.contains(',')) {
                    throw GradleException("${file.name}: <uses-feature> name must not contain ',': '$name'")
                }
                // Picodroid's default is "not required" (docs/designs/app-portability-2026-10.md
                // D9): the stock widgets work on every input profile, so only an app that reads
                // raw touch, say, has to insist.
                when (val required = el.getAttribute("required").ifBlank { "false" }) {
                    "true" -> name
                    "false" -> null
                    else -> throw GradleException(
                        "${file.name}: <uses-feature> required must be 'true' or 'false', got '$required'"
                    )
                }
            }

            val set = listOfNotNull(mainClass, activity, application)
            if (set.isEmpty()) {
                throw GradleException(
                    "${file.name}: <application> must set exactly one of 'main-class', 'activity', or 'application', or declare an <activity>"
                )
            }
            if (set.size > 1) {
                throw GradleException(
                    "${file.name}: <application> sets multiple of 'main-class'/'activity'/'application' — pick one"
                )
            }

            return PicodroidManifest(
                packageName = pkg,
                version = version,
                versionCode = versionCode,
                label = label,
                icon = icon,
                mainClass = mainClass,
                activity = activity,
                application = application,
                designWidth = designWidth,
                designHeight = designHeight,
                requiredFeatures = requiredFeatures,
                activities = activities,
                services = services,
                theme = theme,
            )
        }
    }
}
