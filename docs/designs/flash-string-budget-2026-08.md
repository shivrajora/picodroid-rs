# Flash string budget — where the bytes go, and what to do about it

> Measured 2026-08-31 against `1438e87` on a `--shrink --release` build of
> `picoenvmon` for `pico_enviro_mon`. Every number below came from reading the
> linked ELF and the class corpora it embeds, not from estimation, unless a row
> is explicitly marked *(projected)*. Reproduction commands are in §7.

## 0. TL;DR

**~10.5% of the firmware image is Java name text.** Class-name shrinking
(`--shrink`, `sdk/shrink-maps/`) covers about a fifth of it. The rest is
untouched: method names, field names, descriptors, `java/**` class names, and
~22 KB of `.class` debug attributes that no firmware code path reads.

The single highest-value change is not a shrinker change at all — it is
**stripping `LineNumberTable` / `StackMapTable` / `SourceFile` from the embedded
class corpora**, which is worth a measured **22.6 KB (2.2% of flash)** with no
API impact and no runtime cost. That matters because `testbench_rp2040` sits at
896,343 bytes against an 896 K program-region ceiling
(`bench/parity/ratchet.toml`, `docs/bugs-rp2040-flash-2026-08-01.md`) — it has
effectively zero headroom today.

*Landed 2026-09-01 — see the status block in §4: 45.7 KB (4.5 %) off the
measured image, −27.3 KB on `testbench_rp2040`, −34.4 KB on `testbench_rp2350`.*

## 1. The build under measurement

```bash
touch picodroid-core/build.rs   # see project_sdk_embed_rerun_gap: build.rs only
                                # re-embeds SDK files it saw at its last run
./scripts/build.sh --board pico_enviro_mon --app picoenvmon --release --shrink
```

```text
text 1009788   data 4   bss 507488
Flash: 1009792 / 2883584 bytes (35% of program region; chip total 4194304)
RAM:   507492 / 532480 bytes (95%)
```

`size` counts `text` as every allocated read-only section, so the embedded PAPK
in `.papk_flash_init` is inside that 1,009,792.

## 2. Section budget

| Section | Bytes | % flash |
|---|---:|---:|
| `.text` (ARM code) | 726,096 | 71.9% |
| `.rodata` | 207,452 | 20.5% |
| &nbsp;&nbsp;├─ embedded SDK `.class` corpus (137 files) | 103,712 | 10.3% |
| &nbsp;&nbsp;└─ Rust literals, tables, consts | 103,740 | 10.3% |
| `.papk_flash_init` (embedded `picoenvmon.papk`) | 72,992 | 7.2% |
| `.data`, `.vector_table`, `.start_block`, `.init_array` | 3,252 | 0.3% |

Half of `.rodata` is not Rust data at all — it is the SDK framework class corpus
that `build_support/papk.rs::embed_framework_classes` inlines via
`include_bytes!`. This is why `.rodata` string scans surface JVM descriptors at
unaligned addresses (`0x100d445f`, `0x100d6ede`, …): those are constant-pool
entries inside embedded `.class` blobs, not Rust `&str` literals.

## 3. Java name text in the image

| Source | Bytes | % flash |
|---|---:|---:|
| SDK corpus UTF-8 constant pool | 46,412 | 4.6% |
| App PAPK UTF-8 constant pool | 32,427 | 3.2% |
| Rust `.rodata` — JVM class names + descriptors | 23,726 | 2.3% |
| Rust `.rodata` — SDK method/field name tables | ~3,500 | 0.3% |
| **Total** | **~106,065** | **10.5%** |

### 3.1 Embedded SDK corpus (103,712 B, 137 classes)

44.8% of the corpus is constant-pool UTF-8 text — **with class names already
shrunk**.

| Role | Entries | Distinct | Bytes | % corpus |
|---|---:|---:|---:|---:|
| descriptor | 855 | 311 | 11,941 | 11.5% |
| unreferenced/other | 366 | 159 | 7,236 | 7.0% |
| member-name (`NameAndType`) | 573 | 335 | 7,105 | 6.9% |
| method-name (`method_info`) | 474 | 391 | 6,432 | 6.2% |
| class-name | 466 | 152 | 5,899 | 5.7% |
| attr-name | 337 | 8 | 3,952 | 3.8% |
| field-name | 167 | 161 | 2,641 | 2.5% |
| string-literal | 61 | 54 | 1,206 | 1.2% |
| **Total** | | | **46,412** | **44.8%** |

By attribute payload (header + body, `Code` shown with its bytecode broken out):

| Attribute | Bytes | % corpus |
|---|---:|---:|
| `Code` | 32,403 | 31.2% |
| &nbsp;&nbsp;└─ of which actual bytecode | 11,051 | 10.7% |
| `LineNumberTable` | 10,448 | 10.1% |
| `StackMapTable` | 2,588 | 2.5% |
| `ConstantValue` | 1,288 | 1.2% |
| `SourceFile` | 1,096 | 1.1% |
| `InnerClasses` | 920 | 0.9% |

Worst individual offenders: `java/lang/Object` appears **109 times** (2,071 B),
`<init>` 101 times (909 B), `LineNumberTable` 103 times (1,854 B of name text
alone), `SourceFile` 137 times (1,781 B), `()V` 104 times, `nativeCreate` 25
times.

### 3.2 Embedded app PAPK (`picoenvmon`, 67,284 B of bytecode, 35 classes)

47.1% of the PAPK is constant-pool UTF-8 text.

| Role | Bytes | % PAPK |
|---|---:|---:|
| descriptor | 10,396 | 15.1% |
| class-name | 7,212 | 10.5% |
| member-name | 6,531 | 9.5% |
| string-literal | 3,021 | 4.4% |
| unreferenced/other | 2,193 | 3.2% |
| attr-name | 1,167 | 1.7% |
| method-name | 979 | 1.4% |
| field-name | 928 | 1.3% |
| **Total** | **32,427** | **47.1%** |

Attribute payloads: `Code` 21,721 (of which 9,981 is bytecode),
`LineNumberTable` 6,032, `StackMapTable` 2,136, `SourceFile` 280.

App class names are **not** shrunk — the map only covers the SDK — so
`picoenvmon/service/SensorLoggerService$LocalBinder` ships in full, four times.

### 3.3 Rust `.rodata` literal tables

Exact non-overlapping accounting against a dictionary harvested from every
`.rs` file in `picodroid-core/`, `platforms/rp/`, `jvm/`, `papk-format/`,
`compat/` plus the generated `OUT_DIR` files:

| Kind | Occurrences | Distinct | Bytes |
|---|---:|---:|---:|
| `java/**` and other class names | 394 | 81 | 7,906 |
| method descriptors | 414 | 108 | 6,397 |
| framework class names (original form) | 170 | 149 | 5,285 |
| framework class names (shrunk, `a/XX`) | 761 | 149 | 2,995 |
| type descriptors | 62 | 3 | 1,143 |
| **Total** | | | **23,726** |

The named blobs behind those numbers:

| Address | Bytes | What it is |
|---|---:|---|
| `0x100bc540` | 2,745 | `unshrink_class` original-name return values (generated `framework_unshrink.rs`) |
| `0x100bb8b4` | 2,240 | `PICODROID_NATIVE_CLASSES` ([class_registry.rs:22](../../picodroid-core/src/native_handler/class_registry.rs#L22)) — 83 entries, all full names |
| `0x100de1b8` | 1,977 | dispatch method-name literals (`<init>`, `nativeSetVisibility`, `nativeRegisterKeyListener`, …) |
| `0x100ba478` | 681 | `BUILTIN_CLASS_NAMES` java/lang exception block ([jvm/src/native/mod.rs:45](../../jvm/src/native/mod.rs#L45)) |
| `0x100bb2ac` | 394 | JVM opcode mnemonics ([jvm/src/types.rs:166](../../jvm/src/types.rs#L166)) |
| `0x100bd548` | 305 | `fire*` upcall method names |
| `0x100bd2f8` | 290 | `picodroid/pio` native method names |
| `0x100b3b6c`, `0x100b3c9c` | 285 | shrunk-name key tables (`a/AM a/EE a/CS …`) |

`PICODROID_NATIVE_CLASSES` (2,272 B of name text) is a strict subset of the
shrink map's original-name column, and LLVM already dedupes the shared literals
— which is why the `unshrink_class` blob is 2,745 B rather than the map's full
4,713 B. Both tables still exist in full-name form in a shrink build, where the
JVM never presents a long name to dispatch.

`BUILTIN_CLASS_NAMES` is 80 entries / 1,910 B, and the builtin dispatch modules
under `jvm/src/native/` add ~865 B of method-name literals.

## 4. Opportunity 1 — strip dead debug attributes (measured, 22.6 KB)

> **Status — landed 2026-09-01** (ASM route, §4.2). Measured on this tree, same
> lockfile, before → after:
>
> | Artefact | Before | After | Saved |
> |---|---:|---:|---:|
> | `picoenvmon` on `pico_enviro_mon`, `--release --shrink`, `Flash:` | 1,015,184 | 969,452 | **45,732 (4.5 %)** |
> | ├─ embedded SDK corpus (shrunk, 145 classes) | 131,150 | 98,511 | 32,639 (24.9 %) |
> | └─ embedded `picoenvmon.papk` (shrink) | 69,140 | 56,048 | 13,092 (18.9 %) |
> | raw SDK tree, `sdk/build/classes/java/main` → `classes-stripped` | 149,015 | 114,731 | 34,284 (23.0 %) |
> | `picoenvmon.papk`, no shrink | 75,495 | 61,734 | 13,761 (18.2 %) |
> | `hellokt.papk` (Kotlin — already stripped; line numbers only) | 3,873 | 3,417 | 456 (11.8 %) |
> | `helloworld.papk` | 911 | 814 | 97 |
> | ratchet: `testbench_rp2040` flash (`helloworld`, release) | 901,223 | 873,902 | **27,321** |
> | ratchet: `testbench_rp2350` flash (`helloworld`, release) | 941,235 | 906,854 | **34,381** |
>
> More than the 22.6 KB + 9.1 KB projected below, because the ASM rewrite also
> drops the attributes Kotlin apps already lost (`InnerClasses`, `Signature`,
> annotations, `NestHost/Members`, `MethodParameters`) with their constant-pool
> entries, and the corpus is 145 classes today, not 137. `testbench_rp2040` now
> has 43,346 B of headroom under its 896 K ceiling (was ~16 KB). RAM unchanged.

**`LineNumberTable` is read only under `#[cfg(debug_assertions)]`**
([jvm/src/class_file/parse.rs:303-304](../../jvm/src/class_file/parse.rs#L303-L304)),
and firmware builds compile with debug-assertions off in *both* profiles
([scripts/lib.sh:341-342](../../scripts/lib.sh#L341-L342)). `StackMapTable` and
`SourceFile` have no reader anywhere in `jvm/` or `picodroid-core/` — picodroid
does not run a bytecode verifier. In this release image all three are parsed
past and discarded.

Measured by actually rewriting the corpora (not projected):

| Corpus | Before | After | Saved |
|---|---:|---:|---:|
| SDK corpus (rp2350, 137 classes) | 103,712 | 89,580 | **14,132 (13.6%)** |
| SDK corpus (rp2040, 128 after excludes) | 95,569 | 82,642 | **12,927 (13.5%)** |
| `picoenvmon.papk` | 67,284 | 58,836 | **8,448 (12.6%)** |

Dropping the attributes leaves their constant-pool `Utf8` entries orphaned
(`"LineNumberTable"`, `"SourceFile"`, `"SensorLoggerService.java"`, …): a further
**6,817 B** in the SDK corpus and **2,272 B** in the PAPK, recoverable only with
a constant-pool compaction pass (§6).

**Total: 22,580 B stripping alone; 31,669 B with CP compaction — 2.2% / 3.1% of
flash.** The SDK half lands on every board and every app; the PAPK half scales
with app size.

### 4.1 The gate

`CARGO_CFG_DEBUG_ASSERTIONS` is the correct build-script gate and it is exact —
verified empirically:

| Build | `CARGO_CFG_DEBUG_ASSERTIONS` | `DEBUG` | `PROFILE` |
|---|---|---|---|
| `cargo build` | set (empty) | `true` | `debug` |
| `cargo build --release` | **absent** | `false` | `release` |
| `cargo build --config profile.dev.debug-assertions=false` | **absent** | `true` | `debug` |

It tracks the `--config` override that `build_firmware` applies, so it matches
the `#[cfg(debug_assertions)]` in the LNT parser exactly. `DEBUG` and `PROFILE`
both get the third row wrong and must not be used.

Consequence: sim builds keep `debug_assertions` on and would keep their line
numbers; only the firmware image loses them, and only where it could not have
read them anyway.

### 4.2 Where it hooks in — as landed (2026-09-01)

Two call sites, because the app PAPK and the SDK corpus are packed by different
toolchains, but **one implementation**: the ASM strip Kotlin apps already went
through (`buildSrc/src/main/kotlin/picodroid/classfile/ClassStrip.kt`,
`strip()`), which gained a `keepLineNumbers` switch. It re-serialises each
class through `ClassWriter(0)` without a source reader, so the constant pool is
rebuilt from what is written — the §6 compaction of the orphaned `Utf8` entries
came for free — and the rest of the strip (annotations, `InnerClasses`,
`Signature`, `NestHost/Members`, …) applies too; pico-jvm reads none of it
(`Code`, `BootstrapMethods`, and `LineNumberTable` under `debug_assertions` are
the only attributes it parses).

- **SDK corpus** — `sdk/build.gradle.kts` registers `:sdk:stripClasses` (a
  `ClassStripTask`, `keepLineNumbers = false`) writing to
  `sdk/build/classes-stripped/java/main`, a separate tree so a sim build's
  Gradle run can never overwrite what a firmware build has `include_bytes!`'d.
  `build_support/papk.rs::embed_framework_classes` runs that task instead of
  `:sdk:compileJava` when `CARGO_CFG_DEBUG_ASSERTIONS` is absent and points the
  unchanged shrink step at its output — `apply_active_shrink` needed no
  restructuring after all, because the strip happens upstream of it. The
  generated `framework_classes.rs` carries `FRAMEWORK_CLASSES_DEBUG_STRIPPED`,
  and `picodroid-core/src/framework_classes.rs` pins it to
  `!cfg!(debug_assertions)` and to the embedded bytes.
- **App PAPK** — `PicodroidPapkPlugin` reads `-Ppicodroid.stripDebug=true`
  (what `scripts/build-apk.sh --strip-debug` passes): Java apps get a
  `stripClassMetadata` stage between `verifyApiContract` and `shrinkClasses` /
  `packPapk`; Kotlin apps' existing `stripClassMetadata` drops line numbers.
  Every device path passes the flag — `lib.sh build_firmware` (so `build.sh`,
  `flash.sh`, CI and the size ratchet), `hil-run.sh`, pre-commit's firmware
  snapshot. Sim paths (`sim.sh`, `sim-run.sh`, `test.sh`) do not, so a
  dev-profile sim run still resolves `(:line)` for app frames and a Java PAPK
  built without the flag is byte-identical to before.

`tools/class-shrink` is untouched: its opaque-tail `ClassFile` rewrites the
stripped output's `Utf8` entries exactly as it did the raw output's (Kotlin
apps have run strip → shrink since Session 2). The Rust walker this section
originally sketched was not needed.

## 5. Opportunity 2 — extend the shrink map

`sdk/keep.toml` records the two deliberate gaps: "v1 shrinks only class names
(methods/fields are deferred to a later release map)" and a `java/**` keep glob
because "Pico-JVM hardcodes these names inside its native handler".

Projected savings across both embedded corpora (arithmetic on the measured
constant pools — *projected*, not built):

| Change | SDK corpus | App PAPK | Total | % flash |
|---|---:|---:|---:|---:|
| Shrink `java/**` class names | 2,598 | 1,618 | 4,216 | 0.42% |
| …and the same names inside descriptors | 3,553 | 2,915 | 6,468 | 0.64% |
| Shrink method + field names (3-char targets) | 8,603 | 4,385 | 12,988 | 1.29% |
| **Total** | **14,754** | **8,918** | **23,672** | **2.34%** |

Plus a Rust-side reduction: `BUILTIN_CLASS_NAMES` (1,910 B) and the builtin
method literals (~865 B) shrink with them.

### 5.1 `java/**` — tractable, and cheaper than it looks

> **Landed 2026-09-01 (map v0.15.0, branch `java-shrink-b-prefix`).** Measured on
> the same `pico_enviro_mon` / `picoenvmon` `--release --shrink` build, main
> (`72dd3bf`) vs. the change: **−10,758 B** (1,015,184 → 1,004,426). SDK corpus
> 131,150 → 122,021 (**−9,129**), `picoenvmon.papk` 69,140 → 64,570
> (**−4,570**), against **+1,920 B** of `.text` (the class-file boundary and the
> translating descriptor compare) and **+1,021 B** of Rust `.rodata` (the 88-entry
> `b/` → original table). `testbench_rp2040 --release --shrink` links at
> 898,230 / 917,248 B. The design differs from the sketch below in one respect:
> the JVM's tables stay in **original** names and `b/` is undone at the
> `ClassFile` accessors (`jvm/src/class_file/names.rs`), so the "Rust-side
> reduction" of §5 does not happen — that is what keeps pre-0.15 PAPKs loading
> and `getName()` correct without a second copy of the names. Descriptors stay
> shrunk on flash; `find_method` and the handful of literal descriptor compares
> go through `desc_eq` / `desc_starts_with`. `cut-release` now harvests the
> `java/**` names the SDK references and takes `--extra-names
> sdk/api-contract.tsv` for the rest. Note the savings are **shrink-only**:
> `bench/parity/ratchet.toml` and the rp2040 flash gate measure no-shrink
> builds, where this change is pure cost — held to **+84 B** (rp2040) /
> **+40 B** (rp2350) because the translating descriptor walk is guarded by
> the table's emptiness and folds away. Re-measured after §4 landed (main
> `20577a2`, the attribute strip): 969,452 → **959,911 B** (−9,541) — the two
> steps compose, with a small overlap where the strip's constant-pool rebuild
> had already dropped orphaned name text (`.rodata` −7,452, PAPK −4,009,
> `.text` +1,920 unchanged).

The keep glob's stated reason no longer fully holds. Since T2.2 the only
`java/**` SDK class files are `java/lang/{Class,Math,System}` and
`java/util/{Arrays,Collections}` — `java/lang/Object`, `String`, `StringBuilder`
and the rest exist only as *names* in `BUILTIN_CLASS_NAMES`
([jvm/src/native/mod.rs:45](../../jvm/src/native/mod.rs#L45)); `BuiltinHandler`
intercepts on method-not-found and `instanceof` walks `BUILTIN_INTERFACES`. So
for almost all of them there is no class file to rewrite — only names in other
classes' constant pools — and a reverse translation at the point those names
are read covers everything.

Costs and traps:

- `java/lang/Object` at 109 occurrences is by far the biggest single win
  (2,071 B in the SDK corpus, 513 B in this one PAPK).
- The shrunk names must not collide with the `a/` namespace the class map
  already owns — allocate `java/**` from a separate prefix (`b/`).
- `Class.getName()` / `getSimpleName()` and every `toString()` that embeds a
  class name must reverse-translate, or user-visible output degrades to `b/z`.
  `canonical_class_name()` at the boundary is the existing pattern
  (`project_classname_dynstring_dangle`).
- Exception class names reaching `catch` clauses resolve by name; the shrink
  must be applied consistently to `BUILTIN_CLASS_NAMES`, `BUILTIN_SUPER`,
  `BUILTIN_INTERFACES` and `throw_net_exception`'s table in one step.

### 5.2 Method/field names — the biggest prize and the hardest

> **Landed 2026-09-01 (map v0.16.0, branch `member-shrink`).** Measured on
> `pico_enviro_mon` / `picoenvmon` `--release --shrink`, main `2fb3d9e` (§4 +
> §5.1 composed) vs. the change: **959,911 → 943,959 B (−15,952)**. Stripped SDK
> corpus 114,731 → 103,864 (**−10,867**), `picoenvmon.papk` (stripped) 60,123 →
> 58,283 (**−1,840**; app-private names stay verbatim), the rest is the
> dispatch/upcall literal blob and `method_tables`-adjacent text leaving
> `.rodata`. `testbench_rp2040` `helloworld --release --shrink` links at
> **872,853 → 859,307 / 917,248 B (−13,546)**, `testbench_rp2350` at
> **900,149 → 886,015 B (−14,134)** — baselines rebuilt from `2fb3d9e` with the
> same toolchain and lockfile. The design differs from the sketch below in every
> obstacle: (1) no X-macro and no `unshrink_method` — `build.rs` generates one
> `const` per SDK method name (`shrink_names::m::setText`, value = the map's
> target under `--shrink`, the original otherwise) from the committed
> `sdk/member-names.tsv`, and every arm/upcall matches through it, so dispatch
> costs nothing and the literal blob disappears (sim `benchmark` TOTAL 1,178 →
> 1,089 ms under `--shrink`); (2) the map is global **by name**, so overrides and
> callers rename in lockstep with no per-class analysis, and the keep set is
> exactly `sdk/api-contract.tsv`'s member column — every `java/**` member the
> runtime matches by literal on any receiver — plus `main`/`injectMembers`;
> (3) `<init>`, `$` synthetics and names ≤ 2 chars stay; (4) the kotlin-shim
> needs no rename: it rides in the PAPK and is rewritten by the same map, its
> names only *reserved* so no target aliases one (`cut-release --reserve`). The
> rewrite is an ASM `ClassRemapper` in `buildSrc` (`ShrinkMembersTask`), not
> `tools/class-shrink` — `ClassWriter(0)` rebuilds the constant pool, which
> dissolves the shared-`Utf8` trap (an enum constant's field name *is* the
> `String` its `<init>` receives; verified on `TimeUnit`). One real cost:
> unlike §5.1, older shrunk PAPKs cannot keep loading (existing members were
> renamed), so `compat::MEMBER_SHRINK_FLOOR = 0.16.0` rejects them with a clear
> error; every workflow rebuilds both sides together. No-shrink images are
> byte-identical (consts fold to the old literals); the ratchet is flat.

12,988 B projected, but four real obstacles:

1. **Rust dispatch matches method names as literals.** Every arm in
   `picodroid-core/src/native_handler/**` and every row in
   `method_tables.rs` is a string literal. Either generate the arms in shrunk
   space (the X-macro Phase 2 in
   [method-level-native-registry.md](method-level-native-registry.md) is exactly
   the right vehicle) or emit an `unshrink_method` companion — but note the
   latter *adds* the original-name column back to `.rodata` and would erase most
   of the win. Generating in shrunk space is the only version that pays.
2. **Overrides must rename in lockstep with their supertype**, including
   supertypes with no class file. `toString`/`equals`/`hashCode` on a picodroid
   class cannot be renamed without renaming `BuiltinHandler`'s matcher for the
   same name — a whole-program consistency requirement the class-only map never
   had.
3. **`<init>` and `<clinit>` must stay verbatim** (JVMS), which is also the
   single largest member-name entry (909 B) and therefore not recoverable.
4. **Kotlin's `kotlin/**` shim** is on the keep list too and calls into SDK
   members; it must be shrunk in the same pass or it breaks.

Do this only after §4 and §5.1 are banked, and only with the X-macro landed.
*(As landed: after §4 and §5.1, without the X-macro — see the status block.)*

## 6. Opportunity 3 — constant-pool compaction (9.1 KB)

Independent of everything above, and worth **6,817 B (SDK) + 2,272 B (PAPK)**
once §4 orphans those entries. It also picks up entries that are already dead
today. This is the expensive one: dropping a CP slot means renumbering every
index in the tail — `Class`, `NameAndType`, all `*ref` entries, `method_info` /
`field_info` name and descriptor indices, and every `ldc` / `getstatic` /
`invoke*` operand inside `Code`. It is real bytecode rewriting, not a filter.

Worth noting that `Code` bytecode is only 11,051 B of the SDK corpus's 32,403 B
`Code` total and 9,981 B of the PAPK's 21,721 B: **most of `Code`'s weight is its
sub-attributes, not instructions.** §4 already collects that.

## 7. Reproducing these numbers

Analysis scripts are in the session scratchpad
(`/tmp/claude-1000/-home-shiv-projects-picodroid-rs/<session>/scratchpad/`);
they are throwaway but worth re-deriving if this doc is picked up later:

| Script | What it does |
|---|---|
| `strings_report.py` | extracts every printable run from allocated ELF sections with addresses, categorises, dumps `strings.json` |
| `jvmnames.py` | exact non-overlapping JVM-name accounting in `.rodata` against a literal dictionary harvested from the `.rs` sources |
| `runs.py` | per-blob `.rodata` breakdown — which contiguous literal run is which source table |
| `classdir_cp.py` / `papk_cp.py` | constant-pool role accounting for a `.class` directory / a `.papk` |
| `attrs.py` | attribute payload sizes and `java/**` name cost per corpus |
| `strip.py` | **rewrites** a corpus with named attributes removed and reports the real delta (the §4 numbers) |

Key gotchas when re-running:

- `touch picodroid-core/build.rs` first, or `build.rs` will not re-embed SDK
  files it did not see at its last run and the sizes will be stale.
- The embedded SDK corpus lives at
  `target/<target>/<profile>/build/picodroid-core-*/out/framework_classes_shrunk`
  — the `x86_64-unknown-linux-gnu` copy under `target/` is the *host* build and
  is not what shipped.
- `.text` string scans are almost entirely false positives: ARM instruction
  bytes that happen to be printable ASCII. Only `.rodata`,
  `.papk_flash_init` and `.data` carry real strings.

## 8. Recommended order

| # | Change | Saving | Risk | Effort |
|---|---|---:|---|---|
| 1 | Strip `LineNumberTable` / `StackMapTable` / `SourceFile` from the SDK corpus, gated on `CARGO_CFG_DEBUG_ASSERTIONS` | 14.1 KB every board | low | **done 2026-09-01** — ASM in `buildSrc`, §4.2 |
| 2 | Same strip in the PAPK packer (`buildSrc`) | 8.4 KB this app, scales | low | **done 2026-09-01** — `--strip-debug`, §4.2 |
| 3 | Emit `PICODROID_NATIVE_CLASSES` in shrunk form from `build.rs` (`shrink_class` already exists) | ~1.9 KB | low | small |
| 4 | Shrink `java/**` class names under a `b/` prefix — **landed 2026-09-01, −10,758 B alone, −9,541 B on top of #1+#2 (§5.1)** | ~10.7 KB | medium — `getName`, `catch`, `instanceof` | map v0.15 + JVM name tables |
| 5 | Match dispatch on shrunk names directly, retire `unshrink_class`'s original column | ~4.7 KB | medium | the `m::` const mechanism from #7 applied to class names (`c::`); no X-macro needed, and it also removes the per-native-call `unshrink_class` match |
| 6 | Constant-pool compaction | ~9.1 KB | medium — bytecode renumbering | the §4 orphans are gone with #1/#2 (ASM rebuilds the pool); only pre-existing dead entries remain |
| 7 | Shrink method/field names — **landed 2026-09-01, −15,952 B on top of #1+#2+#4 (§5.2)** | ~13 KB | high — override consistency, Kotlin shim | map v0.16.0; did not need #5 |

Steps 1–3 are **24.4 KB (2.4% of flash) at low risk** and are independent of
each other. On `testbench_rp2040` — 896,343 bytes against an 896 K ceiling —
that is the difference between "no headroom" and room to re-admit the
`java.util.concurrent` set that `framework_class_excludes` currently drops.

Each step must advance `bench/parity/ratchet.toml` in the same commit, per that
file's header.

## 9. Open questions

- ~~Does anything read `SourceFile` for `pdb` stack traces or the sim's
  exception reporting?~~ Answered 2026-09-01: no reader anywhere in `jvm/`,
  `picodroid-core/`, `platforms/rp/` or `tools/pdb`; `papk-pack`'s class check
  skips attributes by length. Device stack traces already print `pc=N`.
- ~~Is `LineNumberTable` worth keeping in *sim* builds specifically?~~ Kept:
  dev-profile sim builds (`sim.sh`, `test.sh`) embed the raw tree and build
  their PAPKs without `--strip-debug`, so `tracedemo` still prints
  `at tracedemo.TraceDemo.deepest(:39)` there, and the same PAPK built with
  `--strip-debug` and run via `sim.sh --apk` prints `(pc=9)` — both confirmed
  after the change. `--release` sim builds
  (`sim-run.sh` lanes, CI `sim-smoke`) embed the stripped corpus, consistent
  with a JVM whose `pc_to_line` is compiled out.
- Would a shared cross-class constant pool (one dedup'd string table for the
  whole SDK corpus, indices into it) beat per-class shrinking outright?
  `java/lang/Object` ×109 and `()V` ×104 suggest the corpus-wide duplication is
  large, but it changes the on-flash format and every `ClassFile` accessor.
