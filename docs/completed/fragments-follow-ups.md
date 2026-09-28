# Completed: Follow-up backlog: Fragments and `ViewPager2` — 2026-09-27

Items closed out of [fragments-follow-ups.md](../fragments-follow-ups.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

## FR-3: Cut the next shrink map

**Status: closed 2026-09-27.** Map v0.35.0, cut on `main` together with TLS-3 for the v0.35.0
release, names the Fragment and `ViewPager2` classes and their members.

## FR-5: The `claudeusage` row reads TIMED OUT until `sim-run` kills on match

**Status: closed 2026-09-27.** The `claudeusage` row (`sim`, 60 s, `pico_display2_w`) matches
all four patterns (`ui ready`, `page -> Claude usage`, `discovery: failed`, `state -> …`) but its
Activity never exits, and `sim-run.sh` used to let the deadline expire and call that a failure.
`d8339e1e` (the 2026-09-27 nightly fixes, merged as `f6f9aee6` while this backlog was being
written) stops a row's app once every pattern has matched (`patterns matched; stopping the
app`); re-run on that tree, the row is PASS in both shrink modes.

## FR-7: A thread sleeping across an Activity reclaim dies with `InvalidReference`

**Status: closed 2026-09-27, by `d8339e1e` as far as the repro shows.** Found writing
`fragmentdemo` on `f1803997`: a covering Activity whose `onResume` started a
`picodroid.concurrent.Thread` that slept 150 ms and then posted `finish()` to the main executor
(the `qa_life` `T.later` helper) never finished; the log ended with
`Thread.start: picodroid/concurrent/Thread left the interpreter: InvalidReference` right after
the covered Activity was reclaimed under `PICODROID_DONT_KEEP_ACTIVITIES=1`, and the same helper
worked when no reclaim happened mid-sleep. The demos were written around it (a
`ScheduledExecutorService`, or `finish()` straight from `onResume` as `reclaimdemo` does). Put
back into `SecondActivity.onResume` on the merged tree, the thread runs to its `finish()` (the
Activity round trip took 219 ms, against 54–67 ms with an immediate `finish()`) and the
`fragmentdemo` row passes in both shrink modes with no such line. `d8339e1e`'s thread-slot fix
fits the shape: a finished thread's freed slot was the next one `Thread.start` handed out, and
the first thread's trailing cleanup then took the newcomer's entry — a timer thread ending
while the reclaim's own thread traffic reuses slots is exactly that. If the line comes back,
start there (`crates/picodroid-core/src/threads.rs`, `terminate_by_obj`), then the parked-frame
rooting in [designs/jvm-run-lock-2026-09.md](../designs/jvm-run-lock-2026-09.md).

## FR-8: The hardware run and the budget checks

**Status: closed 2026-09-27, on `5dadeb74`.** Results below the plan. The "before" build is
the pre-fragment app (`examples/claudeusage` at `f1803997`) on today's firmware, not the
`f1803997` firmware; the fragment classes are all the app side can reach, so the difference is
the port.

- **Board.** `pico_display2_w` (debug build) with the live bridge: `discovery: found`, then 8 B,
  4 A and 6 B presses 0.6 s apart. Every press delivered in order, each with its `page ->` and
  `built`. Y went home, a long Y turned AUTO on, and AUTO turned about 7 pages. After a power
  cycle AUTO kept turning with no key pressed. There were no `slow handler` lines, no errors,
  and only the known benign `spi0: … 0 of 0 bytes` warning.
- **Memory.** Sim `--mem-diag`, 30 AUTO turns, with a fixed recorded payload replayed on port
  8791 for both builds. From turn 2 to turn 30 the new build's live heap went 16,914 → 17,244 B
  and the old build's went 15,884 → 16,214 B. That is the same +330 B in both: +8 objects,
  3 classes parsed late. The LVGL pool stayed flat in both (20,288 B used new, 19,848 B old).
  After the first minute `nused` held flat at about 271 KB new and 256 KB old. Most of the
  +15 KB is class metadata (parsed 75,974 against 64,340 B). The live Java heap is only +1 KB.
  There was no `OOM: tried` line. One difference: the native low-water mark is 30 KB lower,
  122,600 B free against 152,784. It dropped once, on one Limits → Models turn, as the
  object/field storage grew a chunk; it did not recur over the remaining turns. The census
  shows no `Bundle` among the top classes.
  `fragmentdemo`: the LVGL pool read 7,912 → 7,928 B across the pops and the re-created
  Activity, and 6,336 B at the end. Its final heap delta was −776 B.
- **Page-turn budget.** Sim `--sched-diag` with `PICODROID_TRACE_SPANS=1`, 30 AUTO turns:
  0 `slow handler` lines in both builds, and the same three `HOG fs` findings (16–18, 9 and 9 ms,
  LittleFS) in both. Mean idle was 98.8 % in both. Every span was ≤ 1 ms (79 at 1 ms new,
  63 old), so the host is too fast to tell the two apart. The board run above is the budget
  evidence.
- **Pixels.** Screenshots rather than `fbhash` (Xvfb, `scrot`, PIL diff), because band hashes
  from the fades are partial rectangles that do not compose into a final frame. All five page
  shots (Limits, Models, Burn rate, History, Limits) are byte-identical between the builds,
  including the header. The status screen (bridge down) differs only in the retry countdown
  digit (14 s against 15 s). One logging difference: the new build logs `page -> Limits`
  before `page -> Claude usage`, because the pager builds page 0 under the status screen (§6
  of the design); nothing about it is visible.
- **Cost.** `fragmentdemo` with `PICODROID_SLOW_HANDLER_MS=10`, and again at 1 ms: no line.
  Traced, 115 of 118 spans were 0 ms. The rest were the 9 ms Runnable that runs the demo's 11
  replace-and-pop rounds (about 0.8 ms a round), a 2 ms drain for the Activity re-creation and
  one 1 ms Runnable. The logs showed `replace took 0 ms` and `activity round trip took 67 ms`.

Not run: `heapcensus` on the board, and an `fbhash` comparison on the device.

The plan, as written:

- **`claudeusage` on the board.** `flash.sh --board pico_display2_w --app claudeusage` with a
  live bridge on the LAN: the four screens on B, B held every half second, Y home, Y long for
  AUTO, a power cycle keeping AUTO; no `slow handler` line on a turn. D4 of the gaps roadmap
  (every tick of a page swap overran the budget; closed 2026-09-25) must still hold now that a
  turn is the pager's three ticks (unbind, bind, promote) plus the page's own build chain.
- **Memory.** `python3 examples/claudeusage/bridge/claude_usage_bridge.py --demo`, then
  `./scripts/sim.sh --board pico_display2_w --app claudeusage --mem-diag`, AUTO on
  (`sim-ctrl.sh input keyevent --longpress 4`), `sim-ctrl.sh heapcensus` after turn 2 and turn
  30: live heap flat, no `OOM: tried`, only the four saved page Bundles new. In `fragmentdemo`,
  `sim-ctrl.sh memstats` after a `replace`: the LVGL pool regains Home's widgets.
- **Page-turn budget.** The same run with `--sched-diag`; `slow handler` count per turn against
  the pre-fragment build (`f1803997`).
- **Pixels.** Both builds with `PICODROID_EXTRA_FEATURES=parity-fbhash` and the demo bridge,
  `input keyevent 20` per page, `sim-ctrl.sh fbhash`: band CRCs in rows 26..216 identical for
  the four screens and the status screen.
- **Cost.** `PICODROID_SLOW_HANDLER_MS=10 ./scripts/sim.sh --app fragmentdemo`: no span per
  tick beyond what an Activity switch shows. The sim already measured a `replace` at 0–1 ms
  against 54–67 ms for an Activity round trip.
