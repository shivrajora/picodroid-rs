// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.GradleException
import java.io.File

/** Locates a board's `board.toml` the way `scripts/lib.sh resolve_board` does: `platforms/<platform>/boards/<name>/board.toml`. */
object BoardResolver {
    fun boardToml(repoRoot: File, board: String): File {
        val platforms = repoRoot.resolve("platforms")
        val hit = (platforms.listFiles() ?: emptyArray())
            .filter { it.isDirectory }
            .map { it.resolve("boards/$board/board.toml") }
            .firstOrNull { it.isFile }
        if (hit != null) return hit
        val known = (platforms.listFiles() ?: emptyArray())
            .flatMap { p -> (p.resolve("boards").listFiles() ?: emptyArray()).filter { it.resolve("board.toml").isFile } }
            .map { it.name }
            .sorted()
        throw GradleException("picodroid.board=$board: no platforms/<platform>/boards/$board/board.toml — known boards: ${known.joinToString()}")
    }
}
