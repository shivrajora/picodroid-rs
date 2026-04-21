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
    val mainClass: String?,
    val activity: String?,
    val application: String?,
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

            val appNodes = root.getElementsByTagName("application")
            if (appNodes.length == 0) {
                throw GradleException("${file.name}: missing <application> element")
            }
            val app = appNodes.item(0) as org.w3c.dom.Element
            val mainClass = app.getAttribute("main-class").ifBlank { null }
            val activity = app.getAttribute("activity").ifBlank { null }
            val application = app.getAttribute("application").ifBlank { null }

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
                mainClass = mainClass,
                activity = activity,
                application = application,
            )
        }
    }
}
