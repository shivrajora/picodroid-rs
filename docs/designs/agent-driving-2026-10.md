# Design: agent-driven UI verification — 2026-10-20

**Status: not started** (roadmap only, by the owner's decision on 2026-10-20; see
`../decisions.md`). Written against `1f571977`. Gap G2 in [`../gaps.md`](../gaps.md).

## Why

An agent that changes a screen today proves it with `scripts/sim-shot.sh` (a PNG and the log)
and reads the picture, or trusts a `[layout]` line. On a board it drives `pdb input` blind. The
storytold craft apps let their agents test what they built through a JSON control channel with
`ui.inspect` (the live widget state as data) and `ui.screenshot`; "verify UI changes visually"
is a golden rule there because the tooling makes it cheap. Picodroid has the control channel
(the sim's FIFO, `pdb` on a device) and the capture; what it lacks is the *inspect* half and one
page that puts the pieces together.

## What exists

| Piece | Where | Sim | Device |
|---|---|---|---|
| Launch | `scripts/sim.sh`, `sim-remote.sh` (Xvfb + VNC) | yes | `flash.sh` |
| Drive | `scripts/sim-ctrl.sh` (`input tap x y`, `input keyevent N`, `tap A`, `net up`), `examples/*/test.ctrl` replayed by `sim-run.sh::drive_test_ctrl` | yes | `pdb input tap / keyevent / swipe` |
| Capture | `scripts/sim-shot.sh <png> <settle> <sim args> -- <lines>` | yes | none |
| Read state | the app's log lines; `[layout]` fit line; `[memmon]` | partial | RTT log |
| Docs | `website/.../get-started/simulator.md`, `guides/debugging.md` § Driving the simulator headlessly, `reference/pdb-commands.md`, the recipe in `../qa-app-portability-2026-10.md` | spread over four pages | |

## Proposed

### AD-1. `pdb dump-ui`: the view tree as text

Mirrors Android's `uiautomator dump` / `dumpsys activity top`. A `pd-pdb` command that walks
the handle table under the JVM run lock and prints one line per live view, depth-indented:

```text
Activity picoclock/ClockActivity resumed
  LinearLayout id=root bounds=0,0-320,240 visible
    TextView id=time bounds=20,40-300,120 visible focused text="12:34"
    Button id=alarms bounds=20,160-150,200 visible enabled text="Alarms"
```

Fields: class (the Java class, not the LVGL one), `android:id` name when set, bounds in logical
pixels, visibility, enabled, focused, text for text widgets, checked for compound buttons,
progress for bars, the selected index for pickers. Same output in the sim (through the sim's
pdb socket, `pdb -s sim dump-ui`) and on a board over USB CDC. No pixels, so it works on every
board, partial draw buffers included, and it diffs.

Cost: one pdb command id in `pdb-protocol`, the walker in `pd-pdb` using the handle table's
binding (widget → Java object) and the LVGL coords, a `tools/pdb` subcommand, a fuzz seam for
the reply framing (the reply is bounded by a line count and truncates with a marker).

### AD-2. `sim-ctrl.sh dump-ui` and `expect-ui` in `test.ctrl`

The sim's FIFO gains a `dump-ui` verb that prints the same tree to the log, so a `test.ctrl`
line `expect-ui <regex>` (handled in `sim-run.sh::drive_test_ctrl` beside `wait`) can assert
UI state in a nightly row without a capture: `expect-ui TextView id=time .* text="12:3[0-9]"`.

### AD-3. The driving page

One website page, `guides/driving-the-app.md`: launch (sim, remote, device), drive (the verbs,
the board caveats: `testbench_rp2350` has no buttons, swipes need a touch board), capture
(`sim-shot.sh`, the snackbar gap, the heap line only on a graceful exit), read state
(`dump-ui`, the `[layout]` line, `memstats`), and the one-line recipes an agent pastes. The
four existing pages link to it instead of repeating the recipes.

### AD-4. Device screenshot (deferred)

A `pdb screencap` reading the frame back is not planned: most boards render in partial LVGL
draw bands and never hold a full frame; the ones with a full buffer (PSRAM boards) could, but
`dump-ui` answers the agent's question on every board. Revisit if pixel-exact checks on a
device become a need (the `parity-fbhash` feature already proves byte identity for rendering
changes).

### AD-5. Definition of done hook

Once AD-1 and AD-2 exist, the "UI verified with a capture" line in `CLAUDE.md` § 6 becomes
"verified with a capture or an `expect-ui` row", and the QA apps' `test.ctrl` files gain the
assertions their log greps stand in for today.

## Estimate

12–20 agent-hours: AD-1 6–10 h (the walker is the work; the handle table already binds widget
to object), AD-2 2–3 h, AD-3 3–5 h, AD-5 1–2 h. No hardware needed beyond one board to confirm
the USB reply framing.
