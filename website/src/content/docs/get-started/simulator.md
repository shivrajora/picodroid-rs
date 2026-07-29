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

The simulator builds with `--features sim` and runs natively on the host, with the **real FreeRTOS kernel** compiled in (its POSIX port) — so tasks, `synchronized`, the UI tick and `Thread.start` are the device's scheduler rather than a host-thread model of it. Hardware calls (GPIO, UART, I2C, SPI, ADC, PWM) are stubbed with logged output. File I/O (`picodroid.io`) and `picodroid.content.SharedPreferences` are backed by a host-file LittleFS image so writes persist across sim runs. Networking (`picodroid.net`) is backed by the host network stack. Display apps (e.g. `displaydemo`) open a graphical window with mouse-as-touch input.

## Running a UI demo

The window-based demos (`displaydemo`, `dragdemo`, `keydemo`, `pickerdemo`, `swipedemo`, etc.) open a 320×240 window. Mouse drag is treated as touch.

If you're driving the sim from a script (e.g. for end-to-end tests), prefer `xdotool mousedown / sleep 0.3 / mouseup` over `xdotool click 1` — minifb at 60 Hz misses very fast clicks.

## Sim vs. hardware: where they differ

- **Networking** — hits the host stack rather than cyw43 + FreeRTOS+TCP. HTTP / TCP / UDP code that runs on the sim should run on the device, but the latency is wildly different.
- **Display** — minifb-backed window vs. ST7789 over SPI. LVGL is the same; rendering paths are not.
- **GPIO / PWM / ADC / UART** — stubbed; reads return zero, writes log to stdout. Use the sim for app-logic verification, not bus-level work.
- **Touch** — the sim feeds minifb mouse position through the **same `Xpt2046` driver** that runs on hardware (so calibration / `swap_xy` behave identically), rather than stubbing it.
- **Sensors / I2C** — the sim answers I2C sensor reads with a fake BME688 (and synthesizes LTR559 readings) instead of returning zeros, so sensor-driven UI works on the host. The real drivers still run on-device.
- **Threads** — `Thread.start()` runs, as a real FreeRTOS task, and `Executors.backgroundExecutor()` gets the same four worker tasks the device has. The difference that remains is **cores**: the simulator's kernel is single-core where the chip has two, so races that need genuine parallelism are still hardware-only. Sleeps quantise to the 1 ms tick, as on device.

## Threads and the scheduler

Because the kernel is real, threaded apps behave here the way they do on the board:

```bash
./scripts/sim.sh --app threaddemo
```

Each Java thread is a FreeRTOS task with the device's 16 KiB stack charged from the simulated heap and released when it exits, `synchronized` uses the kernel's recursive mutexes, and the filesystem runs on the same worker task the device uses.

Two caveats. The kernel is **single-core**, where the chip is dual-core, so cross-core interleavings remain hardware-only — the simulator will not invent a race the hardware cannot produce, but the hardware can produce ones it will not show you. And a thread whose `run()` returns leaves its (host-side, uncounted) task parked rather than freeing it, so an app churning tens of thousands of threads will run the host out of them.

See `docs/designs/freertos-host-sim.md` for the design.

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
