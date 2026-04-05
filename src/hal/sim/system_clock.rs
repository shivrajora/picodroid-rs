pub fn sleep(ms: u32) {
    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
}

pub fn elapsed_realtime_nanos() -> i64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = EPOCH.get_or_init(Instant::now);
    epoch.elapsed().as_nanos() as i64
}

/// Sim-mode frame pacer using `Instant`-based elapsed-time compensation.
pub struct FramePacer {
    last: std::time::Instant,
}

impl FramePacer {
    pub fn new() -> Self {
        Self {
            last: std::time::Instant::now(),
        }
    }

    /// Sleep until the next frame boundary (`period_ms` after last wakeup).
    pub fn pace(&mut self, period_ms: u32) {
        let target = std::time::Duration::from_millis(period_ms as u64);
        let elapsed = self.last.elapsed();
        if elapsed < target {
            std::thread::sleep(target - elapsed);
        }
        self.last = std::time::Instant::now();
    }
}
