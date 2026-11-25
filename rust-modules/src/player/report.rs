//! **The playback attempt number.** One per committed plan, kept across reloads of the same
//! attempt, so Stats for nerds can tell one attempt's history from the next and a late result
//! from an earlier attempt is recognised as stale.

use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

static NEXT_TRACE_GENERATION: AtomicU32 = AtomicU32::new(0);

/// **A new attempt.** Called where the app commits to a plan, before anything opens a socket.
/// Never 0, which means "no attempt" (a preview's placeholder).
pub(crate) fn requested() -> u32 {
    let mut current = NEXT_TRACE_GENERATION.load(Relaxed);
    loop {
        let next = if current == u32::MAX { 1 } else { current + 1 };
        match NEXT_TRACE_GENERATION.compare_exchange_weak(current, next, Relaxed, Relaxed) {
            Ok(_) => return next,
            Err(actual) => current = actual,
        }
    }
}
