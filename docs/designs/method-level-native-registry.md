# Design: Method-level native-registry cross-check (audit P1-6)

> Produced 2026-07-25 by the audit fix session's design panel (4 parallel
> design agents, each adversarially critiqued against source; the critique's
> verdict and amendments are at the bottom and OVERRIDE the design body where
> they conflict). Execute from this doc; update it if reality diverges.

## Status (added 2026-08-31 — this doc had none, and read as unexecuted)

**Phase 1: DONE.** `picodroid-core/src/native_handler/method_tables.rs` holds the
per-handler `(class, method, descriptor)` const tables plus the
bidirectional test — direction A (every SDK `ACC_NATIVE` method is handled or
allowlisted) and direction B (every table row matches a real SDK declaration).
The failure message prints ready-to-paste rows, which is the intended migration
workflow and works: adding `ListView.nativeBindAdapter` during the E2 work
produced exactly one paste-ready row. Wired via the `#[cfg(test)] #[path]`
include in `main.rs`, as §0 requires.

**Phase 2 (the X-macro): OPEN.** There is no
`picodroid-core/src/native_handler/tables/` directory and no
`native_table_macros.rs` anywhere in the tree; handlers still hand-match. The
goal — making drift structurally impossible rather than test-enforced — stands.

## DESIGN
# Method-Level Native-Registry Cross-Check — Design (quality-roadmap.md Stage 2)

## 0. Facts the design rests on (verified in-tree)

- **Dispatch key is `(unshrunk_class_name, method_name)` — never the descriptor.** Every handler calls `crate::shrink_names::unshrink_class(class_name)` at entry, then matches string literals. `ctx.descriptor` exists on `NativeContext` but no platform handler reads it; overloads are disambiguated inside one arm by inspecting args (`Activity.setResult` ×2 descriptors, `Arrays.sort` ×7 by `atype`).
- **Shrink v1 renames classes only** (`sdk/keep.toml`: "v1 shrinks only class names"). Method names in tables need no un-shrinking. **But descriptors read from loaded class files DO embed shrunk class names** (`(La/B;)V`), so the test must un-shrink descriptors — a gotcha the class-level test never hit.
- **`system::native_handler` is `#[cfg(not(test))]`** (`platforms/rp/src/system/mod.rs:8`). Any const the host test consumes must live in a hardware-free file re-included via `#[cfg(test)] #[path = ...]` in `main.rs` — the established pattern (`state.rs`, `class_registry.rs`, `listener_map.rs`; see `main.rs:60-89`).
- **Enumerating SDK natives is already solved machinery**: `every_native_class_is_registered` (class_registry.rs:227) iterates `crate::framework_classes::FRAMEWORK_CLASSES`, `ClassFile::parse`, filters `m.access_flags & ACC_NATIVE`, un-shrinks via `unshrink_class`. Method name/descriptor come from `cf.cp_utf8(m.name_index)` / `cf.cp_utf8(m.descriptor_index)` (`jvm/src/class_file/mod.rs:49-52`, accessors.rs:44).
- **`java/*` natives are small**: only `sdk/java/java/{lang,util}` classes that *have* class files matter for the diff — `Arrays` (29), `Math` (27), `System` (2: `currentTimeMillis` handled in platform os.rs, `arraycopy` in jvm arrays.rs), `Class.getName` (1). `Object/String/StringBuilder/HashMap/...` have **no class files** — BuiltinHandler intercepts method-not-found — so they are outside the ACC_NATIVE diff by construction.
- **Precedent for "declare once, index at runtime, iterate in test"**: `picodroid-core/src/dispatch_sites.rs` (`DISPATCH_SITES` + `every_site_resolves_under_active_shrink_map`).
- **Compat aliases exist**: `lvgl_backend.rs:51-56` accepts `"nativeSetVisibility" | "setVisibility"` (pre-rename PAPKs). The alias is *not* an SDK native today — it must be matchable but must not appear in the diffed table.
- **`net_stub.rs` is a catch-all** (`Some(Err(UnsupportedOperationException))` for all `picodroid/net/*`), so on `not(has_network)` boards there is no silent-miss surface for net regardless of the table.

## 1. (a) Table shape and the adjacency mechanism

### Evaluated options

| Option | Drift table↔SDK | Drift table↔match-arm | Churn | Verdict |
|---|---|---|---|---|
| Doc convention ("keep table next to arms") | detected | **possible** | none | reject — this is the status quo failure mode |
| Parallel const table + test | detected (both dirs) | **possible** (row added to appease test, arm forgotten/typoed → green build, runtime NoSuchMethod) | 1 new file, zero handler edits | **Phase 1 scaffolding only** |
| `build.rs` regex-parse of handler sources → generated table | detected | detected (fragile) | build machinery | reject — cannot see through graphics trait indirection / wildcard arms |
| Fn-pointer table dispatch (BUILTIN_DISPATCH style, per-method) | impossible | impossible | one uniform-signature fn per arm (~240), breaks `&mut self` arms, linear-scan cost on the hot graphics path | reject |
| **X-macro: match arms generated FROM the entry list; the same list emits the const table** | impossible | **impossible** (arm and row are one entry) | entry lists move to per-handler table files; bodies stay one-line delegations | **End state (Phase 2)** |

**Recommendation: land the parallel table + test first (Phase 1 — pure addition, immediately kills the silent surface in both directions), then converge handlers onto the X-macro (Phase 2) so table↔arm drift becomes structurally impossible. The test is identical in both phases, so Phase 1 is not throwaway: Phase 2 merely swaps a hand-written const for a macro-emitted one.**

### Row shape

```rust
/// (declaring_class, method_name, jvm_descriptor) — original (un-shrunk) names.
/// One row per SDK overload; several rows may share one match arm.
pub const IO_HANDLED: &[(&str, &str, &str)] = &[
    ("picodroid/io/File", "exists", "()Z"),
    ("picodroid/io/File", "renameTo", "(Lpicodroid/io/File;)Z"),
    ("picodroid/io/FileInputStream", "read", "([BII)I"),
    // ...
];
```

Flat `(class, method, descriptor)` tuples (matching `API_HINTS`' existing 3-tuple style) rather than a nested descriptor list — repeated rows per overload keep the diff set-shaped and the paste-from-test-failure workflow (see §2) trivial.

Aggregation avoids const-slice concat pain:

```rust
pub const ALL_HANDLED: &[&[(&str, &str, &str)]] =
    &[PIO_HANDLED, OS_HANDLED, CONCURRENT_HANDLED, IO_HANDLED, NET_HANDLED,
      SENSORS_HANDLED, APP_SERVICES_HANDLED, CORE_HANDLED /* mod.rs arms */,
      GRAPHICS_HANDLED /* or per-widget consts listed individually */];
```

**Rows are keyed by the DECLARING class**, exactly as the SDK class files see them:
- wildcard receiver arms (`(_, "startActivity")` in mod.rs, method-name-only matches in app_services.rs) → rows under `picodroid/app/Activity`, `picodroid/content/Context`, `picodroid/app/Service` (verified: those .java files declare these natives);
- inherited-View routing (`is_view`/`is_view_group` in graphics/mod.rs) → rows under `picodroid/view/View` / `picodroid/view/ViewGroup` only. No inheritance modeling needed anywhere: the SDK side also enumerates by declaring class.
- The test deliberately does NOT model wildcard *swallowing* (a future `picodroid/foo/Bar.finish` native would be claimed at runtime by the `(_, "finish")` arm, but Direction A still fails until a row is consciously added — which is the desired forcing function).

### Phase 2: the X-macro (single declaration → match + table)

Per handler, the entry list moves to a **hardware-free sibling table file** containing only a `macro_rules!` definition (no `use`, no HAL — bodies are inert token trees there). Example `platforms/rp/src/system/native_handler/tables/io_table.rs`:

```rust
// Hardware-free: only a macro definition. Bodies are token trees; they are
// type-checked ONLY where a dispatch emitter expands them (io.rs), never in
// the test build, which expands the table emitter and discards them.
macro_rules! io_native_methods {
    ($emit:path) => { $emit! {
        // idents passed explicitly so body tokens and binder tokens share
        // this macro's hygiene context (macro_rules locals are def-site).
        args = (class_name, method_name, ctx);
        ("picodroid/io/File", "exists", ["()Z"]) =>
            file_bool(ctx, backend::exists),
        ("picodroid/io/File", "renameTo", ["(Lpicodroid/io/File;)Z"]) =>
            file_rename_to(ctx),
        ("picodroid/io/FileOutputStream", "flush", ["()V"]) => Ok(None),
        // ...
    }};
}
```

`io.rs` becomes:

```rust
include!("tables/io_table.rs");
io_native_methods!(picodroid_core::emit_native_dispatch);
// helper fns (file_bool, fis_read, backend, ...) unchanged below
```

expanding to exactly today's function:

```rust
pub fn dispatch(class_name: &str, method_name: &str, ctx: &mut NativeContext<'_>)
    -> Option<Result<Option<Value>, JvmError>> {
    let class_name = crate::shrink_names::unshrink_class(class_name);
    match (class_name, method_name) {
        ("picodroid/io/File", "exists") => Some(file_bool(ctx, backend::exists)),
        // ... one arm per entry; N descriptor rows collapse to one arm
        _ => None,
    }
}
```

The test side (`method_tables.rs`, §2) expands the *other* emitter:

```rust
include!("tables/io_table.rs");
io_native_methods!(picodroid_core::emit_handled_rows); // → pub const IO_HANDLED: &[(&str,&str,&str)]
```

**Emitters** live in `picodroid-core/src/native_table_macros.rs` (`#[macro_export]`; picodroid-core is already the shared home of `shrink_names`/`framework_classes`, and platforms/esp inherits them for free):

```rust
#[macro_export] macro_rules! emit_native_dispatch { /* ~40 lines */ }
#[macro_export] macro_rules! emit_handled_rows    { /* ~20 lines */ }
```

Entry grammar (one grammar, four features):

```text
( [@any_receiver] $class:literal , $method:literal , [ $($desc:literal),+ ]
  [ , aliases: [ $($alias:literal),+ ] ] ) => $body:expr ,
```

- `@any_receiver` → emitted pattern is `(_, $method [| $alias]*)`; table rows still use `$class` (the declaring class). Covers mod.rs's `startActivity/…/finish/getIntent` and all of app_services.
- `aliases:` → aliases join the match pattern but produce **no table rows** (Direction B stays clean for `setVisibility`/`setEnabled`/`setAlpha`).
- multiple descriptors → one arm, N rows (`setResult`, `Arrays.sort`).
- Bodies are normalized to `Result<Option<Value>, JvmError>` expressions; the emitter wraps `Some(...)`. (Mechanical: today's arms are almost all already `Some(one_helper_call(...))`; `os.rs` `Thread.start`'s 100-line body gets extracted to a named fn first.)
- Handlers needing state (`app_services::dispatch(handler: &mut PicodroidNativeHandler, ...)`, mod.rs's `self` arms) use a `state = (handler: &mut PicodroidNativeHandler);` header line; the emitter threads the extra param. mod.rs's trait method shrinks to un-shrink + `pio::dispatch(...)` chain + `core_native_methods!`-generated `dispatch_core(self, class_name, method_name, ctx)` + the miss-hint fallthrough.

**Hygiene note (the one sharp edge):** `macro_rules!` locals are definition-site-hygienic, so the binder idents (`class_name`, `method_name`, `ctx`, `handler`) must be *written in the table file* and passed to the emitter via the `args = (...)` header; the emitter uses the captured `$ident`s as the generated fn's parameter names. Item paths in bodies (`backend::exists`, `widgets::text_view_set_text`) resolve at the expansion site (the handler module) — standard mixed-site behavior, works.

**Graphics** (`lvgl_backend.rs`, ~140 arms across 30 trait methods): one hardware-free `graphics/tables/widget_tables.rs` holding per-widget macros with a class header:

```rust
macro_rules! text_view_native_methods {
    ($emit:path) => { $emit! {
        class = "picodroid/widget/TextView";
        args = (method, ctx);
        ("nativeCreate", ["()I"]) => widgets::text_view_native_create(),
        ("setText", ["(ILjava/lang/String;)V"]) =>
            widgets::text_view_set_text(ctx.args, ctx.strings, ctx.objects),
        // ...
    }};
}
```

A third emitter `emit_method_match` generates a method-name-only `match` used as each trait-method body in `LvglBackend` (trait `GraphicsBackend` and the class-routing/precedence logic in `graphics/mod.rs` — class-specific → ViewGroup → View — stay hand-written and untouched). `emit_handled_rows` prefixes `class = ...` onto each row. `GRAPHICS_HANDLED` is the per-widget consts listed in `ALL_HANDLED`.

**Flash cost: zero.** The emitted consts are referenced only from `#[cfg(test)]` code; unreferenced consts emit no data into firmware. Corollary: runtime dispatch must NOT iterate the tables (no "outer gate" lookup) — this keeps both the RP2040 896K ceiling and hot-path dispatch untouched.

## 2. (b) The cross-check test

New hardware-free file `platforms/rp/src/system/native_handler/method_tables.rs`, wired exactly like class_registry.rs:
- `mod method_tables;` inside `native_handler/mod.rs` (not(test) side — Phase 2 handlers don't need it, but graphics'/net's consts live here for `ALL_HANDLED`);
- `#[cfg(test)] #[path = "system/native_handler/method_tables.rs"] mod native_method_tables_tests;` in `main.rs` next to the class_registry include (main.rs:85-89).

Contents: Phase 1 the hand consts; Phase 2 the `include!`+`emit_handled_rows` expansions replacing them one module at a time; plus `ALL_HANDLED`, the allowlists, and `#[cfg(test)] mod tests`.

```rust
#[cfg(test)]
mod tests {
    const ACC_NATIVE: u16 = 0x0100;

    /// SDK natives with intentionally no handler entry. Every entry is a
    /// deliberate runtime failure (or a not-yet-migrated module during
    /// incremental landing — see prefix form). Justify each in a comment.
    /// Goal: empty.  Entries: exact ("picodroid/x/Y", "m", "()V") or
    /// class-prefix ("picodroid/widget/*") for per-module landing.
    const ALLOWED_UNHANDLED: &[UnhandledEntry] = &[];

    /// Direction A: every ACC_NATIVE (class, method, descriptor) in the
    /// loaded framework has a handler row. Direction B: every handler row
    /// names a real SDK ACC_NATIVE method. Plus: no duplicate rows across
    /// tables. Runs under both shrink modes (scripts/test.sh).
    #[test]
    fn every_native_method_is_handled_and_every_handler_entry_is_real() {
        // 1. SDK set: for each FRAMEWORK_CLASSES class file:
        //      class = unshrink_class(cf.class_name())
        //      for m in cf.methods() where m.access_flags & ACC_NATIVE != 0:
        //        name = cp_utf8(m.name_index); desc = cp_utf8(m.descriptor_index)
        //        insert (class, name, unshrink_descriptor(desc))
        // 2. handled = union of ALL_HANDLED (+ pico_jvm::native::BUILTIN_SDK_HANDLED),
        //    asserting no (class,method,desc) row appears twice (two dispatchers
        //    claiming one method — today mod.rs chain order decides silently).
        // 3. Direction A: sdk − handled − ALLOWED_UNHANDLED == ∅.
        //    Failure prints missing rows as ready-to-paste Rust tuples,
        //    grouped by class, sorted — this IS the migration workflow.
        // 4. Direction B: handled − sdk == ∅. Failure labels near-misses:
        //    same (class, method) with different descriptor → "descriptor
        //    typo/overload drift"; unknown method → "stale row or SDK rename".
        // 5. Non-vacuity: assert sdk.len() >= 250 (today ~310) so a parser
        //    or FRAMEWORK_CLASSES regression can't make the test vacuous.
    }

    /// Shrink v1 renames classes, and descriptors embed class names:
    /// rewrite every `L<name>;` chunk via unshrink_class; primitives and
    /// array prefixes pass through. ~20 lines, test-only.
    fn unshrink_descriptor(desc: &str) -> String { /* ... */ }
}
```

Shrink-awareness: identical to the class-level test — loaded class names un-shrunk before comparison; method names are never shrunk in v1; **descriptors are un-shrunk by `unshrink_descriptor`** (without it, every object-typed signature fails only in the `PICODROID_SHRINK=1` lane of `scripts/test.sh`). If a future shrink version renames methods, dispatch itself gains `unshrink_method` and the test follows the same path — note this in the module doc.

Cfg-gated handlers: table files/consts are **cfg-free** even where the handler is gated (`net.rs` behind `has_network`, `sensors.rs` behind `not(test)`), so the diff asserts the union surface all boards share. Two micro-invariants alongside:
- `net_stub`'s two explicit rows (`NetworkInfo.isConnected/getIpAddress`) ⊆ `NET_HANDLED`, and a comment stating the stub's catch-all makes `not(has_network)` misses loud (`UnsupportedOperationException`), never silent.
- jvm-side: `jvm/src/native/mod.rs` gains `pub const BUILTIN_SDK_HANDLED: &[(&str,&str,&str)]` (~59 rows: Arrays 29, Math 27, System.arraycopy 1, Class.getName 1, System.currentTimeMillis is a *platform* row in OS_HANDLED) next to `BUILTIN_DISPATCH`, plus a test mirroring `builtin_dispatch_classes_subset_of_names`: every class in it ∈ `BUILTIN_CLASS_NAMES`. These four builtin dispatchers are descriptor-blind by design (atype-driven), stable, and compile under test normally — a parallel const with the diff test is enough; no X-macro in pico_jvm. (Value is real: a future `Arrays.sort(int[], int, int)` range overload would be silently mis-handled by today's `"sort"` arm; Direction A pins it.)
- `API_HINTS` sanity (optional, 3 lines): no hint's (class, method) may collide with a handled row — a hint that can never fire is dead flash.

## 3. (c) Migration effort per file

Counts verified by reading every dispatcher (`rg 'fn dispatch'`: 10 platform dispatch fns + 30 `GraphicsBackend` trait methods + 4 relevant jvm builtin modules). "Rows" = SDK overload rows; "arms" = match arms today.

| File | Arms | Rows | Phase 1 (table) | Phase 2 (X-macro) | Notes |
|---|---|---|---|---|---|
| `native_handler/pio.rs` | 31 | 33 | 30 min | 1 h | flat, fully regular |
| `native_handler/io.rs` | 12 | 12 | 15 min | 30 min | flat |
| `native_handler/net.rs` | 23 | 23 | 20 min | 45 min | table cfg-free; stub untouched |
| `native_handler/net_stub.rs` | catch-all | 0 | — | — | keep catch-all + stub⊆NET micro-test |
| `native_handler/sensors.rs` | 3 | 3 | 5 min | 15 min | |
| `native_handler/concurrent.rs` | 4 | 4 | 5 min | 20 min | |
| `native_handler/os.rs` | 5 | 5 | 10 min | 45 min | extract `Thread.start` body to a fn first |
| `native_handler/app_services.rs` | 8 | 8 | 10 min | 45 min | all `@any_receiver`; `state =` param |
| `native_handler/mod.rs` (self arms) | ~18 | ~19 | 20 min | 1.5 h | Log 5, Runtime 7, Activity 6 (setResult ×2 desc); `state =` + `@any_receiver`; convert LAST |
| `graphics/lvgl_backend.rs` (30 trait methods) | ~140 | ~136 | 2–3 h* | 4–6 h | per-widget macros; 3 alias entries; routing in graphics/mod.rs untouched |
| `jvm/src/native/{arrays,math,class_obj}.rs` + System | 23 | 59 | 45 min | not planned | parallel const stays |
| infra: emitters + test + `unshrink_descriptor` | — | — | 3–4 h | 2–3 h | picodroid-core macros ~60 lines; test ~150 lines |

*Phase-1 transcription is not really hand-typing: land empty tables → run the test → paste Direction A's ready-to-paste missing-row output per module → assign rows to the owning module's const. Direction B then catches any mis-assignment/typo instantly.

**Totals: Phase 1 ≈ 1 day (test live for all ~310 methods, both directions). Phase 2 ≈ 1 day flat handlers + 1 day graphics. ~2–3 days overall.**

## 4. (d) Incremental landing strategy

Every step keeps `./scripts/pre-commit` + both-shrink `scripts/test.sh` green; each is an independent commit.

1. **Infra** — `unshrink_descriptor`, `method_tables.rs` scaffold with empty consts, the test with `ALLOWED_UNHANDLED = &[prefix("picodroid/*"), prefix("java/*")]` (test compiles, vacuously green), jvm `BUILTIN_SDK_HANDLED` + subset test.
2. **Per-module Phase 1 commits** (any order, parallelizable): fill one module's const from the test's paste output, shrink the allowlist prefix accordingly (e.g. drop `picodroid/pio/*` when PIO_HANDLED lands). Suggested order: pio → io → net → sensors/concurrent/os → app_services/mod-core → graphics (biggest last, prefix-allowlisted meanwhile) → builtins.
3. **Burn-in**: whatever Direction A still reports after all tables land is a *real* pre-existing silent-NoSuchMethod (the class-level test can't see method gaps). Triage each: implement, or move to `ALLOWED_UNHANDLED` with a justification comment (the `ALLOWED_UNREGISTERED` discipline: "must stay empty unless intentional").
4. **Per-module Phase 2 commits**: convert one handler at a time to its table file + `emit_native_dispatch`; the already-green diff test plus the sim smoke (`helloworld`/`benchmark`/`blinky`) guard each conversion. Flat handlers first, `app_services`/`mod.rs` (stateful emitters) next, graphics last.
5. **Docs**: quality-roadmap Stage 2 entry marked landed; module doc on `method_tables.rs` names the test (per the "document test-enforced invariants and name the test" convention).

Yes — tables land per-module (step 2's prefix allowlist is exactly the mechanism), and Phase 2 conversions are naturally per-file.

## 5. Test plan

- `scripts/test.sh` (both shrink modes) — the new test itself, the jvm subset test, dispatch_sites and class-level tests unchanged.
- Mutation checks while developing infra: (a) delete a row → Direction A red with paste-able tuple; (b) add a bogus row → Direction B red with near-miss label; (c) descriptor typo → Direction B "descriptor drift"; (d) duplicate row in two tables → dedupe red; (e) run mutation (a) under `PICODROID_SHRINK=1` to prove `unshrink_descriptor` (message must show original names).
- Phase 2 per-handler conversion: full pre-commit + the three sim smoke apps; `cargo expand`-eyeball one emitter expansion once.
- Flash guard: compare `print_memory_usage` before/after Phase 1 on RP2040 — expected delta 0 (consts stripped).

## KEY FILES
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/native_handler/class_registry.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/native_handler/mod.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/native_handler/method_tables.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/native_handler/io.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/native_handler/pio.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/native_handler/os.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/native_handler/net.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/native_handler/net_stub.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/native_handler/app_services.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/native_handler/graphics/mod.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/native_handler/graphics/lvgl_backend.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/main.rs
/home/shiv/projects/picodroid-rs/picodroid-core/src/native_table_macros.rs
/home/shiv/projects/picodroid-rs/picodroid-core/src/dispatch_sites.rs
/home/shiv/projects/picodroid-rs/picodroid-core/src/shrink_names.rs
/home/shiv/projects/picodroid-rs/jvm/src/native/mod.rs
/home/shiv/projects/picodroid-rs/jvm/src/class_file/accessors.rs
/home/shiv/projects/picodroid-rs/docs/quality-roadmap.md
/home/shiv/projects/picodroid-rs/scripts/test.sh

## RISKS
Shrunk descriptors: under PICODROID_SHRINK=1 loaded descriptors embed shrunk class names (e.g. (La/B;)V); without the unshrink_descriptor helper every object-typed row fails only in the shrink lane of scripts/test.sh — build and mutation-test this helper first.
Burn-in may surface real pre-existing unhandled natives (the class-level test cannot see method gaps); triage could block landing unless the ALLOWED_UNHANDLED escape hatch (exact + class-prefix entries) is in from day one.
macro_rules hygiene: binder idents (class_name/method_name/ctx/handler) must be written in the table file and passed to the emitter via the args=() header; writing them in the emitter instead silently fails to resolve body references — verify with cargo expand before converting the first handler.
Phase-2 graphics conversion touches lvgl_backend.rs, a churn-critical file; dispatch precedence (class-specific -> ViewGroup -> View, and alias arms) must be preserved — keep routing in graphics/mod.rs hand-written, convert only per-widget method matches, and lean on sim smoke + the already-green diff test per commit.
Parallel-table residual gap during Phase 1: a table row can exist without a working arm (green build, runtime NoSuchMethod); this window closes only when the module converts to the X-macro — prioritize Phase 2 for handlers that churn (graphics), tolerate it for stable ones (mod.rs Log/Runtime arms).
Flash budget: tables must never be referenced from runtime code (no dispatch-time table lookup), or ~310 string triples land in RP2040 flash that is already at the 896K ceiling; verify with print_memory_usage after Phase 1.
Wildcard arms ((_, method)) are modeled by declaring class only; the test intentionally does not detect wildcard swallowing of a same-named method on an unrelated future class — the duplicate-row check plus Direction A forcing a conscious row addition is the mitigation, but reviewers should know the limit.
Test-include wiring is duplicated cfg surface: method_tables.rs compiles both as native_handler::method_tables (not(test)) and as a #[path] root module (test); an accidental HAL import or non-inert token at file top breaks the host test build for every module at once — keep the file as strictly hardware-free as class_registry.rs.

## SCOPE
~2-3 days total across ~6-10 independent commits: Phase 1 (parallel tables + both-direction diff test + unshrink_descriptor + jvm BUILTIN_SDK_HANDLED) ~1 day and immediately kills the silent-NoSuchMethod surface for all ~310 SDK native methods; Phase 2 (picodroid-core emitter macros + per-handler X-macro conversion making table<->arm drift structurally impossible) ~1 day for the flat handlers (pio/io/net/sensors/concurrent/os/app_services) plus ~1 day for graphics/lvgl_backend (~140 arms, 30 per-widget tables); mod.rs stateful arms convert last or remain parallel-table by explicit choice.

## CRITIQUE VERDICT: needs_changes

### ISSUES
- INHERITANCE GAP (falsifies a stated design fact): 'No inheritance modeling needed anywhere' is wrong. sdk/java/picodroid/widget/CompoundButton.java declares 2 natives (performCheckedChange, nativeRegisterCheckedChangeListener) but graphics/mod.rs has NO CompoundButton class arm — the arms live in FOUR concrete-widget dispatchers (lvgl_backend.rs ~161-167 switch, ~197-202 toggle_button, ~276-281 check_box, ~301-306 radio_button). Under the proposed per-widget 'class = ...' header, either Direction B fails (rows emitted under Switch/ToggleButton/CheckBox/RadioButton, which don't declare these natives) or, with a naive per-entry class override on all four tables, the 'no duplicate rows' assertion fires on 4 identical (CompoundButton, performCheckedChange, ()V) rows. The graphics grammar as specified cannot express this case.
- WILDCARD MULTI-CLASS ROWS: picodroid/app/Application.java:12 declares native startActivity(Intent) in addition to Activity.java:86 — one (_, "startActivity") arm must emit rows under TWO declaring classes. The @any_receiver grammar binds exactly one $class per entry, and adding a second @any_receiver entry for the same method would expand to an unreachable duplicate match arm, failing deny(warnings). The design's mod.rs row count (Log 5 + Runtime 7 + Activity 6) also omits the Application row.
- BUILD BREAKAGE, verified empirically: declaring `mod method_tables;` on the not(test) side of native_handler fails the pre-commit clippy/-D warnings lanes — main.rs has private `mod system;`, so unused pub consts in that tree trigger dead_code (reproduced with rustc -D warnings). Nothing at runtime may reference the tables (the design itself forbids it for flash reasons), so the not(test) declaration serves no consumer; its stated rationale ('graphics'/net's consts live here for ALL_HANDLED') is wrong — ALL_HANDLED is consumed only by the cfg(test) #[path] inclusion, which compiles the same file.
- BURN-IN IS GUARANTEED NON-EMPTY and the plan should say so: NotificationManager.java:21,23 declare native notify(int,Notification)/cancel(int); app_services.rs:4's doc comment claims to bridge them, but grep shows no "notify"/"cancel" arm anywhere in platforms/rp/src — a live silent NoSuchMethod today. The migration table's app_services count (8) silently excludes these 2; step 3 needs an implement-vs-ALLOWED_UNHANDLED decision named up front, not discovered mid-landing.
- STALE FACT: platforms/esp was removed in commit 8300bf8 ('chore!: remove ESP32-S3 platform support'); 'platforms/esp inherits them for free' is no longer a real benefit (picodroid-core is still the right home for the emitters, for shrink_names/framework_classes adjacency).
- MINOR COUNT DRIFT: Arrays declares 28 natives, not 29 (BUILTIN_SDK_HANDLED ~57-58 rows, not 59); docs/quality-roadmap.md:71 says ~294 methods vs the design's ~310 (my regex count of SDK declarations: 308); pio.rs has 33 arms, not 31; lvgl_backend has 29 trait methods, not 30. None change conclusions; the >=250 non-vacuity floor is safe either way.
- NUANCE for hand-transcription: AlertDialog.java's 7 native declarations split across the outer class and the $Builder inner class — the class files enumerate them under distinct declaring names (picodroid/app/AlertDialog vs picodroid/app/AlertDialog$Builder). The paste-from-test-failure workflow handles this; anyone typing rows from .java files by hand will get Direction B failures. Worth one sentence in the module doc.
- EMITTER REQUIREMENT left implicit: emitted match arms must preserve entry order (today's alias pairs and the core wildcard arms rely on non-overlap, but a structural order-preservation guarantee keeps a future overlapping entry from silently changing precedence). State it as a contract of emit_native_dispatch/emit_method_match.

### AMENDMENTS
1) Extend the graphics entry grammar with a per-entry declaring-class override plus row suppression, e.g. `("performCheckedChange", ["()V"], declared_by: "picodroid/widget/CompoundButton") => ...` on ONE widget table (emits the CompoundButton row) and `("performCheckedChange", ["()V"], row_suppressed) => ...` (or reuse the aliases mechanism) on the other three — arms in all four dispatchers, exactly one table row. Alternatively keep all four arms row-suppressed and declare a hand-written COMPOUND_BUTTON_HANDLED const in method_tables.rs. Either way, document that the duplicate-row assertion is what forces this to stay consistent, and add a mutation test: delete the CompoundButton row -> Direction A red; add it to a second table -> dedupe red. 2) Change @any_receiver to accept a class LIST: `(@any_receiver ["picodroid/app/Activity", "picodroid/app/Application"], "startActivity", ["(Lpicodroid/content/Intent;)V"]) => ...` emitting one `(_, "startActivity")` arm and one row per listed class; update the mod.rs row count to include Application.startActivity (19 rows: Log 5 + Runtime 7 + Activity 6 + Application 1). 3) Drop the not(test) `mod method_tables;` declaration entirely — wire the file ONLY via the `#[cfg(test)] #[path = "system/native_handler/method_tables.rs"]` include in main.rs (the ALL_HANDLED aggregation lives in the same file, so the test build needs nothing from the not(test) side). If a not(test) declaration is ever reintroduced, it must open with `#![allow(dead_code)]` per the dispatch_sites.rs precedent — verified that unused pub consts under the private `system::` tree otherwise fail -D warnings. Table files included by handlers (Phase 2) are unaffected: their macros expand into live dispatch code. 4) Pre-declare the known burn-in finding: add NotificationManager.{notify,cancel} to step 3 as a named triage item (implement in app_services.rs, whose doc comment already promises them and whose PendingServiceOp plumbing is adjacent, or allowlist with a justification), and fix the app_services doc comment if allowlisted. 5) Correct the fact sheet: Arrays 28 natives; total SDK ACC_NATIVE ~308 (roadmap's ~294 is stale); pio 33 arms; lvgl 29 trait methods; delete the platforms/esp inheritance claim (platform removed in 8300bf8). 6) Add to the emitter spec: generated match arms appear in entry order (contract, tested by the cargo-expand eyeball step). Everything else checked out against source — the dispatch-key/no-descriptor claim, the cfg(not(test)) gating and #[path] pattern, shrink-v1 class-only naming with descriptor rewriting in tools/class-shrink (unshrink_descriptor is genuinely required in the shrink lane), net_stub catch-all, Thread.start in os.rs, System.currentTimeMillis/arraycopy split, both-shrink test lanes, zero-flash const behavior, and the X-macro hygiene scheme ($emit:path callback with binder idents passed from the table file compiles and runs exactly as designed).
