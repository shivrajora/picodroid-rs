# Fuzzing

**Status: built 2026-10-15** (branch `fuzzing`). Answers the production
readiness review's "No fuzzing anywhere. The PAPK parser, class parser, pdb
framing, and HTTP head parser all take untrusted bytes and have no fuzz
targets." How to run it: [../fuzzing.md](../fuzzing.md). The amendments at
the bottom record where execution diverged from this design, and they
override the body.

## 0. Why

A PAPK is unsigned (app-store S3 not started), pdb has no authentication, and
the pack-time bytecode checker bounds operands and branch targets only. So the
parsers *are* the security boundary: whatever survives `validate_structure`
is then read through unchecked accessors (`Link::from_raw`, the verified-receipt
fast path that skips the deep check on later scans, `validate_embedded` for
system apps). The hand-written malformed-input tests are good but finite. The
nearest thing to a fuzzer, `papk-format/tests/generative.rs`, corrupts only the
first 64 bytes and never calls `resources()` or the deep class-section check.

The survey for this design already turned up four bugs without running a
fuzzer (§5, Phase 0). That suggests a fuzzer will find more.

## 1. The shape

- **Engine:** cargo-fuzz (libFuzzer) on nightly. Nightly is already on the
  bench machine and in `rust_nightly.yml`. cargo-fuzz is the only new tool.
- **Two crates, so the workspace stays dependency-free and stable CI gets
  the regressions for free:**

```text
crates/fuzz-targets/     workspace member, host-only, no new deps
  src/lib.rs             pub fn papk(data: &[u8]) … one fn per target;
                         a ~40-line byte cursor for structure-aware targets
  tests/replay.rs        every file in fuzz/regress/<target>/ through its fn
fuzz/                    cargo-fuzz crate, `exclude`d from the workspace
  Cargo.toml             libfuzzer-sys + fuzz-targets (own gitignored lock)
  fuzz_targets/<t>.rs    fuzz_target!(|d: &[u8]| fuzz_targets::<t>(d));
  regress/<t>/           COMMITTED: seeds and minimized crash reproducers
  dict/<t>.dict          magic numbers, section tags, HTTP/JSON tokens
```

- **The target logic lives in `fuzz-targets`, not in `fuzz/`.**
  - `scripts/test.sh` replays `fuzz/regress/` on the pinned 1.98.1 toolchain in
    every CI run. This takes seconds, needs no nightly, and is the regression
    gate.
  - The clippy host leg (`clippy_host_tools` in `scripts/pre-commit`, and
    CI's `linting` job) adds `-p fuzz-targets`.
  - `fuzz/` stays a thin shim, so the nightly build only bitrots if the API of
    libfuzzer-sys itself changes.
- **Build flags:**
  - Always use `cargo fuzz run -a`, which turns on debug assertions and
    overflow checks, matching `[profile.test]`.
  - Use ASan, the cargo-fuzz default, for targets that reach `unsafe`:
    class-link, pico-jvm and pd-install.
  - Use `--sanitizer none` for the `forbid(unsafe_code)` crates (papk-format,
    pdb-protocol, http-head, pd-json). ASan buys them nothing and costs about
    2× throughput.
- **Build directory:** `fuzz/target/`, cargo-fuzz's own, so nightly
  artifacts never thrash `target/` or the lane dirs.

## 2. Targets and what each one checks

"No panic" is the floor. Each target also asserts a property, because the
bugs that matter here are acceptances, not crashes.

### Phase 1 — the four the review named

| Target | Entry | Property beyond "no panic" |
|---|---|---|
| `papk` | `Papk::parse` + every accessor, `resources()`, `class_section().validate()`, `flash_image::parse_meta` | `validate_structure(b).is_ok()` ⇒ every accessor and every asset/class/resource iterator completes without `Err`. Each asset satisfies `stride*height <= data.len()` and has a known `cf`. `validate_structure` ok ⇒ `validate_embedded` ok |
| `papk_prefix` | `find_manifest_value_in_prefix` over `b[..n]` | When it returns `Some`, it equals `find_manifest_value(b)`. This is the 512-byte install peek |
| `class_section` | copy to a 4-aligned buffer → `ClassSection::parse` → `validate` | `validate` ok ⇒ for every class, `Link` accessors (`cp_tag`, `cp_offset`, `string_desc`, …) stay in bounds for every index the table admits, and `find_class` agrees with a linear scan. **ASan**, because this is the `unsafe` the JVM trusts |
| `link_class` | `build::link_class(raw .class)` | Ok ⇒ `build_section([out])` validates. This differential check holds the builder and the validator to the same rules. It also runs `pico_jvm::ClassFile::parse` on the same bytes (leaked to `'static`) |
| `bytecode_check` | `bytecode::check_code` | Ok ⇒ every branch and switch target is in `0..len`, re-derived independently in the harness |
| `pdb_decode` | `InputEvent::decode`, `Greeting::parse`, `SysmonView::parse` + `task(i)`/`mem_diag()` | `Greeting` build → parse round-trips for structured inputs |
| `install` | `pd_install::install` over `MemTransport::for_papk(b)` into a `MemRegion` | Returns `false`, or the installed run equals the input byte-for-byte and passes `validate_structure`. A refused install leaves the region erased. Uses ASan, because of the `from_raw_parts` on the mapped image. `PAGE_BUF` is global, but libFuzzer runs one input at a time, so it is fine |
| `http_head` | `find_header_end`, `parse_head_bytes`, `header_value`, `is_chunked` | `find_header_end` gives the same answer fed in random splits (the `from` resume) as fed in one shot |
| `http_chunked` | `ChunkDecoder::push` byte by byte | Encoding fuzz-chosen chunks with a reference encoder and decoding them returns the original body. On raw input the decoder never emits more bytes than it consumed |

### Phase 3 — the next ring out

| Target | Entry | Why |
|---|---|---|
| `tls_server` | `TlsSession::open` over a scripted socket whose server-to-client bytes are the input. The seed is one captured handshake from `pd-tls/tests/live.rs` | Hostile-network bytes reach the vendored embedded-tls record and handshake parser. The README in our fork lists a "no panic on malformed RSA key" patch, which is the kind of bug this target finds |
| `der` | `der_lite::cert_names`, `PdVerifier::verify_certificate` | Certificate bytes come off the network |
| `json` | `pd_json::parse` with a fresh `Pool` | Depth cap and pool caps hold. Parse → serialize → parse is a fixed point |
| `small_records` | `sntp::parse_reply`, `wifi::decode`, `time_service::Record::decode`, `compat::check`, `ResTable::parse` | Cheap. SNTP is network input; the rest is flash or manifest input |
| `pdb_stream` | the device pdb loop, one frame at a time | Needs `run_pdb_task`'s body split into `serve_one(&mut transport, …)`. Property: after any frame, the next well-formed frame is answered correctly (no desync), and the transport is never asked for more than a bounded number of bytes per frame |
| `interp` | a class whose method body is the input, accepted by `check_code`, run with a fuel limit | The checker does not type the stack or locals, so the interpreter's run-time bounds checks are the last line of defence. Property: every outcome is a normal return or a Java exception, never a Rust panic or an ASan report |

The parsers in `picodroid-core` (the SNTP reply, Wi-Fi credentials, and the
pdb loop) need `fuzz-targets` to depend on `picodroid-core` with the `sim`
feature, which brings in the LVGL C build. Check that cost first. If it is
heavy, move the pure decoders into a leaf crate instead. The current home of
`sntp::parse_reply` and `wifi::decode` is accidental.

## 3. Seeds, dictionaries, corpus

- **Seeds** (committed to `fuzz/regress/<t>/`, each ≤ 4 KB, the whole tree
  ≤ 1 MB):
  - `papk-format/tests/fixtures`, plus a shrunk `helloworld.papk` truncated to
    its interesting sections.
  - A handful of `.class` files from the SDK build.
  - Frames captured from the pdb `MockPipe` tests.
  - Recorded HTTP heads, and the chunked bodies from `http-head`'s tests.
  - Full-size PAPKs from `build/apks/` go into the untracked corpus, not git.
- **Dictionaries:** PAPK magic and section tags, class-file magic and
  constant-pool tags, `PDBP` and the command bytes, and HTTP header names and
  `chunked`.
- **Living corpus:** `build/fuzz/corpus/<t>/` on the bench machine, untracked,
  so the nightly `git pull --ff-only` stays clean. Run `cargo fuzz cmin` once a
  week (Sunday) to keep it from growing.

## 4. When it runs

| Where | What | Cost |
|---|---|---|
| `scripts/test.sh` (CI `testing`, local) | `cargo test -p fuzz-targets` replays `fuzz/regress/` on stable | seconds |
| `scripts/pre-commit --full` | `cargo +nightly fuzz build` for every target, so the shims cannot bitrot before a release | ~1 min warm |
| bench cron `0 1 * * *` | `scripts/fuzz.sh --nightly`: 45 min split across targets by weight, `-fork=14` (16 cores, idle at night), done by about 01:55 so the 02:00 sim nightly gets the machine | free |
| `rust_nightly.yml` (Monday) | `cargo fuzz build` + 60 s per target | ~15 min, never a gate |

- **`scripts/fuzz.sh`:**
  - `--target T`, `--time S`, `--replay` (stable, what `test.sh` runs) and
    `--nightly`.
  - It checks for `cargo +nightly fuzz` and prints the one-line install if
    it is missing.
  - It writes `PASS|FAIL fuzz-<t>` lines to `build/fuzz/results/<run>.txt`, in
    the same format as sim-run.
  - It mails through `hil-email.py --suite fuzz`, so new-vs-persistent triage
    comes for free.
- **When a target crashes:**
  1. `fuzz.sh` runs `cargo fuzz tmin`, puts the minimized input in
     `build/fuzz/crashes/<run>/<t>/`, and puts the reproducer command in the
     email.
  2. The fix commit copies that input into `fuzz/regress/<t>/`, so the stable
     replay pins it.
  3. The nightly never commits anything itself.
- **Miri:** `cargo +nightly miri test -p fuzz-targets -- class_section
  link_class` replays the class-link regress set under Miri (cargo-miri is
  installed). It checks the `words_as` / `Link::from_bytes` casts in a way ASan
  cannot. Run it weekly, alongside `cmin`.
- **Coverage:** `cargo fuzz coverage <t>` feeds `coverage-summary.py` as a
  `fuzz` layer in the coverage history CSV (docs/coverage.md). This is how to
  tell whether a target has plateaued. It is optional and comes after Phase 2.

## 5. Phases

0. **Fix what the survey found (no fuzzer needed):**
   - `tools/pdb/src/protocol.rs` `recv_response`: `vec![0u8; len]` on a
     device-supplied `u32`. A hostile or garbled device can make the host
     allocate 4 GiB. Cap it at the largest legitimate response.
   - Device pdb `handle_ping` (and LIST, SYSMON) reads only the CRC and never
     drains `len` payload bytes, so a non-zero `len` desyncs the stream.
     `input::handle` and uninstall drain up to `len` (a `u32`) with blocking
     reads. Refuse any `len` above the verb's maximum and resync through
     `wait_for_magic`.
   - Assets: neither `AssetIter::step` nor `graphics/assets.rs` checks that
     `stride*height` (and the bytes per pixel of `cf`) fit in `data_size`, or
     that `cf` is a known format. LVGL then reads past the pixel data. Add the
     check to `validate_structure`, which makes the install refuse the asset,
     and to `step`.
   - `http_head::parse_status_code`: a saturated `i64` is cast `as i32`, so
     `HTTP/1.1 4294967496` reads as 200. This is harmless (a server can just
     send 200), but limit the code to three digits.
   - Each fix gets a unit test, in the repo's usual style.
1. **Infrastructure + the Phase 1 table:**
   - The two crates, `scripts/fuzz.sh`, seeds and dictionaries, and the replay
     in `test.sh`.
   - Run each target by hand for 30 min and fix what falls out.
   - Done when every Phase 1 target has run for 30 min clean and its seeds and
     reproducers are committed.
2. **Unattended:**
   - The cron entry, the email, the `--full` build stage, the weekly
     `rust_nightly.yml` build, and Miri plus `cmin` on Sundays.
   - Done after a week of nightlies with no new FAIL, or with every FAIL
     fixed and pinned.
3. **The Phase 3 table**, in this order:
   1. `tls_server`, because it has the largest reachable surface from the
      network.
   2. `pdb_stream`, which needs the `serve_one` refactor.
   3. `interp`.
   4. The rest.

## 6. What is not here, and why

- **FreeRTOS+TCP DNS/DHCP and the cyw43 driver** are C code under
  `third_party/`. A Rust harness cannot reach them. Fuzzing them would need a
  separate libFuzzer C harness against the IP task. Revisit this if the
  network becomes a priority. Until then, the defence is staying current with
  upstream.
- **LittleFS on-flash metadata:** the attacker would need to write flash
  directly, and by then they own the device.
- **Pack-time inputs** (papk-pack's resource XML and PNG decoding, and
  class-shrink): these are the developer's own files, on the developer's
  machine. The exception is `papk-pack --repack` and `papk-info`, which read a
  PAPK. They share the `papk` target's parser, so they are already covered.
- **UI or system monkey fuzzing** (random input through the sim control FIFO,
  with core dumps on) is a different tool for a different bug class: LVGL
  pointer lifetimes. An ad hoc run once found a keyboard use-after-free that
  the handle sanitizer missed. It is worth its own design, built on sim-run's
  `drive_test_ctrl`. It is not part of this one.
- **Property-test crates** (proptest, arbitrary in the workspace) are not
  used. The workspace stays dependency-free, and the structure-aware targets
  get their structure from the in-crate cursor. `arbitrary` would be allowed
  inside `fuzz/` only, if the cursor ever gets in the way.

## 7. Verification

- **Phase 0:** each fix has a unit test that fails before the fix and passes
  after it. Also run `./scripts/test.sh`, the sim smoke, and pdb install,
  list and ping against the sim (`sim.sh`'s pdb endpoint).
- **Phase 1:**
  - `scripts/fuzz.sh --replay` passes on 1.98.1.
  - `scripts/fuzz.sh --target papk --time 60` reaches a stable exec/s with
    coverage still growing.
  - Planting a bug proves the oracle works: drop one bounds check in a scratch
    branch, and the target finds it within minutes. Do this at least for
    `papk` and `class_section`.
- **Phase 2:** read a week of `build/fuzz/results/` and the emails.

## 8. Amendments

Built 2026-10-15 on branch `fuzzing`. Where it differs from the body above,
this section wins.

- **Phases 0–2 and most of Phase 3 were built in one pass.** That covers
  every target in §2 except `pdb_stream`. The targets are `papk`,
  `papk_prefix`, `papk_roundtrip`, `class_section`, `link_class`,
  `bytecode_check`, `interp`, `pdb_decode`, `install`, `http_head`,
  `http_chunked`, `json`, `der`, `tls_server` and `records`. `records` is
  what §2 called `small_records`: compat, the boot-meta sector, and a bare
  `ResTable`.
- **`pdb_stream` and the `picodroid-core` decoders are not built.** The
  decoders are `sntp::parse_reply` and `wifi::decode`. Both need
  `picodroid-core` as a dependency, which means the `sim` feature and the C
  build behind it. The oversized-`len` refusal is pinned by a unit test
  instead (`an_oversized_len_is_refused_unread`). This is still open.
- **The cron runs at 01:00, and the budget is 45 minutes, not 150.** The sim
  nightly starts at **02:00** (c2b4f111), not at 03:00 as this design
  assumed. `fuzz.sh --nightly` splits 45 minutes by weight; on
  Sundays it gives 15 of them to `cmin` and Miri. The cron line's
  `timeout 3500` is a backstop.
- **`link_class` does not call `pico_jvm::ClassFile::parse`.** It leaks a
  section per input. The interpreter has its own target, `interp`, instead.
- **`interp` needed two things the design did not foresee.**
  - The interpreter has no instruction budget, so the target only runs
    bytecode that provably ends: forward branches to instruction boundaries
    only, and no `jsr`/`ret`.
  - The target runs under a device-sized heap. `budget.rs` is a metered
    global allocator, 1 MB per input. Without it, a class that allocates
    forever met a ten-second libFuzzer timeout instead of the
    OutOfMemoryError a board gives it.
- **`bytecode_check` is a differential check.** Code that `check_code`
  accepts must link, and code it refuses must not. §2's "re-derive every
  target independently" was not built.
- **The build directory is `fuzz/target/`, cargo-fuzz's own default.**
  `fuzz/Cargo.toml` restates the root's `[patch.crates-io]` entry for
  `embedded-tls`, and `fuzz.sh` refuses to run if the two drift apart.
- **Findings.**
  - Fixed by Phase 0, before any fuzzing:
    - asset geometry was unchecked (validate and refuse);
    - the pdb host allocated whatever `len` it was sent;
    - device pdb verbs drained a hostile `u32` `len`;
    - HTTP status codes truncated.
  - Found by the fuzzer, each with a `finding-*` reproducer under
    `fuzz/regress/`:
    1. `http_chunked`: a 17-digit chunk size overflowed. It panicked in a
       test build and wrapped to a small size on the device. Now it is
       `Bad`.
    2. `papk`: `Linked::method_max_stack`/`max_locals`/`code_len`
       subtracted from a `code_offset` under 8 on an unvalidated table.
       `method_code` could also overflow `start + len` on a 32-bit target.
    3. `interp`: the 65,536th array wrapped its `u16` reference to 0,
       aliasing the first array. The GC compactor's sort key then mixed the
       slot into the arena offset. `ArrayHeap` and `ObjectHeap` (and its
       list, map and `StringBuilder` stores) now refuse at `REF_TOP`, as
       the string table already did.
    4. `interp`: `multianewarray` truncated a count past `u16`. A tree
       needing more references than exist now throws OutOfMemoryError
       straight away.
    5. `interp`: a `multianewarray` tree too big for the heap spun for
       ever. Each emergency GC freed the partial tree, so the retry never
       saw "nothing freed". The partial tree is now freed on failure.
    6. `interp`: an uncaught OutOfMemoryError aborted while building its
       stack trace, because the heap was still full. On a board that is a
       reset. The trace is now reserved fallibly.
    7. `interp`, under the 1 MB budget: six more allocations a program
       reaches grew infallibly, so a full heap aborted (a board reset)
       instead of throwing OutOfMemoryError. They are now fallible:
       - `Frame::store_local` past `max_locals` (`wide astore 65535` is a
         megabyte of locals);
       - `Frame::push` past `max_stack`, and the `dup_x*` inserts;
       - `multianewarray`'s count vector;
       - the superclass chain built for class initialization;
       - the `Class` object cache;
       - interning a literal (`ldc`).

       `push`'s growth moved out of line (`#[cold]`), so the inlined fast
       path lost `Vec::push`'s own grow branch. With the GC fix below, the
       branch's net flash is −76 B on rp2040 and −304 B on rp2350 (ratchet
       accepted).

     Items 3–7 are ordinary Java programs, not hostile bytecode. Item 5 is
     any `new int[a][b]` too big for the heap.

     The checker's lack of stack and local discipline is why the first two
     bullets of item 7 are reachable at all.
- **Found by the Miri pass (Tree Borrows) and fixed: the GC read its own
  stack through a raw pointer.** Every `execute` registers its frame stack
  in `GcState::parked_frames` so that other executors' collections root it.
  A collection from inside the run loop walked its own stack twice: once
  from the `frames` slice, and once back through that raw pointer. The
  second read aliased the `&mut Vec<Frame>` the loop holds across the
  collection, which is undefined behavior. The collecting executor now
  takes its own entry out of the list for the collection
  (`Executor::own_frames`) and puts it back afterwards.
  - The first try passed `&mut Vec<Frame>` into `collect_garbage`. That cost
    1.5 KB of rp2350 flash: LLVM reloaded the frame stack after every call
    in the run loop.
  - The pointer now rides in the `Executor`, and the change costs +208 B on
    rp2350 and +12 B on rp2040.
- **Open — Miri under Stacked Borrows rejects `Link::from_raw`.** This is
  not fixed; it is a decision for the maintainer.
  - **What happens.** `ClassFile` keeps a pointer it takes as
    `NonNull::from(&link.words()[0])`. That pointer's provenance is one
    `u16`. `from_raw` then reads the whole table through it, and the
    section's pool past the table. Stacked Borrows calls that undefined
    behavior. Tree Borrows, the newer model, accepts it.
  - **What the Sunday Miri pass does.** It runs with
    `-Zmiri-tree-borrows`, so it checks everything else.
  - **The fix if Stacked Borrows should hold.** Have `ClassSection::class`
    hand out, beside the slices, a first-word pointer derived from the whole
    section; `ClassFile` stores that one instead. `ClassFile` stays one thin
    pointer, but `Link` or `Linked` grows by a pointer, and both are on the
    JVM's hot path. That is a change to measure, not to slip in. The interpreter's run-time checks held
     everywhere else the fuzzer reached: no out-of-bounds access, and no
     ASan report.
