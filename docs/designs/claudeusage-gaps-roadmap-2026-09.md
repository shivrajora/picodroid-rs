# Platform gaps found building `claudeusage`

**Status: open list; G1, G2 and G3 closed 2026-09-23, G8 closed 2026-09-24 (long-press and repeat 2026-09-26), D5 (app bug) fixed 2026-09-23, D2 fixed 2026-09-25 (`LV_DRAW_SW_SUPPORT_RGB888`, the horizontal gradient's source format), D1 (sim run-lock hand-off) closed 2026-09-25. D4 closed for this app 2026-09-25: the RP2350's flash clock was the ROM's divider of 3 and is now 2 (`hal/rp/xip.rs`, every RP2350 board), the History and Models first paints are split, and no page turn has a span over 50 ms; the RAM-resident interpreter loop landed the same day on every RP2350 board, paid for by H7, H8 and H9. G11 attributed 2026-09-24: the board has 98 KB free on its worst page, the simulator 9 KB; the levers are listed there. G10 closed 2026-09-25: the collector compacts in bounded slices from a 4 KB buffer claimed at boot. G4 closed 2026-09-26: `View.onDraw(Canvas)` over a retained display list; both charts are one view each. H4–H7 closed 2026-09-26 with docs/parity-audit.md M8–M10: every runtime table is the same bytes on both targets; re-measured the same day on main `4b6a17dc`, the History page is 259 KB in the simulator (was 399) and 243 KB on the board (was 320, of a 372 KB arena now), 129 KB free.**

Completed items: [completed/claudeusage-gaps-roadmap-2026-09.md](../completed/claudeusage-gaps-roadmap-2026-09.md) — D1, D2, D3, D4, D5, G1, G2, G3, G4, G8, G9, G10, H1, H2, H4, H5, H6, H7, H8, H9.

`examples/claudeusage` is a desk display for Claude usage limits on a new board, `pico_display2_w`
(Pimoroni Pico Display Pack 2.0 on a Pico 2 W). It was built to look like a modern product rather
than a demo, modelled on the ESP32 usage monitors people have published (Clawdmeter, TokenMeter,
claude-code-usage-monitor, ohmyclawd). This file records every place picodroid made that harder than
it should be, what the app does instead, and what closing the gap would take.

The app works around all of these; none blocks it. They are ordered by how much they cost a
polished UI, with the three defects found during simulator QA first because they are bugs rather than
missing features.

Everything below was observed on the simulator (`./scripts/sim.sh --board pico_display2_w`)
unless it says hardware.

## Defects

All five defects (D1–D5) are closed; see [completed/claudeusage-gaps-roadmap-2026-09.md](../completed/claudeusage-gaps-roadmap-2026-09.md). One runtime
residual of D4 stays open: halve the RAM-resident interpreter copy by keeping the cold
handlers (math, convert, arrays, indy, monitors, the exception and GC tails) out of line in
flash, or XIP-cache pinning as the no-RAM alternative. The P backlog below is the rest.

## Gaps, by cost to the UI

### G5. Image assets lose their alpha

PAPK assets are RGB565 with alpha discarded. Sprites must be drawn onto the exact colour they will
sit on and cannot be placed over a gradient or a second card colour. The numeral sprites
hard-coded `Palette.CARD`, in one colour, until the figures became `TextView`s (2026-09-23).

**Ask:** an RGB565A8 (or A8-only, tintable) asset format. A8 glyph masks would also cover most of
G1 for apps that ship their own numerals.

### G6. No backlight brightness

The backlight is a GPIO, on or off. An always-on desk display wants dimming at night. `pin_bl` is
PWM-capable on every current board.

**Ask:** PWM backlight with a settings-level brightness API, and an idle *dim* stage before idle
*off*.

### G7. No general build-time app configuration

The only build-time string an app can receive is `NetTestConfig.HOST`, a test hook this app reuses
for its bridge address (port hard-coded). There is no `BuildConfig`, and a four-button device has
no practical way to type an address into `SharedPreferences`.

**Ask:** a `buildConfigField`-style Gradle block generating `BuildConfig` constants from properties
or environment variables. mDNS / DNS-SD would remove the need for an address at all.

*Amendment 2026-09-22:* the app now reads the host from the `bridge_host` key of its `settings`
preferences, with `NetTestConfig.HOST` as the default, so an installed unit can be repointed
without a rebuild (`pdb`, or a future settings screen). The `BuildConfig` ask stands for the
default. See `claudeusage-android-shape-2026-09.md`.

*Amendment 2026-09-25:* the app no longer needs an address at all on an ordinary LAN. It
broadcasts `PICODROID-USAGE?` to UDP 8788 and the bridge answers with its HTTP port; the reply's
source address is the PC (`BridgeDiscovery`, cached in the `bridge_found` preference, re-asked once
a minute while the bridge is unreachable). `NetTestConfig.HOST` is now only the fallback for a LAN
that swallows broadcasts. The SDK grew the Java spellings the probe wanted: `DatagramSocket()`,
`setSoTimeout`, `setBroadcast`/`getBroadcast`, and a `DatagramPacket(byte[], int, InetAddress,
int)` constructor. A general mDNS / DNS-SD (`NsdManager`) browse is still the Android shape and
still open.

### G7 amendment (2026-09-27): `BuildConfig`

`picodroidBuildConfig { field("NAME", …) }` in an app's `build.gradle.kts` generates
`<package>.BuildConfig` with string constants (Android's `buildConfigField` shape;
`buildSrc/.../BuildConfigExtension.kt`). `fieldFromProperty` reads a Gradle property, else an
environment variable, else a default — the `NetTestConfig` precedence. First user: `askclaude`'s
API key, model and endpoint.

### G11. Every class an app touches is parsed into RAM, and the sim charges 1.7x the device

Found 2026-09-24 landing `java.time`: with the SDK's port, this app OOM'd in the simulator on
the first page after a sync (`OOM: tried 4096 B — free 12 KB, largest block 3 KB`). The heap
census (`sim.sh -m -l 0` + `heapcensus`, after the first sync, on the Limits page) explained it:

| tree | native heap used | classes parsed | parsed metadata (host) | device estimate |
|---|---|---|---|---|
| main before the round | 353 KB | 61 of 201 | 127 KB | 74 KB |
| with `java.time` in `TimeFormat` | 388 KB | 70 of 226 | 171 KB | 99 KB |
| plus the D4 follow-ups of the same day | 399 KB of 408 | 70 | 171 KB | 99 KB |

Class metadata is parsed lazily on first use and kept for the run (`ClassFile::parsed`,
`OnceCell<Box<Parsed>>`): about 5 KB per class in the simulator's 64-bit model, 3 KB on the
RP2350 (`devB~` in the census line), and it is the single largest consumer of this app's heap —
larger than every live Java object put together (12 KB). A `LocalDateTime.ofInstant(Instant,
ZoneId)` reaches nine classes; the app now formats through `LocalTime`, `ZoneOffset`,
`Duration` and `DateTimeFormatter` only (`util/TimeFormat.java`), which is the four the screens
need, and the sim runs again with about 25 KB to spare.

**Ask:** cheaper parsed metadata (per-method entries are the bulk: name and descriptor slices,
offsets, flags — a packed table would halve them), a census line per class so the cost of an
import is visible, and a sim model that charges device-sized metadata rather than host-sized,
so an app that fits the RP2350 is not refused by the simulator. Until then, an app on this
board should count the SDK classes it touches, not only its own objects.

**Attributed (2026-09-24):** the census now names every parsed class and every part of the
parsed record, and the simulator's allocator can keep the call stack behind every live arena
block (`PICODROID_MEMDIAG_SITES=1`, docs/memory-diagnostics.md). With that, and a mem-diag
firmware on the board beside it, the whole heap has owners. Measured on the same afternoon's
tree, the live bridge, pages turned in order after the first sync:

| page | sim arena used (of 408 KB) | RP2350 heap used (of 408 KB) | RP2350 free / lowest ever |
|---|---|---|---|
| before the first sync | 120 KB | 244 KB | 174 KB |
| Limits | 383 KB | 301 KB | 117 KB / 106 KB |
| Models | 391 KB | 314 KB | 103 KB / 86 KB |
| Burn rate | 396 KB | 317 KB | 101 KB / 78 KB |
| History | 399 KB | 320 KB | 98 KB / 78 KB, largest block 48 KB |

The device is not at the edge: 98 KB free on the worst page, 78 KB at the deepest transient.
The simulator is, by 79 KB, and the ledger says why — pointer width: parsed class metadata
161 KB on the host against 94 KB modelled for the device, the class table 11 against 4.5,
the static field store 14 against 7, the seven dispatch memos 11 against 5. Where the bytes
are on the History page (sim ledger, device model beside it):

| owner | sim | device | what |
|---|---|---|---|
| task stacks, TCBs, queues | 121 KB | 121 KB | JVM 33; `usage-poll` 16.5; `usage-tick` 16.5; 4 pool workers 25; pdb 8.3; fs 8.3; cyw43, flash parker, timer, idle, queues 10.8 |
| parsed class metadata, 73 classes | 161 KB | 94 KB | CP offsets (`usize` each) 43 %, method table 35 %, CP tags 6 %, the `Box<Parsed>` 7 % — the rest is fields, interfaces, bootstrap and exception tables |
| JVM heap storage | 41 KB | 41 KB | slot chunks, the fields arena (14 KB), side tables, arrays, strings, GC buffers; holding 11–26 KB of live Java objects |
| resolution tables | 14 KB | 16 KB | sized on purpose (D4) |
| static field store | 14 KB | 7 KB | a `Vec` of (class name, field name, value), doubled once to 256 entries |
| LittleFS | 12.7 KB | 12.7 KB | read cache, program cache and lookahead each default to the 4 KB block size |
| class table, 226 entries | 10.8 KB | 4.5 KB | |
| dispatch memos, 7 handlers | 10.8 KB | 5.4 KB | main + 4 pool workers + 2 Java threads, 64 rows each |
| JSON pool, frames, misc | 8 KB | 8 KB | |

The device sum (310 KB) is 10 KB under the board's own `nused`: the network stack's sockets
and buffers, which the simulator does not model (host sockets). The LVGL pool is a second
heap with the same disease: the widget tree costs the simulator 1.6× (Burn page 27.0 KB of
its 42.6 KB pool against 16.9 KB of the device's 45.7 KB; `lv=` on the `[memmon]` line).
**2026-09-26, after M8–M10**, both sides on main `4b6a17dc` (the merge of M8–M10, G4 and
the bridge discovery): the board under a `mem-diag` build with the live bridge, the simulator
under `PICODROID_MEMDIAG_SITES=1` + `heapcensus`, pages turned in order after the first sync,
two laps. The device's arena is 372 KB since the RAM-resident loop (the simulator's cap keeps
408):

| page | sim arena used (of 408 KB) | RP2350 heap used (of 372 KB) | RP2350 free / lowest ever |
|---|---|---|---|
| before the first sync | — (synced within the first window) | 212 KB | 160 KB |
| Limits | 240 KB | 227 KB | 145 KB / 141 KB |
| Models | 250 KB | 234 KB | 138 KB / 113 KB, largest block 84 KB |
| Burn rate | 258 KB | 240 KB | 132 KB / 113 KB |
| History | 259 KB | 243 KB | 129 KB / 113 KB, largest block 84 KB |
| History, second lap | 262 KB | 245 KB | 127 KB / 113 KB |

The board went from 98 KB free on History to 129 KB, with 36 KB less arena to draw on; the
simulator from 9 KB free to 149 KB. The sim's charge is now 16 KB over the board on the same
screen where it was 79 KB over; the known host-side remainder is the class table (32 B against
20 per entry, 2.8 KB over 236), one fat pointer per parsed class (0.7 KB), a `Vec`/`Box` header
on each of ~390 live blocks (12 B each) and about 1.9 KB the device's network stack holds
that M9's model does not (its calibration, the same day: docs/parity-audit.md M9). Per term on the History
page (sim ledger, device model beside it; the device figures are pinned by
`class_metadata_tests` and the `const` asserts, not measured): parsed metadata 57.4 KB
host / 56.7 device for 86 classes (the 24th had 73; G4's charts and the discovery classes
are the difference), class table 7.5 / 4.7, static store 4.8 / 4.8, resolution tables
12.3 / 12.3, five dispatch memos 1.5 / 1.5, the LVGL pool 19.1 of 71.4 KB (27 %) against the
board's 12.3 of 45.3 (27 %) — the History tree is smaller on both sides since G4 (23.8 and
15.1 on the 24th) and the ratio is still 1.56×. The IP task and each open socket are charged
now (M9). The lowest-ever free on the board (113 KB) and its 84 KB largest block are set by
the Models first paint, not by History. The next lever is H10; H1–H3 stay the app's own.
The whole divergence, term by term, and the plan that closed it (M8–M10) are in
docs/parity-audit.md, "2026-09-24 memory-model divergence". Classes that cost the most on
the device, after M8 (the 24th's figures in brackets): `MainActivity` 3.5 KB (5.7),
`JSONObject` 3.2 (6.0), `View` 2.6 (4.0), `UsageService` 2.2 (3.5), `LocalTime` 2.1 (3.9),
`JSONArray` 1.9 (3.5), `Duration` 1.7 (3.2), `HttpURLConnection` 1.5 (2.7), `Thread` 1.3
(2.5), `LayoutInflater` 1.3 (2.3), `SharedPreferences` 1.3 (2.3), `UsageFetcher` 1.1 (2.2;
its 24 exception-table entries now stay in flash).

The levers are tracked as H1–H10 below.

## Heap levers, ranked (H1–H10)

Device bytes on the History page, from the G11 attribution. Each is its own piece of work;
the runtime ones shrink every app and close most of the simulator's gap at the same time
(docs/parity-audit.md, M8).

*App (about 39 KB):*

- **H3. Format times without `java.time`** — open, only when the budget is wanted. The seven
  classes `TimeFormat` reaches cost 11.9 KB (20.7 KB in the simulator) before M8, and about
  half that since (`LocalTime` 2.1 KB and `Duration` 1.7 in the 2026-09-26 census); integer
  arithmetic on the epoch does what the screens need. Undoes part of the 2026-09-24 showcase.

*Runtime, every app (about 36 KB):*

H4–H7 are closed; see [completed/claudeusage-gaps-roadmap-2026-09.md](../completed/claudeusage-gaps-roadmap-2026-09.md).

*Platform (about 20 KB):*

- **H10. The JVM task's 32 KB stack** — open, measure first. The largest single block; it
  needs a high-water reading (`pdb sysmon` task table) before it is touched.

Not levers: the resolution tables (D4 bought them), the JVM heap storage (the live set is a
quarter of it, the rest is pre-reservation and chunking that keeps first-fit placement
stable), the class table.

### Minor

- `--shrink-app` refuses any app that spells a one- or two-letter member name, because the
  release map hands those names to SDK members. The app hit it with `final Palette p` (renamed to
  `palette` in `91c4234c`); five other examples fail the same way. Tracked with the fix in
  `docs/quality-roadmap.md`.

- `board.toml` has no pin-conflict check; collisions between a display, buttons and the CYW43 pins
  are caught only by review.
- `website/.../get-started/build.md` describes `pico_enviro_mon` as 240x135; its `board.toml` says
  240x240.
- `GradientDrawable` honours the colour's alpha as background opacity (so `0x00000000` gives a
  transparent container), which is useful and undocumented.

## Performance backlog after the SRAM study (P1–P5)

**Status: open (recorded 2026-09-26).** Source: `sram-hotpath-2026-09.md` §6, from a DWT
PC-sample profile of claudeusage page turns on `pico_display2_w` with every hot set in SRAM
(option D, landed the same day for this board). These are the costs that remain when the
fetch problem is solved: each is *how much work* is asked for, not how fast it is fetched, so
each needs an algorithmic change. Shares are of the CPU time left after option D.

### P1. The fade repaints the whole screen eight times per page turn

`lv_draw_sw_blend_color_to_rgb565_swapped` is 17 % of what is left, executing from RAM at
full speed. A turn paints the 320×240 screen once blank and eight times for the fade-in
(`parity-fbhash`: 90–110 bands per turn). Fewer fade steps, or fading only the region that
changed, is worth more than any further placement. App-level; the fade lives in
`MainActivity.paintPage`.

### P2. LVGL style lookups

`get_prop_core`, `get_selector_style_prop`, `lv_style_prop_get_default`,
`lv_obj_get_style_prop_internal`, `lv_style_get_prop_internal`: 12 % of busy before, 7 %
after, all now from RAM. Every draw and layout pass asks each widget for its padding,
colours and borders by walking its style list and its parents'. Levers: LVGL's per-object
style cache (`LV_OBJ_STYLE_CACHE`), or fewer local styles per object in `Ui.box`/`label`
(each `setPadding` and `GradientDrawable` is a local style entry).

### P3. Name-based lookups on every native call

`find_class` (a hashed linear scan over every loaded class), `cp_utf8`, `core::str::from_utf8`
(UTF-8 validation of a constant-pool name per call — `names.rs:38`, `native/mod.rs:1180`) and
`memcmp` together are 5–6 % of busy and unchanged by placement. The answer is fixed per call
site: cache the class index or the `&str` alongside the resolution tables (D4 built the
table; the native path does not use it for the class name yet), and skip validation for
names the class-file parser already checked at load.

### P4. `memcmp` / `memset` / `memcpy` live in flash and cannot be tagged

2.5 % of busy, from `compiler_builtins`. To move them: define the three as strong symbols in
the platform crate under `#[link_section = ".data"]`, in a module with `#![no_builtins]` so
the copy loop is not lowered back into a `memcpy` call. About 1.1 KB of RAM. Also benefits
LVGL's `lv_memcpy`/`lv_memset` only where they forward to the libc names (they do not: LVGL's
builtins are in the hot list already).

### P5. 64-bit division on every clock read

`u64_div_rem` and `__udivmoddi4` are 1–1.3 %: `now_ms()` divides a nanosecond count by 10⁶ at
the start and end of every timed span, and `SystemClock` does the same for the app. A
microsecond counter with milliseconds derived by a 32-bit multiply-and-shift removes the
software division. The `parity-metrics` build pays this twice as often (the span counters);
the plain build still pays it per span and per `SystemClock` call.
