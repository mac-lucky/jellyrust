//! Shared fixtures and helpers for the `ui::widgets` test modules split out below.

use super::*;
use crate::ui::tile::Resume;




// ── The poster's state mark: one vocabulary, one mark at a time ───────────────────────────
//
// `card` needs a GL context, so the CHOICE is what a host test can reach — and the choice is
// where this has been wrong before: the mark used to say "unwatched" in amber, which is the
// opposite claim, and the bar/disc precedence is only observable on an item PMS reports as both.

/// A MOVIE's facts at a given watched state, `in_progress` being the application's answer to its
/// resume rule (30 of 100 minutes in, when it is). For a LEAF the two flags really are each
/// other's negation — which is exactly what stops being true for a container, hence the show cases
/// below. The resume rule's own edges (an offset past the end, no runtime) are the application's
/// and are graded where it applies them, in `screens::registry::tile_facts`.
pub(super) fn row(watched: bool, in_progress: bool) -> TileFacts<'static> {
    TileFacts {
        watched,
        unwatched: !watched,
        resume: in_progress.then_some(Resume { frac: 0.3, left_ms: 70 * 60 * 1000 }),
        ..Default::default()
    }
}

// ── PageGround: the policy every browsing screen shares ────────────────────────────────────
//
// Pure over `theme` tokens and the real corner springs, as the block above is.

pub(super) fn ground_hash(ground: &PageGround) -> u64 {
    let mut c = nj_machine::machine::Canon::new();
    ground.write_motion(&mut c);
    c.finish()
}

pub(super) struct StatusMetrics;

impl nj_machine::machine::Measure for StatusMetrics {
    fn width(&self, _: &core::ffi::CStr, size: i32, bold: bool) -> f32 {
        // the status note wraps in the reason rung's regular face; everything else measured is
        // the action pill
        if size == STATUS_REASON_SZ && !bold {
            return 96.0;
        }
        assert_eq!(size, STATUS_CAP_SZ);
        assert!(bold, "the action uses the Button's bold face");
        96.0
    }
    fn cap_h(&self, _: i32) -> f32 { panic!("status layout uses line boxes") }
    fn line_h(&self, size: i32) -> f32 {
        match size {
            STATUS_FAILED_VERDICT_SZ => 52.0,
            STATUS_CAP_SZ => 40.0,
            STATUS_REASON_SZ => 28.0,
            _ => panic!("no status rung is {size}"),
        }
    }
}

// ---- the legibility yardstick ---------------------------------------------------------------

/// One sRGB channel, linearized (WCAG 2.x): the contrast yardstick lives in the tests rather than
/// in the app.
pub(super) fn srgb_lin(c: f32) -> f32 {
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// A spread of pill widths, wider than the device's (a real "TV" pill measures ≈67px at
/// `size::BODY`, "Kids & Family Movies" ≈350), so the capsule tests travel real distances.
pub(super) fn widths_for(n: usize) -> Vec<f32> {
    (0..n).map(|i| 140.0 + (i % 5) as f32 * 60.0).collect()
}

// `Spring::step` is `gfx::spring`, pure and already driven frame-by-frame by `card_row.rs`'s
// tests, so capsule MOTION is fully host-testable. What is not: anything through
// `with_tab_metrics` (it measures with SDL2_ttf), which is why these drive the pure
// `Capsule`/`TabStrip` against `widths_for` spans instead of the real row.

/// One frame at the app's own cadence, the value `card_row.rs`'s tests step with.
pub(super) const DT: f32 = 1.0 / 60.0;

/// Pill `i`'s content-space `(x, w)` in a fixture strip of `w`, laid out at [`STRIP_GAP`].
pub(super) fn span_of(w: &[f32], i: usize) -> (f32, f32) {
    (w[..i].iter().sum::<f32>() + STRIP_GAP * i as f32, w[i])
}

/// The capsule/strip tests drive `Capsule::step`, whose landing frame `Spring::jump`s — and
/// `Spring::jump` reports to `nj_machine::idle`'s process-global dirty flag. So they are serial by
/// obligation, not precaution (`xfade.rs`'s rule): under parallel libtest they intermittently
/// failed OTHER modules' "a settled screen asks for nothing" assertions.
pub(super) fn serial_for_motion() -> nj_base::testlock::Serial {
    nj_base::testlock::serial()
}
