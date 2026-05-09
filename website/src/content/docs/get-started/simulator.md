---
title: "Host simulator"
description: "Run any Picodroid app on your dev machine without hardware."
---

Run any app on the host machine without hardware using the simulator:

```bash
./scripts/sim.sh --app helloworld
./scripts/sim.sh --app blinky          # loops forever — Ctrl-C to stop
./scripts/sim.sh --app uart --release
```

The simulator builds with `--features sim` and runs natively on the host. Hardware calls (GPIO, UART, I2C, SPI, ADC, PWM) are stubbed with logged output. File I/O (`picodroid.io`) and `picodroid.content.Preferences` are backed by a host-file LittleFS image so writes persist across sim runs. Networking (`picodroid.net`) is backed by the host network stack. Display apps (e.g. `displaydemo`) open a graphical window with mouse-as-touch input.

## Running a UI demo

The window-based demos (`displaydemo`, `dragdemo`, `keydemo`, `pickerdemo`, `swipedemo`, etc.) open a 320×240 window. Mouse drag is treated as touch.

If you're driving the sim from a script (e.g. for end-to-end tests), prefer `xdotool mousedown / sleep 0.3 / mouseup` over `xdotool click 1` — minifb at 60 Hz misses very fast clicks.

## Sim vs. hardware: where they differ

- **Networking** — hits the host stack rather than cyw43 + FreeRTOS+TCP. HTTP / TCP / UDP code that runs on the sim should run on the device, but the latency is wildly different.
- **Display** — minifb-backed window vs. ST7789 over SPI. LVGL is the same; rendering paths are not.
- **GPIO / PWM / ADC / SPI / I2C / UART** — stubbed; reads return zero, writes log to stdout. Use the sim for app-logic verification, not bus-level work.
- **Sensors** — the BME688 and LTR559 drivers run on the device only.

## Filesystem persistence

The sim's LittleFS image lives at `target/picodroid/sim_lfs.bin` — same wire format as on-device flash, so you can copy it onto a device for inspection (or vice versa). Boot count + persistence checks via `bootcount` work identically.
