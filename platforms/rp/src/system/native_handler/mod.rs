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

/// Picodroid framework class names the JVM must canonicalise to a stable
/// `&'static str` for pointer-identity caching. Returned from
/// [`PicodroidNativeHandler::native_class_names`] so the JVM never needs to
/// hardcode any `picodroid/*` names itself.
///
/// Add a class here whenever a new framework class becomes the receiver of a
/// virtual or static method call (i.e. anything dispatched through the
/// per-domain handlers in this module). Missing entries silently break virtual
/// dispatch; the `picodroid_classes_in_handlers` test in this module guards
/// against drift between the dispatch sites and this list.
pub const PICODROID_NATIVE_CLASSES: &[&str] = &[
    "picodroid/pio/Adc",
    "picodroid/pio/Gpio",
    "picodroid/pio/I2cDevice",
    "picodroid/pio/PeripheralManager",
    "picodroid/pio/Pwm",
    "picodroid/pio/SpiDevice",
    "picodroid/pio/UartDevice",
    "picodroid/os/SystemClock",
    "picodroid/util/Log",
    "picodroid/concurrent/Executor",
    "picodroid/concurrent/Executors",
    "picodroid/concurrent/MainExecutor",
    "picodroid/concurrent/BackgroundExecutor",
    "picodroid/app/Application",
    "picodroid/app/Activity",
    "picodroid/app/Service",
    "picodroid/app/IBinder",
    "picodroid/app/Notification",
    "picodroid/app/NotificationManager",
    "picodroid/content/Context",
    "picodroid/content/Intent",
    "picodroid/content/ServiceConnection",
    "picodroid/view/View",
    "picodroid/view/MotionEvent",
    "picodroid/view/KeyEvent",
    "picodroid/view/OnKeyListener",
    "picodroid/view/OnSwipeListener",
    "picodroid/view/OnTouchListener",
    "picodroid/view/GestureDetector",
    "picodroid/view/GestureDetector$OnGestureListener",
    "picodroid/view/ViewPropertyAnimator",
    "picodroid/graphics/Theme",
    "picodroid/graphics/drawable/Drawable",
    "picodroid/graphics/drawable/GradientDrawable",
    "picodroid/graphics/drawable/GradientDrawable$Orientation",
    "picodroid/graphics/Display",
    "picodroid/widget/TextView",
    "picodroid/widget/Button",
    "picodroid/widget/LinearLayout",
    "picodroid/widget/ProgressBar",
    "picodroid/widget/Switch",
    "picodroid/widget/ListView",
    "picodroid/widget/ImageView",
    "picodroid/widget/ToggleButton",
    "picodroid/widget/SeekBar",
    "picodroid/widget/CheckBox",
    "picodroid/widget/ScrollView",
    "picodroid/widget/FrameLayout",
    "picodroid/widget/Spinner",
    "picodroid/widget/DatePicker",
    "picodroid/widget/TimePicker",
    "picodroid/widget/EditText",
    "picodroid/widget/Toast",
    "picodroid/widget/Snackbar",
    "picodroid/widget/SwipeRefreshLayout",
    "picodroid/widget/AlertDialog",
    "picodroid/widget/AlertDialog$Builder",
    "picodroid/widget/Keyboard",
    "picodroid/net/Socket",
    "picodroid/net/ServerSocket",
    "picodroid/net/DatagramSocket",
    "picodroid/net/DatagramPacket",
    "picodroid/net/InetAddress",
    "picodroid/net/NetworkInfo",
    "picodroid/net/Url",
    "picodroid/net/HttpUrlConnection",
    "picodroid/net/HttpInputStream",
    "picodroid/net/HttpOutputStream",
    "picodroid/io/File",
    "picodroid/io/FileInputStream",
    "picodroid/io/FileOutputStream",
    "picodroid/hardware/Sensor",
    "picodroid/hardware/SensorEvent",
    "picodroid/hardware/SensorEventListener",
    "picodroid/hardware/SensorManager",
];

/// Maximum Activity stack depth. Each entry holds a `u16` ObjectRef plus a
/// `&'static str` class name (12 bytes on 32-bit, 16 on 64-bit). 8 covers
/// any realistic embedded UI flow without burning RAM.
pub const MAX_ACTIVITY_STACK: usize = 8;

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
/// handler holds a strong root via [`PicodroidNativeHandler::pending_ops`]).
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
/// [`PicodroidNativeHandler::pending_ops`] between frames.
#[derive(Copy, Clone, Debug)]
pub enum PendingOp {
    Activity(PendingActivityOp),
    Service(PendingServiceOp),
}

/// Maximum pending ops per frame. A typical Activity onCreate that calls
/// `startService` + `bindService` queues 2 service ops; allowing 8 leaves
/// headroom for chained transitions without burning RAM.
pub const MAX_PENDING_OPS: usize = 8;

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
    /// Active Activity stack — top is at `len - 1`. Empty before the first
    /// `startActivity` and after the last `finish()`.
    activity_stack: [Option<ActivityStackEntry>; MAX_ACTIVITY_STACK],
    activity_stack_len: usize,
    /// Pending Activity / Service ops queued by Java in FIFO order, drained
    /// by [`crate::lifecycle`] between frames. A typical Activity onCreate
    /// that does both `startService` and `bindService` queues two ops;
    /// excess ops past `MAX_PENDING_OPS` are silently dropped (logged).
    pending_ops: [Option<PendingOp>; MAX_PENDING_OPS],
    pending_ops_len: usize,
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
            activity_stack: [None; MAX_ACTIVITY_STACK],
            activity_stack_len: 0,
            pending_ops: [None; MAX_PENDING_OPS],
            pending_ops_len: 0,
        }
    }

    /// Append `op` to the pending queue. Returns `true` on success; `false`
    /// (with a log) if the queue is full — apps shouldn't be queueing more
    /// than [`MAX_PENDING_OPS`] transitions per frame.
    pub fn enqueue_op(&mut self, op: PendingOp) -> bool {
        if self.pending_ops_len >= MAX_PENDING_OPS {
            return false;
        }
        self.pending_ops[self.pending_ops_len] = Some(op);
        self.pending_ops_len += 1;
        true
    }

    /// Pop the oldest pending op (FIFO). Returns `None` when the queue is
    /// empty.
    pub fn take_next_pending_op(&mut self) -> Option<PendingOp> {
        if self.pending_ops_len == 0 {
            return None;
        }
        let op = self.pending_ops[0].take();
        // Shift the remaining entries down — bounded loop, max 7 moves.
        for i in 1..self.pending_ops_len {
            self.pending_ops[i - 1] = self.pending_ops[i].take();
        }
        self.pending_ops_len -= 1;
        op
    }

    /// Top of the activity stack as `(obj_ref, class_name)`, or `None` when
    /// the stack is empty.
    pub fn current_activity(&self) -> Option<(u16, &'static str)> {
        if self.activity_stack_len == 0 {
            return None;
        }
        let entry = self.activity_stack[self.activity_stack_len - 1].as_ref()?;
        Some((entry.obj_ref, entry.class_name))
    }

    /// Push an Activity entry onto the stack. Returns `false` and silently
    /// drops the push if the stack is full — apps can't sensibly recover
    /// from a 9-deep nav stack on an MCU, so we'd rather fail soft than
    /// thread a Result through `dispatch`.
    pub fn push_activity(&mut self, obj_ref: u16, class_name: &'static str) -> bool {
        if self.activity_stack_len >= MAX_ACTIVITY_STACK {
            return false;
        }
        self.activity_stack[self.activity_stack_len] = Some(ActivityStackEntry {
            obj_ref,
            class_name,
            root_handle: 0,
        });
        self.activity_stack_len += 1;
        true
    }

    /// Pop the top activity off the stack. Returns the popped entry, or
    /// `None` if the stack was already empty. The third tuple element is
    /// the saved content-view handle (`0` if none); callers that own
    /// teardown must `g.delete` it to free the view tree.
    pub fn pop_activity(&mut self) -> Option<(u16, &'static str, i32)> {
        if self.activity_stack_len == 0 {
            return None;
        }
        self.activity_stack_len -= 1;
        let entry = self.activity_stack[self.activity_stack_len].take()?;
        Some((entry.obj_ref, entry.class_name, entry.root_handle))
    }

    /// Saved content-view handle of the top entry, or `0` if the stack
    /// is empty / the top Activity has not yet called `setContentView`.
    pub fn current_root_handle(&self) -> i32 {
        if self.activity_stack_len == 0 {
            return 0;
        }
        match self.activity_stack[self.activity_stack_len - 1].as_ref() {
            Some(e) => e.root_handle,
            None => 0,
        }
    }

    /// Set the saved content-view handle on the top entry. No-op when the
    /// stack is empty.
    pub fn set_current_root_handle(&mut self, h: i32) {
        if self.activity_stack_len == 0 {
            return;
        }
        if let Some(e) = self.activity_stack[self.activity_stack_len - 1].as_mut() {
            e.root_handle = h;
        }
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

    fn report_gc(&mut self, time_ns: u64, freed: usize) {
        self.gc_time_ns += time_ns;
        self.gc_count += 1;
        self.gc_freed += freed as u32;
        self.total_gc_time_ns += time_ns;
        self.total_gc_count += 1;
        self.total_gc_freed += freed as u32;
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
        match (class_name, method_name) {
            ("picodroid/util/Log", "i") => Some(
                crate::system::picodroid::util::log::log_i(ctx.args, ctx.strings).map(|_| None),
            ),
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
            // ── Application / Activity navigation ───────────────────────
            // Match any class name: invokevirtual dispatches with the runtime
            // subclass name (e.g. "displaydemo/DisplayDemoApp"), not the declaring
            // class "picodroid/app/Application".
            (_, "startActivity") => {
                // args[0] = this (Application or Activity), args[1] = Intent ObjectRef.
                // The Intent's `targetClassName` (slot 0, a String Reference) names the
                // Activity to launch; the framework allocates and runs <init> when the
                // pending op is processed in lifecycle.rs.
                if let Some(Value::ObjectRef(intent_ref)) = ctx.args.get(1) {
                    if let Some(Value::Reference(name_idx)) = ctx.objects.get_field(*intent_ref, 0)
                    {
                        if let Some(class_name) = ctx.strings.resolve(name_idx) {
                            // SAFETY: targetClassName originates from `Class.getName()`,
                            // whose returned string-table slot is backed by Flash bytes
                            // for the lifetime of the JVM (see jvm/src/heap.rs).
                            let static_name: &'static str =
                                unsafe { core::mem::transmute::<&str, &'static str>(class_name) };
                            self.enqueue_op(PendingOp::Activity(PendingActivityOp::Push {
                                class_name: static_name,
                            }));
                        }
                    }
                }
                Some(Ok(None))
            }
            // `Activity.finish()` — request a pop. The handler doesn't
            // validate that the caller is actually the current top: even if
            // a paused Activity calls finish, a single Pop op still pops
            // exactly the top, and that's the documented Android behavior
            // ("a paused Activity finishing itself" doesn't happen unless
            // the app is misbehaving anyway).
            (_, "finish") => {
                self.enqueue_op(PendingOp::Activity(PendingActivityOp::Pop));
                Some(Ok(None))
            }
            _ => None,
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
