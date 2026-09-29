# Follow-up backlog: TLS — 2026-09-27

TLS 1.3 landed on `main` on 2026-09-27 (`38a6d916`, merged as `67b75cda`): `HttpsURLConnection`,
`SntpClient`, the `javax.net.ssl` exceptions, `crates/pd-tls` over our fork of `embedded-tls`
under `third_party/embedded-tls`, the `askclaude` and `https_get` examples and the nightly's
TLS listener. Board-verified on `pico_display2_w`; design and every number in
[designs/tls-2026-09.md](designs/tls-2026-09.md).

What remains is **follow-up work, not blockers**: each item below is self-contained, with
its evidence and where to start. Status lines are kept here as items close.

Completed items: [completed/tls-follow-ups.md](completed/tls-follow-ups.md) — TLS-3, TLS-10.

## TLS-1: Push to origin and watch CI

**Status: open.** `main` was two commits ahead of `origin/main` when this was written; the
repo rule is push only when asked. After the push, `gh run list --limit 3` shows CI; the
`framework` sim-smoke shard now carries `https_get` (its public-host legs are logged, not
asserted, and its local legs need only the runner's own `scripts/tls-listener.py`).

## TLS-2: The first nightly

**Status: open.** The 3 AM `sim-run` gained the `https_get` and `askclaude` `net` rows; the
4 AM `hil-fleet` runs `https_get` on `testbench_rp2350w` against the bench's TLS listener
(started by `net-lib.sh::start_net_listeners`, with the host's LAN IP in the leaf's
`iPAddress` SANs). `askclaude` is SKIPped on hardware by design — its `test.ctrl` presses
buttons through the simulator's control channel, which `hil-run` cannot drive. Where to look
if a row fails: `build/sim/results/`, `build/hil/results/`, the row's `net-tls.log`.

## TLS-4: Flash

**Status: open, decision needed.** The shipped client costs **328,640 B** of flash on the W
boards (`pico_display2_w` release, helloworld image: 1,587,892 → 1,916,532 B, the program
region at 91 %); static RAM is unchanged. RSA is 56,656 B of it; P-384 (fiat-crypto with
64-bit limbs, unrolled for a 32-bit core) and its SHA-512 are 109 KB. Per-package
`opt-level = "s"` does nothing under fat LTO (1.8 KB). The levers, from
`designs/tls-2026-09.md` §7:

- A Cortex-M assembly P-384 (`mcu-crypto-asm` publishes audited P-256/P-384 kernels with a
  Rust fallback): most of the 81 KB back, and a faster handshake.
- A per-board opt-out of RSA (a `tls_rsa = false` key driving `pd-tls`'s `rsa` feature):
  57 KB, losing every Let's Encrypt RSA site. Not wired.
- Anchoring at P-256 intermediates instead of the P-384 roots: 109 KB, at the cost of
  re-shipping the store on every intermediate rotation. Rejected while the roots fit.

The app-store roadmap's slot arithmetic (S1) must count TLS at this size.

## TLS-5: A live `askclaude` run

**Status: open.** The nightly proves the app against the listener's canned
`/v1/messages`; a run against the real API needs a key and was not done:

```bash
PICODROID_ANTHROPIC_API_KEY=sk-ant-api03-... ./scripts/flash.sh --app askclaude --board pico_display2_w
```

Use a workspace-scoped, spend-capped Console key (`examples/askclaude/README.md`); it is
baked into the papk. Expect a 1.2 s handshake plus the model's latency; the reply is
capped at 150 tokens.

## TLS-6: Runtime key provisioning (`pdb prefs set`)

**Status: open, optional.** The API key is a `picodroidBuildConfig` constant, so it sits in
the papk and in flash in the clear. A `pdb prefs set <package> <file> <key> <value>` verb
(`tools/pdb` has no file or preference write today; `SharedPreferences` is the `PPRF v1`
format in `sdk/java/picodroid/content/SharedPreferences.java`) would let a key be pushed over
USB only and forgotten from the board. Would also close the claudeusage README's "pdb can set
`bridge_host`" gap.

## TLS-7: Trim the handshake task's stack

**Status: open, optional.** The handshake runs on a transient `tls-handshake` task with a
40 KB stack; the measured peak is 32,760 B on every chain (P-256, P-384 and RSA alike),
12,752 B for a chain refused before the inner verifier — so the peak is `rustpki`'s DER
decode (two `DecodedCertificate`s and the derived `Decode` frames), not the arithmetic.
Boxing the two decoded certificates in `third_party/embedded-tls/src/pki.rs::verify_certificate`
(our fork; `alloc` is already on for RSA) should cut it well below 32 KB. Every handshake logs
`tls: handshake stack 40960 B, N B unused` on the device — re-measure before lowering
`HANDSHAKE_STACK_BYTES` in `crates/picodroid-core/src/net/tls.rs`.

## TLS-8: `picoenvmon`'s weather over HTTPS

**Status: open, small.** `examples/picoenvmon/java/picoenvmon/net/WeatherFetcher.java` still
fetches `http://api.open-meteo.com`; the RSA chain under ISRG Root X1 now verifies (1,991 ms
on the board). Switching the scheme needs an SNTP sync first (the app already has one) and
a `Thread` or the network thread it runs on already; the Kotlin twin follows.

## TLS-9: The fork of `embedded-tls`

**Status: decided 2026-09-27.** `third_party/embedded-tls` is our own fork of 0.19.0 with
three marked `PICODROID` changes (`set_ca`, no `unwrap` on a malformed RSA key, `iPAddress`
SAN matching); no upstream pull requests are planned. A future bump re-applies the patches
listed in its `README-PICODROID.md`; the crate's tests and examples are not vendored.

