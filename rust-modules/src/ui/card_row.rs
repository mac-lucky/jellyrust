//! `CardRow` — the shared animated poster-shelf component.
//!
//! Extracts the home shelf's per-row animation state (a focus-scale [`Spring`] per cell + a
//! horizontal scroll spring) and its tile rendering into ONE reusable widget: Litefin's classic
//! card (`docs/litefin-look.md` 4.2) — the picture, its accent badges and progress bar, the 3px
//! accent ring and 1.02 pop on focus, and the title and subtitle left-aligned under EVERY card —
//! so the home `Grid` and the detail Related row are literally the same component.
//! [`RowStyle::HOME`] is the single source of the shelf's motion and geometry that both screens read.
//!
//! What stays per-caller is only the x/`base_y` positioning loop, because home threads `env.sp` and
//! splits the loop across the `Grid`'s two-pass **cross-row** focused-last z-order (invariant #3 in
//! `ui/CLAUDE.md`) — which a lone detail row doesn't have. So `CardRow` deliberately owns spring
//! state + the leaf tile draws, and NEVER draws a whole row or applies an in-row focused-last pass
//! on a caller's behalf. It's decoupled from any focus globals / catalog: focus is a plain
//! `Option<usize>`, the item art is a [`Art`] the caller supplies.
use crate::ui::consts::*;
use crate::ui::theme;
use crate::ui::tile::{TileFacts, TileKind};
use crate::ui::widgets::{card, Art};
use crate::ui::{Painter, Rect, Spring};

pub(crate) const MAX_ROW_ITEMS: usize = 24; // == home MAX_ITEMS

/// The landscape card's box (4.2 table: 400 on a 424 pitch).
const LANDSCAPE_BOX: f32 = 400.0;

/// A card row's motion + geometry. [`RowStyle::HOME`] is the single value both the home grid and the
/// detail Related row pass, so the two rows are indistinguishable in look and animation.
#[derive(Clone, Copy)]
pub(crate) struct RowStyle {
    pub w: f32,
    pub h: f32,
    pub gap: f32,
    pub margin_x: f32,
    pub radius: f32,
    pub focus_scale: f32,
    pub k_scale: f32,
    pub k_scroll: f32,
    /// Circular tiles (cast headshots, who's-watching avatars) vs rounded-rect posters. A circle is
    /// just a tile drawn at `radius = width/2`; the shared springs/scroll/ring are identical.
    pub circular: bool,
    /// **How much of the panel's RIGHT edge is spoken for by something that is not this row** —
    /// zero everywhere but the Library grid, whose A–Z rail stands in that band.
    ///
    /// A label block is clamped against the PANEL's right edge ([`place_label`]); on every screen
    /// but one that is also the content's edge, and on the Library a last-column caption used to
    /// run through the rail's letters (photographed on the television 2026-09-05).
    ///
    /// A reserve from the physical edge, intersected with the shared safe area;
    /// [`RowStyle::with_right_reserve`] is how a screen declares one.
    pub right_reserve: f32,
    /// **The scroll policy**: `None` scrolls minimally ([`scroll_into_view`]); `Some(cx)` rests the
    /// focused card's centre on screen x `cx`, clamped at both ends so a row that fits never
    /// scrolls (Litefin's rows, `docs/litefin-look.md` 4.2 Scrolling). `None` on every preset.
    pub centre_x: Option<f32>,
}
impl RowStyle {
    /// This style with `r` px of the panel's right edge reserved — see [`RowStyle::right_reserve`].
    /// A `const fn` so a screen can name the result once, beside the constant it derives it from,
    /// rather than rebuilding it per frame.
    pub(crate) const fn with_right_reserve(self, r: f32) -> RowStyle {
        RowStyle {
            right_reserve: r,
            ..self
        }
    }

    /// **Litefin's classic poster shelf** (`docs/litefin-look.md` 4.2): a 240 card box on a 264
    /// pitch, so a 228 x 351 picture ([`CARD_W`]/[`CARD_H`]) 36 from the next, the first at
    /// [`CARD_LEFT`]; Latest rows, similar, seasons and the cast. The 1.02 pop is Litefin's effective
    /// `.media-card.focused` scale (2.6), the ring the accent's 3px.
    pub(crate) const HOME: RowStyle = RowStyle {
        w: CARD_W,
        h: CARD_H,
        gap: GAP,
        margin_x: CARD_LEFT,
        radius: theme::CARD_RING_RAD,
        focus_scale: crate::ui::widgets::CARD_FOCUS_SCALE,
        k_scale: K_SCALE,
        k_scroll: K_SCROLL,
        circular: false,
        right_reserve: 0.0,
        centre_x: None,
    };
    /// **Litefin's landscape shelf** — My Media, Continue Watching, Next Up: a 400 card box on a
    /// 424 pitch, so a 388 x 221.6 picture (4.2 table). Same motion, gap, edge and pop as [`HOME`];
    /// only the box and the 16:9 ratio differ.
    pub(crate) const EPISODE: RowStyle = RowStyle {
        w: card_image(LANDSCAPE_BOX, 0.5625).0,
        h: card_image(LANDSCAPE_BOX, 0.5625).1,
        ..RowStyle::HOME
    };
    /// "Who's watching" profile pictures: big circular avatars with a clear pop. Centered by the
    /// caller (a short roster), so the scroll spring stays put unless the row overflows.
    ///
    /// The pop is much larger than the shelves' 1.09 **on purpose**: a poster shelf marks its focus
    /// against close, hard-edged neighbours, but a lone row of widely-spaced circles has no such
    /// reference and the same 9% read as "nothing is selected". 1.18 still leaves 28px between
    /// adjacent tiles at full pop (slot 292 − 264), and `profiles::NAME_DY` derives the name band's
    /// drop from this number so the label can never collide with the popped circle.
    pub(crate) const PROFILES: RowStyle = RowStyle {
        w: 220.0,
        h: 220.0,
        gap: 72.0,
        margin_x: MARGIN_X,
        radius: 110.0,
        focus_scale: 1.18,
        k_scale: K_SCALE,
        k_scroll: K_SCROLL,
        circular: true,
        right_reserve: 0.0,
        centre_x: None,
    };
    /// Corner radius for a tile of size `rect` at radius-scale `s`: half-width for a circle, else the
    /// style's fixed radius scaled with the focus pop.
    #[inline]
    pub(crate) fn tile_radius(&self, rect: Rect, s: f32) -> f32 {
        if self.circular {
            rect.w * 0.5
        } else {
            self.radius * s
        }
    }
}

/// Per-row animation state: a focus-scale spring per cell + the horizontal scroll spring. `Copy` so
/// a grid can build `vec![CardRow::new(); n]` and carry a surviving row over by value.
#[derive(Clone, Copy)]
pub(crate) struct CardRow {
    scale: [Spring; MAX_ROW_ITEMS],
    /// The ONE spring every cell PAST the array shares — see [`CardRow::scale`]. A cast row is
    /// routinely 60 names long, so "the tiles past the 24th simply don't animate" is not a corner
    /// case; it is most of the Cast & Crew shelf, and the crew now lives at its far end.
    overflow: Spring,
    /// Which cell holds focus (`-1` none), so an overflow cell can claim `overflow` while its
    /// neighbours — which read the same spring — stay at rest.
    focus: i32,
    scroll_x: Spring,
    lift: Spring,
    pub base_y: f32,
    /// **The row is where the pointer's sideways glide put it** ([`glide`](Self::glide)): `update`
    /// stops steering the scroll to the focused cell until [`unfree`](Self::unfree). A hand-off
    /// from the pointer: what it decides, `scroll_x`, is in the canon.
    free: bool,
}
impl CardRow {
    /// Motion read by placement/reveal is behavioral state for an owned, replayable screen.
    /// Destructure exhaustively so adding a field requires revisiting the census.
    pub(crate) fn write_motion(&self, c: &mut nj_machine::machine::Canon) {
        let Self { scale, overflow, focus, scroll_x, lift, base_y, free: _ } = self;
        c.seq(scale.len());
        for spring in scale { c.f32(spring.pos).f32(spring.vel); }
        for spring in [overflow, scroll_x, lift] { c.f32(spring.pos).f32(spring.vel); }
        c.u32(*focus as u32).f32(*base_y);
    }

    pub(crate) const fn new() -> Self {
        CardRow {
            scale: [Spring::at(1.0); MAX_ROW_ITEMS],
            overflow: Spring::at(1.0),
            focus: -1,
            scroll_x: Spring::at(0.0),
            lift: Spring::at(0.0),
            base_y: 0.0,
            free: false,
        }
    }
    /// Whether every spring sits bit-exactly at its target with no velocity and nothing holds
    /// focus. An unfocused [`update`](Self::update) of such a row writes back the same bits and
    /// reports nothing to the idle gate, so a caller may skip the call (Home does, per row).
    pub(crate) fn at_exact_rest(&self) -> bool {
        let at = |sp: &Spring, rest: f32| sp.pos == rest && sp.vel == 0.0;
        self.focus == -1
            && self.scale.iter().all(|sp| at(sp, 1.0))
            && at(&self.overflow, 1.0)
            && at(&self.lift, 0.0)
    }
    /// Step every cell's scale spring (all `MAX_ROW_ITEMS` every call — invariant #10; Home skips the
    /// call for a row at [`at_exact_rest`](Self::at_exact_rest), a fixed point of this method) toward
    /// `focus_scale` for the focused cell else 1.0; then, ONLY when this row is focused, glide the
    /// scroll spring to the style's policy ([`RowStyle::centre_x`]). By default *minimally*: scroll
    /// just far enough that the focused cell (plus one gap of breathing room) sits fully inside the
    /// viewport, and not at all when it already does — no slot pinning, so entering a row never
    /// shuffles tiles that were already visible.
    /// `focused == None` freezes the scroll (a non-focused row holds its position — exactly the
    /// home shelf's behavior).
    pub(crate) fn update(&mut self, n: usize, focused: Option<usize>, sty: &RowStyle, dt: f32) {
        self.focus = focused.map(|f| f as i32).unwrap_or(-1);
        for (i, sp) in self.scale.iter_mut().enumerate() {
            let target = if focused == Some(i) {
                sty.focus_scale
            } else {
                1.0
            };
            sp.step(target, sty.k_scale, dt);
            if focused != Some(i) { sp.land(1.0, dt); }
        }
        // the shared overflow spring pops only while focus is actually out past the array
        let ot = if focused.is_some_and(|f| f >= MAX_ROW_ITEMS) {
            sty.focus_scale
        } else {
            1.0
        };
        self.overflow.step(ot, sty.k_scale, dt);
        if !focused.is_some_and(|f| f >= MAX_ROW_ITEMS) { self.overflow.land(1.0, dt); }
        if let Some(fc) = focused {
            if n > 0 && !self.free {
                self.scroll_x
                    .step(self.scroll_target(fc, n, sty), sty.k_scroll, dt);
            }
        }
        self.lift.step(
            heading_clearance(focused, sty, self.scroll_x.pos),
            sty.k_scale,
            dt,
        );
    }
    /// **How far the row still has to scroll for `focused` to be where it will rest** — live
    /// scroll minus [`CardRow::scroll_target`], so a tile's settled screen x is its live x plus
    /// this. `0` once the glide has landed, and `0` for a focus that needs no scroll at all.
    /// Derived from the spring and the focus, never stored, so it is not part of the row's
    /// replayable motion. A focused label hands it to [`place_label`] through
    /// [`TileLabel::settling`], so its alignment is judged where the tile is GOING.
    pub(crate) fn settle_lag(&self, n: usize, focused: usize, sty: &RowStyle) -> f32 {
        if n == 0 {
            return 0.0;
        }
        self.scroll_x.pos - self.scroll_target(focused, n, sty)
    }
    /// The scroll offset `sty`'s policy wants for `focused` ([`RowStyle::centre_x`]) — what
    /// `update` steers the spring to and `settle_lag` measures the live offset against.
    pub(crate) fn scroll_target(&self, focused: usize, n: usize, sty: &RowStyle) -> f32 {
        match sty.centre_x {
            None => scroll_into_view(self.scroll_x.pos, focused, n, sty.w, sty.gap, viewport(sty)),
            Some(cx) => (sty.margin_x + focused as f32 * (sty.w + sty.gap) + sty.w * 0.5 - cx)
                .clamp(0.0, max_scroll(n, sty)),
        }
    }
    /// Cell `i`'s live focus-pop scale. Cells inside the spring array own a spring each; every cell
    /// PAST it shares `overflow`, and only the focused one reads it — otherwise the whole tail of a
    /// long row would pop together. The cost of sharing is that stepping between two overflow cells
    /// swaps the pop instantly instead of crossfading; the alternative (this used to clamp the index,
    /// so an overflow cell read cell 23's spring) was a focused tile with NO pop, NO lifted shadow
    /// and NO ring — the entire focus treatment is derived from this number.
    #[inline]
    pub(crate) fn scale(&self, i: usize) -> f32 {
        if i < MAX_ROW_ITEMS {
            self.scale[i].pos
        } else if self.focus == i as i32 {
            self.overflow.pos
        } else {
            1.0
        }
    }
    #[inline]
    pub(crate) fn scroll_x(&self) -> f32 {
        self.scroll_x.pos
    }
    /// Read-only witness for the finite hero/grid device scene. A retained offset
    /// must have a stationary shelf spring while snap transforms its placement.
    #[cfg(feature = "devtriggers")]
    pub(crate) fn scroll_velocity(&self) -> f32 { self.scroll_x.vel }

    /// **The pointer's sideways glide** (`InputKind::Pan`): move the row by `dx` px now, held to
    /// its `n` cells, and leave it there: the scroll stops following the focused cell until
    /// [`unfree`](Self::unfree). Moved directly, frame by frame, so a steady glide is a steady
    /// motion (a spring stepped toward each frame's target would trail it).
    pub(crate) fn glide(&mut self, dx: f32, n: usize, sty: &RowStyle) {
        self.scroll_x.jump((self.scroll_x.pos + dx).clamp(0.0, max_scroll(n, sty)));
        self.free = true;
    }
    /// The row follows its focused cell again (a key's move along it, a click on one of its cards).
    pub(crate) fn unfree(&mut self) {
        self.free = false;
    }
    /// Restore a saved viewport without animating from the constructor's origin. Clamp against
    /// current content because the row may have shrunk while its screen was covered or evicted.
    pub(crate) fn restore_scroll(&mut self, scroll: f32, n: usize, sty: &RowStyle) {
        self.scroll_x.jump(scroll.clamp(0.0, max_scroll(n, sty)));
    }
    /// Which cell holds focus (`-1` none), as `update` last recorded it.
    #[inline]
    pub(crate) fn focus(&self) -> i32 {
        self.focus
    }
    /// How far this row's heading must rise, live — see [`heading_clearance`] for the rule and
    /// [`CardRow::update`] for the spring that holds it there.
    #[inline]
    pub(crate) fn lift(&self) -> f32 {
        self.lift.pos
    }
    /// This shelf's label-band expansion: always 1, because every Litefin card wears its label
    /// (`docs/litefin-look.md` 4.2), so no shelf has a band to open or close. The screens feed it to
    /// [`under_band`] to build their pitch.
    #[inline]
    pub(crate) fn band_expand(&self) -> f32 {
        1.0
    }
    /// The band this shelf reserves — [`under_band`] of [`CardRow::band_expand`].
    #[inline]
    pub(crate) fn under_band(&self) -> f32 {
        under_band(self.band_expand())
    }
    /// How much of the label block this shelf shows — [`band_reveal`] of [`CardRow::band_expand`],
    /// all of it. Chain it onto the `TileLabel` with [`TileLabel::revealed`].
    #[inline]
    pub(crate) fn band_reveal(&self) -> f32 {
        band_reveal(self.band_expand())
    }
}

/// The span a row's pictures may occupy: from `margin_x` to the same inset from the right edge,
/// or, for a centring row, to that inset mirrored about its centre. Litefin's rows run beside the
/// rail (x 100..1920), so their last picture rests at 1842, not at the panel's mirror of 178.
fn viewport(sty: &RowStyle) -> f32 {
    match sty.centre_x {
        Some(cx) => 2.0 * (cx - sty.margin_x),
        None => SCR_W - 2.0 * sty.margin_x,
    }
}

/// How far a row of `n` cells scrolls before its last picture reaches the [`viewport`]'s end.
fn max_scroll(n: usize, sty: &RowStyle) -> f32 {
    (n as f32 * (sty.w + sty.gap) - sty.gap - viewport(sty)).max(0.0)
}

/// **The tile in a row whose drawn centre is nearest a screen x** — the shared rule for VERTICAL
/// focus movement between two rows that scroll independently.
///
/// Directional navigation follows where the user SEES a tile, not where it lives in the row's
/// array. Two shelves hold different horizontal scrolls, so carrying the index across
/// (`dst = src`) lands focus on a tile that can be most of a screen away from the one that was
/// focused — and the destination row then scrolls hard to reveal it, which reads as the page
/// lurching sideways for no reason the user can see. Pressing DOWN with a tile at the right of the
/// screen should land on the tile that is also at the right of the screen.
///
/// `x` is the SOURCE tile's centre ([`tile_centre_x`]); the four row parameters are the
/// DESTINATION's. Rounding to nearest is what makes UP↔DOWN↔UP spatially stable: both rows sit on
/// a uniform lattice, so the return trip rounds by the same fraction the outward trip did and lands
/// back where it started — unless the clamp moved it, which is the one case where the destination
/// genuinely has no tile under the cursor.
///
/// It is also what keeps the destination row STILL: [`scroll_into_view`] scrolls only for a tile
/// that would clip the viewport, and the tile nearest the source's centre is on screen whenever the
/// source was. Scrolling is then a consequence of the geometry rather than a thing to suppress.
///
/// Lifted out of the home grid's private `Grid::vert`, which has done this since that grid was
/// written and was the only screen that did. Every shelf-based screen shares it now.
pub(crate) fn column_near_x(
    x: f32,
    origin: f32,
    adv: f32,
    w: f32,
    scroll: f32,
    n: usize,
    from: usize,
) -> usize {
    if n == 0 || adv <= 0.0 {
        return 0;
    }
    // where the source centre falls on the destination's lattice, in tiles
    let t = (x - origin - w * 0.5 + scroll) / adv;
    let lo = t.floor();
    let (dlo, dhi) = (t - lo, lo + 1.0 - t);
    // **`from` is the tie-break, and it is what makes the round trip stable.** Two rows offset by
    // exactly half a tile put the cursor equidistant between two destination tiles, and ANY rule
    // that decides a tie by direction alone — round half up, half down, half to even — ratchets:
    // going out picks the right-hand tile, coming back picks the right-hand tile again, and
    // bouncing UP/DOWN walks focus along the row one tile per press while the user presses nothing
    // horizontal. Measured by `bouncing_between_two_rows_is_stable`, which failed
    // `[7, 8, 8, 9, 9, 10, 10, 11]` on a plain round-to-nearest. Keeping the index you already had
    // is the only tie-break that is symmetric under swapping the two rows, and it is also the
    // answer a viewer expects: an exact tie is not a reason to move sideways.
    let k = if (dlo - dhi).abs() < 1.0e-3 {
        let (a, b) = (lo, lo + 1.0);
        let f = from as f32;
        if (a - f).abs() <= (b - f).abs() {
            a
        } else {
            b
        }
    } else if dlo < dhi {
        lo
    } else {
        lo + 1.0
    };
    // `floor`-based rather than a cast: a cast truncates TOWARD ZERO, so a source tile left of the
    // destination row's origin rounded the wrong way before the clamp hid it.
    (k.max(0.0) as usize).min(n - 1)
}

/// Where tile `i` of a row is drawn, centre x. The other half of [`column_near_x`], so a caller
/// never spells the same lattice out twice with the chance of disagreeing about the half-tile.
#[inline]
pub(crate) fn tile_centre_x(i: usize, origin: f32, adv: f32, w: f32, scroll: f32) -> f32 {
    origin + i as f32 * adv - scroll + w * 0.5
}

/// The SETTLED rect of tile `i` — the one formula [`strip`] draws by and `ui::geom::Shelf::place`
/// answers with (spec §7.1: draw calls place). The focus pop is applied on top by the caller
/// (`Rect::scaled` by the cell's spring), never here.
#[inline]
pub(crate) fn tile_rect(i: usize, origin: f32, pitch: f32, scroll: f32, row_y: f32, size: (f32, f32)) -> Rect {
    Rect::new(origin + i as f32 * pitch - scroll, row_y, size.0, size.1)
}

/// THE clamp-into-view core every scroller shares — uniform slots ([`scroll_into_view`]),
/// variable-width season tabs, and the home grid's vertical spring: the nearest scroll in
/// `[0, max]` at which the target band is fully revealed. `lo`/`hi` are the scrolls where the
/// item's trailing/leading edge (plus breathing room) just fits; `cur` already inside ⇒ returned
/// unchanged (no pinning).
pub(crate) fn reveal(cur: f32, lo: f32, hi: f32, max: f32) -> f32 {
    cur.clamp(lo.min(hi), hi).clamp(0.0, max)
}

/// The minimal scroll-into-view rule for uniform-slot strips (home shelves, detail
/// Related/Cast/episodes, the profiles roster): scroll only when the focused item — padded by one
/// `gap` of breathing room (which also covers the focus pop) — would clip the viewport `vw`,
/// clamped to the content range so short rows never over-scroll. (Season tabs and the vertical
/// grid have non-uniform geometry and feed their own edges straight into [`reveal`].)
pub(crate) fn scroll_into_view(cur: f32, fc: usize, n: usize, w: f32, gap: f32, vw: f32) -> f32 {
    let slot = w + gap;
    let max_sx = (n as f32 * slot - gap - vw).max(0.0);
    let lo = fc as f32 * slot + w + gap - vw; // right edge (+gap) on screen
    let hi = fc as f32 * slot - gap; // left edge (−gap) on screen
    reveal(cur, lo, hi, max_sx)
}

/// **The FULL height a `sty` heading ever rises by** — half the focus pop's growth, which is what a
/// tile magnified about its own centre lifts its top edge by.
///
/// [`heading_clearance`] is this number tapered by a proximity ramp, so it is the CEILING of that
/// function and never a value it merely happens to reach. It is named and public because a screen
/// has to RESERVE room for it: the home grid's vertical reveal bounds the scroll so a raised
/// heading cannot SETTLE inside the shared top band ([`crate::screens::home`]'s `row_reveal_band` — a
/// row travelling to that destination still crosses the band, under chrome drawn after it, exactly
/// as its cards do), and it can only do that arithmetic if the lift is a fact it can ask for rather
/// than one that lives inside a draw. Written out by hand at the call site it would be the same three terms in two
/// places, and a change to `focus_scale` would move the drawn heading while leaving the layout that
/// makes room for it behind.
#[inline]
pub(crate) fn heading_lift_max(sty: &RowStyle) -> f32 {
    sty.h * (sty.focus_scale - 1.0) * 0.5
}

/// How far a row's heading must rise to stay clear of the focused tile — the ONE rule home hub
/// titles and the detail strip headings ("Related", "Cast & Crew") share. A popped tile's focus
/// glow spills past its top edge and washes over the heading above it, but only while that tile is
/// near the row's left edge (under the heading), so this is the FULL pop height tapered by a
/// proximity ramp. The first TWO slots need the whole clearance — a heading plus the glow's spill
/// reaches past slot 1, and a partial lift there let the glow climb into the title's descender
/// line; the ramp tapers from slot 2 out.
///
/// It is a **destination, not a readout**: the height comes from the style's `focus_scale`, never
/// from a live scale spring. Multiplying by the pop's current magnitude tied the heading to the
/// magnification, so it rode every selection change down and back up — and worse, the frame focus
/// moved the incoming cell's spring was still at rest, so the heading dropped its whole lift in
/// one frame and sprang back. [`CardRow`] holds the result on its own spring, so the heading
/// simply STAYS up for as long as something is under it and settles once when nothing is.
pub(crate) fn heading_clearance(focused: Option<usize>, sty: &RowStyle, scroll: f32) -> f32 {
    let Some(c) = focused else {
        return 0.0;
    };
    let x = sty.margin_x + c as f32 * (sty.w + sty.gap) - scroll;
    let near = ((sty.margin_x + sty.w * 2.5 + sty.gap - x) / sty.w).clamp(0.0, 1.0);
    heading_lift_max(sty) * near
}
// ---- the shelf HEADING, shared by every screen that draws a shelf --------------------------
//
// It lived in `home.rs` as a private pair for as long as Home was the only screen with shelves.
// The Library grew its own on 2026-09-05 (the server owner's `Library Recommended` rows), and a
// second copy would have drifted on exactly the details that make a heading readable: which run is
// the title's rung, how far the annotation sits from it, and which ink names a source.
/// The gap either side of the heading's separator dot.
pub(crate) const HEADING_SOURCE_PAD: f32 = theme::space::XS;


/// The shelf heading, flowed left→right from the heading origin: the hub's title, and — only when
/// the row came from ANOTHER server — a quiet `· handle` naming that source ("Recently Added in
/// Film Club · friend").
///
/// **With an empty source the annotation is ABSENT, not empty**: no gap, no dot, no second run, no
/// draw call. That is the design's "with one source, none of this is drawn" implemented as absence
/// rather than as a branch that draws nothing visible — so the ANNOTATION costs a single-server
/// library nothing at all, in geometry, in draw calls and in ink.
///
/// The heading is NOT unchanged overall, and the difference is deliberate: its ink moved from
/// `TEXT_PRIMARY` (`#f7fafc`) to `TEXT_HEADING` (`#ebf0f7`) in the same pass. Every shelf heading on
/// Home is ~4% darker as a result, today, with no source string anywhere. That is a harmonization,
/// not a side effect — `TEXT_HEADING` is the shared section-heading ink that `detail.rs` and
/// `person.rs` already use, and Home was the one screen still inking its headings as body text.
///
/// Two runs and not one string because they are two SIZES, two weights and three inks, and
/// `Painter::text` can express exactly one of each per call (`theme::TEXT_SEPARATOR`'s doc: a dot
/// baked into a joined string is one run at one colour by construction). They are BASELINE-aligned
/// via `text::baseline_y` — a `BODY` run top-aligned against a `HEADLINE` one reads as a
/// superscript.
///
/// `run(text, dx, sz, bold, ink) -> advance` is the seam: the draw passes a closure that paints and
/// returns `Painter::text`'s advance, the host tests pass one that measures. The drawn flow and the
/// graded one are therefore ONE expression, not `dotted_run`/`dotted_run_w`'s two that have to be
/// kept agreeing. Returns the total advance.
///
/// The flow is PURE — it takes no font metric, which is also why the host suite can drive it: the
/// per-run baseline drop is resolved in [`draw_heading`], from the very `sz`/`bold` handed out here.
///
/// Not `ui::fit::two_runs`: the runs are a sequential server-text flow, each placed after the last,
/// not a primary/secondary pair competing for one span.
pub(crate) fn heading_flow(
    title: &str,
    source: &str,
    mut run: impl FnMut(&str, f32, std::os::raw::c_int, std::os::raw::c_int, [f32; 4]) -> f32,
) -> f32 {
    let mut dx = run(title, 0.0, theme::size::HEADLINE, 1, theme::TEXT_HEADING);
    if source.is_empty() {
        return dx; // one source: the heading is the title and nothing else
    }
    dx += HEADING_SOURCE_PAD;
    dx += run("\u{b7}", dx, theme::size::BODY, 0, theme::TEXT_SEPARATOR);
    dx += HEADING_SOURCE_PAD;
    dx += run(source, dx, theme::size::BODY, 0, theme::TEXT_TERTIARY);
    dx
}

/// Draw [`heading_flow`] with its title's cap top at `(x, y)`. Every run rides the painter handed
/// in, so the snap fade and the row's heading `lift` move the whole heading together — an
/// annotation that detached from its title mid-scroll would read as two objects.
///
/// Each run drops onto the TITLE's baseline (`text::baseline_y`, a no-op for the title itself,
/// which is measured against its own tokens): the annotation is a rung down, and top-aligning a
/// `BODY` run against a `HEADLINE` one would read as a superscript. That one line is the only part
/// of this heading the host suite cannot grade — it opens no SDL_ttf, so there are no cap bands to
/// measure (the same boundary `widgets`' anchor table works within); what the tests DO pin is the
/// tokens it resolves from.
pub(crate) fn draw_heading(
    p: Painter,
    title: &str,
    source: &str,
    x: f32,
    y: f32,
    max_w: f32,
    measure: &dyn nj_machine::machine::Measure,
) {
    bounded_heading_flow(title, source, max_w, measure, |s, dx, sz, bold, ink| {
        // the CString must outlive the draw call, not the closure (`ui/CLAUDE.md`'s first gotcha)
        match std::ffi::CString::new(s) {
            Ok(cs) => p.text(
                cs.as_ptr(),
                x + dx,
                nj_gfx::text::baseline_y(sz, bold, theme::size::HEADLINE, 1, y),
                sz,
                ink,
                0,
                bold,
            ),
            Err(_) => 0.0,
        }
    });
}

/// [`heading_flow`] against a right boundary: the one elision rule every shelf heading shares,
/// [`draw_heading`]'s and `ui::linked_heading`'s. `run` receives each run already elided and
/// returns its advance, so the measured and the drawn flow stay one expression.
///
/// **`max_w` is a RIGHT BOUNDARY, and a run that would cross it is elided rather than clipped** —
/// through `text::elide`, the single truncation impl in the app, so a bounded heading reads like
/// every other bounded string here. It exists because the Library's A–Z rail occupies the right
/// of the content region: a heading drawn to the panel edge runs UNDER the letters, which looks
/// broken and is not something the rail can fix from its own side. Every other caller passes
/// `INFINITY` and pays nothing — the elide is skipped outright, so the memoised binary search is
/// never entered for a heading nobody bounded.
pub(crate) fn bounded_heading_flow(
    title: &str,
    source: &str,
    max_w: f32,
    measure: &dyn nj_machine::machine::Measure,
    mut run: impl FnMut(&str, f32, std::os::raw::c_int, std::os::raw::c_int, [f32; 4]) -> f32,
) -> f32 {
    heading_flow(title, source, |s, dx, sz, bold, ink| {
        let room = max_w - dx;
        if max_w.is_finite() && room < MIN_HEADING_RUN {
            return 0.0; // no room left for this run at all — drop it rather than draw a stub
        }
        if max_w.is_finite() {
            let owned =
                nj_gfx::text::elide_by(s, room, false, |t| measure.width_str(t, sz, bold != 0));
            run(&owned, dx, sz, bold, ink)
        } else {
            run(s, dx, sz, bold, ink)
        }
    })
}

/// The narrowest run [`draw_heading`] will draw against a bound. Below it an elide has nothing left
/// but its own ellipsis, and a heading ending in a bare `…` beside another bare `…` reads as
/// damage rather than as truncation.
const MIN_HEADING_RUN: f32 = 48.0;


/// Conservative paint bounds for a complete card: its focus ring and, when `labelled`, the label
/// block under it. The label anchors to the unscaled bottom even during a press, as [`draw_cell`]
/// does.
fn tile_paint_bounds(rect: Rect, scale: f32, labelled: bool) -> Rect {
    let mut bounds = rect.inset(-(theme::FOCUS_RING_W + 1.0));
    if labelled {
        let label_bottom = rect.y + rect.h * 0.5 + rect.h / scale * 0.5 + UNDER_LABEL_H;
        bounds.h = bounds.h.max(label_bottom - bounds.y);
    }
    bounds
}

/// Reject a whole card before preparing artwork or fitting overlay text. The blur source is a
/// crop of the page; its primitive-level rejection comes too late to avoid that CPU work.
pub(crate) fn paint_visible(p: Painter, rect: Rect, scale: f32, labelled: bool) -> bool {
    // Recording follows the buffered composition's text queries, including its offscreen rows.
    if p.is_recording() { return true; }
    let mut bounds = tile_paint_bounds(rect, scale, labelled);
    bounds.x += p.dx();
    bounds.y += p.dy();
    let visible = bounds.intersect(Rect::FULL);
    visible.w > 0.0 && visible.h > 0.0
        && (crate::ui::frame::backdrop::discovering()
            || !nj_gfx::gfx::culled(bounds.x, bounds.y, bounds.w, bounds.h))
}

/// **THE card draw** — every shelf and grid card, focused or not: the picture and its badges
/// (`widgets::card`), the progress bar, the focus ring on the focused card, and the label block
/// under the picture (`docs/litefin-look.md` 4.2).
///
/// `rect` is the caller's already-scaled rect and **`s` IS THE SCALE `rect` WAS BUILT FROM — pop
/// AND press**: the label anchors to the UNSCALED card bottom as `rect.h / s`, which is the resting
/// height only while `s` is the whole scale, so a press that reached the rect but not `s` would
/// slide the caption on every click. Every caller passes the press-inclusive scale.
///
/// **A catalog card names itself.** When `art` carries the row ([`Art::Poster`]/[`Art::Still`]
/// with facts), the text is [`tile_label`] of it and `label` lends only its reveal: Litefin builds
/// one label per item (`CardRenderer`), so a card reads the same at rest and under focus on every
/// screen. A caller's own text is drawn for art that carries no row (a library tile, an extra).
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_cell(
    p: Painter,
    art: Art,
    rect: Rect,
    s: f32,
    sty: &RowStyle,
    resume: Option<f32>,
    label: &TileLabel,
    focused: bool,
    measure: &dyn nj_machine::machine::Measure,
) {
    let rad = sty.tile_radius(rect, s);
    let own = art_label(&art).map(|own| TileLabel { reveal: label.reveal, settle_lag: label.settle_lag, ..own });
    let label = own.as_ref().unwrap_or(label);
    // `card` takes the RESTING rect and the scale, not the popped rect: whatever it sets from the
    // tile's size (a fan's live name) must not re-layout on every frame of a pop. Litefin's ring has
    // no transition, so the focused card wears all of it and every other card none.
    card(p, rect.scaled(1.0 / s), art, rad, true, s, f32::from(focused));
    if let Some(frac) = resume {
        resume_bar(p, rect, frac, rad);
    }
    // The label block anchors to the UNSCALED card bottom (rect is scaled about its center, so the
    // base bottom = cy + (h/s)/2): a pop or press never moves it, the pop eats into its gap.
    let ty = rect.y + rect.h * 0.5 + (rect.h / s) * 0.5;
    if label.reveal <= 0.0 {
        return;
    }
    draw_label_block(p.alpha(label.reveal), rect, sty, label, ty, focused, measure);
}

/// The metadata block under a tile: its title and, optionally, a subtitle line (a year, an
/// episode's name, a character name). Every card carries one (`docs/litefin-look.md` 4.2 Labels),
/// built by [`tile_label`] for a catalog row.
///
/// It is a type rather than two parameters because the block's height is a fact the SCREEN needs:
/// every shelf must reserve room for it in its own flow, and [`TileLabel::height`] is that number.
pub(crate) struct TileLabel {
    pub title: Option<std::ffi::CString>,
    pub caption: Option<std::ffi::CString>,
    /// **How much of this block is on screen, 0..1** — [`band_reveal`] of the row's label band,
    /// which is always all of it now; `Default` gives 1.
    pub reveal: f32,
    /// **How far the tile's row still has to scroll before this tile rests** (see
    /// [`CardRow::settle_lag`]); `0` for a tile that is not mid-glide. [`place_label`] judges the
    /// room the panel leaves the block at the tile's SETTLED position, so a glide does not re-elide
    /// it on every frame.
    pub settle_lag: f32,
}

/// **Hand-written, not derived, for one field**: `reveal` defaults to 1 (fully on screen), where a
/// derive would give 0 and every tile in the app would draw no label at all.
impl Default for TileLabel {
    fn default() -> Self {
        TileLabel {
            title: None,
            caption: None,
            reveal: 1.0,
            settle_lag: 0.0,
        }
    }
}

impl TileLabel {
    /// Chain this off a shelf's [`CardRow::band_reveal`].
    pub(crate) fn revealed(mut self, a: f32) -> Self {
        self.reveal = a;
        self
    }
    /// Chain this off a shelf's [`CardRow::settle_lag`]: the label's width is judged where the tile
    /// will REST, and the block only translates with it on the way there.
    pub(crate) fn settling(mut self, lag: f32) -> Self {
        self.settle_lag = lag;
        self
    }
    /// Title only — the plain poster shelf.
    pub(crate) fn title(t: &str) -> Self {
        TileLabel {
            title: std::ffi::CString::new(t).ok(),
            ..Default::default()
        }
    }
    /// Title + a caption line; an empty caption is simply absent (never a blank row).
    pub(crate) fn titled(t: &str, caption: &str) -> Self {
        TileLabel {
            caption: std::ffi::CString::new(caption)
                .ok()
                .filter(|_| !caption.is_empty()),
            ..Self::title(t)
        }
    }
    /// THE height a screen must reserve under the picture for this block. Derived from the same
    /// constants [`draw_label_block`] lays it out with, so a screen's declared band and the block on
    /// screen cannot drift. Each line is exactly one line: a title too wide for its card marquees in
    /// place under focus and elides at rest, a subtitle elides.
    pub(crate) fn height(caption: bool) -> f32 {
        UNDER_DROP
            + UNDER_LINE_H
            + if caption {
                UNDER_LINE_GAP + UNDER_CAPTION_H
            } else {
                0.0
            }
    }
    /// [`TileLabel::height`] for a block that has whatever THIS one has.
    fn own_height(&self) -> f32 {
        Self::height(self.caption.is_some())
    }
}

/// The focused cell — the caller draws this LAST for its z-order: [`draw_cell`] with the focus
/// ring and a marquee for a title that overflows its card. `label` supplies the text only for art
/// that carries no catalog row (see [`draw_cell`]).
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_focused(
    p: Painter,
    art: Art,
    rect: Rect,
    s: f32,
    sty: &RowStyle,
    resume: Option<f32>,
    label: &TileLabel,
    measure: &dyn nj_machine::machine::Measure,
) {
    draw_cell(p, art, rect, s, sty, resume, label, true, measure);
}

/// THE one-row strip loop: draw a whole horizontal shelf, non-focused tiles first (off-axis
/// culled) and the focused tile LAST for its z-order — the in-row focused-last rule, which is all
/// a lone strip needs (home's `Grid` keeps its own CROSS-row pass, which this deliberately does
/// not try to own).
///
/// This is the loop the detail page's Related and Cast rows and the person page's Movies/Shows
/// shelves all run; it lives here rather than in a screen so the culling discipline travels with
/// it. **Only tiles that pass `on_axis` are drawn, and therefore only they call `resolve_tex_wh_on`** —
/// the 64-slot poster LRU must never see an off-screen request, which is exactly what a
/// hand-rolled per-screen copy of this loop keeps getting wrong.
///
/// The caller supplies the per-index content as closures: `art` the tile's [`Art`], `resume` its
/// progress fraction (`None` = not in progress), `label` each drawn tile's [`TileLabel`] (a catalog
/// card names itself, see [`draw_cell`]; `TileLabel::default()` for a row that captions every tile
/// through `extra` instead), and `extra` any per-tile caption drawn for EVERY tile (cast
/// names/roles). `axis_span` widens the cull band where captions should survive slightly off-screen.
#[allow(clippy::too_many_arguments)]
pub(crate) fn strip<'a>(
    p: Painter,
    row: &CardRow,
    n: usize,
    focus_col: std::os::raw::c_int,
    row_y: f32,
    size: (f32, f32),
    pitch: f32,
    sty: &RowStyle,
    axis_span: f32,
    art: impl Fn(usize) -> Art<'a>,
    resume: impl Fn(usize) -> Option<f32>,
    label: impl Fn(usize) -> TileLabel,
    extra: impl Fn(Painter, usize, f32, bool),
    measure: &dyn nj_machine::machine::Measure,
) {
    let sx = row.scroll_x();
    let pr = p.translate(-sx, 0.0);
    for i in 0..n {
        if i as std::os::raw::c_int == focus_col {
            continue; // focused tile drawn last
        }
        // `tile_rect` in the row's own (unscrolled) space: `pr` carries the scroll
        let x = tile_rect(i, sty.margin_x, pitch, 0.0, row_y, size).x;
        if !crate::ui::on_axis(x - sx, size.0, axis_span, 0.0) {
            continue;
        }
        let s = row.scale(i);
        let rect = tile_rect(i, sty.margin_x, pitch, 0.0, row_y, size).scaled(s);
        draw_cell(pr, art(i), rect, s, sty, resume(i), &label(i), false, measure);
        extra(pr, i, x, false);
    }
    if focus_col >= 0 && (focus_col as usize) < n {
        let i = focus_col as usize;
        let x = tile_rect(i, sty.margin_x, pitch, 0.0, row_y, size).x;
        // fold the ui::press click dip into the focused tile's scale (1.0 when idle) — same as home
        let s = row.scale(i) * crate::ui::press::scale();
        let rect = tile_rect(i, sty.margin_x, pitch, 0.0, row_y, size).scaled(s);
        draw_focused(
            pr,
            art(i),
            rect,
            s,
            sty,
            resume(i),
            &label(i).revealed(row.band_reveal()).settling(row.settle_lag(n, i, sty)),
            measure,
        );
        extra(pr, i, x, true);
    }
}

/// **A catalog card's label, as Litefin words it** (`CardRenderer`'s text rules with the
/// household's defaults, `docs/litefin-look.md` 4.2 Labels): the title and one subtitle line.
///
/// | kind | title | subtitle |
/// |---|---|---|
/// | movie, show, collection | its name | its year |
/// | season | its show | its own name |
/// | episode on a landscape card | its show | its name (the code is its badge, `widgets::episode_code`) |
/// | episode on a poster | its name | none (the badge again) |
/// | actor (cast) | the person | "as" their character (`cards.as_role`) |
/// | crew | the person | the job |
/// | a person page's film or show | its name | its year · "as" the part they play |
///
/// `landscape` is the card's shape ([`Art::Still`]); Litefin words an episode by it. A field the
/// server left empty leaves its line out rather than drawing a blank one.
pub(crate) fn tile_label(m: &TileFacts<'_>, landscape: bool) -> TileLabel {
    let has_show = !m.show_title.is_empty();
    match m.kind {
        TileKind::Episode if landscape && has_show => TileLabel::titled(m.show_title, m.title),
        TileKind::Episode => TileLabel::title(m.title),
        TileKind::Season if has_show => TileLabel::titled(m.show_title, m.title),
        TileKind::Crew => TileLabel::titled(m.title, m.role),
        _ if !m.role.is_empty() => {
            let role = nj_platform::i18n::msg::cards_as_role(m.role);
            TileLabel::titled(m.title, &if m.year > 0 { format!("{} · {role}", m.year) } else { role })
        }
        _ if m.year > 0 => TileLabel::titled(m.title, &m.year.to_string()),
        _ => TileLabel::title(m.title),
    }
}

/// The label a card's own row gives it, when the art carries the row — see [`draw_cell`].
fn art_label(art: &Art<'_>) -> Option<TileLabel> {
    match art {
        Art::Poster(Some(m)) => Some(tile_label(m, false)),
        Art::Still(Some(m)) => Some(tile_label(m, true)),
        _ => None,
    }
}

// The label block's metrics (`docs/litefin-look.md` 4.2 Labels) — the ONLY authority on its
// geometry, which [`TileLabel::height`] hands back to the screens that must reserve room for it.
/// Picture bottom → the title's line box: the picture's 3px ring, then `.card-info`'s 12 of top
/// padding.
const UNDER_DROP: f32 = theme::FOCUS_RING_W + 12.0;
/// The title's line box, 19.2px at `line-height: 1.5` = 28.8, held at 28.75 so every pitch summed
/// from it is exact in `f32` (a 585.75 shelf rather than a 585.8 one that never compares equal).
const UNDER_LINE_H: f32 = 28.75;
/// Title → subtitle (`.card-subtitle` `margin-top: 6px`).
const UNDER_LINE_GAP: f32 = 6.0;
/// The subtitle's line box, 16px at 1.5.
const UNDER_CAPTION_H: f32 = 24.0;
/// The label block's total height with a subtitle, 73.75 — exposed so a SCREEN's row pitch is built
/// from the one number that also drives [`TileLabel::height`]; `consts::UNDER_LABEL_AIR` is the
/// other half, the air from the block to the next row's heading.
pub(crate) const UNDER_LABEL_H: f32 = UNDER_DROP + UNDER_LINE_H + UNDER_LINE_GAP + UNDER_CAPTION_H;
/// The title's and the subtitle's type: Litefin's 19.2px at 600 and 16px at 400, on their rungs.
const TITLE_SZ: std::os::raw::c_int = theme::size::lf::S19;
const CAPTION_SZ: std::os::raw::c_int = theme::size::lf::S16;
/// The label column's inset from the picture's left and right edges: `.card-info` spans the card
/// box inside its 3px border with 4px of side padding (4.2 Labels: "padding 12 top, 4 sides"), and
/// the picture is that border plus the ring in from the box, so the text sits `4 - ring` = 1px in.
const LABEL_INSET: f32 = 4.0 - theme::FOCUS_RING_W;

/// **What a shelf reserves under its pictures when it does not hold focus** — the same
/// [`UNDER_LABEL_H`], because every Litefin card wears its label (4.2), so there is nothing to
/// collapse. The band arithmetic below is kept whole and degenerates exactly: [`under_band`] is
/// constant, [`band_reveal`] is 1, [`BAND_OPEN`] is 0, and every screen's running sum of
/// `ROW_PITCH_FIXED + under_band(e)` is a uniform `consts::ROW_PITCH`.
pub(crate) const LABEL_BAND_COLLAPSED: f32 = UNDER_LABEL_H;

/// The label band a shelf reserves at expansion `e` (0 collapsed → 1 focused) — **the ONE
/// expression every screen's row pitch is built from**, so Home, the Library, Search and the person
/// page cannot reserve three different amounts. [`UNDER_LABEL_H`] at every `e`.
pub(crate) fn under_band(e: f32) -> f32 {
    LABEL_BAND_COLLAPSED + (UNDER_LABEL_H - LABEL_BAND_COLLAPSED) * e.clamp(0.0, 1.0)
}

/// **How much of the label block may be on screen at expansion `e`**: 0 while the room under the
/// card cannot hold it clear of the next heading, smoothstepping to 1 once it can. The room is
/// `UNDER_LABEL_AIR + under_band(e)` down from the card bottom and the block needs
/// [`UNDER_LABEL_H`] of it, so with the band always open this is 1 at every `e`.
pub(crate) fn band_reveal(e: f32) -> f32 {
    let room = crate::ui::consts::UNDER_LABEL_AIR + under_band(e);
    let holds = UNDER_LABEL_H; // the room at which the block is fully clear of the next heading
    let settled = crate::ui::consts::UNDER_LABEL_AIR + UNDER_LABEL_H;
    let t = ((room - holds) / (settled - holds)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The fixed half of a poster shelf's pitch — everything `consts::ROW_PITCH` is made of EXCEPT the
/// label band. Add [`under_band`] to it.
pub(crate) const ROW_PITCH_FIXED: f32 = crate::ui::consts::ROW_PITCH - UNDER_LABEL_H;

/// How much a shelf GROWS when it takes focus — the travel of [`under_band`]: nothing.
pub(crate) const BAND_OPEN: f32 = UNDER_LABEL_H - LABEL_BAND_COLLAPSED;

// ---- The focused single-line title's marquee ------------------------------------------------
//
// Litefin's card title is one line with an ellipsis, and "overflow marquee on focus"
// (`docs/litefin-look.md` 2.5, 4.2): a resting card elides, the focused one falls through to a
// looping marquee whenever its title does not fit the card: a beat of rest so the couch can start
// reading, a slow glide left, a gap, then the same run re-entering from the right, forever while
// this tile holds focus.

/// How long the run rests before it starts gliding — long enough to read the opening words of an
/// ordinary title before it moves.
const MARQUEE_HOLD_MS: f32 = crate::ui::marquee::HOLD_MS;
/// Glide speed, in px/s. Chosen to be readable from a couch rather than merely legible paused — a
/// scrolling news-ticker speed blurs on a TV's own motion smoothing.
const MARQUEE_SPEED: f32 = crate::ui::marquee::SPEED;
/// Air between the outgoing run's tail and its follower's head — enough that the two never read as
/// one run with a repeated word running into itself.
const MARQUEE_GAP: f32 = 60.0;

/// The marquee's horizontal offset at `t_ms` since this run became (or stayed) the focused title,
/// for a run `text_w` px wide inside a `budget` px window.
///
/// A pure function of elapsed time, which is what makes it trivially unit-testable and trivially
/// resettable — the caller just starts `t_ms` back at zero. `0` for the first [`MARQUEE_HOLD_MS`]
/// (the run sits at rest, left edge in the window), then glides at [`MARQUEE_SPEED`] until the run
/// plus [`MARQUEE_GAP`] of air has fully passed, then loops. A run that already fits (`text_w <=
/// budget`) is inert at every `t_ms` — the caller should not even reach here for one, but this
/// stays harmless if it does.
///
/// Draw TWO copies at `x - offset` and `x - offset + text_w + MARQUEE_GAP` (the follower), both
/// clipped to `budget` — the follower is what makes the wrap seamless instead of a visible pop back
/// to the start: by the time `offset` reaches the full travel distance the follower has arrived
/// exactly where the primary run started, so the loop boundary is invisible.
fn marquee_x(t_ms: f32, text_w: f32, budget: f32) -> f32 {
    if text_w <= budget {
        return 0.0;
    }
    let period = marquee_period(text_w, budget);
    let t = t_ms.max(0.0) % period;
    if t < MARQUEE_HOLD_MS {
        0.0
    } else {
        (t - MARQUEE_HOLD_MS) / 1000.0 * MARQUEE_SPEED
    }
}

/// Is the marquee actually GLIDING at `t_ms` — i.e. is this the frame that must keep
/// [`nj_machine::idle`] awake? `false` during the rest beat and whenever the run fits, so a settled
/// screen full of short (or currently-resting) titles still meets the idle present gate's fps
/// ceiling — see [`title_marquee`]'s doc for why this has to be a separate question from
/// [`marquee_x`] rather than "moved since last frame": the rest beat's `offset == 0` is not motion,
/// but it is also not the screen being settled in the sense the gate cares about — nothing is
/// drawn differently between two resting frames, which is exactly what should NOT report.
fn marquee_moving(t_ms: f32, text_w: f32, budget: f32) -> bool {
    if text_w <= budget {
        return false;
    }
    t_ms.max(0.0) % marquee_period(text_w, budget) >= MARQUEE_HOLD_MS
}

/// One full marquee cycle in ms — the rest beat plus the glide that carries the run and its
/// [`MARQUEE_GAP`] of air fully past. The ONE place the period is spelled; [`marquee_x`],
/// [`marquee_moving`] and [`marquee_phase`] all fold on it.
fn marquee_period(text_w: f32, budget: f32) -> f32 {
    let _ = budget; // a fitting run never reaches here; the period is a property of the run alone
    let travel = text_w + MARQUEE_GAP; // one full cycle's glide distance
    MARQUEE_HOLD_MS + travel / MARQUEE_SPEED * 1000.0
}

/// Fold the `f64` clock onto one period BEFORE it becomes an `f32`. The accumulator is `f64` so
/// it never stops advancing, but an `f32` of a six-day-old clock (~2^29 ms) still only moves in
/// 64 ms steps — which at [`MARQUEE_SPEED`] is a 2.5 px judder at 15 Hz rather than a glide
/// (Codex review, 2026-09-02). A phase is never larger than one period, so it is exact in `f32`.
fn marquee_phase(t_ms: f64, text_w: f32, budget: f32) -> f32 {
    if text_w <= budget {
        return 0.0;
    }
    (t_ms.max(0.0) % marquee_period(text_w, budget) as f64) as f32
}

thread_local! {
    /// Which title currently owns the marquee clock below, and since when. Only one tile can hold
    /// focus app-wide, so ONE clock is enough — keyed by the drawn TEXT rather than a catalog id,
    /// because `draw_focused` is called from five sites across three lanes' files (home, library,
    /// detail, person, profiles) with no uniform notion of "item identity" to key on. A coincidental
    /// identical title on a genuinely different item simply keeps the marquee running rather than
    /// resetting it, which is invisible — the two runs read the same either way.
    static MARQUEE_KEY: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
    /// The [`nj_machine::idle::now_ms`] reading when `MARQUEE_KEY` last changed. `marquee_clock`
    /// reads its elapsed time as `now_ms().wrapping_sub(this)` rather than summing a per-frame
    /// delta — this runs inside `draw`, which — unlike [`CardRow::update`] — gets no `Tick` of its
    /// own, so it cannot advance a `motion::Phase` directly, but a `wrapping_sub` of two absolute
    /// readings is the same drift-free idiom `Phase::advance` uses, and needs no `f64` accumulator
    /// to survive a six-day-old title the way the retired summed clock did.
    static MARQUEE_START_MS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Advance (or restart) the marquee clock for `text`, returning its value in ms. Called once per
/// frame from [`title_marquee`], which is called at most once per frame (the focused tile is drawn
/// exactly once) — so this cannot double-advance within a frame the way a naively-shared clock read
/// from two draws in the same pass would.
fn marquee_clock(text: &str) -> f64 {
    let now = nj_machine::idle::now_ms();
    let changed = MARQUEE_KEY.with(|k| {
        let mut k = k.borrow_mut();
        if k.as_str() == text {
            false
        } else {
            *k = text.to_string();
            true
        }
    });
    if changed {
        MARQUEE_START_MS.with(|m| m.set(now));
        0.0
    } else {
        MARQUEE_START_MS.with(|m| now.wrapping_sub(m.get()) as f64)
    }
}

/// **Where a tile's label block lands**: the unscaled card's leading edge, [`LABEL_INSET`] in, and
/// the card's width less both insets — Litefin's labels are left aligned and never wider than their
/// card (4.2). Returns `(x, w)` in the painter's own space.
///
/// The width only narrows where the PANEL ends — the physical edge less [`EDGE_PAD`] and less
/// whatever band the screen reserves there (`RowStyle::right_reserve`, the Library's A–Z rail) — so
/// a card cut by the panel's right edge elides its label to what is on screen. **That test is a
/// screen fact and `rect` usually is not**: [`strip`] draws a shelf through `translate(-scroll_x,
/// 0)`, so `p.dx()` converts, and `lag` ([`CardRow::settle_lag`]) judges it where the tile will
/// REST, so a gliding tile does not re-elide on every frame. Whole pixels, for the same reason.
pub(crate) fn place_label(p: Painter, rect: Rect, sty: &RowStyle, lag: f32) -> (f32, f32) {
    let x = rect.cx() - sty.w * 0.5 + LABEL_INSET;
    let hi = SCR_W - sty.right_reserve - EDGE_PAD - (p.dx() + lag);
    (x, (sty.w - 2.0 * LABEL_INSET).min(hi - x).max(0.0).floor())
}

/// Air kept between a label block and the PANEL's edge (or a band the screen reserves there).
const EDGE_PAD: f32 = 16.0;

/// **Where a tile's label block actually lands on the panel** — its left edge and its width, in
/// `p`'s own space, for a screen with a constraint on it to grade the block that is DRAWN.
pub(crate) fn label_band(p: Painter, rect: Rect, sty: &RowStyle) -> (f32, f32) {
    place_label(p, rect, sty, 0.0)
}

/// The focused tile's single-line title at `(x, y)`: plain whenever the run fits `w`, else a
/// looping [`marquee_x`] inside it — see the section doc above. Reports to [`nj_machine::idle`]
/// only on a frame the marquee is actually gliding, so a screen of short (or resting) titles costs
/// the present gate nothing.
fn title_marquee(p: Painter, x: f32, w: f32, text: &std::ffi::CStr, y: f32, measure: &dyn nj_machine::machine::Measure) {
    let run_w = measure.width(text, TITLE_SZ, true);
    if run_w <= w {
        // a fitting title RELEASES the clock, so an overflowing one focused again later starts
        // from its rest beat rather than resuming mid-glide
        MARQUEE_KEY.with(|k| k.borrow_mut().clear());
        p.text(text.as_ptr(), x, y, TITLE_SZ, theme::TEXT_PRIMARY, 0, 1);
        return;
    }
    let t_ms = marquee_phase(marquee_clock(&text.to_string_lossy()), run_w, w);
    // The clock above advances by `idle::now_ms` ON DRAWN FRAMES ONLY, and a drawn frame is one the
    // present gate let through. So the rest beat has to buy its own frames, or it never ends (a
    // focused overflowing title sat clipped and motionless at 4 s and 7 s of focus, sim
    // 2026-09-02). `wake` is the hold's half (a frame, no claim that pixels changed); `invalidate`
    // is the glide's, where they do.
    if marquee_moving(t_ms, run_w, w) {
        nj_machine::idle::invalidate();
    } else {
        nj_machine::idle::wake();
    }
    let off = marquee_x(t_ms, run_w, w);
    p.clip(Rect::new(x, y - 6.0, w, UNDER_LINE_H + 12.0));
    p.text(text.as_ptr(), x - off, y, TITLE_SZ, theme::TEXT_PRIMARY, 0, 1);
    p.text(text.as_ptr(), x - off + run_w + MARQUEE_GAP, y, TITLE_SZ, theme::TEXT_PRIMARY, 0, 1);
    p.clip_clear();
}

/// A tile's label block under its picture, whose unscaled bottom is `bottom`: the title (S19, 600,
/// white) and the subtitle (S16, 0.8 white) left aligned in the card's column, one line each, each
/// centred in its CSS line box — Poppins' cap band sits exactly where Chromium centres its content
/// area, so `text_vcenter_y` reproduces Litefin's placement. At rest a long title elides; the
/// `focused` one marquees.
fn draw_label_block(
    p: Painter,
    rect: Rect,
    sty: &RowStyle,
    label: &TileLabel,
    bottom: f32,
    focused: bool,
    measure: &dyn nj_machine::machine::Measure,
) {
    let (x, w) = place_label(p, rect, sty, label.settle_lag);
    let line = bottom + UNDER_DROP;
    if let Some(t) = &label.title {
        let y = nj_gfx::text::text_vcenter_y(TITLE_SZ, 1, line + UNDER_LINE_H * 0.5);
        if focused {
            title_marquee(p, x, w, t, y, measure);
        } else {
            let run = measure.fit_line(&t.to_string_lossy(), w, TITLE_SZ, true);
            p.text(run.as_ptr(), x, y, TITLE_SZ, theme::TEXT_PRIMARY, 0, 1);
        }
    }
    if let Some(c) = &label.caption {
        let mid = line + UNDER_LINE_H + UNDER_LINE_GAP + UNDER_CAPTION_H * 0.5;
        let y = nj_gfx::text::text_vcenter_y(CAPTION_SZ, 0, mid);
        let run = measure.fit_line(&c.to_string_lossy(), w, CAPTION_SZ, false);
        p.text(run.as_ptr(), x, y, CAPTION_SZ, theme::TEXT_SECONDARY, 0, 0);
    }
}

/// The card's progress bar: the bottom band of the picture itself, [`theme::RESUME_BAR_H`] tall
/// (`docs/litefin-look.md` 4.2 Progress). Delegates to [`crate::ui::widgets::progress_bar`], the ONE
/// bar the whole app draws, whose doc carries the pixel-level rules.
pub(crate) fn resume_bar(p: Painter, r: Rect, frac: f32, rad: f32) {
    crate::ui::widgets::progress_bar(p, r, rad, theme::RESUME_BAR_H, frac);
}

// ---------------------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    /// **A card's label is Litefin's** (`docs/litefin-look.md` 4.2 Labels): its title white at 600
    /// and its subtitle at the 0.8 white, both starting 1px inside the card's own leading edge,
    /// neither wider than the card, a long title elided at rest — and the subtitle a line below.
    #[test]
    fn a_card_label_is_left_aligned_inside_its_card_at_litefin_s_type() {
        use crate::ui::draw_census::capture_runs;
        use nj_machine::machine::Measure;
        let _serial = nj_base::testlock::serial();
        let measure = &nj_base::fontcov::advances::ShippedMeasure;
        let film = TileFacts { title: "Everything Everywhere All at Once", year: 2022, ..Default::default() };
        let label = tile_label(&film, false);
        for x in [CARD_LEFT, 700.0] {
            let card = Rect::new(x, 300.0, CARD_W, CARD_H);
            let drawn = capture_runs(|| draw_label_block(Painter::recording(), card, &RowStyle::HOME, &label,
                card.y + card.h, false, measure));
            let [title, year] = &drawn[..] else { panic!("a title and a subtitle: {drawn:?}") };
            assert_eq!((title.size, title.bold, title.ink), (TITLE_SZ, true, theme::TEXT_PRIMARY));
            assert_eq!((year.text.as_str(), year.size, year.bold, year.ink),
                ("2022", CAPTION_SZ, false, theme::TEXT_SECONDARY));
            for run in [title, year] {
                assert_eq!(run.at.x, x + LABEL_INSET, "left aligned on its card");
                assert!(measure.width_str(&run.text, run.size, run.bold) <= CARD_W - 2.0 * LABEL_INSET + 0.01);
            }
            assert!(title.text.ends_with('\u{2026}'), "a long title elides at rest: {}", title.text);
            assert!(year.at.y > title.at.y + UNDER_LINE_H - 1.0, "the subtitle is the next line");
        }
    }

    /// A centring row rests its focused card's centre on `centre_x` and clamps at both ends: the
    /// first cards never scroll the row right of its start, the last never past its end, and a row
    /// that fits never scrolls at all. Its end is Litefin's (`docs/litefin-look.md` 4.2): the track
    /// runs x 160..1860 inside the page beside the rail, so the last picture rests with its right
    /// edge at 1842, the first's left inset mirrored about the centre.
    #[test]
    fn a_centring_row_centres_its_focus_and_clamps_at_both_ends() {
        let sty = RowStyle { centre_x: Some(1010.0), ..RowStyle::HOME };
        let mut row = CardRow::new();
        let pitch = sty.w + sty.gap;
        let end = SCR_W - 60.0 - 12.0 - CARD_INSET;
        let max = sty.margin_x + 12.0 * pitch - sty.gap - end;
        let centre = |row: &CardRow, f: usize| sty.margin_x + f as f32 * pitch - row.scroll_target(f, 12, &sty) + sty.w * 0.5;
        assert_eq!(row.scroll_target(0, 12, &sty), 0.0, "the first card holds the row at its start");
        assert_eq!(row.scroll_target(11, 12, &sty), max, "the last card holds the row at its end");
        assert_eq!(end, 1842.0);
        assert!((centre(&row, 5) - 1010.0).abs() < 1e-3, "a middle card rests centred: {}", centre(&row, 5));
        for f in 0..12 {
            let t = row.scroll_target(f, 12, &sty);
            assert!((0.0..=max).contains(&t), "focus {f}: {t}");
        }
        for f in 0..3 {
            assert_eq!(row.scroll_target(f, 3, &sty), 0.0, "a row that fits never scrolls");
        }
        // My Media's four libraries, 1660 wide, fit Litefin's 1664 and never scroll
        let landscape = RowStyle { centre_x: Some(1010.0), ..RowStyle::EPISODE };
        for f in 0..4 {
            assert_eq!(row.scroll_target(f, 4, &landscape), 0.0, "focus {f} of four libraries");
        }
        // every preset keeps the minimal reveal
        let minimal = RowStyle::HOME;
        assert!(minimal.centre_x.is_none() && RowStyle::EPISODE.centre_x.is_none());
        assert_eq!(row.scroll_target(5, 12, &minimal), scroll_into_view(0.0, 5, 12, minimal.w, minimal.gap,
            SCR_W - 2.0 * minimal.margin_x));
        // the pointer's glide and a restored viewport stop at the same end
        row.restore_scroll(1.0e6, 12, &sty);
        assert_eq!(row.scroll_x(), max, "a restore clamps to the row's end");
        row.glide(1.0e6, 12, &sty);
        assert_eq!(row.scroll_x(), max, "a glide stops at the row's end");
    }

    /// The four rows of [`tile_label`]'s table, and the empty fields that leave a line out.
    #[test]
    fn a_catalog_card_is_worded_by_its_kind_and_shape() {
        let words = |m: TileFacts<'_>, landscape: bool| {
            let l = tile_label(&m, landscape);
            let s = |c: Option<std::ffi::CString>| c.map(|c| c.into_string().unwrap());
            (s(l.title), s(l.caption))
        };
        let some = |s: &str| Some(s.to_string());
        let film = TileFacts { title: "Sintel", year: 2010, ..Default::default() };
        assert_eq!(words(film, false), (some("Sintel"), some("2010")));
        assert_eq!(words(TileFacts { year: 0, ..film }, false), (some("Sintel"), None), "undated: no subtitle");
        let show = TileFacts { kind: TileKind::Show, title: "Caminandes", year: 2013, ..Default::default() };
        assert_eq!(words(show, false), (some("Caminandes"), some("2013")));
        let season = TileFacts { kind: TileKind::Season, title: "Season 1", show_title: "Caminandes",
            season_index: 1, ..Default::default() };
        assert_eq!(words(season, false), (some("Caminandes"), some("Season 1")));
        let episode = TileFacts { kind: TileKind::Episode, title: "Llamigos", show_title: "Caminandes",
            season_index: 1, ep_index: 3, ..Default::default() };
        assert_eq!(words(episode, true), (some("Caminandes"), some("Llamigos")), "landscape: the show, then the episode");
        assert_eq!(words(episode, false), (some("Llamigos"), None), "a poster: the episode; its code is the badge");
        assert_eq!(words(TileFacts { show_title: "", ..episode }, true), (some("Llamigos"), None));
        // the label a card's own art lends it follows the art's shape
        assert_eq!(art_label(&Art::Still(Some(episode))).unwrap().title.unwrap().to_str().unwrap(), "Caminandes");
        assert!(art_label(&Art::Poster(None)).is_none());
        assert!(art_label(&Art::Thumb { sid: 0, key: "k", res: (1, 1) }).is_none());
        // a credit: an actor "as" the part, a crew member the job, a person page's film its year too
        let _serial = nj_base::testlock::serial();
        let _lang = nj_platform::i18n::language_on_this_thread_for_test(nj_platform::i18n::Preference::En);
        let actor = TileFacts { kind: TileKind::Person, title: "Derek de Lint", role: "Thom", ..Default::default() };
        assert_eq!(words(actor, false), (some("Derek de Lint"), some("as Thom")));
        assert_eq!(words(TileFacts { role: "", ..actor }, false), (some("Derek de Lint"), None));
        let crew = TileFacts { kind: TileKind::Crew, title: "Ian Hubert", role: "Director", ..Default::default() };
        assert_eq!(words(crew, false), (some("Ian Hubert"), some("Director")));
        assert_eq!(words(TileFacts { role: "Thom", ..film }, false), (some("Sintel"), some("2010 · as Thom")));
    }

    /// The focused card's title runs as a marquee rather than an ellipsis once it overflows its card;
    /// a title that fits is drawn once, plainly, focused or not.
    #[test]
    fn a_focused_overflowing_title_marquees_and_a_resting_one_elides() {
        use crate::ui::draw_census::capture_runs;
        let _serial = nj_base::testlock::serial();
        let measure = &nj_base::fontcov::advances::ShippedMeasure;
        let card = Rect::new(CARD_LEFT, 300.0, CARD_W, CARD_H);
        let draw = |title: &str, focused: bool| capture_runs(|| draw_label_block(Painter::recording(), card,
            &RowStyle::HOME, &TileLabel::title(title), card.y + card.h, focused, measure));
        let long = "Everything Everywhere All at Once";
        let focused = draw(long, true);
        assert_eq!(focused.len(), 2, "the run and its follower");
        assert!(focused.iter().all(|r| r.text == long), "a marquee draws the whole title");
        let resting = draw(long, false);
        assert_eq!(resting.len(), 1);
        assert_ne!(resting[0].text, long);
        for focused in [true, false] {
            let short = draw("Up", focused);
            assert_eq!((short.len(), short[0].text.as_str()), (1, "Up"));
        }
    }

    #[test]
    fn discovery_and_text_recording_keep_visible_cards_when_gl_is_suppressed() {
        use crate::ui::frame::backdrop::{self, Sources};
        use std::{cell::RefCell, rc::Rc};
        let _guard = nj_base::testlock::serial();
        let sources = Rc::new(RefCell::new(Sources::default()));
        sources.borrow_mut().begin(vec![]);
        let _walk = backdrop::discover(sources);
        let card = super::Rect::new(100.0, 200.0, 400.0, 220.0);
        assert!(nj_gfx::gfx::culled(card.x, card.y, card.w, card.h));
        assert!(super::paint_visible(super::Painter::root(), card, 1.0, true));
        assert!(super::paint_visible(super::Painter::recording(), card, 1.0, true));
        let offscreen = super::Rect::new(100.0, -500.0, 400.0, 220.0);
        assert!(!super::paint_visible(super::Painter::root(), offscreen, 1.0, true));
        assert!(super::paint_visible(super::Painter::recording(), offscreen, 1.0, true));
    }

    #[test]
    fn card_paint_culling_keeps_the_ring_and_the_pressed_caption_at_viewport_edges() {
        let _guard = nj_base::testlock::serial();
        let p = super::Painter::root();
        let ring_only = super::Rect::new(100.0, -222.0, 400.0, 220.0);
        assert!(super::paint_visible(p, ring_only, 1.0, false), "the focus ring reaches the panel");
        let label_only = super::Rect::new(100.0, -260.0, 400.0, 220.0);
        assert!(!super::paint_visible(p, label_only, 1.0, false));
        assert!(super::paint_visible(p, label_only, 0.94, true), "a pressed card's label remains visible");
        let far_above = super::Rect::new(100.0, -500.0, 400.0, 220.0);
        assert!(!super::paint_visible(p, far_above, 0.94, true));
        assert!(super::paint_visible(p.translate(0.0, 400.0), far_above, 0.94, true));
        let below = super::Rect::new(100.0, super::SCR_H + 6.0, 400.0, 220.0);
        assert!(!super::paint_visible(p, below, 1.0, true), "no shadow pulls a card below the panel back in");
    }

    use super::*;

    fn motion_hash(row: &CardRow) -> u64 {
        let mut canon = nj_machine::machine::Canon::new();
        row.write_motion(&mut canon);
        canon.finish()
    }

    #[test]
    fn a_fresh_row_is_at_exact_rest_and_update_is_a_fixed_point() {
        let mut row = CardRow::new();
        assert!(row.at_exact_rest());
        let before = motion_hash(&row);
        row.update(24, None, &RowStyle::HOME, 1.0 / 60.0);
        assert_eq!(motion_hash(&row), before);
        row.update(0, None, &RowStyle::HOME, 1.0 / 60.0);
        assert_eq!(motion_hash(&row), before);
    }

    /// FINDING: this FAILS, so a row that has held focus never parks and Home's skip only helps
    /// rows never visited. After 30 focused frames and 1,200 released ones at dt = 1/60 (still
    /// stuck after 20,000): the focused cell's `scale` sits at pos 1.0000001, vel -9.8e-7 (one ulp
    /// above rest with a velocity too small to move it, a fixed point of `step` that is not rest).
    /// `lift` and `overflow` do land exactly. Parking a visited row needs a snap at rest; kept
    /// ignored until that lands.
    #[test]
    #[ignore = "a released row stalls one ulp from rest (scale)"]
    fn a_released_row_reaches_exact_rest() {
        let dt = 1.0 / 60.0;
        let mut row = CardRow::new();
        for _ in 0..30 {
            row.update(24, Some(3), &RowStyle::HOME, dt);
        }
        assert!(!row.at_exact_rest());
        for _ in 0..1200 {
            row.update(24, None, &RowStyle::HOME, dt);
        }
        assert!(row.at_exact_rest());
    }

    /// **The pointer's sideways glide moves the row and leaves it there.** A glided row does not
    /// steer back to its focused cell until `unfree`, and the glide never leaves the row's range.
    #[test]
    fn a_glided_row_stays_where_the_glide_put_it_until_it_is_released() {
        let dt = 1.0 / 60.0;
        let sty = &RowStyle::HOME;
        let mut row = CardRow::new();
        for _ in 0..60 { row.update(24, Some(0), sty, dt); }
        assert_eq!(row.scroll_x(), 0.0, "premise: the first cell needs no scroll");
        for _ in 0..30 { row.glide(20.0, 24, sty); }
        assert_eq!(row.scroll_x(), 600.0, "the glide moves the row by what it is handed");
        for _ in 0..120 { row.update(24, Some(0), sty, dt); }
        assert_eq!(row.scroll_x(), 600.0, "a glided row does not steer back to its focused cell");
        row.unfree();
        for _ in 0..240 { row.update(24, Some(0), sty, dt); }
        assert!(row.scroll_x() < 1.0, "released, it brings its focused cell back: {}", row.scroll_x());
        row.glide(1.0e6, 24, sty);
        let end = row.scroll_x();
        assert!(end > 600.0 && end < 1.0e6, "never past the row's end: {end}");
        row.glide(-1.0e6, 24, sty);
        assert_eq!(row.scroll_x(), 0.0, "nor before its start");
    }

    #[test]
    fn restored_scroll_is_retained_and_clamped_to_current_content() {
        let mut row = CardRow::new();
        row.restore_scroll(900.0, 24, &RowStyle::HOME);
        assert_eq!(row.scroll_x(), 900.0);
        assert_eq!(row.scroll_x.vel, 0.0);
        row.restore_scroll(900.0, 2, &RowStyle::HOME);
        assert_eq!(row.scroll_x(), 0.0);
        row.restore_scroll(-50.0, 24, &RowStyle::HOME);
        assert_eq!(row.scroll_x(), 0.0);
    }

    /// Every placement is card-anchored: the block starts [`LABEL_INSET`] inside the card's own
    /// leading edge whatever the scroll or the rail reserve, is the card's width less both insets
    /// where the panel has room, and never runs past the panel (or the rail) on the right.
    #[test]
    fn label_blocks_lead_from_their_card_and_stop_at_the_panel() {
        for dx in [0.0, -290.0, -2900.0, 240.0] {
            let p = Painter::root().translate(dx, 0.0);
            for reserve in [0.0, 112.0] {
                let sty = RowStyle::HOME.with_right_reserve(reserve);
                for screen_x in [-120.0, CARD_LEFT, 700.0, SCR_W - MARGIN_X - CARD_W, SCR_W - CARD_W] {
                    let rect = Rect::new(screen_x - dx, 300.0, sty.w, sty.h);
                    for s in [1.0, sty.focus_scale] {
                        let (x, w) = place_label(p, rect.scaled(s), &sty, 0.0);
                        assert!((x - (rect.x + LABEL_INSET)).abs() < 1e-3, "the block leads from its unscaled card");
                        assert!(w <= sty.w - 2.0 * LABEL_INSET, "never wider than its card");
                        assert!(x + dx + w <= SCR_W - reserve - EDGE_PAD + 0.01, "and never past the panel");
                    }
                }
            }
        }
        let mid = Rect::new(700.0, 300.0, CARD_W, CARD_H);
        assert_eq!(label_band(Painter::root(), mid, &RowStyle::HOME), (mid.x + LABEL_INSET, CARD_W - 2.0 * LABEL_INSET));
    }

    const DT: f32 = 1.0 / 60.0;

    /// The full clearance a HOME shelf's heading needs over a popped tile — asked of
    /// [`heading_lift_max`] rather than restating its three terms, because a screen now RESERVES
    /// this same number in its layout (the home grid's `row_reveal_band`) and a test that re-derived it
    /// would keep passing while the two drifted.
    fn full() -> f32 {
        heading_lift_max(&RowStyle::HOME)
    }

    // The lattice every shelf in the app is drawn on: 250-wide tiles, 96px left margin.
    const ADV: f32 = CARD_W + 40.0;

    /// **Vertical focus follows the SCREEN, not the array index.** Two shelves hold independent
    /// horizontal scrolls, so carrying the index across lands focus on a tile that can be most of a
    /// screen away — and the destination row then scrolls hard to reveal it.
    #[test]
    fn a_vertical_step_lands_under_the_tile_you_were_looking_at() {
        // the source row is scrolled four tiles along and focus is on its tile 6, which is
        // therefore drawn at column 2 on screen
        let src_x = tile_centre_x(6, MARGIN_X, ADV, CARD_W, 4.0 * ADV);
        assert_eq!(src_x, tile_centre_x(2, MARGIN_X, ADV, CARD_W, 0.0));

        // an UNSCROLLED destination row: the tile under the cursor is its tile 2, not its tile 6
        assert_eq!(column_near_x(src_x, MARGIN_X, ADV, CARD_W, 0.0, 20, 6), 2);
        // …and a destination scrolled the SAME amount keeps the index, which is the case the old
        // index-carrying rule got right and is why it survived so long
        assert_eq!(column_near_x(src_x, MARGIN_X, ADV, CARD_W, 4.0 * ADV, 20, 6), 6);

        // a destination scrolled FURTHER than the source: the visual column is preserved again
        assert_eq!(column_near_x(src_x, MARGIN_X, ADV, CARD_W, 9.0 * ADV, 20, 6), 11);
    }

    /// UP → DOWN → UP must land back where it started, or the page walks sideways under a user
    /// doing nothing but bouncing between two rows.
    #[test]
    fn bouncing_between_two_rows_is_stable() {
        // a half-tile offset between the rows, the worst case for a rounding rule
        let (a, b) = (3.0 * ADV, 3.0 * ADV + ADV * 0.5);
        let mut i = 7usize;
        let first =
            column_near_x(tile_centre_x(i, MARGIN_X, ADV, CARD_W, a), MARGIN_X, ADV, CARD_W, b, 20, i);
        let mut seen = vec![i, first];
        for step in 0..6 {
            let (from_s, to_s) = if step % 2 == 0 { (b, a) } else { (a, b) };
            let cur = *seen.last().unwrap();
            let x = tile_centre_x(cur, MARGIN_X, ADV, CARD_W, from_s);
            i = column_near_x(x, MARGIN_X, ADV, CARD_W, to_s, 20, cur);
            seen.push(i);
        }
        // it settles into a two-cycle rather than drifting: every even entry is the same index and
        // so is every odd one
        assert!(seen.iter().step_by(2).all(|v| *v == seen[0]), "drifted: {seen:?}");
        assert!(seen[1..].iter().step_by(2).all(|v| *v == seen[1]), "drifted: {seen:?}");
    }

    /// A SHORTER destination row clamps to its last tile — the one case where the destination
    /// genuinely has nothing under the cursor and must scroll.
    #[test]
    fn a_shorter_row_clamps_to_its_last_tile() {
        let x = tile_centre_x(9, MARGIN_X, ADV, CARD_W, 0.0);
        assert_eq!(column_near_x(x, MARGIN_X, ADV, CARD_W, 0.0, 4, 9), 3);
        assert_eq!(column_near_x(x, MARGIN_X, ADV, CARD_W, 0.0, 1, 9), 0);
        // an EMPTY row answers 0 rather than panicking on `n - 1`
        assert_eq!(column_near_x(x, MARGIN_X, ADV, CARD_W, 0.0, 0, 9), 0);
    }

    /// A source tile drawn LEFT of the destination row's origin rounds to 0 rather than wrapping.
    /// `floor` after the +0.5 is what makes that true: a cast truncates toward zero, so -0.8 + 0.5
    /// became 0 by accident on one side of the axis and would have become -1 on the other.
    #[test]
    fn a_tile_left_of_the_origin_lands_on_the_first() {
        let x = tile_centre_x(0, MARGIN_X, ADV, CARD_W, 2.0 * ADV); // off the left edge
        assert_eq!(column_near_x(x, MARGIN_X, ADV, CARD_W, 0.0, 20, 0), 0);
    }

    fn run(row: &mut CardRow, frames: usize, focused: Option<usize>, sty: &RowStyle) {
        for _ in 0..frames {
            row.update(8, focused, sty, DT);
        }
    }

    /// Regression, two defects in one: the heading clearance used to be `live_scale - 1`, i.e. a
    /// readout of the focus-pop spring. So (a) the frame focus moved, the incoming cell's spring
    /// was still at rest and the heading dropped its ENTIRE lift in one frame before springing
    /// back, and (b) between those it rode the pop up and down. It is a held destination now:
    /// walking slots 0 → 1 must leave the heading exactly where it is, all the way through.
    #[test]
    fn the_heading_holds_still_while_focus_walks_the_slots_beneath_it() {
        let sty = RowStyle::HOME;
        let mut row = CardRow::new();
        run(&mut row, 180, Some(0), &sty);
        let held = row.lift();
        assert!(
            (held - full()).abs() < 0.1,
            "slot 0 must hold the full clearance ({held} vs {})",
            full()
        );

        // slot 1 is also under the heading, so nothing should move — not by a pixel, not for a frame
        for f in 0..180 {
            row.update(8, Some(1), &sty, DT);
            assert!(
                (row.lift() - held).abs() < 0.1,
                "heading moved {:.2}px at frame {f} while focus stayed under it",
                row.lift() - held
            );
        }
    }

    /// And when the popped tile really does leave, the heading settles back — once, smoothly, with
    /// no frame moving it more than a spring step (the defect moved ~16px in a single frame).
    #[test]
    fn the_heading_settles_back_smoothly_once_nothing_is_under_it() {
        let sty = RowStyle::HOME;
        let mut row = CardRow::new();
        run(&mut row, 180, Some(0), &sty);
        let mut prev = row.lift();
        for _ in 0..240 {
            row.update(8, Some(5), &sty, DT); // far down the row — no clearance needed
            let l = row.lift();
            assert!(
                (l - prev).abs() < 3.0,
                "heading jumped {:.1}px in one frame",
                l - prev
            );
            prev = l;
        }
        assert!(
            prev < 0.5,
            "the heading must come all the way back down (got {prev})"
        );
    }

    /// A row that owns no focus rests flat — including a row that just LOST focus, which used to
    /// snap its heading back the same way.
    #[test]
    fn an_unfocused_row_lifts_its_heading_by_nothing() {
        let sty = RowStyle::HOME;
        let mut row = CardRow::new();
        run(&mut row, 180, Some(0), &sty);
        run(&mut row, 240, None, &sty);
        assert!(row.lift() < 0.5);
    }

    /// The clearance tapers with distance from the heading: slots 0 and 1 sit under it and get the
    /// whole thing, slot 2 is on the ramp, and a tile far down the row leaves it alone.
    #[test]
    fn the_clearance_tapers_as_the_popped_tile_moves_away_from_the_heading() {
        let sty = RowStyle::HOME;
        let at = |c: usize| heading_clearance(Some(c), &sty, 0.0);
        assert_eq!(at(0), at(1), "the first two slots share the full clearance");
        assert!((at(0) - full()).abs() < 0.001);
        assert!(
            at(2) > 0.5 && at(2) < at(1),
            "slot 2 is on the taper ({} vs {})",
            at(2),
            at(1)
        );
        assert_eq!(at(4), 0.0, "a tile far from the heading must not move it");
        assert_eq!(heading_clearance(None, &sty, 0.0), 0.0);
    }

    /// A row LONGER than the spring array. Every focus treatment a tile gets — the pop, the lifted
    /// shadow, the ring — is derived from `scale(i)`, and the index used to be clamped into the
    /// array, so focusing tile 30 of a 60-name cast row popped nothing at all (it read cell 23's
    /// resting spring). The shared overflow spring must pop for the focused tile, and for it alone:
    /// its neighbours read the same spring.
    #[test]
    fn a_focused_tile_past_the_spring_array_still_pops_and_its_neighbours_do_not() {
        let sty = RowStyle::HOME;
        let mut row = CardRow::new();
        let far = MAX_ROW_ITEMS + 6; // a crew credit at the end of a long cast list
        for _ in 0..180 {
            row.update(far + 2, Some(far), &sty, DT);
        }
        assert!(
            (row.scale(far) - sty.focus_scale).abs() < 0.01,
            "the focused overflow tile must reach the focus scale (got {})",
            row.scale(far)
        );
        assert_eq!(
            row.scale(far + 1),
            1.0,
            "the tile after it shares the spring, not the focus"
        );
        assert_eq!(row.scale(far - 1), 1.0, "nor the one before it");
        assert!(
            (row.scale(MAX_ROW_ITEMS - 1) - 1.0).abs() < 0.01,
            "and the last in-array cell rests"
        );

        // focus coming back inside the array releases the overflow pop
        for _ in 0..180 {
            row.update(far + 2, Some(0), &sty, DT);
        }
        assert_eq!(
            row.scale(far),
            1.0,
            "focus left the tail, so no overflow tile reads the spring"
        );
        assert!(
            (row.scale(0) - sty.focus_scale).abs() < 0.01,
            "and the in-array cell pops as ever"
        );
    }

    // ---- item 10: the focused title marquee -------------------------------------------------

    /// A run that already fits the budget never moves and never reports — the case that must cost
    /// the idle present gate nothing.
    #[test]
    fn a_title_that_fits_the_budget_never_moves() {
        assert_eq!(marquee_x(0.0, 100.0, 300.0), 0.0);
        assert_eq!(marquee_x(50_000.0, 100.0, 300.0), 0.0);
        assert!(!marquee_moving(0.0, 100.0, 300.0));
        assert!(!marquee_moving(50_000.0, 100.0, 300.0));
    }

    /// An over-wide run sits at rest for the whole hold beat: `t_ms < MARQUEE_HOLD_MS` must read
    /// zero offset and report no motion, so a title lands and holds still long enough to start
    /// reading before anything moves.
    #[test]
    fn an_overflowing_title_rests_before_it_glides() {
        let (w, budget) = (500.0, 300.0);
        assert_eq!(marquee_x(0.0, w, budget), 0.0);
        assert_eq!(marquee_x(MARQUEE_HOLD_MS - 1.0, w, budget), 0.0);
        assert!(!marquee_moving(0.0, w, budget));
        assert!(!marquee_moving(MARQUEE_HOLD_MS - 1.0, w, budget));
    }

    /// Past the hold beat it glides at the documented speed and reports motion — and the offset is
    /// monotone through the glide (no snap-back mid-cycle).
    #[test]
    fn an_overflowing_title_glides_at_the_documented_speed_and_reports_motion() {
        let (w, budget) = (500.0, 300.0);
        assert!(marquee_moving(MARQUEE_HOLD_MS + 1.0, w, budget));
        let a = marquee_x(MARQUEE_HOLD_MS + 100.0, w, budget);
        let b = marquee_x(MARQUEE_HOLD_MS + 600.0, w, budget);
        assert!(b > a, "the run must keep moving left through the glide");
        // 500ms of glide at 40px/s = 20px
        assert!((b - a - 20.0).abs() < 0.01, "got {} vs expected 20", b - a);
    }

    /// The cycle LOOPS: a full period (hold + the whole glide) must land back at the rest offset,
    /// exactly like the first cycle did — a marquee that drifted cycle to cycle would slowly
    /// desync its two drawn copies from the clip window.
    #[test]
    fn the_marquee_loops_cleanly() {
        let (w, budget) = (500.0, 300.0);
        let travel = w + MARQUEE_GAP;
        let glide_ms = travel / MARQUEE_SPEED * 1000.0;
        let period = MARQUEE_HOLD_MS + glide_ms;
        assert_eq!(marquee_x(period, w, budget), marquee_x(0.0, w, budget));
        assert_eq!(
            marquee_x(period + 50.0, w, budget),
            marquee_x(50.0, w, budget)
        );
        // just before the wrap the run has travelled (almost) the full distance
        let just_before = marquee_x(period - 0.001, w, budget);
        assert!(
            (just_before - travel).abs() < 0.1,
            "got {just_before} vs travel {travel}"
        );
    }

    /// [`marquee_clock`] restarts at zero the instant the focused text changes, and keeps
    /// advancing by real elapsed time (read off [`nj_machine::idle::now_ms`]) while it stays the
    /// same. `frame_begin` stands in for the real frame loop's own per-frame call, advancing the
    /// same clock `title_marquee` reads in production — nothing about `marquee_clock` itself is
    /// untestable now that it reads an absolute snapshot instead of summing a `dt` of its own.
    #[test]
    fn the_marquee_clock_restarts_when_the_focused_text_changes() {
        MARQUEE_KEY.with(|k| k.borrow_mut().clear());
        nj_machine::idle::frame_begin(0.0);
        assert_eq!(marquee_clock("Alpha"), 0.0, "first sight of a title starts at 0");
        nj_machine::idle::frame_begin(1.0 / 60.0);
        let t1 = marquee_clock("Alpha");
        assert!(t1 > 0.0, "the clock must advance while the title holds focus");
        nj_machine::idle::frame_begin(1.0 / 60.0);
        let t2 = marquee_clock("Alpha");
        assert!(t2 > t1, "and keep advancing frame over frame");
        assert_eq!(
            marquee_clock("Beta"),
            0.0,
            "a different focused title restarts the clock at 0"
        );
    }

    // ---- word-wrap is gone: the focused title is always exactly one line ---------------------

    /// [`TileLabel::height`] no longer varies with the row style at all — a shelf of long titles
    /// marquees in place rather than growing its label band, so every row in the app reserves the
    /// same fixed height for the same `caption` choice.
    #[test]
    fn the_label_height_is_fixed_regardless_of_row_style() {
        assert_eq!(TileLabel::height(true), UNDER_LABEL_H);
        assert!(TileLabel::height(false) < TileLabel::height(true));
    }

    /// [`TileLabel::own_height`] agrees with the free function for a label that carries whatever
    /// it carries.
    #[test]
    fn a_label_s_own_height_matches_its_caption() {
        let with_caption = TileLabel::titled("Title", "Caption");
        let without = TileLabel::title("Title");
        assert_eq!(with_caption.own_height(), TileLabel::height(true));
        assert_eq!(without.own_height(), TileLabel::height(false));
    }

    // ---- The label band: every card wears its label ---------------------------------------------

    /// No shelf collapses: the band is the whole label block at every expansion, so the room under
    /// a card always holds it, and a shelf's card bottom is Litefin's label block plus its air above
    /// the next heading's cap top (`docs/litefin-look.md` 4.2: ring 3, padding 12, a 28.75 title
    /// line, 6, a 24 subtitle line, then the border 3, 38, 28 and the heading's 13).
    #[test]
    fn every_shelf_reserves_its_whole_label_block() {
        for e in [-0.2, 0.0, 0.5, 1.0, 1.4] {
            assert_eq!(under_band(e), UNDER_LABEL_H);
            assert_eq!(band_reveal(e), 1.0, "the label is on screen at every expansion");
        }
        assert_eq!(BAND_OPEN, 0.0);
        assert_eq!(UNDER_LABEL_H, 73.75);
        use crate::ui::consts::{CARD_DY, CARD_H, TITLE_DY, UNDER_LABEL_AIR};
        let pitch = ROW_PITCH_FIXED + under_band(0.0);
        let card_bottom = CARD_DY + CARD_H;
        assert!((pitch - TITLE_DY - card_bottom - (UNDER_LABEL_H + UNDER_LABEL_AIR)).abs() < 1e-3);
        assert_eq!(pitch, 585.75, "Litefin's classic poster shelf, heading to heading");
        let row = CardRow::new();
        assert_eq!((row.under_band(), row.band_reveal()), (UNDER_LABEL_H, 1.0));
    }

    /// **The animator obligation** (`ui/CLAUDE.md`): a row must report to the frame gate while its
    /// focused card pops and stop reporting once it has arrived, and the same on release.
    #[test]
    fn a_row_reports_while_its_card_pops_and_goes_quiet_once_it_has() {
        let _g = nj_base::testlock::serial();
        let mut row = CardRow::new();
        let sty = RowStyle::HOME;
        let (_, moved) = nj_machine::idle::scoped_motion(|| row.update(6, Some(1), &sty, DT));
        assert!(moved, "a popping card must wake the present gate");
        for _ in 0..240 { row.update(6, Some(1), &sty, DT); }
        assert!((row.scale(1) - sty.focus_scale).abs() < 1e-4);
        let (_, still) = nj_machine::idle::scoped_motion(|| row.update(6, Some(1), &sty, DT));
        assert!(!still, "a settled focused card must not keep the panel awake");
        let (_, moved) = nj_machine::idle::scoped_motion(|| row.update(6, Some(2), &sty, DT));
        assert!(moved, "the hand-over pops the next card");
        for _ in 0..240 { row.update(6, Some(2), &sty, DT); }
        let (_, still) = nj_machine::idle::scoped_motion(|| row.update(6, Some(2), &sty, DT));
        assert!(!still, "and the row rests again");
    }

    /// The label block's width is decided ONCE per focus move, not re-decided on every frame of the
    /// scroll glide: judged where the tile will rest, a card gliding in from the panel's right edge
    /// keeps one width (and one elision) all the way.
    #[test]
    fn a_label_block_keeps_its_width_through_the_scroll_glide() {
        for sty in [RowStyle::HOME, RowStyle::EPISODE] {
            let n = 14;
            let pitch = sty.w + sty.gap;
            for from in 0..n - 1 {
                let mut row = CardRow::new();
                for _ in 0..600 {
                    row.update(n, Some(from), &sty, DT);
                }
                let to = from + 1;
                let rect = Rect::new(sty.margin_x + to as f32 * pitch, 300.0, sty.w, sty.h);
                let mut seen = Vec::new();
                for _ in 0..120 {
                    row.update(n, Some(to), &sty, DT);
                    let p = Painter::root().translate(-row.scroll_x(), 0.0);
                    seen.push(place_label(p, rect, &sty, row.settle_lag(n, to, &sty)).1);
                }
                let settled = *seen.last().unwrap();
                assert!(
                    seen.iter().all(|w| *w == settled),
                    "{}x{} → col {to}: width changed mid-glide: {seen:?}",
                    sty.w, sty.h,
                );
            }
        }
    }
}

#[cfg(test)]
mod exact_rest_regressions {
    use super::*;
    #[test]
    fn a_released_card_lands_exactly_while_the_row_keeps_focus() {
        let dt = 1.0 / 60.0;
        for style in [RowStyle::HOME, RowStyle::EPISODE, RowStyle::PROFILES] {
            let mut row = CardRow::new();
            for _ in 0..60 { row.update(12, Some(3), &style, dt); }
            assert!(row.scale(3) > 1.0);
            for _ in 0..120 { row.update(12, Some(4), &style, dt); }
            assert_eq!(row.scale(3), 1.0, "a released cell must leave the focus shader exactly");
        }
    }
}
