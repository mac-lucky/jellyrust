//! The legibility contract of Home's hero copy over its masked backdrop.

use super::*;
use super::test_support::*;

/// **Home's hero copy over its masked backdrop** (`docs/litefin-look.md` 4.1). There is no scrim:
/// the backdrop box starts at 35% of the width and fades into the black page by its two masks, so
/// what stands under a line is the art at `landing_hero::art_alpha` over black. Graded at the
/// Ken Burns ceiling, where the box reaches furthest left, and at each line's RIGHT end, the
/// worst point of its run: the title and the meta line end on bare page ground; the synopsis runs
/// 1008 px, into the first fifth of the box, where it still gets 3:1 over bright art. An alpha ink
/// is graded as what it composites to over that ground.
#[test]
fn home_hero_text_reads_over_its_masked_backdrop() {
    use crate::ui::landing_hero::{
        art_alpha, logo_top, LOGO_GAP, LOGO_H, LOGO_W, META_GAP, META_LH, SYN_LH, SYN_LINES, SYN_W,
        TEXT_X, ZOOM_MAX,
    };
    let lum = |c: [f32; 3]| 0.2126 * srgb_lin(c[0]) + 0.7152 * srgb_lin(c[1]) + 0.0722 * srgb_lin(c[2]);
    let contrast = |ink: [f32; 4], art: f32, x: f32, y: f32| {
        let g = art * art_alpha(x, y, ZOOM_MAX); // the art over the black page
        let over = |i: usize| g * (1.0 - ink[3]) + ink[i] * ink[3];
        let (l1, l2) = (lum([over(0), over(1), over(2)]), lum([g, g, g]));
        (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05)
    };
    let top = logo_top(SYN_LINES);
    let meta_top = top + LOGO_H + LOGO_GAP;
    let syn_top = meta_top + META_LH + META_GAP;
    // (label, x, y, ink, floor over BRIGHT art, floor over BLOWN WHITE)
    let rows = [
        // the S51 fallback's cap band, quoted (the host suite opens no SDL_ttf): Poppins' "H" is 0.698 em
        ("title (right end)", TEXT_X + LOGO_W, top + LOGO_H - 0.698 * theme::size::lf::S51 as f32, theme::TEXT_PRIMARY, 15.0, 15.0),
        ("meta (a full line)", TEXT_X + 0.5 * SYN_W, meta_top, theme::home_lf::META_INK, 12.0, 12.0),
        ("synopsis (first line)", TEXT_X + SYN_W, syn_top, theme::TEXT_SECONDARY, 3.0, 2.5),
        ("synopsis (last line)", TEXT_X + SYN_W, syn_top + (SYN_LINES - 1) as f32 * SYN_LH, theme::TEXT_SECONDARY, 3.0, 2.5),
    ];
    for (label, x, y, ink, min_bright, min_white) in rows {
        for (art, floor) in [(0.85f32, min_bright), (1.0, min_white)] {
            let c = contrast(ink, art, x, y);
            assert!(c >= floor, "home {label} at ({x},{y}) over art {art}: {c:.2}:1, under its {floor}:1 floor");
        }
    }
    // the box's left edge is page ground at every zoom: no hard edge of art beside the copy
    for zoom in [1.0, ZOOM_MAX] {
        let b = crate::ui::landing_hero::art_box(zoom);
        assert!(art_alpha(b.x + 1.0, 300.0, zoom) < 0.01);
    }
}
