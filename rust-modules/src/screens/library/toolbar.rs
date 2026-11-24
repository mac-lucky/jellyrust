//! Litefin's library head (`docs/litefin-look.md` 6): the centred tab pills - the section's
//! listing types and Genres - and the centred controls row - the item count, then Shuffle, Sort,
//! Filter and View Mode. One pill model supplies both focus geometry and paint.
use std::ffi::CString;
use super::*;
use super::layout::{CENTRE_X, CONTROL_H, TAB_H};
use crate::browse::LibraryType;
use crate::ui::icons::Icon;
use crate::ui::theme;

/// The air between two pills of either row (6: "20 apart").
const PILL_GAP: f32 = 20.0;
/// A tab's side padding and label rung (6 Tabs: padding 12x28, 20px/600).
const TAB_PAD_X: f32 = 28.0;
const TAB_SIZE: i32 = theme::size::lf::S20;
/// A control's side padding, icon and label rung (6 Controls: padding 12x18, 18px/600, icon +
/// label).
const CONTROL_PAD_X: f32 = 18.0;
const CONTROL_SIZE: i32 = theme::size::lf::S18;
const CONTROL_ICON: f32 = 22.0;
const CONTROL_ICON_GAP: f32 = 8.0;
/// The item count's rung: Litefin's 14px rounds up to the lowest rung (6 Controls).
const COUNT_SIZE: i32 = theme::size::lf::S16;
/// The head pills' slots in the page's one pop: tabs first, then the controls.
pub(super) const TAB_POPS: usize = 8;
pub(super) const HEAD_POPS: usize = TAB_POPS + 4;

/// One tab: a listing type of the section, or the Genres rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tab {
    Type(LibraryType),
    Genres,
}

/// One pill of the head as drawn: its label, its icon and whether it is the selected tab.
pub(super) struct Pill {
    pub label: CString,
    pub icon: Option<Icon>,
    pub size: i32,
    pub selected: bool,
}

impl Pill {
    pub(super) fn width(&self, measure: &dyn nj_machine::machine::Measure) -> f32 {
        let text = measure.width(&self.label, self.size, true);
        match self.icon {
            Some(_) => 2.0 * CONTROL_PAD_X + CONTROL_ICON + CONTROL_ICON_GAP + text,
            None => 2.0 * TAB_PAD_X + text,
        }
    }

    /// Paint the pill at `rest` grown by `scale` about its centre: Litefin's action fill and
    /// 2px border at rest, the lavender fill with accent text when selected, the accent fill with
    /// lavender text under focus (6 Tabs and Controls, 2.6 Primary buttons).
    pub(super) fn draw(&self, p: crate::ui::Painter, rest: Rect, scale: f32, focused: bool, measure: &dyn nj_machine::machine::Measure) {
        let r = rest.scaled(scale);
        let rad = theme::radius::LG * scale;
        let ink = if focused {
            p.rect(r, rad, theme::BRAND_FILL, theme::BRAND_FILL, 0.0);
            theme::BRAND_INK
        } else if self.selected {
            p.rect(r, rad, theme::SELECTED_FILL, theme::SELECTED_FILL, 0.0);
            theme::SELECTED_INK
        } else {
            p.rect_rimmed_w(r, rad, theme::ACTION_FILL, theme::ACTION_FILL, theme::ACTION_BORDER, 0.0,
                theme::ACTION_BORDER_W * scale);
            theme::TEXT_PRIMARY
        };
        let text = measure.width(&self.label, self.size, true);
        let content = text + self.icon.map_or(0.0, |_| CONTROL_ICON + CONTROL_ICON_GAP);
        let mut x = r.cx() - content * 0.5;
        if let Some(icon) = self.icon {
            crate::ui::icons::draw(p, icon, Rect::new(x, r.cy() - CONTROL_ICON * 0.5, CONTROL_ICON, CONTROL_ICON), ink);
            x += CONTROL_ICON + CONTROL_ICON_GAP;
        }
        crate::ui::label::Label::new(self.label.as_ptr(), self.size, ink).bold()
            .draw(p, Rect::new(x, r.y, text, r.h));
    }
}

/// Lay a centred row of pills of widths `widths`, led by a `lead`-wide run of text: each pill's
/// `(x, w)` and the lead's x.
pub(super) fn centred(lead: f32, widths: &[f32]) -> (f32, Vec<(f32, f32)>) {
    let pills = widths.iter().sum::<f32>() + PILL_GAP * widths.len().saturating_sub(1) as f32;
    let total = pills + if lead > 0.0 { lead + PILL_GAP } else { 0.0 };
    let start = CENTRE_X - total * 0.5;
    let mut x = start + if lead > 0.0 { lead + PILL_GAP } else { 0.0 };
    let spans = widths.iter().map(|&w| { let span = (x, w); x += w + PILL_GAP; span }).collect();
    (start, spans)
}

impl LibraryScreen {
    /// The controls row: Shuffle, Sort, Filter while the listed type takes filters (collections
    /// take none), and View Mode, which opens the TYPE menu.
    pub(super) fn toolbar_elems(&self) -> &'static [u32] {
        if self.listed.filters() { &[SHUFFLE, SORT, FILTER, TYPE] } else { &[SHUFFLE, SORT, TYPE] }
    }

    /// The tabs: every listing type the section's kind offers, then Genres.
    pub(super) fn tabs(&self) -> Vec<Tab> {
        LibraryType::offered(self.kind).iter().map(|&kind| Tab::Type(kind)).chain([Tab::Genres]).collect()
    }

    pub(super) fn tab_elems(&self) -> Vec<u32> {
        (0..self.tabs().len()).map(|i| TAB_BASE + i as u32).collect()
    }

    pub(super) fn tab_of(&self, elem: u32) -> Option<Tab> {
        elem.checked_sub(TAB_BASE).and_then(|i| self.tabs().get(i as usize).copied())
    }

    /// The tab the page shows: Genres while its rows are up, else the listing's type - the one a
    /// queued TYPE edit names, once asked for.
    pub(super) fn active_tab<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> Tab {
        if self.genres == GenreView::Rows { return Tab::Genres; }
        let listing = H::listing(cx);
        match self.pending.grid().filter(|(target, _)| target.matches(listing)).map(|(_, action)| action) {
            Some(GridAction::LibraryType(kind)) => Tab::Type(*kind),
            _ => Tab::Type(listing.library_type()),
        }
    }

    pub(super) fn view_section<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> Option<usize> {
        let directory = H::directory(cx);
        match &self.wanted {
            Some(wanted) => super::wanted_index(directory, wanted), None => directory.current(),
        }.filter(|index| *index < directory.sections().len())
    }

    /// The page title: the genre inside one genre, else the library's own name.
    pub(super) fn title<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> String {
        if let GenreView::Drill(id) = &self.genres {
            let hubs = H::section_hubs(cx);
            if let Some(shelf) = hubs.shelves().iter().find(|shelf| shelf.genre.as_deref() == Some(id.as_str())) {
                return shelf.title.clone();
            }
            if let Some(genre) = H::listing(cx).genre() { return genre.title.clone(); }
        }
        let directory = H::directory(cx);
        self.view_section(cx).and_then(|index| directory.sections().get(index))
            .map(|section| section.row.title.clone()).unwrap_or_default()
    }

    pub(super) fn tab_pill<H: LibraryLike>(&self, tab: Tab, cx: &Cx<'_, H>) -> Pill {
        let label = match tab {
            Tab::Genres => nj_platform::i18n::msg::browse_litefin_genres().to_owned(),
            Tab::Type(kind) => {
                let listing = H::listing(cx);
                let assorted = listing.id().and_then(|id| H::directory(cx).sections().iter()
                    .find(|section| section.sid == Some(id.sid) && section.key == id.section))
                    .is_some_and(|section| section.row.assorted);
                kind.menu_title(self.kind, assorted).to_owned()
            }
        };
        Pill { label: CString::new(label).unwrap_or_default(), icon: None, size: TAB_SIZE, selected: self.active_tab(cx) == tab }
    }

    /// A control pill. Filter names what it applies - a queued edit at once - in place of its
    /// own name, so a filtered grid says so on its face; Litefin's Reset control says it instead.
    pub(super) fn control_pill<H: LibraryLike>(&self, elem: u32, cx: &Cx<'_, H>) -> Pill {
        let (label, icon): (CString, Icon) = match elem {
            SHUFFLE => (nj_platform::i18n::msg::browse_litefin_shuffle_c().into(), Icon::Shuffle),
            SORT => (nj_platform::i18n::msg::browse_library_sort_c().into(), Icon::Sort),
            FILTER => (self.filter_label(cx), Icon::Filter),
            _ => (nj_platform::i18n::msg::browse_litefin_view_mode_c().into(), Icon::ViewMode),
        };
        Pill { label, icon: Some(icon), size: CONTROL_SIZE, selected: false }
    }

    fn filter_label<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> CString {
        let listing = H::listing(cx);
        let queued = self.pending.grid().filter(|(target, _)| target.matches(listing)).map(|(_, action)| action);
        let genre = match queued {
            Some(GridAction::Genre { id }) => id.as_ref().and_then(|id| listing.genres().iter().find(|genre| &genre.id == id)),
            _ => listing.genre(),
        }.map(|genre| genre.title.as_str());
        let unwatched = match queued {
            Some(GridAction::Unwatched { desired }) => *desired,
            _ => listing.unwatched(),
        };
        let label = match (genre, unwatched) {
            (None, false) => nj_platform::i18n::msg::browse_library_filter().into(),
            (None, true) => nj_platform::i18n::msg::browse_library_unwatched().into(),
            (Some(genre), false) => genre.into(),
            (Some(genre), true) => nj_platform::i18n::msg::browse_library_genre_unwatched(genre),
        };
        CString::new(label).unwrap_or_default()
    }

    /// The count before the controls: the titles on screen and the listing's total, Litefin's
    /// `ListPaging`.
    pub(super) fn count_text<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> CString {
        let total = H::listing(cx).total().max(0);
        let cols = self.layout.cols();
        let (lo, hi) = self.layout.visible_rows(self.scroll.pos);
        let on_screen = |row: usize| {
            let y = self.layout.row_y(row, self.scroll.pos);
            y + self.layout.card_h() > 0.0 && y < SCR_H
        };
        let top = (lo..hi).find(|&row| on_screen(row)).unwrap_or(lo);
        let bottom = (top..hi).rfind(|&row| on_screen(row)).unwrap_or(top);
        let first = (top * cols + 1).min(total as usize);
        let last = ((bottom + 1) * cols).min(total as usize);
        CString::new(nj_platform::i18n::msg::browse_litefin_paging(first as i64, last as i64, total)).unwrap_or_default()
    }

    pub(super) fn count_w<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> f32 {
        if H::listing(cx).total() <= 0 { return 0.0; }
        cx.measure.width(&self.count_text(cx), COUNT_SIZE, false)
    }

    pub(super) fn count_rung() -> i32 { COUNT_SIZE }

    /// The pop slot of a head pill.
    pub(super) fn head_pop_slot(&self, elem: u32) -> Option<usize> {
        if let Some(i) = elem.checked_sub(TAB_BASE).filter(|&i| (i as usize) < TAB_POPS) {
            return Some(i as usize);
        }
        self.toolbar_elems().iter().position(|key| *key == elem).map(|i| TAB_POPS + i)
    }

    fn head_scale<H: LibraryLike>(&self, elem: u32, cx: &Cx<'_, H>, at: At) -> f32 {
        let focused = cx.focus.current.is_some_and(|key| key.entry == self.entry && key.elem == elem);
        match at {
            At::Drawn => self.head_pop_slot(elem).map_or(1.0, |slot| self.head_pop.scale_with(slot, cx.press.scale)),
            At::SpringTarget => if focused { crate::ui::widgets::CTRL_FOCUS_SCALE } else { 1.0 },
        }
    }

    /// A tab pill's resting rect, and the scale it draws at.
    pub(super) fn tab_rect<H: LibraryLike>(&self, elem: u32, cx: &Cx<'_, H>, at: At) -> Option<(Rect, f32)> {
        let tabs = self.tabs();
        let index = elem.checked_sub(TAB_BASE).map(|i| i as usize).filter(|&i| i < tabs.len())?;
        let widths: Vec<_> = tabs.iter().map(|&tab| self.tab_pill(tab, cx).width(cx.measure)).collect();
        let (_, spans) = centred(0.0, &widths);
        let (layout, scroll) = match at { At::Drawn => (&self.layout, self.scroll.pos), At::SpringTarget => (&self.target_layout, self.scroll_target) };
        let (x, w) = spans[index];
        Some((Rect::new(x, layout.tabs_y(scroll), w, TAB_H), self.head_scale(elem, cx, at)))
    }

    /// A control pill's resting rect, and the scale it draws at.
    pub(super) fn control_rect<H: LibraryLike>(&self, elem: u32, cx: &Cx<'_, H>, at: At) -> Option<(Rect, f32)> {
        let elems = self.toolbar_elems();
        let index = elems.iter().position(|key| *key == elem)?;
        let widths: Vec<_> = elems.iter().map(|&key| self.control_pill(key, cx).width(cx.measure)).collect();
        let (_, spans) = centred(self.count_w(cx), &widths);
        let (layout, scroll) = match at { At::Drawn => (&self.layout, self.scroll.pos), At::SpringTarget => (&self.target_layout, self.scroll_target) };
        let (x, w) = spans[index];
        Some((Rect::new(x, layout.controls_y(scroll), w, CONTROL_H), self.head_scale(elem, cx, at)))
    }

    /// The count's left edge in the controls row.
    pub(super) fn count_x<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> f32 {
        let widths: Vec<_> = self.toolbar_elems().iter().map(|&key| self.control_pill(key, cx).width(cx.measure)).collect();
        centred(self.count_w(cx), &widths).0
    }

    /// The rect a menu anchors to and the focus engine places: the pill as it draws.
    pub(super) fn toolbar_chip_rect<H: LibraryLike>(&self, elem: u32, cx: &Cx<'_, H>, at: At) -> Rect {
        self.control_rect(elem, cx, at).map_or(Rect::new(0.0, 0.0, 0.0, 0.0), |(rect, scale)| rect.scaled(scale))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pill_row_is_centred_on_the_page_beside_the_rail_with_its_lead_first() {
        let (start, spans) = centred(0.0, &[100.0, 200.0]);
        assert_eq!(start, spans[0].0);
        let right = spans[1].0 + spans[1].1;
        assert!((start + right - 2.0 * CENTRE_X).abs() < 0.01, "the row is centred on {CENTRE_X}");
        assert_eq!(spans[1].0 - (spans[0].0 + spans[0].1), PILL_GAP);
        let (lead, led) = centred(80.0, &[100.0, 200.0]);
        assert_eq!(led[0].0 - lead, 80.0 + PILL_GAP, "the count stands a gap before the first pill");
        assert!((lead + led[1].0 + led[1].1 - 2.0 * CENTRE_X).abs() < 0.01);
    }
}
