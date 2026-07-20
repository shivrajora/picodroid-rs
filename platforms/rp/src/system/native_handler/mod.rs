// SPDX-License-Identifier: GPL-3.0-only
#[cfg(not(feature = "sim"))]
use pico_jvm::types::MonitorKey;
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext, NativeMethodHandler,
};

mod app_services;
mod concurrent;
mod graphics;
mod io;
#[cfg(has_network)]
mod net;
#[cfg(not(has_network))]
mod net_stub;
mod os;
mod pio;
#[cfg(not(test))]
mod sensors;

mod class_registry;
pub use class_registry::PICODROID_NATIVE_CLASSES;

pub mod state;
pub use state::{PendingActivityOp, PendingOp, PendingServiceOp};

pub struct PicodroidNativeHandler {
    /// Resettable counters exposed to Java via Runtime.gcTimeNanos() etc.
    gc_time_ns: u64,
    gc_count: u32,
    gc_freed: u32,
    /// Cumulative counters (never reset) for sim summary output.
    #[cfg_attr(not(feature = "sim"), allow(dead_code))]
    total_gc_time_ns: u64,
    #[cfg_attr(not(feature = "sim"), allow(dead_code))]
    total_gc_count: u32,
    #[cfg_attr(not(feature = "sim"), allow(dead_code))]
    total_gc_freed: u32,
    /// Live-bytes high-water mark exposed to Java via `Runtime.peakMemory()`.
    /// Updated from `report_gc` (pre-sweep sample, catches GC-trigger peaks)
    /// and from each `Runtime.usedMemory()` call. Reset via `resetPeakMemory()`.
    peak_used: u32,
    /// Active Activity stack — top is at `len - 1`. Empty before the first
    /// `startActivity` and after the last `finish()`.
    activity_stack: state::ActivityStack,
    /// Pending Activity / Service ops queued by Java in FIFO order, drained
    /// by [`crate::lifecycle`] between frames. A typical Activity onCreate
    /// that does both `startService` and `bindService` queues two ops;
    /// excess ops past [`MAX_PENDING_OPS`] are silently dropped (logged).
    pending_ops: state::PendingOpQueue,
}

impl PicodroidNativeHandler {
    pub fn new() -> Self {
        Self {
            gc_time_ns: 0,
            gc_count: 0,
            gc_freed: 0,
            total_gc_time_ns: 0,
            total_gc_count: 0,
            total_gc_freed: 0,
            peak_used: 0,
            activity_stack: state::ActivityStack::new(),
            pending_ops: state::PendingOpQueue::new(),
        }
    }

    /// Append `op` to the pending queue. Returns `true` on success; `false`
    /// (with a log) if the queue is full — apps shouldn't be queueing more
    /// than [`MAX_PENDING_OPS`] transitions per frame.
    pub fn enqueue_op(&mut self, op: PendingOp) -> bool {
        self.pending_ops.enqueue(op)
    }

    /// Pop the oldest pending op (FIFO). Returns `None` when the queue is
    /// empty.
    pub fn take_next_pending_op(&mut self) -> Option<PendingOp> {
        self.pending_ops.take_next()
    }

    /// True if an Activity transition (startActivity / finish) is queued but
    /// not yet applied. The key dispatcher stops feeding input to a departing
    /// Activity once it has launched or finished within the frame.
    pub fn has_pending_activity_transition(&self) -> bool {
        self.pending_ops.has_pending_activity()
    }

    /// Top of the activity stack as `(obj_ref, class_name)`, or `None` when
    /// the stack is empty.
    pub fn current_activity(&self) -> Option<(u16, &'static str)> {
        self.activity_stack.current()
    }

    /// Push an Activity entry onto the stack. Returns `false` and silently
    /// drops the push if the stack is full — apps can't sensibly recover
    /// from a 9-deep nav stack on an MCU, so we'd rather fail soft than
    /// thread a Result through `dispatch`.
    pub fn push_activity(
        &mut self,
        obj_ref: u16,
        class_name: &'static str,
        intent_ref: Option<u16>,
        request_code: Option<i32>,
        caller_ref: u16,
    ) -> bool {
        self.activity_stack
            .push(obj_ref, class_name, intent_ref, request_code, caller_ref)
    }

    /// `Activity.setResult` — record the result on the calling Activity's
    /// stack entry, delivered to its launcher's `onActivityResult` on pop.
    pub fn set_activity_result(&mut self, obj_ref: u16, code: i32, intent_ref: Option<u16>) {
        self.activity_stack.set_result(obj_ref, code, intent_ref);
    }

    /// Shared body of `startActivity` / `startActivityForResult`: resolve the
    /// Intent's target class and enqueue a Push op carrying the result-launch
    /// metadata. `args[1]` is the Intent ObjectRef.
    fn enqueue_activity_push(
        &mut self,
        ctx: &NativeContext<'_>,
        request_code: Option<i32>,
        caller_ref: u16,
    ) {
        if let Some(Value::ObjectRef(intent_ref)) = ctx.args.get(1) {
            if let Some(Value::Reference(name_idx)) = ctx.objects.get_field(*intent_ref, 0) {
                if let Some(class_name) = ctx.strings.resolve(name_idx) {
                    // targetClassName is `Class.getName().replace('.', '/')` — a
                    // runtime dynamic String the GC can free — so canonicalize to
                    // the loaded class file's Flash-backed name before storing it
                    // in the enqueued op, rather than transmuting to `&'static`.
                    if let Some(static_name) = ctx.canonical_class_name(class_name) {
                        self.enqueue_op(PendingOp::Activity(PendingActivityOp::Push {
                            class_name: static_name,
                            intent_ref: Some(*intent_ref),
                            request_code,
                            caller_ref,
                        }));
                    }
                }
            }
        }
    }

    /// The top Activity's pending-result tuple `(request_code, caller_ref,
    /// result_code, result_intent_ref)`, or `None` when it wasn't launched
    /// for-result. Read by `handle_pop_op` before popping.
    pub fn top_activity_result(&self) -> Option<(i32, u16, i32, Option<u16>)> {
        self.activity_stack.top_result()
    }

    /// Pop the top activity off the stack. Returns the popped entry, or
    /// `None` if the stack was already empty. The third tuple element is
    /// the saved content-view handle (`0` if none); callers that own
    /// teardown must `g.delete` it to free the view tree.
    pub fn pop_activity(&mut self) -> Option<(u16, &'static str, i32)> {
        self.activity_stack.pop()
    }

    /// Saved content-view handle of the top entry, or `0` if the stack
    /// is empty / the top Activity has not yet called `setContentView`.
    pub fn current_root_handle(&self) -> i32 {
        self.activity_stack.current_root_handle()
    }

    /// Set the saved content-view handle on the top entry. No-op when the
    /// stack is empty.
    pub fn set_current_root_handle(&mut self, h: i32) {
        self.activity_stack.set_current_root_handle(h);
    }

    /// Returns cumulative (gc_time_ns, gc_count, gc_freed) for the entire run.
    #[cfg_attr(not(feature = "sim"), allow(dead_code))]
    pub fn gc_stats(&self) -> (u64, u32, u32) {
        (
            self.total_gc_time_ns,
            self.total_gc_count,
            self.total_gc_freed,
        )
    }
}

impl NativeMethodHandler for PicodroidNativeHandler {
    fn clock_nanos(&self) -> u64 {
        crate::hal::system_clock::elapsed_realtime_nanos() as u64
    }

    fn native_class_names(&self) -> &'static [&'static str] {
        PICODROID_NATIVE_CLASSES
    }

    fn gc_visit_roots(&self, visit: &mut dyn FnMut(Value)) {
        // Activity stack: every entry holds the Activity instance whose
        // lifecycle methods are about to be invoked. Without rooting these,
        // GC during a quiet frame (between `onResume` and the next callback)
        // sweeps the Activity out from under us.
        for (obj_ref, _class) in self.activity_stack.iter() {
            visit(Value::ObjectRef(obj_ref));
        }
        // ...and each entry's retained launch Intent, which backs
        // Activity.getIntent() for the entry's whole lifetime.
        for intent_ref in self.activity_stack.iter_intents() {
            visit(Value::ObjectRef(intent_ref));
        }
        // ...and any pending setResult Intent, which must survive until it is
        // delivered to the caller's onActivityResult on pop.
        for intent_ref in self.activity_stack.iter_result_intents() {
            visit(Value::ObjectRef(intent_ref));
        }

        // Pending ops: the Service `intent` / `conn` / `owner_activity`
        // references and the Activity Push `intent_ref` must survive until
        // [`take_next_pending_op`] runs the op.
        self.pending_ops
            .visit_object_refs(&mut |r| visit(Value::ObjectRef(r)));

        // View/dialog callback targets: a View (or AlertDialog) referenced only
        // by a native listener map — key/touch/swipe/click/dialog — and not by a
        // Java field would otherwise be swept by GC, after which dispatch
        // resolves a live lv_obj to a dead ref and input silently drops (the
        // keypad appears to "lose focus" a few seconds in, post-GC).
        {
            use crate::system::picodroid::graphics::lvgl::{events, widgets};
            let mut root = |r: u16| visit(Value::ObjectRef(r));
            events::visit_view_listener_roots(&mut root);
            widgets::button::visit_click_listener_roots(&mut root);
            widgets::button::visit_long_click_listener_roots(&mut root);
            crate::system::picodroid::graphics::lvgl::animations::visit_end_action_roots(&mut root);
            widgets::list_view::visit_item_click_listener_roots(&mut root);
            widgets::alert_dialog::visit_dialog_obj_roots(&mut root);
            // Compound-button + EditText listener maps: same unrooted-View hazard
            // as the click/item-click maps above. A Switch/CheckBox/ToggleButton
            // or EditText kept alive only by its native listener map (a local in
            // onCreate, never stored in a Java field) is otherwise swept on the
            // first GC, its slot reused, and the next Activity's onCreate hits a
            // dead ref → NoSuchMethod.
            widgets::switch::visit_checked_change_listener_roots(&mut root);
            widgets::check_box::visit_checked_change_listener_roots(&mut root);
            widgets::radio_button::visit_checked_change_listener_roots(&mut root);
            widgets::toggle_button::visit_checked_change_listener_roots(&mut root);
            widgets::edit_text::visit_editor_action_listener_roots(&mut root);
            widgets::edit_text::visit_text_changed_listener_roots(&mut root);
            // NumberPicker registers every instance (not just listener
            // holders): the obj_ref is the fireStep dispatch target.
            widgets::number_picker::visit_picker_roots(&mut root);
        }

        // Delegate to sub-modules that own their own native object refs.
        crate::system::picodroid::graphics::display::visit_gc_roots(&mut *visit);
        crate::system::picodroid::hardware::sensors::visit_gc_roots(&mut *visit);
        crate::service_lifecycle::visit_gc_roots(&mut *visit);
        crate::lifecycle::visit_gc_roots(&mut *visit);
    }

    fn report_gc(&mut self, time_ns: u64, freed: usize, pre_gc_used: usize) {
        self.gc_time_ns += time_ns;
        self.gc_count += 1;
        self.gc_freed += freed as u32;
        self.total_gc_time_ns += time_ns;
        self.total_gc_count += 1;
        self.total_gc_freed += freed as u32;
        // GC fires at heap pressure peaks — sample the high-water mark.
        let pre_gc_used = pre_gc_used.min(u32::MAX as usize) as u32;
        if pre_gc_used > self.peak_used {
            self.peak_used = pre_gc_used;
        }
    }

    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        // Delegate to domain-specific sub-handlers. Each sub-dispatcher
        // reverse-translates its own class_name arg at entry; we pass the
        // raw (possibly-shrunk) name through.
        if let result @ Some(_) = pio::dispatch(class_name, method_name, ctx) {
            return result;
        }
        if let result @ Some(_) = os::dispatch(class_name, method_name, ctx) {
            return result;
        }
        if let result @ Some(_) = concurrent::dispatch(class_name, method_name, ctx) {
            return result;
        }
        if let result @ Some(_) = graphics::dispatch(class_name, method_name, ctx) {
            return result;
        }
        if let result @ Some(_) = io::dispatch(class_name, method_name, ctx) {
            return result;
        }
        #[cfg(has_network)]
        if let result @ Some(_) = net::dispatch(class_name, method_name, ctx) {
            return result;
        }
        #[cfg(not(has_network))]
        if let result @ Some(_) = net_stub::dispatch(class_name, method_name, ctx) {
            return result;
        }
        #[cfg(not(test))]
        if let result @ Some(_) = sensors::dispatch(class_name, method_name, ctx) {
            return result;
        }

        let class_name = crate::shrink_names::unshrink_class(class_name);
        // Service / notification dispatch — needs `self` for the pending-op
        // queue, so it lives outside the module-style sub-dispatchers above.
        if let result @ Some(_) = app_services::dispatch(self, class_name, method_name, ctx) {
            return result;
        }
        // Arms that need access to `self` stay here.
        use crate::system::picodroid::util::log::{self, LogLevel};
        match (class_name, method_name) {
            ("picodroid/util/Log", "v") => {
                Some(log::log(LogLevel::Verbose, ctx.args, ctx.strings).map(|_| None))
            }
            ("picodroid/util/Log", "d") => {
                Some(log::log(LogLevel::Debug, ctx.args, ctx.strings).map(|_| None))
            }
            ("picodroid/util/Log", "i") => {
                Some(log::log(LogLevel::Info, ctx.args, ctx.strings).map(|_| None))
            }
            ("picodroid/util/Log", "w") => {
                Some(log::log(LogLevel::Warn, ctx.args, ctx.strings).map(|_| None))
            }
            ("picodroid/util/Log", "e") => {
                Some(log::log(LogLevel::Error, ctx.args, ctx.strings).map(|_| None))
            }
            ("picodroid/os/Runtime", "gcTimeNanos") => {
                Some(Ok(Some(Value::Long(self.gc_time_ns as i64))))
            }
            ("picodroid/os/Runtime", "gcCount") => Some(Ok(Some(Value::Int(self.gc_count as i32)))),
            ("picodroid/os/Runtime", "gcFreed") => Some(Ok(Some(Value::Int(self.gc_freed as i32)))),
            ("picodroid/os/Runtime", "resetGcStats") => {
                self.gc_time_ns = 0;
                self.gc_count = 0;
                self.gc_freed = 0;
                Some(Ok(None))
            }
            ("picodroid/os/Runtime", "usedMemory") => {
                let used =
                    ctx.objects.live_bytes() + ctx.arrays.live_bytes() + ctx.strings.live_bytes();
                let used32 = used.min(u32::MAX as usize) as u32;
                if used32 > self.peak_used {
                    self.peak_used = used32;
                }
                Some(Ok(Some(Value::Long(used as i64))))
            }
            ("picodroid/os/Runtime", "peakMemory") => {
                Some(Ok(Some(Value::Long(self.peak_used as i64))))
            }
            ("picodroid/os/Runtime", "resetPeakMemory") => {
                let used =
                    ctx.objects.live_bytes() + ctx.arrays.live_bytes() + ctx.strings.live_bytes();
                self.peak_used = used.min(u32::MAX as usize) as u32;
                Some(Ok(None))
            }
            // ── Application / Activity navigation ───────────────────────
            // Match any class name: invokevirtual dispatches with the runtime
            // subclass name (e.g. "displaydemo/DisplayDemoApp"), not the declaring
            // class "picodroid/app/Application".
            (_, "startActivity") => {
                self.enqueue_activity_push(ctx, None, 0);
                Some(Ok(None))
            }
            (_, "startActivityForResult") => {
                // args[0] = this (Activity), args[1] = Intent, args[2] = int requestCode.
                // The receiver is the caller that will get onActivityResult.
                let caller_ref = match ctx.args.first() {
                    Some(Value::ObjectRef(r)) => *r,
                    _ => 0,
                };
                let request_code = match ctx.args.get(2) {
                    Some(Value::Int(c)) => *c,
                    _ => 0,
                };
                self.enqueue_activity_push(ctx, Some(request_code), caller_ref);
                Some(Ok(None))
            }
            (_, "setResult") => {
                // args[0] = this (Activity), args[1] = int resultCode,
                // optional args[2] = Intent. Recorded on the caller's stack
                // entry, delivered to its launcher on finish.
                if let Some(Value::ObjectRef(obj)) = ctx.args.first() {
                    let code = match ctx.args.get(1) {
                        Some(Value::Int(c)) => *c,
                        _ => 0,
                    };
                    let intent_ref = match ctx.args.get(2) {
                        Some(Value::ObjectRef(r)) => Some(*r),
                        _ => None,
                    };
                    self.set_activity_result(*obj, code, intent_ref);
                }
                Some(Ok(None))
            }
            // `Activity.finish()` — request a pop. The handler doesn't
            // validate that the caller is actually the current top: even if
            // a paused Activity calls finish, a single Pop op still pops
            // exactly the top, and that's the documented Android behavior
            // ("a paused Activity finishing itself" doesn't happen unless
            // the app is misbehaving anyway).
            (_, "getIntent") => {
                // args[0] = this (an Activity). Returns the Intent retained on
                // the stack entry at push time, or null for the boot Activity
                // — Android's contract.
                let intent = match ctx.args.first() {
                    Some(Value::ObjectRef(obj)) => self.activity_stack.intent_of(*obj),
                    _ => None,
                };
                Some(Ok(Some(match intent {
                    Some(r) => Value::ObjectRef(r),
                    None => Value::Null,
                })))
            }
            (_, "finish") => {
                self.enqueue_op(PendingOp::Activity(PendingActivityOp::Pop));
                Some(Ok(None))
            }
            _ => {
                // True native miss: no sub-dispatcher or arm above claimed this
                // (class, method). The JVM turns our None into NoSuchMethod; if
                // it's a known Android idiom picodroid omits, log the picodroid
                // alternative first (devs live in the sim, so println there).
                if let Some(hint) = class_registry::api_hint(class_name, method_name) {
                    #[cfg(not(feature = "sim"))]
                    defmt::warn!(
                        "no native {=str}.{=str} — {=str}",
                        class_name,
                        method_name,
                        hint
                    );
                    #[cfg(feature = "sim")]
                    eprintln!("[sim] no native {class_name}.{method_name} — {hint}");
                }
                None
            }
        }
    }

    #[cfg(all(not(any(test, feature = "sim")), feature = "family-rp"))]
    fn interrupted(&self) -> bool {
        crate::pdb::pending::is_stop_jvm()
    }

    #[cfg(all(not(any(test, feature = "sim")), not(feature = "family-rp")))]
    fn interrupted(&self) -> bool {
        false
    }

    #[cfg(not(feature = "sim"))]
    fn monitor_enter(&mut self, key: MonitorKey) -> Result<(), JvmError> {
        crate::system::monitor_store::enter(key)
    }

    #[cfg(not(feature = "sim"))]
    fn monitor_exit(&mut self, key: MonitorKey) -> Result<(), JvmError> {
        crate::system::monitor_store::exit(key)
    }

    #[cfg(not(feature = "sim"))]
    fn monitors_clear(&mut self) {
        crate::system::monitor_store::clear();
    }
}
