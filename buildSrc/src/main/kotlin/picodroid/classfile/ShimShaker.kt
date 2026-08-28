// SPDX-License-Identifier: GPL-3.0-only
package picodroid.classfile

import org.objectweb.asm.AnnotationVisitor
import org.objectweb.asm.ClassReader
import org.objectweb.asm.ClassVisitor
import org.objectweb.asm.ClassWriter
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes

/** A class member: owner internal name, member name, descriptor. */
data class MemberKey(val owner: String, val name: String, val desc: String)

/** One class going through the pipeline: its path relative to the classes root, and its bytes. */
data class ClassEntry(val relPath: String, val bytes: ByteArray) {
    val className: String get() = relPath.removeSuffix(".class")
}

data class ShakeReport(
    val appClasses: Int,
    val shimClassesIn: Int,
    val shimClassesKept: List<String>,
    val shimClassesPruned: List<String>,
    val methodsShaken: Map<String, List<String>>,
    val renames: Map<MemberKey, String>,
    val bytesIn: Int,
    val bytesOut: Int,
    val cpIn: Int,
    val cpOut: Int,
    val iterations: Int,
) {
    fun render(): String = buildString {
        appendLine("strip: $appClasses app classes, $shimClassesIn shim classes in, ${shimClassesKept.size} kept, ${shimClassesPruned.size} pruned; $bytesIn -> $bytesOut bytes, $cpIn -> $cpOut CP entries, $iterations shake iteration(s)")
        if (renames.isNotEmpty()) appendLine("  @ShimName: " + renames.entries.joinToString { "${it.key.owner}.${it.key.name}${it.key.desc} -> ${it.value}" })
        if (shimClassesPruned.isNotEmpty()) appendLine("  pruned: " + shimClassesPruned.joinToString())
        methodsShaken.forEach { (c, ms) -> appendLine("  shaken $c: " + ms.joinToString()) }
        if (shimClassesKept.isNotEmpty()) appendLine("  kept: " + shimClassesKept.joinToString())
    }
}

/**
 * The Kotlin-app class pipeline (roadmap § Cross-cutting decisions → Strip +
 * shake): strip every class ([strip]), rename `@ShimName` methods, drop the
 * `picodroid/shim/…` annotation classes, then for `kotlin/…` classes only
 * **prune** what no app class reaches (closure over `CONSTANT_Class` refs)
 * and **shake** unreferenced `static` methods out of `*Kt` facades, to a
 * fixpoint. Instance methods and non-`Kt` classes are never touched (they are
 * subclassed / dispatched by runtime class); `@ShimKeep` exempts a method or
 * class. Pure: no file I/O, no Gradle.
 */
object ShimShaker {
    const val SHIM_NAME_DESC = "Lpicodroid/shim/ShimName;"
    const val SHIM_KEEP_DESC = "Lpicodroid/shim/ShimKeep;"

    private val MEMBER_REF_KINDS = setOf(
        "invokestatic", "invokevirtual", "invokespecial", "invokeinterface",
        "getstatic", "putstatic", "getfield", "putfield",
        "indy_impl", "indy_arg_handle", "ldc_handle",
    )

    fun isShimClass(name: String): Boolean = name.startsWith("kotlin/") || name.startsWith("kotlinx/")

    fun isAnnotationClass(name: String): Boolean = name.startsWith("picodroid/shim/")

    fun isFacade(name: String): Boolean = name.endsWith("Kt")

    class Marks(val renames: MutableMap<MemberKey, String> = HashMap(), val keepMethods: MutableSet<MemberKey> = HashSet(), val keepClasses: MutableSet<String> = HashSet())

    /** Pre-pass over the unstripped shim bytes: `@ShimName` / `@ShimKeep` are CLASS-retention, so they must be read before the strip drops them. Shared with [ShimContract]. */
    fun collectMarks(entries: List<ClassEntry>): Marks {
        val marks = Marks()
        entries.forEach { e ->
            var owner = e.className
            ClassReader(e.bytes).accept(object : ClassVisitor(Opcodes.ASM9) {
                override fun visit(version: Int, access: Int, name: String, signature: String?, superName: String?, interfaces: Array<String>?) {
                    owner = name
                }

                override fun visitAnnotation(descriptor: String, visible: Boolean): AnnotationVisitor? {
                    if (descriptor == SHIM_KEEP_DESC) marks.keepClasses += owner
                    return null
                }

                override fun visitMethod(access: Int, name: String, descriptor: String, signature: String?, exceptions: Array<String>?): MethodVisitor {
                    val key = MemberKey(owner, name, descriptor)
                    return object : MethodVisitor(Opcodes.ASM9) {
                        override fun visitAnnotation(descriptor: String, visible: Boolean): AnnotationVisitor? {
                            when (descriptor) {
                                SHIM_KEEP_DESC -> marks.keepMethods += key
                                SHIM_NAME_DESC -> return object : AnnotationVisitor(Opcodes.ASM9) {
                                    override fun visit(name: String?, value: Any?) {
                                        if (name == "value" && value is String) marks.renames[key] = value
                                    }
                                }
                            }
                            return null
                        }
                    }
                }
            }, ClassReader.SKIP_CODE or ClassReader.SKIP_FRAMES)
        }
        return marks
    }

    private fun dropMethods(bytes: ByteArray, drop: Set<Pair<String, String>>): ByteArray {
        val cw = ClassWriter(0)
        ClassReader(bytes).accept(object : ClassVisitor(Opcodes.ASM9, cw) {
            override fun visitMethod(access: Int, name: String, descriptor: String, signature: String?, exceptions: Array<String>?): MethodVisitor? =
                if ((name to descriptor) in drop) null else super.visitMethod(access, name, descriptor, signature, exceptions)
        }, ClassReader.SKIP_FRAMES)
        return cw.toByteArray()
    }

    private fun staticMethods(bytes: ByteArray): List<Pair<String, String>> {
        val out = ArrayList<Pair<String, String>>()
        ClassReader(bytes).accept(object : ClassVisitor(Opcodes.ASM9) {
            override fun visitMethod(access: Int, name: String, descriptor: String, signature: String?, exceptions: Array<String>?): MethodVisitor? {
                if (access and Opcodes.ACC_STATIC != 0 && name != "<clinit>") out += name to descriptor
                return null
            }
        }, ClassReader.SKIP_CODE or ClassReader.SKIP_FRAMES)
        return out
    }

    fun process(input: List<ClassEntry>): Pair<List<ClassEntry>, ShakeReport> {
        val annotationClasses = input.filter { isAnnotationClass(it.className) }
        val shimIn = input.filter { isShimClass(it.className) }
        val app = input.filter { !isShimClass(it.className) && !isAnnotationClass(it.className) }
        val marks = collectMarks(shimIn + annotationClasses)

        val bytesIn = (app + shimIn).sumOf { it.bytes.size }
        val cpIn = (app + shimIn).sumOf { ClassReader(it.bytes).itemCount }

        // 1. Strip (and rename) everything that ships.
        val strippedApp = app.map { ClassEntry(it.relPath, strip(it.bytes, marks.renames).first) }
        var shim = shimIn.associate { it.className to ClassEntry(it.relPath, strip(it.bytes, marks.renames).first) }.toMutableMap()
        val shaken = LinkedHashMap<String, MutableList<String>>()

        // 2. Prune + shake to a fixpoint.
        var iterations = 0
        var kept: Map<String, ClassEntry>
        while (true) {
            iterations++
            // Reachability over CONSTANT_Class entries, starting from the app classes.
            val reachable = LinkedHashSet<String>()
            val queue = ArrayDeque<ClassEntry>()
            queue += strippedApp
            marks.keepClasses.filter { it in shim }.forEach { reachable += it }
            marks.keepClasses.mapNotNull { shim[it] }.forEach { queue += it }
            while (queue.isNotEmpty()) {
                val e = queue.removeFirst()
                census(e.bytes).cpClasses.forEach { (_, name) ->
                    val plain = name.trimStart('[').let { if (it.startsWith("L") && it.endsWith(";")) it.substring(1, it.length - 1) else it }
                    val target = shim[plain]
                    if (target != null && reachable.add(plain)) queue += target
                }
            }
            kept = reachable.associateWith { shim.getValue(it) }

            // Member references from everything that ships.
            val refs = HashSet<MemberKey>()
            (strippedApp + kept.values).forEach { e ->
                extract(e.bytes).forEach { r -> if (r.kind in MEMBER_REF_KINDS) refs += MemberKey(r.owner, r.name, r.desc) }
            }

            var changed = false
            kept.values.filter { isFacade(it.className) }.forEach { e ->
                val drop = staticMethods(e.bytes)
                    .filter { (n, d) -> MemberKey(e.className, n, d) !in refs && MemberKey(e.className, n, d) !in marks.keepMethods }
                    .toSet()
                if (drop.isNotEmpty()) {
                    shim[e.className] = ClassEntry(e.relPath, dropMethods(e.bytes, drop))
                    shaken.getOrPut(e.className) { ArrayList() } += drop.map { (n, d) -> n + d }
                    changed = true
                }
            }
            if (!changed) break
        }

        val out = strippedApp + kept.keys.sorted().map { shim.getValue(it) }
        val report = ShakeReport(
            appClasses = app.size,
            shimClassesIn = shimIn.size,
            shimClassesKept = kept.keys.sorted(),
            shimClassesPruned = (shimIn.map { it.className }.toSet() - kept.keys).sorted(),
            methodsShaken = shaken,
            renames = marks.renames,
            bytesIn = bytesIn,
            bytesOut = out.sumOf { it.bytes.size },
            cpIn = cpIn,
            cpOut = out.sumOf { ClassReader(it.bytes).itemCount },
            iterations = iterations,
        )
        return out to report
    }
}
