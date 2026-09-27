# pd-tls

TLS 1.3 client pieces for picodroid, built on the vendored `embedded-tls`
(`third_party/embedded-tls`, `rustpki` chain verification, P-256/P-384 and,
behind the default `rsa` feature, RSA):

- `anchors` — the compiled-in trust store. `build.rs` turns every
  `roots/*.der` (Mozilla-bundle roots, extracted by `scripts/tls-roots.sh`)
  plus the optional `PICODROID_TLS_EXTRA_CA=<file.der>` (the nightly's test
  CA) into an `Anchor` with its Subject TLV precomputed.
- `verifier` — `PdVerifier`: picks the anchor a server chain is issued
  under, cuts the chain there (public chains end in a cross-signed copy of
  the root, which the single-CA verifier upstream would reject) and fails
  closed when the wall clock is unset.
- `session` — `TlsSession<S>`: one heap-resident connection over any
  blocking `embedded_io` socket, record buffers included (16 KB read, 4 KB
  write); `open` runs the handshake with a ChaCha20 RNG seeded by the caller.

picodroid-core's `net::tls` supplies the socket, the wall clock and the
entropy; this crate has no HAL or JVM dependency so its tests run on the host.
