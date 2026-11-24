//! The **Skip Intro / Skip Credits** control. While the playhead sits inside one of the playing
//! leaf's server markers (`metadata::playing_markers`, fetched with `?includeMarkers=1`) this
//! button TAKES THE CONTROL ROW: it floats where Litefin's skip-intro widget does, 6 % in from the
//! left with its foot 28 % up the panel (`plugins/installed/skip-intro/skip-intro.css`), styled as
//! the Up Next card's buttons (`docs/litefin-look.md` 10), and the OSD buttons return the moment the
//! segment ends.
//!
//! It owning the row is the design call: one target the ring is already on. The cost is that the
//! OSD buttons are unreachable for the length of the segment, and that the button only exists while
//! the HUD is up — which is why `app.rs` RAISES the HUD once when a segment begins and parks focus
//! here, so a bare OK still skips in one press instead of three.
#![allow(dead_code)]
use crate::metadata::{self, MarkerKind};
use crate::ui::consts::{SCR_H, SCR_W};
use crate::ui::widgets::OsdPill;
use crate::ui::{Env, Painter, Rect, View};
use std::ffi::CString;

/// What pressing the button does. The distinction is the `final` flag on a credits marker: an
/// ordinary segment is a seek and playback continues past it, but a `final` one runs to the end of
/// the item, so "skip" means **the episode is over** — seeking to its end would race the decoder
/// against its own last frames for no benefit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SkipAction {
    /// jump to this position (ns) and keep playing
    Seek(i64),
    /// the item is finished — hand off to the end-of-playback path (next episode, or leave)
    Finish,
}

/// The offer the button is currently making. Carries the marker's TYPED kind, not its label:
/// asking "is the playhead in a credits segment?" by string-comparing user-facing copy made a pure
/// copy-edit — the sort of thing a design pass does — silently disable Up Next and auto-advance,
/// with no compile error and nothing to fail.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Prompt {
    /// the segment itself — carried so activating the button can retire it (`metadata::mark_skipped`)
    pub(crate) marker: metadata::Marker,
    pub(crate) kind: MarkerKind,
    pub(crate) action: SkipAction,
}

impl Prompt {
    /// The button's copy — presentation, derived from the kind at the point of drawing.
    pub(crate) fn label(&self) -> &'static str {
        match self.kind {
            MarkerKind::Intro => nj_platform::i18n::msg::widgets_skip_intro(),
            MarkerKind::Credits => nj_platform::i18n::msg::widgets_skip_credits(),
        }
    }
}

/// PURE: the offer a given segment makes. Takes the marker rather than reading the playhead, so
/// the precedence it feeds ([`crate::appkit::player_hud::slot_for`]) is host-testable and the whole frame
/// decides from ONE playhead sample — `playpos_ns` is written by LG's media thread, and re-reading
/// it per call site let the input path and the draw path disagree within a single frame.
///
/// Markers belong to the PLAYING leaf (`metadata::playing()`), never to `metadata::current()`:
/// during a show-page episode play `current()` is the SHOW, and offering episode 1's intro timing
/// during episode 5 is the same identity bug the track store exists to prevent.
pub(crate) fn prompt_for(m: metadata::Marker) -> Prompt {
    Prompt {
        marker: m,
        kind: m.kind,
        action: match m.kind {
            // a `final` credits segment runs to the end of the item, so "skip" means the episode is
            // over — seeking to its end would race the decoder against its own last frames
            MarkerKind::Credits if m.final_seg => SkipAction::Finish,
            _ => SkipAction::Seek(m.end_ms * 1_000_000),
        },
    }
}

/// The button's rect: left-anchored at 6 %, its foot at 72 % of the panel height.
pub(crate) fn rect(pr: Prompt, measure: &dyn nj_machine::machine::Measure) -> Rect {
    let (w, h) = OsdPill::size(pr.label(), measure);
    Rect::new(SCR_W * 0.06, SCR_H * 0.72 - h, w, h)
}

/// Draw the button. Called by `player_hud` INSTEAD of the OSD buttons. Its slot's only item, so
/// index 0 — the pop is the control ROW's ([`crate::appkit::player_hud::TransportRow::pop`]).
pub(crate) fn draw(row: &crate::appkit::player_hud::TransportRow, p: Painter, pr: Prompt, focused: bool, measure: &dyn nj_machine::machine::Measure) {
    let Ok(label) = CString::new(pr.label()) else {
        return;
    };
    OsdPill::new(label.as_ptr(), rect(pr, measure))
        .primary(true)
        .focused(focused)
        .scale(row.pop(0, OsdPill::FOCUS_SCALE))
        .draw(&Env::inert(), p);
}
