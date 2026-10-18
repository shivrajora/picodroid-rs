# A view keeps its listener: tree-owned listeners, no native listener tables

**Status: built 2026-10-17 — the native binding (§4), the Java holders (§5), the app cleanups
(§6); verification in §7.**

## 1. The finding

A review of the picodroid apps against Android (2026-10-16) ranked this gap above any missing
class:

> The GC and native tables make apps hold views alive by hand. Settings, Launcher, calculator
> and picoenvmon keep view arrays "so the rows stay reachable while their click listeners are
> live" (SettingsActivity.java:25-26, HomeActivity.java:35-37). Every listener kind is capped at
> 32 entries, which is why TimeZoneActivity cannot give 38 rows a click listener each. On
> Android the view tree owns its listeners. Now that `ViewGroup` keeps a Java-side child list,
> listeners can live as Java fields on the View and be rooted through the tree, with the native
> side holding only a per-object flag. This is a correctness gap that fails silently, so I would
> rank it above any missing class.

What was true when the work started, verified in source:

- The listener objects were already Java fields on `View` (`onClickListener` and five more);
  `setOnClickListener` stored the field and called a no-arg native. `fireClick()` read the field.
- The native side kept about twenty fixed-capacity `PtrMap<N>` tables, one per listener kind, of
  `(raw lv_obj_t*, View obj_ref)`: 32 for click, long-click, key, touch, swipe, focus and the
  compound buttons; 16 for ListView item clicks, NumberPicker, the EditText listeners; 4 for the
  pickers, SwipeRefreshLayout and Keyboard; 8 for AlertDialog; 4 for Snackbar. Each table did
  two jobs: the **reverse lookup** from an LVGL object to its View at dispatch time, and a **GC
  root** for the View (`gc_root_registration.rs`, 21 providers). The 33rd `setOnClickListener`
  threw `IllegalStateException`; every other kind logged `warn_full` and dropped the
  registration.
- The Java tree already reached every attached View: `Activity.mContentView`, then
  `ViewGroup.mChildren` (since `a97321ee`), then `View.mParent`; Activities are rooted by the
  native activity stack. The apps' arrays were left over from the 2026-07 bug (PEM-2,
  docs/bugs-memory-stress-2026-07-23.md) in which the tables did **not** root the views and a
  view held only by one was swept on the first GC.

So the caps existed only because the reverse lookup was a fixed array, and the arrays in the
apps guarded against a bug fixed three months earlier. Replacing the lookup removes the caps,
the throw and the native rooting together.

## 2. Android's behaviour

- `View.mListenerInfo` is a field of the view. A view lives while something reaches it: the
  window's tree, or a field. Nothing registers listeners anywhere else.
- A shown `Dialog` is held by the `WindowManager` until dismissed; a shown `Snackbar` by the
  `SnackbarManager`. `new AlertDialog.Builder(this)....show()` is a complete statement.
- `performClick` on a detached view fires its listener: the caller holds the view.
- A view nothing reaches is gone with its window. picodroid differs in one place: a widget is
  created under the screen before any `addView`, so a view never added to a parent is still on
  the screen (§4, the diagnostic).

## 3. Decisions

1. **Tree-owned semantics.** No native GC root for a View. A view with a listener that nothing
   in Java reaches is collected.
2. **One binding per widget in the handle table** (`handle_table.rs`): `java_ref[slot]`, set by
   every listener-registration native through `register_listener_with` (`graphics/view.rs`),
   read by every dispatch through `bound_java(handle)`.
3. **Event queues carry handles, never raw pointers.** A trampoline reads `handle_of(target)`
   while the object is alive (a scan of the slot table: the object's own `user_data` would be one
   read, but the drawing view's canvas keeps its state there, `pd_canvas.c`); the drain resolves
   `bound_java`, which is `None` for a widget deleted since the event was queued — a click that
   finished its Activity — so the per-kind `LV_EVENT_DELETE` purges are gone with the tables.
4. **Registration = bind + idempotent attach.** `lv_obj_remove_event_cb` then `add_event_cb`,
   so a listener set twice attaches one trampoline and no "already attached" flag is needed.
5. **`AlertDialog` and `Snackbar` bind at `show()`** and a package-private static showing list
   in each class holds them while shown. The native side reports every framework-driven
   dismissal to Java: BACK, an Activity's teardown or covering and a ninth dialog shown at once
   all queue `AlertDialog.fireDismiss`; the Snackbar timeout tick queues the new
   `Snackbar.fireDismiss` (dispatch site `SNACKBAR_DISMISS`). A dialog built and never shown is
   garbage like any other object.
6. **Key routing reads the view's `onKeyListener` field** (slot 2, `fields::view`) before
   firing `fireKey`, instead of upcalling every key — and every 50 ms auto-repeat — on any bound
   focused view for a `false`. The long-press query reads `onLongClickListener` the same way.
7. **A NumberPicker's box carries `LV_OBJ_FLAG_USER_2`** so the keypad's edit-mode filter can
   tell it from any other plain `lv_obj`; the picker map that used to answer is gone.
8. **The `legacy-handle-cast` escape hatch is deleted** (its own commit). It had no table to
   carry the binding, was promised for one release after 2026-09-15, and v0.35–v0.37 shipped
   since.

## 4. The native design

- **`handle_table`** gains `java_ref: [Cell<u16>; SLOTS]` (stored as `obj_ref + 1`, so 0 is
  "unbound": heap slot 0 is an ordinary object, and the first cut stored the ref itself — the
  `callbacktest` Switch landed in slot 0 under the GC-stress lane and never dispatched), `bind`,
  `bound_java`, `handle_of` and `prune_dead_java`. `invalidate_if_current` and `reset` clear a binding with its slot.
  `native_state_prune` (`native_handler/mod.rs`) calls the prune after every collection, before
  any allocation can reuse a freed slot, on whichever JVM task collected — so the prune never
  calls LVGL. A dead binding on a slot whose widget is still alive is the formerly silent bug
  made loud: the sim prints `a View with a listener was collected while its widget is still
  alive (nativeHandle N): add it to a parent (addView), keep a field, or close() it` and, under
  `PICODROID_HANDLE_SANITIZER` (on by default in `sim.sh`), stops with a backtrace of the
  collection. An Activity's finish deletes its LVGL tree before the Activity leaves the root
  set (`activity_stack.rs`), so a finishing Activity never trips it.
- **A delete callback never calls `handle_of`**: LVGL runs an object's callbacks in registration
  order and the handle table's own hook may have cleared the slot first. A callback that needs
  the handle takes it from the event's `user_data`, as `scrim_delete_cb` and the handle hook do.
- **Every kind** (`button`, `events/{touch,swipe,focus}`, the compound buttons, `seek_bar`,
  `spinner`, `date_picker`, `time_picker`, `swipe_refresh_layout`, `edit_text`, `keyboard`,
  `list_view`, `number_picker`, `alert_dialog`, `snackbar`): the map, its cap, its delete hook,
  its `lookup_*` and its `visit_*roots` are gone; the queue holds `i32` handles; the dispatch in
  `lifecycle/widget_events.rs` and `lifecycle/input.rs` resolves `bound_java`. A ListView row's
  trampoline queues `(list handle, lv_obj_get_index(row))`, the position at click time, as
  Android reports it. `AlertDialog`'s button, list and matrix records are keyed by the dialog's
  handle, registered right after the scrim is made. `keep_on.rs` is a `LocalSet` of raw
  pointers, as before in substance. `listener_map.rs` is deleted.
- **GC root providers** 21 → 11: the `Display` singleton, animation end-actions, the executor
  and scheduled queues, threads, sensors, input, services, and two queues of callbacks the
  framework still owes — the dialog and snackbar dismissals.
- **RAM**: about 3.8 KB of `.bss` for the tables goes; `java_ref` is 512 B on a device (2 KB on
  the host).

## 5. The Java design

- `View`'s six `setOn*Listener` javadocs state the rule; no code changed.
- `AlertDialog`: `sShowing` (an array with manual compaction, the `EditText.watchers` idiom; no
  new class), added in `show()`, removed in `dismissed()`, which both `dismiss()` and
  `fireDismiss()` reach. `nativeRegisterButtonClickListener()` moved from `Builder.create()` to
  `show()`.
- `Snackbar`: `sShowing` of four (the native shows at most four), `showing`, `isShown()`,
  `fireDismiss()`; `nativeRegisterActionClickListener()` moved from `setAction` to `show()`.
- Divergences kept: `removeView` frees the widget, and a native on a released view throws
  `IllegalStateException` (`performClick` included).

## 6. What changed in the apps

| App | Before | After |
|---|---|---|
| Settings: `SettingsActivity`, `AppsActivity` | `View[] rows` "held so the rows stay reachable" | gone; the focus goes to `column.getChildAt(1)` |
| Settings: `WifiActivity` | `View[] rows` | a `scanRow` field for the focus |
| Settings: `DateTimeActivity`, `StorageActivity`; launcher | arrays kept, comment said "reachable" | kept (read back); the comment says why |
| Settings: `TimeZoneActivity` | comment: 38 rows would overrun the table | a pick from 38 entries is a list on Android |
| calculator | `View[] keys` never read | gone |
| picoenvmon (Java and Kotlin) | `menu` field "defense-in-depth against the sweep" | a local |
| picoclock `AlarmListActivity` | `int[] shown` "held so their listeners stay reachable" | gone |
| tutorial_screens, tutorial_service | buttons as fields "so the GC always sees them rooted" | locals, with the Android sentence; the website tutorials follow |
| website guides | "Hold a Java field to any View that has a listener" | "A view in the tree keeps its listener" |

## 7. Verification

- Host: `./scripts/test.sh` green in both shrink modes — the handle-table tests (bind, prune,
  ABA, the sanitizer stop), the root-provider scan at 11, the six listener field slots, the
  dispatch-site resolution with `Snackbar.fireDismiss`, the name tables.
- Sim: `helloworld`, `callbacktest` (every widget kind's token), `dialogdemo` (item and choice
  picks), `qa_ui` — 237 checks before the new section, plus `treeRooted`: 40 buttons with a
  listener of each kind held by nothing but their container, clicked after a forced collection
  (eight a tick: the click ring holds 16); a field-held view never added to a tree fires; a
  removed view's `performClick` is refused; a dialog and a snackbar held only by the framework
  deliver a pick and report a timeout. `churn()` no longer keeps its `rooted[]` array.
- Sim lanes for every app edited (§6), and the GC-stress pass (`sim-run.sh --app gc-stress`),
  which is what caught the slot-0 encoding bug: `callbacktest` lost its `SWITCH` token there
  and in every release build, never in a dev build.
- HIL: `callbacktest` and `dialogdemo` on `testbench_rp2040`, `calculator` on `pico_touch_kit`
  (both shrink modes, all green). The `qa_ui` row needs a plain `rp2350` slot, which another
  session held on the day; it is still to be run on hardware.

## 8. Follow-ups

- `Snackbar.performActionClick`, and synthetic touch/key drivers, so those kinds can be asserted
  as clicks are.
- Whether the prune should also delete the orphan widget of a collected view (it cannot: the
  collecting task may not be the UI task), or the framework should refuse to show a view with
  no parent.
- An `AlertDialog` built and never shown leaks its hidden scrim, as before this change.
