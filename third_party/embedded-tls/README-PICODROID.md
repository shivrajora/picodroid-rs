# embedded-tls, vendored

Upstream: https://github.com/embassy-rs/embedded-tls — crates.io `embedded-tls` **0.19.0**
(Apache-2.0, see LICENSE). Vendored 2026-09-27 for `crates/pd-tls`, which the root
`Cargo.toml` points here through `[patch.crates-io]`. Tests, examples and CI files are
dropped; `src/` is upstream plus the marked `PICODROID` changes below:

1. `CertVerifier::set_ca(&mut self, ca)` — swap the trust anchor of a built verifier.
   The verifier holds an owned copy of the whole received chain (`Certificate<CERT_SIZE>`,
   about 8 KB), so it lives on the heap; `pd_tls::PdVerifier` keeps one and picks the
   anchor per handshake once the chain names its issuer.
2. `RsaPublicKey::from_pkcs1_der(...).unwrap()` in the `RsaPssRsaeSha256` branch of
   `verify_signature` returns `TlsError::DecodeError` instead of panicking on a malformed
   server key.

3. IP-literal hosts: `der_certificate.rs` gains `extract_san_ip_addresses` (the
   `iPAddress` SAN entries) and `pki.rs::tls_hostname_match` matches a dotted-quad
   host against them only, per RFC 6125 §6.4.4. The bench rows dial the test host
   by IP. (`pd_tls::session` then sends no SNI for such a host, as RFC 6066 asks.)
4. `TlsConnection::close_notify(&mut self)` in `blocking.rs`, the body of `close` without
   the move: `close(self)` carries the connection (key schedule, record state) through the
   caller's stack, and a picodroid pool worker closing a session after a fetch overflowed
   an 8 KB stack in the record write (2026-09-27). `pd_tls::TlsSession::close` sends the
   alert in place and drops the boxed connection.

This copy is picodroid's own fork of the crate for now: the patches stay here and are
carried across any future bump; no upstream pull requests are planned.
