# pd-pdb

The device half of PDBP, the picodroid debug bridge, as a leaf crate:
resync on the frame magic, read a command byte and its length, refuse a
length the verb cannot carry, dispatch, check the CRC, answer. `serve_one`
handles one frame; `run_pdb_task` loops over it for ever and resets after a
committed install or uninstall.

Everything the protocol cannot know arrives through traits:

| Trait | From | Supplies |
|---|---|---|
| `PdbTransport` | the family | the byte pipe (USB CDC on the RP boards, a socket in the simulator) |
| `pd_install::PapkFlash`, `CoreCoordinator` | the family | the app region and parking the JVM core |
| `SysmonSource` | the family | task statistics |
| `PdbDevice` | the firmware (`picodroid-core`) | the board name, the serial, the framework-map version, input injection, the package list, uninstall targets |

`pdb-protocol` holds the wire constants both ends agree on; this crate is
the loop that uses them, moved out of `picodroid-core` so the host can test
it and fuzz the byte stream (`docs/fuzzing.md`, target `pdb_stream`). Core
re-exports the public items under `picodroid_core::pdb`.

```sh
cargo test -p pd-pdb --features vec-pipe --target "$(rustc -vV | sed -n 's/^host: //p')"
```
