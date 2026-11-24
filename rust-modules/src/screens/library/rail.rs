//! Litefin's alphabet bar (`docs/litefin-look.md` 6): one centred pill of `#` and A..Z between
//! the controls and the grid, owned by MasterDetail as the master. Only Input owns its focused or
//! remembered letter; OK on a letter jumps the grid to its first title.
use std::ffi::CString;
use super::identity::KeyRegistry;
use super::layout::{alpha_bar, ALPHABET, ALPHA_H, ALPHA_PAD_X, ALPHA_PAD_Y, LETTER, LETTER_GAP};
use crate::screens::registry::{LibraryIdentity, LibraryLike, LibrarySectionIdentity};
use crate::ui::frame::Budget;
use nj_machine::machine::{Canon, Cx, EntryId, FocusKey, GroupId};
use crate::ui::screen::{Activate, At, AxisMask, Dir, DrawFrame, EdgeRule, ElemKind, Focusable,
    GroupKind, GroupSpec, Hover, Part, Placed, Seat, Step, Stop};
use crate::ui::{Rect, theme};

/// A focused or in-view letter pops 1.2 (6 Alphabet bar): the 18px letter is drawn on the
/// 22px rung and its disc grows with it.
const LETTER_POP: f32 = 1.2;

pub(super) struct RailPart {
    entry: EntryId,
    group: GroupId,
    pub(super) elems: Vec<u32>,
    labels: Vec<CString>,
    starts: Vec<usize>,
    /// The bar as drawn this frame, and its top at the page's spring target.
    rect: Rect,
    target_y: f32,
    /// The page shows the bar: a grid under its controls, sorted by title.
    shown: bool,
}

/// Each bar letter's first grid index: the first title under that letter or the next letter the
/// listing has, else the listing's last title. `letters` are the listing's own first characters
/// in its title order, each with the index its titles start at; a character outside the bar files
/// under `#`.
pub(super) fn letter_starts(letters: &[(String, usize)], total: usize) -> Vec<usize> {
    let slot = |label: &str| label.chars().next().map(|c| c.to_ascii_uppercase())
        .and_then(|c| ALPHABET.find(c)).unwrap_or(0);
    (0..ALPHABET.len()).map(|letter| letters.iter().find(|(label, _)| slot(label) >= letter)
        .map_or(total.saturating_sub(1), |(_, start)| *start)).collect()
}

impl RailPart {
    pub(super) const SHAPE: &'static str = "LibraryRail{group:u32,elems:[u32],labels:[str],starts:[u32],rect:{x:f32,y:f32,w:f32,h:f32},target_y:f32,shown:bool}";

    pub(super) fn new(entry: EntryId, group: GroupId) -> Self {
        Self { entry, group, elems: Vec::new(), labels: Vec::new(), starts: Vec::new(),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0), target_y: 0.0, shown: false }
    }

    pub(super) fn clear_projection(&mut self) {
        self.elems.clear(); self.labels.clear(); self.starts.clear();
    }

    pub(super) fn write(&self, c: &mut Canon) {
        let Self { entry: _, group, elems, labels, starts, rect, target_y, shown } = self;
        c.u32(group.0).seq(elems.len());
        for elem in elems { c.u32(*elem); }
        c.seq(labels.len());
        for label in labels { c.str(label.to_str().unwrap_or("")); }
        c.seq(starts.len());
        for index in starts { c.u32(*index as u32); }
        c.f32(rect.x).f32(rect.y).f32(rect.w).f32(rect.h).f32(*target_y).bool(*shown);
    }

    pub(super) fn refresh<H: LibraryLike>(&mut self, cx: &Cx<'_, H>, keys: &mut KeyRegistry) {
        let view = H::listing(cx);
        let Some(id) = view.id() else { self.clear_projection(); return };
        if view.total() <= 0 || !view.rail_available() { self.clear_projection(); return; }
        let letters: Vec<_> = view.letters().iter().enumerate()
            .map(|(index, (label, _))| (label.clone(), view.letter_start(index))).collect();
        self.starts = letter_starts(&letters, view.total() as usize);
        let section = LibrarySectionIdentity { sid: id.sid, key: id.section };
        self.elems.clear(); self.labels.clear();
        for (index, letter) in ALPHABET.chars().enumerate() {
            self.elems.push(keys.register(LibraryIdentity::Rail { section: section.clone(), label: letter.to_string() }, self.group, index));
            self.labels.push(CString::new(letter.to_string()).unwrap_or_default());
        }
    }

    /// Place the bar: its top as drawn and at the spring target, and whether the page shows it.
    pub(super) fn set_geometry(&mut self, y: f32, target_y: f32, shown: bool) {
        let (x, w) = alpha_bar(ALPHABET.len());
        self.rect = Rect::new(x, y, w, ALPHA_H);
        self.target_y = target_y;
        self.shown = shown;
    }

    pub(super) fn eligible<H: LibraryLike>(&self, _cx: &Cx<'_, H>) -> bool { self.shown() }

    /// The bar is on the page: placed under the controls, with letters to show.
    pub(super) fn shown(&self) -> bool { self.shown && !self.elems.is_empty() }

    pub(super) fn letter_for_item(&self, index: usize) -> usize {
        self.starts.iter().enumerate().rev().find(|(_, start)| **start <= index).map(|(i, _)| i).unwrap_or(0)
    }

    pub(super) fn start_for_elem(&self, elem: u32) -> Option<usize> {
        self.elems.iter().position(|key| *key == elem).and_then(|index| self.starts.get(index).copied())
    }

    fn slot(&self, index: usize, at: At) -> Rect {
        let y = match at { At::Drawn => self.rect.y, At::SpringTarget => self.target_y };
        Rect::new(self.rect.x + ALPHA_PAD_X + index as f32 * (LETTER + LETTER_GAP), y + ALPHA_PAD_Y, LETTER, LETTER)
    }

    pub(super) fn record_stops<H: LibraryLike>(&self, f: &mut DrawFrame<'_, '_, H>) {
        if !f.records_stops() || !self.eligible(f.cx) { return; }
        for &elem in &self.elems {
            let Some(placed) = <Self as Focusable<H>>::place(self, &elem, f.cx, At::Drawn) else { continue };
            f.stop(f.painter, Stop { key: FocusKey { entry: self.entry, elem },
                rect: placed.rect, rest_rect: placed.rest_rect, clip: placed.clip,
                hover: Hover::Focus, activate: Activate::Direct });
        }
    }

    /// `current` is the grid index the reader is looking at: its letter wears the accent.
    pub(super) fn draw_with_current<H: LibraryLike>(&self, f: &mut DrawFrame<'_, '_, H>, _frame: Rect, current: Option<usize>) {
        if !self.eligible(f.cx) || !crate::ui::on_axis(self.rect.y, self.rect.h, crate::ui::consts::SCR_H, 0.0) { return; }
        let p = f.painter.alpha(f.page_alpha);
        p.rect(self.rect, theme::browse_lf::ALPHA_BAR_RAD, theme::browse_lf::ALPHA_BAR, theme::browse_lf::ALPHA_BAR, 0.0);
        let current = current.map(|index| self.letter_for_item(index));
        for (index, label) in self.labels.iter().enumerate() {
            let slot = self.slot(index, At::Drawn);
            let focused = f.focus.current == Some(FocusKey { entry: self.entry, elem: self.elems[index] });
            let (size, ink) = if focused {
                let disc = slot.scaled(LETTER_POP);
                p.rect(disc, disc.w * 0.5, theme::BRAND_FILL, theme::BRAND_FILL, 0.0);
                (theme::size::lf::S22, theme::BRAND_INK)
            } else if current == Some(index) {
                (theme::size::lf::S22, theme::BRAND_ACTIVE_INK)
            } else { (theme::size::lf::S18, theme::TEXT_SECONDARY) };
            crate::ui::label::Label::new(label.as_ptr(), size, ink).bold()
                .h(crate::ui::label::HAlign::Center).draw(p, slot);
        }
        self.record_stops(f);
    }
}

impl<H: LibraryLike> Focusable<H> for RailPart {
    fn groups(&self, cx: &Cx<'_, H>, out: &mut Vec<GroupSpec>) {
        if !self.eligible(cx) { return; }
        out.push(GroupSpec { id: self.group, kind: GroupKind::Row { wrap: false }, seat: Seat::First,
            reachable: AxisMask::VERTICAL,
            edge: [EdgeRule::Geometric, EdgeRule::Geometric, EdgeRule::Stop, EdgeRule::Stop],
            extent: Rect::new(self.rect.x, self.target_y, self.rect.w, self.rect.h), len: self.elems.len(), elem: ElemKind::Bare });
    }
    fn group_of(&self, elem: &u32, cx: &Cx<'_, H>) -> Option<GroupId> {
        (self.eligible(cx) && self.elems.contains(elem)).then_some(self.group)
    }
    fn neighbour(&self, key: FocusKey<u32>, dir: Dir, _: &Cx<'_, H>) -> Step<u32> {
        let Some(index) = self.elems.iter().position(|elem| *elem == key.elem) else { return Step::Edge };
        let next = match dir { Dir::Left => index.checked_sub(1),
            Dir::Right => (index + 1 < self.elems.len()).then_some(index + 1), _ => None };
        next.map_or(Step::Edge, |i| Step::Move(FocusKey { entry: self.entry, elem: self.elems[i] }))
    }
    fn place(&self, elem: &u32, cx: &Cx<'_, H>, at: At) -> Option<Placed> {
        if !self.eligible(cx) { return None; }
        let index = self.elems.iter().position(|key| key == elem)?;
        let rect = self.slot(index, at);
        Some(Placed { rect, rest_rect: rect, clip: Rect::FULL, index: Some(index as u32) })
    }
    fn reconcile(&self, want: FocusKey<u32>, _: &Cx<'_, H>) -> FocusKey<u32> {
        if self.elems.contains(&want.elem) { want } else {
            self.elems.first().map(|elem| FocusKey { entry: self.entry, elem: *elem }).unwrap_or(want)
        }
    }
    fn seat(&self, _: GroupId, from: Placed, _: &Cx<'_, H>) -> FocusKey<u32> {
        let index = self.letter_for_item(from.index.unwrap_or(0) as usize);
        FocusKey { entry: self.entry, elem: self.elems.get(index).copied().unwrap_or(0) }
    }
}
impl<H: LibraryLike> Part<H> for RailPart {
    fn prepare(&mut self, _: &mut Budget, _: &Cx<'_, H>) {}
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, H>, frame: Rect) { self.draw_with_current(f, frame, None); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bar_letter_jumps_to_its_own_or_the_next_titles_start() {
        let letters = [("#".to_string(), 0), ("A".into(), 2), ("C".into(), 5), ("z".into(), 6)];
        let starts = letter_starts(&letters, 10);
        assert_eq!(starts.len(), ALPHABET.len());
        let at = |c: char| starts[ALPHABET.find(c).unwrap()];
        assert_eq!(at('#'), 0);
        assert_eq!(at('A'), 2);
        assert_eq!(at('B'), 5, "a letter the listing lacks jumps to the next one it has");
        assert_eq!(at('C'), 5);
        assert_eq!(at('D'), 6);
        assert_eq!(at('Z'), 6, "a lower-case first character files under its capital");
        assert!(starts.windows(2).all(|pair| pair[0] <= pair[1]), "the starts follow the title order");
        assert_eq!(letter_starts(&[("A".into(), 0)], 3)[ALPHABET.find('B').unwrap()], 2,
            "past the last letter the bar jumps to the last title");
        let other = letter_starts(&[("\u{c9}".into(), 0), ("B".into(), 2)], 3);
        assert_eq!(other[0], 0, "a character outside the bar files under #");
    }

    #[test]
    fn the_letter_in_view_is_the_last_letter_starting_at_or_before_the_index() {
        let mut rail = RailPart::new(EntryId(1), GroupId(2));
        rail.starts = letter_starts(&[("A".into(), 0), ("C".into(), 3)], 5);
        assert_eq!(rail.letter_for_item(0), ALPHABET.find('A').unwrap(), "# and A share a start; A is in view");
        assert_eq!(rail.letter_for_item(3), ALPHABET.find('C').unwrap());
    }

    #[test]
    fn the_bar_places_its_letters_at_the_drawn_and_the_target_top() {
        let mut rail = RailPart::new(EntryId(1), GroupId(2));
        rail.set_geometry(285.0, 200.0, true);
        let drawn = rail.slot(1, At::Drawn);
        let target = rail.slot(1, At::SpringTarget);
        assert_eq!(drawn.y, 285.0 + ALPHA_PAD_Y);
        assert_eq!(target.y, 200.0 + ALPHA_PAD_Y);
        assert_eq!(drawn.x - rail.slot(0, At::Drawn).x, LETTER + LETTER_GAP);
        assert_eq!((drawn.w, drawn.h), (LETTER, LETTER));
        let last = rail.slot(ALPHABET.len() - 1, At::Drawn);
        assert!((last.x + last.w + ALPHA_PAD_X - (rail.rect.x + rail.rect.w)).abs() < 0.01);
    }

    #[test]
    fn every_bar_component_enters_the_canonical_state() {
        let hash = |rail: &RailPart| { let mut c = Canon::new(); rail.write(&mut c); c.finish() };
        let initial = hash(&RailPart::new(EntryId(1), GroupId(2)));
        for component in 0..3 {
            let mut rail = RailPart::new(EntryId(1), GroupId(2));
            match component { 0 => rail.set_geometry(1.0, 0.0, false), 1 => rail.target_y = 1.0, _ => rail.shown = true }
            assert_ne!(hash(&rail), initial, "component {component}");
        }
    }
}
