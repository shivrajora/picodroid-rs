// SPDX-License-Identifier: GPL-3.0-only
package picodroid.classfile

import org.objectweb.asm.ClassReader
import org.objectweb.asm.ClassVisitor
import org.objectweb.asm.FieldVisitor
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes
import org.objectweb.asm.Type

/**
 * The kotlin-shim contract (roadmap Session 5), three directions:
 *
 * - **A — missing (error):** every load-bearing `kotlin/…` reference a fixture
 *   app makes must resolve in the shim by Java resolution rules (the owner
 *   class exists; a member exists on the owner or a shim superclass /
 *   superinterface, under its `@ShimName`). Owners on the never-shimmed list
 *   are rejected with a docs pointer. The report prints ready-to-paste Java
 *   signatures grouped by class.
 * - **B — unused (warning, or error with [ContractOptions.strictUnused]):**
 *   shim members nothing references — neither a fixture nor another shim
 *   class; `@ShimKeep` exempts.
 * - **C — JDK allowlist (error):** every `java/…` reference made by a shim
 *   class or a fixture must be a row of the allowlist, so the `java/…`
 *   surface Kotlin apps lean on is an explicit, reviewed list that a JVM-side
 *   test cross-checks against the builtin tables. Missing rows are printed as
 *   TSV.
 *
 * Pure: no Gradle, no file I/O.
 */
object ShimContract {
    /** Kinds that name a member of `owner`. */
    private val MEMBER_KINDS = setOf(
        "invokestatic", "invokevirtual", "invokespecial", "invokeinterface",
        "getstatic", "putstatic", "getfield", "putfield", "indy_impl",
    )

    /** Kinds that name only a class. */
    private val CLASS_KINDS = setOf(
        "new", "anewarray", "multianewarray", "checkcast", "instanceof", "ldc_class",
        "super", "interface", "catch_type", "indy_sam", "indy_arg_type",
    )

    /** Owners the roadmap rules out (§ Never shimmed). Prefix match. */
    val NEVER_SHIMMED: List<String> = listOf(
        "kotlin/jvm/internal/Reflection",
        "kotlin/jvm/internal/FunctionReferenceImpl",
        "kotlin/jvm/internal/FunctionReference",
        "kotlin/jvm/internal/PropertyReference",
        "kotlin/jvm/internal/CallableReference",
        "kotlin/jvm/internal/TypeIntrinsics",
        "kotlin/reflect/",
        "kotlin/sequences/",
        "kotlin/text/Regex",
        "kotlin/coroutines/",
        "kotlinx/",
        "kotlin/collections/builders/",
        "kotlin/concurrent/",
        "kotlin/jvm/JvmClassMappingKt",
        "kotlin/io/",
    )

    const val NEVER_SHIMMED_DOCS = "docs/designs/kotlin-roadmap-2026-08.md § Shim inventory → Never shimmed"

    private const val LAMBDA_METAFACTORY = "java/lang/invoke/LambdaMetafactory"

    data class ContractOptions(val strictUnused: Boolean = false)

    /** One `owner<TAB>name<TAB>desc` row; class-only rows have empty name and desc. */
    data class AllowRow(val owner: String, val name: String, val desc: String) {
        fun tsv(): String = "$owner\t$name\t$desc"
    }

    /** Parse the allowlist text: `#` comments and blank lines ignored; a bare owner is a class-only row. */
    fun parseAllowlist(text: String): Set<AllowRow> =
        text.lineSequence()
            .map { it.trimEnd('\r', ' ') }
            .filter { it.isNotBlank() && !it.startsWith("#") }
            .map { line ->
                val parts = line.split('\t')
                require(parts.size == 1 || parts.size == 3) { "allowlist row needs 1 or 3 tab-separated columns: '$line'" }
                if (parts.size == 1) AllowRow(parts[0], "", "") else AllowRow(parts[0], parts[1], parts[2])
            }
            .toSet()

    private data class Member(val name: String, val desc: String, val access: Int)

    private class ShimClass(
        val name: String,
        val superName: String?,
        val interfaces: List<String>,
        val methods: List<Member>,
        val fields: List<Member>,
    )

    /** A Direction-A miss: what the fixture wanted and where. */
    data class Miss(val ref: Ref, val reason: String)

    data class ContractReport(
        val fixtureClasses: Int,
        val shimClasses: Int,
        val missing: List<Miss>,
        val rejected: List<Ref>,
        val unused: List<MemberKey>,
        val allowlistMissing: List<AllowRow>,
        val allowlistUnused: List<AllowRow>,
        val informational: Map<String, Int>,
        val strictUnused: Boolean,
    ) {
        val ok: Boolean
            get() = missing.isEmpty() && rejected.isEmpty() && allowlistMissing.isEmpty() &&
                (!strictUnused || unused.isEmpty())

        fun render(): String = buildString {
            appendLine("shim contract: $fixtureClasses fixture classes, $shimClasses shim classes")
            if (rejected.isNotEmpty()) {
                appendLine()
                appendLine("REJECTED — out-of-scope kotlin/… owners ($NEVER_SHIMMED_DOCS):")
                rejected.groupBy { it.owner }.toSortedMap().forEach { (owner, refs) ->
                    appendLine("  $owner  (from ${refs.map { "${it.fromClass}.${it.fromMember}" }.distinct().sorted().joinToString()})")
                }
            }
            if (missing.isNotEmpty()) {
                appendLine()
                appendLine("MISSING — kotlin/… references no shim class serves. Ready to paste:")
                missing.groupBy { it.ref.owner }.toSortedMap().forEach { (owner, misses) ->
                    appendLine()
                    appendLine("  // $owner  (from ${misses.map { it.ref.fromClass }.distinct().sorted().joinToString()})")
                    misses.map { javaSignature(it.ref) to it.reason }.distinct().forEach { (sig, reason) ->
                        appendLine("  $sig  // $reason")
                    }
                }
            }
            if (allowlistMissing.isNotEmpty()) {
                appendLine()
                appendLine("ALLOWLIST — java/… references absent from kotlin-shim/jdk-allowlist.tsv (paste, then make sure pico-jvm serves them):")
                allowlistMissing.distinct().sortedWith(compareBy({ it.owner }, { it.name }, { it.desc })).forEach { appendLine("  " + it.tsv()) }
            }
            if (unused.isNotEmpty()) {
                appendLine()
                appendLine((if (strictUnused) "UNUSED (error)" else "unused (warning)") + " — shim members no fixture or shim class references (@ShimKeep exempts):")
                unused.groupBy { it.owner }.toSortedMap().forEach { (owner, ms) ->
                    appendLine("  $owner: " + ms.map { it.name + it.desc }.sorted().joinToString())
                }
            }
            if (allowlistUnused.isNotEmpty()) {
                appendLine()
                appendLine("allowlist rows nothing references (informational): " + allowlistUnused.size)
            }
            if (informational.isNotEmpty()) {
                appendLine("informational kinds (descriptor/signature/annotation only): " + informational.toSortedMap().entries.joinToString { "${it.key}=${it.value}" })
            }
            appendLine(if (ok) "shim contract: OK" else "shim contract: FAILED")
        }
    }

    fun check(
        shim: List<ClassEntry>,
        fixtures: List<ClassEntry>,
        allowlist: Set<AllowRow>,
        options: ContractOptions = ContractOptions(),
    ): ContractReport {
        val shimEntries = shim.filter { ShimShaker.isShimClass(it.className) }
        val marks = ShimShaker.collectMarks(shim)
        val index = shimEntries.associate { e -> e.className to readShimClass(e.bytes, marks.renames) }

        val fixtureRefs = fixtures.flatMap { extract(it.bytes) }
        val shimRefs = shimEntries.flatMap { extract(it.bytes) }

        // ── Direction A ──
        val missing = ArrayList<Miss>()
        val rejected = ArrayList<Ref>()
        val informational = HashMap<String, Int>()
        for (r in fixtureRefs) {
            if (!ShimShaker.isShimClass(r.owner)) continue
            if (r.kind in NON_LOAD_BEARING_KINDS) {
                informational.merge(r.kind, 1, Int::plus)
                continue
            }
            if (NEVER_SHIMMED.any { r.owner.startsWith(it) }) {
                rejected += r
                continue
            }
            val owner = index[r.owner]
            if (owner == null) {
                missing += Miss(r, "class missing")
                continue
            }
            when (r.kind) {
                in MEMBER_KINDS -> {
                    val reason = resolveMember(index, owner, r, allowlist)
                    if (reason != null) missing += Miss(r, reason)
                }
                "indy_sam" -> if (owner.methods.none { it.name == r.name && it.access and Opcodes.ACC_ABSTRACT != 0 }) {
                    missing += Miss(r, "SAM interface has no abstract method named ${r.name}")
                }
                in CLASS_KINDS -> {}
                else -> informational.merge(r.kind, 1, Int::plus)
            }
        }

        // ── Direction B ──
        // Every member reference, by owner, under strip-time names. A shim
        // method is used when the reference names its class, a shim supertype
        // that declares it, or a java/… supertype (an override of
        // `List.size()` is reached through `invokeinterface List.size`).
        val used = HashSet<MemberKey>()
        (fixtureRefs + shimRefs).filter { it.kind in MEMBER_KINDS }.forEach { r ->
            val key = MemberKey(r.owner, r.name, r.desc)
            used += key
            // A shim-internal call site still names the Java method; the strip renames it too.
            marks.renames[key]?.let { used += MemberKey(r.owner, it, r.desc) }
        }
        val unused = ArrayList<MemberKey>()
        index.values.forEach { c ->
            val supertypes = supertypesOf(index, c)
            c.methods.forEach { m ->
                val key = MemberKey(c.name, m.name, m.desc)
                val isPrivateOrSynthetic = m.access and (Opcodes.ACC_PRIVATE or Opcodes.ACC_SYNTHETIC or Opcodes.ACC_BRIDGE) != 0
                if (m.name == "<clinit>" || isPrivateOrSynthetic) return@forEach
                if (m.name == "<init>" && index.values.any { it.superName == c.name }) return@forEach // subclass ctor chains to it
                if (key in used || key in marks.keepMethods || c.name in marks.keepClasses) return@forEach
                // Abstract interface members are the SAM/protocol surface; only bodies are "unused".
                if (m.access and Opcodes.ACC_ABSTRACT != 0) return@forEach
                // Overrides of Object's contract are reached by the JVM's virtual dispatch, not by name.
                if (m.name in setOf("toString", "equals", "hashCode")) return@forEach
                if (supertypes.any { MemberKey(it, m.name, m.desc) in used }) return@forEach
                unused += key
            }
        }

        // ── Direction C ──
        val allowMissing = ArrayList<AllowRow>()
        val allowUsed = HashSet<AllowRow>()
        for (r in fixtureRefs + shimRefs) {
            if (!r.owner.startsWith("java/")) continue
            if (r.kind in NON_LOAD_BEARING_KINDS) continue
            if (r.kind == "indy_bsm") {
                if (r.owner != LAMBDA_METAFACTORY) allowMissing += AllowRow(r.owner, r.name, r.desc)
                continue
            }
            val row = when (r.kind) {
                in MEMBER_KINDS -> AllowRow(r.owner, r.name, r.desc)
                in CLASS_KINDS -> AllowRow(r.owner, "", "")
                else -> continue
            }
            if (row in allowlist) allowUsed += row else allowMissing += row
        }
        val allowUnused = (allowlist - allowUsed).sortedWith(compareBy({ it.owner }, { it.name }, { it.desc }))

        return ContractReport(
            fixtureClasses = fixtures.size,
            shimClasses = shimEntries.size,
            missing = missing,
            rejected = rejected.distinctBy { it.owner to it.fromClass },
            unused = unused.sortedWith(compareBy({ it.owner }, { it.name }, { it.desc })),
            allowlistMissing = allowMissing.distinct(),
            allowlistUnused = allowUnused,
            informational = informational,
            strictUnused = options.strictUnused,
        )
    }

    /** Null when [r] resolves; otherwise why not. Members are looked up under their strip-time names. */
    private fun resolveMember(index: Map<String, ShimClass>, owner: ShimClass, r: Ref, allowlist: Set<AllowRow>): String? {
        val isField = r.kind.startsWith("get") || r.kind.startsWith("put")
        val declarer = declarerOf(index, owner, r.name, r.desc, isField)
        if (declarer != null) return null
        // The walk left the shim: a java/… supertype (class or interface) whose
        // member is allowlisted is served by the JVM's runtime-class dispatch
        // (`EnumEntries.size()` → `List.size()`, `Pair.hashCode()` → Object's).
        val javaSupers = supertypesOf(index, owner).filter { it.startsWith("java/") }
        if (javaSupers.any { AllowRow(it, r.name, r.desc) in allowlist }) return null
        return if (isField) "field missing" else "method missing (walked ${r.owner}" +
            (if (javaSupers.isEmpty()) "" else " → … → ${javaSupers.joinToString(" | ")}") + ")"
    }

    /** The shim class declaring `name`/`desc`, searching the owner, its superclasses and superinterfaces. */
    private fun declarerOf(index: Map<String, ShimClass>, start: ShimClass, name: String, desc: String, isField: Boolean = false): String? {
        val seen = HashSet<String>()
        val queue = ArrayDeque<ShimClass>()
        queue += start
        while (queue.isNotEmpty()) {
            val c = queue.removeFirst()
            if (!seen.add(c.name)) continue
            val members = if (isField) c.fields else c.methods
            if (members.any { it.name == name && it.desc == desc }) return c.name
            (listOfNotNull(c.superName) + c.interfaces).mapNotNull { index[it] }.forEach { queue += it }
        }
        return null
    }

    /** Every supertype name of [start] (shim and java/…, classes and interfaces), excluding itself. */
    private fun supertypesOf(index: Map<String, ShimClass>, start: ShimClass): Set<String> {
        val out = LinkedHashSet<String>()
        val queue = ArrayDeque<String>()
        queue += listOfNotNull(start.superName) + start.interfaces
        while (queue.isNotEmpty()) {
            val n = queue.removeFirst()
            if (!out.add(n)) continue
            index[n]?.let { queue += listOfNotNull(it.superName) + it.interfaces }
        }
        return out
    }

    private fun readShimClass(bytes: ByteArray, renames: Map<MemberKey, String>): ShimClass {
        var name = "?"
        var superName: String? = null
        var interfaces: List<String> = emptyList()
        val methods = ArrayList<Member>()
        val fields = ArrayList<Member>()
        ClassReader(bytes).accept(object : ClassVisitor(Opcodes.ASM9) {
            override fun visit(version: Int, access: Int, n: String, signature: String?, sup: String?, ifs: Array<String>?) {
                name = n
                superName = sup
                interfaces = ifs?.toList() ?: emptyList()
            }

            override fun visitMethod(access: Int, n: String, descriptor: String, signature: String?, exceptions: Array<String>?): MethodVisitor? {
                methods += Member(renames[MemberKey(name, n, descriptor)] ?: n, descriptor, access)
                return null
            }

            override fun visitField(access: Int, n: String, descriptor: String, signature: String?, value: Any?): FieldVisitor? {
                fields += Member(n, descriptor, access)
                return null
            }
        }, ClassReader.SKIP_CODE or ClassReader.SKIP_FRAMES)
        return ShimClass(name, superName, interfaces, methods, fields)
    }

    // ── Java signature rendering for the MISSING section ──

    private fun javaType(t: Type): String = when (t.sort) {
        Type.VOID -> "void"
        Type.BOOLEAN -> "boolean"
        Type.CHAR -> "char"
        Type.BYTE -> "byte"
        Type.SHORT -> "short"
        Type.INT -> "int"
        Type.LONG -> "long"
        Type.FLOAT -> "float"
        Type.DOUBLE -> "double"
        Type.ARRAY -> javaType(t.elementType) + "[]".repeat(t.dimensions)
        else -> t.internalName.replace('/', '.').replace('$', '.').let {
            if (it.startsWith("java.lang.") && it.count { c -> c == '.' } == 2) it.removePrefix("java.lang.") else it
        }
    }

    fun javaSignature(r: Ref): String {
        val simple = r.owner.substringAfterLast('/').substringAfterLast('$')
        return when (r.kind) {
            "invokestatic" -> {
                val mt = Type.getMethodType(r.desc)
                "public static ${javaType(mt.returnType)} ${r.name}(${params(mt)}) { }"
            }
            "invokevirtual", "invokeinterface", "invokespecial", "indy_impl" -> {
                val mt = Type.getMethodType(r.desc)
                if (r.name == "<init>") "public $simple(${params(mt)}) { }"
                else "public ${javaType(mt.returnType)} ${r.name}(${params(mt)}) { }"
            }
            "getstatic", "putstatic" -> "public static ${javaType(Type.getType(r.desc))} ${r.name};"
            "getfield", "putfield" -> "public ${javaType(Type.getType(r.desc))} ${r.name};"
            "indy_sam" -> "public interface $simple { ... ${r.name}(...); }"
            "super" -> "public class $simple { }  // used as a superclass"
            "interface" -> "public interface $simple { }"
            else -> "public final class $simple { }  // ${r.kind}"
        }
    }

    private fun params(mt: Type): String =
        mt.argumentTypes.mapIndexed { i, t -> "${javaType(t)} p$i" }.joinToString(", ")
}
