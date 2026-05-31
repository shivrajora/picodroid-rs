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
    /// current top, if any, is paused first.
    Push { class_name: &'static str },
    /// `Activity.finish()` — pop the current top off the stack. If the
    /// stack is left empty, [`run_activity`] returns and the app exits.
    Pop,
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
        class_name: &'static str,
        intent_ref: u16,
    },
    /// `Context.stopService(intent)` or `Service.stopSelf()` — clear the
    /// started flag; if no clients are bound, run `onDestroy`.
    Stop { class_name: &'static str },
    /// `Context.bindService(intent, conn)` — `onCreate` (first time) then
    /// `onBind`, then deliver the IBinder to `conn.onServiceConnected`.
    Bind {
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
    obj_ref: u16,
    class_name: &'static str,
    /// Java `nativeHandle` of the content view installed by this Activity's
    /// most recent `setContentView`. `0` = no view set yet, or the view has
    /// been freed. Snapshotted from `display::CURRENT_ROOT_ID` on push (so
    /// the view survives while a child Activity is on top) and restored
    /// back into `CURRENT_ROOT_ID` on pop.
    root_handle: i32,
}

/// Fixed-capacity LIFO of Activity entries. Push fails soft (returns
/// `false`) on overflow rather than threading a Result through the JVM
/// dispatch path — there's no useful recovery for a 9-deep nav stack on
/// an MCU.
pub struct ActivityStack {
    entries: [Option<ActivityStackEntry>; MAX_ACTIVITY_STACK],
    len: usize,
}

impl ActivityStack {
    pub const fn new() -> Self {
        Self {
            entries: [None; MAX_ACTIVITY_STACK],
            len: 0,
        }
    }

    pub fn current(&self) -> Option<(u16, &'static str)> {
        if self.len == 0 {
            return None;
        }
        let entry = self.entries[self.len - 1].as_ref()?;
        Some((entry.obj_ref, entry.class_name))
    }

    pub fn push(&mut self, obj_ref: u16, class_name: &'static str) -> bool {
        if self.len >= MAX_ACTIVITY_STACK {
            return false;
        }
        self.entries[self.len] = Some(ActivityStackEntry {
            obj_ref,
            class_name,
            root_handle: 0,
        });
        self.len += 1;
        true
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
    /// from bottom to top — used by the GC visit-roots path.
    pub fn iter(&self) -> impl Iterator<Item = (u16, &'static str)> + '_ {
        self.entries[..self.len]
            .iter()
            .filter_map(|e| e.as_ref().map(|x| (x.obj_ref, x.class_name)))
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
    pub fn has_pending_activity(&self) -> bool {
        self.entries[..self.len]
            .iter()
            .flatten()
            .any(|op| matches!(op, PendingOp::Activity(_)))
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
                PendingOp::Activity(_) => {}
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
    fn new_stack_is_empty() {
        let s = ActivityStack::new();
        assert!(s.current().is_none());
        assert_eq!(s.current_root_handle(), 0);
    }

    #[test]
    fn push_then_current_returns_pushed() {
        let mut s = ActivityStack::new();
        assert!(s.push(7, A));
        assert_eq!(s.current(), Some((7, A)));
    }

    #[test]
    fn current_returns_top_of_stack() {
        let mut s = ActivityStack::new();
        s.push(1, A);
        s.push(2, B);
        s.push(3, C);
        assert_eq!(s.current(), Some((3, C)));
    }

    #[test]
    fn pop_uncovers_parent() {
        let mut s = ActivityStack::new();
        s.push(1, A);
        s.push(2, B);
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
            assert!(s.push(i, A), "push {} should fit", i);
        }
        assert!(!s.push(99, B), "push past MAX_ACTIVITY_STACK must fail");
        assert_eq!(s.current(), Some((MAX_ACTIVITY_STACK as u16 - 1, A)));
    }

    /// N pushes followed by N pops must empty the stack and yield no
    /// surprise leftover state.
    #[test]
    fn push_pop_round_trip_is_symmetric() {
        let mut s = ActivityStack::new();
        for i in 0..MAX_ACTIVITY_STACK as u16 {
            s.push(i, A);
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
        s.push(1, A);
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
        s.push(1, A);
        s.set_current_root_handle(11);
        s.push(2, B);
        assert_eq!(s.current_root_handle(), 0);
        s.pop();
        assert_eq!(s.current_root_handle(), 11);
    }

    /// Pop must surface the saved root_handle so the lifecycle caller can
    /// `g.delete()` the view tree.
    #[test]
    fn pop_returns_saved_root_handle() {
        let mut s = ActivityStack::new();
        s.push(7, A);
        s.set_current_root_handle(123);
        assert_eq!(s.pop(), Some((7, A, 123)));
    }

    #[test]
    fn iter_walks_bottom_to_top() {
        let mut s = ActivityStack::new();
        s.push(1, A);
        s.push(2, B);
        s.push(3, C);
        let v: alloc::vec::Vec<_> = s.iter().collect();
        assert_eq!(v, alloc::vec![(1, A), (2, B), (3, C)]);
    }

    // ── Pending op queue ────────────────────────────────────────────────────

    #[test]
    fn pending_ops_drain_fifo() {
        let mut q = PendingOpQueue::new();
        q.enqueue(PendingOp::Activity(PendingActivityOp::Push {
            class_name: A,
        }));
        q.enqueue(PendingOp::Activity(PendingActivityOp::Push {
            class_name: B,
        }));
        q.enqueue(PendingOp::Activity(PendingActivityOp::Pop));
        match q.take_next() {
            Some(PendingOp::Activity(PendingActivityOp::Push { class_name })) => {
                assert_eq!(class_name, A)
            }
            other => panic!("expected first Push(A), got {:?}", other),
        }
        match q.take_next() {
            Some(PendingOp::Activity(PendingActivityOp::Push { class_name })) => {
                assert_eq!(class_name, B)
            }
            other => panic!("expected second Push(B), got {:?}", other),
        }
        match q.take_next() {
            Some(PendingOp::Activity(PendingActivityOp::Pop)) => {}
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
            assert!(q.enqueue(PendingOp::Activity(PendingActivityOp::Pop)));
        }
        assert!(
            !q.enqueue(PendingOp::Activity(PendingActivityOp::Pop)),
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
        }));
        q.enqueue(PendingOp::Service(PendingServiceOp::Stop { class_name: B }));
        q.enqueue(PendingOp::Activity(PendingActivityOp::Pop));
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
            Some(PendingOp::Activity(PendingActivityOp::Pop))
        ));
    }

    // ── GC root visiting ────────────────────────────────────────────────────

    #[test]
    fn visit_object_refs_skips_activity_only_ops() {
        let mut q = PendingOpQueue::new();
        q.enqueue(PendingOp::Activity(PendingActivityOp::Push {
            class_name: A,
        }));
        q.enqueue(PendingOp::Activity(PendingActivityOp::Pop));
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
