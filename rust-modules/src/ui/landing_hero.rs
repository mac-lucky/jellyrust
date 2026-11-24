//! Litefin's immersive hero (`docs/litefin-look.md` 4.1) as pure geometry: the backdrop box, the
//! two alpha masks that fade it into the black page, the Ken Burns zoom and the bottom-anchored
//! content block. Pure so the renderer (`screens::home`, `widgets::hero_backdrop`) and the
//! legibility table (`widgets_hero_text_tests`) read one definition.

use super::consts::{CONTENT_LEFT, SCR_W};
use super::Rect;

/// The hero's height; Home's rows overlap its lower 450.
pub(crate) const HEIGHT: f32 = 972.0;
/// The backdrop box: anchored right, 65% of the width, the hero's full height.
pub(crate) const ART: Rect = Rect::new(0.35 * SCR_W, 0.0, 0.65 * SCR_W, HEIGHT);
/// Ken Burns: the active slide's box grows from 1 to this scale about its centre, linearly over
/// [`ZOOM_S`] seconds.
pub(crate) const ZOOM_MAX: f32 = 1.08;
pub(crate) const ZOOM_S: f32 = 12.0;

/// The content block's left edge (the rail's 100 plus 80 of padding) and the line its bottom rests
/// on (the slide's 470 of bottom padding).
pub(crate) const TEXT_X: f32 = CONTENT_LEFT + 80.0;
pub(crate) const TEXT_BOTTOM: f32 = HEIGHT - 470.0;
/// The block, top to bottom: the logo box (a logo is contained in it, bottom-aligned), the meta
/// line's 20.8 px at line-height 1.5, the synopsis' 20 px lines at 1.5, each with its gap below.
pub(crate) const LOGO_W: f32 = 400.0;
pub(crate) const LOGO_H: f32 = 130.0;
pub(crate) const LOGO_GAP: f32 = 16.0;
pub(crate) const META_LH: f32 = 31.2;
pub(crate) const META_GAP: f32 = 16.0;
pub(crate) const SYN_LH: f32 = 30.0;
pub(crate) const SYN_LINES: usize = 3;
pub(crate) const SYN_GAP: f32 = 20.0;
/// The synopsis' width: 70% of the slide's 1440 inner width.
pub(crate) const SYN_W: f32 = 1008.0;

/// Where the logo box's top sits over a synopsis of `lines` lines: the block is bottom-anchored.
pub(crate) const fn logo_top(lines: usize) -> f32 {
    TEXT_BOTTOM - SYN_GAP - lines as f32 * SYN_LH - META_GAP - META_LH - LOGO_GAP - LOGO_H
}

/// The horizontal mask across the box, `(u, alpha)` from its left edge: the art is gone at the
/// left edge and whole from 92%.
pub(crate) const MASK_X: [(f32, f32); 9] = [
    (0.0, 0.0),
    (0.08, 0.02),
    (0.18, 0.10),
    (0.30, 0.26),
    (0.44, 0.48),
    (0.58, 0.70),
    (0.70, 0.86),
    (0.82, 0.96),
    (0.92, 1.0),
];
/// The vertical mask down the slide, `(v, alpha)`: whole to 35%, gone from 96%.
pub(crate) const MASK_Y: [(f32, f32); 7] = [
    (0.0, 1.0),
    (0.35, 1.0),
    (0.48, 0.96),
    (0.62, 0.75),
    (0.76, 0.38),
    (0.88, 0.08),
    (0.96, 0.0),
];

/// A mask's alpha at `t`, linear between its stops and clamped to its ends.
pub(crate) fn mask(stops: &[(f32, f32)], t: f32) -> f32 {
    let (first, last) = (stops[0], stops[stops.len() - 1]);
    if t <= first.0 {
        return first.1;
    }
    stops
        .windows(2)
        .find(|w| t <= w[1].0)
        .map(|w| w[0].1 + (w[1].1 - w[0].1) * (t - w[0].0) / (w[1].0 - w[0].0))
        .unwrap_or(last.1)
}

/// The backdrop box at Ken Burns scale `zoom`.
pub(crate) fn art_box(zoom: f32) -> Rect {
    ART.scaled(zoom)
}

/// The Ken Burns scale `t` seconds into a slide.
pub(crate) fn zoom_at(t: f32) -> f32 {
    1.0 + (ZOOM_MAX - 1.0) * (t / ZOOM_S).clamp(0.0, 1.0)
}

/// How much of the backdrop reaches the page at `(x, y)` (slide coordinates) when the box is at
/// `zoom`: the horizontal mask rides the box, the vertical one the slide. 0 outside the box.
pub(crate) fn art_alpha(x: f32, y: f32, zoom: f32) -> f32 {
    let b = art_box(zoom);
    if x < b.x || x > b.x + b.w {
        return 0.0;
    }
    mask(&MASK_X, (x - b.x) / b.w) * mask(&MASK_Y, y / HEIGHT)
}

/// The width of the detail page's narrative column (`ui::route_screen`).
pub(crate) const COL_W: f32 = 660.0;
/// The field strength a bound preview raises the detail hero to, mirrored for `ui`.
///
/// The player publishes the same number as `player::preview::PREVIEW_FIELD` (its `View::field` once
/// a picture is up). `player` may not name `ui` and `ui` may not name `player`, so each owns a copy
/// and `screens::detail`'s `preview_plane_tests` pin them equal.
pub(crate) const PREVIEW_FIELD: f32 = 1.35;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_masks_hold_their_stops_and_clamp_at_both_ends() {
        for (t, a) in MASK_X.iter().chain(MASK_Y.iter()) {
            assert!((0.0..=1.0).contains(a), "{t}: {a}");
        }
        assert_eq!(mask(&MASK_X, -1.0), 0.0);
        assert_eq!(mask(&MASK_X, 0.30), 0.26);
        assert!((mask(&MASK_X, 0.37) - 0.37).abs() < 1e-6, "halfway between 0.26 and 0.48");
        assert_eq!(mask(&MASK_X, 0.95), 1.0);
        assert_eq!(mask(&MASK_Y, 0.2), 1.0);
        assert_eq!(mask(&MASK_Y, 0.96), 0.0);
        assert_eq!(mask(&MASK_Y, 2.0), 0.0);
    }

    #[test]
    fn the_box_zooms_about_its_centre_and_the_x_mask_rides_it() {
        assert_eq!(art_box(1.0), Rect::new(672.0, 0.0, 1248.0, 972.0));
        let z = art_box(ZOOM_MAX);
        assert!((z.x + z.w * 0.5 - (ART.x + ART.w * 0.5)).abs() < 1e-3);
        assert_eq!(zoom_at(0.0), 1.0);
        assert!((zoom_at(6.0) - 1.04).abs() < 1e-6);
        assert_eq!(zoom_at(60.0), ZOOM_MAX);
        // the text side of the box is page ground; the zoomed box reaches further left
        assert_eq!(art_alpha(600.0, 300.0, 1.0), 0.0);
        assert!(art_alpha(z.x + 200.0, 300.0, ZOOM_MAX) > 0.0);
        assert_eq!(art_alpha(1800.0, 950.0, 1.0), 0.0, "the slide's foot is page ground");
    }

    #[test]
    fn the_block_rests_on_its_line() {
        // 1, 2 or 3 synopsis lines: the block grows upward from the same bottom
        assert_eq!(logo_top(3) + LOGO_H + LOGO_GAP + META_LH + META_GAP + 3.0 * SYN_LH + SYN_GAP, TEXT_BOTTOM);
        assert!(logo_top(1) > logo_top(3));
    }
}
