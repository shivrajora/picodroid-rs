// SPDX-License-Identifier: GPL-3.0-only
//! Simulator debug-bridge stub.
//!
//! Deliberately empty: the simulator ships no PDB *endpoint* — no transport,
//! no command loop serving a host. The module exists only so `hal::pdb_usb`
//! resolves.
//!
//! That is a statement about the endpoint, not the protocol. Since the
//! stage-3 extraction (`docs/designs/family-neutral-residue.md`) the whole
//! bridge — command loop, framing, install orchestration, sysmon, input —
//! compiles on the host in `crate::pdb` and `crate::install`, tested against
//! mock transports; the payload layouts live in `pdb-protocol` beside the
//! decoders `tools/pdb` uses. Wiring a real sim endpoint (a `PdbTransport`
//! over TCP or a pty) was considered and deferred — see
//! `docs/designs/pdb-schema-as-code.md` §3 for the analysis and the scoped
//! v1, so it need not be re-derived.
