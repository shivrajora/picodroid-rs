// SPDX-License-Identifier: GPL-3.0-only
//! The join supervisor: keeps a wanted WiFi network joined
//! (docs/networking-followups-2026-08.md NET-12).
//!
//! A link driver issues a join and then only mirrors what the chip reports.
//! Nothing in the chip's driver retries a join that ends in `NONET`
//! (the AP missed the probe), `FAIL`, a deauth, a link-down during the
//! handshake, or one whose events never arrive at all — the station then
//! stays down until the next reboot, which is what a few percent of
//! power cycles did on the bench. Android keeps a saved network joined
//! for as long as it is in range; this is the device-side half of that.
//!
//! Family-neutral and pure: the driver feeds it the station status it
//! mirrors anyway, plus the clock, and executes the [`Retry`] it hands
//! back. The policy:
//!
//! - A verdict of `NoNet`, `Fail` or `Down` (lost after being up, or the
//!   driver gave up without a verdict) schedules a rejoin after a backoff
//!   that doubles from [`BACKOFF_FIRST_MS`] to [`BACKOFF_MAX_MS`] and
//!   resets once the station is joined. `NoNet` starts lower, at
//!   [`NONET_BACKOFF_FIRST_MS`]: the AP missed one probe, a join scan
//!   takes under a second, and the AP is usually there on the next one —
//!   on the bench (2026-10-10) one boot in ten drew that verdict, and a
//!   boot that drew it five times in a row spent 93 s on the 3 s ladder
//!   before the sixth join landed.
//! - A `Joining` that has not become `Joined` within [`JOIN_TIMEOUT_MS`]
//!   is treated as lost: the chip is told to leave first, so a stale
//!   join-state word cannot swallow the new attempt, then rejoined.
//! - `BadAuth` is retried [`BADAUTH_ATTEMPTS`] times on the ladder, then
//!   every [`BADAUTH_HOLDOFF_MS`]: Android disables a network for five
//!   minutes after three authentication failures, but the chip reports a
//!   handshake that timed out under AP load with the same verdict as a
//!   wrong password — three times in a row on one bench boot — so the
//!   ladder runs to its cap first. A wrong password stays visible as such
//!   in Settings until the user changes it, and never hammers the AP.
//! - An explicit leave clears the wanted network; a new join request
//!   starts the policy afresh.

use crate::hal::wifi::Status;

/// A join still `Joining` after this long is retried.
pub const JOIN_TIMEOUT_MS: u32 = 15_000;
/// The first retry's delay; doubles per consecutive failure.
pub const BACKOFF_FIRST_MS: u32 = 3_000;
/// The first retry's delay after `NoNet` (a missed probe): the join scan
/// itself is under a second, so a quick second look costs nothing, and the
/// doubling (1, 2, 4, 8, 16, 32, 60 s) still stops hammering an AP that
/// is really gone.
pub const NONET_BACKOFF_FIRST_MS: u32 = 1_000;
/// The longest delay between two retries.
pub const BACKOFF_MAX_MS: u32 = 60_000;
/// How many consecutive bad-password verdicts stay on the backoff
/// ladder (the whole of it: 3, 6, 12, 24, 48, 60 s); the next retries
/// wait [`BADAUTH_HOLDOFF_MS`] each.
pub const BADAUTH_ATTEMPTS: u32 = 6;
/// The delay between bad-password retries once the ladder is spent.
pub const BADAUTH_HOLDOFF_MS: u32 = 300_000;

/// What made the supervisor schedule a retry; the driver logs it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reason {
    /// `NONET`: no AP with that SSID answered.
    NoNet,
    /// A join failure the driver did not classify.
    Fail,
    /// The AP rejected the password (or deauthed with that reason).
    BadAuth,
    /// The station reported down: the link was lost, or dropped during
    /// the join.
    Down,
    /// Still `Joining` after [`JOIN_TIMEOUT_MS`]: the chip never reported
    /// a verdict.
    Stuck,
}

/// A retry that is due now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Retry {
    pub reason: Reason,
    /// Issue a leave and let it settle before the join: the driver still
    /// believes a join is in progress.
    pub leave_first: bool,
    /// Which consecutive attempt this is (1 for the first retry).
    pub attempt: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    /// No network wanted: nothing to keep joined.
    Idle,
    /// A join was issued at `.0` and no verdict is in yet.
    Joining(u32),
    /// Joined; a `Down` from here is a lost link.
    Joined,
    /// A retry is due at `.0`, for `.1`.
    Waiting(u32, Reason),
}

/// Keeps one wanted network joined. One per link driver.
pub struct JoinSupervisor {
    phase: Phase,
    /// Consecutive failed attempts since the last `Joined`.
    failures: u32,
}

impl JoinSupervisor {
    pub const fn new() -> Self {
        JoinSupervisor {
            phase: Phase::Idle,
            failures: 0,
        }
    }

    /// A new network is wanted (boot, or the app asked): the retry ladder
    /// starts over, and a bad password gets its attempts again.
    pub fn requested(&mut self, now_ms: u32) {
        self.failures = 0;
        self.issued(now_ms);
    }

    /// The driver issued a join (at boot, for the app, or a retry) at
    /// `now_ms`. The driver may also have failed to issue it; it then
    /// mirrors `Fail` and the next `observe` schedules the retry.
    pub fn issued(&mut self, now_ms: u32) {
        self.phase = Phase::Joining(now_ms);
    }

    /// The driver left the network on request: nothing is wanted now.
    pub fn cleared(&mut self) {
        self.phase = Phase::Idle;
        self.failures = 0;
    }

    /// Whether a network is wanted and not joined.
    pub fn is_pending(&self) -> bool {
        !matches!(self.phase, Phase::Idle | Phase::Joined)
    }

    /// The station status the driver sees at `now_ms`; `Some` when a
    /// retry is due, which the driver issues at once (and reports back
    /// through [`issued`](Self::issued)).
    pub fn observe(&mut self, status: Status, now_ms: u32) -> Option<Retry> {
        match self.phase {
            Phase::Idle => None,
            Phase::Joined => {
                if status == Status::Down {
                    self.schedule(Reason::Down, now_ms);
                }
                None
            }
            Phase::Joining(since) => {
                match status {
                    Status::Joined => {
                        self.phase = Phase::Joined;
                        self.failures = 0;
                    }
                    Status::NoNet => self.schedule(Reason::NoNet, now_ms),
                    Status::Fail => self.schedule(Reason::Fail, now_ms),
                    Status::BadAuth => self.schedule(Reason::BadAuth, now_ms),
                    Status::Down => self.schedule(Reason::Down, now_ms),
                    Status::Joining => {
                        if now_ms.wrapping_sub(since) >= JOIN_TIMEOUT_MS {
                            self.schedule(Reason::Stuck, now_ms);
                        }
                    }
                }
                None
            }
            Phase::Waiting(due, reason) => {
                if status == Status::Joined {
                    // The chip got there on its own after all.
                    self.phase = Phase::Joined;
                    self.failures = 0;
                    return None;
                }
                if now_ms.wrapping_sub(due) < u32::MAX / 2 {
                    // Due. The driver reports the join it issues through
                    // `issued`; until then stay here, so a driver that
                    // could not issue it asks again next pass.
                    return Some(Retry {
                        reason,
                        leave_first: reason == Reason::Stuck,
                        attempt: self.failures,
                    });
                }
                None
            }
        }
    }

    fn schedule(&mut self, reason: Reason, now_ms: u32) {
        let delay = if reason == Reason::BadAuth && self.failures >= BADAUTH_ATTEMPTS {
            BADAUTH_HOLDOFF_MS
        } else {
            backoff_ms(reason, self.failures)
        };
        self.failures = self.failures.saturating_add(1);
        self.phase = Phase::Waiting(now_ms.wrapping_add(delay), reason);
    }
}

impl Default for JoinSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

/// The delay before retry number `failures + 1` for `reason`: doubling
/// from [`BACKOFF_FIRST_MS`] ([`NONET_BACKOFF_FIRST_MS`] for `NoNet`),
/// capped at [`BACKOFF_MAX_MS`].
pub fn backoff_ms(reason: Reason, failures: u32) -> u32 {
    let first = match reason {
        Reason::NoNet => NONET_BACKOFF_FIRST_MS,
        _ => BACKOFF_FIRST_MS,
    };
    let shift = failures.min(16);
    (first.saturating_mul(1u32 << shift)).min(BACKOFF_MAX_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive `observe` with `status` from `from_ms` to `to_ms` in 1 s
    /// steps; the first retry it hands back, with the time it fired at.
    fn run(
        s: &mut JoinSupervisor,
        status: Status,
        from_ms: u32,
        to_ms: u32,
    ) -> Option<(u32, Retry)> {
        let mut t = from_ms;
        while t <= to_ms {
            if let Some(r) = s.observe(status, t) {
                return Some((t, r));
            }
            t = t.wrapping_add(1_000);
        }
        None
    }

    #[test]
    fn backoff_doubles_to_the_cap() {
        assert_eq!(backoff_ms(Reason::Fail, 0), 3_000);
        assert_eq!(backoff_ms(Reason::Fail, 1), 6_000);
        assert_eq!(backoff_ms(Reason::Fail, 2), 12_000);
        assert_eq!(backoff_ms(Reason::Fail, 3), 24_000);
        assert_eq!(backoff_ms(Reason::Fail, 4), 48_000);
        assert_eq!(backoff_ms(Reason::Fail, 5), 60_000);
        assert_eq!(backoff_ms(Reason::Fail, 40), 60_000);
        assert_eq!(backoff_ms(Reason::Down, 0), 3_000);
        assert_eq!(backoff_ms(Reason::BadAuth, 0), 3_000);
        assert_eq!(backoff_ms(Reason::Stuck, 0), 3_000);
    }

    #[test]
    fn a_missed_probe_is_retried_sooner_and_still_doubles_to_the_cap() {
        assert_eq!(backoff_ms(Reason::NoNet, 0), 1_000);
        assert_eq!(backoff_ms(Reason::NoNet, 1), 2_000);
        assert_eq!(backoff_ms(Reason::NoNet, 2), 4_000);
        assert_eq!(backoff_ms(Reason::NoNet, 3), 8_000);
        assert_eq!(backoff_ms(Reason::NoNet, 4), 16_000);
        assert_eq!(backoff_ms(Reason::NoNet, 5), 32_000);
        assert_eq!(backoff_ms(Reason::NoNet, 6), 60_000);
        assert_eq!(backoff_ms(Reason::NoNet, 40), 60_000);
    }

    #[test]
    fn nothing_wanted_means_nothing_retried() {
        let mut s = JoinSupervisor::new();
        assert_eq!(run(&mut s, Status::Down, 0, 120_000), None);
        assert_eq!(run(&mut s, Status::NoNet, 0, 120_000), None);
        assert!(!s.is_pending());
    }

    #[test]
    fn a_join_that_succeeds_is_left_alone() {
        let mut s = JoinSupervisor::new();
        s.issued(0);
        assert!(s.is_pending());
        assert_eq!(run(&mut s, Status::Joining, 0, 5_000), None);
        assert_eq!(s.observe(Status::Joined, 6_000), None);
        assert!(!s.is_pending());
        assert_eq!(run(&mut s, Status::Joined, 7_000, 600_000), None);
    }

    #[test]
    fn no_such_network_is_retried_after_the_first_backoff() {
        let mut s = JoinSupervisor::new();
        s.issued(0);
        assert_eq!(s.observe(Status::Joining, 1_000), None);
        // The verdict lands at 4 s; the retry is due 1 s later.
        assert_eq!(s.observe(Status::NoNet, 4_000), None);
        assert_eq!(s.observe(Status::NoNet, 4_999), None);
        assert_eq!(
            s.observe(Status::NoNet, 5_000),
            Some(Retry {
                reason: Reason::NoNet,
                leave_first: false,
                attempt: 1
            })
        );
    }

    #[test]
    fn the_rung_is_shared_across_verdicts_and_the_base_follows_the_latest() {
        // Bench cycle 20 of 2026-10-10: NONET, then the retry's AUTH timed
        // out (FAIL). The second retry is the second rung of the 3 s ladder.
        let mut s = JoinSupervisor::new();
        s.issued(0);
        assert_eq!(s.observe(Status::NoNet, 1_500), None);
        let (fired, r) = run(&mut s, Status::NoNet, 1_500, 10_000).expect("retry");
        assert_eq!((fired, r.reason, r.attempt), (2_500, Reason::NoNet, 1));
        s.issued(fired);
        assert_eq!(s.observe(Status::Fail, 8_800), None);
        let (fired, r) = run(&mut s, Status::Fail, 8_800, 60_000).expect("retry");
        assert_eq!((fired, r.reason, r.attempt), (14_800, Reason::Fail, 2));
    }

    #[test]
    fn a_retry_is_repeated_until_the_driver_reports_it_issued() {
        let mut s = JoinSupervisor::new();
        s.issued(0);
        s.observe(Status::Fail, 1_000);
        let first = s.observe(Status::Fail, 4_000).expect("due");
        assert_eq!(first.reason, Reason::Fail);
        // Not issued yet (the driver could not): asked again next pass.
        assert!(s.observe(Status::Fail, 5_000).is_some());
        s.issued(5_000);
        assert_eq!(s.observe(Status::Joining, 6_000), None);
    }

    #[test]
    fn consecutive_failures_back_off_and_a_join_resets_them() {
        let mut s = JoinSupervisor::new();
        s.issued(0);
        let mut t = 1_000;
        let mut gaps = Vec::new();
        for _ in 0..6 {
            // Verdict now; measure the delay to the retry.
            assert_eq!(s.observe(Status::NoNet, t), None);
            let (fired, r) = run(&mut s, Status::NoNet, t, t + 120_000).expect("retry");
            gaps.push(fired - t);
            assert_eq!(r.reason, Reason::NoNet);
            s.issued(fired);
            t = fired + 1_000;
        }
        assert_eq!(gaps, vec![1_000, 2_000, 4_000, 8_000, 16_000, 32_000]);
        // Joined: the ladder starts over, and a lost link waits the full
        // first step.
        s.observe(Status::Joined, t);
        assert!(!s.is_pending());
        s.observe(Status::Down, t + 1_000);
        let (fired, r) = run(&mut s, Status::Down, t + 1_000, t + 120_000).expect("retry");
        assert_eq!(r.reason, Reason::Down);
        assert_eq!(fired - (t + 1_000), 3_000);
        assert_eq!(r.attempt, 1);
    }

    #[test]
    fn a_join_with_no_verdict_is_retried_with_a_leave_first() {
        let mut s = JoinSupervisor::new();
        s.issued(10_000);
        assert_eq!(run(&mut s, Status::Joining, 10_000, 24_000), None);
        // Stuck at 15 s; the retry is due 3 s after that.
        assert_eq!(s.observe(Status::Joining, 25_000), None);
        let (fired, r) = run(&mut s, Status::Joining, 25_000, 60_000).expect("retry");
        assert_eq!(fired, 28_000);
        assert_eq!(r.reason, Reason::Stuck);
        assert!(r.leave_first);
    }

    #[test]
    fn a_lost_link_is_rejoined() {
        let mut s = JoinSupervisor::new();
        s.issued(0);
        s.observe(Status::Joined, 5_000);
        assert_eq!(run(&mut s, Status::Joined, 5_000, 300_000), None);
        // The AP went away at 300 s.
        assert_eq!(s.observe(Status::Down, 300_000), None);
        assert!(s.is_pending());
        let (fired, r) = run(&mut s, Status::Down, 300_000, 400_000).expect("retry");
        assert_eq!(fired, 303_000);
        assert_eq!(r.reason, Reason::Down);
        assert!(!r.leave_first);
    }

    #[test]
    fn a_down_during_the_join_is_rejoined() {
        let mut s = JoinSupervisor::new();
        s.issued(0);
        // The driver's DISASSOC arrives mid-join: status Down, never Joined.
        assert_eq!(s.observe(Status::Joining, 2_000), None);
        assert_eq!(s.observe(Status::Down, 3_000), None);
        let (fired, r) = run(&mut s, Status::Down, 3_000, 60_000).expect("retry");
        assert_eq!(fired, 6_000);
        assert_eq!(r.reason, Reason::Down);
    }

    #[test]
    fn bad_password_rides_the_whole_ladder_then_waits_five_minutes() {
        let mut s = JoinSupervisor::new();
        s.issued(0);
        let mut t = 2_000;
        let mut gaps = Vec::new();
        for _ in 0..8 {
            assert_eq!(s.observe(Status::BadAuth, t), None);
            let (fired, r) = run(&mut s, Status::BadAuth, t, t + 400_000).expect("retry");
            assert_eq!(r.reason, Reason::BadAuth);
            gaps.push(fired - t);
            s.issued(fired);
            t = fired + 2_000;
        }
        assert_eq!(
            gaps,
            vec![3_000, 6_000, 12_000, 24_000, 48_000, 60_000, 300_000, 300_000]
        );
        assert!(s.is_pending());
        // A new password from the user starts the ladder over.
        s.requested(t);
        assert_eq!(s.observe(Status::BadAuth, t + 1_000), None);
        let (fired, _) = run(&mut s, Status::BadAuth, t + 1_000, t + 60_000).expect("retry");
        assert_eq!(fired - (t + 1_000), 3_000);
    }

    #[test]
    fn a_handshake_timeout_read_as_bad_password_recovers_on_the_first_retry() {
        // Bench cycle 13 of 2026-10-09: PSK_SUP status 4 reason 15 (the
        // EAPOL timeout), the driver's own rejoin, then a PSK_SUP status 4
        // reason 0 the driver files as BADAUTH. One retry joined.
        let mut s = JoinSupervisor::new();
        s.issued(0);
        assert_eq!(s.observe(Status::Joining, 6_000), None);
        assert_eq!(s.observe(Status::BadAuth, 9_700), None);
        let (fired, r) = run(&mut s, Status::BadAuth, 9_700, 60_000).expect("retry");
        assert_eq!(fired, 12_700);
        assert_eq!(r.attempt, 1);
        s.issued(fired);
        assert_eq!(s.observe(Status::Joining, 13_000), None);
        assert_eq!(s.observe(Status::Joined, 15_000), None);
        assert!(!s.is_pending());
    }

    #[test]
    fn a_leave_cancels_the_pending_retry() {
        let mut s = JoinSupervisor::new();
        s.issued(0);
        s.observe(Status::NoNet, 1_000);
        s.cleared();
        assert!(!s.is_pending());
        assert_eq!(run(&mut s, Status::Down, 1_000, 120_000), None);
    }

    #[test]
    fn a_join_that_lands_while_waiting_cancels_the_retry() {
        let mut s = JoinSupervisor::new();
        s.issued(0);
        s.observe(Status::Down, 1_000);
        // The chip rejoined by itself (the driver's own key-timeout rejoin).
        assert_eq!(s.observe(Status::Joined, 2_000), None);
        assert!(!s.is_pending());
        assert_eq!(run(&mut s, Status::Joined, 2_000, 120_000), None);
    }

    #[test]
    fn the_clock_may_wrap() {
        let mut s = JoinSupervisor::new();
        let start = u32::MAX - 2_000;
        s.issued(start);
        assert_eq!(s.observe(Status::Fail, start.wrapping_add(1_000)), None);
        let due = start.wrapping_add(4_000);
        assert_eq!(s.observe(Status::Fail, due.wrapping_sub(1)), None);
        assert!(s.observe(Status::Fail, due).is_some());
    }
}
