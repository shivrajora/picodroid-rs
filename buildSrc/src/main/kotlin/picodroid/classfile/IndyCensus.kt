// SPDX-License-Identifier: GPL-3.0-only
package picodroid.classfile

import org.objectweb.asm.ClassReader
import org.objectweb.asm.ClassVisitor
import org.objectweb.asm.Handle
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes
import org.objectweb.asm.Type

/** One `invokedynamic` site, decoded the way `jvm/src/interpreter/ops_invoke.rs` reads it. */
data class IndyRow(
    val fromClass: String,
    val fromMember: String,
    val sourceFile: String,
    val indyName: String,
    val indyDesc: String,
    val samInterface: String,
    val bsmOwner: String,
    val bsmName: String,
    val bsmDesc: String,
    val bsmRefKind: String,
    val bsmArgCount: Int,
    val implOwner: String,
    val implName: String,
    val implDesc: String,
    val implRefKind: String,
    val implIsInterface: String,
    val instantiatedDesc: String,
    val extraArgs: String,
) {
    val isMetafactory: Boolean
        get() = bsmOwner == "java/lang/invoke/LambdaMetafactory" && (bsmName == "metafactory" || bsmName == "altMetafactory")
}

private fun renderArg(a: Any): String = when (a) {
    is Handle -> "Handle(${refKindName(a.tag)} ${a.owner}.${a.name}${a.desc} itf=${a.isInterface})"
    is Type -> "Type(${a.descriptor})"
    is String -> "\"$a\""
    else -> "${a::class.java.simpleName}($a)"
}

/** Every `invokedynamic` site in the class file at [bytes]. */
fun indy(bytes: ByteArray): List<IndyRow> {
    val rows = ArrayList<IndyRow>()
    var className = "?"
    var sourceFile = ""
    val visitor = object : ClassVisitor(Opcodes.ASM9) {
        override fun visit(version: Int, access: Int, name: String, signature: String?, superName: String?, interfaces: Array<String>?) {
            className = name
        }

        override fun visitSource(source: String?, debug: String?) {
            sourceFile = source ?: ""
        }

        override fun visitMethod(access: Int, name: String, descriptor: String, signature: String?, exceptions: Array<String>?): MethodVisitor {
            val member = name + descriptor
            return object : MethodVisitor(Opcodes.ASM9) {
                override fun visitInvokeDynamicInsn(name: String, descriptor: String, bsm: Handle, vararg args: Any) {
                    val metafactory = bsm.owner == "java/lang/invoke/LambdaMetafactory" &&
                        (bsm.name == "metafactory" || bsm.name == "altMetafactory")
                    val impl = if (metafactory) args.getOrNull(1) as? Handle else null
                    val instantiated = if (metafactory) (args.getOrNull(2) as? Type)?.descriptor else null
                    val extra = if (metafactory) args.drop(3) else args.toList()
                    val sam = Type.getReturnType(descriptor)
                    rows += IndyRow(
                        fromClass = className,
                        fromMember = member,
                        sourceFile = "",
                        indyName = name,
                        indyDesc = descriptor,
                        samInterface = objectInternalName(sam) ?: sam.descriptor,
                        bsmOwner = bsm.owner,
                        bsmName = bsm.name,
                        bsmDesc = bsm.desc,
                        bsmRefKind = "${bsm.tag}:${refKindName(bsm.tag)}",
                        bsmArgCount = args.size,
                        implOwner = impl?.owner ?: "?",
                        implName = impl?.name ?: "?",
                        implDesc = impl?.desc ?: "?",
                        implRefKind = impl?.let { "${it.tag}:${refKindName(it.tag)}" } ?: "?",
                        implIsInterface = impl?.isInterface?.toString() ?: "?",
                        instantiatedDesc = instantiated ?: "?",
                        extraArgs = extra.joinToString(" ") { renderArg(it) },
                    )
                }
            }
        }
    }
    ClassReader(bytes).accept(visitor, ClassReader.SKIP_FRAMES)
    return rows.map { it.copy(sourceFile = sourceFile) }
}
