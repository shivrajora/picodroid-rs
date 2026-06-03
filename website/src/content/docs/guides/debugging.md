---
title: "Debugging"
description: "RTT logging, the host simulator, pdb sysmon, and on-device GDB."
---

## RTT Logging

`flash.sh` flashes the firmware and streams RTT log output via [defmt](https://defmt.ferrous-systems.com/) and probe-rs. Log levels are controlled by `DEFMT_LOG` (set to `debug` by default in `.cargo/config.toml`).

## Host Simulator

The host simulator lets you run apps on your development machine without hardware. Hardware calls are stubbed with logged output, making it useful for testing app logic and debugging JVM behaviour.

```bash
./scripts/sim.sh --app helloworld
./scripts/sim.sh --app blinky          # loops forever — Ctrl-C to stop
./scripts/sim.sh --app benchmark       # JVM performance benchmark (host-only)
./scripts/sim.sh --app gcstress        # GC stress test (host-only)
./scripts/sim.sh --app displaydemo     # opens a 320x240 graphical window
```

For display apps, the simulator opens a graphical window (via minifb) that renders the LVGL widget tree with mouse-as-touch input. Close the window or press Escape to exit.

## System Monitor (pdb sysmon)

The `pdb sysmon` command queries runtime system health over the device's USB CDC port without reflashing or adding debug prints:

```bash
pdb -s /dev/cu.usbmodem102 sysmon
```

This reports:

- **Heap**: free bytes, minimum-ever free bytes (high-water mark)
- **Uptime**: tick count and wall-clock seconds
- **Task table**: every FreeRTOS task with name, state, priority, stack high-water mark, and CPU %

CPU % is computed from the delta between consecutive queries — run it twice with a few seconds in between. The first query shows CPU % as N/A.

Under the hood this uses `xPortGetFreeHeapSize()`, `xPortGetMinimumEverFreeHeapSize()`, and `uxTaskGetSystemState()` from FreeRTOS, with run-time stats driven by the hardware microsecond timer (TIMERAWL register). There is no background sampling task — stats are collected on-demand when the host sends the query, so there is zero impact on power consumption or scheduling.

## GDB

GDB debugging is a two-terminal workflow. First, start probe-rs as a GDB server (it listens on `localhost:1337` by default):

```bash
# RP2040
probe-rs gdb --chip RP2040

# RP2350
probe-rs gdb --chip RP2350
```

Then, in a second terminal, launch GDB against the ELF and connect to the server:

```bash
# RP2040
arm-none-eabi-gdb target/thumbv6m-none-eabi/debug/picodroid \
    -ex "target remote localhost:1337"

# RP2350
arm-none-eabi-gdb target/thumbv8m.main-none-eabihf/debug/picodroid \
    -ex "target remote localhost:1337"
```
