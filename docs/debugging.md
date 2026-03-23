# Debugging

`cargo run` flashes the firmware and streams RTT log output via [defmt](https://defmt.ferrous-systems.com/) and probe-rs. Log levels are controlled by `DEFMT_LOG` (set to `debug` by default in `.cargo/config.toml`).

For GDB debugging, run probe-rs in GDB server mode and connect with:

```bash
# RP2040
arm-none-eabi-gdb target/thumbv6m-none-eabi/debug/picodroid

# RP2350
arm-none-eabi-gdb target/thumbv8m.main-none-eabihf/debug/picodroid
```
