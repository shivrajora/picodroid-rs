// SPDX-License-Identifier: GPL-3.0-only
//! A headless platform registration for this crate's own test binary.
//!
//! Every `extern "Rust"` symbol the facades declare must be defined by
//! *someone* at link time. In a firmware or simulator build that is the
//! platform crate; under `cargo test -p picodroid-core` there is no platform
//! crate, so this module stands in. Without it the crate would not be
//! independently testable — and independent testability is most of the point
//! of extracting it.
//!
//! It also earns its keep as a contract check. Declaration and definition are
//! matched by the linker on name alone, so a facade `extern` whose signature
//! drifts from its `set_*!`-generated shim would be undefined behaviour in a
//! firmware build and catch nothing at compile time. Registering here puts
//! both in one crate, where `clashing_extern_declarations` fires — so the
//! drift becomes a build error in the fastest test lane we have.
//!
//! Behaviour is deliberately inert: no window, no threads, no clock. Tests
//! that need real behaviour should exercise the logic directly rather than
//! through the seam.

use crate::hal::sim::rtos as sim_rtos;
use crate::hal::types::{EdgeTrigger, GpioEvent, Pull};
use crate::host::{NativeHeapStats, PlatformHooks};
use crate::rtos::{RawMutex, RawQueue, RawSem, RawTask, Rtos, TaskSpec, Timeout};

pub struct TestHal;

impl crate::hal::HalDisplay for TestHal {
    fn init() {}
    fn set_window(_x0: u16, _y0: u16, _x1: u16, _y1: u16) {}
    fn write_pixels(_data: &[u8]) {}
    fn set_backlight(_on: bool) {}
    fn display_sleep() {}
    fn display_wake() {}
    fn update_window() {}
    fn is_window_open() -> bool {
        true
    }
}

impl crate::hal::HalGpio for TestHal {
    fn set_direction(_pin: u8, _direction: i32) {}
    fn set_value(_pin: u8, _high: bool) {}
    fn set_input(_pin: u8, _pull: Pull) {}
    fn read(_pin: u8) -> bool {
        false
    }
    fn enable_edge_irq(_pin: u8, _edge: EdgeTrigger) {}
    fn disable_edge_irq(_pin: u8) {}
    fn init_gpio_irq() {}
    fn inject(_pin: u8, _rising: bool) {}
    fn drain_gpio_event() -> Option<GpioEvent> {
        None
    }
    fn has_pending_event() -> bool {
        false
    }
    /// Returning immediately keeps a stray call from hanging the test binary.
    fn wait_for_button_event() {}
}

impl crate::hal::HalClock for TestHal {
    fn sleep(_ms: u32) {}
    fn elapsed_realtime_nanos() -> i64 {
        0
    }
}

impl crate::hal::HalTouch for TestHal {
    fn init() {}
    fn read_point() -> Option<(u16, u16)> {
        None
    }
    fn read_raw_unfiltered() -> (u16, u16) {
        (0, 0)
    }
    fn set_calibration(_x_min: u16, _x_max: u16, _y_min: u16, _y_max: u16) {}
    fn inject_override(_x: u16, _y: u16) {}
    fn release_override() {}
    fn clear_override() {}
}

impl crate::hal::HalI2c for TestHal {
    fn init(_i2c_id: u8) {}
    fn set_speed(_i2c_id: u8, _hz: u32) {}
    fn write_slice(_i2c_id: u8, _address: u8, _data: &[u8]) -> i32 {
        -1
    }
    fn read_slice(_i2c_id: u8, _address: u8, _buf: &mut [u8]) -> i32 {
        -1
    }
    fn write(
        _i2c_id: u8,
        _address: u32,
        _data_idx: u16,
        _len: usize,
        _arrays: &pico_jvm::array_heap::ArrayHeap,
    ) -> i32 {
        -1
    }
    fn read(
        _i2c_id: u8,
        _address: u32,
        _buf_idx: u16,
        _len: usize,
        _arrays: &mut pico_jvm::array_heap::ArrayHeap,
    ) -> i32 {
        -1
    }
}

impl crate::hal::HalAdc for TestHal {
    fn init(_pin: u8) {}
    fn read(_pin: u8) -> f64 {
        0.0
    }
}

impl crate::hal::HalPwm for TestHal {
    fn init(_pin: u8) {}
    fn apply(_pin: u8, _freq_hz: f64, _duty_cycle: f64, _enabled: bool) {}
}

impl crate::hal::HalSpi for TestHal {
    fn init(_spi_id: u8) {}
    fn reconfigure(_spi_id: u8, _freq_hz: u32, _mode: u32) {}
    fn write_raw(_spi_id: u8, _data: &[u8]) {}
    fn transfer_raw(_spi_id: u8, _tx: &[u8], _rx: &mut [u8]) {}
    fn transfer(
        _spi_id: u8,
        _tx_idx: u16,
        _rx_idx: u16,
        _len: usize,
        _arrays: &mut pico_jvm::array_heap::ArrayHeap,
    ) -> i32 {
        -1
    }
    fn write(
        _spi_id: u8,
        _data_idx: u16,
        _len: usize,
        _arrays: &pico_jvm::array_heap::ArrayHeap,
    ) -> i32 {
        -1
    }
}

impl crate::hal::HalUart for TestHal {
    fn init(_uart_id: u8) {}
    fn write_byte(_uart_id: u8, _byte: u8) {}
    fn read_byte(_uart_id: u8) -> i32 {
        -1
    }
    fn reconfigure(
        _uart_id: u8,
        _baudrate: i32,
        _data_size: i32,
        _parity: i32,
        _stop_bits: i32,
        _hw_flow: i32,
    ) {
    }
}

/// In-memory filesystem, so `File` natives are testable without LittleFS or
/// a host image. This is the backend `native_handler/io.rs` carries in its
/// own `cfg(test)` module today; it lands here so that when io.rs moves into
/// this crate its tests keep a real backend to run against.
impl crate::hal::HalFs for TestHal {
    fn exists(path: &str) -> bool {
        store().lock().expect("test fs poisoned").contains_key(path)
    }
    fn is_file(path: &str) -> bool {
        <Self as crate::hal::HalFs>::exists(path)
    }
    fn is_dir(_path: &str) -> bool {
        false
    }
    fn length(path: &str) -> i64 {
        store()
            .lock()
            .expect("test fs poisoned")
            .get(path)
            .map(|v| v.len() as i64)
            .unwrap_or(0)
    }
    fn delete(path: &str) -> bool {
        store()
            .lock()
            .expect("test fs poisoned")
            .remove(path)
            .is_some()
    }
    fn mkdir(_path: &str) -> bool {
        true
    }
    fn rename(from: &str, to: &str) -> bool {
        let mut s = store().lock().expect("test fs poisoned");
        if let Some(data) = s.remove(from) {
            s.insert(to.to_string(), data);
            true
        } else {
            false
        }
    }
    fn truncate(path: &str) {
        store()
            .lock()
            .expect("test fs poisoned")
            .insert(path.to_string(), Vec::new());
    }
    fn read_at(path: &str, pos: u64, out: &mut Vec<u8>, len: usize) -> i32 {
        let s = store().lock().expect("test fs poisoned");
        let Some(v) = s.get(path) else {
            return -1;
        };
        let start = pos as usize;
        if start >= v.len() {
            return 0;
        }
        let end = (start + len).min(v.len());
        out.extend_from_slice(&v[start..end]);
        (end - start) as i32
    }
    fn write_at(path: &str, pos: u64, data: &[u8]) -> i32 {
        let mut s = store().lock().expect("test fs poisoned");
        let entry = s.entry(path.to_string()).or_default();
        let start = pos as usize;
        if entry.len() < start + data.len() {
            entry.resize(start + data.len(), 0);
        }
        entry[start..start + data.len()].copy_from_slice(data);
        data.len() as i32
    }
}

fn store() -> &'static std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>> {
    static STORE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>,
    > = std::sync::OnceLock::new();
    STORE.get_or_init(|| std::sync::Mutex::new(std::collections::BTreeMap::new()))
}

crate::set_hal! {
    display = TestHal,
    gpio    = TestHal,
    clock   = TestHal,
    touch   = TestHal,
    i2c     = TestHal,
    adc     = TestHal,
    pwm     = TestHal,
    spi     = TestHal,
    uart    = TestHal,
}

crate::set_hal_fs!(TestHal);

#[cfg(has_network)]
mod net_stub {
    use super::TestHal;
    use crate::hal::types::NetError;
    use core::ffi::c_void;

    const DOWN: NetError = NetError(-1);

    impl crate::hal::HalNet for TestHal {
        fn tcp_socket() -> Result<*mut c_void, NetError> {
            Err(DOWN)
        }
        fn tcp_connect(_s: *mut c_void, _addr: u32, _port: u16) -> Result<(), NetError> {
            Err(DOWN)
        }
        fn tcp_send(_s: *mut c_void, _data: &[u8]) -> Result<usize, NetError> {
            Err(DOWN)
        }
        fn tcp_recv(_s: *mut c_void, _buf: &mut [u8]) -> Result<usize, NetError> {
            Err(DOWN)
        }
        fn tcp_listen(_s: *mut c_void, _port: u16) -> Result<(), NetError> {
            Err(DOWN)
        }
        fn tcp_accept(_s: *mut c_void) -> Result<*mut c_void, NetError> {
            Err(DOWN)
        }
        fn udp_socket(_local_port: u16) -> Result<*mut c_void, NetError> {
            Err(DOWN)
        }
        fn udp_sendto(
            _s: *mut c_void,
            _buf: &[u8],
            _addr: u32,
            _port: u16,
        ) -> Result<usize, NetError> {
            Err(DOWN)
        }
        fn udp_recvfrom(_s: *mut c_void, _buf: &mut [u8]) -> Result<(usize, u32, u16), NetError> {
            Err(DOWN)
        }
        fn close(_s: *mut c_void) {}
        fn set_recv_timeout(_s: *mut c_void, _ms: u32) {}
        fn is_network_up() -> bool {
            false
        }
        fn get_ip_address() -> u32 {
            0
        }
        fn dns_resolve(_hostname: &str) -> Result<u32, NetError> {
            Err(DOWN)
        }
    }

    crate::set_hal_net!(TestHal);
}

/// Host RTOS for this crate's tests.
///
/// Queues, mutexes and semaphores are *real* — `executors::main_queue`'s
/// tests exercise genuine FIFO and coalescing semantics, so inert stubs
/// would make them vacuous. They are the simulator's, from
/// [`crate::hal::sim::rtos`]: this used to be a second implementation of the
/// same primitives, kept in step by hand with the one in the platform crate.
///
/// What stays local is what a test binary genuinely wants differently from a
/// simulator: spawning refuses outright, and the tick timer does nothing —
/// nothing in a test binary should start background work.
pub struct TestRtos;

// SAFETY: the primitives below are backed by `std` synchronisation, which
// provides the mutual exclusion and wakeup guarantees the trait requires.
unsafe impl Rtos for TestRtos {
    /// Refuses to spawn: a test binary must not start background work.
    /// The body is dropped, never run.
    fn spawn(_spec: &TaskSpec, _body: alloc::boxed::Box<dyn FnOnce() + Send>) -> bool {
        false
    }

    fn queue_create(depth: usize) -> RawQueue {
        sim_rtos::queue_create(depth)
    }
    fn queue_send(q: RawQueue, word: u32, t: Timeout) -> bool {
        sim_rtos::queue_send(q, word, t)
    }
    fn queue_recv(q: RawQueue, t: Timeout) -> Option<u32> {
        sim_rtos::queue_recv(q, t)
    }

    fn task_current() -> RawTask {
        sim_rtos::task_current()
    }
    fn scheduler_running() -> bool {
        sim_rtos::scheduler_running()
    }
    fn task_notify(t: RawTask) {
        sim_rtos::task_notify(t)
    }
    fn task_wait_notification(t: Timeout) -> bool {
        sim_rtos::task_wait_notification(t)
    }

    fn queue_create_ptr(depth: usize) -> RawQueue {
        sim_rtos::queue_create_ptr(depth)
    }
    fn queue_send_ptr(q: RawQueue, val: usize, t: Timeout) -> bool {
        sim_rtos::queue_send_ptr(q, val, t)
    }
    fn queue_recv_ptr(q: RawQueue, t: Timeout) -> Option<usize> {
        sim_rtos::queue_recv_ptr(q, t)
    }

    fn mutex_recursive_create() -> Option<RawMutex> {
        sim_rtos::mutex_recursive_create()
    }
    fn mutex_recursive_lock(m: RawMutex, t: Timeout) -> bool {
        sim_rtos::mutex_recursive_lock(m, t)
    }
    fn mutex_recursive_unlock(m: RawMutex) {
        sim_rtos::mutex_recursive_unlock(m)
    }

    fn sem_binary_create() -> RawSem {
        sim_rtos::sem_binary_create()
    }
    fn sem_give(s: RawSem) {
        sim_rtos::sem_give(s)
    }
    fn sem_take(s: RawSem, t: Timeout) -> bool {
        sim_rtos::sem_take(s, t)
    }

    fn tick_timer_start(_period_ms: u32, _cb: fn()) {}
    fn tick_timer_pause() {}
    fn tick_timer_resume() {}
    fn tick_timer_stop() {}
    fn delay_ms(_ms: u32) {}
}

crate::set_rtos!(TestRtos);

pub struct TestHooks;

impl PlatformHooks for TestHooks {
    fn stop_requested() -> bool {
        false
    }
    fn heap_bypass_enter() {}
    fn heap_bypass_exit() {}
    fn heap_checkpoint(_label: &str) {}
    fn native_heap_stats() -> NativeHeapStats {
        NativeHeapStats::default()
    }
    /// Nothing to register: the test binary has no platform modules holding
    /// Java object references, and `gc_root_registration::register_all` is
    /// `cfg(not(test))` anyway.
    fn register_gc_roots() {}
}

crate::set_platform_hooks!(TestHooks);

#[cfg(test)]
mod tests {
    //! Round-trip every facade through the stub registration. These assert
    //! almost nothing about behaviour — their job is to force the linker to
    //! resolve each `extern "Rust"` symbol, so a facade declaration with no
    //! matching shim fails here rather than in a firmware link.

    use crate::hal;

    #[test]
    fn display_facade_links() {
        hal::display::init();
        hal::display::set_window(0, 0, 1, 1);
        hal::display::write_pixels(&[0u8; 2]);
        hal::display::set_backlight(true);
        hal::display::display_sleep();
        hal::display::display_wake();
        hal::display::update_window();
        assert!(hal::display::is_window_open());
    }

    #[test]
    fn gpio_facade_links() {
        use hal::types::{EdgeTrigger, Pull};
        hal::gpio::set_direction(0, 1);
        hal::gpio::set_value(0, true);
        hal::gpio::set_input(0, Pull::Up);
        assert!(!hal::gpio::read(0));
        hal::gpio::enable_edge_irq(0, EdgeTrigger::Both);
        hal::gpio::disable_edge_irq(0);
        hal::gpio::init_gpio_irq();
        hal::gpio::inject(0, true);
        assert!(hal::gpio::drain_gpio_event().is_none());
        assert!(!hal::gpio::has_pending_event());
        hal::gpio::wait_for_button_event();
    }

    #[test]
    fn clock_touch_and_bus_facades_link() {
        hal::system_clock::sleep(0);
        assert_eq!(hal::system_clock::elapsed_realtime_nanos(), 0);

        hal::touch::init();
        assert!(hal::touch::read_point().is_none());
        assert_eq!(hal::touch::read_raw_unfiltered(), (0, 0));
        hal::touch::set_calibration(0, 1, 0, 1);
        hal::touch::inject_override(1, 1);
        hal::touch::release_override();
        hal::touch::clear_override();

        hal::i2c::init(0);
        hal::i2c::set_speed(0, 100_000);
        assert_eq!(hal::i2c::write_slice(0, 0x40, &[0]), -1);
        assert_eq!(hal::i2c::read_slice(0, 0x40, &mut [0]), -1);

        hal::adc::init(26);
        assert_eq!(hal::adc::read(26), 0.0);

        hal::pwm::init(0);
        hal::pwm::apply(0, 1000.0, 0.5, true);

        hal::spi::init(0);
        hal::spi::reconfigure(0, 1_000_000, 0);
        hal::spi::write_raw(0, &[0]);
        hal::spi::transfer_raw(0, &[0], &mut [0]);

        hal::uart::init(0);
        hal::uart::write_byte(0, b'x');
        assert_eq!(hal::uart::read_byte(0), -1);
    }

    #[test]
    fn rtos_facade_links() {
        use crate::rtos::{self, TaskKind, TaskSpec, Timeout};
        let spec = TaskSpec {
            name: "t",
            kind: TaskKind::BgWorker,
            priority: 5,
            stack_bytes: None,
        };
        assert!(!rtos::spawn(&spec, alloc::boxed::Box::new(|| {})));

        // FIFO order, bounded depth, and empty-means-None.
        let q = rtos::queue_create(2);
        assert!(rtos::queue_recv(q, Timeout::None).is_none());
        assert!(rtos::queue_send(q, 1, Timeout::None));
        assert!(rtos::queue_send(q, 2, Timeout::None));
        assert!(
            !rtos::queue_send(q, 3, Timeout::None),
            "send past depth must fail rather than grow the queue"
        );
        assert_eq!(rtos::queue_recv(q, Timeout::None), Some(1));
        assert_eq!(rtos::queue_recv(q, Timeout::None), Some(2));
        assert!(rtos::queue_recv(q, Timeout::None).is_none());

        // The pointer queue must not truncate. `usize::MAX` is the value that
        // distinguishes a real pointer-width queue from one backed by a `u32`
        // on a 64-bit host — a fixture that could not tell those apart would
        // pass against exactly the bug this triple exists to avoid.
        let qp = rtos::queue_create_ptr(2);
        assert!(rtos::queue_recv_ptr(qp, Timeout::None).is_none());
        assert!(rtos::queue_send_ptr(qp, usize::MAX, Timeout::None));
        assert!(rtos::queue_send_ptr(qp, 1, Timeout::None));
        assert!(
            !rtos::queue_send_ptr(qp, 2, Timeout::None),
            "send past depth must fail rather than grow the queue"
        );
        assert_eq!(rtos::queue_recv_ptr(qp, Timeout::None), Some(usize::MAX));
        assert_eq!(rtos::queue_recv_ptr(qp, Timeout::None), Some(1));
        assert!(rtos::queue_recv_ptr(qp, Timeout::None).is_none());

        // Notifications. A handle is available (this is a task context as far
        // as the backing is concerned), notifying self wakes self, and taking
        // clears rather than decrements.
        let me = rtos::task_current();
        assert_ne!(me, 0, "the calling thread must have a notification slot");
        assert!(
            !rtos::task_wait_notification(Timeout::None),
            "starts unsignalled"
        );
        rtos::task_notify(me);
        rtos::task_notify(me);
        assert!(rtos::task_wait_notification(Timeout::None));
        assert!(
            !rtos::task_wait_notification(Timeout::None),
            "taking clears the count — two notifies do not buy two wakes"
        );
        rtos::task_notify(0); // no task context: must be a no-op, not a crash

        // Recursion depth must be tracked: Java monitors re-enter.
        let m = rtos::mutex_recursive_create().expect("stub mutex");
        assert!(rtos::mutex_recursive_lock(m, Timeout::Forever));
        assert!(rtos::mutex_recursive_lock(m, Timeout::Forever));
        rtos::mutex_recursive_unlock(m);
        rtos::mutex_recursive_unlock(m);

        let s = rtos::sem_binary_create();
        assert!(!rtos::sem_take(s, Timeout::None), "starts unsignalled");
        rtos::sem_give(s);
        assert!(rtos::sem_take(s, Timeout::None), "give then take succeeds");
        assert!(!rtos::sem_take(s, Timeout::None), "binary, so not re-armed");
        rtos::tick_timer_start(16, || {});
        rtos::tick_timer_pause();
        rtos::tick_timer_resume();
        rtos::tick_timer_stop();
        rtos::delay_ms(0);
    }

    #[test]
    fn host_hooks_facade_links() {
        use crate::host;
        assert!(!host::stop_requested());
        host::heap_checkpoint("test");
        assert_eq!(host::native_heap_stats(), host::NativeHeapStats::default());
        {
            let _guard = host::heap_bypass();
        }
    }

    #[test]
    fn pd_log_macros_compile_and_run() {
        crate::pd_trace!("trace {}", 1);
        crate::pd_debug!("debug {}", 2);
        crate::pd_info!("info {}", 3);
        crate::pd_warn!("warn {}", 4);
        crate::pd_error!("error {}", 5);
    }
}
