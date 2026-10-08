// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.GradleException
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory

/**
 * Parsed PicodroidManifest.xml. Exactly one of [mainClass], [activity], or
 * [application] is non-null — matches the shell script's runtime check in
 * scripts/build-apk.sh, but fails here with a clear error instead of at
 * papk-pack time.
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
) {
    companion object {
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
            val app = appNodes.item(0) as org.w3c.dom.Element
            val mainClass = app.getAttribute("main-class").ifBlank { null }
            val activity = app.getAttribute("activity").ifBlank { null }
            val application = app.getAttribute("application").ifBlank { null }
            val label = app.getAttribute("label").ifBlank { null }
            val icon = app.getAttribute("icon").ifBlank { null }

            val screens = root.getElementsByTagName("supports-screens")
            var designWidth: Int? = null
            var designHeight: Int? = null
            if (screens.length > 0) {
                val el = screens.item(0) as org.w3c.dom.Element
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
                val el = features.item(i) as org.w3c.dom.Element
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
                    "${file.name}: <application> must set exactly one of 'main-class', 'activity', or 'application'"
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
            )
        }
    }
}
