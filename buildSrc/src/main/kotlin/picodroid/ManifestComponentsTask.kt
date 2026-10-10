// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.provider.ListProperty
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import org.objectweb.asm.ClassReader
import org.objectweb.asm.ClassVisitor
import org.objectweb.asm.Opcodes
import picodroid.classfile.extract

/**
 * `verifyManifest` — the declared surface of an Android-shaped manifest
 * (docs/designs/manifest-components-2026-10.md) against the app's bytecode. Runs only
 * when the manifest declares `<activity>` or `<service>` children; the short form
 * (`activity=` / `application=` alone) declares nothing and is not checked, so no
 * existing app changes behaviour.
 *
 * What is checked: every `new Intent(X.class)` / `new Intent(context, X.class)` whose
 * `X` extends `picodroid.app.Service` must be a declared `<service>`, and, when the
 * manifest declares activities, every such `X` extending `picodroid.app.Activity` must
 * be a declared `<activity>` — what Android enforces at run time
 * (`ActivityNotFoundException`, a silently ignored `startService`), caught here at pack
 * time instead, since the packer sees every class literal. `Intent.setClassName(...)`
 * is dynamic and is not checked. Superclasses are walked through the app's own classes;
 * a chain that leaves them (a framework base other than Activity / Service) is not
 * classified.
 */
abstract class ManifestComponentsTask : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val classesDir: DirectoryProperty

    /** Declared `<activity android:name>` classes, slash form. */
    @get:Input
    abstract val manifestActivities: ListProperty<String>

    /** Declared `<service android:name>` classes, slash form. */
    @get:Input
    abstract val manifestServices: ListProperty<String>

    @TaskAction
    fun run() {
        val declaredActivities = manifestActivities.get().toSet()
        val declaredServices = manifestServices.get().toSet()
        if (declaredActivities.isEmpty() && declaredServices.isEmpty()) {
            return
        }
        val root = classesDir.get().asFile
        val entries = root.walkTopDown()
            .filter { it.isFile && it.name.endsWith(".class") }
            .map { it.relativeTo(root).invariantSeparatorsPath.removeSuffix(".class") to it.readBytes() }
            .sortedBy { it.first }
            .toList()
        if (entries.isEmpty()) {
            throw GradleException("verifyManifest: no .class files under $root")
        }

        // Superclass of every app class, for the walk below.
        val superOf = HashMap<String, String>()
        for ((name, bytes) in entries) {
            ClassReader(bytes).accept(object : ClassVisitor(Opcodes.ASM9) {
                override fun visit(version: Int, access: Int, cname: String, signature: String?, superName: String?, interfaces: Array<String>?) {
                    if (superName != null) superOf[cname] = superName
                }
            }, ClassReader.SKIP_CODE or ClassReader.SKIP_FRAMES)
            if (name !in superOf) superOf[name] = "java/lang/Object"
        }
        fun extendsBase(cls: String, base: String): Boolean {
            var c: String? = cls
            var hops = 0
            while (c != null && hops < 32) {
                if (c == base) return true
                c = superOf[c]
                hops++
            }
            return false
        }

        // Intent targets: the class literal that precedes an Intent constructor
        // taking a Class, within one method.
        val targets = LinkedHashMap<String, String>() // class → "from"
        for ((_, bytes) in entries) {
            var lastLdc: Pair<String, String>? = null // (member, class)
            for (ref in extract(bytes)) {
                when (ref.kind) {
                    "ldc_class" -> lastLdc = ref.fromMember to ref.owner
                    "invokespecial" -> {
                        val intentCtor = ref.owner == "picodroid/content/Intent" && ref.name == "<init>" &&
                            (ref.desc == "(Ljava/lang/Class;)V" || ref.desc == "(Lpicodroid/content/Context;Ljava/lang/Class;)V")
                        if (intentCtor) {
                            val ldc = lastLdc
                            if (ldc != null && ldc.first == ref.fromMember) {
                                targets.putIfAbsent(ldc.second, "${ref.fromClass}.${ref.fromMember}")
                            }
                        }
                    }
                }
            }
        }

        val missing = ArrayList<String>()
        for ((cls, from) in targets) {
            if (extendsBase(cls, "picodroid/app/Service") && cls !in declaredServices) {
                missing += "<service android:name=\"${cls.replace('/', '.')}\"/>  (started from $from)"
            } else if (declaredActivities.isNotEmpty() && extendsBase(cls, "picodroid/app/Activity") && cls !in declaredActivities) {
                missing += "<activity android:name=\"${cls.replace('/', '.')}\"/>  (started from $from)"
            }
        }
        if (missing.isNotEmpty()) {
            throw GradleException(
                "PicodroidManifest.xml declares components, and the code starts some it does not declare " +
                    "(Android refuses these at run time; declare them under <application>):\n  " +
                    missing.joinToString("\n  ")
            )
        }
    }
}
