// SPDX-License-Identifier: GPL-3.0-only
package picodroid.survey

import java.util.TreeMap

/** One surveyed class: where it came from, its bytes, and every analysis over it. */
data class ClassResult(
    val relPath: String,
    val bytes: ByteArray,
    val refs: List<Ref>,
    val census: Census,
    val indy: List<IndyRow>,
)

/** One distinct `(owner, name, desc)` with everything that references it. */
data class Tuple(
    val owner: String,
    val name: String,
    val desc: String,
    val kinds: Set<String>,
    val count: Int,
    val sourceFiles: Set<String>,
    val fromClasses: Set<String>,
) {
    val loadBearing: Boolean get() = kinds.any { it !in NON_LOAD_BEARING_KINDS }
    val pkg: String get() = owner.substringBeforeLast('/', "")
}

private fun cell(v: Any?): String = v.toString().replace('\t', ' ').replace("\n", "\\n")

fun tsv(header: List<String>, rows: List<List<Any?>>): String = buildString {
    append(header.joinToString("\t")).append('\n')
    rows.forEach { r -> append(r.joinToString("\t") { cell(it) }).append('\n') }
}

fun tuples(refs: List<Ref>): List<Tuple> =
    refs.groupBy { Triple(it.owner, it.name, it.desc) }
        .map { (k, rs) ->
            Tuple(
                k.first, k.second, k.third,
                rs.mapTo(sortedSetOf()) { it.kind }, rs.size,
                rs.mapTo(sortedSetOf()) { it.sourceFile }, rs.mapTo(sortedSetOf()) { it.fromClass },
            )
        }
        .sortedWith(compareBy({ it.owner }, { it.name }, { it.desc }))

val REFS_HEADER = listOf("kind", "owner", "name", "desc", "from_class", "from_member", "source_file", "detail")

fun refsTsv(refs: List<Ref>): String =
    tsv(REFS_HEADER, refs.map { listOf(it.kind, it.owner, it.name, it.desc, it.fromClass, it.fromMember, it.sourceFile, it.detail) })

fun tuplesTsv(ts: List<Tuple>): String = tsv(
    listOf("owner", "name", "desc", "kinds", "count", "load_bearing", "source_files", "from_classes"),
    ts.map { listOf(it.owner, it.name, it.desc, it.kinds.joinToString("|"), it.count, it.loadBearing, it.sourceFiles.joinToString("|"), it.fromClasses.joinToString("|")) },
)

/** Attribute names → census column, in column order. */
val ATTR_COLUMNS: List<Pair<String, List<String>>> = listOf(
    "code_bytes" to listOf("Code"),
    "rva_bytes" to listOf("RuntimeVisibleAnnotations"),
    "ria_bytes" to listOf("RuntimeInvisibleAnnotations"),
    "rpa_bytes" to listOf("RuntimeVisibleParameterAnnotations", "RuntimeInvisibleParameterAnnotations"),
    "rta_bytes" to listOf("RuntimeVisibleTypeAnnotations", "RuntimeInvisibleTypeAnnotations"),
    "annotation_default_bytes" to listOf("AnnotationDefault"),
    "signature_bytes" to listOf("Signature"),
    "stackmap_bytes" to listOf("StackMapTable"),
    "lvt_bytes" to listOf("LocalVariableTable"),
    "lvtt_bytes" to listOf("LocalVariableTypeTable"),
    "innerclasses_bytes" to listOf("InnerClasses"),
    "enclosingmethod_bytes" to listOf("EnclosingMethod"),
    "nest_bytes" to listOf("NestHost", "NestMembers"),
    "methodparameters_bytes" to listOf("MethodParameters"),
    "sde_bytes" to listOf("SourceDebugExtension"),
    "linenumber_bytes" to listOf("LineNumberTable"),
    "sourcefile_bytes" to listOf("SourceFile"),
    "exceptions_bytes" to listOf("Exceptions"),
    "bootstrap_bytes" to listOf("BootstrapMethods"),
    "constantvalue_bytes" to listOf("ConstantValue"),
)

/** Attribute columns the strip removes (everything pico-jvm skips by length except what we keep for line numbers). */
val STRIPPED_ATTR_COLUMNS: List<String> = listOf(
    "rva_bytes", "ria_bytes", "rpa_bytes", "rta_bytes", "annotation_default_bytes", "signature_bytes",
    "stackmap_bytes", "lvt_bytes", "lvtt_bytes", "innerclasses_bytes", "enclosingmethod_bytes", "nest_bytes",
    "methodparameters_bytes", "sde_bytes",
)

fun attrColumn(c: Census, column: String): Int =
    ATTR_COLUMNS.first { it.first == column }.second.sumOf { c.attr(it) }

private val KNOWN_ATTRS: Set<String> = ATTR_COLUMNS.flatMap { it.second }.toSet()

fun otherAttrBytes(c: Census): Int = c.attrBytes.filterKeys { it !in KNOWN_ATTRS }.values.sum()

val CENSUS_HEADER: List<String> = listOf(
    "class", "source_file", "major", "minor", "bytes", "cp_count", "access", "super", "interfaces",
    "fields", "methods", "synchronized_methods", "default_methods", "bridge_methods", "synthetic_methods",
    "default_arg_bridges", "has_metadata",
) + ATTR_COLUMNS.map { it.first } + listOf("other_attr_bytes") +
    listOf("cp_tag15", "cp_tag16", "cp_tag17", "cp_tag18", "cp_tag19", "cp_tag20", "cp_class_entries")

fun censusRow(c: Census): List<Any?> = listOf(
    c.className, c.sourceFile, c.major, c.minor, c.bytes, c.cpCount, "0x" + c.access.toString(16), c.superName,
    c.interfaces.joinToString("|"), c.fields, c.methods, c.synchronizedMethods, c.defaultMethods, c.bridgeMethods,
    c.syntheticMethods, c.defaultArgBridges, c.hasMetadata,
) + ATTR_COLUMNS.map { attrColumn(c, it.first) } + listOf(otherAttrBytes(c)) +
    listOf(c.cpTag(15), c.cpTag(16), c.cpTag(17), c.cpTag(18), c.cpTag(19), c.cpTag(20), c.cpClasses.size)

fun censusTsv(cs: List<Census>): String = tsv(CENSUS_HEADER, cs.map { censusRow(it) })

fun cpClassesTsv(cs: List<Census>): String = tsv(
    listOf("from_class", "cp_index", "class_name"),
    cs.flatMap { c -> c.cpClasses.map { (i, n) -> listOf(c.className, i, n) } },
)

val INDY_HEADER = listOf(
    "from_class", "from_member", "source_file", "indy_name", "indy_desc", "sam_interface", "bsm_owner", "bsm_name",
    "bsm_desc", "bsm_ref_kind", "bsm_arg_count", "impl_owner", "impl_name", "impl_desc", "impl_ref_kind",
    "impl_is_interface", "instantiated_desc", "extra_args",
)

fun indyTsv(rows: List<IndyRow>): String = tsv(
    INDY_HEADER,
    rows.map {
        listOf(
            it.fromClass, it.fromMember, it.sourceFile, it.indyName, it.indyDesc, it.samInterface, it.bsmOwner, it.bsmName,
            it.bsmDesc, it.bsmRefKind, it.bsmArgCount, it.implOwner, it.implName, it.implDesc, it.implRefKind,
            it.implIsInterface, it.instantiatedDesc, it.extraArgs,
        )
    },
)

// ---------------------------------------------------------------------------
// Markdown summary
// ---------------------------------------------------------------------------

private fun md(v: Any?): String = v.toString().replace("|", "\\|").replace("\n", " ")

private fun StringBuilder.table(header: List<String>, rows: List<List<Any?>>) {
    append("| ").append(header.joinToString(" | ")).append(" |\n")
    append("|").append(header.joinToString("|") { "---" }).append("|\n")
    rows.forEach { r -> append("| ").append(r.joinToString(" | ") { md(it) }).append(" |\n") }
    append('\n')
}

private fun <K : Comparable<K>> histogram(items: List<K>): List<List<Any?>> =
    items.groupingBy { it }.eachCount().toSortedMap().map { (k, v) -> listOf(k, v) }

fun summaryMarkdown(label: String, results: List<ClassResult>, external: Regex): String = buildString {
    val allRefs = results.flatMap { it.refs }
    val extRefs = allRefs.filter { external.containsMatchIn(it.owner) }
    val ts = tuples(extRefs)
    val censuses = results.map { it.census }
    val indyRows = results.flatMap { it.indy }

    append("# Survey dump: `$label`\n\n")
    append("## Totals\n\n")
    table(
        listOf("metric", "value"),
        listOf(
            listOf("classes", results.size),
            listOf("bytes", censuses.sumOf { it.bytes }),
            listOf("constant-pool entries", censuses.sumOf { it.cpCount }),
            listOf("CONSTANT_Class entries", censuses.sumOf { it.cpClasses.size }),
            listOf("methods", censuses.sumOf { it.methods }),
            listOf("references (all)", allRefs.size),
            listOf("references (external: `${external.pattern}`)", extRefs.size),
            listOf("distinct external tuples", ts.size),
            listOf("distinct external tuples, load-bearing", ts.count { it.loadBearing }),
            listOf("distinct external owners", ts.map { it.owner }.toSet().size),
            listOf("invokedynamic sites", indyRows.size),
        ),
    )

    append("## Red flags\n\n")
    val flags = ArrayList<String>()
    censuses.filter { it.cpTag(17) + it.cpTag(19) + it.cpTag(20) > 0 }
        .forEach { flags += "CP tag 17/19/20 in `${it.className}` (17=${it.cpTag(17)}, 19=${it.cpTag(19)}, 20=${it.cpTag(20)}) — pico-jvm rejects the class at registration" }
    allRefs.filter { it.kind == "ldc_condy" }.forEach { flags += "`ldc` of a dynamic constant in `${it.fromClass}.${it.fromMember}`" }
    indyRows.filter { !it.isMetafactory }
        .forEach { flags += "non-LambdaMetafactory bootstrap `${it.bsmOwner}.${it.bsmName}` in `${it.fromClass}.${it.fromMember}` — ops_invoke.rs would misread arguments[1]" }
    indyRows.filter { it.isMetafactory && it.bsmName == "altMetafactory" }
        .forEach { flags += "`altMetafactory` in `${it.fromClass}.${it.fromMember}` (extra args: ${it.extraArgs})" }
    indyRows.filter { it.implRefKind.startsWith("8:") }
        .forEach { flags += "`REF_newInvokeSpecial` impl handle `${it.implOwner}.<init>${it.implDesc}` in `${it.fromClass}.${it.fromMember}` — would call <init> without `new`" }
    censuses.filter { it.className.endsWith("\$DefaultImpls") }
        .forEach { flags += "`${it.className}` exists — -Xjvm-default=all did not apply" }
    censuses.filter { it.major > 52 }.forEach { flags += "`${it.className}` has class version ${it.major} (> 52)" }
    if (flags.isEmpty()) append("None.\n\n") else flags.forEach { append("- ").append(it).append('\n') }.also { append('\n') }

    append("## External references by owner\n\n")
    val byOwner = ts.groupBy { it.owner }.toSortedMap()
    val byPkg = ts.groupBy { it.pkg }.toSortedMap()
    table(
        listOf("package", "owners", "tuples", "load-bearing tuples", "refs"),
        byPkg.map { (p, xs) -> listOf(p, xs.map { it.owner }.toSet().size, xs.size, xs.count { it.loadBearing }, xs.sumOf { it.count }) },
    )
    table(
        listOf("owner", "tuples", "load-bearing", "kinds", "source files"),
        byOwner.map { (o, xs) ->
            listOf(o, xs.size, xs.count { it.loadBearing }, xs.flatMap { it.kinds }.toSortedSet().joinToString(" "), xs.flatMap { it.sourceFiles }.toSortedSet().joinToString(" "))
        },
    )

    append("## External tuples\n\n")
    append("`load` = load-bearing (needs a class file / dispatch arm in pico-jvm); rows with only ")
    append(NON_LOAD_BEARING_KINDS.sorted().joinToString("/") { "`$it`" }).append(" kinds do not.\n\n")
    byPkg.forEach { (p, xs) ->
        append("### `$p`\n\n")
        table(
            listOf("owner", "name", "desc", "kinds", "load", "count", "source files"),
            xs.map { listOf(it.owner, it.name, it.desc, it.kinds.joinToString(" "), if (it.loadBearing) "yes" else "no", it.count, it.sourceFiles.joinToString(" ")) },
        )
    }

    append("## External tuples by source file\n\n")
    append("`unique` = no other source file references the tuple.\n\n")
    val filesOf = ts.associate { Triple(it.owner, it.name, it.desc) to it.sourceFiles }
    extRefs.groupBy { it.sourceFile }.toSortedMap().forEach { (sf, rs) ->
        append("### `${sf.ifEmpty { "(no SourceFile)" }}`\n\n")
        val local = tuples(rs)
        table(
            listOf("owner", "name", "desc", "kinds", "load", "unique"),
            local.map {
                val u = filesOf[Triple(it.owner, it.name, it.desc)]?.size == 1
                listOf(it.owner, it.name, it.desc, it.kinds.joinToString(" "), if (it.loadBearing) "yes" else "no", if (u) "yes" else "")
            },
        )
    }

    append("## invokedynamic census\n\n")
    table(listOf("bootstrap", "sites"), histogram(indyRows.map { "${it.bsmOwner}.${it.bsmName}" }))
    table(listOf("impl ref_kind", "sites"), histogram(indyRows.map { it.implRefKind }))
    table(listOf("SAM interface", "sites"), histogram(indyRows.map { it.samInterface }))
    table(listOf("impl owner package", "sites"), histogram(indyRows.map { it.implOwner.substringBeforeLast('/', it.implOwner) }))
    table(
        listOf("from", "SAM", "indy name/desc", "bsm", "impl", "impl ref_kind", "instantiated", "extra"),
        indyRows.map {
            listOf(
                "${it.fromClass}.${it.fromMember}", it.samInterface, "${it.indyName}${it.indyDesc}", "${it.bsmOwner}.${it.bsmName}",
                "${it.implOwner}.${it.implName}${it.implDesc}", it.implRefKind, it.instantiatedDesc, it.extraArgs,
            )
        },
    )

    append("## Class census\n\n")
    val attrTotals = TreeMap<String, Int>()
    censuses.forEach { c -> c.attrBytes.forEach { (k, v) -> attrTotals.merge(k, v, Int::plus) } }
    table(listOf("attribute", "bytes (len+6, all occurrences)"), attrTotals.map { (k, v) -> listOf(k, v) })
    table(
        listOf("metric", "total"),
        listOf(
            listOf("classes with @Metadata", censuses.count { it.hasMetadata }),
            listOf("synthetic methods", censuses.sumOf { it.syntheticMethods }),
            listOf("bridge methods", censuses.sumOf { it.bridgeMethods }),
            listOf("\$default bridges", censuses.sumOf { it.defaultArgBridges }),
            listOf("ACC_SYNCHRONIZED methods", censuses.sumOf { it.synchronizedMethods }),
            listOf("interface default methods", censuses.sumOf { it.defaultMethods }),
            listOf("classes named *\$WhenMappings", censuses.count { it.className.endsWith("\$WhenMappings") }),
            listOf("classes named *\$Companion", censuses.count { it.className.endsWith("\$Companion") }),
            listOf("classes named *\$DefaultImpls", censuses.count { it.className.endsWith("\$DefaultImpls") }),
            listOf("anonymous/local classes (*\$<digit>)", censuses.count { Regex("\\$\\d+$").containsMatchIn(it.className) }),
        ),
    )
    table(
        listOf("class", "source", "bytes", "cp", "methods", "sync", "bridge", "synthetic", "\$default", "rva", "stackmap", "lvt", "inner", "sig", "meta"),
        censuses.map {
            listOf(
                it.className, it.sourceFile, it.bytes, it.cpCount, it.methods, it.synchronizedMethods, it.bridgeMethods, it.syntheticMethods,
                it.defaultArgBridges, attrColumn(it, "rva_bytes"), attrColumn(it, "stackmap_bytes"), attrColumn(it, "lvt_bytes"),
                attrColumn(it, "innerclasses_bytes"), attrColumn(it, "signature_bytes"), if (it.hasMetadata) "yes" else "",
            )
        },
    )
}

fun stripStatsTsv(rows: List<Pair<String, StripStats>>): String {
    val body = rows.map { (n, s) -> stripRow(n, s) }
    val total = StripStats(rows.sumOf { it.second.bytesBefore }, rows.sumOf { it.second.bytesAfter }, rows.sumOf { it.second.cpBefore }, rows.sumOf { it.second.cpAfter })
    return tsv(listOf("class", "bytes_before", "bytes_after", "bytes_saved", "pct", "cp_before", "cp_after", "cp_saved"), body + listOf(stripRow("TOTAL", total)))
}

private fun stripRow(name: String, s: StripStats): List<Any?> {
    val saved = s.bytesBefore - s.bytesAfter
    val pct = if (s.bytesBefore == 0) 0.0 else saved * 100.0 / s.bytesBefore
    return listOf(name, s.bytesBefore, s.bytesAfter, saved, "%.1f".format(pct), s.cpBefore, s.cpAfter, s.cpBefore - s.cpAfter)
}

fun stripSummaryMarkdown(label: String, rows: List<Triple<String, Census, StripStats>>): String = buildString {
    append("# Strip prototype: `$label`\n\n")
    val before = rows.sumOf { it.third.bytesBefore }
    val after = rows.sumOf { it.third.bytesAfter }
    val cpBefore = rows.sumOf { it.third.cpBefore }
    val cpAfter = rows.sumOf { it.third.cpAfter }
    table(
        listOf("metric", "before", "after", "saved", "pct"),
        listOf(
            listOf("bytes", before, after, before - after, "%.1f".format(if (before == 0) 0.0 else (before - after) * 100.0 / before)),
            listOf("constant-pool entries", cpBefore, cpAfter, cpBefore - cpAfter, "%.1f".format(if (cpBefore == 0) 0.0 else (cpBefore - cpAfter) * 100.0 / cpBefore)),
        ),
    )
    append("## Where the bytes went\n\n")
    append("Attribute bytes (header included) present before the strip, by column; the remainder of the saving is constant-pool entries only those attributes referenced (e.g. `@Metadata` protobuf strings).\n\n")
    val perCol = STRIPPED_ATTR_COLUMNS.map { col -> col to rows.sumOf { attrColumn(it.second, col) } }
    val attrSum = perCol.sumOf { it.second }
    table(
        listOf("column", "bytes"),
        perCol.map { (c, b) -> listOf(c, b) } + listOf(listOf("(attributes total)", attrSum), listOf("(constant pool + other)", (before - after) - attrSum)),
    )
    append("## Per class\n\n")
    table(
        listOf("class", "before", "after", "saved", "pct", "cp before", "cp after"),
        rows.map { (n, _, s) -> stripRow(n, s).let { listOf(it[0], it[1], it[2], it[3], it[4], it[5], it[6]) } },
    )
}
