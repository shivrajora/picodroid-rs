# Decisions

Dated rationale for the choices that are not obvious from the code, so they are not
re-litigated by the next session. Newest first; append, do not rewrite. A decision that is
reversed gets a new entry pointing at the old one. The design docs under `designs/` carry the
long form where one exists.

## 2026-10-20: ListView recycles behind getView/convertView, no RecyclerView

- **Java owns the window, Rust owns the geometry.** The binding loop (`getView`, `addView`,
  the child list) has to run in Java anyway, so a Rust-side decider would still upcall once per
  row and would need a per-list table with a delete hook; instead one deferred `fireScroll`
  per frame while a long list scrolls, two natives, and nothing at all for a list of eight items
  or fewer (`MIN_WINDOW`), which keeps every menu-sized list byte-identical to before.
- **Padding, not spacer children.** The list's `pad_top` / `pad_bottom` stand in for the rows
  outside the window; the theme's `pad_ver 0` / `pad_gap 0` are pinned in `create()`, so a row
  for position *p* is at `p × pitch` whatever the window holds and the click trampoline's
  `lv_obj_get_index(row)` stays the window slot.
- **Rows share one height.** The first row's height is the pitch and later rows are pinned to
  it. Android measures each row; here variable heights would make the padding an estimate and
  the viewport jump on every re-bind. Documented as partial.
- **Keypad order by adjacent swaps.** LVGL's group has no insert-at; `focus_order::sync` walks
  each row to its slot with `lv_group_swap_obj`, which preserves the relative order of the
  widgets around the list. Rejected: driving the rows' selection through `edit_mode.rs` (a
  keypad redesign for one widget).
- **`setSelection` scrolls but does not focus.** Android's selection model is keyed to touch
  mode, which picodroid does not have; scrolling is what every caller wanted.
- **RecyclerView stays out** (as the 2026-08 roadmap decided): the gap it answered was the
  memory cost of a long `ListView`, which is now its window.

## 2026-10-20: a touched class costs nothing; `onTrimMemory`

- **The "count your imports" rule is withdrawn.** Since pack-time class linking (2026-09-28,
  `designs/class-link-2026-09.md`) a framework class costs 16 B of RAM registered and nothing on
  first use; `claudeusage`'s `TimeFormat` is back to `LocalDateTime.ofInstant(..)` with the
  census showing the class table unchanged. Write the readable `java.time` form; the class
  store's RAM half is done and only its flash half remains (`quality-roadmap.md`, G4).
- **`onTrimMemory` levels map to the two pressure tests the framework already has** (LVGL pool
  below 1/8 free, native heap below 1/16): `TRIM_MEMORY_RUNNING_CRITICAL` is delivered before a
  covered Activity is destroyed and once per episode from the event loop; `onLowMemory` means
  the framework is about to destroy a covered Activity anyway. The other levels are constants:
  one app runs at a time, so nothing is ever "hidden" or "background", and a second native
  threshold for `RUNNING_LOW` would double the state machine for a level Android 14
  deprecated. No interest flag: the registry is never empty once an Activity exists and one
  no-op static call per rare episode is cheaper than the bookkeeping. A dropped Java cache
  stops the JVM growing but returns nothing to the native heap (the JVM keeps its capacity);
  dropped views do return LVGL pool, which is what saves a covered Activity.

## 2026-10-20: the agent harness

- **Direct to `main`, no automated review.** PR-per-task with a review bot (what the storytold
  craft suite runs) was declined: a mostly solo repo with a file-scoped hook, CI and three
  nightlies; review ceremony would cost more than it catches. Revisit when more than one agent
  lands work daily.
- **Never-panic as a ratchet, not a flag day.** Ten crates at zero get the deny attribute now;
  the other eight (99 sites) ratchet down through `scripts/panic-baseline.conf`. Host-side
  crates are excluded: a panic there fails a build or a test, not a board.
- **Crate layers are declared, not inferred.** `scripts/layers.conf` is hand-written and the
  guard checks the graph against it, so a new crate must say where it sits.
- **The scorecard is generated and never hand-edited;** the nightly section is this machine's
  and outside `--check`, so CI and the bench agree on the rest. Android parity is counted at
  the class level (`sdk/android-parity.tsv`); member-level rows can be added where a class is
  tracked member by member.
- **`pdb dump-ui` is a roadmap entry, not built** (`designs/agent-driving-2026-10.md`), by the
  owner's call on 2026-10-20; `sim-shot.sh` stays the UI oracle.
- **Session memory moves into the repo.** Conventions that lived in one machine's memory
  directory are now `.claude/rules/` and the nested `CLAUDE.md` files; a second machine, a
  Codex session or a contributor sees them.

## 2026-10-16: dependency majors stay blocked

- `pio 0.4` (rp235x-hal 0.4 pins 0.3), `rp2040-hal 0.12` (rp-pico 0.9 pins 0.10), RustCrypto
  `hmac 0.13` / `sha2 0.11` / `rand_core 0.10` (embedded-tls 0.19 and p256 0.13 use the old
  ones; bumping `pd-tls` alone duplicates them in flash). `Cargo.lock` stays gitignored: CI
  resolves fresh, the nightly ratchet uses this machine's lock, so a lock refresh is a ratchet
  accept, not a commit. No Dependabot for the same reason: automated bumps would fight the
  ratchet.

## 2026-10-15: the nightly runs at 3 AM, fuzz first

- Moved from 2 AM sim + 4 AM fleet: the sim run had grown to ~93 min and its `net` rows clashed
  with the fleet's listeners on 7000 / 8000 / 8443. One cron entry, three stages back to back:
  `fuzz.sh --nightly`, `sim-run.sh`, `hil-fleet.sh`.

## 2026-10-14: line numbers off on device

- `LineNumberTable`s cost 71.5 KB on the W debug image and are read only by stack traces;
  `--release` and the RP2040 drop them, the `line-numbers` feature (`PICODROID_LINE_NUMBERS=1`)
  brings them back, and it is the only debug-vs-release marker the firmware carries
  (`cfg(debug_assertions)` is off in both device profiles). Host tests keep them on.

## 2026-10-11: keep the 75 MHz display SPI clock, declare it

- The clock was programmed at 75 MHz when 62.5 was asked for (bug scan A2). It works on every
  panel on the bench and the frame time matters more than the datasheet margin; the config now
  says 75 and the comment says why.

## 2026-10-10: the manifest, reflection-lite, the time service

- **F4, the Android-shaped manifest:** `<activity>` / `<service>` children and `android:theme`,
  checked at build time (`papk-pack`, Gradle `verifyManifest`); permissions read and ignored.
- **F2, `Class.forName` / `newInstance` as interpreter prechecks** that run `<init>` as frames,
  so `ViewModelProvider.NewInstanceFactory`, the default `FragmentFactory` and
  `LayoutInflater` construct by name as Android does; no general reflection.
- **F5, a platform time service:** one `timesync` task anchors the clock from `pool.ntp.org`;
  the zone is a user setting (`/system/time`); apps' own SNTP code is gone.

## 2026-10-06: no attribution trailers

- Commits and PRs carry no `Co-Authored-By: Claude` or "Generated with" line; a local
  `commit-msg` hook strips one anyway.

## 2026-09-30: the toolchain is pinned, clippy is back in the hook

- `RUST_TOOLCHAIN` in `ci_checks.yml` (1.98.1) is what CI and the hook lint on; a floating
  `stable` kept producing clippy-only red mains the hook could not see. Bumping it means fixing
  the new lints and re-baselining the size ratchet in the same change.

## 2026-09-15: one interpreting task at a time; the handle table everywhere

- The JVM run lock is a kernel mutex, not scheduler configuration; blocking paths go through
  `unlocked()`. The generation-tagged handle table became every board's default after its soak;
  the `legacy-handle-cast` hatch was deleted 2026-10-17.

## 2026-09-14: the hook is thin; CI and the nightlies are the gates

- `pre-commit` builds nothing by default: guards and formatters, clippy when Rust changed.
  `--full` (mem-diag / sched-diag builds, the shrunk-image check, the size ratchet) is the
  release-cut gate only. When CI goes red on a stage the hook skips, add the stage to the hook
  scoped to the files that can break it (the `sdk` bucket, the embed guards, the leaf clippy leg
  were added that way).

## 2026-09-12: architecture rules become offensive guards

- A fixed architecture-class bug pairs with a text-scan test with a pinned count, a config
  assertion where a number is load-bearing, and, where the property is only visible at run
  time, an opt-in zero-cost monitor with a strict mode the soaks run. `spin_guard`,
  `seam_guard`, `task_affinity`, `cfg_gates`, the ratchets.

## 2026-09-02: the size ratchet is 0 %, and advancing it is consent

- `bench-report.py --ratchet` fails on any flash growth; `bench/parity/ratchet.toml` is advanced
  in the commit that spends the bytes, with a `size:` trailer saying why. Release cuts happen on
  `main` only, after the feature is merged, when the owner says go; a class added between cuts
  stays un-shrunk until the next one.

## 2026-08-31: a compile-time API contract, as a bytecode check

- Apps type-check against the JDK's full `java.*` (`--release 8`); an SDK stub cannot fence it
  (javac ignores it). `verifyApiContract` checks every `java/**` reference an app's classes make
  against `sdk/api-contract.tsv`, generated from pico-jvm's own tables. Zero device bytes.

## 2026-08-30: executor-only concurrency

- No `Handler`, `Looper`, `postDelayed`, `CountDownTimer`, `Timer`, `AsyncTask`: the main loop
  is an executor-driven dispatcher, and the Android pieces are leak-prone and temporally
  coupled. `Executors.mainExecutor()` / `View.post` post to the UI,
  `Executors.mainScheduledExecutor()` delays, broadcasts carry the clock.

## 2026-08: no `android.*` alias layer

- Built and reverted (`6663c4c`). The API is *named* to mirror `android.*`; apps import
  `picodroid.*`. An alias jar would hide which classes exist and cost a second name table.

## v0.9.0: GPL-3.0-only, dual-licensed

- Relicensed from Apache-2.0 (`9407d86`): GPL-3.0-only to the public plus a commercial license;
  contributions under the CLA. New source files carry the GPL SPDX header.
