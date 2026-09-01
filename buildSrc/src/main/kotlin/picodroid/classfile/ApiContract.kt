// SPDX-License-Identifier: GPL-3.0-only
package picodroid.classfile

import org.objectweb.asm.ClassReader
import org.objectweb.asm.ClassVisitor
import org.objectweb.asm.FieldVisitor
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes

/**
 * The compile-time API contract (android-parity roadmap E3 phase 1): every
 * load-bearing `java/…` / `javax/…` reference an app's compiled classes
 * make must be served by pico-jvm, as recorded in the GENERATED
 * `sdk/api-contract.tsv` (picodroid-core/src/native_handler/api_contract.rs
 * writes it from the runtime's own tables). Apps compile against the host
 * JDK's full `java.*`, so without this check `new LinkedList<>()` or
 * `str.matches(..)` compiles cleanly and dies on device with `NoSuchMethod`.
 *
 * Resolution models `dispatch_native` (jvm/src/interpreter/ops_invoke.rs):
 * a member is served on an owner when a row matches on the owner, on any
 * `@extends` ancestor, or on `java/lang/Object`; a member of an interface is
 * served when a builtin that implements it (transitively) serves it, or an
 * app class implementing it declares it. An app-typed owner is walked up
 * through the app's own classes to its `java/…` supertypes, so
 * `class MyEx extends RuntimeException` + `e.printStackTrace()` is rejected.
 *
 * Also rejects any reference to a class the target board drops through
 * `framework_class_excludes` (board.toml), when a board is given.
 *
 * Pure: no Gradle, no file I/O.
 */
object ApiContract {
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

    private const val LAMBDA_METAFACTORY = "java/lang/invoke/LambdaMetafactory"
    private const val OBJECT = "java/lang/Object"

    /** `@hint` row: [owner] is exact, or a prefix when it ends with `*`. */
    data class Hint(val owner: String, val name: String, val text: String)

    /** Class kinds that instantiate: a name-only class does not resolve for these. */
    private val INSTANTIATING_KINDS = setOf("new", "anewarray", "multianewarray")

    class Contract(
        val classes: Set<String>,
        /** Exact rows (`desc` set) and name-level rows (`desc == ""`). */
        val members: Set<MemberKey>,
        val extends: Map<String, String>,
        val implements: Map<String, Set<String>>,
        val hints: List<Hint>,
        /** `@nameonly` rows: catch / super / cast targets that are never instantiated. */
        val nameOnly: Set<String> = emptySet(),
    ) {
        private val ownersWithMembers: Set<String> = members.mapTo(HashSet()) { it.owner }

        /** Every builtin class (transitively) below each type: interface → implementors, class → subclasses. */
        private val implementors: Map<String, Set<String>> = run {
            val out = HashMap<String, MutableSet<String>>()
            val all = HashSet<String>(classes) + extends.keys + implements.keys
            for (c in all) for (s in supertypes(c)) out.getOrPut(s) { HashSet() } += c
            out
        }

        /** Whether a class-kind reference of [kind] to [owner] resolves. */
        fun resolvesClass(owner: String, kind: String): Boolean =
            owner in classes || (kind !in INSTANTIATING_KINDS && owner in nameOnly)

        /** True when the owner is known at all — a class row, a name-only row, or member rows (`java/util/Locale`). */
        fun knows(owner: String): Boolean = owner in classes || owner in nameOnly || owner in ownersWithMembers

        /** Every transitive supertype of [type] in the contract's hierarchy, excluding itself. */
        fun supertypes(type: String): Set<String> {
            val out = LinkedHashSet<String>()
            val queue = ArrayDeque<String>()
            queue += type
            while (queue.isNotEmpty()) {
                val t = queue.removeFirst()
                extends[t]?.let { if (out.add(it)) queue += it }
                implements[t].orEmpty().forEach { if (out.add(it)) queue += it }
            }
            return out
        }

        private fun ownRow(owner: String, name: String, desc: String): Boolean =
            MemberKey(owner, name, desc) in members || MemberKey(owner, name, "") in members

        /** The owner, its `@extends` chain, then `java/lang/Object` (every chain ends there, as in `dispatch_native`). */
        private fun chainServes(owner: String, name: String, desc: String): Boolean {
            var cur: String? = owner
            val seen = HashSet<String>()
            while (cur != null && seen.add(cur)) {
                if (ownRow(cur, name, desc)) return true
                cur = extends[cur] ?: if (cur != OBJECT) OBJECT else null
            }
            return false
        }

        /**
         * Whether a reference `owner.name(desc)` is served: on the owner's
         * own chain, or — the owner being an interface or abstract base the
         * receiver's runtime class refines — on any builtin below it.
         */
        fun servesMember(owner: String, name: String, desc: String): Boolean {
            if (chainServes(owner, name, desc)) return true
            return implementors[owner].orEmpty().any { chainServes(it, name, desc) }
        }

        fun hintFor(owner: String, name: String): String? {
            hints.firstOrNull { it.owner == owner && it.name == name }?.let { return it.text }
            hints.firstOrNull { it.owner == owner && it.name.isEmpty() }?.let { return it.text }
            return hints.filter { it.owner.endsWith("*") && owner.startsWith(it.owner.dropLast(1)) }
                .maxByOrNull { it.owner.length }?.text
        }
    }

    /** Parse `sdk/api-contract.tsv`; malformed lines are an error (the file is generated). */
    fun parse(text: String): Contract {
        val classes = HashSet<String>()
        val members = HashSet<MemberKey>()
        val extends = HashMap<String, String>()
        val implements = HashMap<String, MutableSet<String>>()
        val hints = ArrayList<Hint>()
        val nameOnly = HashSet<String>()
        text.lineSequence().forEachIndexed { i, raw ->
            val line = raw.trimEnd('\r')
            if (line.isBlank() || line.startsWith("#")) return@forEachIndexed
            val parts = line.split('\t')
            fun bad(): Nothing = throw IllegalArgumentException("api-contract.tsv line ${i + 1}: malformed row '$line'")
            when {
                parts[0] == "@extends" -> if (parts.size == 3) extends[parts[1]] = parts[2] else bad()
                parts[0] == "@implements" ->
                    if (parts.size == 3) implements.getOrPut(parts[1]) { HashSet() } += parts[2] else bad()
                parts[0] == "@hint" -> if (parts.size == 4) hints += Hint(parts[1], parts[2], parts[3]) else bad()
                parts[0] == "@nameonly" -> if (parts.size == 2) nameOnly += parts[1] else bad()
                parts[0].startsWith("@") -> bad()
                parts.size == 1 -> classes += parts[0]
                parts.size == 3 && parts[1].isNotEmpty() -> members += MemberKey(parts[0], parts[1], parts[2])
                else -> bad()
            }
        }
        return Contract(classes, members, extends, implements, hints, nameOnly)
    }

    /** What a board drops: [name] and the `framework_class_excludes` list of its board.toml. */
    data class BoardExcludes(val name: String, val tomlPath: String, val classes: Set<String>) {
        /** Inner classes follow their outer class (build_support/papk.rs). */
        fun excludes(owner: String): Boolean = owner in classes || owner.substringBefore('$') in classes
    }

    /**
     * The top-level `framework_class_excludes = "a;b,c"` key of a board.toml
     * — the same hand-rolled, line-based read as `build_support/board_cfg.rs`
     * (`;` or `,` separated, one line, top level only).
     */
    fun parseBoardExcludes(toml: String): Set<String> {
        for (raw in toml.lineSequence()) {
            val line = raw.trim()
            if (line.startsWith("[")) break
            if (!line.startsWith("framework_class_excludes")) continue
            val rest = line.substringAfter("framework_class_excludes").trimStart().removePrefix("=").trim()
            val value = rest.trim('"', '\'')
            return value.split(';', ',').map { it.trim() }.filter { it.isNotEmpty() }.toSet()
        }
        return emptySet()
    }

    private class AppClass(
        val name: String,
        val superName: String?,
        val interfaces: List<String>,
        val methods: Set<Pair<String, String>>,
        val fields: Set<Pair<String, String>>,
    )

    private fun readAppClass(bytes: ByteArray): AppClass {
        var name = "?"
        var superName: String? = null
        var interfaces: List<String> = emptyList()
        val methods = HashSet<Pair<String, String>>()
        val fields = HashSet<Pair<String, String>>()
        ClassReader(bytes).accept(object : ClassVisitor(Opcodes.ASM9) {
            override fun visit(version: Int, access: Int, n: String, signature: String?, sup: String?, ifs: Array<String>?) {
                name = n
                superName = sup
                interfaces = ifs?.toList() ?: emptyList()
            }

            override fun visitMethod(access: Int, n: String, descriptor: String, signature: String?, exceptions: Array<String>?): MethodVisitor? {
                methods += n to descriptor
                return null
            }

            override fun visitField(access: Int, n: String, descriptor: String, signature: String?, value: Any?): FieldVisitor? {
                fields += n to descriptor
                return null
            }
        }, ClassReader.SKIP_CODE or ClassReader.SKIP_FRAMES)
        return AppClass(name, superName, interfaces, methods, fields)
    }

    /** One rejected reference. */
    data class Miss(val ref: Ref, val reason: String, val hint: String?)

    class Report(
        val classCount: Int,
        val misses: List<Miss>,
        val excluded: List<Miss>,
        val board: BoardExcludes?,
    ) {
        val ok: Boolean get() = misses.isEmpty() && excluded.isEmpty()

        private fun where(refs: List<Ref>): String =
            refs.map { "${it.fromClass}.${it.fromMember.substringBefore('(')}" }.distinct().sorted().joinToString()

        private fun describe(r: Ref): String = when {
            r.kind !in MEMBER_KINDS -> "${r.owner}  [${r.kind}]"
            r.kind.startsWith("get") || r.kind.startsWith("put") -> "${r.owner}.${r.name} : ${r.desc}"
            else -> "${r.owner}.${r.name}${r.desc}"
        }

        fun render(mode: String): String = buildString {
            appendLine("api contract: $classCount app classes checked against sdk/api-contract.tsv" +
                (board?.let { ", board ${it.name} (${it.classes.size} excluded classes)" } ?: ""))
            if (misses.isNotEmpty()) {
                appendLine()
                appendLine("NOT SERVED — java/… references pico-jvm does not implement (they compile against the host JDK and die on device with NoSuchMethod):")
                misses.groupBy { it.ref.owner }.toSortedMap().forEach { (_, ms) ->
                    appendLine()
                    ms.groupBy { describe(it.ref) to it.reason }.toSortedMap(compareBy({ it.first }, { it.second })).forEach { (key, group) ->
                        val (what, reason) = key
                        appendLine("  $what  // $reason")
                        group.firstNotNullOfOrNull { it.hint }?.let { appendLine("      hint: $it") }
                        appendLine("      from: ${where(group.map { it.ref })}")
                    }
                }
            }
            if (excluded.isNotEmpty()) {
                appendLine()
                appendLine("EXCLUDED ON BOARD ${board?.name} — classes this board drops from its framework (framework_class_excludes in ${board?.tomlPath}):")
                excluded.groupBy { it.ref.owner }.toSortedMap().forEach { (owner, ms) ->
                    appendLine("  $owner  (from: ${where(ms.map { it.ref })})")
                }
            }
            if (!ok) {
                appendLine()
                appendLine("sdk/api-contract.tsv is GENERATED from the runtime's tables — do not paste rows into it.")
                appendLine("To support a member, add the arm and its BUILTIN_METHODS row (jvm/src/native/mod.rs), then run scripts/gen-api-contract.sh.")
                appendLine("To bypass while experimenting: -Ppicodroid.apiContract=warn (or off).")
            }
            appendLine(if (ok) "api contract: OK" else if (mode == "error") "api contract: FAILED" else "api contract: WARN")
        }
    }

    fun check(app: List<ClassEntry>, contract: Contract, board: BoardExcludes?): Report {
        val index: Map<String, AppClass> = app.map { readAppClass(it.bytes) }.associateBy { it.name }

        /** Transitive supertypes of an app class: through the app index, then the contract's hierarchy. */
        fun appSupertypes(start: AppClass): Set<String> {
            val out = LinkedHashSet<String>()
            val queue = ArrayDeque<String>()
            queue += listOfNotNull(start.superName) + start.interfaces
            while (queue.isNotEmpty()) {
                val n = queue.removeFirst()
                if (!out.add(n)) continue
                val ac = index[n]
                if (ac != null) queue += listOfNotNull(ac.superName) + ac.interfaces
                else out += contract.supertypes(n)
            }
            return out
        }

        /**
         * App classes below each type they reach — the receiver's runtime
         * class refines the static owner: an app `List` implementation serves
         * `List.indexOf`, and a shim `EnumEntriesList` serves
         * `EnumEntries.indexOf` for the `EnumEntries` interface.
         */
        val appSubtypes = HashMap<String, MutableList<AppClass>>()
        for (c in index.values) for (s in appSupertypes(c)) appSubtypes.getOrPut(s) { ArrayList() } += c

        fun declares(c: AppClass, r: Ref): Boolean {
            val isField = r.kind.startsWith("get") || r.kind.startsWith("put")
            return (r.name to r.desc) in (if (isField) c.fields else c.methods)
        }

        val misses = ArrayList<Miss>()
        val excluded = ArrayList<Miss>()
        fun miss(r: Ref, reason: String) {
            misses += Miss(r, reason, contract.hintFor(r.owner, if (r.kind in MEMBER_KINDS) r.name else ""))
        }

        for (entry in app) for (r in extract(entry.bytes)) {
            if (r.kind in NON_LOAD_BEARING_KINDS) continue
            if (r.owner.startsWith("[")) continue
            if (board != null && board.excludes(r.owner)) {
                excluded += Miss(r, "excluded on board ${board.name}", null)
                continue
            }
            if (r.kind == "indy_bsm") {
                if (r.owner != LAMBDA_METAFACTORY) miss(r, "invokedynamic bootstrap not supported (only LambdaMetafactory lambdas)")
                continue
            }
            val owner = r.owner
            val isJava = owner.startsWith("java/") || owner.startsWith("javax/")
            when {
                isJava && (r.kind in CLASS_KINDS) ->
                    if (!contract.resolvesClass(owner, r.kind)) miss(r, "class not available in pico-jvm")
                isJava && (r.kind in MEMBER_KINDS) -> {
                    if (contract.servesMember(owner, r.name, r.desc)) continue
                    if (appSubtypes[owner].orEmpty().any { declares(it, r) }) continue
                    miss(r, if (contract.knows(owner)) "member not served" else "class not available in pico-jvm")
                }
                isJava -> {}
                r.kind in MEMBER_KINDS && index.containsKey(owner) -> {
                    // App-typed owner: resolve through the app's own classes,
                    // then through the java/… supertypes the walk reaches.
                    val start = index.getValue(owner)
                    var found = declares(start, r)
                    val frontier = LinkedHashSet<String>()
                    if (!found) {
                        val seen = HashSet<String>()
                        val queue = ArrayDeque<String>()
                        queue += listOfNotNull(start.superName) + start.interfaces
                        while (queue.isNotEmpty() && !found) {
                            val n = queue.removeFirst()
                            if (!seen.add(n)) continue
                            val ac = index[n]
                            if (ac == null) frontier += n
                            else if (declares(ac, r)) found = true
                            else queue += listOfNotNull(ac.superName) + ac.interfaces
                        }
                    }
                    if (found) continue
                    // The runtime receiver may be an app subtype that declares it.
                    if (appSubtypes[owner].orEmpty().any { declares(it, r) }) continue
                    // A picodroid/** or other non-java supertype: javac already
                    // resolved the member against the SDK; nothing to model.
                    if (frontier.any { !(it.startsWith("java/") || it.startsWith("javax/")) }) continue
                    val javaSupers = if (frontier.isEmpty()) listOf(OBJECT) else frontier.toList()
                    if (javaSupers.any { contract.servesMember(it, r.name, r.desc) }) continue
                    miss(r, "inherited member not served (walked $owner → ${javaSupers.joinToString(" | ")})")
                }
                else -> {}
            }
        }
        return Report(app.size, misses, excluded, board)
    }
}
