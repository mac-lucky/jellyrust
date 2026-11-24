//! The season's episode list for [`super::DetailScreen`]: Litefin's season page in its default
//! `list` layout (`docs/litefin-look.md` 5, `styles/season.css`).
//!
//! Each episode is one 250-tall row on an accent-dark card: the still on the left, the index,
//! title, meta and four lines of overview beside it, and a round button at the right. A row owns
//! two element identities: the card plays (and may be held for its item menu), the round button
//! opens the episode's own detail page. Encoding the stop in the key keeps both targets distinct
//! through restoration and reconciliation.

use std::ffi::CString;

use crate::metadata::{Detail, Episode};
use nj_machine::machine::GroupId;
use crate::ui::text_lift::TextLift;
use crate::ui::text_view::TextView;
use crate::ui::widgets::{self, PosterMark};
use crate::ui::{theme, Env, Painter, Rect, View};

pub(crate) const EPISODES_ELEM_RANGE_START: u32 = 128;
pub(crate) const EPISODES_ELEM_RANGE_END: u32 = 640;
pub(crate) const EPISODES_GROUP: GroupId = GroupId(2);

pub(crate) const MAX_ITEMS: usize =
    ((EPISODES_ELEM_RANGE_END - EPISODES_ELEM_RANGE_START) / 2) as usize;
/// The list: 20 above the first row, rows 250 tall and 24 apart, 80 below the last (5).
pub(crate) const LIST_TOP: f32 = 20.0;
pub(crate) const ROW_H: f32 = 250.0;
pub(crate) const ROW_GAP: f32 = 24.0;
pub(crate) const PITCH: f32 = ROW_H + ROW_GAP;
pub(crate) const LIST_BOTTOM: f32 = 80.0;
const LIST_X: f32 = super::section::ROW_X;
const LIST_W: f32 = crate::ui::detail_layout::INFO_R - LIST_X;
/// The still: 390 wide, the row's full height, on the card's left.
const THUMB_W: f32 = 390.0;
/// The text block's padding (18 x 24) and its runs: index 24/700, title 22.4/600 on one line,
/// meta 19.2 secondary, overview 20.8 secondary at .8 for four lines.
const PAD_X: f32 = 24.0;
const PAD_Y: f32 = 18.0;
const INDEX_SZ: i32 = theme::size::lf::S24;
const TITLE_SZ: i32 = theme::size::lf::S22;
const META_SZ: i32 = theme::size::lf::S19;
const OVERVIEW_SZ: i32 = theme::size::lf::S21;
const OVERVIEW_LEAD: f32 = OVERVIEW_SZ as f32 * 1.5;
const OVERVIEW_LINES: usize = 4;
const OVERVIEW_A: f32 = 0.8;
const LINE_GAP: f32 = 6.0;
/// The round button at the row's right: 64px, centred on the row.
const BTN_D: f32 = 64.0;
/// The card's corner and the focus treatment: scale 1.02 and a 3px accent ring (5).
const CARD_RADIUS: f32 = theme::radius::BASE;
pub(crate) const FOCUS_SCALE: f32 = 1.02;
/// The round button's focus pop.
const BTN_FOCUS_SCALE: f32 = 1.1;
pub(crate) const STALE_ALPHA: f32 = 0.35;
const TEXT_X: f32 = THUMB_W + PAD_X;
const TEXT_W: f32 = LIST_W - TEXT_X - PAD_X - BTN_D - PAD_X;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Row {
    /// The row's card: plays the episode.
    #[default]
    Still,
    /// The round button: opens the episode's detail page.
    Text,
}

pub(crate) fn elem(index: usize, row: Row) -> Option<u32> {
    (index < MAX_ITEMS)
        .then_some(EPISODES_ELEM_RANGE_START + index as u32 * 2 + u32::from(row == Row::Text))
}

pub(crate) fn locate(key: u32) -> Option<(usize, Row)> {
    if !(EPISODES_ELEM_RANGE_START..EPISODES_ELEM_RANGE_END).contains(&key) {
        return None;
    }
    let raw = key - EPISODES_ELEM_RANGE_START;
    Some((
        (raw / 2) as usize,
        if raw & 1 == 0 { Row::Still } else { Row::Text },
    ))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
    None,
    Play(usize),
    OpenDetail(crate::catalog::ServerId, String),
}

pub(crate) fn action(d: &Detail, key: u32, loading: bool) -> Action {
    if loading {
        return Action::None;
    }
    let Some((i, row)) = locate(key) else {
        return Action::None;
    };
    let Some(ep) = d.episodes.get(i) else {
        return Action::None;
    };
    match row {
        Row::Still => Action::Play(i),
        Row::Text => Action::OpenDetail(d.sid, ep.rk.clone()),
    }
}

/// Row `i`'s top below the section top `top`.
pub(crate) fn row_top(i: usize, top: f32) -> f32 {
    top + LIST_TOP + i as f32 * PITCH
}

/// Row `i`'s card at rest.
pub(crate) fn still_rect(i: usize, top: f32) -> Rect {
    Rect::new(LIST_X, row_top(i, top), LIST_W, ROW_H)
}

/// Row `i`'s round button at rest.
pub(crate) fn button_rect(i: usize, top: f32) -> Rect {
    let card = still_rect(i, top);
    Rect::new(card.x + card.w - PAD_X - BTN_D, card.y + (ROW_H - BTN_D) * 0.5, BTN_D, BTN_D)
}

/// The row nearest screen line `y` for a list whose section top is `top` — the seat focus takes
/// when it enters the list from above or below.
pub(crate) fn row_near_y(y: f32, top: f32, n: usize) -> usize {
    let i = ((y - top - LIST_TOP) / PITCH).floor().max(0.0) as usize;
    i.min(n.saturating_sub(1))
}

pub(crate) fn block_h(d: &Detail) -> f32 {
    let n = d.episodes.len().min(MAX_ITEMS);
    if n == 0 { 0.0 } else { LIST_TOP + n as f32 * PITCH - ROW_GAP }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Glyph {
    Play,
    Watched,
    None,
}

struct State {
    glyph: Glyph,
    label: String,
    progress: Option<f32>,
}

fn state(ep: &Episode) -> State {
    let in_progress = ep.dur_ms > 0 && ep.resume_ms > 0 && ep.resume_ms < ep.dur_ms;
    if in_progress {
        return State {
            glyph: Glyph::None,
            label: crate::ui::fmt::time_left(ep.dur_ms - ep.resume_ms),
            progress: Some((ep.resume_ms as f32 / ep.dur_ms as f32).clamp(0.0, 1.0)),
        };
    }
    State {
        glyph: if ep.watched { Glyph::Watched } else { Glyph::Play },
        label: super::hero::runtime(ep.dur_ms).unwrap_or_default(),
        progress: None,
    }
}

pub(crate) fn watch_state(ep: &Episode) -> PosterMark {
    let st = state(ep);
    if st.progress.is_some() {
        PosterMark::InProgress
    } else if st.glyph == Glyph::Watched {
        PosterMark::Watched
    } else {
        PosterMark::None
    }
}

/// The meta line: the runtime (or time left), the air date and the content rating.
fn meta_line(ep: &Episode) -> String {
    let date = crate::ui::fmt::pretty_date(&ep.aired, 0);
    [state(ep).label, date, ep.rating.clone()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("   ")
}

/// Draw the rows whose cards are on screen. `scale(i)` is row `i`'s card pop, `lift(i)` its
/// button's focus progress.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    p: Painter,
    d: &Detail,
    top: f32,
    focused: Option<(usize, Row)>,
    scale: impl Fn(usize) -> f32,
    lift: impl Fn(usize) -> TextLift,
    measure: &dyn nj_machine::machine::Measure,
    meta: crate::metadata::MetadataView<'_>,
) {
    let stale = if meta.season_loading() { STALE_ALPHA } else { 1.0 };
    let p = p.alpha(stale);
    for (i, ep) in d.episodes.iter().take(MAX_ITEMS).enumerate() {
        let y = row_top(i, top);
        if y > crate::ui::consts::SCR_H || y + ROW_H < 0.0 {
            continue;
        }
        draw_row(p, d, i, ep, top, focused, scale(i), &lift(i), measure);
    }
}

/// The focused row, drawn again over its neighbours so its pop reads on top of them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_focused(
    p: Painter,
    d: &Detail,
    index: usize,
    row: Row,
    top: f32,
    scale: f32,
    lift: &TextLift,
    measure: &dyn nj_machine::machine::Measure,
    meta: crate::metadata::MetadataView<'_>,
) {
    let Some(episode) = d.episodes.get(index) else { return };
    let stale = if meta.season_loading() { STALE_ALPHA } else { 1.0 };
    draw_row(p.alpha(stale), d, index, episode, top, Some((index, row)), scale, lift, measure);
}

#[allow(clippy::too_many_arguments)]
fn draw_row(
    p: Painter,
    d: &Detail,
    i: usize,
    ep: &Episode,
    top: f32,
    focused: Option<(usize, Row)>,
    scale: f32,
    lift: &TextLift,
    measure: &dyn nj_machine::machine::Measure,
) {
    let card_focused = focused == Some((i, Row::Still));
    let pop = ((scale - 1.0) / (FOCUS_SCALE - 1.0)).clamp(0.0, 1.0);
    let card = still_rect(i, top).scaled(scale);
    p.rrect(card, CARD_RADIUS, CARD_RADIUS, theme::mix(theme::BRAND_PANEL, theme::BRAND_PANEL_FOCUS, pop));
    let s = card.w / LIST_W;
    let thumb = Rect::new(card.x, card.y, THUMB_W * s, card.h);
    p.rrect(thumb, CARD_RADIUS, CARD_RADIUS, theme::CARD_PLACEHOLDER);
    let (tex, tw, th) = if ep.thumb.is_empty() {
        (0, 0.0, 0.0)
    } else {
        widgets::resolve_tex_wh_on(d.sid.raw(), &ep.thumb, 640, 360, 0)
    };
    if tex != 0 {
        p.tex_uv(tex, thumb.cover_uv(tw, th, crate::ui::Crop::Centre), thumb, CARD_RADIUS, theme::TINT_WHITE);
    }
    let st = state(ep);
    if let Some(frac) = st.progress {
        widgets::progress_bar(p, thumb, CARD_RADIUS, 6.0, frac);
    }
    let ring = if card_focused { 1.0 } else { pop };
    if ring > 0.0 {
        p.rring(card, CARD_RADIUS, theme::FOCUS_RING_W, theme::with_a(theme::FOCUS_RING, ring));
    }

    let x = card.x + TEXT_X * s;
    let mut y = card.y + PAD_Y;
    // One line each, elided to the block: index, title, meta. Each run is centred in its 1.5 line.
    let run = |text: &str, y: f32, sz: i32, ink: [f32; 4], bold: bool| -> f32 {
        let line = nj_gfx::text::elide_by(text, TEXT_W, false, |t| measure.width_str(t, sz, bold));
        let line_h = sz as f32 * 1.5;
        if let Ok(c) = CString::new(line) {
            let ty = nj_gfx::text::text_vcenter_y(sz, i32::from(bold), y + line_h * 0.5);
            p.text(c.as_ptr(), x, ty, sz, ink, 0, i32::from(bold));
        }
        line_h
    };
    y += run(&ep.index.to_string(), y, INDEX_SZ, theme::TEXT_PRIMARY, true);
    y += run(&ep.title, y, TITLE_SZ, theme::TEXT_PRIMARY, true);
    let meta = meta_line(ep);
    let meta_y = y;
    if !meta.is_empty() {
        y += run(&meta, y, META_SZ, theme::TEXT_SECONDARY, false);
    }
    if st.glyph == Glyph::Watched {
        let g = META_SZ as f32;
        let gap = if meta.is_empty() { 0.0 } else { 12.0 };
        let gx = x + measure.width_str(&meta, META_SZ, false) + gap;
        let gy = meta_y + (META_SZ as f32 * 1.5 - g) * 0.5;
        crate::ui::icons::draw(p, crate::ui::icons::Icon::Check, Rect::new(gx, gy, g, g), theme::BRAND_ACTIVE_INK);
    }
    if !ep.summary.is_empty() {
        TextView::new(&ep.summary, OVERVIEW_SZ, theme::with_a(theme::TEXT_SECONDARY, OVERVIEW_A))
            .with_measure(measure)
            .leading(OVERVIEW_LEAD)
            .max_lines(OVERVIEW_LINES)
            .draw(p, Rect::new(x, y + LINE_GAP, TEXT_W, 0.0));
    }

    let btn_focused = focused == Some((i, Row::Text));
    let btn = button_rect(i, top).scaled(1.0 + (BTN_FOCUS_SCALE - 1.0) * lift.factor());
    widgets::Button::new(c"".as_ptr(), theme::size::lf::S20, btn)
        .icon(crate::ui::icons::Icon::Info)
        .style(widgets::ControlStyle::Action { active: false })
        .focused(btn_focused)
        .draw(&Env::inert(), p);
    if btn_focused {
        p.rring(btn, btn.h * 0.5, theme::FOCUS_BORDER_W, theme::FOCUS_BORDER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_and_button_have_distinct_stable_keys() {
        for i in 0..MAX_ITEMS {
            let still = elem(i, Row::Still).unwrap();
            let text = elem(i, Row::Text).unwrap();
            assert_ne!(still, text);
            assert_eq!(locate(still), Some((i, Row::Still)));
            assert_eq!(locate(text), Some((i, Row::Text)));
        }
    }

    #[test]
    fn in_progress_wins_over_watched() {
        let ep = Episode { watched: true, resume_ms: 30, dur_ms: 100, ..Default::default() };
        assert_eq!(watch_state(&ep), PosterMark::InProgress);
    }

    #[test]
    fn a_resume_at_the_end_is_not_in_progress() {
        let ep = Episode { watched: true, resume_ms: 100, dur_ms: 100, ..Default::default() };
        assert_eq!(watch_state(&ep), PosterMark::Watched);
    }

    #[test]
    fn an_episode_resolves_its_three_states_into_one_mark() {
        for (ep, expected) in [
            (Episode { dur_ms: 100, ..Default::default() }, PosterMark::None),
            (Episode { dur_ms: 100, resume_ms: 25, ..Default::default() }, PosterMark::InProgress),
            (Episode { dur_ms: 100, watched: true, ..Default::default() }, PosterMark::Watched),
        ] {
            assert_eq!(watch_state(&ep), expected);
        }
        assert_eq!(state(&Episode { dur_ms: 100, resume_ms: 25, ..Default::default() }).progress, Some(0.25));
    }

    /// Rows stack 24 apart at 250 tall, the button sits inside its card at the right, and the
    /// nearest-row seat answers the row under a line and clamps at both ends.
    #[test]
    fn the_list_stacks_its_rows_and_seats_on_the_row_under_a_line() {
        let top = 1000.0;
        for i in 0..6 {
            let card = still_rect(i, top);
            assert_eq!(card.y, top + LIST_TOP + i as f32 * (ROW_H + ROW_GAP));
            let btn = button_rect(i, top);
            assert!(btn.x > card.x + TEXT_X && btn.x + btn.w < card.x + card.w);
            assert!(btn.y > card.y && btn.y + btn.h < card.y + card.h);
            assert_eq!(row_near_y(card.y + 10.0, top, 6), i);
        }
        assert_eq!(row_near_y(0.0, top, 6), 0);
        assert_eq!(row_near_y(1e6, top, 6), 5);
        let d = Detail { episodes: vec![Episode::default(); 3], ..Default::default() };
        assert_eq!(block_h(&d), LIST_TOP + 3.0 * ROW_H + 2.0 * ROW_GAP);
        assert_eq!(block_h(&Detail::default()), 0.0);
    }

    /// The text block keeps clear of the still and of the button.
    #[test]
    fn the_text_block_sits_between_the_still_and_the_button() {
        assert!(TEXT_X > THUMB_W);
        assert!(TEXT_X + TEXT_W < LIST_W - PAD_X - BTN_D);
        assert!(TEXT_W > 600.0);
    }
}
