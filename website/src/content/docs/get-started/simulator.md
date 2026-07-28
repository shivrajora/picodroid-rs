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

The simulator builds with `--features sim` and runs natively on the host. Hardware calls (GPIO, UART, I2C, SPI, ADC, PWM) are stubbed with logged output. File I/O (`picodroid.io`) and `picodroid.content.SharedPreferences` are backed by a host-file LittleFS image so writes persist across sim runs. Networking (`picodroid.net`) is backed by the host network stack. Display apps (e.g. `displaydemo`) open a graphical window with mouse-as-touch input.

## Running a UI demo

The window-based demos (`displaydemo`, `dragdemo`, `keydemo`, `pickerdemo`, `swipedemo`, etc.) open a 320×240 window. Mouse drag is treated as touch.

If you're driving the sim from a script (e.g. for end-to-end tests), prefer `xdotool mousedown / sleep 0.3 / mouseup` over `xdotool click 1` — minifb at 60 Hz misses very fast clicks.

## Sim vs. hardware: where they differ

- **Networking** — hits the host stack rather than cyw43 + FreeRTOS+TCP. HTTP / TCP / UDP code that runs on the sim should run on the device, but the latency is wildly different.
- **Display** — minifb-backed window vs. ST7789 over SPI. LVGL is the same; rendering paths are not.
- **GPIO / PWM / ADC / UART** — stubbed; reads return zero, writes log to stdout. Use the sim for app-logic verification, not bus-level work.
- **Touch** — the sim feeds minifb mouse position through the **same `Xpt2046` driver** that runs on hardware (so calibration / `swap_xy` behave identically), rather than stubbing it.
- **Sensors / I2C** — the sim answers I2C sensor reads with a fake BME688 (and synthesizes LTR559 readings) instead of returning zeros, so sensor-driven UI works on the host. The real drivers still run on-device.
- **Threads** — `Thread.start()` is a **no-op in the sim** (there's no FreeRTOS) and logs a warning naming the Runnable, so its body never runs. On device it spawns a real task. Use `Executors.mainExecutor()`/`backgroundExecutor()` for work that must run in both.
- **Pointer width** — your machine is 64-bit and the RP2040/RP2350 are not. Anything whose behavior depends on the size of a pointer differs by default: object graphs occupy more heap than they will on device, and the LVGL handle table takes a safer path than the one hardware ships. Run with `--arch arm32` (below) when that matters.

## Matching the device's pointer width

`--arch arm32` builds the simulator for 32-bit ARM Linux and runs it under qemu, so `usize` is 4 bytes and the code is ARM:

```bash
./scripts/sim.sh --arch arm32 --app myapp
```

It needs a cross toolchain and an emulator, once:

```bash
sudo apt-get install -y qemu-user-static gcc-arm-linux-gnueabihf libc6-dev-armhf-cross
rustup target add armv7-unknown-linux-gnueabihf
```

Reach for it when you are chasing something the native sim cannot show you: heap figures that should match the device's, arithmetic on 32-bit offsets, or a widget handle used after the widget was deleted (at 32 bits the sim compiles the same raw-pointer handle path the devices run, where such a lookup reads freed memory instead of being caught). Rendering is identical either way — the software renderer produces the same framebuffer bytes on both.

It runs several times slower than native and headless by default, so it complements the native sim rather than replacing it. `./scripts/test.sh --arch arm32` runs the unit tests the same way.

## Slow-handler watchdog

The main loop warns when a single handler — widget-event dispatch, a posted Runnable, or the pending-op drain (a big `onCreate`) — overruns the threshold and stalls the UI tick. The default is 50 ms; set `PICODROID_SLOW_HANDLER_MS` to tune it (`0` disables) without a rebuild:

```bash
PICODROID_SLOW_HANDLER_MS=20 ./scripts/sim.sh --app myapp
```

It ships on device too, where the threshold is the compile-time default.

## Filtering logs

`pdb logcat --stdin` filters the sim's `[Tag] msg` output (or piped, already-decoded device logs) by tag and level:

```bash
./scripts/sim.sh --app myapp | pdb logcat --stdin --tag MyApp --level W
```

## Filesystem persistence

The sim's LittleFS image lives at `platforms/rp/target/sim-fs.img` (override with the `PICODROID_SIM_FS` env var) — same wire format as on-device flash, so you can copy it onto a device for inspection (or vice versa). Boot count + persistence checks via `bootcount` work identically.
