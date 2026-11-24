//! Pure geometry for the owned Library's single vertical document: Litefin's library page
//! (`docs/litefin-look.md` 6) - a title and the tab pills at the head, the controls row and the
//! alphabet bar over a poster grid, or the genre rows / one genre's grid in their place.

use crate::ui::consts::{CONTENT_LEFT, MARGIN_X, MARGIN_Y, SCR_H, SCR_W};
use crate::ui::card_row::RowStyle;

/// Genre rows are the Library's shelves in this fork (`browse::section_hubs`), and a Jellyfin
/// library lists about twenty genres.
pub(super) const MAX_SHELVES: usize = 24;
pub(super) use crate::ui::poster_grid::{GridBand, MAX_GRID_BANDS};
/// The page's padding: `.library-header` 40 50 0 and `.library-content` 20 50 40 50 from the
/// rail's edge, so the title and the first card's box stand at x=150 (6 Header, Grid). The head
/// stops at the overscan frame instead of Litefin's 50 and 40 (`consts::SAFE`): it ends at its
/// right keep-out and starts at its top one.
pub(super) const LEFT: f32 = CONTENT_LEFT + 50.0;
pub(super) const RIGHT: f32 = SCR_W - MARGIN_X;
/// The cards' pictures: the first column's [`CARD_INSET`](crate::ui::consts::CARD_INSET) inside
/// its box at [`LEFT`] (`poster_grid::LEFT`), the last one's right edge as far from the screen's
/// edge, so the grid spans Litefin's content box (6 Grid) and its seven columns are the shared
/// poster grid's.
pub(super) const CARD_LEFT: f32 = crate::ui::poster_grid::LEFT;
pub(super) const CARD_RIGHT: f32 = SCR_W - (CARD_LEFT - CONTENT_LEFT);
/// A grid row's air under its label block: the shared poster grid's (the 20 box gap, 6 Grid).
const ROW_AIR: f32 = crate::ui::poster_grid::ROW_PITCH - crate::ui::poster_grid::CARD_H
    - crate::ui::card_row::LABEL_BAND_COLLAPSED;
/// Where the document starts: the header's top padding, at the overscan frame.
pub(super) const CONTENT_TOP: f32 = MARGIN_Y;
/// The centre line the tabs, the controls and the alphabet bar are centred on: the middle of
/// the page beside the rail (6: "centred row").
pub(super) const CENTRE_X: f32 = (CONTENT_LEFT + SCR_W) * 0.5;
/// The title's line box, 32px at line-height 1.2.
pub(super) const TITLE_H: f32 = 38.0;
/// The tab pills' top below the title's, and their height: padding 12 over a 20px label at
/// line-height 1.5, inside a 2px border (6 Tabs; `library.png` puts them at y=115).
pub(super) const TABS_DY: f32 = 75.0;
pub(super) const TAB_H: f32 = 58.0;
/// The head band: title, tabs, and the air down to the next block (`library.png`: the controls
/// row's top at y=200).
pub(super) const TOP_H: f32 = 160.0;
/// One genre's head: the title alone, with the air its grid keeps below it (`genres.png`: the
/// first card's top at y=118).
pub(super) const DRILL_TOP_H: f32 = 78.0;
/// The controls row: padding 12 over an 18px label at line-height 1.5, inside a 2px border.
pub(super) const CONTROL_H: f32 = 55.0;
/// The alphabet bar's top below the controls row's, and the bar itself: 38px letters 8 apart in
/// a pill padded 12x24 (6 Alphabet bar; `library.png` puts it at y=285).
pub(super) const ALPHA_DY: f32 = 85.0;
pub(super) const LETTER: f32 = 38.0;
pub(super) const LETTER_GAP: f32 = 8.0;
pub(super) const ALPHA_PAD_X: f32 = 24.0;
pub(super) const ALPHA_PAD_Y: f32 = 12.0;
pub(super) const ALPHA_H: f32 = LETTER + 2.0 * ALPHA_PAD_Y;
/// Litefin's alphabet, the bar's fixed letters (`LibraryPage.js` `alphaPickerChars`).
pub(super) const ALPHABET: &str = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// The controls row and the alphabet bar above the grid, down to the grid's first row
/// (`library.png`: the first poster's top at y=393).
pub(super) const GRID_HEAD_H: f32 = 193.0;
/// Thumb view: four landscape stills a row (6 Grid, `_getDefaultColumnsForMode`).
const EPISODE_COLS: usize = 4;
/// One genre's grid: card boxes of 240 (the Home poster's picture, `RowStyle::HOME`) at a 272
/// pitch (6 Genre drill-in grid).
const DRILL_PITCH: f32 = 272.0;

/// The alphabet bar's pill, `(x, w)`, centred under the controls.
pub(super) fn alpha_bar(letters: usize) -> (f32, f32) {
    let w = letters as f32 * LETTER + letters.saturating_sub(1) as f32 * LETTER_GAP + 2.0 * ALPHA_PAD_X;
    (CENTRE_X - w * 0.5, w)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Block {
    Tabs,
    Shelf(usize),
    Toolbar,
    Grid(usize),
    Status,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub shelves: usize,
    pub rows: usize,
    pub grid_head: bool,
    pub status: bool,
    /// An empty answer under the heading row: the document keeps one grid pitch below the
    /// heading for its read-out, so the read-out scrolls with the page and never lands on a shelf.
    pub empty: bool,
    episodes: bool,
    /// One genre's grid: the title alone at the head, Litefin's genre columns.
    drill: bool,
    pitches: [f32; MAX_SHELVES],
    grid_bands: [GridBand; MAX_GRID_BANDS],
}

impl Layout {
    pub(super) const SHAPE: &'static str = "LibraryLayout{shelves:u32,rows:u32,grid_head:bool,status:bool,empty:bool,episodes:bool,drill:bool,pitches:[f32;24],grid_bands:[(row:u32,expansion:f32)]}";

    pub(super) fn write(&self, c: &mut nj_machine::machine::Canon) {
        let Self { shelves, rows, grid_head, status, empty, episodes, drill, pitches, grid_bands } = self;
        c.u32(*shelves as u32).u32(*rows as u32).bool(*grid_head).bool(*status).bool(*empty).bool(*episodes).bool(*drill);
        for pitch in pitches { c.f32(*pitch); }
        c.seq(grid_bands.iter().filter(|band| band.row != usize::MAX).count());
        for band in grid_bands.iter().filter(|band| band.row != usize::MAX) {
            c.u32(band.row as u32).f32(band.expansion);
        }
    }

    pub(super) fn new(shelf_pitches: &[f32], rows: usize, grid_head: bool) -> Self {
        let mut pitches = [crate::ui::consts::ROW_PITCH; MAX_SHELVES];
        for (to, from) in pitches.iter_mut().zip(shelf_pitches.iter().take(MAX_SHELVES)) {
            *to = *from;
        }
        Self {
            shelves: shelf_pitches.len().min(MAX_SHELVES),
            rows,
            grid_head,
            status: false,
            empty: false,
            episodes: false,
            drill: false,
            pitches,
            grid_bands: [GridBand::CLOSED; MAX_GRID_BANDS],
        }
    }

    pub(super) fn failed(shelf_pitches: &[f32]) -> Self {
        Self { status: true, ..Self::new(shelf_pitches, 0, false) }
    }

    pub(super) fn with_episodes(self, episodes: bool) -> Self { Self { episodes, ..self } }

    pub(super) fn with_drill(self, drill: bool) -> Self { Self { drill, ..self } }

    pub(super) fn with_grid_bands(mut self, bands: [GridBand; MAX_GRID_BANDS]) -> Self {
        self.grid_bands = bands;
        self
    }

    pub(super) fn with_grid_focus(mut self, row: Option<usize>) -> Self {
        self.grid_bands = [GridBand::CLOSED; MAX_GRID_BANDS];
        self.grid_bands[..1].copy_from_slice(&crate::ui::poster_grid::settled(row.filter(|&row| row < self.rows)));
        self
    }

    #[cfg(test)]
    pub(super) fn row_expansion(&self, row: usize) -> f32 {
        self.grid_bands.iter().find(|band| band.row == row).map_or(0.0, |band| band.expansion)
    }

    fn band_growth_before(&self, row: usize) -> f32 {
        crate::ui::poster_grid::growth_before(row.min(self.rows), &self.grid_bands)
    }

    fn row_top(&self, row: usize) -> f32 {
        self.grid_top() + row as f32 * self.grid_pitch() + self.band_growth_before(row)
    }

    pub(super) fn is_episodes(&self) -> bool { self.episodes }

    /// The tab pills head the page everywhere but inside one genre.
    pub(super) fn tabs(&self) -> bool { !self.drill }

    pub(super) fn cols(&self) -> usize {
        if self.episodes { EPISODE_COLS }
        else if self.drill { ((CARD_RIGHT - CARD_LEFT + DRILL_PITCH - RowStyle::HOME.w) / DRILL_PITCH) as usize }
        else { crate::ui::poster_grid::COLS }
    }

    /// The grid's card: the shared poster grid's (`(1720 + 20) / 7 - 20` boxes); four stills a
    /// row across the same band for episodes, at its gap; the Home poster at a 272 pitch inside one
    /// genre. Hit boxes, the scroll window and the draws all read this, so they describe one card.
    pub(super) fn style(&self) -> RowStyle {
        let style = if self.episodes {
            let gap = crate::ui::poster_grid::GAP;
            let w = (CARD_RIGHT - CARD_LEFT - (EPISODE_COLS - 1) as f32 * gap) / EPISODE_COLS as f32;
            RowStyle { w, h: w * RowStyle::EPISODE.h / RowStyle::EPISODE.w, gap, ..RowStyle::EPISODE }
        } else if self.drill {
            RowStyle { gap: DRILL_PITCH - RowStyle::HOME.w, ..RowStyle::HOME }
        } else {
            crate::ui::poster_grid::STYLE
        };
        RowStyle { margin_x: CARD_LEFT, ..style }.with_right_reserve(SCR_W - CARD_RIGHT)
    }

    pub(super) fn card_w(&self) -> f32 { self.style().w }

    pub(super) fn card_h(&self) -> f32 { self.style().h }

    pub(super) fn grid_pitch(&self) -> f32 {
        self.card_h() + crate::ui::card_row::LABEL_BAND_COLLAPSED + ROW_AIR
    }

    pub(super) fn cell_x(&self, col: usize) -> f32 {
        let style = self.style();
        CARD_LEFT + col as f32 * (style.w + style.gap)
    }

    /// The grid's right edge: the content band's, or the last genre column's.
    pub(super) fn grid_right(&self) -> f32 {
        self.cell_x(self.cols() - 1) + self.card_w()
    }

    pub(super) fn top_h(&self) -> f32 { if self.drill { DRILL_TOP_H } else { TOP_H } }

    /// The title's top on screen, the document's own top; the tabs hang [`TABS_DY`] under it.
    pub(super) fn title_y(&self, scroll: f32) -> f32 { CONTENT_TOP - scroll }

    pub(super) fn tabs_y(&self, scroll: f32) -> f32 { self.title_y(scroll) + TABS_DY }

    pub(super) fn shelf_pitch(&self, index: usize) -> f32 {
        self.pitches.get(index).copied().unwrap_or(crate::ui::consts::ROW_PITCH)
    }

    /// A shelf hangs its heading [`TITLE_DY`](crate::ui::consts::TITLE_DY) above its origin, so
    /// the run of shelves starts that far under the head band.
    pub(super) fn shelf_origin(&self, index: usize) -> f32 {
        let lead = if self.shelves > 0 { crate::ui::consts::TITLE_DY } else { 0.0 };
        self.top_h() + lead + self.pitches[..index.min(self.shelves)].iter().sum::<f32>()
    }

    pub(super) fn grid_block_top(&self) -> f32 {
        self.shelf_origin(self.shelves)
    }

    pub(super) fn grid_top(&self) -> f32 {
        self.grid_block_top() + if self.grid_head { GRID_HEAD_H } else { 0.0 }
    }

    pub(super) fn controls_y(&self, scroll: f32) -> f32 { CONTENT_TOP + self.grid_block_top() - scroll }

    pub(super) fn alpha_y(&self, scroll: f32) -> f32 { self.controls_y(scroll) + ALPHA_DY }

    pub(super) fn row_y(&self, row: usize, scroll: f32) -> f32 {
        CONTENT_TOP + self.row_top(row) - scroll
    }

    pub(super) fn shelf_y(&self, shelf: usize, scroll: f32) -> f32 {
        CONTENT_TOP + self.shelf_origin(shelf) - scroll
    }

    pub(super) fn doc_to_grid(&self, scroll: f32) -> f32 { scroll - self.grid_top() }

    pub(super) fn doc_h(&self) -> f32 { self.row_top(self.rows) + self.empty_band().map_or(0.0, |(_, h)| h) }

    /// The document band an empty answer's read-out stands in — `(top, height)` in document
    /// space, directly under the heading row — when the layout reserves one.
    pub(super) fn empty_band(&self) -> Option<(f32, f32)> {
        (self.empty && self.grid_head && self.rows == 0).then(|| (self.grid_top(), self.grid_pitch()))
    }

    pub(super) fn max_scroll(&self) -> f32 {
        (self.doc_h() - (SCR_H - CONTENT_TOP) + MARGIN_Y).max(0.0)
    }

    /// Row 0 keeps the head above it on screen - the whole page head, or the controls and the
    /// alphabet under a run of shelves; every other row snaps its own top to the content top,
    /// Litefin's 40 below the panel's edge (2.4 Vertical scroll focus offset).
    pub(super) fn row_reveal(&self, row: usize) -> f32 {
        let top = if row > 0 { self.row_top(row) } else if self.shelves == 0 { 0.0 } else { self.grid_block_top() };
        top.clamp(0.0, self.max_scroll())
    }

    /// The first shelf keeps the head on screen; later ones snap their heading to the top.
    pub(super) fn shelf_reveal(&self, shelf: usize) -> f32 {
        if shelf == 0 { return 0.0; }
        (self.shelf_origin(shelf) - crate::ui::consts::TITLE_DY).clamp(0.0, self.max_scroll())
    }

    /// The document's top block: where the page starts and BACK returns to.
    pub(super) fn first(&self) -> Option<Block> {
        if self.tabs() { Some(Block::Tabs) } else { self.seat() }
    }

    /// The block the page seats its own focus in: the first block that holds content, else the
    /// controls an empty listing keeps, else the tabs (Litefin starts in the grid when it has
    /// items and falls back to the tabs).
    pub(super) fn seat(&self) -> Option<Block> {
        if self.shelves > 0 { Some(Block::Shelf(0)) }
        else if self.status { Some(Block::Status) }
        else if self.rows > 0 { Some(Block::Grid(0)) }
        else if self.grid_head { Some(Block::Toolbar) }
        else if self.tabs() { Some(Block::Tabs) }
        else { None }
    }

    pub(super) fn seat_for_scroll(&self, scroll: f32, saved_row: usize) -> Option<Block> {
        let first = self.seat()?;
        if scroll <= 0.5 { return Some(first); }
        let mut best = (f32::INFINITY, first);
        for shelf in 0..self.shelves {
            let d = (self.shelf_reveal(shelf) - scroll).abs();
            if d < best.0 { best = (d, Block::Shelf(shelf)); }
        }
        if self.grid_head || self.drill {
            for row in [saved_row.min(self.rows.saturating_sub(1)), 0] {
                let d = (self.row_reveal(row) - scroll).abs();
                if d < best.0 {
                    best = (d, if self.rows > 0 { Block::Grid(row) } else { Block::Toolbar });
                }
            }
        }
        Some(best.1)
    }

    /// Grid rows one screenful spans below the content top: a row revealed at the top of the
    /// viewport leaves every row at least this far below it undrawn. The grid head band only
    /// pushes rows further down, so the document's head shows no more than this either.
    #[cfg(any(test, feature = "devtriggers"))]
    pub(super) fn rows_per_screen(&self) -> usize {
        ((SCR_H - CONTENT_TOP) / self.grid_pitch()).ceil() as usize
    }

    pub(super) fn visible_rows(&self, scroll: f32) -> (usize, usize) {
        if self.rows == 0 { return (0, 0); }
        let local = self.doc_to_grid(scroll);
        let pitch = self.grid_pitch();
        // Prefix growth is nonnegative and at most the sparse bands' total. This conservative
        // inverse retains only nearby rows without scanning a catalog or binary-searching it.
        let growth = self.band_growth_before(self.rows);
        let lo = ((local - self.card_h() - growth) / pitch).floor().max(0.0) as usize;
        let hi = (((local + SCR_H - CONTENT_TOP) / pitch).ceil().max(0.0) as usize + 1).min(self.rows);
        (lo.min(hi), hi)
    }

}

pub(super) fn shelf_pitch(landscape: bool, expanded: f32) -> f32 {
    let art = if landscape { crate::ui::card_row::RowStyle::EPISODE.h } else { crate::ui::consts::CARD_H };
    crate::ui::consts::TITLE_DY + crate::ui::consts::CARD_DY + art
        + crate::ui::card_row::under_band(expanded)
        + crate::ui::consts::UNDER_LABEL_AIR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_rows_without_focus_use_the_shared_collapsed_band() {
        for episodes in [false, true] {
            let layout = Layout::new(&[], 10_000, true).with_episodes(episodes);
            let pitch = layout.card_h() + crate::ui::card_row::under_band(0.0) + ROW_AIR;
            assert!((layout.row_y(1, 0.0) - layout.row_y(0, 0.0) - pitch).abs() < 0.001);
        }
    }

    #[test]
    fn only_the_focused_all_row_reserves_its_caption_and_the_last_caption_fits() {
        use crate::ui::card_row::{BAND_OPEN, UNDER_LABEL_H};
        for episodes in [false, true] {
            let compact = Layout::new(&[shelf_pitch(false, 0.0)], 40, true)
                .with_episodes(episodes);
            for focused in [0, 3, 39] {
                let layout = compact.with_grid_focus(Some(focused));
                assert!((layout.doc_h() - compact.doc_h() - BAND_OPEN).abs() < 0.002);
                for row in [0, 3, 39] {
                    let growth = if row > focused { BAND_OPEN } else { 0.0 };
                    assert!((layout.row_y(row, 0.0) - compact.row_y(row, 0.0) - growth).abs() < 0.002);
                    assert_eq!(layout.row_expansion(row), f32::from(row == focused));
                }
                let scroll = layout.row_reveal(focused);
                let (lo, hi) = layout.visible_rows(scroll);
                assert!((lo..hi).contains(&focused));
                assert!(hi - lo < 8, "paging remains bounded for both card shapes");
                assert!(layout.row_y(focused, scroll) + layout.card_h() + UNDER_LABEL_H
                    <= SCR_H - MARGIN_Y + 0.01);
            }
        }
    }

    fn posters(shelves: usize, rows: usize, grid_head: bool) -> Layout {
        Layout::new(&vec![crate::ui::consts::ROW_PITCH; shelves], rows, grid_head)
    }

    #[test]
    fn the_document_stacks_the_head_shelves_then_the_grid() {
        use crate::ui::consts::TITLE_DY;
        let bare = posters(0, 40, true);
        assert_eq!(bare.grid_block_top(), TOP_H, "no shelves: the controls follow the head");
        assert_eq!(bare.grid_top(), TOP_H + GRID_HEAD_H, "...and the grid follows the controls and the alphabet");

        let shelved = posters(12, 40, true);
        assert_eq!(
            shelved.grid_block_top(),
            TOP_H + TITLE_DY + 12.0 * crate::ui::consts::ROW_PITCH,
            "twelve shelves push the grid exactly twelve pitches down"
        );
        // ...and a shelf's own origin is the one Home hangs a shelf from, so `card_row` draws here
        // unchanged: heading at origin - TITLE_DY, cards at origin + CARD_DY, clear of the tabs
        assert_eq!(shelved.shelf_origin(0), TOP_H + TITLE_DY);
        assert!(shelved.shelf_origin(0) - TITLE_DY >= TABS_DY + TAB_H);
        assert_eq!(
            shelved.shelf_origin(3) - shelved.shelf_origin(2),
            crate::ui::consts::ROW_PITCH
        );

        // a grid with nothing in it draws no controls and no alphabet, so the block IS the grid
        let empty = posters(2, 0, false);
        assert_eq!(empty.grid_top(), empty.grid_block_top());
        // one genre's grid stands under its title alone
        let drill = posters(0, 40, false).with_drill(true);
        assert_eq!(drill.grid_top(), DRILL_TOP_H);
        assert!(!drill.tabs());
    }

    /// The head's measured Litefin geometry (`library.png`), from the overscan frame's top in
    /// place of Litefin's 40: tabs 75 under the title, the controls 160, the alphabet 245 and the
    /// first poster 353; the first card's box at x=150, its picture inset in it.
    #[test]
    fn the_head_lands_on_litefins_library_lines() {
        let lay = posters(0, 40, true);
        assert_eq!(lay.title_y(0.0), MARGIN_Y);
        assert_eq!(lay.tabs_y(0.0) - lay.title_y(0.0), 75.0);
        assert_eq!(lay.controls_y(0.0) - lay.title_y(0.0), 160.0);
        assert_eq!(lay.alpha_y(0.0) - lay.title_y(0.0), 245.0);
        assert_eq!(lay.row_y(0, 0.0) - lay.title_y(0.0), 353.0);
        assert_eq!(lay.cell_x(0), 150.0 + crate::ui::consts::CARD_INSET);
        assert!(lay.alpha_y(0.0) + ALPHA_H < lay.row_y(0, 0.0));
        let (x, w) = alpha_bar(ALPHABET.len());
        assert!((x + w * 0.5 - CENTRE_X).abs() < 0.01);
        assert!(x >= LEFT && x + w <= RIGHT, "the 27-letter bar fits the content band");
    }

    /// **`doc_to_grid` is the fix for this screen's sharpest geometry bug.** `browse::want` derived
    /// its page window straight from `SCROLL`, i.e. assuming scroll 0 is grid row 0 — so under a
    /// document with a header and twelve shelves it asked for pages thirteen rows into the catalog
    /// while row 0's own slots sat unloaded.
    #[test]
    fn the_page_window_is_grid_local_whatever_is_above_the_grid() {
        let rows = 1667; // a 10k-item section
        let flat = posters(0, rows, true);
        let deep = posters(12, rows, true);

        // at each document's own head, both ask for row 0 — the bug was that only the flat one did
        assert_eq!(flat.visible_rows(0.0).0, 0);
        assert_eq!(deep.visible_rows(0.0).0, 0);
        assert_eq!(
            deep.visible_rows(deep.grid_top()).0,
            0,
            "scrolled exactly to the grid's top, the first wanted row is still 0"
        );
        // …and one pitch further down, both have moved by exactly one row
        assert_eq!(
            deep.visible_rows(deep.grid_top() + deep.grid_pitch()).0,
            flat.visible_rows(flat.grid_top() + flat.grid_pitch()).0
        );
        // the window never runs past the catalog
        assert!(deep.visible_rows(1.0e9).1 <= rows);
    }

    /// **`max_scroll` and `row_reveal` are ONE invariant**, and this is the test that holds them to
    /// it: if they disagree the last row's caption sits off the panel, which is the bug the old
    /// hand-written `max_y`/`lo` pair carried a paragraph about.
    #[test]
    fn the_last_row_can_always_reach_its_own_caption() {
        for shelves in [0usize, 1, 12] {
            for rows in [1usize, 2, 40, 1667] {
                let lay = posters(shelves, rows, true);
                let last = lay.row_reveal(rows - 1);
                assert!(
                    last <= lay.max_scroll() + 0.001,
                    "row_reveal must never ask past max_scroll ({shelves} shelves, {rows} rows)"
                );
                // the whole last row — card, label band and the air under it — is on the panel
                let bottom = CONTENT_TOP + lay.doc_h() - last;
                assert!(
                    bottom <= SCR_H + 0.001,
                    "the last row's caption is off the panel ({shelves} shelves, {rows} rows): {bottom}"
                );
            }
        }
    }

    /// Row snapping puts the focused row's OWN TOP EDGE at the viewport, clamped — not the minimal
    /// reveal `card_row::reveal` performs, which is right for a shelf inside a page and wrong for a
    /// wall of posters you walk down.
    #[test]
    fn row_snapping_targets_the_rows_own_top_edge() {
        let lay = posters(0, 40, true);
        // **Row 0 is the canvas's own exception**: its target keeps the head on screen - the
        // title, the tabs, the controls and the alphabet directly above it. Snapping past them
        // would scroll away the count and the controls the moment focus entered the grid.
        assert_eq!(lay.row_reveal(0), 0.0);
        let shelved = posters(2, 40, true);
        assert_eq!(shelved.row_reveal(0), shelved.grid_block_top(), "under shelves, the controls lead");
        assert_ne!(
            lay.row_reveal(0),
            lay.grid_top(),
            "…which is NOT the row's own top edge — every other row is"
        );
        assert_eq!(lay.row_reveal(3), lay.grid_top() + 3.0 * lay.grid_pitch());
        // …and the clamp is the document's, so the last rows share one resting scroll
        assert_eq!(lay.row_reveal(39), lay.max_scroll());
    }

    // ---- the focus projection ----------------------------------------------------------------

    #[test]
    fn the_poster_grid_fills_litefins_content_band() {
        let layout = posters(0, 40, true);
        assert_eq!(layout.cols(), crate::ui::poster_grid::COLS);
        assert_eq!(layout.cell_x(0), CARD_LEFT);
        assert!((layout.grid_right() - CARD_RIGHT).abs() < 0.01);
        assert!((layout.cell_x(1) - layout.cell_x(0) - layout.card_w() - crate::ui::poster_grid::GAP).abs() < 0.01);
        assert_eq!((layout.card_w(), layout.card_h()), (crate::ui::poster_grid::CARD_W, crate::ui::poster_grid::CARD_H));
        assert_eq!(layout.grid_pitch(), crate::ui::poster_grid::ROW_PITCH, "rows stand the shared grid's 20 apart");
    }

    #[test]
    fn a_genres_grid_keeps_litefins_240_columns_at_a_272_pitch() {
        let lay = posters(0, 40, false).with_drill(true);
        assert_eq!(lay.card_w() + 2.0 * crate::ui::consts::CARD_INSET, 240.0, "a 240 card box");
        assert_eq!(lay.cell_x(1) - lay.cell_x(0), 272.0);
        assert_eq!(lay.cols(), 6);
        assert!(lay.grid_right() <= CARD_RIGHT);
        assert_eq!(lay.row_reveal(0), 0.0, "row 0 keeps the genre's title on screen");
    }

    #[test]
    fn episode_grid_fits_four_stills_across_the_band_and_reveals_the_last_caption() {
        let lay = posters(2, 40, true).with_episodes(true);
        assert_eq!(lay.cols(), 4);
        assert_eq!(lay.cell_x(0), CARD_LEFT);
        assert!((lay.grid_right() - CARD_RIGHT).abs() < 0.01);
        assert!((lay.card_w() / lay.card_h() - RowStyle::EPISODE.w / RowStyle::EPISODE.h).abs() < 0.001);
        assert_eq!(lay.row_reveal(8), lay.grid_top() + 8.0 * lay.grid_pitch());
        let last = lay.row_reveal(39);
        assert!(lay.row_y(39, last) + lay.grid_pitch() <= SCR_H - MARGIN_Y + 0.01);
        let (lo, hi) = lay.visible_rows(lay.row_reveal(8));
        assert!(lo <= 8 && hi > 8);
        assert!(hi - lo < 8, "the episode page window stays bounded: {lo}..{hi}");
    }

    #[test]
    fn document_stacks_shelves_of_both_shapes_and_the_grid() {
        let portrait = shelf_pitch(false, 1.0);
        let landscape = shelf_pitch(true, 1.0);
        let lay = Layout::new(&[portrait, landscape], 10, true);
        let first = TOP_H + crate::ui::consts::TITLE_DY;
        assert_eq!(lay.shelf_origin(0), first);
        assert_eq!(lay.shelf_origin(1), first + portrait);
        assert_eq!(lay.grid_block_top(), first + portrait + landscape);
        assert_eq!(lay.grid_top(), lay.grid_block_top() + GRID_HEAD_H);
        assert!(landscape < portrait);
    }

    #[test]
    fn page_window_is_grid_local_with_a_large_header() {
        let pitches = [shelf_pitch(false, 1.0); MAX_SHELVES];
        let lay = Layout::new(&pitches, 80, true);
        assert_eq!(lay.visible_rows(0.0), (0, 1), "the legacy window prefetches row zero above the grid");
        let at_grid = lay.row_reveal(8);
        let (lo, hi) = lay.visible_rows(at_grid);
        assert!(lo <= 8 && hi > 8, "{lo}..{hi}");
    }

    #[test]
    fn rows_per_screen_bounds_what_a_revealed_row_leaves_on_screen() {
        // The poster-gate scenes derive their targets from this: a row this far below the
        // revealed one (or below the document's head) must not be on screen at all.
        let lay = posters(2, 40, true);
        let per = lay.rows_per_screen();
        assert!(per >= 2, "{per}");
        for row in [0, 8, 20] {
            let scroll = lay.row_reveal(row);
            assert!(lay.row_y(row + per, scroll) >= SCR_H, "row {row}: +{per} is on screen");
            // Tight, not merely safe: below the head band a full screenful really is this many.
            assert!(row == 0 || lay.row_y(row + per - 1, scroll) < SCR_H, "row {row}: +{} is off screen", per - 1);
        }
    }

    #[test]
    fn last_row_caption_settles_inside_overscan() {
        let lay = Layout::new(&[], 17, true);
        let scroll = lay.row_reveal(16);
        let bottom = lay.row_y(16, scroll) + lay.grid_pitch();
        assert!(bottom <= SCR_H - MARGIN_Y + 0.01, "{bottom}");
    }

    #[test]
    fn restored_scroll_seats_the_block_it_displays() {
        let lay = Layout::new(&[500.0, 360.0], 20, true);
        assert_eq!(lay.seat_for_scroll(0.0, 12), Some(Block::Shelf(0)));
        assert_eq!(lay.seat_for_scroll(lay.shelf_reveal(1), 12), Some(Block::Shelf(1)));
        assert_eq!(lay.seat_for_scroll(lay.row_reveal(12), 12), Some(Block::Grid(12)));
    }
}
