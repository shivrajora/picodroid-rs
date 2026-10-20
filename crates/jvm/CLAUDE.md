# crates/jvm (pico-jvm)

The interpreter, object heap, GC, threads and the `java/**` builtins. L1: depends on `class-link` only; nothing here knows LVGL, the HAL or a board.

- **Test:** `./scripts/test.sh` (both shrink modes); one crate while iterating: `cargo test -p pico-jvm --target "$(rustc -vV | awk '/^host:/{print $2}')"`. `PICODROID_APK_PATH` is needed for the dispatch-site tests; `test.sh` builds it.
- **Never panics on bytecode or a full heap.** A checked class file still carries hostile numbers: `get()`, `checked_*`, caps. Allocation that cannot be served throws `OutOfMemoryError` (eight sites that reset the board instead were fixed 2026-10-15; `fuzz/regress/interp`, `bytecode_check` replay them).
- **Rooting.** Anything the native side holds across a call that can collect is rooted (`PendingOpQueue` roots the op it runs; a thread's terminate goes by task, never by an id that may have been reused). The GC-stress nightly pass (`gc_alloc_threshold=16`) finds the rest; run `./scripts/sim-run.sh --app gc-stress` after touching the heap or threads.
- **`find_method_by_name` scans one class's table**, no superclass walk; upcalls land on the exact class. `class_name_to_static_in` (`interpreter/helpers.rs`) must know every `BuiltinHandler` class or virtual dispatch silently breaks.
- **Field slots.** `field_slot_in` counts instance fields only; slot-addressed SDK classes add instance fields at the end (see `.claude/rules/java-sdk.md`).
- **Served surface = `sdk/api-contract.tsv`**, generated from this crate's tables by `api_contract_is_current`; Gradle rejects what is not in it. A new builtin adds rows through `scripts/gen-api-contract.sh`. `java.util.function` SAMs and `Thread.UncaughtExceptionHandler` are name-only rows.
- **Chars are bytes** in the string model (UTF-8 bytes, Modified UTF-8 names); `StringBuilder.append(CharSequence, int, int)` dispatches by name to the plain append.
- Design: `docs/designs/jvm-run-lock-2026-09.md` (one interpreting task at a time, `unlocked()` for blocking paths), `value-slot-8b.md`, `class-link-2026-09.md`; memory tooling in `docs/memory-diagnostics.md`.
