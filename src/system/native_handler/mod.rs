#[cfg(not(feature = "sim"))]
use pico_jvm::types::MonitorKey;
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext, NativeMethodHandler,
};

mod graphics;
#[cfg(feature = "has-network")]
mod net;
mod os;
mod pio;

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
    pending_activity: Option<(u16, &'static str)>,
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
            pending_activity: None,
        }
    }

    pub fn take_pending_activity(&mut self) -> Option<(u16, &'static str)> {
        self.pending_activity.take()
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
        // Delegate to domain-specific sub-handlers.
        if let result @ Some(_) = pio::dispatch(class_name, method_name, ctx) {
            return result;
        }
        if let result @ Some(_) = os::dispatch(class_name, method_name, ctx) {
            return result;
        }
        if let result @ Some(_) = graphics::dispatch(class_name, method_name, ctx) {
            return result;
        }
        #[cfg(feature = "has-network")]
        if let result @ Some(_) = net::dispatch(class_name, method_name, ctx) {
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
            // ── Application ──────────────────────────────────────────────
            // Match any class name: invokevirtual dispatches with the runtime
            // subclass name (e.g. "displaydemo/DisplayDemoApp"), not the declaring
            // class "picodroid/app/Application".
            (_, "startActivity") => {
                // args[0] = this (Application), args[1] = activity ObjectRef
                if let Some(Value::ObjectRef(activity_ref)) = ctx.args.get(1) {
                    let class_name = ctx.objects.class_name(*activity_ref).unwrap_or("unknown");
                    // SAFETY: class names from ObjectHeap are Flash-backed &'static str.
                    let static_name: &'static str =
                        unsafe { core::mem::transmute::<&str, &'static str>(class_name) };
                    self.pending_activity = Some((*activity_ref, static_name));
                }
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
