// SPDX-License-Identifier: GPL-3.0-only
//! Pure data types for the native handler's lifecycle state.
//!
//! The Activity stack and the pending Activity/Service op queue are plain
//! arrays + indices with no hardware or FFI deps. Kept here as a separate
//! module so they compile under `cfg(test)` (the parent `native_handler`
//! module is gated to `cfg(not(test))` because of its FFI/HAL imports), and
//! `main.rs` pulls this file in via `#[path]` to expose the tests to the
//! workspace test runner.
//!
//! `MAX_ACTIVITY_STACK` and `MAX_PENDING_OPS` are sourced from the active
//! board's `[jvm]` section in `board.toml` (see
//! `platforms/rp/build.rs::emit_jvm_config`). Defaults of 8 reproduce the
//! pre-tunables behaviour for boards that don't opt in.

include!(concat!(env!("OUT_DIR"), "/jvm_state_config.rs"));

/// Maximum Activity stack depth. Each entry holds a `u16` ObjectRef plus a
/// `&'static str` class name (12 bytes on 32-bit, 16 on 64-bit). Default 8
/// covers any realistic embedded UI flow without burning RAM; raise via
/// `[jvm] activity_stack_depth = N` in `board.toml`.
pub const MAX_ACTIVITY_STACK: usize = ACTIVITY_STACK_DEPTH;

/// Pending Activity transition signaled from Java to the framework loop in
/// [`crate::lifecycle::run_activity`]. Wrapped in [`PendingOp`] so it shares
/// a single FIFO with Service ops, preserving the order the app issued them
/// (a `startActivity` then `startService` from the same frame must process
/// in that order).
#[derive(Copy, Clone, Debug)]
pub enum PendingActivityOp {
    /// `Application.startActivity(intent)` or `Activity.startActivity(intent)`
    /// — push a new Activity of the named class on top of the stack. The
    /// framework allocates the instance and runs its no-arg constructor; the
    /// current top, if any, is paused first. `intent_ref` is the launching
    /// Intent, retained on the stack entry so `Activity.getIntent()` can
    /// return it for the Activity's whole lifetime (`None` for the boot
    /// Activity, which Android also launches without an app-visible Intent).
    Push {
        class_name: &'static str,
        intent_ref: Option<u16>,
        /// `Some(code)` for `startActivityForResult`; `None` for plain
        /// `startActivity`. Carried onto the new stack entry.
        #[cfg_attr(test, allow(dead_code))]
        request_code: Option<i32>,
        /// Stack-entry token of the launching Activity
        /// ([`ActivityStack::token_of`], resolved when the launch is queued;
        /// 0 for an Application-level
        /// boot launch). The result delivery guard checks this on pop.
        caller: u16,
    },
    /// `Activity.finish()` — pop the current top off the stack. If the
    /// stack is left empty, [`run_activity`] returns and the app exits.
    /// `finishing` is the Activity that called `finish()`: the handler
    /// refuses a second Pop for the same object while one is queued, so
    /// `finish(); finish();` (Android's `mFinished` idempotence) pops one
    /// Activity, not the caller *and* its parent.
    Pop { finishing: u16 },
    /// `Activity.recreate()` — destroy `target` (saving its instance state)
    /// and start a new instance in the same stack entry. Ignored unless
    /// `target` is still the top when the op is drained.
    Recreate { target: u16 },
    /// A `startActivity` whose Intent names another package (multi-app
    /// boards): leave this app. The target is recorded in
    /// `crate::packages` (`request_launch`); the lifecycle loop answers
    /// this op by tearing every Activity down and returning, and the
    /// supervisor then runs `packages::next_image()`.
    Launch,
}

/// Pending Service transition signaled from Java to the framework loop. The
/// `intent_ref` carries any extras the Service callback needs to read; the
/// referenced object must remain reachable until the op is processed (the
/// handler holds a strong root via [`PendingOpQueue`]).
#[derive(Copy, Clone, Debug)]
pub enum PendingServiceOp {
    /// `Context.startService(intent)` — `onCreate` (first time) then
    /// `onStartCommand`.
    Start {
        #[cfg_attr(test, allow(dead_code))]
        class_name: &'static str,
        intent_ref: u16,
    },
    /// `Context.stopService(intent)` or `Service.stopSelf()` — clear the
    /// started flag; if no clients are bound, run `onDestroy`.
    #[cfg_attr(test, allow(dead_code))]
    Stop { class_name: &'static str },
    /// `Context.bindService(intent, conn)` — `onCreate` (first time) then
    /// `onBind`, then deliver the IBinder to `conn.onServiceConnected`.
    Bind {
        #[cfg_attr(test, allow(dead_code))]
        class_name: &'static str,
        intent_ref: u16,
        conn_ref: u16,
        owner_activity_ref: u16,
    },
    /// `Context.unbindService(conn)` — last-bind triggers `onUnbind` and
    /// possibly `onDestroy`.
    Unbind { conn_ref: u16 },
}

/// Either an Activity or a Service transition. Drained in FIFO order from
/// [`PendingOpQueue`].
#[derive(Copy, Clone, Debug)]
pub enum PendingOp {
    Activity(PendingActivityOp),
    Service(PendingServiceOp),
}

/// Maximum pending ops per frame. A typical Activity onCreate that calls
/// `startService` + `bindService` queues 2 service ops; default 8 leaves
/// headroom for chained transitions without burning RAM. Raise via
/// `[jvm] pending_op_queue = N` in `board.toml`.
pub const MAX_PENDING_OPS: usize = PENDING_OP_QUEUE_DEPTH;

#[derive(Copy, Clone)]
struct ActivityStackEntry {
    /// This entry's identity for as long as it is on the stack: unlike
    /// `obj_ref` it survives the instance being destroyed and re-created
    /// (`recreate()`, a reclaim). Never 0.
    token: u16,
    /// The live instance; meaningless while `destroyed`.
    obj_ref: u16,
    /// The framework destroyed this covered entry's instance to get its
    /// memory back (`mark_destroyed`); `saved_state_ref` holds what
    /// `onSaveInstanceState` wrote, and uncovering the entry re-creates the
    /// Activity from it. By-`obj_ref` lookups and the GC roots skip it.
    destroyed: bool,
    class_name: &'static str,
    /// Intent that launched this Activity (`getIntent()`'s return value);
    /// `None` for the boot Activity. Rooted by the GC visitor below.
    #[cfg_attr(test, allow(dead_code))]
    intent_ref: Option<u16>,
    /// Java `nativeHandle` of the content view installed by this Activity's
    /// most recent `setContentView`. `0` = no view set yet, or the view has
    /// been freed. Snapshotted from `display::CURRENT_ROOT_ID` on push (so
    /// the view survives while a child Activity is on top) and restored
    /// back into `CURRENT_ROOT_ID` on pop.
    root_handle: i32,
    /// `Some(code)` when this Activity was launched via
    /// `startActivityForResult` — the request code delivered to the caller's
    /// `onActivityResult` when this Activity finishes. `None` for a plain
    /// `startActivity`.
    request_code: Option<i32>,
    /// Token of the entry whose Activity launched this one with a request
    /// code — a token, not an obj_ref, so the caller is still found after it
    /// was reclaimed and re-created. The result is delivered only when this
    /// caller is the one uncovered on pop (the A→B→C guard — B finishing
    /// must not deliver to C).
    caller: u16,
    /// Result code set via `setResult` (default `RESULT_CANCELED` == 0, the
    /// Android default for a finished Activity that never called setResult).
    result_code: i32,
    /// Result Intent set via `setResult(int, Intent)`. Rooted by the GC
    /// visitor below until delivered on pop.
    result_intent_ref: Option<u16>,
    /// The `Bundle` a re-creation carries from this entry's old instance
    /// (`onSaveInstanceState`) to its new one (`onCreate(Bundle)`,
    /// `onRestoreInstanceState`). `Some` while `handle_recreate_op` runs and
    /// for as long as the entry is `destroyed`; rooted by the GC visitor
    /// below, being reachable from neither instance.
    saved_state_ref: Option<u16>,
}

/// Fixed-capacity LIFO of Activity entries. Push fails soft (returns
/// `false`) on overflow rather than threading a Result through the JVM
/// dispatch path — there's no useful recovery for a 9-deep nav stack on
/// an MCU.
pub struct ActivityStack {
    entries: [Option<ActivityStackEntry>; MAX_ACTIVITY_STACK],
    len: usize,
    /// Source of entry tokens. Wraps past 0; a collision needs 65,535 pushes
    /// while one entry stays on the stack, and costs a misdelivered result.
    next_token: u16,
    /// A result Intent between the pop of the entry that set it and its
    /// delivery to a caller that has to be re-created first — Java runs in
    /// between, and nothing else holds the Intent. Rooted by the GC visitor.
    delivery_intent_ref: Option<u16>,
}

impl ActivityStack {
    pub const fn new() -> Self {
        Self {
            entries: [None; MAX_ACTIVITY_STACK],
            len: 0,
            next_token: 1,
            delivery_intent_ref: None,
        }
    }

    pub fn current(&self) -> Option<(u16, &'static str)> {
        if self.len == 0 {
            return None;
        }
        let entry = self.entries[self.len - 1].as_ref()?;
        Some((entry.obj_ref, entry.class_name))
    }

    pub fn push(
        &mut self,
        obj_ref: u16,
        class_name: &'static str,
        intent_ref: Option<u16>,
        request_code: Option<i32>,
        caller: u16,
    ) -> bool {
        if self.len >= MAX_ACTIVITY_STACK {
            return false;
        }
        let token = self.next_token;
        self.next_token = match self.next_token.wrapping_add(1) {
            0 => 1,
            t => t,
        };
        self.entries[self.len] = Some(ActivityStackEntry {
            token,
            obj_ref,
            destroyed: false,
            class_name,
            intent_ref,
            root_handle: 0,
            request_code,
            caller,
            // RESULT_CANCELED — Android's default when an Activity finishes
            // without calling setResult.
            result_code: 0,
            result_intent_ref: None,
            saved_state_ref: None,
        });
        self.len += 1;
        true
    }

    /// Live entries: the ones a by-`obj_ref` lookup may match.
    fn live(&self) -> impl Iterator<Item = &ActivityStackEntry> + '_ {
        self.entries[..self.len]
            .iter()
            .flatten()
            .filter(|e| !e.destroyed)
    }

    /// Number of entries, destroyed ones included.
    pub fn depth(&self) -> usize {
        self.len
    }

    /// Token of the live entry whose instance is `obj_ref`, or 0.
    pub fn token_of(&self, obj_ref: u16) -> u16 {
        self.live()
            .find(|e| e.obj_ref == obj_ref)
            .map_or(0, |e| e.token)
    }

    /// Token of the top entry, or 0 on an empty stack.
    pub fn top_token(&self) -> u16 {
        self.entries[..self.len]
            .last()
            .and_then(Option::as_ref)
            .map_or(0, |e| e.token)
    }

    /// True when the top entry's instance was reclaimed while it was covered
    /// — only between the pop that uncovered it and its re-creation.
    pub fn top_destroyed(&self) -> bool {
        self.entries[..self.len]
            .last()
            .and_then(Option::as_ref)
            .is_some_and(|e| e.destroyed)
    }

    /// The covered, still-live entry at `index` (0 = bottom) as `(obj_ref,
    /// class_name, parked root_handle)`. `None` for the top entry, which is
    /// in the foreground, and for one already reclaimed.
    pub fn covered(&self, index: usize) -> Option<(u16, &'static str, i32)> {
        if index + 1 >= self.len {
            return None;
        }
        let e = self.entries[index].as_ref().filter(|e| !e.destroyed)?;
        Some((e.obj_ref, e.class_name, e.root_handle))
    }

    /// Root (or drop) the saved-state Bundle of the entry at `index`.
    pub fn set_saved_state_at(&mut self, index: usize, bundle_ref: Option<u16>) {
        if let Some(e) = self.entries[..self.len]
            .get_mut(index)
            .and_then(Option::as_mut)
        {
            e.saved_state_ref = bundle_ref;
        }
    }

    /// The instance of the entry at `index` is gone (reclaimed). The entry
    /// keeps its place, token, launch Intent, for-result metadata and saved
    /// state; the result is per-instance and goes with the instance, as in
    /// `replace_top`.
    pub fn mark_destroyed(&mut self, index: usize) {
        if let Some(e) = self.entries[..self.len]
            .get_mut(index)
            .and_then(Option::as_mut)
        {
            e.destroyed = true;
            e.obj_ref = 0;
            e.root_handle = 0;
            e.result_code = 0;
            e.result_intent_ref = None;
        }
    }

    /// Root (or drop) a result Intent on its way to a re-created caller.
    pub fn set_delivery_intent(&mut self, intent_ref: Option<u16>) {
        self.delivery_intent_ref = intent_ref;
    }

    pub fn delivery_intent(&self) -> Option<u16> {
        self.delivery_intent_ref
    }

    /// Hand the top entry to a new instance of its Activity (`recreate()`, or
    /// the re-creation of a reclaimed entry).
    /// The launch Intent and the for-result launch metadata carry over, as
    /// they belong to the launch, not the instance; the result is
    /// per-instance and goes back to Android's RESULT_CANCELED default.
    pub fn replace_top(&mut self, new_ref: u16) {
        if let Some(e) = self.entries[..self.len].last_mut().and_then(Option::as_mut) {
            e.obj_ref = new_ref;
            e.destroyed = false;
            e.root_handle = 0;
            e.result_code = 0;
            e.result_intent_ref = None;
        }
    }

    /// Root (or, with `None`, drop) the top entry's saved-state Bundle.
    pub fn set_top_saved_state(&mut self, bundle_ref: Option<u16>) {
        if let Some(e) = self.entries[..self.len].last_mut().and_then(Option::as_mut) {
            e.saved_state_ref = bundle_ref;
        }
    }

    /// Record a result on the Activity identified by `obj_ref` (`setResult`).
    /// Searches the whole stack so a paused Activity can set its result.
    #[cfg_attr(test, allow(dead_code))]
    pub fn set_result(&mut self, obj_ref: u16, code: i32, intent_ref: Option<u16>) {
        if let Some(entry) = self.entries[..self.len]
            .iter_mut()
            .flatten()
            .find(|e| !e.destroyed && e.obj_ref == obj_ref)
        {
            entry.result_code = code;
            entry.result_intent_ref = intent_ref;
        }
    }

    /// The top entry's result delivery info `(request_code, caller token,
    /// result_code, result_intent_ref)`, or `None` when the top wasn't
    /// launched for-result. Read before `pop` so `handle_pop_op` can deliver
    /// `onActivityResult` to the uncovered caller.
    pub fn top_result(&self) -> Option<(i32, u16, i32, Option<u16>)> {
        let entry = self.entries[..self.len].last()?.as_ref()?;
        entry
            .request_code
            .map(|rc| (rc, entry.caller, entry.result_code, entry.result_intent_ref))
    }

    /// The Intent that launched the Activity identified by `obj_ref`, for
    /// `getIntent()`. Searches the whole stack: a paused Activity below the
    /// top may legitimately call getIntent from a callback.
    pub fn intent_of(&self, obj_ref: u16) -> Option<u16> {
        self.live()
            .find(|e| e.obj_ref == obj_ref)
            .and_then(|e| e.intent_ref)
    }

    /// Pops the top entry. Returns `(obj_ref, class_name, saved_root_handle)`,
    /// or `None` if the stack was already empty.
    pub fn pop(&mut self) -> Option<(u16, &'static str, i32)> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        let entry = self.entries[self.len].take()?;
        Some((entry.obj_ref, entry.class_name, entry.root_handle))
    }

    pub fn current_root_handle(&self) -> i32 {
        if self.len == 0 {
            return 0;
        }
        match self.entries[self.len - 1].as_ref() {
            Some(e) => e.root_handle,
            None => 0,
        }
    }

    pub fn set_current_root_handle(&mut self, h: i32) {
        if self.len == 0 {
            return;
        }
        if let Some(e) = self.entries[self.len - 1].as_mut() {
            e.root_handle = h;
        }
    }

    /// Iterate over the live stack entries' `(obj_ref, class_name)` pairs
    /// from bottom to top — used by the GC visit-roots path. A reclaimed
    /// entry has no instance and is skipped.
    pub fn iter(&self) -> impl Iterator<Item = (u16, &'static str)> + '_ {
        self.live().map(|x| (x.obj_ref, x.class_name))
    }

    /// Live entries' retained Intent refs, for the GC visit-roots path —
    /// without this, the Intent backing a paused Activity's `getIntent()`
    /// would be swept on the first GC after launch.
    pub fn iter_intents(&self) -> impl Iterator<Item = u16> + '_ {
        self.entries[..self.len]
            .iter()
            .flatten()
            .filter_map(|e| e.intent_ref)
    }

    /// Live entries' pending result Intent refs, for the GC visit-roots path
    /// — a result Intent set via `setResult` must survive until it is
    /// delivered to the caller's `onActivityResult` on pop.
    pub fn iter_result_intents(&self) -> impl Iterator<Item = u16> + '_ {
        self.entries[..self.len]
            .iter()
            .flatten()
            .filter_map(|e| e.result_intent_ref)
    }

    pub fn top_saved_state(&self) -> Option<u16> {
        self.entries[..self.len].last()?.as_ref()?.saved_state_ref
    }

    /// Entries' saved-state Bundle refs, for the GC visit-roots path:
    /// mid-recreate, and for as long as an entry stays reclaimed, the Bundle
    /// is held by the entry alone.
    pub fn iter_saved_states(&self) -> impl Iterator<Item = u16> + '_ {
        self.entries[..self.len]
            .iter()
            .flatten()
            .filter_map(|e| e.saved_state_ref)
    }
}

impl Default for ActivityStack {
    fn default() -> Self {
        Self::new()
    }
}

/// Fixed-capacity FIFO of [`PendingOp`]s. Enqueue fails soft on overflow.
pub struct PendingOpQueue {
    entries: [Option<PendingOp>; MAX_PENDING_OPS],
    len: usize,
}

impl PendingOpQueue {
    pub const fn new() -> Self {
        Self {
            entries: [None; MAX_PENDING_OPS],
            len: 0,
        }
    }

    /// Append `op`. Returns `true` on success; `false` when the queue is
    /// full (caller logs and drops).
    pub fn enqueue(&mut self, op: PendingOp) -> bool {
        if self.len >= MAX_PENDING_OPS {
            return false;
        }
        self.entries[self.len] = Some(op);
        self.len += 1;
        true
    }

    /// True if any queued op is an Activity transition (push/pop). The key
    /// dispatcher uses this to stop feeding input to a departing Activity once
    /// it has launched or finished within the current frame.
    #[cfg_attr(test, allow(dead_code))]
    pub fn has_pending_activity(&self) -> bool {
        self.entries[..self.len]
            .iter()
            .flatten()
            .any(|op| matches!(op, PendingOp::Activity(_)))
    }

    /// True if a Pop for `finishing` is already queued — `finish()` is
    /// idempotent per Activity.
    /// Whether a cross-package launch is queued: this app is leaving, so
    /// anything that would push onto its Activity stack should hold off.
    pub fn has_pending_launch(&self) -> bool {
        self.entries[..self.len]
            .iter()
            .flatten()
            .any(|op| matches!(op, PendingOp::Activity(PendingActivityOp::Launch)))
    }

    pub fn has_pending_pop_for(&self, finishing: u16) -> bool {
        self.entries[..self.len].iter().flatten().any(|op| {
            matches!(op, PendingOp::Activity(PendingActivityOp::Pop { finishing: f }) if *f == finishing)
        })
    }

    pub fn has_pending_recreate_for(&self, target: u16) -> bool {
        self.entries[..self.len].iter().flatten().any(|op| {
            matches!(op, PendingOp::Activity(PendingActivityOp::Recreate { target: t }) if *t == target)
        })
    }

    /// Take the oldest queued op. Returns `None` when empty.
    pub fn take_next(&mut self) -> Option<PendingOp> {
        if self.len == 0 {
            return None;
        }
        let op = self.entries[0].take();
        for i in 1..self.len {
            self.entries[i - 1] = self.entries[i].take();
        }
        self.len -= 1;
        op
    }

    /// Invoke `visit` on every heap object reference embedded in queued
    /// ops. Used by GC root scanning — without this the `intent` / `conn` /
    /// `owner_activity` refs in a queued Service op could be swept before
    /// the op is processed.
    pub fn visit_object_refs(&self, visit: &mut dyn FnMut(u16)) {
        for op in self.entries[..self.len].iter().flatten() {
            match op {
                PendingOp::Activity(PendingActivityOp::Push { intent_ref, .. }) => {
                    if let Some(r) = intent_ref {
                        visit(*r);
                    }
                }
                PendingOp::Activity(PendingActivityOp::Pop { .. })
                | PendingOp::Activity(PendingActivityOp::Recreate { .. })
                | PendingOp::Activity(PendingActivityOp::Launch) => {}
                PendingOp::Service(svc) => match *svc {
                    PendingServiceOp::Start { intent_ref, .. } => {
                        visit(intent_ref);
                    }
                    PendingServiceOp::Stop { .. } => {}
                    PendingServiceOp::Bind {
                        intent_ref,
                        conn_ref,
                        owner_activity_ref,
                        ..
                    } => {
                        visit(intent_ref);
                        visit(conn_ref);
                        visit(owner_activity_ref);
                    }
                    PendingServiceOp::Unbind { conn_ref } => {
                        visit(conn_ref);
                    }
                },
            }
        }
    }
}

impl Default for PendingOpQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "com/test/ActA";
    const B: &str = "com/test/ActB";
    const C: &str = "com/test/ActC";

    // ── Activity stack ──────────────────────────────────────────────────────

    #[test]
    fn has_pending_pop_for_distinguishes_finishing_activities() {
        // finish(); finish(); from one Activity must collapse to a single Pop,
        // while two different Activities finishing both get theirs.
        let mut q = PendingOpQueue::new();
        assert!(!q.has_pending_pop_for(7));
        assert!(q.enqueue(PendingOp::Activity(PendingActivityOp::Pop { finishing: 7 })));
        assert!(q.has_pending_pop_for(7));
        assert!(!q.has_pending_pop_for(8));
        q.take_next();
        assert!(!q.has_pending_pop_for(7));
    }

    #[test]
    fn new_stack_is_empty() {
        let s = ActivityStack::new();
        assert!(s.current().is_none());
        assert_eq!(s.current_root_handle(), 0);
    }

    #[test]
    fn push_then_current_returns_pushed() {
        let mut s = ActivityStack::new();
        assert!(s.push(7, A, None, None, 0));
        assert_eq!(s.current(), Some((7, A)));
    }

    #[test]
    fn replace_top_keeps_the_launch_and_resets_the_instance() {
        // recreate(): the Intent and the for-result launch belong to the
        // launch and carry over; the root view and the result belonged to
        // the old instance and do not.
        let mut s = ActivityStack::new();
        s.push(1, A, None, None, 0);
        let a = s.token_of(1);
        s.push(2, B, Some(50), Some(9), a);
        s.set_current_root_handle(77);
        s.set_result(2, -1, Some(51));
        s.replace_top(3);
        assert_eq!(s.current(), Some((3, B)));
        assert_eq!(s.intent_of(3), Some(50));
        assert_eq!(s.intent_of(2), None);
        assert_eq!(s.current_root_handle(), 0);
        assert_eq!(s.top_result(), Some((9, a, 0, None)));
        assert_eq!(s.iter_result_intents().count(), 0);
    }

    /// T3.1-D: a reclaimed entry keeps its place and identity, drops out of
    /// every by-obj_ref lookup and of the instance roots, and comes back
    /// through `replace_top`.
    #[test]
    fn a_reclaimed_entry_keeps_its_token_and_leaves_the_obj_ref_lookups() {
        let mut s = ActivityStack::new();
        s.push(10, A, Some(70), None, 0);
        let a = s.token_of(10);
        assert_ne!(a, 0);
        s.push(20, B, None, Some(5), a);
        // The top is in the foreground: never a reclaim candidate.
        assert_eq!(s.covered(1), None);
        s.set_current_root_handle(0);
        assert_eq!(s.covered(0), Some((10, A, 0)));

        s.set_saved_state_at(0, Some(60));
        s.mark_destroyed(0);
        assert_eq!(s.covered(0), None, "reclaimed once");
        assert_eq!(s.token_of(10), 0);
        assert_eq!(s.intent_of(10), None);
        assert_eq!(s.iter().collect::<Vec<_>>(), [(20, B)]);
        assert_eq!(s.iter_saved_states().collect::<Vec<_>>(), [60]);
        assert_eq!(
            s.iter_intents().collect::<Vec<_>>(),
            [70],
            "launch Intent stays"
        );
        s.set_result(0, -1, None); // obj_ref 0 must not match the dead entry

        // B finishes: its caller is still identified, by token.
        let (_, caller, _, _) = s.top_result().unwrap();
        s.pop();
        assert!(s.top_destroyed());
        assert_eq!(caller, s.top_token());
        assert_eq!(s.top_saved_state(), Some(60));

        s.replace_top(11);
        assert!(!s.top_destroyed());
        assert_eq!(s.token_of(11), a, "same entry, new instance");
        assert_eq!(s.intent_of(11), Some(70));
    }

    #[test]
    fn tokens_are_never_zero_across_the_wrap() {
        let mut s = ActivityStack::new();
        s.next_token = u16::MAX;
        s.push(1, A, None, None, 0);
        s.push(2, B, None, None, 0);
        assert_eq!(s.token_of(1), u16::MAX);
        assert_eq!(s.token_of(2), 1);
    }

    #[test]
    fn saved_state_is_rooted_on_the_top_entry_until_dropped() {
        let mut s = ActivityStack::new();
        s.set_top_saved_state(Some(5)); // empty stack: nowhere to put it
        assert_eq!(s.top_saved_state(), None);
        s.push(1, A, None, None, 0);
        s.push(2, B, None, None, 0);
        s.set_top_saved_state(Some(60));
        assert_eq!(s.top_saved_state(), Some(60));
        assert_eq!(s.iter_saved_states().collect::<Vec<_>>(), [60]);
        // It rides through the instance swap, which is when it is needed.
        s.replace_top(3);
        assert_eq!(s.top_saved_state(), Some(60));
        s.set_top_saved_state(None);
        assert_eq!(s.iter_saved_states().count(), 0);
    }

    #[test]
    fn has_pending_recreate_for_collapses_repeats() {
        let mut q = PendingOpQueue::new();
        assert!(!q.has_pending_recreate_for(7));
        assert!(q.enqueue(PendingOp::Activity(PendingActivityOp::Recreate {
            target: 7
        })));
        assert!(q.has_pending_recreate_for(7));
        assert!(!q.has_pending_recreate_for(8));
        assert!(!q.has_pending_pop_for(7));
    }

    #[test]
    fn current_returns_top_of_stack() {
        let mut s = ActivityStack::new();
        s.push(1, A, None, None, 0);
        s.push(2, B, None, None, 0);
        s.push(3, C, None, None, 0);
        assert_eq!(s.current(), Some((3, C)));
    }

    #[test]
    fn pop_uncovers_parent() {
        let mut s = ActivityStack::new();
        s.push(1, A, None, None, 0);
        s.push(2, B, None, None, 0);
        let popped = s.pop();
        assert_eq!(popped, Some((2, B, 0)));
        assert_eq!(s.current(), Some((1, A)));
    }

    #[test]
    fn pop_on_empty_is_none() {
        let mut s = ActivityStack::new();
        assert!(s.pop().is_none());
    }

    #[test]
    fn push_when_full_returns_false_without_corruption() {
        let mut s = ActivityStack::new();
        for i in 0..MAX_ACTIVITY_STACK as u16 {
            assert!(s.push(i, A, None, None, 0), "push {} should fit", i);
        }
        assert!(
            !s.push(99, B, None, None, 0),
            "push past MAX_ACTIVITY_STACK must fail"
        );
        assert_eq!(s.current(), Some((MAX_ACTIVITY_STACK as u16 - 1, A)));
    }

    /// N pushes followed by N pops must empty the stack and yield no
    /// surprise leftover state.
    #[test]
    fn push_pop_round_trip_is_symmetric() {
        let mut s = ActivityStack::new();
        for i in 0..MAX_ACTIVITY_STACK as u16 {
            s.push(i, A, None, None, 0);
        }
        for _ in 0..MAX_ACTIVITY_STACK {
            assert!(s.pop().is_some());
        }
        assert!(s.current().is_none());
        assert!(s.pop().is_none());
    }

    // ── Root handle ─────────────────────────────────────────────────────────

    #[test]
    fn set_current_root_handle_updates_top_entry() {
        let mut s = ActivityStack::new();
        s.push(1, A, None, None, 0);
        s.set_current_root_handle(42);
        assert_eq!(s.current_root_handle(), 42);
    }

    #[test]
    fn set_current_root_handle_on_empty_stack_is_noop() {
        let mut s = ActivityStack::new();
        s.set_current_root_handle(42);
        assert_eq!(s.current_root_handle(), 0);
        assert!(s.current().is_none());
    }

    /// Pushing a child must not disturb the parent's saved root handle.
    /// This is the contract that lets a parent Activity's view tree survive
    /// while a child is on top, then restore on pop.
    #[test]
    fn pushing_child_preserves_parent_root_handle() {
        let mut s = ActivityStack::new();
        s.push(1, A, None, None, 0);
        s.set_current_root_handle(11);
        s.push(2, B, None, None, 0);
        assert_eq!(s.current_root_handle(), 0);
        s.pop();
        assert_eq!(s.current_root_handle(), 11);
    }

    /// Pop must surface the saved root_handle so the lifecycle caller can
    /// `g.delete()` the view tree.
    #[test]
    fn pop_returns_saved_root_handle() {
        let mut s = ActivityStack::new();
        s.push(7, A, None, None, 0);
        s.set_current_root_handle(123);
        assert_eq!(s.pop(), Some((7, A, 123)));
    }

    #[test]
    fn iter_walks_bottom_to_top() {
        let mut s = ActivityStack::new();
        s.push(1, A, None, None, 0);
        s.push(2, B, None, None, 0);
        s.push(3, C, None, None, 0);
        let v: alloc::vec::Vec<_> = s.iter().collect();
        assert_eq!(v, alloc::vec![(1, A), (2, B), (3, C)]);
    }

    // ── Pending op queue ────────────────────────────────────────────────────

    #[test]
    fn pending_ops_drain_fifo() {
        let mut q = PendingOpQueue::new();
        q.enqueue(PendingOp::Activity(PendingActivityOp::Push {
            class_name: A,
            intent_ref: None,
            request_code: None,
            caller: 0,
        }));
        q.enqueue(PendingOp::Activity(PendingActivityOp::Push {
            class_name: B,
            intent_ref: None,
            request_code: None,
            caller: 0,
        }));
        q.enqueue(PendingOp::Activity(PendingActivityOp::Pop { finishing: 0 }));
        match q.take_next() {
            Some(PendingOp::Activity(PendingActivityOp::Push { class_name, .. })) => {
                assert_eq!(class_name, A)
            }
            other => panic!("expected first Push(A), got {:?}", other),
        }
        match q.take_next() {
            Some(PendingOp::Activity(PendingActivityOp::Push { class_name, .. })) => {
                assert_eq!(class_name, B)
            }
            other => panic!("expected second Push(B), got {:?}", other),
        }
        match q.take_next() {
            Some(PendingOp::Activity(PendingActivityOp::Pop { finishing: 0 })) => {}
            other => panic!("expected Pop, got {:?}", other),
        }
        assert!(q.take_next().is_none());
    }

    #[test]
    fn pending_ops_take_on_empty_is_none() {
        let mut q = PendingOpQueue::new();
        assert!(q.take_next().is_none());
    }

    #[test]
    fn pending_ops_full_queue_rejects_further() {
        let mut q = PendingOpQueue::new();
        for _ in 0..MAX_PENDING_OPS {
            assert!(q.enqueue(PendingOp::Activity(PendingActivityOp::Pop { finishing: 0 })));
        }
        assert!(
            !q.enqueue(PendingOp::Activity(PendingActivityOp::Pop { finishing: 0 })),
            "enqueue past MAX_PENDING_OPS must return false"
        );
    }

    /// Activity and Service ops share the queue and must preserve insertion
    /// order — a `startActivity` then `startService` from the same frame
    /// processes Activity-first per Android semantics.
    #[test]
    fn pending_ops_preserve_activity_service_interleaving() {
        let mut q = PendingOpQueue::new();
        q.enqueue(PendingOp::Activity(PendingActivityOp::Push {
            class_name: A,
            intent_ref: None,
            request_code: None,
            caller: 0,
        }));
        q.enqueue(PendingOp::Service(PendingServiceOp::Stop { class_name: B }));
        q.enqueue(PendingOp::Activity(PendingActivityOp::Pop { finishing: 0 }));
        assert!(matches!(
            q.take_next(),
            Some(PendingOp::Activity(PendingActivityOp::Push { .. }))
        ));
        assert!(matches!(
            q.take_next(),
            Some(PendingOp::Service(PendingServiceOp::Stop { .. }))
        ));
        assert!(matches!(
            q.take_next(),
            Some(PendingOp::Activity(PendingActivityOp::Pop { finishing: 0 }))
        ));
    }

    // ── GC root visiting ────────────────────────────────────────────────────

    #[test]
    fn visit_object_refs_skips_activity_only_ops() {
        let mut q = PendingOpQueue::new();
        q.enqueue(PendingOp::Activity(PendingActivityOp::Push {
            class_name: A,
            intent_ref: None,
            request_code: None,
            caller: 0,
        }));
        q.enqueue(PendingOp::Activity(PendingActivityOp::Pop { finishing: 0 }));
        let mut visited: alloc::vec::Vec<u16> = alloc::vec::Vec::new();
        q.visit_object_refs(&mut |r| visited.push(r));
        assert!(visited.is_empty(), "activity-only ops carry no heap refs");
    }

    #[test]
    fn visit_object_refs_yields_service_intent_and_conn() {
        let mut q = PendingOpQueue::new();
        q.enqueue(PendingOp::Service(PendingServiceOp::Start {
            class_name: A,
            intent_ref: 11,
        }));
        q.enqueue(PendingOp::Service(PendingServiceOp::Bind {
            class_name: B,
            intent_ref: 22,
            conn_ref: 33,
            owner_activity_ref: 44,
        }));
        q.enqueue(PendingOp::Service(PendingServiceOp::Unbind {
            conn_ref: 55,
        }));
        // Stop has only a class name, no heap refs.
        q.enqueue(PendingOp::Service(PendingServiceOp::Stop { class_name: C }));
        let mut visited: alloc::vec::Vec<u16> = alloc::vec::Vec::new();
        q.visit_object_refs(&mut |r| visited.push(r));
        assert_eq!(visited, alloc::vec![11, 22, 33, 44, 55]);
    }
}
