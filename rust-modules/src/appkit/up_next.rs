//! **Up Next** — the next-episode tile in the transport HUD (`Player Screen.dc.html`, the control
//! row's third occupant). It appears when the playing episode reaches a FINAL credits marker (one
//! that runs to the end of the item) and the PlayQueue has another episode behind it, under the
//! default Next episode preference (Up Next countdown): the next episode's still, its address and
//! name, and a countdown that starts it on its own.
//!
//! The data is `route::up_next()` — lifted from the `continuous=1` PlayQueue that every playback
//! already creates, so nothing here asks the server what plays next.
//!
//! It TAKES THE CONTROL ROW as Litefin's Up Next card (`docs/litefin-look.md` 10, `styles/upnext.css`
//! style `normal`): a 600-wide card over the bottom right — the still with its episode badge,
//! "UP NEXT" in the accent, the episode in two lines, the countdown — and a right-aligned pair of
//! buttons, *Next Episode* (the primary, on the left as Litefin's *Play* is) and *Watch Credits*,
//! which cancels the countdown and leaves the card up as a plain OK target.
//!
//! It is HUD furniture, not an overlay: playback keeps running underneath and the credits keep
//! rolling. That is what makes the countdown honest — `app.rs` holds the HUD up for as long as it
//! is armed, so the timer is never running behind a hidden transport.
//!
//! **The row's cursor is not kept here.** It is `hud_nav.btn`, exactly as it is for the OSD, so
//! LEFT/RIGHT need no special case and [`ControlSlot::items`](crate::appkit::player_hud::ControlSlot)
//! answers for the clamp. This module owns the countdown and nothing else — which is also why the
//! cancel rule lives at `app.rs`'s one frame block rather than being spread across the key arms.
//!
//! *(A full-screen post-play CARD — show poster, DISPLAY title, countdown ring — was built for the
//! cross-show case and lives at commit `84ff9328`. It is not here because its trigger does not
//! exist: `continuous=1` is per-show, and a final episode returns `totalCount=1`, so the queue
//! never hands us a different show. Sourcing it from Continue Watching is the owner's call.)*
#![allow(dead_code)]
use crate::route::UpNext;
use crate::ui::consts::SCR_H;
use crate::ui::label::{HAlign, Label};
use crate::ui::theme::{self, osd};
use crate::ui::widgets::{draw_card, OsdPill};
use crate::ui::{Env, Painter, Rect, View};
use std::ffi::CString;
use std::os::raw::c_int;

/// How long the tile holds before starting the next episode by itself. Long enough to read the
/// title and reach for the remote, short enough that "always the next episode" stays automatic.
pub(crate) const COUNTDOWN_MS: u32 = 10_000;

/// The row's two items. Drawn *Next Episode* first, as Litefin's *Play* is
/// (`ControlSlot::neighbour` walks that order); indexed with *Watch Credits* at 0 so a cursor left
/// at 0 by the OSD buttons lands on the alternative, not on the countdown.
pub(crate) const BTN_CREDITS: c_int = 0;
pub(crate) const BTN_NEXT: c_int = 1;
/// Where the focus ring parks when the row appears: the PRIMARY. `ControlSlot::primary_btn` is
/// what reads this, so the appear edge in `app.rs` stays one line for every occupant; it is not
/// item 0, because the countdown's cancel rule is a steady state and item 0 declines it.
pub(crate) const PRIMARY_BTN: c_int = BTN_NEXT;

/// **The auto-advance countdown, owned by the player's instance** (`PlayerScreen::up_next`,
/// restructure spec §9). Two `static mut`s until phase 9, and the reason they had to move is the
/// reason every other piece of this route's state did: the countdown belongs to ONE playback, and a
/// module global cannot be told that a playback has ended — `reset` was called by hand from four
/// places to stand in for that.
#[derive(Default)]
pub(crate) struct Countdown {
    /// `SDL_GetTicks` deadline at which the next episode auto-starts. 0 = not armed (the tile is
    /// not up, or the countdown was cancelled).
    ///
    /// It stays a stored INSTANT rather than becoming a `Fx::Timer` alone, because the tile's
    /// primary button IS the countdown: its fill sweeps off the REMAINING milliseconds every frame
    /// ([`draw`]), so the deadline has to be readable, not merely scheduled. A timer beside it
    /// would be a second representation of one instant and a second thing to keep in step.
    deadline: u32,
    /// The user cancelled the auto-advance for THIS segment (they took hold of the row). Latched,
    /// because `tick` runs every frame and would otherwise re-arm the countdown the very next one.
    /// Cleared when the segment ends, so the next episode's credits get their own chance.
    cancelled: bool,
}

/// Whether this owns the control row this frame. The precedence itself lives in ONE place —
/// [`crate::appkit::player_hud::slot_for`] — so this is just a read of the resolved slot.
pub(crate) fn is_shown(slot: crate::appkit::player_hud::ControlSlot) -> bool {
    matches!(slot, crate::appkit::player_hud::ControlSlot::UpNext(_))
}

/// Per-frame tick: arm the countdown the moment the tile appears, and forget everything about it
/// once the segment is over. Idempotent — an already-running countdown is never pushed forward.
///
/// It arms on APPEARANCE, never on focus: the tile's whole promise is that it starts the next
/// episode on its own. Focus only ever [`cancel`]s it.
impl Countdown {
pub(crate) fn tick(&mut self, slot: crate::appkit::player_hud::ControlSlot, now: u32) {
    if !is_shown(slot) {
        // segment over (or the queue emptied): drop the deadline AND the cancel latch, so the
        // next episode's credits arm normally instead of inheriting this one's refusal
        self.reset();
        return;
    }
    if self.deadline == 0 && !self.cancelled {
        self.deadline = now.wrapping_add(COUNTDOWN_MS).max(1);
    }
}
}

/// PURE — may the countdown keep running, the transport standing this way?
///
/// ONE rule for every way a user can take hold of the transport, and it is a STEADY STATE rather
/// than an edge on purpose: focus off the control row means they are driving something else, and
/// focus on *Watch Credits* means they have reached for the alternative. Either way, engaging the
/// transport is not consent to be pulled into the next episode. Arrowing back to *Next Episode*
/// does not re-arm — [`cancel`] latches — so a countdown, once refused, stays refused for the
/// segment.
///
/// `bare_transport` is the third way and the one that reads as an omission until it bites: an
/// OVERLAY — a track menu, the Info card, the Chapters strip — is the only way of taking hold
/// of the transport that never moves the focus ring. Worse,
/// [`crate::appkit::player_hud::draw_hud`] draws the control row only for the BARE transport, so with
/// the Info card open the tile is not on screen at all. Without this term a countdown that was
/// already running when the card opened kept its clock behind a panel nobody could see it through
/// and cut to the next episode out of nowhere — the same failure `HudState::raise_for_offer`
/// exists to forbid, reached by the other door. A panel one presses OPEN is exactly as much
/// "engaging the transport" as an arrow key.
pub(crate) fn countdown_may_run(bare_transport: bool, row_focused: bool, btn: c_int) -> bool {
    bare_transport && row_focused && btn == BTN_NEXT
}

impl Countdown {
/// Stop the auto-advance without hiding the tile — the user took hold of the row, or is leaving.
/// The tile stays up as a plain "OK to play" target; only the clock is off.
pub(crate) fn cancel(&mut self) {
    self.deadline = 0;
    self.cancelled = true;
}

/// True while the countdown is running (drives holding the HUD up, so the timer is never invisible).
pub(crate) fn armed(&self) -> bool {
    self.deadline != 0
}

/// Milliseconds left (0 when disarmed or elapsed).
///
/// `SDL_GetTicks` wraps every ~49 days, so the comparison is the codebase's wrapping idiom
/// (`wrapping_sub` against the 0x8000_0000 half-range), not `now < deadline` — the same guard
/// `app.rs` uses for its deferred-refresh deadline.
///
/// While the screenshot pipeline holds the free-running clocks (`stillclock`,
/// [`nj_machine::motion::held_clock_ms`]) the countdown holds with them, at that much elapsed: its
/// fill then draws one fixed picture and it never runs out. Never in a build without
/// `devtriggers`.
fn remaining_ms(&self, now: u32) -> u32 {
    let d = self.deadline;
    if d == 0 {
        return 0;
    }
    if let Some(held) = nj_machine::motion::held_clock_ms() {
        return COUNTDOWN_MS.saturating_sub(held).max(1);
    }
    let left = d.wrapping_sub(now);
    if left == 0 || left >= 0x8000_0000 {
        0
    } else {
        left
    }
}

/// True once the countdown has run out — `app.rs` polls this and starts the next episode.
pub(crate) fn expired(&self, now: u32) -> bool {
    self.armed() && self.remaining_ms(now) == 0
}

/// The descriptor to start, cloned off the `&'static` store. Cloning is mandatory, not tidiness:
/// `route::request_play_up_next` clears `UP_NEXT` as its first act, so handing it a borrow of the
/// static would be a use-after-free the borrow checker cannot see through a `'static` lifetime.
pub(crate) fn take(&mut self, ps: &crate::route::PlaybackSession) -> Option<UpNext> {
    self.cancel();
    crate::route::up_next(ps).cloned()
}

/// Per-session reset — a new playback must not inherit the previous episode's countdown OR its
/// cancel latch.
pub(crate) fn reset(&mut self) {
    self.deadline = 0;
    self.cancelled = false;
}
}

// ---- geometry (`styles/upnext.css` at its 20px base) ----------------------------------------

/// The card: 600 wide (`max-width: 30em`), its foot 270 above the panel's bottom (`bottom: 13.5em`)
/// and its right edge on the OSD's ([`crate::appkit::player_hud::CTRL_RIGHT`]; Litefin's 48 is
/// inside the overscan keep-out).
const CARD_W: f32 = 600.0;
const CARD_BOTTOM: f32 = SCR_H - 270.0;
/// 1.4em around the card's content; the content row closes on .6em and the buttons open on .8em.
const PAD: f32 = 28.0;
const ROW_PAD_BOTTOM: f32 = 12.0;
const ACTIONS_PAD_TOP: f32 = 16.0;
/// The still, 7.5 x 4.25em, 1em before the text.
const THUMB_W: f32 = 150.0;
const THUMB_H: f32 = 85.0;
const INFO_GAP: f32 = 20.0;
/// .8em between the two buttons.
const PILL_GAP: f32 = 16.0;
/// The text block: "UP NEXT" 14 (the nearest rung, 16), the title 20 bold over two 1.3 lines, the
/// countdown 16.4 (16).
const LABEL_SZ: c_int = theme::size::lf::S16;
const TITLE_SZ: c_int = theme::size::lf::S20;
const COUNTDOWN_SZ: c_int = theme::size::lf::S16;
const LABEL_H: f32 = 22.0;
const TITLE_LEADING: f32 = 26.0;
const COUNTDOWN_H: f32 = 26.0;
const INFO_H: f32 = LABEL_H + 2.0 * TITLE_LEADING + COUNTDOWN_H;

/// The card's frames.
pub(crate) struct Layout {
    pub(crate) card: Rect,
    pub(crate) thumb: Rect,
    pub(crate) info: Rect,
    pub(crate) next: Rect,
    pub(crate) hide: Rect,
}

/// PURE — the whole card from the two MEASURED button sizes, so the draw and the pointer hit-test
/// are one arrangement and the arrangement is host-testable.
pub(crate) fn layout_of(next_w: f32, hide_w: f32, btn_h: f32) -> Layout {
    let right = crate::appkit::player_hud::CTRL_RIGHT;
    let h = PAD + INFO_H.max(THUMB_H) + ROW_PAD_BOTTOM + ACTIONS_PAD_TOP + btn_h + PAD;
    let card = Rect::new(right - CARD_W, CARD_BOTTOM - h, CARD_W, h);
    let thumb = Rect::new(card.x + PAD, card.y + PAD, THUMB_W, THUMB_H);
    let info_x = thumb.x + THUMB_W + INFO_GAP;
    let info = Rect::new(info_x, card.y + PAD, card.x + card.w - PAD - info_x, INFO_H);
    let btn_y = card.y + card.h - PAD - btn_h;
    let hide = Rect::new(card.x + card.w - PAD - hide_w, btn_y, hide_w, btn_h);
    let next = Rect::new(hide.x - PILL_GAP - next_w, btn_y, next_w, btn_h);
    Layout { card, thumb, info, next, hide }
}

/// [`layout_of`] over the two measured buttons. Only the two BUTTONS are pointer targets: with two
/// actions on the card, a click on the artwork has no single obvious meaning, and guessing one is
/// how a stray click starts an episode the user did not ask for.
pub(crate) fn layout(measure: &dyn nj_machine::machine::Measure) -> Layout {
    let (next_w, h) = OsdPill::size(nj_platform::i18n::msg::widgets_next_episode(), measure);
    let (hide_w, _) = OsdPill::size(nj_platform::i18n::msg::widgets_next_credits(), measure);
    layout_of(next_w, hide_w, h)
}

/// The card's title: the episode's address and name (`fmt::episode_kicker`, shared with the HUD
/// and the pre-roll ctx line), or the bare name for an unaddressed leaf.
fn caption(u: &UpNext) -> String {
    if u.season > 0 || u.index > 0 {
        crate::ui::fmt::episode_kicker(u.season, u.index, &u.ep_title)
    } else {
        u.ep_title.clone()
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    ps: &crate::route::PlaybackSession,
    row: &crate::appkit::player_hud::TransportRow,
    up: &Countdown,
    p: Painter,
    focused: bool,
    btn: c_int,
    now: u32,
    measure: &dyn nj_machine::machine::Measure,
) {
    let Some(u) = crate::route::up_next(ps) else {
        return;
    };
    let l = layout(measure);
    p.rrect(l.card, theme::radius::LG, theme::radius::LG, osd::CARD_FILL);
    p.rring(l.card, theme::radius::LG, 1.0, osd::CARD_BORDER);
    p.rrect(l.thumb, theme::radius::BASE, theme::radius::BASE, osd::CARD_THUMB_FILL);
    // decoration on a focusable card, not a focusable tile itself: drawn at rest scale
    draw_card(p, l.thumb, crate::route::item_sid(crate::route::cur_sid(ps)).raw(), &u.thumb, (480, 270),
        theme::radius::BASE, false, 1.0);
    if u.season > 0 || u.index > 0 {
        if let Ok(badge) = CString::new(format!("S{}:E{}", u.season, u.index)) {
            let w = measure.width(&badge, LABEL_SZ, true) + 10.0;
            let b = Rect::new(l.thumb.x + l.thumb.w - 4.0 - w, l.thumb.y + l.thumb.h - 4.0 - LABEL_H, w, LABEL_H);
            p.rrect(b, theme::radius::BASE, theme::radius::BASE, osd::CARD_BADGE_FILL);
            Label::new(badge.as_ptr(), LABEL_SZ, theme::TEXT_PRIMARY).bold().h(HAlign::Center).draw(p, b);
        }
    }
    let i = l.info;
    if let Ok(label) = CString::new(nj_platform::i18n::msg::player_osd_up_next().to_uppercase()) {
        Label::new(label.as_ptr(), LABEL_SZ, osd::CARD_LABEL).bold().draw(p, Rect::new(i.x, i.y, i.w, LABEL_H));
    }
    let title = caption(u);
    crate::ui::text_view::TextView::new(&title, TITLE_SZ, theme::TEXT_PRIMARY)
        .bold()
        .with_measure(measure)
        .leading(TITLE_LEADING)
        .max_lines(2)
        .draw(p, Rect::new(i.x, i.y + LABEL_H, i.w, 2.0 * TITLE_LEADING));
    if up.armed() {
        // It animates from a CLOCK, so `nj_machine::idle`'s spring instrumentation cannot see it — the trap
        // `Xfade::tick` and `Spinner::draw` both shipped frozen in. A HELD clock (`remaining_ms`) draws
        // one fixed second, so there is nothing to redraw — the same exception `widgets::Spinner` makes.
        if !nj_machine::motion::phase_clocks_held() {
            nj_machine::idle::invalidate();
        }
        let secs = i64::from(up.remaining_ms(now).div_ceil(1000));
        if let Ok(text) = CString::new(nj_platform::i18n::msg::player_osd_starts_in(&secs.to_string())) {
            Label::new(text.as_ptr(), COUNTDOWN_SZ, osd::CARD_COUNTDOWN)
                .draw(p, Rect::new(i.x, i.y + LABEL_H + 2.0 * TITLE_LEADING, i.w, COUNTDOWN_H));
        }
    }
    let e = Env::inert();
    // The focus pop is the CONTROL ROW's, not this card's: these two stand in the transport's own
    // row and share its cursor, so they share its springs (`TransportRow::pop`).
    for (idx, rect, label, primary) in [
        (BTN_NEXT, l.next, nj_platform::i18n::msg::widgets_next_episode_c(), true),
        (BTN_CREDITS, l.hide, nj_platform::i18n::msg::widgets_next_credits_c(), false),
    ] {
        OsdPill::new(label.as_ptr(), rect)
            .primary(primary)
            .focused(focused && btn == idx)
            .scale(row.pop(idx, OsdPill::FOCUS_SCALE))
            .draw(&e, p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one cancel rule, exhaustively. It is a STEADY STATE, so it has to answer for every
    /// (row focused × which button) pair rather than for a transition: the countdown survives only
    /// while the ring is resting on the primary, which is where the appear edge parks it.
    #[test]
    fn the_countdown_runs_only_while_the_ring_rests_on_the_primary() {
        assert!(
            countdown_may_run(true, true, BTN_NEXT),
            "the resting state the appear edge creates"
        );
        assert!(
            !countdown_may_run(true, true, BTN_CREDITS),
            "reaching for Watch Credits is reaching for the alternative"
        );
        for b in [BTN_CREDITS, BTN_NEXT] {
            assert!(
                !countdown_may_run(true, false, b),
                "focus off the row is the user driving the transport, whatever btn still says"
            );
        }
    }

    /// The overlay term, which no focus state can stand in for: an open panel never moves the ring,
    /// and the Info card and Chapters strip do not draw the control row at all — so the ONE state
    /// that looks most like consent (ring at rest on the primary) is exactly the one that would run
    /// a countdown nobody can see. Every combination is refused while a panel is up.
    #[test]
    fn an_open_panel_refuses_the_countdown_however_the_ring_rests_under_it() {
        for b in [BTN_CREDITS, BTN_NEXT] {
            for focused in [false, true] {
                assert!(
                    !countdown_may_run(false, focused, b),
                    "a tile behind a panel counts down where it cannot be seen or refused"
                );
            }
        }
    }

    /// The ring parks on the primary, *Next Episode*. If it ever parked on *Watch Credits* the
    /// countdown would arm and be cancelled by its own resting state on the very next frame, and the
    /// card would look like a timer that never runs.
    #[test]
    fn the_ring_parks_on_the_primary_and_the_countdown_survives_it() {
        assert_eq!(PRIMARY_BTN, BTN_NEXT);
        assert!(countdown_may_run(true, true, PRIMARY_BTN));
    }

    /// Geometry, from the card's own CSS: the card holds the OSD's right edge with its foot 270 above
    /// the panel's bottom, the still and the text sit inside its 28 padding, and the pair is
    /// right-aligned with the primary on the LEFT (so LEFT reaches it from Watch Credits) on one
    /// baseline. Driven through the PURE half with the sizes supplied.
    #[test]
    fn the_card_is_right_anchored_with_its_buttons_right_aligned() {
        let l = layout_of(190.0, 200.0, 50.0);
        let right = crate::appkit::player_hud::CTRL_RIGHT;
        assert_eq!(l.card.x + l.card.w, right, "the card holds the OSD's right edge");
        assert_eq!(l.card.y + l.card.h, CARD_BOTTOM);
        assert_eq!(l.card.w, CARD_W);
        assert_eq!(l.hide.x + l.hide.w, right - PAD, "the pair is right-aligned inside the padding");
        assert_eq!(l.hide.x - (l.next.x + l.next.w), PILL_GAP);
        assert_eq!((l.next.y, l.next.h), (l.hide.y, l.hide.h), "one baseline");
        assert_eq!(l.card.y + l.card.h - PAD, l.hide.y + l.hide.h);
        assert_eq!((l.thumb.x, l.thumb.y), (l.card.x + PAD, l.card.y + PAD));
        assert!(l.info.x + l.info.w <= right - PAD && l.info.y + l.info.h < l.next.y, "the text clears the buttons");
        assert!(l.card.y > crate::ui::consts::MARGIN_Y && l.card.y + l.card.h < crate::appkit::player_hud::CTRL_Y,
            "the card sits on the panel, above the controls row");
    }

    /// The title names the successor: the address comes from the shared `fmt::episode_kicker` (so
    /// the HUD, the pre-roll ctx line and this cannot drift), and an unaddressed leaf degrades to the
    /// bare title instead of drawing "S0, E0".
    #[test]
    fn the_caption_is_the_shared_episode_vocabulary_behind_an_up_next_kicker() {
        let ep = UpNext {
            season: 2,
            index: 4,
            ep_title: "Laura".into(),
            ..Default::default()
        };
        assert_eq!(caption(&ep), crate::ui::fmt::episode_kicker(2, 4, "Laura"));
        let bare = UpNext {
            ep_title: "Laura".into(),
            ..Default::default()
        };
        assert_eq!(caption(&bare), "Laura", "no address, no empty ordinal");
    }
}
