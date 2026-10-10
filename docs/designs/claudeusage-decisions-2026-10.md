# `claudeusage`: the decisions left (handover, 2026-10-03)

Status: **open; nothing here has been started.** Written when packages A to E of
[claudeusage-remaining-shape-2026-09.md](claudeusage-remaining-shape-2026-09.md) landed. That doc's
package F listed nine choices for the project owner and said "F needs decisions from the project
owner before any work". This doc is those nine, each with what exists today, the options, a
recommendation and what the first day of work would be once it is decided, plus the smaller items
the round left behind and three findings that deserve a decision of their own.

The app itself is in its Android shape now: XML pages inflated asynchronously, a repository and a
ViewModel publishing immutable state, a started Service, `SharedPreferences.apply()` that returns at
once. What follows is everything that still differs, and every one of them is a platform choice,
not an app bug.

## The short version

| # | Question | Recommendation | Size |
|---|---|---|---|
| F1 | What the four buttons are | Keep the DPAD/BACK mapping. Offer raw `BUTTON_*` codes as an opt-in later, if an app asks. | none now |
| F2 | A constructor-only `Class.newInstance()` | **Do it.** It deletes three overrides in this app alone and makes custom views in XML work like Android's. | 2 days |
| F3 | `picodroid.concurrent` / `picodroid.net` versus the JDK names | Defer. A rename across every app for a benefit only ported code sees. | 1 week |
| F4 | An Android-shaped manifest | **Do the small half:** `<activity>`, `<service>` and `android:theme`. Leave permissions. | 2 days |
| F5 | Who sets the clock | A platform time service; apps stop calling `SystemClock.setCurrentTimeMillis`. **Built 2026-10-09**, [time-service-2026-10.md](time-service-2026-10.md). | 3 days |
| F6 | HTTP or HTTPS to the bridge | Keep HTTP; say so in the README (done). | none |
| F7 | `NsdManager` | Defer until the network stack has multicast DNS. | weeks |
| F8 | Protobuf builders | Keep the mutable messages; record it. | none |
| F9 | The polling Service | Keep; record it. | none |

Beyond F, the size-ratchet accept and the `pico_display2_w` check went in with the round's
commit; what is still open is under "Engineering leftovers" below.

## F1. What the four buttons are

> Settled 2026-10-05 by [app-portability-2026-10.md](app-portability-2026-10.md) K1: option 1, the
> DPAD/BACK mapping, is the KEYS input profile; HOME is BACK held for a second (K2), which is
> what lets this app stop swallowing BACK.

**Today.** `platforms/rp/boards/pico_display2_w/board.toml` maps A, B, X, Y to key codes 19, 20,
23, 4 (`KEYCODE_DPAD_UP`, `DPAD_DOWN`, `DPAD_CENTER`, `BACK`) through LVGL's `PREV`/`NEXT`/`ENTER`/`ESC`.
The `KEYCODE_BUTTON_A/B/X/Y` constants (96, 97, 99, 100) exist on `picodroid.view.KeyEvent` since
B7 and no board maps a button to them. The app swallows BACK so the appliance never finishes
(tracker row 75).

**Why it is this way.** The DPAD/BACK mapping is what lets stock widgets, Settings and the keyboard
navigate with four buttons. A remap to `BUTTON_*` would break every one of them.

**Options.**

1. Keep. Apps written for the panel read `DPAD_UP` and know it is the A button.
2. Keep the mapping and add an opt-in: a manifest or board flag under which the pins deliver
   `BUTTON_*` codes instead, for an app that wants game-pad semantics and does its own focus.
3. Deliver both: `BUTTON_*` to `onKeyDown`, and let the framework translate to DPAD for focus
   navigation when the app does not consume the event. This is what Android does with a game-pad
   attached, and it is the only option under which `onKeyDown(KEYCODE_BUTTON_A)` in a ported app
   works without a board-specific `if`.

**Recommendation.** Option 1 today. Option 3 is the right end state and is a day's work in
`crates/picodroid-core/src/graphics/lvgl/events/` (`pin_to_keycode` and the LVGL indev feed) and
`board_cfg.rs::keycode_to_pin` plus one row in the simulator's control channel, but nothing asks
for it yet. Do not do option 2; it forks the input
model per app.

## F2. A constructor-only `Class.newInstance()`

**Today.** `sdk/java/java/lang/Class.java` exposes `getName()` only; its comment says reflection
is "intentionally out of scope". That one decision costs this app three overrides, all of which
Android does by reflection:

- `MainActivity.getDefaultViewModelProviderFactory()` (tracker row 57), a `ViewModelProvider.Factory`
  that `new`s the one ViewModel class;
- the `FragmentFactory` the pager's adapter needs (`fragments-2026-09.md`);
- `MainActivity.onCreateView(String, Context, AttributeSet)`, the `LayoutInflater.Factory` that
  compares the class name from the layout against three string literals and calls the matching
  constructor (E8).

Every other app with Fragments, ViewModels or custom views pays the same.

**What "constructor-only" means.** `Class.newInstance()` for the no-argument constructor, and a
two-argument form for views, `(Context, AttributeSet)`. No `forName`, no member discovery, no
`Method`/`Field` objects. The JVM already has everything: a `Class` object names a loaded class,
and the interpreter can allocate and run `<init>` by descriptor (`crates/jvm`, the `new` +
`invokespecial` path). It is one native per form, served the way `java.util.zip.CRC32`'s were
(`project_java_builtin_class_recipe` in the memory notes; a `method_tables.rs` row, a
`sdk/member-names.tsv` row, `gen-api-contract.sh`).

**What it would remove.** `ViewModelProvider` constructs the class it is given; the default factory
override goes. `FragmentStateAdapter` can take `Class<? extends Fragment>` per position, or
`FragmentFactory` gets a default that instantiates; the override goes. `LayoutInflater.one()` looks
up the class named by `ATTR_CLASS_NAME` and calls the two-argument constructor; `Activity` stops
implementing `LayoutInflater.Factory` (the interface can stay for Android fidelity), and
`examples/layoutdemo`'s factory goes too.

**Costs.**

- The layout names a class by its source name. With shrinking on, the class is renamed; the packer
  already has the shrink map (it maps `R` and the framework class names), so it should write the
  shrunk name into the layout table under shrink and the original otherwise. Today the string
  literal in `onCreateView` is compared with the layout's string, which works in both modes only
  because neither side is renamed: check `tools/papk-pack/src/res.rs` where `CLASS_NAME` is pushed.
- Pack-time class linking (`e4708a19`, `f5d93245`) resolves what bytecode references. A class only
  the layout names is referenced by nothing, so it needs a root: the packer should add every
  `CLASS_NAME` string to the link roots. A class only a `Class` literal names is already a root
  (`ldc` of a class constant).
- The two-argument constructor must exist. On Android a missing one is an `InflateException` at
  run time; do the same, with the class name in the message.
- Flash: two natives, under 1 KB.

**Recommendation.** Do it. It is the cheapest item here with the widest reach, and the one whose
absence shows in every app that is more than one Activity. Order: the natives and a test in
`examples/jucdemo` or `qa_lang`; `ViewModelProvider`; `LayoutInflater`; `FragmentFactory` last
(it has the most callers: `fragmentdemo`, `pagerdemo`, `claudeusage`).

## F3. Package names: `picodroid.concurrent` / `picodroid.net` versus the JDK's

**Today.** `Executors`, `TimeUnit`, `ScheduledThreadPoolExecutor`, `Future`, `URL`, `InetAddress`,
`DatagramSocket` and friends live under `picodroid.concurrent` and `picodroid.net` (tracker rows 29,
37). `java.time`, `java.util` and `java.util.zip` classes ship under their real names, so nothing
in the runtime forbids `java.util.concurrent` or `java.net`.

**Why it matters.** Code ported from Android, or from any Java, writes `import
java.util.concurrent.Executors;`. Here that fails to compile and the fix is a mechanical import
rewrite, which is exactly the kind of friction the project goal is about. Developer intuition
("it is `java.util.concurrent`") is wrong in the one place where the project otherwise keeps it
right.

**Costs.** Every app and example changes its imports (a `sed`, then a build of every APK). The
shrink maps are append-only, so the old names stay in the maps forever next to the new ones. The
JDK names carry JDK expectations: `ScheduledThreadPoolExecutor(int corePoolSize)` with pool
semantics, `InetAddress.getAllByName`, checked `IOException`s on everything. Each class moved under
a JDK name invites a comparison with the JDK class, which some of these do not survive
(`InetAddress` here has a public `int` constructor; `TimeUnit` is a subset).

**Options.**

1. Defer; keep the rule "`picodroid.*` for everything that is not already `java.*`".
2. Move, and accept the JDK-shaped scrutiny.
3. Alias: ship the JDK-named classes as thin subclasses or re-exports of the `picodroid.*` ones.
   Costs flash per class for no behaviour; and two names for one class is worse than one wrong name.

**Recommendation.** Defer (option 1). Revisit when a second app is ported from Android rather than
written here; then the import friction is measurable instead of assumed. If it is done, do
`java.util.concurrent` first (its classes are closest to the JDK's) and leave `java.net` until
`InetAddress` loses its `int` forms.

## F4. The manifest

**Today.** `buildSrc/src/main/kotlin/picodroid/ManifestSchema.kt` reads `package`, `version`,
`version-code` and one `<application>` element with exactly one of `main-class`, `activity`,
`application`, plus `label` and `icon`. `examples/claudeusage/PicodroidManifest.xml` is three lines.
`UsageService` is declared nowhere; `startService(new Intent(this, UsageService.class))` names the
class directly. The theme is the style called `AppTheme` in `res/values/themes.xml`, picked by name
by the packer (E9), and `MainActivity` calls `setTheme(R.style.AppTheme)` itself.

**Android shape.** `<application android:theme=…>` containing `<activity android:name=…>` with an
intent filter for `MAIN`/`LAUNCHER`, `<service android:name=…>`, `<uses-permission>`.

**What each piece would buy.**

- `<activity>` and `<service>`: a declared surface. The packer can add the named classes to the
  link roots (today the Activity is a root through the `activity` attribute, the Service only
  because the Activity references it), check at pack time that each one exists and extends the
  right base, and reject `startService` of an undeclared class at run time, which is what Android
  does. The launcher can list activities. Mostly `ManifestSchema.kt` and `tools/papk-pack`.
- `android:theme`: replaces the `AppTheme` naming convention. The packer resolves `?attr/` against
  the theme the manifest names, and the framework calls `setTheme` before `onCreate`, so the app
  stops calling it. Small, and it removes a convention that is documented only in
  `guides/resources.md`.
- Intent filters: only meaningful once there is more than one launcher entry per app or an
  implicit-intent path. Not now.
- `<uses-permission>`: declarative only unless the framework enforces something. `WIFI`, `INTERNET`
  and the LED are all open today. Enforcement is a project of its own; declaring without enforcing
  gives a false sense of a sandbox. Leave it until there is an enforcement story.

**Recommendation.** Do `<activity>`, `<service>` and `android:theme`, keeping the current
attributes as the compatible short form so no example breaks. Make an undeclared Service a
pack-time error, not a run-time one, since the packer sees every `Intent(Context, Class)` target
that is a class literal. Leave intent filters and permissions, and write down that permissions are
not enforced.

## F5. Who sets the clock

> Built 2026-10-09 as option 1, both steps: [time-service-2026-10.md](time-service-2026-10.md).
> The SNTP anchor is a platform task; the zone is `/system/time`, set from Settings → Date & time
> through `AlarmManager.setTimeZone` and read through `TimeZone.getDefault()`;
> `Settings.Global.AUTO_TIME` turns the anchor off. `SystemClock.setCurrentTimeMillis` stays
> public (Android's shape; permissions are not enforced, see F4) with no app left needing it;
> `claudeusage` uses the bridge's time only while the clock is unset.

**Today (as of 2026-10-03).** `UsageService` calls `SystemClock.setCurrentTimeMillis(wall)` from the bridge's reply
(`data/UsageService.java`, the bridge time at the hh:mm:05 mark that the pixel A/B relies on) and
`TimeFormat` keeps a static zone offset (tracker row 35). `picodroid.net.SntpClient` exists and
leaves anchoring to each app. Settings has no time or zone page.

**Android shape.** An app cannot set the clock. The platform anchors it from the network once
connectivity is up, and the zone is a user setting read through `TimeZone.getDefault()`.

**Options.**

1. A platform time service: on `ConnectivityManager`'s network-available callback, one SNTP
   exchange (`SntpClient`) sets the clock; re-anchor every few hours; a Settings page for the zone,
   stored under `/system/` beside the Wi-Fi credentials; `java.time`'s `ZoneId.systemDefault()`
   reads it. Apps delete their clock code. `SystemClock.setCurrentTimeMillis` becomes
   framework-private.
2. Keep letting apps set the clock, and document it as an appliance liberty.

**Costs of option 1.** One more connection at boot on a board whose first Wi-Fi join already takes
seconds; a UDP exchange against a public pool needs a DNS lookup first. TLS already depends on it:
`crates/picodroid-core/src/net/tls.rs` checks certificate validity against the anchored clock and
fails closed while it is unset, so today every HTTPS app must anchor the clock itself before its
first request, and a platform time service would have to run before any of them. A zone store and
its Settings page are the larger half.

**Recommendation.** Option 1, in two steps: the SNTP anchor first (a day, mostly plumbing; apps
keep working because `setCurrentTimeMillis` stays public until every caller is gone), then the
zone. `claudeusage` keeps using the bridge's time only until the platform has anchored once; after
that the two agree to within SNTP error and the app's call can go.

## F6. HTTP or HTTPS to the bridge

**Today.** The bridge is a Python process on the owner's PC; the device talks to it over cleartext
HTTP on the LAN after a UDP broadcast finds it. HTTPS exists in the SDK (`weather` uses it), so
cleartext is a choice, not a limitation (tracker row 39).

**Why keep HTTP.** The bridge keeps the OAuth token on the PC; the device only ever sees usage
numbers. A LAN bridge has no certificate a device could verify without pinning a self-signed one
into the app, and pinning means a build-time secret and a rotation story. The threat is a LAN peer
reading usage percentages.

**Recommendation.** Keep, and say so. The README's "How it is built" section now states it; nothing
else to do. If the bridge ever carries anything but usage numbers, revisit with pinning.

## F7. `NsdManager`

**Today.** `BridgeDiscovery` broadcasts a UDP datagram and waits for a reply (tracker row 77; gaps
doc G7). Android's shape is `NsdManager.discoverServices`, which is multicast DNS.

**What it needs.** mDNS in the network stack (`freertos-plus-tcp`, the `shivrajora` fork): IGMP
join of 224.0.0.251, a responder or at least a querier, and the service-record parsing. Then a
`picodroid.net.nsd.NsdManager` with `DiscoveryListener`/`ResolveListener`, and the bridge
advertising `_claudeusage._tcp` through `zeroconf`. The broadcast protocol stays as the fallback
for networks that filter multicast, which many access points do.

**Recommendation.** Defer. The current discovery works, is 150 lines, and has no user-visible gap.
mDNS is weeks of stack work whose first beneficiary is one app. Reopen when a second app needs to
find a LAN peer, or when a board joins a network that blocks broadcast but passes multicast.

## F8. Protobuf builders

**Today.** `scripts/gen-proto.sh` runs `tools/protoc-gen-picodroid`, which emits mutable messages
with chained setters (`claudeusage/proto/*.java`). protobuf-javalite emits immutable messages built
through `newBuilder()…build()`.

**Costs of builders.** One more class per message (flash: a few hundred bytes each, more under no
shrink), one more allocation per decode on a 160 KB heap, and the generator grows. The gain is
Android fidelity for code that passes messages between threads, which immutability makes safe
without a copy. `UsageData` already plays that role in this app: the message is decoded on the poll
thread and copied into an immutable object before it crosses to the main thread.

**Recommendation.** Keep the mutable form; record it in the generator's README as the deliberate
difference. If a builder form is ever wanted, make it a generator flag, not the default.

## F9. The polling Service

**Today.** `UsageService` is `START_STICKY`, polls the bridge on its own thread for as long as the
device is on, and never shows anything. On Android this would be a foreground service with a
notification, or polling scoped to the visible UI (tracker row 79).

**Why keep it.** The device is an appliance that shows one thing; "visible UI" is always true, and
there is no notification shade to post into. A `JobScheduler`/`WorkManager` shape would be heavier
and change nothing the user sees.

**Recommendation.** Keep, recorded (row 79 says so). The one Android habit worth adopting is
already in: the Service publishes into a repository and the UI observes, so stopping the poll when
the screen is off (a backlight PWM item in the gaps doc, G6) would be a one-line gate in
`UsageService` when that lands.

## Engineering leftovers (not decisions)

Done with the round's commit, so nothing hangs on them:

- **The size ratchet** is accepted at 1,008,204 B on `testbench_rp2040` and 1,531,308 B on
  `testbench_rp2350` (+38,220 B and +47,280 B over the previous baseline, of which about 15 KB each
  was the `picodroid.lifecycle` round, FR-12). RAM is unchanged.
- **The `pico_display2_w` check** of C1, D and E ran on 2026-10-03 with a `parity-metrics` build:
  no `slow handler` line across 25 page turns, five syncs, AUTO and the first refresh, which
  confirms deleting `warmChrome` (E5). With that budget in hand the tick-budget deferrals of E6
  were removed and re-measured: a page binds and paints in one tick, the Models page paints both
  cards at once, and a painted page repaints inside the `LiveData` call beside the chrome. One
  stays, the first paint of a page built before the data came (58 ms with cold call sites and the
  chrome's repaint together). Render cost per page turn is 160 to 220 ms of LVGL work against 235
  to 255 ms for the pre-round build on the same board, with the same 40 to 54 ms worst tick.

Still open, in the order they should be done:

1. **Optional SDK additions from package D**, each a small class: `TextClock` (a `TextView` that
   formats the time itself once a minute); a `Lifecycle.State` enum and `LifecycleObserver`
   (`Lifecycle` is a concrete class with `int` states and a public `setCurrentState`); `Fragment`
   as a `LifecycleOwner` and `ViewModelStoreOwner`. None changes what the app does; each removes a
   line or two of it.
2. **FR-10** (`docs/fragments-follow-ups.md`): a nightly row that runs `claudeusage` against a
   replayed bridge and turns real pages. The pixel A/B harness built for this round (recipe in the
   memory note `reference_claudeusage_pixel_ab`: a deterministic payload with `bridge_time` at
   hh:mm:05, `PICODROID_SIM_NET_BROADCAST=0`) is most of it; it needs a home under `scripts/` and a
   `hil-tests.conf` row.
3. **The one deferral left** (above): it goes when cold call-site resolution is cheaper or the
   first refresh is split another way. Not worth app code; note it if the JVM's resolve path is
   ever worked on.
4. **B8, B10** stay recorded: `GradientDrawable` setters return `this` (source-compatible the useful
   way round) and `Activity`'s lifecycle overrides are `public` (widening is legal, so Android-shaped
   code compiles).

## Findings that deserve their own decision

These came out of the round and are not about this app.

**Static fields cost a slot in every instance.** `instance_slot_count` and `field_slot_in` in
`crates/jvm` count every entry of a class's field table, statics included, so each `static` field
is 8 B per instance of that class and its subclasses. `View` has about ten statics: 80 B per view,
on a 160 KB heap, for fields no instance uses. The round worked around it by keeping new scratch
statics in a nested class that is never instantiated (`View.MeasureSpec.sWidth`). The fix, skipping
`ACC_STATIC` when laying out instances, is small in the JVM but shifts every native slot table
(`graphics/fields.rs` and the slot-addressed classes: `Intent`, `Canvas`, `Paint`, `KeyEvent`, the
view fields), so it is a coordinated change with a measurable RAM win. Worth doing on its own.

**A pool worker needs stack to write a file.** `SharedPreferences.apply()` moved file writes onto
the background pool and `testbench_rp2040`'s 4 KB workers overflowed on the board (the simulator
cannot see it). The floor is now 6144 and that board runs two workers at 8 KB; other boards should
be checked the first time anything else moves file I/O onto a worker. A `--sched-diag` or stack
high-water report per worker would make this visible before hardware.

**LVGL's border insets children.** A stroked `<shape>` background (`GradientDrawable.setStroke`)
makes the parent's content box smaller by the stroke width, so `FrameLayout` gravity and
`getLeft()` are relative to the inset box, unlike Android where a background never affects layout.
`examples/layoutdemo` works around it by stroking a leaf view. Either document it in
`guides/resources.md` as the one place a background affects layout, or have `set_bg_stroke` offset
the padding by the border width to cancel it. The second is a few lines in
`graphics/lvgl/view_ops.rs` and would need the pixel A/B to confirm nothing moves.

## Verification, for whoever picks any of this up

- After any change under `crates/`, `platforms/`, `sdk/` or `system-apps/`:
  `./scripts/sim.sh --app helloworld`, then `./scripts/pre-commit`.
- `claudeusage` in the simulator against a demo bridge:
  `python3 examples/claudeusage/bridge/claude_usage_bridge.py --demo &` then
  `./scripts/sim.sh --board pico_display2_w --app claudeusage`. Keys `1`–`4` are A, B, X, Y.
- Layout or theme changes: the pixel A/B against a replayed payload (above). The round's baseline
  is the current tree; any difference is a regression unless intended.
- Anything touching the tick budget: `pico_display2_w` on the bench with
  `PICODROID_EXTRA_FEATURES=parity-metrics`, keys through `pdb input keyevent 20|19|23|4` on the
  board's tty (`fleet.sh tty pico_enviro_mon_w`), no `slow handler` lines.
- F2 and F4 touch the packer: `cargo test -p papk-pack -p papk-format`, then every example APK
  builds (`./gradlew assemblePapk`), then `examples/layoutdemo` on an RP2350 board prints
  `=== ALL PASSED ===`.
- F5 touches networking: the `net` rows of `hil-tests.conf` on `testbench_rp2350w` with
  `.wifi-creds.env` present.
- SDK additions cost flash: `parity-bench.sh --size-only` and a `size:` trailer.

## Bookkeeping

- Each decision, once made, gets a line in this doc's status and closes its tracker row
  (`claudeusage-android-shape-2026-09.md` rows 75, 77, 78, 79; row 57 for F2).
- `docs/README.md` has a row for this doc; update its status column as items close.
- A decision that becomes work gets its own design doc under `docs/designs/`; this one records
  only the choice.
