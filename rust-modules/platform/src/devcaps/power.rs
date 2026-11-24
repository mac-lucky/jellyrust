//! Whether the television was switched off, as the port's power watcher saw it.
//!
//! The watcher subscribes to the set's power state for the life of the process and publishes here
//! the moment of the last transition into a switched-off state (standby, suspend, power off).
//! The player asks whether one happened while a film was parked (`player::lifecycle`): a film
//! parked by an app switch plays on return, one parked while the set was switched off comes back
//! paused. "Screen Saver" and "Screen Off" are not a switch-off: the C1's remote maps a key to
//! screen off, which does not even background the app.
//!
//! Nothing is known until the set has answered; a refused or silent watcher leaves the answer
//! unknown, and the player then keeps its ordinary rule.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

const UNANSWERED: u8 = 0;
const ANSWERING: u8 = 1;
const REFUSED: u8 = 2;

static WATCH: AtomicU8 = AtomicU8::new(UNANSWERED);
/// [`now_ms`] at the last transition into a switched-off state; 0 = none seen.
static LAST_OFF_MS: AtomicU64 = AtomicU64::new(0);

/// A switch-off reported this long before the park still counts: the set's power report and the
/// app's background event come over two channels, in an order nobody has measured.
pub const PARK_GRACE_MS: u64 = 5_000;

/// Milliseconds on a process-local monotonic clock. The watcher's stamps and the park's own are
/// read off this one clock, so the two compare. It starts a day in, so a moment well before the
/// first stamp is still a real one (and 0 stays "none").
pub fn now_ms() -> u64 {
    const START_MS: u64 = 86_400_000;
    static START: OnceLock<Instant> = OnceLock::new();
    let start = *START.get_or_init(Instant::now);
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX).saturating_add(START_MS)
}

/// Is this power state the television being switched off? The states seen on these sets are
/// "Active", "Active Standby", "Suspend", "Screen Off" and "Screen Saver"; "Power Off" and
/// "Warm Standby" are LG's names for the others.
pub fn is_switch_off(state: &str) -> bool {
    state.contains("Standby") || state == "Suspend" || state == "Power Off"
}

/// Start the port's watcher (once; later calls do nothing). Off-device it does nothing.
pub fn watch() {
    crate::tv::watch_power();
}

/// The watcher's report of the set's current power state.
pub fn publish(state: &str) {
    publish_at(state, now_ms());
}

fn publish_at(state: &str, at_ms: u64) {
    WATCH.store(ANSWERING, Ordering::Release);
    if is_switch_off(state) {
        LAST_OFF_MS.store(at_ms, Ordering::Release);
    }
}

/// The watcher could not subscribe: the answer stays unknown for the life of the process.
pub fn refused() {
    WATCH.store(REFUSED, Ordering::Release);
}

/// Was the television switched off since `parked_at_ms` (a [`now_ms`] stamp), give or take
/// [`PARK_GRACE_MS`]? `None` while the watcher has not answered or was refused.
pub fn switched_off_since(parked_at_ms: u64) -> Option<bool> {
    if WATCH.load(Ordering::Acquire) != ANSWERING {
        return None;
    }
    let last = LAST_OFF_MS.load(Ordering::Acquire);
    Some(last != 0 && last >= parked_at_ms.saturating_sub(PARK_GRACE_MS))
}

/// Test seam (host and simulator have no bus): the set reports `state` at `at_ms`.
#[cfg(any(test, feature = "test-support", feature = "hostsim"))]
pub fn inject_for_test(state: &str, at_ms: u64) {
    publish_at(state, at_ms);
}

/// Test seam: back to "the watcher has not answered".
#[cfg(any(test, feature = "test-support", feature = "hostsim"))]
pub fn reset_for_test() {
    WATCH.store(UNANSWERED, Ordering::Release);
    LAST_OFF_MS.store(0, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standby_suspend_and_power_off_are_a_switch_off_and_the_screen_states_are_not() {
        for off in ["Active Standby", "Warm Standby", "Suspend", "Power Off"] {
            assert!(is_switch_off(off), "{off}");
        }
        for on in ["Active", "Screen Off", "Screen Saver", ""] {
            assert!(!is_switch_off(on), "{on}");
        }
    }

    #[test]
    fn a_switch_off_counts_from_shortly_before_the_park_and_unknown_stays_unknown() {
        let _g = nj_base::testlock::serial();
        reset_for_test();
        assert_eq!(switched_off_since(100_000), None, "the watcher has not answered");
        inject_for_test("Active", 1);
        assert_eq!(switched_off_since(100_000), Some(false));
        inject_for_test("Suspend", 100_000 - PARK_GRACE_MS - 1);
        assert_eq!(switched_off_since(100_000), Some(false), "an old switch-off");
        inject_for_test("Active Standby", 100_000 - PARK_GRACE_MS);
        assert_eq!(switched_off_since(100_000), Some(true));
        inject_for_test("Screen Saver", 120_000);
        assert_eq!(switched_off_since(100_000), Some(true), "a screen state is no new transition");
        refused();
        assert_eq!(switched_off_since(100_000), None);
        reset_for_test();
    }
}
