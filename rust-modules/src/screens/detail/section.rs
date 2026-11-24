//! Section identity versus visual order.
//!
//! Lookups and saved-column memory index by [`SectionId`]. Visual order is a separate list.
//! The compact title no longer has named hide ANCHORS — it fades out as whatever section is first
//! in visual order begins to travel (`super::compact_title_alpha`), so adding a section cannot
//! leave a 72px wordmark drawn over it. A section without a
//! slot cannot be stored by accident: the slot index is this enum's discriminant, and
//! [`crate::metadata::SPOT_SECTION_SLOTS`] is that count.

use crate::metadata::SPOT_SECTION_SLOTS;

pub(crate) const SLOTS: usize = SPOT_SECTION_SLOTS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum SectionId {
    Hero = 0,
    Season = 1,
    Episode = 2,
    Related = 3,
    Cast = 4,
    About = 5,
    Extras = 6,
    /// A member movie's collection shelf, drawn directly above Related.
    Collections = 7,
}

const _: () = assert!(SLOTS == (SectionId::Collections as usize) + 1);
/// The hero is slot 0, which is what `DetailScreen::sections` and every `Spot` walk assume.
const _: () = assert!(SectionId::Hero.raw() == 0);

impl SectionId {
    pub(crate) const fn raw(self) -> i32 {
        self as i32
    }

}

/// Litefin's rows below the split (`docs/litefin-look.md` 5, 4.2): titles 32/600 at x=160 (the
/// rail plus the row's 60 of padding) and the first card's box 12 in from there. The shelf styles
/// are the shared Home and episode presets with only the left edge moved, so a card restyle in
/// `ui::card_row` reaches these rows unchanged.
pub(crate) const ROW_X: f32 = crate::ui::detail_layout::SPLIT_X;
pub(crate) const CARD_X: f32 = ROW_X + 12.0;
pub(crate) const POSTERS: crate::ui::card_row::RowStyle =
    crate::ui::card_row::RowStyle { margin_x: CARD_X, ..crate::ui::card_row::RowStyle::HOME };
pub(crate) const STILLS: crate::ui::card_row::RowStyle =
    crate::ui::card_row::RowStyle { margin_x: CARD_X, ..crate::ui::card_row::RowStyle::EPISODE };
/// The row title's size (4.2 Row title: 32/600).
pub(crate) const HEADING_SZ: std::os::raw::c_int = crate::ui::theme::size::lf::S32;

/// A row's title with its cap top at `y`.
pub(crate) fn heading(p: crate::ui::Painter, title: &std::ffi::CStr, y: f32) {
    p.text(title.as_ptr(), ROW_X, y, HEADING_SZ, crate::ui::theme::TEXT_HEADING, 0, 1);
}
