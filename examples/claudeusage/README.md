# claudeusage

A desk display for Claude usage limits: the 5-hour session, the weekly cap, per-model caps, burn
rate, and token history. Built for the `pico_display2_w` board (Pimoroni Pico Display Pack 2.0 on a
Pico 2 W) and runs on any networked board or in the simulator.

Picodroid has no TLS, so the device does not talk to Anthropic. A small bridge runs on the PC where
you use Claude Code and serves the numbers over plain HTTP on your LAN. Your OAuth token stays on
the PC; the device only ever receives percentages, reset times and token counts.

## Run the bridge

```bash
python3 examples/claudeusage/bridge/claude_usage_bridge.py          # real data, port 8787
python3 examples/claudeusage/bridge/claude_usage_bridge.py --once   # print one payload and exit
python3 examples/claudeusage/bridge/claude_usage_bridge.py --demo   # synthetic data
```

It needs Python 3.8+ and nothing else. It reads `~/.claude/.credentials.json` (written by Claude
Code when you sign in) and the transcripts under `~/.claude/projects/`. Allow inbound TCP 8787 from
your LAN if the PC runs a firewall.

The limits come from an undocumented endpoint that rate limits hard; the bridge asks it once every
three minutes. The "API value" on the History screen is an estimate from public per-token prices,
not a bill.

## Run in the simulator

```bash
python3 examples/claudeusage/bridge/claude_usage_bridge.py --demo &
./scripts/sim.sh --board pico_display2_w --app claudeusage
```

Keys `1` `2` `3` `4` are buttons A B X Y. The bridge address defaults to `127.0.0.1`.

## Run on the board

The bridge's default address is baked in at build time, next to the WiFi credentials:

```bash
env $(grep -v '^#' .wifi-creds.env | xargs) PICODROID_NET_TEST_HOST=192.168.1.20 \
  ./scripts/flash.sh --board pico_display2_w --app claudeusage
```

Give the PC a fixed address (a DHCP reservation) so it stays where the display expects it. An
installed unit can be repointed without a rebuild: the `bridge_host` key in the app's `settings`
preferences overrides the build-time host (there is no screen to type it on yet; `pdb` can set it).
Either form may carry a port, `192.168.1.20:8790`, for a PC whose live bridge already owns 8787
and runs a `--demo --port 8790` one beside it for the simulator.

## Buttons

With the display landscape, A is top-left, B bottom-left, X top-right, Y bottom-right. Each corner
of the screen shows the hint for the button beside it.

| Button | Action |
|---|---|
| A | previous screen |
| B | next screen |
| X | sync now |
| Y | back to Limits; on Limits, toggle `auto`, which cycles the screens every 10 s (remembered across power cycles) |

Screens: **Limits**, **Models**, **Burn rate**, **History**.

Each limit is a ring gauge. The white tick on the ring marks how far through the window you are:
fill past the tick means you are using the limit faster than it replenishes. The countdown inside
the ring is the time to the window's reset.

## When the PC is off

The header dot is green while data is live, amber when the last poll failed but the numbers are
still recent, and red once they are stale (about two and a half minutes). Stale numbers are dimmed
and the footer says why and for how long:

| Footer | Meaning |
|---|---|
| `PC offline` | nothing answers at the bridge's address: the PC is off, asleep, or the address is wrong |
| `Bridge down` | the PC answers but refuses the connection: start the bridge |
| `No reply` | the bridge accepted the connection and said nothing |
| `Login expired` / `No credentials` | run `claude` on the PC to sign in again |
| `Rate limited` | Anthropic is rate limiting the bridge; it recovers by itself |
| `WiFi down` | the display lost the access point |

Reset countdowns keep running while offline. If a window resets while the display cannot sync, its
percentage is replaced by `--%`, since the old figure is then known to be wrong. Until the first
successful sync after power-on, a status screen shows the problem, the bridge address being tried
and the retry countdown.

## Demo failure modes

```bash
curl 'localhost:8787/demo?fail=auth'     # auth | rate | creds | garbage | http500 | hang | nodata | none
curl 'localhost:8787/demo?reset=30'      # the session window resets 30 s from now
```

## Numeral sprites

The SDK renders one font size, so the large figures are images: `tools/gen_digits.py` renders them
into `res/drawable/` (needs Pillow and the Ubuntu font). They are drawn onto the card colour because
PAPK assets carry no alpha; regenerate them if `@color/card` in `res/values/colors.xml` changes.

## Layout

The app is one Activity, declared in `PicodroidManifest.xml`, over a started-and-bound
`UsageService` that polls the bridge. The header and footer are `res/layout/activity_main.xml`;
colours, strings and thresholds live in `res/values/`. The four screens are built in code, a few
views per tick, because inflating a whole screen in one tick overruns the RP2350's UI budget.
`docs/designs/claudeusage-android-shape-2026-09.md` lists every remaining departure from Android
idiom and why.
