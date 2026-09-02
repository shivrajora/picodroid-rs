// SPDX-License-Identifier: GPL-3.0-only
package picodroid.classfile

import java.io.File
import org.objectweb.asm.ClassReader
import org.objectweb.asm.ClassWriter
import org.objectweb.asm.commons.ClassRemapper
import org.objectweb.asm.commons.Remapper

/**
 * The `[[member]]` rows of a shrink map (`sdk/shrink-maps/v*.toml`): original
 * method/field name → shrunk name, owner-agnostic. Mirrors the Rust reader
 * in `tools/class-shrink/src/mapping.rs` for exactly the shape `save()`
 * emits, in pure Kotlin for the same reason as [picodroid.ShrinkMapResolver]:
 * a `cargo run` from Gradle configuration deadlocks on the target-dir lock.
 */
object ShrinkMapMembers {
    fun parse(mapFile: File): Map<String, String> {
        val out = LinkedHashMap<String, String>()
        var section: String? = null
        var from: String? = null
        var to: String? = null
        fun flush() {
            val f = from
            val t = to
            if (section == "member" && f != null && t != null) out[f] = t
            from = null
            to = null
        }
        for (raw in mapFile.readLines()) {
            val line = raw.trim()
            if (line.isEmpty() || line.startsWith("#")) continue
            if (line == "[[class]]" || line == "[[member]]") {
                flush()
                section = line.removeSurrounding("[[", "]]")
                continue
            }
            val eq = line.indexOf('=')
            if (eq < 0) continue
            val key = line.substring(0, eq).trim()
            val value = line.substring(eq + 1).trim().removeSurrounding("\"")
            if (section == "member") {
                when (key) {
                    "from" -> from = value
                    "to" -> to = value
                }
            }
        }
        flush()
        return out
    }
}

/**
 * Renames members through a name-keyed map, leaving class names alone (the
 * Rust `class-shrink shrink-dir` pass owns those, and runs after this one).
 * Owner-agnostic on purpose: the map is global by name so that an app's
 * `onCreate` override and the framework method it overrides — and every call
 * site of either — rename identically.
 */
class MemberRemapper(private val members: Map<String, String>) : Remapper() {
    override fun mapMethodName(owner: String, name: String, descriptor: String): String = members[name] ?: name

    override fun mapFieldName(owner: String, name: String, descriptor: String): String = members[name] ?: name

    override fun mapInvokeDynamicMethodName(name: String, descriptor: String): String = members[name] ?: name

    override fun mapRecordComponentName(owner: String, name: String, descriptor: String): String = members[name] ?: name

    // Annotation element names are looked up by name from annotation payloads
    // (`cut-release` never maps an annotation interface's members).
    override fun mapAnnotationAttributeName(descriptor: String, name: String): String = name
}

/**
 * Rewrite one class through [remapper]. `ClassWriter(0)` without a reader
 * rebuilds the constant pool from what is written, so a `Utf8` slot javac
 * shared between a member name and an `ldc` string literal (every enum
 * constant's name, for instance) comes out as two: the literal keeps its
 * text, the member takes the map's. Frames pass through untouched (flag 0);
 * nothing but pico-jvm loads the result.
 */
fun shrinkMembers(bytes: ByteArray, remapper: Remapper): ByteArray {
    val cr = ClassReader(bytes)
    val cw = ClassWriter(0)
    cr.accept(ClassRemapper(cw, remapper), 0)
    return cw.toByteArray()
}
