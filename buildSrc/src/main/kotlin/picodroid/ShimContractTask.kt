// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.InputFile
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.OutputFile
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import picodroid.classfile.ApiContract
import picodroid.classfile.ClassEntry
import picodroid.classfile.ShimContract
import java.io.File

/**
 * `:kotlin-shim:contractCheck` — runs [ShimContract] over the compiled shim
 * and the fixture apps' compiled classes, resolving `java/…` supertypes
 * through the generated API contract (`sdk/api-contract.tsv`); fails the
 * build on a Direction-A miss, a rejected owner, or (when [strictUnused]) an
 * unused shim member.
 */
abstract class ShimContractTask : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val shimClasses: DirectoryProperty

    /** Class directories of the fixture apps (compileKotlin + compileJava outputs). */
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val fixtureClasses: ConfigurableFileCollection

    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val contractFile: RegularFileProperty

    @get:Input
    abstract val strictUnused: Property<Boolean>

    @get:OutputFile
    abstract val reportFile: RegularFileProperty

    private fun classesUnder(root: File): List<ClassEntry> =
        root.walkTopDown()
            .filter { it.isFile && it.name.endsWith(".class") }
            .map { ClassEntry(it.relativeTo(root).invariantSeparatorsPath, it.readBytes()) }
            .sortedBy { it.relPath }
            .toList()

    @TaskAction
    fun run() {
        val shim = classesUnder(shimClasses.get().asFile)
        val fixtures = fixtureClasses.files.filter { it.isDirectory }.flatMap { classesUnder(it) }
        val contract = ApiContract.parse(contractFile.get().asFile.readText())
        val report = ShimContract.check(shim, fixtures, contract, ShimContract.ContractOptions(strictUnused.get()))
        val text = report.render()
        reportFile.get().asFile.apply { parentFile.mkdirs(); writeText(text) }
        logger.lifecycle(text.trimEnd())
        if (!report.ok) {
            throw GradleException("kotlin-shim contract failed — see ${reportFile.get().asFile}")
        }
    }
}
