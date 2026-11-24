//! The Detail page's rich-metadata table and the overview's Show more pill (Litefin's
//! `.details-rich-meta`, `docs/litefin-look.md` 5).
//!
//! Two stops. The Show more pill under the hero's overview ([`CARD_ELEM`]) opens the About panel
//! with the whole record; the table directly below the split ([`LANGUAGES_ELEM`]) holds Genres,
//! Directors and Writers as chips plus the Audio and Subtitles languages, and opens the Track
//! information panel, so it is a stop only while the item has its own file to inspect. The
//! focused table takes Litefin's accent outline and accent-dark fill.

use std::ffi::CString;

use crate::metadata::Detail;
use crate::ui::widgets::LfPlate;
use crate::ui::{theme, Painter, Rect};
use nj_machine::machine::{GroupId, Measure};

pub(crate) const ABOUT_ELEM_RANGE_START: u32 = 1664;
pub(crate) const ABOUT_ELEM_RANGE_END: u32 = 1728;
pub(crate) const ABOUT_GROUP: GroupId = GroupId(5);
/// The Show more pill.
pub(crate) const CARD_ELEM: u32 = ABOUT_ELEM_RANGE_START;
/// The rich-metadata table.
pub(crate) const LANGUAGES_ELEM: u32 = ABOUT_ELEM_RANGE_START + 1;

/// The table's box: margin 10 above, padding 20 at the sides, its rows 6 above and below the
/// chips with a 1px divider between, the label column 120 wide, the chips 4 apart (5).
const MARGIN_TOP: f32 = 10.0;
const PAD_X: f32 = 20.0;
const ROW_PAD: f32 = 6.0;
const LABEL_W: f32 = 120.0;
const CHIP_GAP: f32 = 4.0;
const LABEL_SZ: i32 = theme::size::lf::S19;
/// The focused table's outline (5: "its outline turns accent").
const FOCUS_RING_W: f32 = 2.0;
/// The air under the table before the rows (5: margin 50 below).
pub(crate) const GAP_BELOW: f32 = 50.0;

fn row_h() -> f32 {
    LfPlate::Chip { focused: false }.h() + 2.0 * ROW_PAD
}

const TABLE_X: f32 = crate::ui::detail_layout::SPLIT_X;
const TABLE_W: f32 = crate::ui::detail_layout::INFO_R - TABLE_X;

pub(crate) fn locate(key: u32, tracks_available: bool) -> Option<usize> {
    match key {
        CARD_ELEM => Some(0),
        LANGUAGES_ELEM if tracks_available => Some(1),
        _ => None,
    }
}

/// One row of the table: its app-owned label and the server's chips.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MetaRow {
    pub(crate) label: &'static str,
    pub(crate) chips: Vec<String>,
}

/// The rows `d` fills: genres, directors, writers and studios as Litefin's `genres-studios-writers`
/// style reads them (the household's default; its Tags row belongs to the `all` style only), then
/// the audio and subtitle languages, each language once. A row with nothing in it is not drawn.
pub(crate) fn rows_of(d: &Detail) -> Vec<MetaRow> {
    use nj_platform::i18n::msg;
    fn langs(streams: &[crate::metadata::Stream]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in streams {
            let name = if s.lang.is_empty() { msg::browse_detail_unknown() } else { s.lang.as_str() };
            if !out.iter().any(|l| l == name) {
                out.push(name.to_string());
            }
        }
        out
    }
    let writers = d.crew.iter().filter(|c| c.role.contains("Writer")).map(|c| c.tag.clone()).collect();
    [
        MetaRow { label: msg::details_genres(), chips: d.genres.clone() },
        MetaRow { label: msg::details_directors(), chips: d.directors.clone() },
        MetaRow { label: msg::details_writers(), chips: writers },
        MetaRow { label: msg::details_studios(), chips: d.studios.clone() },
        MetaRow { label: msg::details_audio(), chips: langs(&d.audio) },
        MetaRow { label: msg::details_subtitles(), chips: langs(&d.subs) },
    ]
    .into_iter()
    .filter(|r| !r.chips.is_empty())
    .collect()
}

/// The table's settled height in the flow, its top margin included; 0 without a row.
pub(crate) fn block_h(d: &Detail) -> f32 {
    let n = rows_of(d).len();
    if n == 0 { 0.0 } else { MARGIN_TOP + n as f32 * row_h() }
}

/// The table's rows, rebuilt once per item and on a same-identity landing ([`Rows::invalidate`]).
pub(crate) struct Rows {
    dirty: bool,
    identity: Option<(crate::catalog::ServerId, String)>,
    rows: Vec<MetaRow>,
}

impl Rows {
    pub(crate) fn new() -> Self {
        Self { dirty: true, identity: None, rows: Vec::new() }
    }

    pub(crate) fn update(&mut self, d: &Detail) {
        if !self.dirty && self.identity.as_ref().is_some_and(|(sid, rk)| *sid == d.sid && rk == &d.rk) {
            return;
        }
        self.dirty = false;
        self.identity = Some((d.sid, d.rk.clone()));
        self.rows = rows_of(d);
    }

    pub(crate) fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// The table's frame below a section top `top` (its margin is above it).
    pub(crate) fn languages_rect(&self, top: f32) -> Rect {
        Rect::new(TABLE_X, top + MARGIN_TOP, TABLE_W, self.rows.len() as f32 * row_h())
    }

    /// Draw the table at section top `top`; `focus` is its focus progress (0 idle, 1 focused).
    pub(crate) fn draw(&self, p: Painter, top: f32, focus: f32, measure: &dyn Measure) {
        if self.rows.is_empty() {
            return;
        }
        let frame = self.languages_rect(top);
        if focus > 0.01 {
            let rad = theme::radius::BASE;
            p.rrect(frame, rad, rad, theme::with_a(theme::BRAND_PANEL, theme::BRAND_PANEL[3] * focus));
            p.rring(frame, rad, FOCUS_RING_W, theme::with_a(theme::FOCUS_RING, focus));
        }
        let rh = row_h();
        let right = frame.x + frame.w - PAD_X;
        for (i, row) in self.rows.iter().enumerate() {
            let y = frame.y + i as f32 * rh;
            if i > 0 {
                let rule = theme::details_lf::DIVIDER;
                p.rect(Rect::new(frame.x + PAD_X, y, frame.w - 2.0 * PAD_X, 1.0), 0.0, rule, rule, 0.0);
            }
            let cy = y + rh * 0.5;
            if let Ok(c) = CString::new(row.label) {
                let ty = nj_gfx::text::text_vcenter_y(LABEL_SZ, 0, cy);
                p.text(c.as_ptr(), frame.x + PAD_X, ty, LABEL_SZ, theme::TEXT_SECONDARY, 0, 0);
            }
            let mut x = frame.x + PAD_X + LABEL_W;
            for chip in &row.chips {
                let plate = LfPlate::Chip { focused: false };
                if x + plate.w(chip, measure) > right {
                    break;
                }
                x += plate.draw(p, x, cy, chip, measure) + CHIP_GAP;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie() -> Detail {
        Detail {
            rk: "m".into(),
            genres: vec!["Drama".into(), "Sci-Fi".into()],
            directors: vec!["Ian Hubert".into()],
            crew: vec![crate::metadata::Cast {
                tag: "Ian Hubert".into(),
                role: "Writer".into(),
                thumb: String::new(),
                id: 0,
                tag_key: String::new(),
            }],
            audio: vec![
                crate::metadata::Stream { lang: "English".into(), ..Default::default() },
                crate::metadata::Stream { lang: "English".into(), ..Default::default() },
            ],
            studios: vec!["Blender Studio".into()],
            ..Default::default()
        }
    }

    #[test]
    fn only_the_pill_and_an_inspectable_table_are_focusable() {
        assert_eq!(locate(CARD_ELEM, false), Some(0));
        assert_eq!(locate(LANGUAGES_ELEM, false), None);
        assert_eq!(locate(LANGUAGES_ELEM, true), Some(1));
        assert_eq!(locate(ABOUT_ELEM_RANGE_START + 2, true), None);
    }

    /// The table lists what the item has, in order, each language once, and drops empty rows.
    #[test]
    fn the_table_rows_are_the_items_own_and_empty_rows_take_no_room() {
        use nj_platform::i18n::msg;
        let rows = rows_of(&movie());
        let labels: Vec<_> = rows.iter().map(|r| r.label).collect();
        assert_eq!(labels, [msg::details_genres(), msg::details_directors(), msg::details_writers(), msg::details_studios(), msg::details_audio()]);
        assert_eq!(rows[3].chips, ["Blender Studio"]);
        assert_eq!(rows[4].chips, ["English"]);
        assert_eq!(block_h(&movie()), MARGIN_TOP + 5.0 * row_h());
        assert_eq!(block_h(&Detail::default()), 0.0);
    }

    /// A same-identity landing that changes what the item says reaches the table once invalidated.
    #[test]
    fn a_same_identity_landing_rebuilds_the_rows_once_invalidated() {
        let mut rows = Rows::new();
        let mut d = movie();
        rows.update(&d);
        d.genres = vec!["Action".into()];
        rows.update(&d);
        assert_eq!(rows.rows[0].chips, ["Drama", "Sci-Fi"], "same identity, not invalidated");
        rows.invalidate();
        rows.update(&d);
        assert_eq!(rows.rows[0].chips, ["Action"]);
    }

    /// Every row label is app text and fits its 120px column in every shipped language, at the
    /// device's own advances.
    #[test]
    fn every_row_label_fits_its_column_in_every_language() {
        use nj_platform::i18n::msg;
        let measure = nj_base::fontcov::advances::ShippedMeasure;
        for lang in nj_platform::i18n::SHIPPED {
            let _locale = nj_platform::i18n::language_on_this_thread_for_test(lang);
            for label in [
                msg::details_genres(),
                msg::details_directors(),
                msg::details_writers(),
                msg::details_studios(),
                msg::details_audio(),
                msg::details_subtitles(),
            ] {
                let w = measure.width_str(label, LABEL_SZ, false);
                assert!(w <= LABEL_W, "{lang:?}: '{label}' is {w}px in a {LABEL_W}px column");
            }
        }
    }
}
