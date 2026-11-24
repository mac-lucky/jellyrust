//! Label placement and landscape assertions, on the owned production helpers.
use crate::catalog_fetch::PmsMovie;
use crate::ui::card_row::{self, RowStyle};
use crate::ui::consts::{self, CARD_H, CARD_W};
use nj_machine::machine::{EntryId, InstanceId};
use crate::ui::{Painter, Rect};
use super::layout::{self, Layout, shelf_pitch, CARD_LEFT, CARD_RIGHT, CONTENT_TOP, LEFT, RIGHT};
use super::COLS;

#[test]
fn a_focused_grid_caption_stays_inside_the_content_band() {
    let p = Painter::root();
    let layout = Layout::new(&[], 40, true);
    let card = |col: usize| Rect::new(layout.cell_x(col), layout.row_y(0, 0.0), layout.card_w(), layout.card_h());
    let style = layout.style();
    let (x, w) = card_row::label_band(p, card(COLS - 1), &style);
    assert!(
        x + w <= CARD_RIGHT + 0.01,
        "the last column's label reaches {} against the content edge {CARD_RIGHT}",
        x + w,
    );

    let (x0, w0) = card_row::label_band(p, card(0), &style);
    let (free_x0, _) = card_row::label_band(p, card(0), &style.with_right_reserve(0.0));
    assert_eq!(
        x0, free_x0,
        "the right-edge reserve must not move the first-column label: {x0} vs {free_x0}",
    );
    assert!(x0 >= 0.0 && x0 + w0 <= CARD_RIGHT + 0.01);
}

#[test]
fn episode_grid_art_and_focus_labels_use_the_landscape_card_contract() {
    let item = PmsMovie { kind: 3, still: "/episode/still".into(),
        thumb: "/show/poster".into(), ..Default::default() };
    let crate::ui::widgets::Art::Still(Some(art)) = super::parts::grid_art(&item) else {
        panic!("episodes must use their own still, not the show poster");
    };
    assert_eq!(crate::ui::widgets::still_key(&art), "/episode/still");
    let layout = Layout::new(&[], 40, true).with_episodes(true);
    let rect = Rect::new(layout.cell_x(layout.cols() - 1), layout.row_y(0, 0.0),
        layout.card_w(), layout.card_h());
    let (x, width) = card_row::label_band(Painter::root(), rect, &layout.style());
    assert!(x + width <= CARD_RIGHT + 0.01, "episode focus label must keep the content band");
    let season = PmsMovie { kind: 2, ..item };
    assert!(matches!(super::parts::grid_art(&season), crate::ui::widgets::Art::Poster(Some(_))));
}

/// The head and the read-out keep the overscan frame; the cards stand in Litefin's content box,
/// 50 from each edge beside the rail (6 Grid), as the shared poster grid places them.
#[test]
fn owned_library_overscan_probe_covers_every_edge() {
    let bare = Layout::new(&[], 40, true);
    let head = Layout::new(&[crate::ui::consts::ROW_PITCH], 40, true);
    let screen = crate::screens::library::LibraryScreen::new(EntryId(1), InstanceId(1), None);
    let (bar_x, bar_w) = layout::alpha_bar(layout::ALPHABET.len());
    let safe = [
        ("library title", Rect::new(LEFT, bare.title_y(0.0), RIGHT - LEFT, layout::TITLE_H)),
        ("library tab row", Rect::new(LEFT, bare.tabs_y(0.0), RIGHT - LEFT, layout::TAB_H)),
        ("library controls row", Rect::new(LEFT, bare.controls_y(0.0), RIGHT - LEFT, layout::CONTROL_H)),
        ("library alphabet bar", Rect::new(bar_x, bare.alpha_y(0.0), bar_w, layout::ALPHA_H)),
        (
            "library shelf heading (first)",
            Rect::new(
                LEFT,
                CONTENT_TOP + head.shelf_origin(0) - crate::ui::consts::TITLE_DY,
                CARD_W,
                crate::ui::consts::TITLE_DY,
            ),
        ),
        ("library failure read-out band", screen.status_frame()),
    ];
    for (name, rect) in safe {
        assert!(consts::inside_safe(rect), "{name} at ({}, {}) {}x{} leaves the safe area",
            rect.x, rect.y, rect.w, rect.h);
    }
    let content = Rect::new(LEFT, 0.0, consts::SCR_W - 50.0 - LEFT, consts::SCR_H);
    let cards = [
        Rect::new(bare.cell_x(0), bare.row_y(0, 0.0), bare.card_w(), bare.card_h()),
        Rect::new(bare.cell_x(COLS - 1), bare.row_y(0, 0.0), bare.card_w(), bare.card_h()),
        Rect::new(CARD_LEFT, CONTENT_TOP + head.shelf_origin(0) + crate::ui::consts::CARD_DY, CARD_W, CARD_H),
    ];
    for card in cards {
        assert!(card.x >= content.x + consts::CARD_INSET && card.x + card.w <= content.x + content.w - consts::CARD_INSET + 0.01,
            "{card:?} leaves Litefin's content box {content:?}");
    }
}

    #[test]
    fn an_episode_shelf_takes_a_shorter_band_than_a_poster_shelf() {
        use crate::ui::consts::ROW_PITCH;
        // graded at the FOCUSED band, where the two shapes' difference is the tile height alone
        let open = 1.0; // owned layout uses the shared under-band expansion ratio
        let poster = shelf_pitch(false, open);
        let landscape = shelf_pitch(true, open);
        assert_eq!(poster, ROW_PITCH);
        assert!(landscape < poster, "a landscape row is shorter");
        assert_eq!(poster - landscape, CARD_H - RowStyle::EPISODE.h);

        // …and the document sums the ACTUAL pitches, so a mixed page puts the grid where the
        // shelves above it really end
        let mixed = Layout::new(&[landscape, poster, landscape], 40, true);
        let first = mixed.shelf_origin(0);
        assert_eq!(first, layout::TOP_H + crate::ui::consts::TITLE_DY);
        assert_eq!(mixed.shelf_origin(1), first + landscape);
        assert_eq!(mixed.shelf_origin(2), first + landscape + poster);
        assert_eq!(
            mixed.grid_block_top(),
            first + 2.0 * landscape + poster
        );
        // the uniform case is unchanged, which is what every other geometry test asserts
        let uniform = Layout::new(&[poster; 3], 40, true);
        assert_eq!(uniform.grid_block_top(), first + 3.0 * poster);
    }

/// The artwork FALLBACK CHAIN: the episode's own still, then the show's POSTER, and its
/// backdrop only as a last resort.
///
/// The middle rung was the backdrop until `Library Screens.dc.html` E ruled otherwise — "where
/// an episode has no still, the tile falls back to the show's poster in the same frame,
/// cover-fitted, label and all. A crop is better than a row of mixed tile shapes." Both are
/// show-level images, so neither escapes the identical-tiles problem; what decides it is that
/// the poster is the show's IDENTIFYING artwork, which is what this tile's own label is about.
///
/// (Moved from `browse::section_hubs`'s tests: `still_key` is the UI's, so a test of it is
/// not the data layer's to own.)
#[test]
fn a_landscape_tile_prefers_the_episodes_own_still() {
    use crate::screens::registry::tile_facts;
    use crate::ui::widgets::still_key;
    let full = PmsMovie {
        still: "/still".into(),
        art: "/art".into(),
        thumb: "/poster".into(),
        ..Default::default()
    };
    assert_eq!(still_key(&tile_facts::of(&full)), "/still");

    // no still — an ordinary answer for a specials folder or an item mid-scan
    let no_still = PmsMovie {
        art: "/art".into(),
        thumb: "/poster".into(),
        ..Default::default()
    };
    assert_eq!(
        still_key(&tile_facts::of(&no_still)),
        "/poster",
        "the show's own poster, cover-fitted in the same frame"
    );

    // …and the backdrop only when there is no poster either
    let bare = PmsMovie {
        art: "/art".into(),
        ..Default::default()
    };
    assert_eq!(still_key(&tile_facts::of(&bare)), "/art");
}

/// **A PARSED episode draws its OWN still even when its show has no poster.** The parse half of
/// this (`pms`'s `an_episode_keeps_its_own_still_without_a_show_poster`) grades what lands in
/// `PmsMovie::still`; this is the half that grades what the tile then DRAWS from it. `still`
/// used to be populated only when `grandparentThumb` was present, so with the show poster absent
/// the episode's own 16:9 frame survived only in `thumb`, and `still_key` — which prefers `art`
/// over `thumb` when there is no `still` — drew the show's shared backdrop: exactly the "every
/// episode is the same picture" symptom the landscape row was built to end.
#[test]
fn a_parsed_episode_draws_its_own_still_with_or_without_a_show_poster() {
    use crate::screens::registry::tile_facts;
    use crate::ui::widgets::still_key;
    let ep = |gp: &str| {
        let it = crate::catalog::Metadata {
            kind: "episode".into(),
            rating_key: "9".into(),
            title: "The Meeting".into(),
            thumb: "/ep/still".into(),
            art: "/show/art".into(),
            grandparent_thumb: gp.into(),
            grandparent_title: "The Office".into(),
            ..Default::default()
        };
        crate::catalog_fetch::parse_item(&it, crate::catalog::ServerId::from_raw(0))
    };

    // the show HAS a poster: the poster substitution stands, and the still is kept beside it
    let with = ep("/show/poster");
    assert_eq!(still_key(&tile_facts::of(&with)), "/ep/still");

    // …and with no show poster the still is STILL the episode's own frame, not the backdrop
    let without = ep("");
    assert_eq!(
        still_key(&tile_facts::of(&without)),
        "/ep/still",
        "…and never the show's shared art, which is the same picture on every episode"
    );
}
