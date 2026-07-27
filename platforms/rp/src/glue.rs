// SPDX-License-Identifier: GPL-3.0-only
//! Platform registration — the one file that binds `picodroid-core`'s seam to
//! this family.
//!
//! Everything shared code needs from a platform arrives through three
//! registrations: the HAL traits, the RTOS trait, and the platform hooks.
//! A new MCU family reimplements this file and nothing else in `picodroid-core`
//! changes — see `docs/designs/shared-core-extraction.md` §5.
//!
//! # Why the HAL impls are not cfg-split
//!
//! [`crate::hal`] already dispatches to `hal/rp/` or `hal/sim/` through its
//! single `#[cfg] mod chip;`, so these impls delegate to `crate::hal::*` and
//! are correct on both. Only the RTOS and hooks need a sim/device split,
//! because those genuinely differ (FreeRTOS vs `std`, real vs absent debug
//! bridge).
//!
//! The cfg predicate for that split is `any(test, feature = "sim")`, matching
//! `hal/mod.rs`. A negated `family-rp` gate would be wrong here: sim builds
//! keep that feature active through the board feature chain, so it does not
//! mean "not the simulator" (docs/parity-audit.md BLD-02).

use picodroid_core::hal::types::{EdgeTrigger, GpioEvent, Pull};
#[cfg(not(any(test, feature = "sim")))]
use picodroid_core::host::{NativeHeapStats, PlatformHooks};

/// This family, as seen through every HAL trait.
pub struct Platform;

impl picodroid_core::hal::HalDisplay for Platform {
    fn init() {
        crate::hal::display::init()
    }
    fn set_window(x0: u16, y0: u16, x1: u16, y1: u16) {
        crate::hal::display::set_window(x0, y0, x1, y1)
    }
    fn write_pixels(data: &[u8]) {
        crate::hal::display::write_pixels(data)
    }
    fn set_backlight(on: bool) {
        crate::hal::display::set_backlight(on)
    }
    fn display_sleep() {
        crate::hal::display::display_sleep()
    }
    fn display_wake() {
        crate::hal::display::display_wake()
    }
    fn update_window() {
        crate::hal::display::update_window()
    }
    fn is_window_open() -> bool {
        crate::hal::display::is_window_open()
    }
}

impl picodroid_core::hal::HalGpio for Platform {
    fn set_direction(pin: u8, direction: i32) {
        crate::hal::gpio::set_direction(pin, direction)
    }
    fn set_value(pin: u8, high: bool) {
        crate::hal::gpio::set_value(pin, high)
    }
    fn set_input(pin: u8, pull: Pull) {
        crate::hal::gpio::set_input(pin, to_family_pull(pull))
    }
    fn read(pin: u8) -> bool {
        crate::hal::gpio::read(pin)
    }
    fn enable_edge_irq(pin: u8, edge: EdgeTrigger) {
        crate::hal::gpio::enable_edge_irq(pin, to_family_edge(edge))
    }
    fn disable_edge_irq(pin: u8) {
        crate::hal::gpio::disable_edge_irq(pin)
    }
    fn init_gpio_irq() {
        crate::hal::gpio::init_gpio_irq()
    }
    fn inject(pin: u8, rising: bool) {
        crate::hal::gpio::inject(pin, rising)
    }
    fn drain_gpio_event() -> Option<GpioEvent> {
        crate::hal::gpio::drain_gpio_event().map(|e| GpioEvent {
            pin: e.pin,
            rising: e.rising,
            t_us: e.t_us,
        })
    }
    fn has_pending_event() -> bool {
        crate::hal::gpio::has_pending_event()
    }
    fn wait_for_button_event() {
        crate::hal::gpio::wait_for_button_event()
    }
}

// The family HAL keeps its own copies of these enums for now; they are
// field-identical to the shared ones, and these two converters are all that
// stands between them. When `hal/sim/` moves into picodroid-core (stage 8)
// and `hal/rp/` adopts the shared types directly, both disappear.
fn to_family_pull(p: Pull) -> crate::hal::gpio::Pull {
    match p {
        Pull::None => crate::hal::gpio::Pull::None,
        Pull::Up => crate::hal::gpio::Pull::Up,
        Pull::Down => crate::hal::gpio::Pull::Down,
    }
}

fn to_family_edge(e: EdgeTrigger) -> crate::hal::gpio::EdgeTrigger {
    match e {
        EdgeTrigger::Rising => crate::hal::gpio::EdgeTrigger::Rising,
        EdgeTrigger::Falling => crate::hal::gpio::EdgeTrigger::Falling,
        EdgeTrigger::Both => crate::hal::gpio::EdgeTrigger::Both,
    }
}

impl picodroid_core::hal::HalClock for Platform {
    fn sleep(ms: u32) {
        crate::hal::system_clock::sleep(ms)
    }
    fn elapsed_realtime_nanos() -> i64 {
        crate::hal::system_clock::elapsed_realtime_nanos()
    }
}

impl picodroid_core::hal::HalTouch for Platform {
    fn init() {
        crate::hal::touch::init()
    }
    fn read_point() -> Option<(u16, u16)> {
        crate::hal::touch::read_point()
    }
    fn read_raw_unfiltered() -> (u16, u16) {
        crate::hal::touch::read_raw_unfiltered()
    }
    fn set_calibration(cal_x_min: u16, cal_x_max: u16, cal_y_min: u16, cal_y_max: u16) {
        crate::hal::touch::set_calibration(cal_x_min, cal_x_max, cal_y_min, cal_y_max)
    }
    fn inject_override(x: u16, y: u16) {
        crate::hal::touch::inject_override(x, y)
    }
    fn release_override() {
        crate::hal::touch::release_override()
    }
    fn clear_override() {
        crate::hal::touch::clear_override()
    }
}

impl picodroid_core::hal::HalI2c for Platform {
    fn init(i2c_id: u8) {
        crate::hal::i2c::init(i2c_id)
    }
    fn set_speed(i2c_id: u8, hz: u32) {
        crate::hal::i2c::set_speed(i2c_id, hz)
    }
    fn write_slice(i2c_id: u8, address: u8, data: &[u8]) -> i32 {
        crate::hal::i2c::write_slice(i2c_id, address, data)
    }
    fn read_slice(i2c_id: u8, address: u8, buf: &mut [u8]) -> i32 {
        crate::hal::i2c::read_slice(i2c_id, address, buf)
    }
    fn write(
        i2c_id: u8,
        address: u32,
        data_idx: u16,
        len: usize,
        arrays: &pico_jvm::array_heap::ArrayHeap,
    ) -> i32 {
        crate::hal::i2c::write(i2c_id, address, data_idx, len, arrays)
    }
    fn read(
        i2c_id: u8,
        address: u32,
        buf_idx: u16,
        len: usize,
        arrays: &mut pico_jvm::array_heap::ArrayHeap,
    ) -> i32 {
        crate::hal::i2c::read(i2c_id, address, buf_idx, len, arrays)
    }
}

impl picodroid_core::hal::HalAdc for Platform {
    fn init(pin: u8) {
        crate::hal::adc::init(pin)
    }
    fn read(pin: u8) -> f64 {
        crate::hal::adc::read(pin)
    }
}

impl picodroid_core::hal::HalPwm for Platform {
    fn init(pin: u8) {
        crate::hal::pwm::init(pin)
    }
    fn apply(pin: u8, freq_hz: f64, duty_cycle: f64, enabled: bool) {
        crate::hal::pwm::apply(pin, freq_hz, duty_cycle, enabled)
    }
}

impl picodroid_core::hal::HalSpi for Platform {
    fn init(spi_id: u8) {
        crate::hal::spi::init(spi_id)
    }
    fn reconfigure(spi_id: u8, freq_hz: u32, mode: u32) {
        crate::hal::spi::reconfigure(spi_id, freq_hz, mode)
    }
    fn write_raw(spi_id: u8, data: &[u8]) {
        crate::hal::spi::write_raw(spi_id, data)
    }
    fn transfer_raw(spi_id: u8, tx: &[u8], rx: &mut [u8]) {
        crate::hal::spi::transfer_raw(spi_id, tx, rx)
    }
    fn transfer(
        spi_id: u8,
        tx_idx: u16,
        rx_idx: u16,
        len: usize,
        arrays: &mut pico_jvm::array_heap::ArrayHeap,
    ) -> i32 {
        crate::hal::spi::transfer(spi_id, tx_idx, rx_idx, len, arrays)
    }
    fn write(
        spi_id: u8,
        data_idx: u16,
        len: usize,
        arrays: &pico_jvm::array_heap::ArrayHeap,
    ) -> i32 {
        crate::hal::spi::write(spi_id, data_idx, len, arrays)
    }
}

impl picodroid_core::hal::HalUart for Platform {
    fn init(uart_id: u8) {
        crate::hal::uart::init(uart_id)
    }
    fn write_byte(uart_id: u8, byte: u8) {
        crate::hal::uart::write_byte(uart_id, byte)
    }
    fn read_byte(uart_id: u8) -> i32 {
        crate::hal::uart::read_byte(uart_id)
    }
    fn reconfigure(
        uart_id: u8,
        baudrate: i32,
        data_size: i32,
        parity: i32,
        stop_bits: i32,
        hw_flow: i32,
    ) {
        crate::hal::uart::reconfigure(uart_id, baudrate, data_size, parity, stop_bits, hw_flow)
    }
}

/// LittleFS, on flash for device builds and a host-file image for the
/// simulator — `crate::fs` already routes between the two, so this arm is
/// shared. Each call re-resolves the path inside `with_fs`, whose closure
/// borrows the filesystem for exactly the operation's duration; that is why
/// [`HalFs`](picodroid_core::hal::HalFs) hands out no file handles.
///
/// `with_fs` returns `None` when the filesystem is unavailable, which folds
/// into the same "failed" value the Java API reports.
#[cfg(not(test))]
impl picodroid_core::hal::HalFs for Platform {
    fn exists(path: &str) -> bool {
        crate::fs::with_fs(|fs| fs.exists(path)).unwrap_or(false)
    }

    fn is_file(path: &str) -> bool {
        crate::fs::with_fs(|fs| {
            matches!(
                fs.stat(path).map(|m| m.file_type),
                Ok(littlefs_rust::FileType::File)
            )
        })
        .unwrap_or(false)
    }

    fn is_dir(path: &str) -> bool {
        crate::fs::with_fs(|fs| {
            matches!(
                fs.stat(path).map(|m| m.file_type),
                Ok(littlefs_rust::FileType::Dir)
            )
        })
        .unwrap_or(false)
    }

    fn length(path: &str) -> i64 {
        crate::fs::with_fs(|fs| fs.stat(path).map(|m| m.size as i64).unwrap_or(0)).unwrap_or(0)
    }

    fn delete(path: &str) -> bool {
        crate::fs::with_fs(|fs| fs.remove(path).is_ok()).unwrap_or(false)
    }

    fn mkdir(path: &str) -> bool {
        crate::fs::with_fs(|fs| fs.mkdir(path).is_ok()).unwrap_or(false)
    }

    fn rename(from: &str, to: &str) -> bool {
        crate::fs::with_fs(|fs| fs.rename(from, to).is_ok()).unwrap_or(false)
    }

    fn truncate(path: &str) {
        let _ = crate::fs::with_fs(|fs| fs.write_file(path, &[]));
    }

    fn read_at(path: &str, pos: u64, out: &mut alloc::vec::Vec<u8>, len: usize) -> i32 {
        crate::fs::with_fs(|fs| {
            let file = match fs.open(path, littlefs_rust::OpenFlags::READ) {
                Ok(f) => f,
                Err(_) => return -1i32,
            };
            if file
                .seek(littlefs_rust::SeekFrom::Start(pos as u32))
                .is_err()
            {
                return -1;
            }
            let mut tmp = alloc::vec![0u8; len];
            match file.read(&mut tmp) {
                Ok(n) => {
                    out.extend_from_slice(&tmp[..n as usize]);
                    n as i32
                }
                Err(_) => -1,
            }
        })
        .unwrap_or(-1)
    }

    fn write_at(path: &str, pos: u64, data: &[u8]) -> i32 {
        crate::fs::with_fs(|fs| {
            let file = match fs.open(
                path,
                littlefs_rust::OpenFlags::WRITE | littlefs_rust::OpenFlags::CREATE,
            ) {
                Ok(f) => f,
                Err(_) => return -1i32,
            };
            if file
                .seek(littlefs_rust::SeekFrom::Start(pos as u32))
                .is_err()
            {
                return -1;
            }
            match file.write(data) {
                Ok(n) => {
                    let _ = file.sync();
                    n as i32
                }
                Err(_) => -1,
            }
        })
        .unwrap_or(-1)
    }
}

picodroid_core::set_hal! {
    display = Platform,
    gpio    = Platform,
    clock   = Platform,
    touch   = Platform,
    i2c     = Platform,
    adc     = Platform,
    pwm     = Platform,
    spi     = Platform,
    uart    = Platform,
}

// Registered separately from the umbrella above: `crate::fs` is
// `cfg(not(test))`, so the host-test build of this crate genuinely has no
// filesystem to bind. Same shape as the network arm below.
#[cfg(not(test))]
picodroid_core::set_hal_fs!(Platform);

#[cfg(has_network)]
mod net_glue {
    use super::Platform;
    use core::ffi::c_void;
    use picodroid_core::hal::types::NetError;

    /// The family HAL has its own `NetError`; both wrap the same errno-style
    /// code, so this is a field lift, not a translation.
    fn lift<T>(r: Result<T, crate::hal::net::NetError>) -> Result<T, NetError> {
        r.map_err(|e| NetError(e.0))
    }

    impl picodroid_core::hal::HalNet for Platform {
        fn tcp_socket() -> Result<*mut c_void, NetError> {
            lift(crate::hal::net::tcp_socket())
        }
        fn tcp_connect(sock: *mut c_void, addr: u32, port: u16) -> Result<(), NetError> {
            lift(crate::hal::net::tcp_connect(sock, addr, port))
        }
        fn tcp_send(sock: *mut c_void, data: &[u8]) -> Result<usize, NetError> {
            lift(crate::hal::net::tcp_send(sock, data))
        }
        fn tcp_recv(sock: *mut c_void, buf: &mut [u8]) -> Result<usize, NetError> {
            lift(crate::hal::net::tcp_recv(sock, buf))
        }
        fn tcp_listen(sock: *mut c_void, port: u16) -> Result<(), NetError> {
            lift(crate::hal::net::tcp_listen(sock, port))
        }
        fn tcp_accept(sock: *mut c_void) -> Result<*mut c_void, NetError> {
            lift(crate::hal::net::tcp_accept(sock))
        }
        fn udp_socket(local_port: u16) -> Result<*mut c_void, NetError> {
            lift(crate::hal::net::udp_socket(local_port))
        }
        fn udp_sendto(
            sock: *mut c_void,
            buf: &[u8],
            addr: u32,
            port: u16,
        ) -> Result<usize, NetError> {
            lift(crate::hal::net::udp_sendto(sock, buf, addr, port))
        }
        fn udp_recvfrom(sock: *mut c_void, buf: &mut [u8]) -> Result<(usize, u32, u16), NetError> {
            lift(crate::hal::net::udp_recvfrom(sock, buf))
        }
        fn close(sock: *mut c_void) {
            crate::hal::net::close(sock)
        }
        fn set_recv_timeout(sock: *mut c_void, ms: u32) {
            crate::hal::net::set_recv_timeout(sock, ms)
        }
        fn is_network_up() -> bool {
            crate::hal::net::is_network_up()
        }
        fn get_ip_address() -> u32 {
            crate::hal::net::get_ip_address()
        }
        fn dns_resolve(hostname: &str) -> Result<u32, NetError> {
            lift(crate::hal::net::dns_resolve(hostname))
        }
    }

    picodroid_core::set_hal_net!(Platform);
}

// ── RTOS ─────────────────────────────────────────────────────────────────────
//
// Device only. The simulator's RTOS is picodroid-core's, registered by the
// `register_sim_platform!` invocation at the bottom of this file — host
// threads, condvars and recursive mutexes are simulator policy, not family
// policy, and every family reimplementing them is how they drift.

/// This family's RTOS: FreeRTOS.
#[cfg(not(any(test, feature = "sim")))]
pub struct PlatformRtos;

#[cfg(not(any(test, feature = "sim")))]
mod rtos_impl {
    //! FreeRTOS backing.
    //!
    //! Stack sizing and core affinity live here rather than crossing the
    //! seam: they are policy this family owns, derived from `boot_budget`.
    //! FreeRTOS counts stacks in words, the seam speaks bytes, and the
    //! conversion happens at exactly this boundary.

    use super::PlatformRtos;
    use alloc::boxed::Box;
    // `MutexInnerImpl` is the trait carrying create/take/give for
    // `MutexRecursive` — the same import monitor_store.rs needs.
    use freertos_rust::{
        Duration, MutexInnerImpl, MutexRecursive, Queue, Semaphore, Task, TaskPriority, Timer,
    };
    use picodroid_core::rtos::{RawMutex, RawQueue, RawSem, Rtos, TaskKind, TaskSpec, Timeout};

    fn to_duration(t: Timeout) -> Duration {
        match t {
            Timeout::None => Duration::zero(),
            Timeout::Ms(ms) => Duration::ms(ms),
            Timeout::Forever => Duration::infinite(),
        }
    }

    struct TimerCell(core::cell::UnsafeCell<Option<Timer>>);
    // SAFETY: mutated only by the UI thread before the timer is in active
    // use; afterwards `Timer`'s own operations go through the FreeRTOS timer
    // command queue and are themselves thread-safe.
    unsafe impl Sync for TimerCell {}

    static TICK_TIMER: TimerCell = TimerCell(core::cell::UnsafeCell::new(None));

    /// Default stack for each task kind, in **bytes**. Sourced from the boot
    /// budget so the sim's pre-charge and the device's real allocation stay
    /// in step (docs/parity-audit.md M4).
    ///
    /// The seam speaks bytes and FreeRTOS counts words; the ×4 here and the
    /// ÷4 in `spawn` are the entirety of that conversion, kept in one file so
    /// a family whose RTOS counts bytes natively simply drops it.
    fn default_stack_bytes(kind: TaskKind) -> u32 {
        match kind {
            TaskKind::JvmChild => crate::boot_budget::JVM_THREAD_STACK_WORDS as u32 * 4,
            TaskKind::BgWorker => picodroid_core::board_cfg::background_pool::POOL_STACK_BYTES,
            TaskKind::Sensor => crate::boot_budget::SENSOR_STACK_WORDS as u32 * 4,
        }
    }

    unsafe impl Rtos for PlatformRtos {
        fn spawn(spec: &TaskSpec, body: Box<dyn FnOnce() + Send>) -> bool {
            let stack_bytes = spec
                .stack_bytes
                .unwrap_or_else(|| default_stack_bytes(spec.kind));
            let words = (stack_bytes / 4) as u16;
            let prio = TaskPriority(spec.priority);

            if spec.kind != TaskKind::JvmChild {
                return Task::new()
                    .name(spec.name)
                    .stack_size(words)
                    .priority(prio)
                    .start(move |_| body())
                    .is_ok();
            }

            // JVM child tasks carry two extra obligations, both of which
            // belong to this family rather than to shared code:
            //
            //  * Pin to core 0. The interpreter's heap safety rests on the
            //    single-core assumption documented in app.rs.
            //  * Register with the debug bridge, and deregister on exit, so
            //    a stop request can reach the child and jvm_task's wait loop
            //    unblocks. The spawn trampoline calls vTaskDelete(NULL)
            //    after the body returns, reclaiming stack and TCB.
            //
            // Written as a whole chain because `TaskBuilder`'s setters
            // borrow the temporary and cannot be split across a `let`.
            let spawned = Task::new()
                .name(spec.name)
                .stack_size(words)
                .priority(prio)
                .core_affinity(0b01)
                .start(move |_| {
                    body();
                    if let Ok(t) = Task::current() {
                        crate::pdb::pending::deregister_child_task(t.raw_handle());
                    }
                });
            match spawned {
                Ok(child) => {
                    crate::pdb::pending::register_child_task(child);
                    true
                }
                Err(_) => false,
            }
        }

        fn queue_create(depth: usize) -> RawQueue {
            match Queue::<u32>::new(depth) {
                Ok(q) => Box::into_raw(Box::new(q)) as RawQueue,
                Err(_) => 0,
            }
        }

        fn queue_send(q: RawQueue, word: u32, t: Timeout) -> bool {
            if q == 0 {
                return false;
            }
            let queue = unsafe { &*(q as *const Queue<u32>) };
            queue.send(word, to_duration(t)).is_ok()
        }

        fn queue_recv(q: RawQueue, t: Timeout) -> Option<u32> {
            if q == 0 {
                return None;
            }
            let queue = unsafe { &*(q as *const Queue<u32>) };
            queue.receive(to_duration(t)).ok()
        }

        fn mutex_recursive_create() -> Option<RawMutex> {
            MutexRecursive::create()
                .ok()
                .map(|m| Box::into_raw(Box::new(m)) as RawMutex)
        }

        fn mutex_recursive_lock(m: RawMutex, t: Timeout) -> bool {
            if m == 0 {
                return false;
            }
            // `take`/`give` rather than a scoped guard: the seam's lock and
            // unlock are separate calls because Java `monitorenter` and
            // `monitorexit` are separate bytecodes, so no Rust scope spans
            // the critical region. This is the same shape monitor_store.rs
            // already used.
            let mutex = unsafe { &*(m as *const MutexRecursive) };
            mutex.take(to_duration(t)).is_ok()
        }

        fn mutex_recursive_unlock(m: RawMutex) {
            if m == 0 {
                return;
            }
            let mutex = unsafe { &*(m as *const MutexRecursive) };
            mutex.give();
        }

        fn sem_binary_create() -> RawSem {
            match Semaphore::new_binary() {
                Ok(s) => Box::into_raw(Box::new(s)) as RawSem,
                Err(_) => 0,
            }
        }

        fn sem_give(s: RawSem) {
            if s == 0 {
                return;
            }
            let sem = unsafe { &*(s as *const Semaphore) };
            sem.give();
        }

        fn sem_take(s: RawSem, t: Timeout) -> bool {
            if s == 0 {
                return false;
            }
            let sem = unsafe { &*(s as *const Semaphore) };
            sem.take(to_duration(t)).is_ok()
        }

        fn tick_timer_start(period_ms: u32, cb: fn()) {
            // SAFETY: callers serialise on the UI thread (see run_activity).
            unsafe {
                if let Some(t) = (*TICK_TIMER.0.get()).as_ref() {
                    let _ = t.start(Duration::ms(0));
                    return;
                }
                let timer = Timer::new(Duration::ms(period_ms))
                    .set_name("lvgl-tick")
                    .set_auto_reload(true)
                    .create(move |_| cb())
                    .expect("lvgl-tick timer alloc");
                timer.start(Duration::ms(0)).expect("lvgl-tick start");
                *TICK_TIMER.0.get() = Some(timer);
            }
        }

        /// Genuinely stop the timer rather than filtering at the callback:
        /// the activity loop pauses the tick before display sleep so the
        /// chip can reach a deeper idle state, which only happens if nothing
        /// is still firing.
        fn tick_timer_pause() {
            // SAFETY: see `tick_timer_start`.
            unsafe {
                if let Some(t) = (*TICK_TIMER.0.get()).as_ref() {
                    let _ = t.stop(Duration::ms(0));
                }
            }
        }

        fn tick_timer_resume() {
            // SAFETY: see `tick_timer_start`.
            unsafe {
                if let Some(t) = (*TICK_TIMER.0.get()).as_ref() {
                    let _ = t.start(Duration::ms(0));
                }
            }
        }

        /// Stop the timer but keep the allocation: dropping a `Timer` blocks
        /// the caller for up to 1 s on the timer command queue
        /// (`freertos_rust::Timer::drop`). The tick is a process-wide
        /// singleton, so holding the handle forever is the cheaper trade.
        fn tick_timer_stop() {
            Self::tick_timer_pause();
        }

        fn delay_ms(ms: u32) {
            freertos_rust::CurrentTask::delay(Duration::ms(ms));
        }
    }
}

#[cfg(not(any(test, feature = "sim")))]
picodroid_core::set_rtos!(PlatformRtos);

// ── Platform hooks ───────────────────────────────────────────────────────────
//
// Device only, like the RTOS above. The simulator's hooks come from
// `register_sim_platform!` below: its stop request is always false (no debug
// bridge), and its heap controls are the simulator allocator's own.

#[cfg(not(any(test, feature = "sim")))]
pub struct PlatformHost;

#[cfg(not(any(test, feature = "sim")))]
impl PlatformHooks for PlatformHost {
    fn stop_requested() -> bool {
        crate::pdb::pending::is_stop_jvm()
    }

    /// No simulated heap on hardware — every allocation is the real one.
    fn heap_bypass_enter() {}
    fn heap_bypass_exit() {}
    fn heap_checkpoint(_label: &str) {}

    /// This family owns no root providers today — every one lives in
    /// picodroid-core. The call is kept rather than the list deleted so that
    /// adding an RP-specific native module holding Java references is a
    /// one-line change here, guarded by the completeness scan in
    /// `gc_root_registration`.
    fn register_gc_roots() {
        crate::gc_root_registration::register_all();
    }

    /// Moved here verbatim from `mem_diag::sample_native_heap`, which used to
    /// carry the sim/device split itself. This half is FreeRTOS FFI; the
    /// simulator's reads its own allocator, and lives in picodroid-core.
    fn native_heap_stats() -> NativeHeapStats {
        /// Mirror of FreeRTOS `HeapStats_t` (heap_4.c). Layout is fixed by
        /// FreeRTOS; `size_t` = u32 on ARM32. The simulator's heap_4 port
        /// mirrors the same numbers host-side.
        #[repr(C)]
        struct FreeRtosHeapStats {
            available_heap_space_in_bytes: u32,
            size_of_largest_free_block_in_bytes: u32,
            size_of_smallest_free_block_in_bytes: u32,
            number_of_free_blocks: u32,
            minimum_ever_free_bytes_remaining: u32,
            number_of_successful_allocations: u32,
            number_of_successful_frees: u32,
        }
        extern "C" {
            fn xPortGetFreeHeapSize() -> u32;
            fn xPortGetMinimumEverFreeHeapSize() -> u32;
            fn vPortGetHeapStats(stats: *mut FreeRtosHeapStats);
        }
        // SAFETY: plain FreeRTOS accessors; vPortGetHeapStats fills the
        // struct it is handed (layout mirrored above).
        unsafe {
            let free = xPortGetFreeHeapSize();
            let mut stats = core::mem::MaybeUninit::<FreeRtosHeapStats>::uninit();
            vPortGetHeapStats(stats.as_mut_ptr());
            let stats = stats.assume_init();
            NativeHeapStats {
                used_bytes: picodroid_core::board_cfg::heap::DEVICE_HEAP_BYTES
                    .saturating_sub(free as usize),
                free_bytes: free as usize,
                min_ever_free_bytes: xPortGetMinimumEverFreeHeapSize() as usize,
                largest_free_block: stats.size_of_largest_free_block_in_bytes as usize,
            }
        }
    }
}

#[cfg(not(any(test, feature = "sim")))]
picodroid_core::set_platform_hooks!(PlatformHost);

// ── Simulator ────────────────────────────────────────────────────────────────

/// Bill the boot budget for a `Thread.start` the simulator refuses to run.
///
/// A wrapper rather than passing `boot_budget::charge_thread_spawn` directly:
/// that function is `cfg(feature = "sim")`, since a plain host test build has
/// no arena to charge, and the cfg belongs next to the budget it guards.
#[cfg(any(test, feature = "sim"))]
fn charge_jvm_child_spawn() {
    #[cfg(feature = "sim")]
    crate::boot_budget::charge_thread_spawn();
}

#[cfg(any(test, feature = "sim"))]
picodroid_core::register_sim_platform! {
    gc_roots = crate::gc_root_registration::register_all,
    charge_jvm_child_spawn = charge_jvm_child_spawn,
}
