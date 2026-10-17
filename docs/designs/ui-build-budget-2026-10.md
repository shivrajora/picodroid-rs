# The UI build budget: `setContentView` is synchronous again

**Status: built 2026-10-17 — the policy (§3), the inflate fast path (§4), the app migrations and
the one-tick pager turn (§6); verification in §7.**

## 1. The finding

A review of the picodroid apps against Android (2026-10-16) named this the number-one
divergence:

> The UI tick budget is an app-level concern. […] claudeusage uses AsyncLayoutInflater for a
> 30-view page, splits the chrome repaint and the page repaint across ticks, and paints "one
> meter row per step". Settings and Launcher build screens one row per tick through
> self-reposting Runnable chains with generation counters; picoclock does the same. An Android
> developer calls setContentView and moves on. The root cause is roughly 7 ms per view and a
> 50 ms watchdog, not the API. […] Android tolerates a slow first frame and only objects to
> sustained jank. Keep AsyncLayoutInflater as the escape hatch, not the default.

Two causes, both verified before anything was changed:

1. **A view costs the RP2350 about 7 ms** (`docs/completed/claudeusage-gaps-roadmap-2026-09.md`
   D4; `docs/designs/multi-app-2026-09.md` §"Rows land one per UI tick"). Measured with the new
   inflate benchmark (§3): an inflated view is ~800 bytecodes, ~28 natives and ~77 invokes —
   eleven of the natives are `LayoutInflater.nativeWord`, one word of the compiled layout per
   call, and most of the bytecodes are the inflater's attribute switch, each setter's dedupe,
   the LayoutParams and `addView`. Every create parents to the screen and `addView` re-parents.
2. **The "50 ms watchdog" was a log line, not a fault**, but every design doc used "no `slow
   handler` line" as its acceptance criterion. `warn_if_slow` reported any timed span — widget
   events, one Runnable, the pending-op drain — at 50 ms or more, so a 150–320 ms screen build
   in a later Activity's `onCreate` read the same as a stuck click handler. The first Activity's
   `onCreate` and `Application.onCreate` were not timed at all.

## 2. Android's reference points

- Choreographer logs "Skipped N frames! The application may be doing too much work on its
  main thread" at 30 skipped frames (~500 ms), not at the first slow frame.
- An input-dispatch ANR is 5 s. StrictMode's per-call penalties are opt-in.
- A cold start's first frame of 100–300 ms is normal; what a developer is told to fix is
  sustained jank and work on the main thread that belongs on another.

## 3. The rule (stage 1, built)

`crates/picodroid-core/src/lifecycle/budget.rs`. Three classes of span, two thresholds, one
window:

| Class | Spans | Threshold | Line |
|---|---|---|---|
| Build | `application create`, `activity start` (`onCreate` → `onResume`, including the boot Activity and a re-creation), `activity resume` (the uncovered Activity after a pop), `service op`; plus any steady span that registered `BUILD_VIEWS_MIN = 8` widgets or more (a Fragment commit, a page bind, a `setContentView` from a Runnable) | `BUILD_BUDGET_MS = 250` | `screen build: {span} [{class}] took N ms, V views (K frames skipped; budget 250 ms)` |
| Steady | `widget events`, `Runnable`, `connectivity callback`, `wifi callback`, `alarm`, `activity finish` | `SLOW_HANDLER_MS = 50` counts; 250 warns | `slow handler: {span} took N ms (K frames skipped) — stalls the UI tick` |
| Jank | steady spans of 50 ms or more, summed as lost frames (`elapsed / 16`) over `JANK_WINDOW_MS = 1000` | `JANK_LOST_FRAMES = 15` | `jank: K frames lost to N slow handlers in the last second (each >= 50 ms)` |

Why 250 ms: fifteen frames of the 16 ms tick, between the old 50 and Android's 500; the touch
sampler's ring (64 samples at 10 ms) loses no gesture through it; after the fast path (§4) a
30-view page is 60–90 ms, three times under. Why a window rather than "three consecutive slow
Runnables": the widget-events span runs every tick between posted Runnables, so a run counter
never fires; lost frames per second is how Android accounts for jank, and a `Column`-style
chain of twenty 30 ms steps shows once, as jank, which is the right verdict for a 600 ms build
in disguise. Why count a single 50 ms handler without logging it: a stutter of three frames
once is not a bug on a 150 MHz MCU, and the old line trained every app to slice its screen.

Where the spans are timed: `budget::end_span` is the one choke point. `activity start` wraps
`lifecycle/mod.rs::start_activity_instance` (bootstrap, push, `recreate()`, re-creation of a
reclaimed entry); `activity resume` the `onRestart`/`onStart`/`onResume` run in
`activity_stack.rs::pop_and_resume_parent`; `activity finish` the `onPause`/`onStop`/`onDestroy`
of a finishing top; `service op` each Service op; `application create` `Application.onCreate`.
The `pending-op drain` span is gone — it lumped the previous top's pause with the new top's
build. The view count comes from a registration counter in
`graphics/lvgl/handle_table.rs::registered_count`.

Knobs (simulator): `PICODROID_SLOW_HANDLER_MS` (0 turns counting and the jank line off; set at
all, it also prints every slow steady span, StrictMode's shape), `PICODROID_BUILD_BUDGET_MS`
(0 turns the build line off), `PICODROID_TRACE_SPANS=1` (every span, with `views=` and
`kind=`). A `parity-metrics` build prints every build line and every slow steady span,
unlimited, each followed by a `counters:` line with the JVM's work counters
(`docs/parity-audit.md` §"Span report"). The first warning may come at once (the old code's
rate limit silenced the boot screen's build for the first second of the clock).

**The aggregation hook.** `budget::stats()` keeps, per kind, the span count, the slow count,
the total and maximum milliseconds and the frames lost, since the app started; `log_summary`
prints them when the loop ends (`ui budget: N builds (M over budget, max X ms); P handlers
(Q slow, R frames lost, max Y ms)`). A `pdb sysmon` column, a parity-bench field or a
memory-monitor-style window report reads the same statics; nothing else needs to change for
that.

Nothing in `scripts/` or `hil-tests.conf` grepped for the old line; the docs that used it as an
acceptance criterion now read "no `screen build`, `slow handler` or `jank` line".

## 4. The inflate fast path (stage 2, built)

`LayoutInflater.nativeInflateNode(View v, int layout, int pos, int parentHandle, int
parentKind, int[] deferred)`: one native per framework view. Java reads the node's header
(one `nativeWord`), adopts a wrapper with no widget yet (`picodroid.widget.InflaterSupport`,
over the `(int nativeHandle)` constructors), and hands it to the runtime, which

- reads the node's words itself (`graphics/inflate_scan.rs`, pure and host-tested: the
  header, then the `(attr, value)` pairs sorted into what LVGL applies, what the
  `LayoutParams` needs, and what goes back to Java);
- creates the widget **under its final parent** (`create_under` on each widget shim; the
  old path created every widget on the screen and `addView` re-parented it), and applies
  every attribute it has a setter for under **one style refresh** (`style_batch.rs`, now
  re-entrant so the padding and margin batches nest), through the same `view_ops` and
  widget shims the setters call: padding, size, weight / margins / frame gravity as
  `ViewGroup.addView` would by the parent's kind, background colour, visibility, enabled,
  focusable, alpha, keep-screen-on, min and max sizes, text, colour, size, font padding,
  line mode, gravity, orientation;
- writes the wrapper's handle and the fields its getters and dedupes read (`View.id`,
  `visibility`, `enabled`, `focusable`, `keepScreenOn`, `minWidth`, `minHeight`, `alpha`;
  `TextView.mText` (interned before anything is made, since an allocation that fails inside a
  native runs the native again), `mTextColor`, `mTextSize`, `mGravity`, `mMaxWidth`, the
  packed `mLineMode`) by field slot (`graphics/fields.rs`, checked against the class files by
  `native_field_tables_tests`), and the inflater's read position, child count and
  `layout_*` words;
- hands back, in `deferred`, the pairs it has no setter for — a `<shape>` background (Java
  builds the `GradientDrawable` so `getBackground()` returns it), `ImageView`'s source and
  tint, the bars' and pickers' values — for Java's unchanged `apply`.

Java then builds the `LayoutParams` object from the words and joins the child without a
native (`ViewGroup.attachInflated`). The classes on this path: `LinearLayout`, `FrameLayout`,
`ScrollView`, `TextView`, `Button`, `ImageView`, `RadioGroup`, `View`, `Space`. The rest
(the compound buttons, whose constructors reach their widget; the bars, pickers, `EditText`,
`Spinner`, `ListView`, `ViewPager2`), a class of the app's own, a plain `View` with a
`wrap_content` side (Java measures it), a node with more than eight pairs to hand back, a
freed parent and a full widget pool all take the old path, unchanged (`oneLegacy`). A child
of a group of the app's own is made under it but joined through `addView`, so an override
runs. `AsyncLayoutInflater`'s slices use the same `one()`.

Flash: the attribute switch moved from Java bytecode to Rust; the Java switch stays for the
classes above. Zero new `unsafe` blocks (every LVGL call goes through an existing wrapper).

Per inflated view in the simulator (`parity-metrics`, the §5 benchmark): natives 27.8 → 4.2
(two of the four are `Object.<init>`, which the interpreter counts as a native: one for the
wrapper, one for its `LayoutParams` — a pack-time lever of its own), bytecodes 804 → 250,
invokes 77 → 17. What is left is the Java object model: two objects with their constructor
chains and field initialisers, and the child-list bookkeeping.

## 5. Benchmark and numbers

`examples/layoutdemo` inflates `R.layout.bench` (45 nodes: 32 labels with text, colour, size,
padding and a margin; four rows of weighted boxes) three times, a round per main-loop turn, and
logs `inflate bench: 135 views, U us, U/135 us/view`. With `parity-metrics` the round's span
line gives the bytecodes and natives.

| Measure | Before (2026-10-17) | After stage 2 |
|---|---|---|
| Bytecodes per inflated view | 804 (36,168 / 45) | 250 (11,245 / 45) |
| Natives per inflated view | 27.8 (1,253 / 45) | 4.2 (190 / 45; two are `Object.<init>`) |
| Invokes per inflated view | 77 (3,462 / 45) | 17 (753 / 45) |
| µs per inflated view, sim host | 72–113 | 58–92 |
| µs per inflated view, RP2350 (`pico_display2_w` firmware, release, `parity-metrics`) | 6,108 | 2,313 |
| One 45-view round on the RP2350 | 275 ms: natives 100 ms / 1,253 calls, invokes 145 ms / 3,461, fields 20 ms, other 33 ms | 104 ms (under the build budget, so no counters line) |
| layoutdemo's `activity start` on the RP2350 (28 views, the checks, a 67 ms `Log.i`) | 426 ms | 325 ms |

The counters build itself costs about a microsecond per opcode (two clock reads), so the
device figures are for comparison with each other, not with a plain release image.

Screen builds as the new policy sees them (`activity start` spans): to be filled per app as the
migrations (§6) land.

## 6. What changed in the apps (stages 4–5, built)

Each of these built its screen one row per tick against the old line; each is a plain
synchronous build now, with its rows in XML row layouts so the fast path reaches them:

| Site | Was | Is |
|---|---|---|
| Settings (`Column.java`, seven screens) | header first, `addNext` reposting one row per tick, `stop()` in every `onDestroy` | `Screens.screen` puts the header up and sets the content view; the rows are inflated from `res/layout/row.xml` / `row_tail.xml` into the column (`Screens.row` / `info`, `tailOf` for a suffix a screen fills in later) inside `onCreate`, then `Screens.size`; `Column` deleted. The Storage worker stays. |
| Launcher | inline `addRow` chain with a `stopped` flag | a loop in `onCreate` over `res/layout/row.xml` (icon) or `row_tile.xml` (letter tile) |
| picoclock `AlarmListActivity` | `generation` counter, one card per tick | `fill()` inflates `alarm_card.xml` / `new_alarm_card.xml` per card, from `onResume` |
| picoclock `SegmentClock` | three segments per tick, `complete` and `stopped` flags | built in the constructor |
| claudeusage `UsagePage` | `AsyncLayoutInflater` on an empty `page_host`, a stale-callback check, one deferred first paint | `onCreateView` inflates the page's layout; `onViewCreated` binds, observes and paints; `page_host.xml` deleted, the page roots carry its background |
| `ViewPager2` | three-tick page turn (unbind, bind, promote) | one posted tick: one build span |
| `LiveData` | "an observer with much to repaint should post the repaint" | delivery is synchronous and the observer repaints in the call, as on Android |

`AsyncLayoutInflater` stays, documented as the escape hatch for a layout that overruns the
build budget. Rejected: pre-inflating the neighbour page for `setOffscreenPageLimit(1)` (RAM
for the whole session to save a turn already under budget); slicing `Fragment(int)` inside the
framework (breaks the `onViewCreated` contract that the tree exists).

## 7. Verification

- Stage 1: `PICODROID_BUILD_BUDGET_MS=2 PICODROID_SLOW_HANDLER_MS=1 ./scripts/sim.sh --app
  layoutdemo` prints `screen build: activity start [layoutdemo/LayoutDemoActivity] took 3 ms,
  28 views (0 frames skipped; budget 2 ms)` at boot and one `slow handler` line a second after
  it; `PICODROID_TRACE_SPANS=1` shows the benchmark's rounds as `Runnable [] 3 ms views=45
  kind=build`. The CLAUDE.md smoke and `./scripts/pre-commit` pass.
