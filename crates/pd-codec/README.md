# pd-codec

The pure decoders `picodroid-core` trusts, as a leaf crate: no HAL, no JVM,
no statics, `no_std`. Core re-exports every item under the path it had
before, so callers never see this crate; it exists so the host can test and
fuzz these parsers without the LVGL build behind core
(`docs/designs/fuzzing-2026-10.md` §8).

| Module | What it reads | Fed by |
|---|---|---|
| `sntp` | The 48-byte SNTP reply (RFC 4330) | A UDP datagram off the network |
| `time` | The `/system/time` record and `ZoneId`-style offset ids | The store; `AlarmManager.setTimeZone` |
| `wifi` | The `/system/wifi` credential record (CRC-32 checked) | The store |
| `serial` | The device serial: a chip ID as hex, or a stored ASCII string | The chip; `/system/serial` |
| `sandbox` | An app's path into its `/data/<package>` directory | Every storage native |
| `inflate_scan` | One compiled-layout node and the `LayoutParams` arithmetic | A PAPK's RESOURCES section |

Each decoder refuses rather than guesses: a short, out-of-range or
CRC-dirty record is `None`, and the caller falls back to its default.

```sh
cargo test -p pd-codec --target "$(rustc -vV | sed -n 's/^host: //p')"
```

The fuzz targets `sntp`, `sysfmt`, `sandbox` and `layout_node` in
`crates/fuzz-targets` drive these on arbitrary input (`docs/fuzzing.md`).
