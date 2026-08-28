# kotlin-survey

Session 1 of `docs/designs/kotlin-roadmap-2026-08.md`: an empirical survey of
what kotlinc 2.1.x emits for a picoenvmon-shaped app, so the Kotlin stdlib shim
and the interpreter sessions implement against measured facts. Its output is
`docs/designs/kotlin-shim-inventory.md`.

This is a **standalone Gradle build**. It is not included by the root
`settings.gradle.kts` (the app build must never see the Kotlin Gradle plugin or
the real `kotlin-stdlib`), and it is not a Cargo workspace member. Run it with
the root wrapper, `./gradlew -p tools/kotlin-survey ...`.

## Layout

- `fixture/` — the survey fixture (`src/main/kotlin/survey/*.kt`): an
  Application, two Activities, a Service, data/enum/sealed classes, collections,
  strings, ranges, and `Probes.kt` for deliberately out-of-scope shapes. Compiled
  against the real `kotlin-stdlib` plus the SDK's compiled classes. **Never
  executed** — only its class files are surveyed.
- `hello/` — the `hello-kotlin-on-sim` milestone: one zero-stdlib Kotlin
  `Application` that is hand-packed with `tools/papk-pack` and run on the host
  simulator with zero JVM changes.
- `dump/` — the ASM tool: `ClassRefs.kt` (every reference a class makes),
  `ClassCensus.kt` (raw class-file walk: attribute bytes, CP tags, method
  flags), `IndyCensus.kt` (`invokedynamic` sites), `ClassStrip.kt` (the
  Session 2 strip, prototyped), `Report.kt` (TSV + Markdown), `Main.kt` (CLI).
  The first four are pure functions over `ByteArray` so Session 2 can lift them
  into `buildSrc`.
- `out/` — generated, gitignored.

## Prerequisites

```bash
./gradlew :sdk:compileJava :examples:picoenvmon:compileJava   # compile classpath + Java baseline
```

The first run downloads the Kotlin Gradle plugin, compiler, stdlib and ASM
from Maven Central / the Gradle plugin portal (~150 MB); later runs work with
`--offline`.

## Commands

```bash
./gradlew -p tools/kotlin-survey survey        # everything below except the sim runs
./gradlew -p tools/kotlin-survey dumpRefs      # fixture  -> out/fixture/
./gradlew -p tools/kotlin-survey dumpHello     # hello    -> out/hello/
./gradlew -p tools/kotlin-survey dumpJavaBaseline   # examples/picoenvmon Java classes -> out/picoenvmon-java/
./gradlew -p tools/kotlin-survey stripProto    # strip prototype -> out/strip/{fixture,hello}/
./gradlew -p tools/kotlin-survey dumpStripped  # re-dump the stripped fixture -> out/fixture-stripped/
./gradlew -p tools/kotlin-survey helloPapk helloPapkStripped   # out/hellokt.papk, out/hellokt-stripped.papk
tools/kotlin-survey/hello-sim.sh               # pack + run hello on the sim (scripts/sim.sh --apk)
tools/kotlin-survey/hello-sim.sh --stripped    # same with the ASM-stripped class
```

## Outputs (per `out/<label>/`)

| File | Contents |
|---|---|
| `refs.tsv` | every reference to `kotlin/**`, `kotlinx/**`, `java/**`, `javax/**`: `kind owner name desc from_class from_member source_file detail` |
| `refs-all.tsv` | the same, unfiltered (includes `picodroid/**` — the SDK surface the app touches) |
| `tuples.tsv` | one row per distinct `(owner, name, desc)` with kinds, count, load-bearing flag, source files |
| `classes.tsv` | per-class census: class version, bytes, CP count and tag counts, method flags, bytes per attribute |
| `cp-classes.tsv` | every `CONSTANT_Class` entry (what pico-jvm registration and Session 2's prune see) |
| `indy.tsv` | every `invokedynamic` site: SAM interface, bootstrap, impl handle and its `ref_kind` |
| `summary.md` | totals, red flags, by-owner / by-package / by-source-file tables, indy histograms, census |

`out/strip/<label>/` holds the stripped classes plus `strip-stats.tsv` and
`strip-summary.md`. Every file is deterministic (sorted walks, no timestamps):
two `--rerun-tasks` runs produce byte-identical output.

`kind` is the opcode or attribute the reference sits in. Kinds
`descriptor_only`, `signature_only`, `annotation_type`, `annotation_enum`,
`inner_class`, `enclosing_method`, `exceptions_attr` are **not load-bearing**:
pico-jvm skips those attributes by length and never resolves descriptors to
classes. Everything else needs a class file or a dispatch arm.

## Frozen toolchain

Kotlin/KGP 2.1.21, `jvmTarget = 1.8`, `allWarningsAsErrors`, and the flag list
in `build.gradle.kts` — quoted verbatim in the inventory doc. `kotlin-stdlib`
is never declared: it is whatever KGP resolves for its own version.
