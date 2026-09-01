// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.InputFile
import org.gradle.api.tasks.Optional
import org.gradle.api.tasks.OutputFile
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import picodroid.classfile.ApiContract
import picodroid.classfile.ClassEntry

/**
 * `verifyApiContract` — runs [ApiContract] over an app's compiled classes
 * (pre-shrink, so names are original; for Kotlin apps the staged+stripped
 * tree, shim classes included) against the generated
 * `sdk/api-contract.tsv`, plus the target board's `framework_class_excludes`
 * when `-Ppicodroid.board=<name>` names one. Fails the build in `error`
 * mode; `warn` prints the report and passes (`-Ppicodroid.apiContract=warn`).
 * Neither the mode nor the board is ever read from the environment: a warm
 * Gradle daemon's environment is frozen (see scripts/build-apk.sh).
 */
abstract class ApiContractTask : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val classesDir: DirectoryProperty

    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val contractFile: RegularFileProperty

    @get:InputFile
    @get:Optional
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val boardToml: RegularFileProperty

    @get:Input
    @get:Optional
    abstract val boardName: Property<String>

    /** `error` or `warn` (`off` is handled by the plugin's `onlyIf`). */
    @get:Input
    abstract val mode: Property<String>

    @get:OutputFile
    abstract val reportFile: RegularFileProperty

    @TaskAction
    fun run() {
        val root = classesDir.get().asFile
        val classes = root.walkTopDown()
            .filter { it.isFile && it.name.endsWith(".class") }
            .map { ClassEntry(it.relativeTo(root).invariantSeparatorsPath, it.readBytes()) }
            .sortedBy { it.relPath }
            .toList()
        if (classes.isEmpty()) {
            throw GradleException("verifyApiContract: no .class files under $root")
        }
        val contractPath = contractFile.get().asFile
        val contract = try {
            ApiContract.parse(contractPath.readText())
        } catch (e: IllegalArgumentException) {
            throw GradleException("${e.message} — regenerate with scripts/gen-api-contract.sh", e)
        }
        if (contract.members.size < 100) {
            throw GradleException(
                "verifyApiContract: $contractPath has only ${contract.members.size} member rows — " +
                    "it is generated; run scripts/gen-api-contract.sh"
            )
        }
        val board = if (boardName.isPresent) {
            val toml = boardToml.get().asFile
            ApiContract.BoardExcludes(boardName.get(), toml.path, ApiContract.parseBoardExcludes(toml.readText()))
        } else null

        val mode = mode.get()
        val report = ApiContract.check(classes, contract, board)
        val text = report.render(mode)
        reportFile.get().asFile.apply { parentFile.mkdirs(); writeText(text) }
        val tagged = "[${project.name}] " + text.trimEnd().replace("\n", "\n[${project.name}] ")
        if (report.ok) {
            logger.info(tagged)
        } else if (mode == "error") {
            logger.error(tagged)
            throw GradleException("api contract failed for ${project.name} — see ${reportFile.get().asFile}")
        } else {
            logger.warn(tagged)
        }
    }
}
