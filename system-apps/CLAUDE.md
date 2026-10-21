# system-apps/

The launcher and settings: ordinary picodroid apps that every multi-app board links in (`PICODROID_SYSTEM_APKS=launcher:settings` in the sim, `--system-apps`). They are built like `examples/*` and held to the same rules (`.claude/rules/java-sdk.md`), plus:

- **They boot on every board**, so they build their screens in `onCreate` within the 250 ms build budget (`docs/designs/ui-build-budget-2026-10.md`) and use `ListView` for data-driven settings lists (it keeps a window of rows alive, however many items).
- **A system package is special to the installer:** `papk-pack --repack … --package-name picodroid.settings` makes a `SystemPackage` fixture; the launcher's boot sweep runs when system APKs are set.
- **Settings own `/system/*`:** `/system/wifi` (the saved network, rejoined at boot; build-time credentials override it), `/system/time` (the zone; `Settings.Global.AUTO_TIME`). Changing a record's layout means a `pd-codec` decoder change and a fuzz target (`sysfmt`, `records`).
- **Verify in the sim** with the launcher booted: `PICODROID_BOOT=launcher`, drive with `sim-ctrl.sh` (`input keyevent 19|20|23|4`), capture with `sim-shot.sh`; on a board, `pdb.sh --board X list|install|uninstall|input keyevent N`.
- **Pre-commit builds every system APK** beside the app PAPK for the firmware lanes; a Java edit here runs the formatter and the name-table tests like an SDK change.
