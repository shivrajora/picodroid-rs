// SPDX-License-Identifier: GPL-3.0-only
package picodroid.survey

import java.util.TreeMap

/**
 * Per-class census from a raw walk of the class file — the same walk
 * `jvm/src/class_file/parse.rs` does (constant pool by tag, then fields,
 * methods and attributes by length), which is why it is the right measurement:
 * [attrBytes] is exactly what pico-jvm skips, [cpCount] what it charges for.
 *
 * ASM is not used here on purpose: it hides attribute lengths.
 */
data class Census(
    val className: String,
    val sourceFile: String,
    val major: Int,
    val minor: Int,
    val bytes: Int,
    val cpCount: Int,
    val access: Int,
    val superName: String,
    val interfaces: List<String>,
    val fields: Int,
    val methods: Int,
    val synchronizedMethods: Int,
    val defaultMethods: Int,
    val bridgeMethods: Int,
    val syntheticMethods: Int,
    val defaultArgBridges: Int,
    val hasMetadata: Boolean,
    /** attribute name → total bytes (attribute_length + 6 header bytes), summed over every occurrence. `Code` includes its sub-attributes. */
    val attrBytes: Map<String, Int>,
    /** constant-pool tag → entry count. */
    val cpTagCounts: Map<Int, Int>,
    /** every CONSTANT_Class entry: (cp index, internal name). */
    val cpClasses: List<Pair<Int, String>>,
) {
    val isInterface: Boolean get() = access and ACC_INTERFACE != 0

    fun attr(name: String): Int = attrBytes[name] ?: 0

    fun cpTag(tag: Int): Int = cpTagCounts[tag] ?: 0

    companion object {
        const val ACC_STATIC = 0x0008
        const val ACC_SYNCHRONIZED = 0x0020
        const val ACC_BRIDGE = 0x0040
        const val ACC_INTERFACE = 0x0200
        const val ACC_ABSTRACT = 0x0400
        const val ACC_SYNTHETIC = 0x1000
    }
}

private class Cursor(val b: ByteArray) {
    var pos = 0

    fun u1(): Int = b[pos++].toInt() and 0xff

    fun u2(): Int = (u1() shl 8) or u1()

    fun u4(): Int = (u2() shl 16) or u2()

    fun skip(n: Int) {
        require(n >= 0 && pos + n <= b.size) { "truncated class file at $pos (+$n of ${b.size})" }
        pos += n
    }
}

/** Census of the class file at [bytes]; throws on any structure pico-jvm would also reject. */
fun census(bytes: ByteArray): Census {
    val c = Cursor(bytes)
    require(c.u4() == 0xCAFEBABE.toInt()) { "bad magic" }
    val minor = c.u2()
    val major = c.u2()
    val cpCount = c.u2()
    val utf8 = HashMap<Int, String>()
    val classNameIdx = TreeMap<Int, Int>()
    val tagCounts = TreeMap<Int, Int>()
    var i = 1
    while (i < cpCount) {
        val tag = c.u1()
        tagCounts.merge(tag, 1, Int::plus)
        when (tag) {
            1 -> {
                val len = c.u2()
                utf8[i] = String(bytes, c.pos, len, Charsets.UTF_8)
                c.skip(len)
            }
            3, 4 -> c.skip(4)
            5, 6 -> {
                c.skip(8)
                i++ // long/double take two slots
            }
            7 -> classNameIdx[i] = c.u2()
            8 -> c.skip(2)
            9, 10, 11, 12 -> c.skip(4)
            15 -> c.skip(3)
            16 -> c.skip(2)
            17, 18 -> c.skip(4)
            19, 20 -> c.skip(2)
            else -> error("unknown CP tag $tag at index $i")
        }
        i++
    }
    fun cls(idx: Int): String = classNameIdx[idx]?.let { utf8[it] } ?: "?"
    val access = c.u2()
    val className = cls(c.u2())
    val superIdx = c.u2()
    val superName = if (superIdx == 0) "" else cls(superIdx)
    val interfaces = List(c.u2()) { cls(c.u2()) }

    val attrBytes = TreeMap<String, Int>()
    var sourceFile = ""

    fun attributes() {
        val n = c.u2()
        repeat(n) {
            val name = utf8[c.u2()] ?: "?"
            val len = c.u4()
            attrBytes.merge(name, len + 6, Int::plus)
            val start = c.pos
            when (name) {
                "SourceFile" -> sourceFile = utf8[c.u2()] ?: ""
                "Code" -> {
                    c.skip(4) // max_stack, max_locals
                    c.skip(c.u4()) // code
                    c.skip(c.u2() * 8) // exception table
                    attributes() // LineNumberTable, LocalVariableTable, StackMapTable, ...
                }
                else -> {}
            }
            c.pos = start + len
        }
    }

    val fields = c.u2()
    repeat(fields) {
        c.skip(6)
        attributes()
    }

    val methods = c.u2()
    var synchronizedMethods = 0
    var defaultMethods = 0
    var bridgeMethods = 0
    var syntheticMethods = 0
    var defaultArgBridges = 0
    val isInterface = access and Census.ACC_INTERFACE != 0
    repeat(methods) {
        val macc = c.u2()
        val mname = utf8[c.u2()] ?: "?"
        c.skip(2)
        if (macc and Census.ACC_SYNCHRONIZED != 0) synchronizedMethods++
        if (macc and Census.ACC_BRIDGE != 0) bridgeMethods++
        if (macc and Census.ACC_SYNTHETIC != 0) syntheticMethods++
        if (mname.endsWith("\$default")) defaultArgBridges++
        if (isInterface && macc and Census.ACC_ABSTRACT == 0 && macc and Census.ACC_STATIC == 0 && mname != "<clinit>") defaultMethods++
        attributes()
    }
    attributes()
    require(c.pos == bytes.size) { "trailing bytes: ${bytes.size - c.pos}" }

    return Census(
        className = className,
        sourceFile = sourceFile,
        major = major,
        minor = minor,
        bytes = bytes.size,
        cpCount = cpCount,
        access = access,
        superName = superName,
        interfaces = interfaces,
        fields = fields,
        methods = methods,
        synchronizedMethods = synchronizedMethods,
        defaultMethods = defaultMethods,
        bridgeMethods = bridgeMethods,
        syntheticMethods = syntheticMethods,
        defaultArgBridges = defaultArgBridges,
        hasMetadata = utf8.values.any { it == "Lkotlin/Metadata;" },
        attrBytes = attrBytes,
        cpTagCounts = tagCounts,
        cpClasses = classNameIdx.map { (idx, nameIdx) -> idx to (utf8[nameIdx] ?: "?") },
    )
}
