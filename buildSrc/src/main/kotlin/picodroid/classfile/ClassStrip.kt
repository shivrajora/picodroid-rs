// SPDX-License-Identifier: GPL-3.0-only
package picodroid.classfile

import org.objectweb.asm.AnnotationVisitor
import org.objectweb.asm.Attribute
import org.objectweb.asm.ClassReader
import org.objectweb.asm.ClassVisitor
import org.objectweb.asm.ClassWriter
import org.objectweb.asm.FieldVisitor
import org.objectweb.asm.Label
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes
import org.objectweb.asm.TypePath

data class StripStats(val bytesBefore: Int, val bytesAfter: Int, val cpBefore: Int, val cpAfter: Int)

/**
 * The Session 2 strip, prototyped: drop every attribute pico-jvm skips by
 * length and rebuild the constant pool without them.
 *
 * Dropped: Runtime{Visible,Invisible}{,Parameter,Type}Annotations (this is
 * where kotlinc's `@Metadata` protobuf strings live), AnnotationDefault,
 * Signature, InnerClasses, EnclosingMethod, NestHost/NestMembers,
 * PermittedSubclasses, MethodParameters, LocalVariable{,Type}Table,
 * StackMapTable, SourceDebugExtension, and any non-standard attribute.
 * Kept: Code (with max_stack/max_locals copied from the reader),
 * LineNumberTable, SourceFile, Exceptions, BootstrapMethods, ConstantValue.
 * [renames] applies `@ShimName` (declaration and shim-internal call sites).
 *
 * `ClassWriter(0)` without a `ClassReader` argument rebuilds the constant pool
 * from what is actually written; with `SKIP_FRAMES` no StackMapTable is read
 * or written. HotSpot would reject the result (no frames at version 52);
 * pico-jvm never verifies and skips the attribute, so this is the intended
 * shape. Never "fix" it with COMPUTE_FRAMES — that needs a class hierarchy the
 * host JVM does not have for the picodroid SDK classes.
 */
fun strip(bytes: ByteArray, renames: Map<MemberKey, String> = emptyMap()): Pair<ByteArray, StripStats> {
    val cr = ClassReader(bytes)
    val cw = ClassWriter(0)
    var owner = ""
    val cv = object : ClassVisitor(Opcodes.ASM9, cw) {
        override fun visit(version: Int, access: Int, name: String, signature: String?, superName: String?, interfaces: Array<String>?) {
            owner = name
            super.visit(version, access, name, null, superName, interfaces)
        }

        override fun visitSource(source: String?, debug: String?) {
            super.visitSource(source, null)
        }

        override fun visitAnnotation(descriptor: String, visible: Boolean): AnnotationVisitor? = null

        override fun visitTypeAnnotation(typeRef: Int, typePath: TypePath?, descriptor: String, visible: Boolean): AnnotationVisitor? = null

        override fun visitInnerClass(name: String, outerName: String?, innerName: String?, access: Int) {}

        override fun visitOuterClass(owner: String, name: String?, descriptor: String?) {}

        override fun visitNestHost(nestHost: String) {}

        override fun visitNestMember(nestMember: String) {}

        override fun visitPermittedSubclass(permittedSubclass: String) {}

        override fun visitAttribute(attribute: Attribute) {}

        override fun visitField(access: Int, name: String, descriptor: String, signature: String?, value: Any?): FieldVisitor =
            object : FieldVisitor(Opcodes.ASM9, super.visitField(access, name, descriptor, null, value)) {
                override fun visitAnnotation(descriptor: String, visible: Boolean): AnnotationVisitor? = null

                override fun visitTypeAnnotation(typeRef: Int, typePath: TypePath?, descriptor: String, visible: Boolean): AnnotationVisitor? = null

                override fun visitAttribute(attribute: Attribute) {}
            }

        override fun visitMethod(access: Int, name: String, descriptor: String, signature: String?, exceptions: Array<String>?): MethodVisitor =
            object : MethodVisitor(Opcodes.ASM9, super.visitMethod(access, renames[MemberKey(owner, name, descriptor)] ?: name, descriptor, null, exceptions)) {
                // `@ShimName` call sites inside the shim follow the declaration rename.
                override fun visitMethodInsn(opcode: Int, owner: String, name: String, descriptor: String, isInterface: Boolean) {
                    super.visitMethodInsn(opcode, owner, renames[MemberKey(owner, name, descriptor)] ?: name, descriptor, isInterface)
                }

                override fun visitAnnotation(descriptor: String, visible: Boolean): AnnotationVisitor? = null

                override fun visitTypeAnnotation(typeRef: Int, typePath: TypePath?, descriptor: String, visible: Boolean): AnnotationVisitor? = null

                override fun visitParameterAnnotation(parameter: Int, descriptor: String, visible: Boolean): AnnotationVisitor? = null

                override fun visitAnnotableParameterCount(parameterCount: Int, visible: Boolean) {}

                override fun visitAnnotationDefault(): AnnotationVisitor? = null

                override fun visitParameter(name: String?, access: Int) {}

                override fun visitLocalVariable(name: String, descriptor: String, signature: String?, start: Label, end: Label, index: Int) {}

                override fun visitLocalVariableAnnotation(
                    typeRef: Int, typePath: TypePath?, start: Array<Label>, end: Array<Label>, index: IntArray,
                    descriptor: String, visible: Boolean,
                ): AnnotationVisitor? = null

                override fun visitInsnAnnotation(typeRef: Int, typePath: TypePath?, descriptor: String, visible: Boolean): AnnotationVisitor? = null

                override fun visitTryCatchAnnotation(typeRef: Int, typePath: TypePath?, descriptor: String, visible: Boolean): AnnotationVisitor? = null

                override fun visitFrame(type: Int, numLocal: Int, local: Array<Any?>?, numStack: Int, stack: Array<Any?>?) {}

                override fun visitAttribute(attribute: Attribute) {}
            }
    }
    cr.accept(cv, ClassReader.SKIP_FRAMES)
    val out = cw.toByteArray()
    return out to StripStats(bytes.size, out.size, cr.itemCount, ClassReader(out).itemCount)
}
