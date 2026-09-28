# Completed: Follow-up backlog: TLS — 2026-09-27

Items closed out of [tls-follow-ups.md](../tls-follow-ups.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

## TLS-3: Cut the next shrink map

**Status: closed 2026-09-27.** Map v0.35.0, cut on `main` for the v0.35.0 release, names
`HttpsURLConnection`, `SntpClient`, `getCipherSuite` and the three `javax/net/ssl` exceptions.

## TLS-10: Host build flags

**Status: done, for the record.** `.cargo/config.toml` now forces the software AES and
POLYVAL backends for every host build (`--cfg aes_force_soft --cfg polyval_force_soft`),
because the x86 intrinsics backends keep 16-byte-aligned state that the simulator's
allocator refuses on behalf of the device heap (parity-audit MEM-05). `pd-tls` has an
`align_of` test that fails without them. A new crypto dependency must pass that test before
anything it holds is boxed.
