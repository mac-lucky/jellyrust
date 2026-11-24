//! Shared detail hero geometry: Litefin's `posterLeft` split (`docs/litefin-look.md` 5), also read
//! by the safe-area audit.
//!
//! The split is padded 60 at the sides and 5% of the page width (91) at the top, so the poster's
//! left edge is x=160 (the 100px rail plus 60). The info column runs from the poster's right edge
//! plus 40 to the right padding, which stops at the overscan keep-out ([`MARGIN_X`]) rather than at
//! Litefin's 60: 36px narrower than the web client, and every logo, title and control stays inside
//! [`crate::ui::consts::SAFE`].
use super::consts::{CONTENT_LEFT, MARGIN_X, SCR_W};
use super::theme;
use super::widgets::{LfPlate, PillTone, ACTION_D};
use super::Rect;
#[cfg(test)]
use super::consts::{MARGIN_Y, SCR_H};

/// The split's side padding and its left edge, x=160 (5 Split).
pub(crate) const SPLIT_PAD: f32 = 60.0;
pub(crate) const SPLIT_X: f32 = CONTENT_LEFT + SPLIT_PAD;
/// The split's top padding: 5% of the page width right of the rail (5 Split, "91").
pub(crate) const SPLIT_TOP: f32 = 0.05 * (SCR_W - CONTENT_LEFT);
/// Air under the split before the first section (5 Split, 40 below).
pub(crate) const SPLIT_BOTTOM: f32 = 40.0;
/// The poster's width and its gap to the info column (5 Poster).
pub(crate) const POSTER_W: f32 = 420.0;
pub(crate) const POSTER_GAP: f32 = 40.0;
/// The info column's left and right edges.
pub(crate) const INFO_X: f32 = SPLIT_X + POSTER_W + POSTER_GAP;
pub(crate) const INFO_R: f32 = SCR_W - MARGIN_X;
pub(crate) const INFO_W: f32 = INFO_R - INFO_X;
/// The clear logo's box at the column's top-right corner, `contain`ed (5 Info column).
pub(crate) const LOGO_W: f32 = 280.0;
pub(crate) const LOGO_H: f32 = 100.0;
/// The title's measure: 60% of the column (5, `.details-title` max-width).
pub(crate) const TITLE_W: f32 = INFO_W * 0.6;
/// Line boxes: the title's 1.1, the overview's 1.5 (5, 2.2).
pub(crate) const TITLE_LEAD: f32 = theme::size::lf::S56 as f32 * 1.1;
pub(crate) const TITLE_LINES: usize = 2;
pub(crate) const OVERVIEW_LEAD: f32 = theme::size::lf::S24 as f32 * 1.5;
pub(crate) const OVERVIEW_LINES: usize = 6;
/// Below the title (12), the episode line, the meta row (16) and the actions row (44).
const TITLE_GAP: f32 = 12.0;
pub(crate) const EPISODE_H: f32 = theme::size::lf::S29 as f32 * 1.5;
/// The original title's line box, under the title when it differs (5 Info column, 28/500).
pub(crate) const ORIGINAL_H: f32 = theme::size::lf::S28 as f32 * 1.5;
pub(crate) const META_H: f32 = theme::size::lf::S22 as f32 * 1.5;
const META_GAP: f32 = 16.0;
const ACTIONS_GAP: f32 = 44.0;
/// The focused control's name floats 18 below the row; the band under the row that keeps it, with
/// the air Litefin leaves between that name and the tagline (about 50 from the row in the
/// reference shot), so the floating name never touches the text under it.
pub(crate) const ACTION_LABEL_DY: f32 = 18.0;
const ACTION_LABEL_AIR: f32 = 16.0;
const ACTION_LABEL_BAND: f32 = ACTION_LABEL_DY + theme::size::lf::S18 as f32 * 1.5 + ACTION_LABEL_AIR;
/// Tagline (12 below it) and the Show more pill 12 under the overview.
pub(crate) const TAGLINE_H: f32 = theme::size::lf::S22 as f32 * 1.5;
const TAGLINE_GAP: f32 = 12.0;
const MORE_GAP: f32 = 12.0;

/// Where a scrolled-to section comes to rest: 40 below the top (2.4 Vertical scroll focus offset),
/// held at the overscan keep-out.
pub(crate) const TOP_MARGIN: f32 = if 40.0 > super::consts::MARGIN_Y { 40.0 } else { super::consts::MARGIN_Y };

/// Top-left anchor for the hero logo while a trailer plays in the background — independent of
/// scroll. Sits inside the safe-area margins.
pub(crate) const PREVIEW_LOGO_X: f32 = INFO_X;
pub(crate) const PREVIEW_LOGO_Y: f32 = SPLIT_TOP;
/// Keeps a wide wordmark from reaching toward the center of the screen once it has shrunk.
pub(crate) const PREVIEW_LOGO_MAX_W: f32 = 480.0;

/// The clear logo's band at `t` of its travel from the column's top-right box (0) to the preview's
/// top-left spot (1), whose band is `compact_h` tall.
pub(crate) fn logo_band(t: f32, compact_h: f32) -> Rect {
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    Rect::new(lerp(INFO_R - LOGO_W, PREVIEW_LOGO_X), lerp(SPLIT_TOP, PREVIEW_LOGO_Y),
        lerp(LOGO_W, PREVIEW_LOGO_MAX_W), lerp(LOGO_H, compact_h))
}

/// `band` narrowed to a logo `w` wide, at the logo's place at `t` of the travel: right-aligned in
/// the top-right box, left-aligned in the preview spot, and the band's slack spent in step with `t`
/// between them, so a logo narrower than the band glides rather than flipping edges halfway.
pub(crate) fn logo_slide(band: Rect, w: f32, t: f32) -> Rect {
    let w = w.min(band.w);
    Rect::new(band.x + (band.w - w) * (1.0 - t.clamp(0.0, 1.0)), band.y, w, band.h)
}

/// The poster's height for an item whose art is portrait or landscape (episodes). Litefin's third
/// shape, square, is for music and Live TV, which this app has no detail page for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PosterShape {
    Portrait,
    Landscape,
}

impl PosterShape {
    pub(crate) fn h(self) -> f32 {
        match self {
            PosterShape::Portrait => 630.0,
            PosterShape::Landscape => 236.0,
        }
    }
}

/// What the info column holds, as measured heights and presence bits — the caller measures, this
/// stacks, so the chain stays pure.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HeroInput {
    /// The title's measured block height (at most [`TITLE_LINES`] of [`TITLE_LEAD`]).
    pub(crate) title_h: f32,
    /// The original title's line, drawn when it differs from the title.
    pub(crate) original: bool,
    pub(crate) episode: bool,
    pub(crate) meta: bool,
    pub(crate) pills: bool,
    pub(crate) tagline: bool,
    /// The overview's measured block height, 0 without one.
    pub(crate) overview_h: f32,
    /// Whether the Show more pill is drawn.
    pub(crate) more: bool,
    pub(crate) poster_h: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HeroChain {
    pub(crate) title_y: f32,
    pub(crate) original_y: f32,
    pub(crate) episode_y: f32,
    pub(crate) meta_y: f32,
    pub(crate) pills_y: f32,
    pub(crate) btn_y: f32,
    pub(crate) tagline_y: f32,
    pub(crate) overview_y: f32,
    pub(crate) more_y: f32,
    /// The split's bottom edge plus its 40: where the first section starts.
    pub(crate) content_top: f32,
}

impl HeroChain {
    /// The line the meta row's runs and plates centre on.
    pub(crate) fn meta_cy(&self) -> f32 {
        self.meta_y + META_H * 0.5
    }
    /// The line the tech pills centre on.
    pub(crate) fn pills_cy(&self) -> f32 {
        self.pills_y + LfPlate::Tech(PillTone::Plain).h() * 0.5
    }
}

/// The info column, top to bottom, each block a measured height plus Litefin's own margin, and the
/// split's bottom under whichever is taller, the poster or the column.
pub(crate) fn split_chain(i: HeroInput) -> HeroChain {
    let title_y = SPLIT_TOP;
    let mut top = title_y + i.title_h + TITLE_GAP;
    let original_y = top;
    if i.original {
        top += ORIGINAL_H;
    }
    let episode_y = top;
    if i.episode {
        top += EPISODE_H;
    }
    let meta_y = top;
    if i.meta {
        top += META_H + META_GAP;
    }
    let pills_y = top;
    if i.pills {
        top += LfPlate::Tech(PillTone::Plain).h();
    } else if i.meta {
        top -= META_GAP; // the actions row's 44 is the only air under the meta row
    }
    let btn_y = top + ACTIONS_GAP;
    top = btn_y + ACTION_D + ACTION_LABEL_BAND;
    let tagline_y = top;
    if i.tagline {
        top += TAGLINE_H + TAGLINE_GAP;
    }
    let overview_y = top;
    top += i.overview_h;
    let more_y = top + MORE_GAP;
    if i.more {
        top = more_y + LfPlate::Chip { focused: false }.h();
    }
    let content_top = (SPLIT_TOP + i.poster_h).max(top) + SPLIT_BOTTOM;
    HeroChain { title_y, original_y, episode_y, meta_y, pills_y, btn_y, tagline_y, overview_y, more_y, content_top }
}

#[cfg(test)]
pub(crate) fn overscan_rects(out: &mut Vec<(&'static str, Rect)>) {
    out.push(("detail poster", Rect::new(SPLIT_X, SPLIT_TOP, POSTER_W, PosterShape::Portrait.h())));
    out.push(("detail info column", Rect::new(INFO_X, SPLIT_TOP, INFO_W, 600.0)));
    out.push(("detail clear logo", Rect::new(INFO_R - LOGO_W, SPLIT_TOP, LOGO_W, LOGO_H)));
    out.push(("detail below-hero flow, at rest", Rect::new(SPLIT_X, TOP_MARGIN, INFO_R - SPLIT_X, 100.0)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> HeroInput {
        HeroInput {
            title_h: TITLE_LEAD,
            original: false,
            episode: false,
            meta: true,
            pills: true,
            tagline: true,
            overview_h: 3.0 * OVERVIEW_LEAD,
            more: true,
            poster_h: PosterShape::Portrait.h(),
        }
    }

    /// The split is Litefin's: poster at (160, 91), the column 40 to its right, and the column's
    /// right edge on the safe area rather than past it.
    #[test]
    fn the_split_sits_where_litefin_puts_it_inside_the_safe_area() {
        assert_eq!(SPLIT_X, 160.0);
        assert_eq!(SPLIT_TOP.round(), 91.0);
        assert_eq!(INFO_X, 620.0);
        assert!(INFO_R <= SCR_W - MARGIN_X);
        assert!(SPLIT_TOP >= MARGIN_Y);
        assert!(TOP_MARGIN >= MARGIN_Y);
    }

    /// Every block sits below the one before it, and an absent block costs nothing.
    #[test]
    fn the_column_stacks_downward_and_an_absent_block_takes_no_room() {
        let c = split_chain(full());
        let ys = [c.title_y, c.meta_y, c.pills_y, c.btn_y, c.tagline_y, c.overview_y, c.more_y];
        assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
        let bare = split_chain(HeroInput { tagline: false, ..full() });
        assert_eq!(bare.tagline_y, c.tagline_y);
        assert!(bare.overview_y < c.overview_y);
        let no_pills = split_chain(HeroInput { pills: false, ..full() });
        assert!(no_pills.btn_y < c.btn_y);
        let episode = split_chain(HeroInput { episode: true, ..full() });
        assert_eq!(episode.meta_y - c.meta_y, EPISODE_H);
        let original = split_chain(HeroInput { original: true, episode: true, ..full() });
        assert_eq!(original.original_y, c.title_y + TITLE_LEAD + TITLE_GAP, "right under the title");
        assert_eq!(original.episode_y - episode.episode_y, ORIGINAL_H);
        assert_eq!(original.meta_y - episode.meta_y, ORIGINAL_H);
    }

    /// The actions row is 44 below the pills, and the first section waits for the taller of the
    /// poster and the column, plus the split's 40.
    #[test]
    fn the_actions_row_and_the_flow_hang_off_the_measured_blocks() {
        let c = split_chain(full());
        assert_eq!(c.btn_y, c.pills_y + LfPlate::Tech(PillTone::Plain).h() + ACTIONS_GAP);
        let short = split_chain(HeroInput { overview_h: 0.0, more: false, tagline: false, ..full() });
        assert_eq!(short.content_top, SPLIT_TOP + PosterShape::Portrait.h() + SPLIT_BOTTOM);
        let tall = split_chain(HeroInput {
            overview_h: 6.0 * OVERVIEW_LEAD,
            poster_h: PosterShape::Landscape.h(),
            ..full()
        });
        let more_h = LfPlate::Chip { focused: false }.h();
        assert_eq!(tall.content_top, tall.more_y + more_h + SPLIT_BOTTOM);
    }

    /// R3.6: the logo GLIDES between its top-right box and the preview's top-left spot. A logo
    /// narrower than the band is right-aligned at the start and left-aligned at the end, and no
    /// frame of the travel jumps it sideways (it used to flip alignment at the halfway point).
    #[test]
    fn the_clear_logo_glides_from_its_box_to_the_preview_spot_without_a_jump() {
        let compact = 54.0;
        // A square logo, contained: as wide as its band is tall.
        let at = |t: f32| { let band = logo_band(t, compact); logo_slide(band, band.h, t) };
        assert_eq!(at(0.0).x + at(0.0).w, INFO_R, "right-aligned in the top-right box");
        assert_eq!(at(1.0).x, PREVIEW_LOGO_X, "left-aligned in the preview spot");
        let steps = 100;
        let worst = (0..steps).map(|i| (at((i + 1) as f32 / steps as f32).x - at(i as f32 / steps as f32).x).abs())
            .fold(0.0, f32::max);
        let travel = (at(0.0).x - at(1.0).x).abs();
        assert!(worst <= 2.0 * travel / steps as f32, "a {worst}px step in a {travel}px glide");
    }

    #[test]
    fn overscan_rects_cover_the_split() {
        let mut got = Vec::new();
        overscan_rects(&mut got);
        assert_eq!(got.len(), 4);
        assert!(got.iter().all(|(_, r)| r.y + r.h < SCR_H));
    }
}
