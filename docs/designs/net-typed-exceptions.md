# Design: typed network exceptions — Android-faithful error surfacing — 2026-08-15

**Goal:** network failures surface to apps as the exception types Android
developers expect — `ConnectException`, `SocketTimeoutException`,
`UnknownHostException`, `BindException`, `SocketException`,
`ProtocolException` — with Android-style messages, catchable per-type or via
their `IOException`/`Exception` superclasses. This closes NET-9's
error-surfacing bullet (`docs/networking-followups-2026-08.md`) across the
whole `picodroid.net` stack, not just `Socket.connect`.

This doc is written to be executed cold in a new session. Every claim below
was verified against the tree at `81b6b7c` (file:line references are from
that commit).

## Why (context)

- `a38d53c` made `Socket.connect` throw a catchable `java/io/IOException`
  instead of `JvmError::InvalidReference`. Everything else in
  `picodroid-core/src/net/` still fails with `InvalidReference` or a silent
  `-1` — and **`InvalidReference` is uncatchable by design**: only
  `JvmError::Exception(_)` routes into `handle_exception`
  (`jvm/src/interpreter/mod.rs:478-484`); every other variant kills the app
  (`Application.onCreate error: InvalidReference`). During WiFi bring-up this
  cost a full debug cycle (NET-9).
- Android's contract is typed: `connect` → `ConnectException("Connection
  refused")` / `SocketTimeoutException`; `DatagramSocket.receive` past
  SO_TIMEOUT → `SocketTimeoutException`; DNS failure →
  `UnknownHostException`; bind conflict → `BindException`; malformed HTTP
  response → `ProtocolException`. Apps branch on these types. The project
  goal (CLAUDE.md) is to preserve exactly this API surface.

## Current state (verified survey)

### What throws / returns what today

| Path | Failure today | Should be |
|---|---|---|
| `Socket.connect` (`net/socket.rs:40-58`) | `IOException("connect failed (err N)")` | `ConnectException` / `SocketTimeoutException` / `NoRouteToHostException`, Android wording |
| `Socket()` create (`socket.rs:19-32`) | `IOException` | keep (see "ctor divergence" below) |
| `Socket.send` (`socket.rs:61-93`) | **silent `-1`**, errno discarded | throw `SocketException` (Android `OutputStream.write` throws) |
| `Socket.recv` (`socket.rs:96-127`) | `-1` on error; `0` ambiguous | `-1` = EOF only; `SocketTimeoutException` on timeout; `SocketException` otherwise |
| `ServerSocket` listen (`net/server_socket.rs:12-24`) | `InvalidReference` (incl. EADDRINUSE!) | `BindException("Address already in use")` / `SocketException` |
| `ServerSocket.accept` (`server_socket.rs:27-41`) | `InvalidReference` (incl. SO_TIMEOUT expiry) | `SocketTimeoutException("Accept timed out")` (see HAL note) |
| `DatagramSocket` create/bind (`net/datagram_socket.rs:15-23`) | `InvalidReference` | `BindException` / `SocketException` |
| `DatagramSocket.send` (`datagram_socket.rs:26-68`) | `InvalidReference` | `SocketException` |
| `DatagramSocket.receive` timeout (`datagram_socket.rs:71-123`) | `InvalidReference` | `SocketTimeoutException` — device already distinguishes it (−11 EWOULDBLOCK) |
| `HttpURLConnection` DNS fail (`net/http_connection.rs:129`) | `InvalidReference` | `UnknownHostException("Unable to resolve host \"…\"")` |
| `HttpURLConnection` connect fail (`http_connection.rs:133-136`) | `InvalidReference` | `ConnectException` / `SocketTimeoutException` |
| HTTP send / malformed response (`http_connection.rs:327-359,413-426,469-478`) | `InvalidReference` | `SocketException` / `ProtocolException("unexpected status line: …")` |
| `HttpInputStream.read` device timeout (`http_connection.rs:303-306`) | indistinguishable from EOF, returns `-1` | `SocketTimeoutException`; `-1` = EOF only |
| Stale/closed handle (`net/helpers.rs:45-60` null lookup) | `InvalidReference` | `SocketException("Socket is closed")` — keep `InvalidReference` only for malformed arg shapes |

### The two HAL error spaces are broken in opposite ways

`NetError(pub i32)` (`picodroid-core/src/hal/types.rs:45-47`) is documented as
"the family's own errno-style code":

- **Device** (`platforms/rp/src/hal/rp/net.rs`): real FreeRTOS negated errnos
  for connect/send/recv/udp (−116 ETIMEDOUT, −128 ENOTCONN, −11 EWOULDBLOCK,
  −22 EINVAL — table in `third_party/FreeRTOS-Kernel/include/projdefs.h`),
  **but** `tcp_socket`/`tcp_accept`/`dns_resolve` flatten everything to
  `NetError(-1)` (net.rs:145-152, 277-285, 308-322).
- **Sim** (`picodroid-core/src/hal/sim/net.rs`): **every single error is
  `NetError(-1)`** — all sites are `.map_err(|_| NetError(-1))`. Refused,
  timeout, unreachable, DNS failure: identical. The sim cannot express the
  Android taxonomy at all today.
- **Inverted recv semantics (the nastiest trap):** on device,
  `FreeRTOS_recv` returns **`0` on SO_RCVTIMEO expiry** and **−128 on
  peer close** (`vendor/freertos-plus-tcp/source/FreeRTOS_Sockets.c:4155-4321`);
  on sim, `read()` returns **`Ok(0)` on peer close (EOF)** and an **error on
  timeout**. Shared code above the HAL cannot currently be correct on both.
  (`http_connection.rs:303-306` treats `Ok(0)` as EOF — right on sim, wrong
  on device, where a slow server reads as clean EOF.)
- `test_platform.rs:259-313`: every method returns `NetError(-1)` ("DOWN").

### Infrastructure that already exists (reuse, don't rebuild)

- **Throw-by-name pattern**: `net/helpers.rs::throw_io_exception` (:20-34) —
  `objects.alloc("<class>")` + `strings.intern_dyn(msg)` +
  `register_exception_message` → `JvmError::Exception(idx)`. Classes need no
  .class file; `ObjectHeap::alloc` takes a `&'static str`.
- **Hierarchy + resolution**: catch matching walks loaded classes then
  `builtin_super` (`jvm/src/interpreter/helpers.rs:266-290`); method
  resolution for classfile-less receivers walks it too
  (`jvm/src/interpreter/ops_invoke.rs:370-400`), which is how `getMessage()`
  reaches Throwable's dispatcher. `"java/io/IOException" =>
  Some("java/lang/Exception")` already exists.
- **Message plumbing**: `register_exception_message` currency is a
  StringTable index from `intern_dyn`; GC-safe (marked while owner lives,
  `jvm/src/gc/mod.rs:276-289`; interpreter never GCs on the error tick,
  `interpreter/mod.rs:440`).

## Design

### Phase 1 — JVM: the java.net hierarchy (no net behavior change)

`jvm/src/interpreter/helpers.rs::builtin_super`, new arms (real Java
hierarchy, so superclass catches work exactly as on Android):

```rust
"java/io/InterruptedIOException" => Some("java/io/IOException"),
"java/net/SocketTimeoutException" => Some("java/io/InterruptedIOException"),
"java/net/SocketException" => Some("java/io/IOException"),
"java/net/ConnectException"
| "java/net/NoRouteToHostException"
| "java/net/BindException" => Some("java/net/SocketException"),
"java/net/UnknownHostException" => Some("java/io/IOException"),
"java/net/ProtocolException" => Some("java/io/IOException"),
```

Tests (both are existing gaps, found in survey):
- Extend `builtin_throwable_hierarchy_resolves_without_classfiles`
  (`jvm/src/interpreter/tests/exceptions.rs:357-391`) with the new chains
  (e.g. ConnectException→SocketException→IOException→Exception;
  SocketTimeoutException NOT a SocketException — real-Java quirk worth
  pinning) plus negatives.
- New bytecode-level test: a native-minted alloc-by-name exception
  (`java/net/ConnectException`) caught by a `catch java/io/IOException`
  handler with **no classfiles for either** — today only classfile-backed
  subclass catching is tested (`athrow_subclass_caught_by_superclass`, :336).
  Also cover "method resolution walks builtin_super" (`getMessage()` on the
  minted object) — currently untested.

### Phase 2 — HAL: semantic `NetError`

Replace `NetError(pub i32)` with a semantic kind + raw code
(`picodroid-core/src/hal/types.rs`):

```rust
pub struct NetError { pub kind: NetErrorKind, pub raw: i32 }
pub enum NetErrorKind {
    Refused,      // RST / eSOCKET_CLOSED during connect
    TimedOut,     // connect/recv/accept timeout
    Unreachable,  // no route / ARP failure (if distinguishable)
    Closed,       // peer closed / not connected (post-connect)
    AddrInUse,    // bind conflict
    HostLookup,   // DNS resolution failed
    Other,        // anything else; raw carries the family code
}
```

Each family translates its own codes at its boundary (this is the family's
job per the `NetError` doc comment — the seam is `hal/facade.rs:342-369`
`__pd_hal_net_*` / `traits.rs:145-161` / `macros.rs::set_hal_net!`):

- **Device** (`platforms/rp/src/hal/rp/net.rs` + `glue.rs:275-338`):
  connect −128→Refused, −116→TimedOut; recv **0→TimedOut** and
  **−128→Ok(0)** (see contract below); udp recvfrom −11→TimedOut;
  send −128→Closed; `dns_resolve` 0-result→HostLookup; listen: read the
  real `FreeRTOS_bind` return instead of flattening (EADDRINUSE→AddrInUse);
  accept null→TimedOut (FreeRTOS accept returns NULL for both timeout and
  error — timeout is the overwhelmingly common case once SO_RCVTIMEO is
  set; document the approximation in a comment).
- **Sim** (`picodroid-core/src/hal/sim/net.rs`): stop discarding
  `std::io::Error` — map `ErrorKind::ConnectionRefused`→Refused,
  `TimedOut`/`WouldBlock`→TimedOut, `ConnectionReset`/`BrokenPipe`→Closed,
  `AddrInUse`→AddrInUse, `to_socket_addrs` failure→HostLookup; keep the
  documented EINTR retry loop (:49-63) exactly as is; carry
  `raw_os_error()` in `raw`.
- **test_platform.rs**: `DOWN` becomes `NetError { kind: Other, raw: -1 }`.

**Normalized `tcp_recv` contract** (fixes the inverted semantics): `Ok(0)` =
orderly EOF, `Err(TimedOut)` = timeout, on every platform. Device maps
`0 → Err(TimedOut)` and `−128 → Ok(0)` (graceful-close-as-EOF; a hard RST is
also −128 there — accept the approximation, Android apps treat both as
end-of-stream in practice). Sim passes `Ok(0)` through and maps timeout
errors. `http_connection.rs:303-306` and `socket.rs::recv_native` then agree
with reality on both platforms.

### Phase 3 — net natives: map kinds → typed exceptions

Generalize `net/helpers.rs`: `throw_io_exception` becomes a thin wrapper over

```rust
pub fn throw_net_exception(objects, strings, e: NetError, ctx: NetOpCtx) -> JvmError
```

where `NetOpCtx` (Connect/Send/Recv/Accept/Bind/Dns) picks class + Android
wording:

| kind + ctx | class | message |
|---|---|---|
| Refused + Connect | `java/net/ConnectException` | `Connection refused` |
| TimedOut + Connect | `java/net/SocketTimeoutException` | `connect timed out` |
| TimedOut + Recv | `java/net/SocketTimeoutException` | `Read timed out` |
| TimedOut + Accept | `java/net/SocketTimeoutException` | `Accept timed out` |
| Unreachable + Connect | `java/net/NoRouteToHostException` | `Host unreachable` |
| AddrInUse + Bind | `java/net/BindException` | `Address already in use` |
| HostLookup + Dns | `java/net/UnknownHostException` | `Unable to resolve host "<name>"` |
| Closed + any | `java/net/SocketException` | `Connection reset` / `Socket closed` |
| Other | `java/io/IOException` | `<op> failed (err <raw>)` |

Then per file (all failure sites listed in the survey table above):
- `socket.rs`: connect/send/recv mapped; recv returns `-1` **only** for
  `Ok(0)` EOF; stale-handle lookups in `helpers.rs::extract_socket_ptr`
  split: bad arg shape stays `InvalidReference`, null table lookup becomes
  `SocketException("Socket is closed")` (needs objects/strings threaded — the
  dispatcher `native_handler/net.rs` has `ctx` at every call site already).
- `server_socket.rs`: listen (Bind ctx), accept (Accept ctx).
- `datagram_socket.rs`: create (Bind), send (Send), receive (Recv — the
  −11 timeout finally surfaces as `SocketTimeoutException`).
- `http_connection.rs`: `native_connect` DNS→Dns ctx, connect→Connect ctx,
  send→Send; `parse_response_head`/`parse_status_code` →
  `ProtocolException("unexpected status line: …")` (malformed *server* data
  is not a JVM fault); `native_input_read`/`native_output_write` → Recv/Send;
  oversize request head (>512 B, :160-163) → `IOException("request header
  too large")`. Stale handles → `SocketException("Socket is closed")`.

### Phase 3b — SDK surface (source contracts)

`sdk/java/picodroid/net/` (throws clauses are javac-only: pico-jvm skips the
`Exceptions` attribute entirely — `class_file/parse.rs` reads only `Code` +
`LineNumberTable`; and shrink maps are class-name-only with `java/**`
keep-globbed, so **zero shrink/registry impact** — verified):

- `Socket`: `send`/`recv` gain `throws IOException` + honest javadoc
  (`-1` = end of stream; timeout → SocketTimeoutException). `connect`'s
  javadoc names the concrete types. **Ctor divergence, documented:**
  `Socket()` can throw IOException at runtime (resource exhaustion) but
  stays undeclared — matching `java.net.Socket()` which doesn't throw
  either; the alternative breaks every `new Socket()` outside a try.
- `ServerSocket(int)` → `throws IOException`; `accept()` →
  `throws IOException`. `DatagramSocket(int)` → `throws SocketException`;
  `send`/`receive` → `throws IOException` (all per `java.net`). No in-repo
  example uses these ctors, so no fallout.
- `HttpURLConnection.connect()`, `getResponseCode()`, `getInputStream()`,
  `HttpInputStream.read(...)`, `HttpOutputStream.write(...)` →
  `throws IOException` (java.net contract).
- **New, the biggest fidelity win:** `InetAddress.getByName(String host)
  throws UnknownHostException` — the SDK has no DNS entry point at all today
  (only `getByAddress(int,int,int,int)`); the HAL call exists
  (`dns_resolve`, used only by HTTP). New native → needs a dispatch arm in
  `native_handler/net.rs` **and** a `(class, method, descriptor)` row in
  `method_tables.rs::NET_HANDLED` — run the test and paste the row from the
  failure, don't transcribe (established procedure). `InetAddress` is
  already in `PICODROID_NATIVE_CLASSES`. Accept dotted-quad fast-path like
  the sim resolver does.

All new throws types exist in javac `--release 8` (java.net/java.io are in
ct.sym) — no SDK stub classes needed, same as `java.io.IOException` today.

**Source-incompat note for the next release cut:** existing apps calling
`send`/`recv`/HTTP methods without handling `IOException` stop compiling
against the new SDK. In-repo examples are updated in the same change
(`http_get` currently has **no catch at all** — a DNS failure today is an
uncatchable app-kill). Add to v0.13.0 release notes alongside the
`Socket.connect` throws from `a38d53c`.

### Phase 4 — examples, tests, docs

- **Examples**: `http_get` gets try/catch(IOException)/finally like netdemo
  (its class javadoc already documents the canonical loop); netdemo's catch
  can stay `IOException` (superclass catch now proves the hierarchy) but log
  `e.getClass()`… (no — `getClass().getName()` on alloc-by-name objects:
  verify `Object.getClass` works for classfile-less classes before using it
  in a demo; if unsupported, log message only).
- **Deterministic sim test for the roster** (`scripts/hil-tests.conf` — one
  new `term` row, runs in both shrink modes automatically): a tiny
  `netexception` example that (1) connects to `127.0.0.1:1` and asserts the
  catch clause entered is `ConnectException` (loopback refusal is
  kernel-deterministic, no listener/CI infra needed — sidesteps NET-7's
  external-host problem), (2) `ServerSocket` on an ephemeral port +
  `setTimeout(200)` + `accept()` → asserts `SocketTimeoutException` (fully
  local), (3) `getByName` on a dotted quad (no resolver dependency).
  Expected-pattern per `app|term|timeout|pat1;pat2` format. Device-only
  behaviors (FreeRTOS code mapping) are covered by the HW recipe instead —
  do NOT put DNS-failure or external-host cases in the sim roster
  (resolver-hijack/NET-7 flakiness).
- **JVM tests**: Phase 1's two tests.
- **Docs**: `website/src/content/docs/api/networking.md` — document the
  exception taxonomy per method (this page is user-facing API reference);
  `docs/networking-followups-2026-08.md` — close NET-9's error-mapping
  bullet, point here; note the `HttpInputStream.read` EOF/timeout fix.
  Website build (`npm run build`) validates links and is NOT in pre-commit;
  new pages need a sidebar entry (not needed here — existing pages only).

## Acceptance criteria

1. Sim `netexception` roster row green in both shrink modes:
   ConnectException on refused, SocketTimeoutException on accept timeout,
   getByName round-trip.
2. On `testbench_rp2350w` hardware: netdemo pointed at a **reachable host,
   closed port** logs `ConnectException: Connection refused` (host RSTs →
   −128 in Connect ctx); pointed at a **non-existent LAN IP** logs
   `SocketTimeoutException: connect timed out` (ARP fail → −116); http_get
   against a live URL still returns 200 end-to-end; http_get with an
   unresolvable host logs `UnknownHostException` (device DNS path).
   *Prerequisite landed 2026-08-15:* the closed-port case only reaches the
   app at all because of the vendored FreeRTOS+TCP fork fix (`e43e446f`,
   see "Vendored FreeRTOS+TCP is now a fork" in
   `docs/networking-followups-2026-08.md`) — pristine V4.4.1 leaves
   `FreeRTOS_connect` asleep forever on RST-during-SYN, and upstream `main`
   still has the bug. Do not "upgrade" the submodule past the fork without
   re-carrying that patch.
3. `catch (IOException e)` continues to catch all of the above (hierarchy);
   `catch (SocketException e)` catches ConnectException but NOT
   SocketTimeoutException (real-Java quirk, asserted in the jvm unit test).
4. A slow-but-alive HTTP server no longer reads as clean EOF on device
   (recv-contract fix) — verifiable with `nc -l` that accepts and stalls:
   read now throws `SocketTimeoutException` after `setTimeout`.
5. `./scripts/test.sh`, sim smoke ×3, `./scripts/pre-commit` green; HIL
   suite 49/49 (no networking rows yet, but the shared HAL types touch
   every board build).

## Traps for the executing session (hard-won, all verified)

- **Formatter strips just-added unused imports**: adding `import
  java.io.IOException;` and the `throws` that uses it must land in ONE edit
  per file, or the on-save hook deletes the import between edits (cost a
  build cycle in `a38d53c`).
- **`InvalidReference` is uncatchable** — never "temporarily" leave an I/O
  path on it during the migration; apps die with no stack.
- **Uncaught-banner limitation**: `intern_dyn` messages do NOT appear in the
  `UncaughtException` banner (`jvm/src/types.rs:92-93` — only
  `resolve_static` messages do). Catch blocks see them via `getMessage()`.
  Don't burn time "debugging" an empty banner message. (Optional follow-up:
  the EIIE wrapper pattern at `interpreter/mod.rs:229-239` shows how to
  propagate dyn messages if someone wants banner parity.)
- **Two errno spaces**: sim `raw` is a positive host errno, device `raw` is
  a negated FreeRTOS errno. The `kind` is the contract; `raw` is for the
  `(err N)` suffix only. Never match on `raw` above the HAL.
- **HW validation mechanics** (from the 2026-08-15 session): flash.sh
  defaults to the **debug** profile — `probe-rs attach` with the release ELF
  silently decodes zero RTT; `pdb install` still works, so silence = ELF
  mismatch, not a dead board. Creds via gitignored `.wifi-creds.env`
  (`env $(grep -v '^#' .wifi-creds.env | xargs) …`). Shrink flag is now a
  per-invocation Gradle property (`81b6b7c`) — papk/firmware
  `framework-map-version` must match or `pdb install` refuses. The app runs
  once at boot: `probe-rs reset`, then attach immediately, or reinstall via
  pdb and attach right after "Install complete".
- **New native = registry row**: only `InetAddress.getByName` adds a native;
  paste its `NET_HANDLED` row from the `method_tables` test failure. The
  `java/net/*` exception classes themselves need NO registry/shrink/keep
  work (alloc-by-name; `java/**` is keep-globbed; maps are class-only).
- `datagram_socket.rs:96` and `socket.rs:121` discard `arrays.store`'s
  result — fix opportunistically while in the files (http_connection already
  does `.ok_or(...)`).

## Out of scope (explicitly)

- Stream-based `Socket` API (`getInputStream()`/`getOutputStream()`) — the
  simplified send/recv surface stays.
- NET-7 (networking rows in nightly HIL/sim against external hosts) — the
  loopback-deterministic sim test is a partial down-payment only.
- `socket_table` 32-bit `remove()` no-op and 256-byte I/O chunking (other
  NET-9 bullets).
- TLS/HTTPS, WPA3 (NET-8), banner dyn-message propagation.

## Suggested execution order

Phase 1 (JVM, self-contained, testable via `./scripts/test.sh`) → Phase 2
(HAL, compiles everywhere incl. `test_platform`; sim + device buildable
before any consumer changes) → Phase 3 + 3b together (natives + SDK + both
examples in one tree state, since throws clauses force the example updates)
→ Phase 4 (tests/roster/docs) → sim validation → HW validation → pre-commit.
Roughly: 1 session, with HW validation the long pole.
