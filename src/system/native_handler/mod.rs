#[cfg(not(feature = "sim"))]
use pico_jvm::types::MonitorKey;
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext, NativeMethodHandler,
};

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
    "picodroid/pio/Gpio",
    "picodroid/pio/PeripheralManager",
    "picodroid/os/SystemClock",
    "picodroid/util/Log",
    "picodroid/concurrent/Executor",
    "picodroid/concurrent/Executors",
    "picodroid/concurrent/MainExecutor",
    "picodroid/concurrent/BackgroundExecutor",
    "picodroid/app/Application",
    "picodroid/view/View",
    "picodroid/view/MotionEvent",
    "picodroid/view/KeyEvent",
    "picodroid/view/OnKeyListener",
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
    "picodroid/widget/EditText",
    "picodroid/widget/Toast",
    "picodroid/widget/AlertDialog",
    "picodroid/widget/AlertDialog$Builder",
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
];

/// Maximum Activity stack depth. Each entry holds a `u16` ObjectRef plus a
/// `&'static str` class name (12 bytes on 32-bit, 16 on 64-bit). 8 covers
/// any realistic embedded UI flow without burning RAM.
pub const MAX_ACTIVITY_STACK: usize = 8;

/// Pending lifecycle transition signaled from Java to the framework loop in
/// [`crate::lifecycle::run_activity`]. The loop checks this between frame
/// ticks and invokes the corresponding lifecycle callbacks before clearing it.
#[derive(Copy, Clone, Debug)]
pub enum PendingActivityOp {
    /// `Application.startActivity(activity)` or
    /// `Activity.startActivity(activity)` — push a new Activity on top of
    /// the stack. The current top, if any, is paused first.
    Push {
        obj_ref: u16,
        class_name: &'static str,
    },
    /// `Activity.finish()` — pop the current top off the stack. If the
    /// stack is left empty, [`run_activity`] returns and the app exits.
    Pop,
}

#[derive(Copy, Clone)]
struct ActivityStackEntry {
    obj_ref: u16,
    class_name: &'static str,
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
    /// At most one pending push/pop, drained by the framework loop between
    /// frames. Multiple `startActivity()` calls in a single frame would
    /// silently drop all but the last; that matches Android's
    /// "one transition per frame" semantics.
    pending_op: Option<PendingActivityOp>,
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
            pending_op: None,
        }
    }

    pub fn take_pending_op(&mut self) -> Option<PendingActivityOp> {
        self.pending_op.take()
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
        });
        self.activity_stack_len += 1;
        true
    }

    /// Pop the top activity off the stack. Returns the popped entry, or
    /// `None` if the stack was already empty.
    pub fn pop_activity(&mut self) -> Option<(u16, &'static str)> {
        if self.activity_stack_len == 0 {
            return None;
        }
        self.activity_stack_len -= 1;
        let entry = self.activity_stack[self.activity_stack_len].take()?;
        Some((entry.obj_ref, entry.class_name))
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
                // args[0] = this (Application or Activity), args[1] = activity ObjectRef
                if let Some(Value::ObjectRef(activity_ref)) = ctx.args.get(1) {
                    let class_name = ctx.objects.class_name(*activity_ref).unwrap_or("unknown");
                    // SAFETY: class names from ObjectHeap are Flash-backed &'static str.
                    let static_name: &'static str =
                        unsafe { core::mem::transmute::<&str, &'static str>(class_name) };
                    self.pending_op = Some(PendingActivityOp::Push {
                        obj_ref: *activity_ref,
                        class_name: static_name,
                    });
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
                self.pending_op = Some(PendingActivityOp::Pop);
                Some(Ok(None))
            }
            _ => None,
        }
    }

    #[cfg(not(any(test, feature = "sim")))]
    fn interrupted(&self) -> bool {
        crate::pdb::pending::is_stop_jvm()
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
