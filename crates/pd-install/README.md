# pd-install

The sequence that puts a PAPK app image into a NOR flash app region, as a
`no_std` library generic over everything device-specific:

| Parameter | What it supplies |
|---|---|
| `InstallTransport` | bytes off the wire, and ready / success / error reports |
| `CoreCoordinator` | park and release the core that executes from the flash being erased |
| `PapkFlash` (or `PapkRegionFlash` via `PapkRegion`) | erase, program, commit and reset for the app region |
| `PackageDirectory` | which runs are installed, where the next one goes, compaction |

`install` validates size, parks the core, peeks the manifest for the
framework-map compatibility gate and the package name, asks the directory for
a `Plan`, and only then erases. It streams 256-byte pages with an incremental
CRC, writes the boot-meta pages last so a run is either whole or invisible,
and evicts a superseded copy only after the new one commits. `uninstall`
erases a run the same way.

The `mem-region` feature adds an in-memory NOR model with real flash
semantics for host tests and simulators.

```sh
cargo test -p pd-install
```
