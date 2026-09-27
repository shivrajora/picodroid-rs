# Protocol Buffers for picodroid (2026-09-26)

`picodroid.protobuf` — protobuf-javalite's stream API over a native wire codec, a `protoc` plugin
that turns a proto3 `.proto` into Java message classes, and `claudeusage` as the first consumer.
Board-gated by `has_protobuf = true` in `board.toml`, the way `has_json` gates `picodroid.json`.

## Why

`claudeusage` read its bridge's 350-byte reply with `picodroid.json`. Replacing that with protobuf
is a modest win for the app on its own — the JSON library is already native Rust, so parse time
does not change; `JSONObject` + `JSONArray` are 5.1 KB of class metadata plus a 2 KB node pool
and every `opt*` call boxes, and the generated message classes cost roughly that back — but the
facility is what the app store roadmap (S8, protobuf control plane decoded in Rust) and the
Meshtastic roadmap (a varint codec) both need and neither had. `claudeusage` gives it a live user
and a nightly row.

## Decisions

- **Native codec, thin Java.** Byte-at-a-time work on this JVM costs ~20 µs per bytecode on the
  RP2350 (the `SharedPreferences` CRC-32 lesson), so tags, varints, fixed-width values and skipping
  are native; slicing strings and byte arrays, zigzag, limits and size arithmetic are Java.
- **micropb 0.6.0 runtime, not a hand-rolled codec.** `no_std`, no `alloc`, `num-traits` only, MIT
  or Apache-2.0, pinned `=0.6.0` because `Cargo.lock` is gitignored. Its `PbRead`/`PbWrite`
  traits let `native_handler/protobuf/heap_io.rs` feed it straight from the JVM array heap (a
  16-byte refill window over `ArrayHeap::load`; there is no `&[u8]` view of a packed `byte[]`),
  and it is the crate S8 already chose for generated Rust structs, so one dependency serves both.
  Caveats: pre-1.0, single maintainer, no conformance runner or fuzzing in its tree — so the
  natives keep their own golden vectors (`heap_io.rs` tests) and `examples/protodemo` round-trips
  every field kind on the runtime. Alternatives: a hand-rolled `pd-proto` (zero deps, ours to
  maintain, S8 would add micropb anyway), `femtopb` (slice-only helpers, unconditional proc-macro
  dependency), `prost` (needs `alloc` and `bytes`).
- **Generated message classes, committed.** `protoc` is installed neither on the bench nor in CI,
  so `scripts/gen-proto.sh` (a pinned `grpcio-tools`, which bundles a matching `protoc`) writes the
  Java and the bridge's `*_pb2.py` into the tree; `--check` in pre-commit (when the toolchain is
  present) and CI (always) keeps them current. The plugin is `tools/protoc-gen-picodroid/`.
- **Exceptions from Rust.** `InvalidProtocolBufferException` declares no instance fields on
  purpose: `throw_exception` allocates the class with zero fields and never runs a constructor (the
  message travels in the exception side table). The first classfile-backed SDK exception raised by
  a native; `protodemo` proves it is catchable as `IOException` with its message.
- **Writes answer a status, reads throw.** A write that does not fit answers `-1` and Java throws
  `OutOfSpaceException` after the bytes that did fit (micropb hands a varint over one byte at a
  time); a read past the limit or a malformed varint throws from the native.

## Deviations from protobuf-javalite

| javalite | picodroid | why |
|---|---|---|
| immutable messages + `Builder` | mutable, `setX` returns `this`, `clearX` | a Builder doubles the class count |
| Java `enum` per proto enum, `getX()`/`getXValue()` | `int` constants in a constants-only class named after the enum (javac inlines them), `getXValue()`/`setXValue(int)`, `getXsCount()`/`getXsValue(i)`/`addXsValue(v)` | one class per enum on the device |
| `List<T>` for repeated | arrays with a count: `getXCount()`, `getX(i)`, `addX(v)` | boxing on this heap |
| `ByteString` | `byte[]` | no `ByteString` class |
| `InputStream`/`OutputStream` overloads | none | the runtime has neither |
| `map`, `oneof`, groups, extensions, `Any`, reflection, text/JSON formats, proto2, editions | refused by the generator (`map`, `oneof`, groups, extensions, proto2) or absent | not needed yet; each is a follow-up, not a redesign |
| UTF-8 validation in `readString` | none | the runtime's strings are byte strings |
| `equals`/`hashCode` on messages | none | cost without a consumer |
| field named `class` → `getClass_()` | same | `Object.getClass()` is final |

## What a message costs

Each message type is one class: about 1–2 KB of device class metadata for a five-field message
(the `claudeusage` reply is five classes). A parse allocates only the message and its arrays and
strings — no wrappers, no boxing, no pool. `examples/claudeusage`'s H2 measurement records the
before/after on the `pico_display2_w`.

## Runtime shape

- `sdk/java/picodroid/protobuf/`: `CodedInputStream` (fields `mBuf`, `mPos`, `mLimit`,
  `mLastTag`, `mStart`, in that order, slot-addressed from Rust), `CodedOutputStream` (`mBuf`,
  `mPos`, `mLimit`, `mStart`), `InvalidProtocolBufferException`, `MessageLite`, `WireFormat`.
- `crates/picodroid-core/src/native_handler/protobuf/`: `mod.rs` (dispatch module 12, twelve
  arms), `heap_io.rs` (`HeapReader`/`HeapWriter` + golden vectors), `fields.rs` (slot table,
  checked by `native_field_tables_tests`).
- `crates/build_support/board_cfg.rs`: `has_protobuf`, `emit_protobuf_cfg`, `PROTOBUF_CLASSES`
  (dropped from the embedded SDK when the key is off; mirrored in `ApiContract.kt`).
- Boards on: every `has_json` board (`pico_display2_w`, `pico_enviro_mon[_w]`, `pico_touch_kit`,
  `testbench_rp2350[w]`); `testbench_rp2040` off.
- Names: member names are `m`-prefixed (`mPos`) because every SDK member name becomes a generated
  `m::` const and the `no_original_name_literals` guard fails on any Rust literal spelling it.

## Tooling

- `tools/protoc-gen-picodroid/protoc_gen_picodroid.py`: the plugin; `requirements.txt` pins
  `grpcio-tools==1.84.0` / `protobuf==7.36.2`; `selftest.py` compares `fixtures/all_kinds.proto`
  against `golden/`.
- `scripts/gen-proto.sh [--check]`: every `examples/*/proto/*.proto` → `examples/<app>/java/` (and
  `bridge/*_pb2.py` where a `bridge/` exists), formatted with the repo's google-java-format.
  `protoc` starts the plugin by its shebang, so the interpreter with `grpc_tools` goes first on
  `PATH` (`PICODROID_PROTO_PYTHON`).
- pre-commit stage `proto` (jvm lane; skips with the install hint when the toolchain is absent);
  CI `Linting` installs the requirements and runs `selftest.py` + `gen-proto.sh --check`.
- `examples/protodemo`: conformance app, nightly row `protodemo|term|120|...|rp2350,rp2350b`.

## claudeusage

`proto/usage.proto` (`UsageReply` + `Window`, `ModelCap`, `Today`, `ModelShare`), generated into
`java/claudeusage/proto/`; `UsageFetcher` sends `Accept: application/x-protobuf` and copies the
parsed reply into `UsageSnapshot` with the old clamps; the bridge serves protobuf on that `Accept`
and JSON to everything else (so `curl /u`, older firmware and the pixel A/B replay keep working),
`--once` prints both sizes (demo: 323 B JSON, 145 B protobuf), and the demo `garbage` mode returns
a truncated protobuf body when asked for protobuf. The bridge now needs the `protobuf` package
(`bridge/requirements.txt`); a bridge started before this change must be restarted with it.

## Follow-ups

- `map`/`oneof` in the generator when an app needs them.
- S8 (app store): the store's natives can decode with the same micropb dependency behind
  `has_protobuf`, or the store app can use generated classes directly.
- Ratchet: the `testbench_rp2350` image grows by the five classes plus the linked micropb paths
  (~10–14 KB); `testbench_rp2040` is unchanged (key off). Accepted in the nightly's next run.
