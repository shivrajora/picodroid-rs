# Unconditional shrink — ProGuard semantics for `--shrink`

> Plan, 2026-09-02. Follows [flash-budget-2026-09.md](flash-budget-2026-09.md)
> §4.3 and §6.2, which found ~9.5 KB of original-form names and ~14 KB of
> run-time name translation still in a `--shrink --release` image.

> **Status — landed 2026-09-02** (branch `unconditional-shrink`, map
> v0.17.0, package 0.17.0). As planned, with these differences: the
> descriptor list `sdk/descriptors.tsv` is hand-named (`String_I__Object`,
> `t_aString`, `p_String` for prefixes) rather than derived; `main` and
> `injectMembers` stay verbatim (app entry points invoked by literal from
> Rust — ProGuard keeps `main` too); javac's `$` synthetics are mapped as
> well; the test-only fixture respeller `pico_jvm::names::spelled` lets the
> JVM's hand-assembled class-file fixtures run in the shrink lane; and the
> image check is `scripts/check-shrunk-image.sh` in `pre-commit --full`,
> with `scripts/retrace.sh` / `class-shrink retrace` as the host inverse.
> Measurements are in the closing section.

## 0. Decision

Under `--shrink`, the firmware behaves like a ProGuard build: **no original
SDK or `java/**` class name, member name or descriptor spelling exists
anywhere in the image**, and nothing translates names at run time.
`Class.getName()`, stack traces, `pdb` output and log lines show the mapped
spelling (`a.DK`, `b.AK`, `xy`). Anyone who wants readable names builds
without `--shrink`; anyone who wants to read a shrunk log runs it through a
host-side retrace (§5). No-shrink images stay byte-identical to today.

This replaces two deliberate half-measures from the earlier work:

- the `a/` translators `shrink_class` / `unshrink_class`
  (`picodroid-core/src/shrink_names.rs`, generated `framework_unshrink.rs`),
  which let dispatch arms keep matching original names by translating at
  every native call; and
- the `b/` boundary in `jvm/src/class_file/names.rs`, which undoes `java/**`
  renames as `CONSTANT_Class` entries are read so the JVM's builtin tables
  could stay in original names, plus the `desc_eq` / `desc_starts_with`
  walk that compares descriptors through it.

Both exist only because Rust source spells names one way and the loaded
framework spells them another, and the build mode is known at compile time.
The `m::` mechanism (map v0.16.0) already solved the same problem for method
names with zero run-time cost; this plan extends it to classes, descriptors
and the members the earlier cut kept.

## 1. What is in the image today and why

| What | Bytes | Why it is still there |
|---|---:|---|
| `unshrink_class` 300-arm `match` + `shrink_class` | 12,128 `.text` | translate `a/` at dispatch entry and at Rust-side allocations |
| `unshrink_class` return strings | 2,556 `.rodata` | the original column of the translator |
| `PICODROID_NATIVE_CLASSES` | 2,240 `.rodata` | full names; canonicalised against translated input |
| `b/` → `java/**` table (`JAVA_ORIGINALS` + blob) | 2,049 `.rodata` | undo `b/` at the class-file boundary |
| `names::walk` / `lookup` / `desc_*` | 790 `.text` | descriptor compares through the `b/` table |
| descriptor literals in original form | 1,306 `.rodata` | 90 distinct in `jvm/` + `picodroid-core/` |
| `BUILTIN_CLASS_NAMES`, `BUILTIN_SUPER`, `BUILTIN_INTERFACES`, exception sites | inside the above | 80 distinct `java/**` literals in `jvm/src` |
| kept member names in the SDK corpus (`toString`, `close`, `append`, `equals`, `getName`, `access$NNN`, …) | ~1,500 | `--keep-contract`: every `java/**` member the JVM matches by literal |
| kept member names in the app PAPK | 590 | same keep set |
| **Total on the shrink image** | **~23 KB** | |

## 2. Principle: one spelling per build, chosen by the generator

Every Rust literal that names a Java class, member or descriptor becomes a
generated `const` whose value is the loaded spelling. Three namespaces:

```rust
c::picodroid_view_View          // "picodroid/view/View" or "a/AB"
c::java_lang_String             // "java/lang/String"    or "b/AQ"
m::toString                     // "toString"            or "xy"   (was kept)
d::string_to_void               // "(Ljava/lang/String;)V" or "(Lb/AQ;)V"
```

- **Inputs are committed, map-independent lists**, so a bare `cargo build`
  with no Gradle run and no active map still compiles: `sdk/member-names.tsv`
  (exists), plus new `sdk/class-names.tsv` (every class the SDK declares and
  every `java/**` name the runtime serves — the union `cut-release` already
  computes from the corpus and `--extra-names sdk/api-contract.tsv`) and
  `sdk/descriptors.tsv` (`ident \t descriptor`, hand-named, ~90 rows). Each is
  kept current by a test in the style of `member_names_are_current`.
- **Values come from the active map** when `PICODROID_SHRINK=1`: class
  targets from `[[class]]`, member targets from `[[member]]`, descriptors
  rewritten through `tools/class-shrink/src/descriptor.rs` (already what the
  shrinker applies to class files). Otherwise the original.
- **One generator, two crates.** `jvm/build.rs` already `#[path]`-includes
  `tools/class-shrink/src/{mapping,rename,version}.rs`; move
  `emit_member_names` out of `build_support/papk.rs` into
  `build_support/names.rs` and include it from both `jvm/build.rs` and
  `picodroid-core/build.rs`. Each crate gets its own `c`/`m`/`d` modules from
  the same inputs; a unit test in `picodroid-core` asserts
  `pico_jvm::names::c::java_lang_String == crate::names::c::java_lang_String`
  for every row so the two cannot drift. (Alternative considered: generate
  once in `pico_jvm` and re-export. Rejected — a family-neutral JVM should
  not carry `picodroid/*` names.)
- **Idents are the full internal name with `/` and `$` → `_`.** Verbose but
  unambiguous and greppable; `View` alone would collide the day
  `picodroid/widget/View` exists.
- **Bans are tests, not review.** `no_sdk_method_literals_in_dispatch`
  (`member_names.rs`) grows two siblings that scan the non-test sources of
  `jvm/` and `picodroid-core/` for `"java/…"`, `"picodroid/…"` and
  `"(…)…"` descriptor literals, and a post-build script (§5) greps the ELF.

## 3. What changes, by crate

### 3.1 `picodroid-core` — retire `shrink_class` / `unshrink_class`

- `PICODROID_NATIVE_CLASSES`, `DISPATCH_SITES`, every `match class_name`
  arm in `native_handler/**`, every `shrink_class("…")` allocation site
  (`display.rs`, `net/server_socket.rs`, `pio/peripheral_manager`,
  `threads.rs`, `lifecycle.rs` `MotionEvent`/`KeyEvent`,
  `service_lifecycle.rs`), `throw_net_exception`'s table and
  `CONTRACT_HINTS` / `TOLERATED` move to `c::`.
- The nine `let class_name = unshrink_class(class_name);` entry lines go.
- `framework_unshrink.rs` keeps only `#[cfg(test)] unshrink_class` /
  `shrink_class` for `api_contract.rs`, `method_tables.rs` and
  `shrink_names::unshrink_descriptor`, mirroring `unshrink_member`.
- `framework_class_excludes` (board.toml, original names) already maps
  through the reverse table at build time; unchanged.

Expected: −12.1 KB `.text`, −4.8 KB `.rodata`.

### 3.2 `pico_jvm` — retire the `b/` boundary

- `BUILTIN_CLASS_NAMES`, `BUILTIN_SUPER`, `BUILTIN_INTERFACES`,
  `BUILTIN_METHODS` rows, `intern_string` / `new_object("java/lang/…")`
  sites, `handle_exception`'s class names, `resolve_ldc`, the
  `LambdaMetafactory` / `metafactory` bootstrap check in `ops_invoke.rs`
  and `helpers.rs` (119 literals) move to `c::` / `m::` / `d::`.
- `class_file/names.rs`: delete `JAVA_ORIGINALS`, `unshrink_java*`,
  `lookup`, `walk`; `desc_eq` becomes `a == b`; the accessors in
  `class_file/accessors.rs`, `parse.rs`, `mod.rs` return the stored bytes.
  `jvm/build.rs::emit_java_names` is replaced by the shared generator.
- `canonical_class_name` keys on `c::` values — same pointer-identity
  property, since consts are single statics.

Expected: −2.0 KB `.rodata`, −0.8 KB `.text`, and the `+1,920 B .text` the
§5.1 boundary cost comes back.

### 3.3 Members — un-keep the contract set

- `cut-release` for the next map drops `--keep-contract`. The contract names
  stay in the **reserve** set (no target may alias them) but are now mapped.
  Kept unconditionally: `<init>`, `<clinit>` (JVMS), names ≤ 2 chars, and
  `$`-synthetics (`access$000`, `lambda$…`) only if the ASM remapper cannot
  prove their call sites are all inside the corpus — `ClassRemapper` renames
  declaration and call sites together, so try mapping them and let the
  conformance suites decide.
- `main` and `injectMembers` leave `sdk/keep.toml` and become `m::main` /
  `m::injectMembers` in `boot.rs` / `lifecycle.rs`.
- `jvm/src/native/*.rs`'s 84 distinct builtin arms match on `m::` (the
  module now exists in `pico_jvm`). Implicit upcalls the interpreter makes by
  name — `toString` for string concatenation, `hashCode` / `equals` from the
  `HashMap` natives, `compare` / `compareTo`, `run` / `call`, `iterator` /
  `hasNext` / `next`, `getMessage`, `uncaughtException` — are the same
  arms, covered by the literal ban.
- Kotlin shim: unchanged mechanism. It rides in the PAPK and is rewritten by
  the same global map; its spellings stay reserved.

Expected: ~−1.5 KB SDK corpus, ~−0.6 KB per app PAPK, and the last dispatch
literals out of `.rodata`.

### 3.4 Compatibility — a new floor and a strict equality rule

Two things become impossible that are possible today:

1. **A PAPK shrunk with a map before this cut** spells `toString` where the
   firmware expects `xy`. `compat::MEMBER_SHRINK_FLOOR` → `SHRINK_FLOOR =
   "0.17.0"` (or whatever version cuts the map).
2. **A no-shrink PAPK on shrink firmware** (`framework-map-version 0.0.0`)
   loads today because the JVM's tables were in original names. After this
   change nothing in the firmware recognises `java/lang/String`, so
   `compat::check` must reject it with a build-side message: *"PAPK built
   without --shrink; this firmware was built with --shrink — rebuild the app
   with `--shrink`, or the firmware without."* The reverse case (shrink PAPK
   on no-shrink firmware) is already rejected.

After the cut, the append-only rule keeps working as before: a 0.17 PAPK
loads on 0.18 firmware because every existing entry is copied verbatim.

## 4. What users see

- `Class.getName()` → `"a.DK"`; `getSimpleName()` → `"DK"`. Accepted.
- Uncaught-exception banners, `pdb ps` / stack output, `Log` lines that embed
  a class name, and `JvmError` `Debug` text show mapped names.
- Java `String` literals are never rewritten (no `-adaptclassstrings`), so an
  app that builds a class name from a string and expects to match it — there
  is no `Class.forName`, so this is currently only visible in log text — sees
  the original in its own string and the mapped name from the runtime.
- Nothing changes for apps: `--shrink` already renames every SDK reference in
  the PAPK; app-private names stay as they are (app-class obfuscation under a
  `c/` prefix is a separate, optional follow-up — flash-budget §6.7).

## 5. Tooling that makes this liveable

- **`class-shrink retrace <map> < log`** — the host-side inverse, as
  ProGuard's `retrace`. Both maps are bijections (`a/XX`, `b/XX`, and the
  by-name member targets), so it is a token substitution over `a.XX` /
  `a/XX` / `b.XX` / `b/XX` and `.<target>(`. `hil-run.sh` and `sim-run.sh`
  pipe shrink-lane logs through it before pattern matching, so nightly
  expectations keep reading original names. Java conformance suites
  (`langsuite*`) run no-shrink today and stay that way.
- **`scripts/check-shrunk-image.sh <elf>`** — greps the linked image's
  `.rodata`, `.text` and `.papk_flash_init` for `java/lang/`, `picodroid/`,
  `javax/`, `(L`-descriptors in original form, and the twenty most common
  contract member names; fails on any hit. Runs in `pre-commit --full` next
  to the size ratchet and in CI's size-ratchet job for the shrink build.
- `sdk/shrink-maps/README.md` and `website/…/advanced-config.md` gain the
  ProGuard framing: `--shrink` = obfuscated names, retrace to read them.

## 6. Order of work

| # | Step | Gate | Expected on `picoenvmon --release --shrink` (943,959) |
|---|---|---|---|
| 1 | Shared generator (`build_support/names.rs`), `sdk/class-names.tsv`, `sdk/descriptors.tsv`, currency tests, cross-crate equality test | `test.sh` both lanes | 0 (consts unused yet) |
| 2 | `picodroid-core` to `c::`/`d::`; delete run-time translators | `test.sh`, sim smoke, `check-shrunk-image` shows no `picodroid/` | ≈ −17 KB |
| 3 | `pico_jvm` to `c::`/`d::`; delete `names.rs` table and `desc_eq` walk | same, plus no `java/` in the image | ≈ −5 KB |
| 4 | Cut map (no `--keep-contract`), `m::` in `pico_jvm`, floor + strict rule, `keep.toml` trims | `test.sh`, both conformance suites in the sim, HIL nightly | ≈ −2 KB |
| 5 | `retrace`, image check, harness wiring, docs | `pre-commit --full` | 0 |

Steps 2 and 3 are independent of 4 and land value on their own; 4 is the
compat-breaking one and wants its own release. Each step advances
`bench/parity/ratchet.toml` (no-shrink numbers should not move at all — the
ratchet is the proof that no-shrink stays byte-identical).

## 7. Risks and how each is caught

- **Two crates disagree on a spelling** — the equality test in step 1, run in
  both `test.sh` lanes.
- **A literal survives** — the source-scan tests and the image grep; a miss
  shows up as `NoSuchMethod` / `ClassNotFound` in the shrink lane only, which
  is exactly the failure class the `native miss` log was built for.
- **An implicit JVM upcall by name is missed** (something the interpreter
  invokes on user objects: `toString`, `hashCode`, `equals`, `compare`,
  `run`, `iterator`, `uncaughtException`) — the conformance suites exercise
  all of them; run them once under `--shrink` in step 4 even though they
  stay no-shrink in CI.
- **`invokedynamic` bootstrap names** (`metafactory`, `LambdaMetafactory`,
  `StringConcatFactory` if ever enabled) — ASM's `ClassRemapper` rewrites
  handles; the JVM side must read them through `c::`/`m::`; the
  `lambda_frame` tests cover it.
- **An older device PAPK stops loading** — by design; the floor error names
  the fix.
- **Debuggability** — retrace, and the no-shrink build.

## 8. Measured (2026-09-02, `picoenvmon` on `pico_enviro_mon`, `--release --shrink`)

| | before (`86ebdfe`, map v0.16.0) | after (map v0.17.0) | Δ |
|---|---:|---:|---:|
| `Flash:` | 943,959 | 916,805 | **−27,154 (−2.9 %)** |
| `.text` | 731,184 | 713,760 | −17,424 |
| `.rodata` | 155,740 | 146,280 | −9,460 |
| ├─ embedded SDK corpus (145 classes) | 79,169 | 78,323 | −846 |
| └─ Rust literals, tables, consts | 76,571 | 67,957 | −8,614 |
| `.papk_flash_init` (`picoenvmon.papk` + meta sector) | 54,295 | 54,025 | −270 |
| original `picodroid/**` or `java/**` spellings in `.rodata` | 238 + 89 distinct | **0** | |

`scripts/check-shrunk-image.sh` passes on the image. The sim `benchmark`
app under `--shrink` reads 1,240–1,260 ms against 1,090–1,160 ms
no-shrink in this session's runs (host timing, ±5 %); the no-shrink lane
is unchanged from before. No-shrink firmware is not byte-identical this
time: the §5.1 class-file boundary and the identity translators are gone
from both modes, but two lookup tables replaced tests that cannot see
through `--shrink` (`boxed_dispatch!`'s `ends_with("Value")`, `net_stub`'s
`starts_with("picodroid/net/")`), and `bench/parity/ratchet.toml` moved
+119 B (`testbench_rp2040`) / +203 B (`testbench_rp2350`) for them.

