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

## Choosing a board

The simulator simulates a board, `testbench_rp2350` unless `--board` names another. The board decides the window (its panel's size), the input (touch, buttons, or both), the heap cap (the chip's FreeRTOS arena) and which framework classes exist, exactly as its `board.toml` does for the firmware:

```bash
./scripts/sim.sh --board pico_enviro_mon --app picoenvmon      # 240×240, four buttons, no touch
./scripts/sim.sh --board testbench_rp2350w --app netdemo       # a board with a network
./scripts/sim.sh --board pico_touch_kit --app tutorial_screens # 320×480, touch, BACK and HOME
```

Two things follow from the default board. It has a touch panel and **no buttons**, so there is no BACK key: an app that needs one runs on a board that has it. And it has **no network**: `picodroid.net` needs a WiFi board (`testbench_rp2350w`, `pico_enviro_mon_w`, `pico_display2_w`, `pico_touch_kit`).

## Running a UI demo

The window-based demos (`displaydemo`, `dragdemo`, `keydemo`, `pickerdemo`, `swipedemo`, etc.) open a window the size of the board's panel — 320×240 on the default board — drawn at twice that size unless the panel is too tall for it. Mouse drag is treated as touch. Close the window or press Escape to exit.

On a board with buttons the host keyboard presses them:

| Key | Button |
|-----|--------|
| ↑ / ↓ | PREV / NEXT (move the focus) |
| Enter | ENTER (activate the focused view) |
| Backspace | ESC, which is BACK |
| 1 – 4 | the board's first to fourth button, in `board.toml` order |

If you're driving the sim from a script (e.g. for end-to-end tests), prefer `xdotool mousedown / sleep 0.3 / mouseup` over `xdotool click 1` — minifb at 60 Hz misses very fast clicks. Better still, use the input verbs of [`pdb -s sim input`](#driving-the-simulator-with-pdb) or of the control channel, which need no window at all — see [Driving the simulator headlessly](/guides/debugging/#driving-the-simulator-headlessly).

## The launcher and several apps

`sim.sh --app X` runs one app and exits when it finishes, the app's last Activity closing included. `--system-apps` also builds the system apps (the launcher and settings) and loads them as a multi-app firmware carries them, so the launcher takes over when the app finishes and BACK on the app's root screen goes home instead of ending the run:

```bash
./scripts/sim.sh --app blinky --system-apps
PICODROID_BOOT=launcher ./scripts/sim.sh --app blinky --system-apps   # boot the launcher first
```

`PICODROID_BOOT` is `flash.sh --boot` (`app`, `launcher`, or a package name), read when the simulator starts. `PICODROID_SIM_APPS` is a colon-separated list of already built `.papk` files to install beside the app under test; [`pdb -s sim install`](#driving-the-simulator-with-pdb) adds one while the simulator runs. See the [launcher guide](/guides/launcher/).

## Sim vs. hardware: where they differ

- **Networking** — hits the host stack rather than cyw43 + FreeRTOS+TCP. HTTP / HTTPS / TCP / UDP code that runs on the sim should run on the device, but the latency is wildly different. The link is up from the start; `net down` / `net up` on the control channel (typed into the simulator's terminal, or sent with `./scripts/sim-ctrl.sh`) drop and restore it, and `PICODROID_SIM_NET=down` starts with it down. Settings → Wi-Fi scans a canned list of access points. See [WiFi & networking setup](/get-started/networking/#the-simulators-wifi).
- **Wall clock** — counts from boot, as on a board, until the app sets it; `PICODROID_SIM_WALL_CLOCK=1` anchors it to the host's clock, which an HTTPS handshake needs.
- **Display** — minifb-backed window vs. an ST7789 or ST7796 panel over SPI. LVGL is the same; rendering paths are not.
- **GPIO / PWM / ADC / UART** — stubbed; reads return zero, writes log to stdout. Use the sim for app-logic verification, not bus-level work.
- **Touch** — the sim feeds minifb mouse position through the **same touch driver** that runs on hardware, over a fake bus — `Xpt2046` on the testbenches (so calibration / `swap_xy` behave identically), `Gt911` on `pico_touch_kit` — rather than stubbing it.
- **Sensors / I2C** — the sim answers I2C sensor reads with a fake BME688 (and synthesizes LTR559 readings) instead of returning zeros, so sensor-driven UI works on the host. The real drivers still run on-device.
- **Threads** — `Thread.start()` runs, as a real FreeRTOS task, and `Executors.backgroundExecutor()` gets the same worker tasks the simulated board has (four by default; `[background_pool]` in `board.toml`). The difference that remains is **cores**: the simulator's kernel is single-core where the chip has two, so races that need genuine parallelism are still hardware-only. Sleeps quantise to the 1 ms tick, as on device.

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

## Driving the simulator with pdb

The simulator is a [`pdb`](/reference/pdb-commands/) device. It runs the same debug-bridge task a board runs, on a Unix socket instead of USB CDC, and prints the socket at boot:

```text
[sim] pdb: listening on /tmp/picodroid-sim/pdb-2696762.sock
```

`./scripts/pdb.sh -s sim <command>` talks to the one simulator that is running; `pdb devices` lists every one (rows ending in `[sim]`), and `-s <socket>` names one when several run side by side — each simulator has its own socket (`pdb-<pid>.sock` under the temp dir, or `PICODROID_SIM_PDB_SOCKET`). `ping`, `list`, `sysmon`, `input` and `install`/`uninstall` all work; an install parks the JVM, writes the app region and *reboots* the simulator — it restarts its own process from a dump of the region, a warm boot that runs the real boot path — and `pdb` sees it come back.

A simulator with nothing left to run exits (one app per process is what `sim.sh --app X` means); with `--system-apps` the launcher keeps it up, and `PICODROID_SIM_WAIT_FOR_INSTALL=1` makes it wait for an install instead, as a device does. A cold `sim.sh` start always begins from an erased app region — only the reboot after an install carries the region over.

## Filesystem persistence

The sim's LittleFS image lives at `crates/picodroid-core/target/sim-fs.img` (override with the `PICODROID_SIM_FS` env var) — same wire format as on-device flash, so you can copy it onto a device for inspection (or vice versa). Boot count + persistence checks via `bootcount` work identically. The default image is shared by every simulator on the machine; give each its own `PICODROID_SIM_FS` when running several.
