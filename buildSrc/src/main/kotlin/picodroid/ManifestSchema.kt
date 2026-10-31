// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.GradleException
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.w3c.dom.Element

/**
 * Parsed PicodroidManifest.xml, which is written as Android writes AndroidManifest.xml
 * (docs/designs/manifest-components-2026-10.md):
 *
 * ```xml
 * <manifest xmlns:android="http://schemas.android.com/apk/res/android"
 *           package="clock" android:versionName="1.2" android:versionCode="3">
 *   <uses-permission android:name="android.permission.INTERNET"/>
 *   <application android:name=".ClockApp" android:label="@string/app_name"
 *                android:theme="@style/AppTheme">
 *     <activity android:name=".ui.MainActivity">
 *       <intent-filter>
 *         <action android:name="android.intent.action.MAIN"/>
 *         <category android:name="android.intent.category.LAUNCHER"/>
 *       </intent-filter>
 *     </activity>
 *     <service android:name=".data.UsageService"/>
 *   </application>
 * </manifest>
 * ```
 *
 * `<application android:name>` is the Application class; the entry Activity is the one
 * with the `MAIN` intent filter, else the first declared. An app may have either or both:
 * with both, the framework runs `Application.onCreate` and then launches the entry
 * Activity, as Android does; with an Application only, the app runs headless. Names may
 * be dotted (`pkg.Main`), slash-form (`pkg/Main`) or relative to the manifest package
 * (`.Main`), as Android reads them; [application], [activity], [activities] and
 * [services] hold the slash form. Declared classes go to the packer, which checks that
 * each exists and extends the right base, and to [ManifestComponentsTask], which checks
 * that every `Intent(X.class)` the code starts is declared. `<uses-permission>` is read
 * and ignored: nothing here enforces a permission, and the reference page says so.
 *
 * Android's attributes carry the `android:` prefix and only that spelling is read; the
 * parser is not namespace-aware, so the prefix must be `android`. Picodroid's own
 * attributes (`icon`, `<supports-screens design-width design-height>`) have no Android
 * counterpart and are bare. The pre-2026-10 short form (`<application application=…>`,
 * `activity=`, `main-class=`, `version=`, `version-code=`, bare `name` / `label` /
 * `theme`) is refused with the Android spelling to write instead.
 */
data class PicodroidManifest(
    val packageName: String,
    /** `<manifest android:versionName>`, "1.0" when absent. */
    val version: String,
    /** `<manifest android:versionCode>`: a positive integer, 1 when absent. */
    val versionCode: Int,
    /**
     * `<application android:label>`: the display name, as written (`@string/name` is
     * resolved by the packer against `res/values`); null means "the package".
     */
    val label: String?,
    /** `<application icon>`: a file name under `assets/`; null means none. */
    val icon: String?,
    /** `<application android:name>`: the Application class, slash form; null means none. */
    val application: String?,
    /** The entry Activity, slash form: the `MAIN` filter's, else the first; null means none. */
    val activity: String?,
    /**
     * `<supports-screens design-width design-height>`: the logical size the app lays out
     * against, which the runtime shows at exactly that size on every panel. Null: the app
     * resizes to the panel.
     */
    val designWidth: Int? = null,
    val designHeight: Int? = null,
    /**
     * The `<uses-feature android:name android:required="true">` names, in document order.
     * A feature not marked required is informational and dropped here: nothing acts on it.
     */
    val requiredFeatures: List<String> = emptyList(),
    /** The `<activity android:name>` classes, slash form, document order. */
    val activities: List<String> = emptyList(),
    /**
     * The `<activity android:label>`s, `class=label` with the class in slash form: what the
     * Activity's title (`getTitle`, an action bar's title) starts as. An activity without one
     * takes the application's label.
     */
    val activityLabels: List<String> = emptyList(),
    /** The `<service android:name>` classes, slash form, document order. */
    val services: List<String> = emptyList(),
    /**
     * The `<receiver android:name>` classes, slash form, each followed by the actions its
     * `<intent-filter>` names (`pkg/Boot|android.intent.action.BOOT_COMPLETED`), document
     * order. Only the boot-time actions may be declared: the runtime delivers them to a
     * package that is not running by a headless run of it (broadcasts-2026-10.md stage 6).
     */
    val receivers: List<String> = emptyList(),
    /**
     * `<application android:theme="@style/Name">`: the `<style>` in `res/values` that is the
     * app's theme — what `?attr/…` reads at build time and what the framework applies before
     * the first Activity's `onCreate`. Null: the style called `AppTheme`, when there is one.
     */
    val theme: String? = null,
) {
    companion object {
        private const val ANDROID_NS = "http://schemas.android.com/apk/res/android"
        private const val ACTION_MAIN = "android.intent.action.MAIN"

        /** The actions a `<receiver>` may declare; the rest are registered-only here. */
        private val MANIFEST_RECEIVER_ACTIONS = listOf(
            "android.intent.action.BOOT_COMPLETED",
            "android.intent.action.TIME_SET",
            "android.intent.action.TIMEZONE_CHANGED",
        )

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
            val where = file.name
            fun fail(msg: String): Nothing = throw GradleException("$where: $msg")

            if (root.tagName != "manifest") {
                fail("expected <manifest> root, got <${root.tagName}>")
            }
            if (root.getAttribute("xmlns:android") != ANDROID_NS) {
                fail("<manifest> must declare xmlns:android=\"$ANDROID_NS\", as an Android manifest does")
            }
            val pkg = root.getAttribute("package").ifBlank { fail("<manifest> missing 'package' attribute") }

            // Android's attributes are read only with the `android:` prefix. A bare
            // spelling the parser used to accept is refused with the one to write.
            fun Element.androidAttr(name: String): String? {
                if (hasAttribute(name)) {
                    fail("<$tagName $name=\"${getAttribute(name)}\">: write android:$name, Android's spelling")
                }
                return getAttribute("android:$name").ifBlank { null }
            }

            for ((old, new) in listOf("version" to "android:versionName", "version-code" to "android:versionCode")) {
                if (root.hasAttribute(old)) {
                    fail("<manifest $old=\"${root.getAttribute(old)}\">: write $new, Android's spelling")
                }
            }
            val version = root.getAttribute("android:versionName").ifBlank { "1.0" }
            val versionCodeText = root.getAttribute("android:versionCode").ifBlank { "1" }
            val versionCode = versionCodeText.toIntOrNull()?.takeIf { it >= 1 }
                ?: fail("<manifest> android:versionCode must be a positive integer, got '$versionCodeText'")

            // A class as Android spells it, to the slash form the packer and the
            // runtime use: `.Main` is relative to the manifest package.
            fun className(raw: String): String = (if (raw.startsWith(".")) pkg + raw else raw).replace('.', '/')
            fun componentClass(el: Element): String =
                className(el.androidAttr("name") ?: fail("<${el.tagName}> missing 'android:name' attribute"))

            var app: Element? = null
            var designWidth: Int? = null
            var designHeight: Int? = null
            val requiredFeatures = ArrayList<String>()
            val top = root.childNodes
            for (i in 0 until top.length) {
                val el = top.item(i) as? Element ?: continue
                when (el.tagName) {
                    "application" -> {
                        if (app != null) fail("more than one <application> element")
                        app = el
                    }
                    // Declared, never enforced (reference/manifest.md).
                    "uses-permission" -> el.androidAttr("name") ?: fail("<uses-permission> missing 'android:name' attribute")
                    "uses-feature" -> {
                        val name = el.androidAttr("name") ?: fail("<uses-feature> missing 'android:name' attribute")
                        if (name.contains(',')) fail("<uses-feature> android:name must not contain ',': '$name'")
                        // Picodroid's default is "not required" (docs/designs/app-portability-2026-10.md
                        // D9): the stock widgets work on every input profile, so only an app that reads
                        // raw touch, say, has to insist.
                        when (val required = el.androidAttr("required") ?: "false") {
                            "true" -> requiredFeatures += name
                            "false" -> {}
                            else -> fail("<uses-feature> android:required must be 'true' or 'false', got '$required'")
                        }
                    }
                    "supports-screens" -> {
                        fun side(attr: String): Int {
                            val text = el.getAttribute(attr)
                            return text.toIntOrNull()?.takeIf { it >= 1 }
                                ?: fail("<supports-screens> $attr must be a positive integer, got '$text'")
                        }
                        designWidth = side("design-width")
                        designHeight = side("design-height")
                    }
                    else -> fail(
                        "<manifest> does not take <${el.tagName}> (supported: <application>, <uses-permission>, <uses-feature>, <supports-screens>)"
                    )
                }
            }
            val application = app ?: fail("missing <application> element")

            // The short form's entry attributes, refused with Android's spelling.
            application.getAttribute("application").ifBlank { null }?.let {
                fail(
                    "<application application=\"$it\"> is the old short form: write <application android:name=\"${it.replace('/', '.')}\"> (or \".${it.substringAfterLast('/')}\" for a class in the manifest package)"
                )
            }
            application.getAttribute("activity").ifBlank { null }?.let {
                fail(
                    "<application activity=\"$it\"> is the old short form: declare the Activity under <application> as <activity android:name=\"${it.replace('/', '.')}\"> with the MAIN / LAUNCHER <intent-filter>"
                )
            }
            application.getAttribute("main-class").ifBlank { null }?.let {
                fail("<application main-class=\"$it\"> is not supported: an app's entry is an Application (android:name) or an <activity>")
            }

            val applicationClass = application.androidAttr("name")?.let { className(it) }
            val label = application.androidAttr("label")
            val icon = application.getAttribute("icon").ifBlank { null }

            val activities = ArrayList<String>()
            val activityLabels = ArrayList<String>()
            val services = ArrayList<String>()
            val receivers = ArrayList<String>()
            var launcher: String? = null
            val children = application.childNodes
            for (i in 0 until children.length) {
                val el = children.item(i) as? Element ?: continue
                when (el.tagName) {
                    "activity" -> {
                        val cls = componentClass(el)
                        if (el.androidAttr("theme") != null) {
                            fail("<activity> android:theme is not supported — there is one theme per app; put android:theme on <application>")
                        }
                        if (cls in activities) fail("<activity> '$cls' is declared twice")
                        activities += cls
                        el.androidAttr("label")?.let { activityLabels += "$cls=$it" }
                        val filters = el.getElementsByTagName("intent-filter")
                        for (f in 0 until filters.length) {
                            val filter = filters.item(f) as Element
                            val categories = filter.getElementsByTagName("category")
                            for (c in 0 until categories.length) {
                                (categories.item(c) as Element).androidAttr("name")
                                    ?: fail("<activity> <category> missing 'android:name'")
                            }
                            val actions = filter.getElementsByTagName("action")
                            for (a in 0 until actions.length) {
                                val action = (actions.item(a) as Element).androidAttr("name")
                                    ?: fail("<activity> <action> missing 'android:name'")
                                if (action == ACTION_MAIN) {
                                    if (launcher != null && launcher != cls) {
                                        fail("two activities carry the MAIN intent filter ('$launcher', '$cls'); one app has one entry")
                                    }
                                    launcher = cls
                                }
                            }
                        }
                    }
                    "service" -> {
                        val cls = componentClass(el)
                        if (cls in services) fail("<service> '$cls' is declared twice")
                        services += cls
                    }
                    "receiver" -> {
                        val cls = componentClass(el)
                        if (receivers.any { it.substringBefore('|') == cls }) fail("<receiver> '$cls' is declared twice")
                        val actions = ArrayList<String>()
                        val filters = el.getElementsByTagName("intent-filter")
                        for (f in 0 until filters.length) {
                            val names = (filters.item(f) as Element).getElementsByTagName("action")
                            for (a in 0 until names.length) {
                                val action = (names.item(a) as Element).androidAttr("name")
                                    ?: fail("<receiver> <action> missing 'android:name'")
                                if (action !in MANIFEST_RECEIVER_ACTIONS) {
                                    fail(
                                        "<receiver> '$cls' declares '$action', which only a receiver registered at run time (Context.registerReceiver) hears here; a manifest receiver may declare ${MANIFEST_RECEIVER_ACTIONS.joinToString(", ")}"
                                    )
                                }
                                if (action !in actions) actions += action
                            }
                        }
                        receivers += (listOf(cls) + actions).joinToString("|")
                    }
                    "uses-permission" -> fail(
                        "<uses-permission> belongs under <manifest>, not <application>, as on Android"
                    )
                    else -> fail(
                        "<application> does not take <${el.tagName}> (supported: <activity>, <service>, <receiver>)"
                    )
                }
            }
            val theme = application.androidAttr("theme")?.let { t ->
                t.removePrefix("@style/").ifBlank { fail("<application> android:theme must name a <style>, got '$t'") }
            }
            val entryActivity = launcher ?: activities.firstOrNull()
            if (applicationClass == null && entryActivity == null) {
                fail(
                    "the app has no entry: name the Application class (<application android:name=\".App\">), declare an <activity>, or both"
                )
            }

            return PicodroidManifest(
                packageName = pkg,
                version = version,
                versionCode = versionCode,
                label = label,
                icon = icon,
                application = applicationClass,
                activity = entryActivity,
                designWidth = designWidth,
                designHeight = designHeight,
                requiredFeatures = requiredFeatures,
                activities = activities,
                activityLabels = activityLabels,
                services = services,
                receivers = receivers,
                theme = theme,
            )
        }
    }
}
