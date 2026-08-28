// SPDX-License-Identifier: GPL-3.0-only
package picodroid.classfile

import org.objectweb.asm.AnnotationVisitor
import org.objectweb.asm.ClassReader
import org.objectweb.asm.ClassVisitor
import org.objectweb.asm.ConstantDynamic
import org.objectweb.asm.FieldVisitor
import org.objectweb.asm.Handle
import org.objectweb.asm.Label
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes
import org.objectweb.asm.Type
import org.objectweb.asm.TypePath
import org.objectweb.asm.signature.SignatureReader
import org.objectweb.asm.signature.SignatureVisitor

/**
 * One reference from a class file to another class, member, or type.
 *
 * [kind] says how the reference is made (the opcode, or the attribute it sits
 * in); [owner]/[name]/[desc] identify the target; [fromClass]/[fromMember]/
 * [sourceFile] identify the referrer. Pure data: Session 2 lifts this file into
 * buildSrc unchanged, so nothing here touches Gradle or the file system.
 */
data class Ref(
    val kind: String,
    val owner: String,
    val name: String,
    val desc: String,
    val fromClass: String,
    val fromMember: String,
    val sourceFile: String,
    val detail: String,
)

/**
 * Kinds that never require a class file in pico-jvm: the attribute they live in
 * is skipped by length (`jvm/src/class_file/parse.rs`), or the descriptor is
 * never resolved to a class. Everything else is load-bearing.
 */
val NON_LOAD_BEARING_KINDS: Set<String> = setOf(
    "descriptor_only", "signature_only", "annotation_type", "annotation_enum",
    "inner_class", "enclosing_method", "exceptions_attr",
)

fun refKindName(tag: Int): String = when (tag) {
    Opcodes.H_GETFIELD -> "getField"
    Opcodes.H_GETSTATIC -> "getStatic"
    Opcodes.H_PUTFIELD -> "putField"
    Opcodes.H_PUTSTATIC -> "putStatic"
    Opcodes.H_INVOKEVIRTUAL -> "invokeVirtual"
    Opcodes.H_INVOKESTATIC -> "invokeStatic"
    Opcodes.H_INVOKESPECIAL -> "invokeSpecial"
    Opcodes.H_NEWINVOKESPECIAL -> "newInvokeSpecial"
    Opcodes.H_INVOKEINTERFACE -> "invokeInterface"
    else -> "ref_kind_$tag"
}

private fun opName(opcode: Int): String = when (opcode) {
    Opcodes.INVOKEVIRTUAL -> "invokevirtual"
    Opcodes.INVOKESPECIAL -> "invokespecial"
    Opcodes.INVOKESTATIC -> "invokestatic"
    Opcodes.INVOKEINTERFACE -> "invokeinterface"
    Opcodes.GETSTATIC -> "getstatic"
    Opcodes.PUTSTATIC -> "putstatic"
    Opcodes.GETFIELD -> "getfield"
    Opcodes.PUTFIELD -> "putfield"
    Opcodes.NEW -> "new"
    Opcodes.ANEWARRAY -> "anewarray"
    Opcodes.CHECKCAST -> "checkcast"
    Opcodes.INSTANCEOF -> "instanceof"
    else -> "op_$opcode"
}

/** Internal name of the object type behind [t] (arrays unwrapped), or null for primitives. */
fun objectInternalName(t: Type): String? = when (t.sort) {
    Type.OBJECT -> t.internalName
    Type.ARRAY -> objectInternalName(t.elementType)
    else -> null
}

private const val CLASS_MEMBER = "<class>"

/** Every reference the class file at [bytes] makes, in visitation order. */
fun extract(bytes: ByteArray): List<Ref> {
    val rows = ArrayList<Ref>()
    var className = "?"
    var sourceFile = ""

    fun emit(kind: String, owner: String, name: String, desc: String, member: String, detail: String) {
        rows += Ref(kind, owner, name, desc, className, member, "", detail)
    }

    fun annotationVisitor(member: String): AnnotationVisitor = object : AnnotationVisitor(Opcodes.ASM9) {
        override fun visitEnum(name: String?, descriptor: String, value: String) {
            emit("annotation_enum", Type.getType(descriptor).internalName, value, descriptor, member, "")
        }

        override fun visitAnnotation(name: String?, descriptor: String): AnnotationVisitor {
            emit("annotation_type", Type.getType(descriptor).internalName, "", descriptor, member, "nested")
            return this
        }

        override fun visitArray(name: String?): AnnotationVisitor = this
    }

    fun annotation(descriptor: String, visible: Boolean, member: String, where: String): AnnotationVisitor {
        emit("annotation_type", Type.getType(descriptor).internalName, "", descriptor, member, "visible=$visible $where")
        return annotationVisitor(member)
    }

    fun descriptorOnly(types: Iterable<Type>, member: String) {
        types.mapNotNullTo(LinkedHashSet()) { objectInternalName(it) }
            .forEach { emit("descriptor_only", it, "", "", member, "") }
    }

    fun signatureOnly(signature: String?, member: String, typeOnly: Boolean) {
        if (signature == null) return
        val seen = LinkedHashSet<String>()
        val sv = object : SignatureVisitor(Opcodes.ASM9) {
            override fun visitClassType(name: String) {
                seen += name
            }

            override fun visitInnerClassType(name: String) {
                seen += "\$$name"
            }
        }
        if (typeOnly) SignatureReader(signature).acceptType(sv) else SignatureReader(signature).accept(sv)
        seen.forEach { emit("signature_only", it, "", "", member, signature) }
    }

    val visitor = object : ClassVisitor(Opcodes.ASM9) {
        override fun visit(
            version: Int, access: Int, name: String, signature: String?, superName: String?, interfaces: Array<String>?,
        ) {
            className = name
            if (superName != null) emit("super", superName, "", "", CLASS_MEMBER, "")
            interfaces?.forEach { emit("interface", it, "", "", CLASS_MEMBER, "") }
            signatureOnly(signature, CLASS_MEMBER, typeOnly = false)
        }

        override fun visitSource(source: String?, debug: String?) {
            sourceFile = source ?: ""
        }

        override fun visitAnnotation(descriptor: String, visible: Boolean): AnnotationVisitor =
            annotation(descriptor, visible, CLASS_MEMBER, "class")

        override fun visitTypeAnnotation(typeRef: Int, typePath: TypePath?, descriptor: String, visible: Boolean) =
            annotation(descriptor, visible, CLASS_MEMBER, "class_type")

        override fun visitInnerClass(name: String, outerName: String?, innerName: String?, access: Int) {
            emit("inner_class", name, innerName ?: "", outerName ?: "", CLASS_MEMBER, "access=0x${access.toString(16)}")
        }

        override fun visitOuterClass(owner: String, name: String?, descriptor: String?) {
            emit("enclosing_method", owner, name ?: "", descriptor ?: "", CLASS_MEMBER, "")
        }

        override fun visitField(access: Int, name: String, descriptor: String, signature: String?, value: Any?): FieldVisitor {
            val member = "$name:$descriptor"
            descriptorOnly(listOf(Type.getType(descriptor)), member)
            signatureOnly(signature, member, typeOnly = true)
            return object : FieldVisitor(Opcodes.ASM9) {
                override fun visitAnnotation(descriptor: String, visible: Boolean) =
                    annotation(descriptor, visible, member, "field")

                override fun visitTypeAnnotation(typeRef: Int, typePath: TypePath?, descriptor: String, visible: Boolean) =
                    annotation(descriptor, visible, member, "field_type")
            }
        }

        override fun visitMethod(
            access: Int, name: String, descriptor: String, signature: String?, exceptions: Array<String>?,
        ): MethodVisitor {
            val member = name + descriptor
            val mt = Type.getMethodType(descriptor)
            descriptorOnly(mt.argumentTypes.toList() + mt.returnType, member)
            signatureOnly(signature, member, typeOnly = false)
            exceptions?.forEach { emit("exceptions_attr", it, "", "", member, "") }
            return object : MethodVisitor(Opcodes.ASM9) {
                override fun visitAnnotation(descriptor: String, visible: Boolean) =
                    annotation(descriptor, visible, member, "method")

                override fun visitTypeAnnotation(typeRef: Int, typePath: TypePath?, descriptor: String, visible: Boolean) =
                    annotation(descriptor, visible, member, "method_type")

                override fun visitParameterAnnotation(parameter: Int, descriptor: String, visible: Boolean) =
                    annotation(descriptor, visible, member, "param=$parameter")

                override fun visitAnnotationDefault(): AnnotationVisitor = annotationVisitor(member)

                override fun visitInsnAnnotation(typeRef: Int, typePath: TypePath?, descriptor: String, visible: Boolean) =
                    annotation(descriptor, visible, member, "insn")

                override fun visitTryCatchAnnotation(typeRef: Int, typePath: TypePath?, descriptor: String, visible: Boolean) =
                    annotation(descriptor, visible, member, "trycatch")

                override fun visitLocalVariableAnnotation(
                    typeRef: Int, typePath: TypePath?, start: Array<Label>, end: Array<Label>, index: IntArray,
                    descriptor: String, visible: Boolean,
                ) = annotation(descriptor, visible, member, "localvar")

                override fun visitMethodInsn(opcode: Int, owner: String, name: String, descriptor: String, isInterface: Boolean) {
                    emit(opName(opcode), owner, name, descriptor, member, "itf=$isInterface")
                }

                override fun visitFieldInsn(opcode: Int, owner: String, name: String, descriptor: String) {
                    emit(opName(opcode), owner, name, descriptor, member, "")
                }

                override fun visitTypeInsn(opcode: Int, type: String) {
                    emit(opName(opcode), type, "", "", member, "")
                }

                override fun visitMultiANewArrayInsn(descriptor: String, numDimensions: Int) {
                    val t = Type.getType(descriptor)
                    emit("multianewarray", objectInternalName(t) ?: descriptor, "", descriptor, member, "dims=$numDimensions")
                }

                override fun visitLdcInsn(value: Any) {
                    when (value) {
                        is Type -> when (value.sort) {
                            Type.METHOD -> emit("ldc_methodtype", value.descriptor, "", value.descriptor, member, "")
                            Type.OBJECT -> emit("ldc_class", value.internalName, "", "", member, "")
                            else -> emit("ldc_class", value.descriptor, "", "", member, "array")
                        }
                        is Handle -> emit(
                            "ldc_handle", value.owner, value.name, value.desc, member,
                            "ref_kind=${value.tag} ${refKindName(value.tag)} itf=${value.isInterface}",
                        )
                        is ConstantDynamic -> emit(
                            "ldc_condy", value.bootstrapMethod.owner, value.name, value.descriptor, member,
                            "bsm=${value.bootstrapMethod.name}",
                        )
                        else -> {}
                    }
                }

                override fun visitInvokeDynamicInsn(name: String, descriptor: String, bsm: Handle, vararg args: Any) {
                    emit("indy_bsm", bsm.owner, bsm.name, bsm.desc, member, "ref_kind=${bsm.tag} ${refKindName(bsm.tag)} args=${args.size}")
                    val sam = Type.getReturnType(descriptor)
                    emit("indy_sam", objectInternalName(sam) ?: sam.descriptor, name, descriptor, member, "")
                    val metafactory = bsm.owner == "java/lang/invoke/LambdaMetafactory" &&
                        (bsm.name == "metafactory" || bsm.name == "altMetafactory")
                    args.forEachIndexed { i, a ->
                        if (a is Handle) {
                            val kind = if (metafactory && i == 1) "indy_impl" else "indy_arg_handle"
                            emit(kind, a.owner, a.name, a.desc, member, "arg=$i ref_kind=${a.tag} ${refKindName(a.tag)} itf=${a.isInterface}")
                        } else if (a is Type && !(metafactory && (i == 0 || i == 2))) {
                            // Marker interfaces etc. in altMetafactory's extra args.
                            objectInternalName(a)?.let { emit("indy_arg_type", it, "", a.descriptor, member, "arg=$i") }
                        }
                    }
                }

                override fun visitTryCatchBlock(start: Label, end: Label, handler: Label, type: String?) {
                    if (type != null) emit("catch_type", type, "", "", member, "")
                }
            }
        }
    }
    ClassReader(bytes).accept(visitor, ClassReader.SKIP_FRAMES)
    return rows.map { it.copy(sourceFile = sourceFile) }
}
