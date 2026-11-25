//! Reusable retui leaves + shared helpers. These are the "reusable UI elements":
//! Button, CircleButton, TabPill, TransportButton, PageDots, Badge, plus the shared art-card
//! core (`card`/`draw_card`) and the poster-resolve helper. (Multi-line text wrapping
//! now lives in the `TextView` primitive in `text_view.rs`.)
use crate::ui::label::{HAlign, Label, VAlign};
use crate::ui::text_view::TextView;
use crate::ui::theme;
use crate::ui::tile::{TileFacts, TileKind};
use crate::ui::{Env, Painter, Rect, Spring, View};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

/// A small set of leaves in this file have no `Measure` in their draw call chain: generic retui
/// `View::draw(&self, env: &Env, p: Painter)` leaves (`TabPill`, `Button`) take no capability, and
/// tab paint receives captured labels but no measurement capability. These leaves can also be
/// exercised by host tests before a font loads, where `TtfMeasure`'s boot-order assertion would be
/// a false alarm. This wraps the same free functions without that assertion. Application
/// vocabulary and profile data are always supplied explicitly by their owner.
///
/// `pub(crate)`, not private: `ui::mod`'s recording `Painter` (the text-prewarm layout pass run
/// ahead of a page push, spec's warming turn) is the same shape of leaf — `Painter` is `Copy` and
/// threaded through hundreds of draw calls with no room to grow a capability field — and reaches
/// for this rather than a second, parallel `impl Measure for` that would only teach the
/// `textmeasure` gate to look somewhere new for the exact call it already forbids.
pub(crate) struct LegacyMeasure;

impl LegacyMeasure {
    pub(crate) fn bounds(&self, s: &CStr, sz: c_int, bold: bool) -> (f32,f32) {
        nj_gfx::text::text_bounds(s.as_ptr(),sz,bold as c_int)
    }
}

impl nj_machine::machine::Measure for LegacyMeasure {
    fn width(&self, s: &CStr, sz: c_int, bold: bool) -> f32 {
        nj_gfx::text::text_width(s.as_ptr(), sz, bold as c_int)
    }
    fn cap_h(&self, sz: c_int) -> f32 {
        nj_gfx::text::cap_h(sz, 0)
    }
    fn line_h(&self, sz: c_int) -> f32 {
        nj_gfx::text::text_height(sz, 0)
    }
    fn live_font(&self) -> bool {
        true
    }
}

// ---- backdrop glass -------------------------------------------------------------------------

/// Live glass declares its sampling rectangle through the ordinary painter. Source lifetime,
/// layering, geometric occlusion and damage are owned solely by `ui::frame::backdrop`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Glass;
impl Glass {
    pub(crate) const DYNAMIC_BACKDROP: Self = Self;

    /// Draw the captured backdrop only. The caller owns the material layered over it, which is
    /// what lets the same policy serve a frosted panel and the sheened tab-track capsule.
    #[must_use]
    pub(crate) fn backdrop(
        self,
        p: Painter,
        r: Rect,
        rest_dy: f32,
        radius: f32,
        tint: [f32; 4],
        rim: nj_gfx::gfx::GlassRim,
        face: nj_gfx::gfx::GlassFace,
        mat: theme::Material,
    ) -> bool {
        // A bare painter outside a frame cannot name an underlay. Never silently fall through
        // to the synthetic load dial's independent scratch-cache policy.
        if !crate::ui::frame::backdrop::active() || !nj_gfx::gfx::live_blur_available() { return false; }
        p.backdrop_blur(r, rest_dy, radius, tint, rim, face, mat.deep())
    }
}

/// **A person's avatar**: their initial, centred on a disc in their own tone
/// ([`theme::AVATAR_TONES`], picked by `id` so one person keeps one colour). The who's-watching
/// screen and *Add a user* draw people with it; a focused disc wears the focus rim around it.
pub(crate) fn avatar_disc(p: Painter, rect: Rect, name: &str, id: &str, focused: bool, size: i32) {
    let tone = theme::AVATAR_TONES[avatar_tone(id)];
    let r = rect.w.min(rect.h) * 0.5;
    p.rrect(rect, r, r, tone);
    if focused {
        p.rring(rect, r, 5.0, theme::CONTROL_RIM_FOCUS_UNKEYED);
    }
    let initial: String = name.trim().chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
    let text = CString::new(initial).unwrap_or_default();
    Label::new(text.as_ptr(), size, theme::TEXT_PRIMARY).bold().h(HAlign::Center).v(VAlign::Middle).draw(p, rect);
}

/// Which of [`theme::AVATAR_TONES`] belongs to the person `id` — a stable hash, never their
/// position in a list, so a list that reorders does not repaint anyone.
pub(crate) fn avatar_tone(id: &str) -> usize {
    let h = id.bytes().fold(2166136261u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619));
    h as usize % theme::AVATAR_TONES.len()
}

/// **A popover panel's GROUND — the page under it, carried into it, WHERE it is.**
///
/// Every popover in the app stands on this: the menus, the alert panels, the person bio, the
/// decision alert. It used to be a real backdrop blur of the host (`Glass::CACHED.panel`, and the
/// bio's `DYNAMIC_BACKDROP`) tinted by a fixed frost — ~11% of a frame's GPU cycles on the
/// account panel (`docs/backdrop-blur-profiling.md`), for a picture the frost then covered all but
/// 15% of. What survives a frost that dense is the page's COLOUR and where it is, which is exactly
/// what the underlay field already holds: the 15x8 grid the modal dim latched from the undimmed
/// page (`containers::modal::ModalUnderlay`). So the panel draws the field's own window at its
/// screen rect ([`crate::ui::underlay::UnderlayField::draw_panel`] — green under the panel's
/// bottom-left stays under its bottom-left), multiplied by `theme::underlay::PANEL_TINT` and held
/// under `PANEL_LUMA_MAX`, and then the SAME frost and the same rim the glass panel wore.
///
/// **The edge is not part of the material's identity**, and it did not change: the rim is
/// [`theme::GLASS_RIM`] with the boost to [`theme::GLASS_RIM_LIGHT`] on the side facing the light —
/// the standing track's two weights, the same lamp, the same one pixel — so a panel and the glass
/// bar above it still read as one family of object.
///
/// `field` is `None`, or not latched yet (the first frame, a refused sample, a panel over the video
/// plane): the flat near-opaque sheet ([`theme::PANEL_TOP`]/[`theme::PANEL_BOT`]) with the same rim.
/// Never a blank, and never the frost alone — [`theme::PANEL_FROST_TOP`] is only legal over the
/// field it is frosting.
///
/// **Glass is chrome-only now**: the top bar's standing track and the profile chip's capsule still
/// sample a live blur, because the page under them moves; nothing that is a popover does.
pub(crate) fn panel_ground(
    p: Painter,
    r: Rect,
    radius: f32,
    field: Option<&crate::ui::underlay::UnderlayField>,
) {
    let boost = theme::GLASS_RIM_LIGHT[3] - theme::GLASS_RIM[3];
    let weight = panel_tint_sweep().unwrap_or(theme::underlay::PANEL_TINT);
    let drew = crate::ui::profile::phase("panel.field", || {
        field.is_some_and(|f| f.draw_panel(p, r, radius, weight))
    });
    let (top, bot) = if drew {
        panel_frost()
    } else {
        (theme::PANEL_TOP, theme::PANEL_BOT)
    };
    crate::ui::profile::phase("panel.frost", || {
        p.rect_rimmed(r, radius, top, bot, theme::GLASS_RIM, boost);
    });
}

#[cfg(feature = "devtriggers")]
fn panel_tint_sweep() -> Option<f32> {
    static SEEN: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    *SEEN.get_or_init(|| {
        let v = nj_base::devtrig::read("paneltint")?.trim().parse::<f32>().ok()?;
        nj_base::eventlog::log(&format!("panel: field tint swept to {v}"));
        Some(v.clamp(0.0, 1.0))
    })
}
#[cfg(not(feature = "devtriggers"))]
fn panel_tint_sweep() -> Option<f32> {
    None
}

/// **What a popover is made of — both halves, from one name.** See [`theme::Material`].
///
/// A panel is HEAVIER and SOFTER than the bar on purpose, and both halves come from the same place
/// so they cannot drift apart. The reference is unambiguous: in one screenshot of iOS 26's TV app
/// the posters behind the tab BAR are readable and behind the context MENU above it almost nothing
/// is. A menu is a surface you read and act on; a bar is chrome you look past.
///
/// Laddered on the television over one ground (.72/.55/.42/.30 frost): at .42 the menu's own rows go
/// soft against the page coming through, and by .30 the panel has stopped reading as a surface. So
/// the density does not come DOWN toward the bar, which is the obvious "unification" and the wrong
/// move; what changed instead is that the panel now also gets the extra sample the bar declines,
/// because lightening the shared snapshot for the bar had lightened the menus with it.
///
/// `/tmp/nativejelly-material=<ultrathin|thin|regular|thick|ultrathick>` swaps the whole material,
/// and `/tmp/nativejelly-panelfrost=<a>` still overrides the density alone for a finer sweep.
fn panel_material() -> theme::Material {
    material_sweep().unwrap_or(theme::PANEL_MATERIAL)
}

fn panel_frost() -> ([f32; 4], [f32; 4]) {
    let a = frost_sweep().unwrap_or(panel_material().frost());
    (
        theme::with_a(theme::PANEL_FROST_TOP, a),
        theme::with_a(theme::PANEL_FROST_BOT, a),
    )
}

#[cfg(feature = "devtriggers")]
fn material_sweep() -> Option<theme::Material> {
    static SEEN: std::sync::OnceLock<Option<theme::Material>> = std::sync::OnceLock::new();
    *SEEN.get_or_init(|| {
        let m = theme::Material::parse(&nj_base::devtrig::read("material")?)?;
        nj_base::eventlog::log(&format!("glass: panel material swept to {m:?}"));
        Some(m)
    })
}
#[cfg(not(feature = "devtriggers"))]
fn material_sweep() -> Option<theme::Material> {
    None
}

#[cfg(feature = "devtriggers")]
fn frost_sweep() -> Option<f32> {
    static SEEN: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    *SEEN.get_or_init(|| {
        let v = nj_base::devtrig::read("panelfrost")?.trim().parse::<f32>().ok()?;
        nj_base::eventlog::log(&format!("glass: panel frost swept to {v}"));
        Some(v.clamp(0.0, 1.0))
    })
}
#[cfg(not(feature = "devtriggers"))]
fn frost_sweep() -> Option<f32> {
    None
}


/// Build the transcode key on the stack and resolve it to a GL texture AND its decoded pixel
/// size — `(0, 0.0, 0.0)` until it is ready. The size is the store's own answer about the slot it
/// just probed for the texture, so knowing the source aspect is free: no second lock or key scan.
///
/// **The size is not optional, which is why this is the only resolver.** Every image is fetched as
/// a `minSize=1` transcode, which COVERS the requested box rather than fitting it, so a texture's
/// aspect is the SOURCE's and not the box's; a picture drawn into its frame without it is
/// stretched. [`card`] turns it into a crop ([`art_uv`]), a backdrop into an overflow
/// ([`Rect::cover`](crate::ui::Rect::cover)). There was an id-only `resolve_tex_on` beside this,
/// and every art tile in the app went through it and drew squashed.
///
/// **A thumb path is only meaningful on the server that issued it** — rating keys are server-local
/// integers from 1, so the same `/library/metadata/42/thumb/…` names a different film on a
/// friend's share — so the server is always NAMED. There is deliberately no current-server twin of
/// this or of [`warm_tex_on`]: a bare form would be the shorter name autocomplete offers for the
/// case where getting it wrong is another item's picture. Art that genuinely belongs to the
/// browsed server (the profile chip's avatar) passes the current server's id and says so.
///
/// `srv` is the server's raw id, as `ui::tex` takes it: the library never names the application's
/// server type, so a caller above it hands over `ServerId::raw()`.
pub(crate) fn resolve_tex_wh_on(
    srv: u16,
    path: &str,
    w: c_int,
    h: c_int,
    png: c_int,
) -> (u32, f32, f32) {
    crate::ui::tex::resolve_wh_on(srv, path, w, h, png != 0)
}

/// The prefetch twin of [`resolve_tex_wh_on`]: the same key through `ui::tex::warm_on` — start the
/// fetch, take no texture, take no LRU protection. Same arguments on purpose, so a screen warms EXACTLY the key it will later
/// resolve; a warm at a different size — or on a different server — is a different slot and buys
/// nothing.
pub(crate) fn warm_tex_on(
    srv: u16,
    path: &str,
    w: c_int,
    h: c_int,
    png: c_int,
) -> crate::ui::tex::Warm {
    crate::ui::tex::warm_on(srv, path, w, h, png != 0)
}

/// Source art for a [`card`]: a catalog poster (resolved at [`POSTER_RES`]), a catalog still, any
/// keyed thumbnail at an explicit resolution, or a person's headshot (the same thumbnail, but its
/// EMPTY case draws a person glyph rather than a blank tile). All load on Litefin's flat
/// [`theme::CARD_PLACEHOLDER`].
pub(crate) enum Art<'a> {
    /// The facts a poster tile reads of its catalog row — see [`TileFacts`], which the caller (a
    /// screen) fills from the row; the library never names the row's own type.
    Poster(Option<TileFacts<'a>>),
    /// **A landscape tile of a real catalog row** — an episode's own still, with the badges and
    /// (through `card_row::resume_bar`) the progress bar a poster wears.
    ///
    /// Carries the ROW rather than a path, which is the whole difference from [`Art::Thumb`] and is
    /// what lets it wear the state marks at all. The artwork it resolves is a FALLBACK CHAIN, and
    /// the order matters: the episode's own still, then the show's backdrop, then the show poster.
    /// A shelf of recently released episodes is exactly where "then the show poster" must be the
    /// LAST resort — several episodes of one show all substituting the same poster is the picture
    /// this variant exists to stop drawing.
    Still(Option<TileFacts<'a>>),
    /// `sid` is the raw id of the server `key` is a path on — an image-transcode path embeds a
    /// server-local ratingKey, so a still or a poster fetched from another machine is a 404 or,
    /// worse, a different item's picture. Every variant here carries one for that reason.
    Thumb {
        sid: u16,
        key: &'a str,
        res: (c_int, c_int),
    },
    /// A credit's headshot (the Cast & Crew shelf). Distinct from [`Art::Thumb`] because a
    /// missing headshot is ROUTINE here — the server has one for most actors and for few crew —
    /// and an empty circle beside named circles reads as a broken image rather than as a person
    /// the metadata agent has no photo of.
    Person {
        sid: u16,
        key: &'a str,
        res: (c_int, c_int),
    },
}

impl Art<'_> {
    fn motion_identity(&self) -> Option<crate::ui::card_motion::Identity> {
        use std::hash::{Hash, Hasher};
        let (owner, sid, key, kind) = match self {
            Self::Poster(Some(m)) => (m.owner, m.src, m.thumb, 0u8),
            Self::Still(Some(m)) => (m.owner, m.src, still_key(m), 1),
            Self::Thumb { sid, key, .. } => (key.as_ptr() as usize, *sid, *key, 2),
            Self::Person { sid, key, .. } => (key.as_ptr() as usize, *sid, *key, 3),
            Self::Poster(None) | Self::Still(None) => return None,
        };
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (sid, key, kind).hash(&mut hash);
        Some(crate::ui::card_motion::Identity { owner, asset: hash.finish() })
    }
}

/// The size a landscape still is transcoded at — [`crate::ui::card_row::RowStyle::EPISODE`]'s
/// 388 x 221.6 picture, rounded, so the server scales once and the texture is 1:1 on the panel.
const STILL_RES: (c_int, c_int) = (388, 222);
/// The box a portrait poster card asks the transcoder for: the classic poster's 228 x 351 picture
/// (`consts::CARD_W`/`CARD_H`); the Library grid's smaller poster samples it down. The collection
/// fan baker requests its members at this box too, so a member already fetched for a card is a
/// disk hit, not a refetch.
pub(crate) const POSTER_RES: (c_int, c_int) = (228, 351);

/// **Which artwork a landscape tile draws, in order of preference.** The episode's own still, the
/// show's backdrop, then the show's poster.
///
/// `TileFacts::still` is empty on anything that is not an episode and on an episode whose server
/// sent no thumb of its own, and BOTH of those must fall through to something 16:9-ish before they
/// reach `thumb` — which for an episode is the SHOW POSTER, i.e. the identical-tiles picture this
/// whole tile exists to replace. Falling back to it at all is still right for the last resort: a
/// poster, cover-cropped to the tile ([`art_uv`]), answers "which show" even when it cannot answer
/// "which episode".
pub(crate) fn still_key<'a>(m: &TileFacts<'a>) -> &'a str {
    if !m.still.is_empty() {
        m.still
    } else if !m.thumb.is_empty() {
        // **The show's POSTER before its backdrop** (`Library Screens.dc.html` E): "where an
        // episode has no still, the tile falls back to the show's poster in the same frame,
        // cover-fitted, label and all — a crop is better than a row of mixed tile shapes." For an
        // episode `thumb` IS `grandparentThumb`, the show poster (`pms::parse_item`); for anything
        // else it is the item's own artwork, which also outranks a shared backdrop. `art` stays as
        // the last resort rather than the second.
        m.thumb
    } else {
        m.art
    }
}

/// **The window of a resolved `tw × th` texture that [`card`] shows in `r`** — a cover crop, so a
/// picture is never stretched to its tile's aspect. Every image the app fetches is a
/// `/photo/:/transcode` with `minSize=1`, which COVERS the requested box rather than fitting it
/// (`img.rs`'s decode-budget note): a 2:3 headshot asked for at 300×300 comes back 300×450, and
/// sampling all of it into a 190-px circle squashed every portrait-shot actor to two-thirds of
/// their height. The crop's placement is the art's ([`art_crop`]); an undecoded texture is
/// [`nj_gfx::gfx::UV_FULL`], as `Rect::cover_uv` documents.
///
/// Pure, and the seam every art tile goes through — the shelf, the Library grid, Search, the
/// person page's portrait, the profile picker, extras, cast — so no one of them can draw a picture
/// at the wrong aspect by forgetting to ask.
pub(crate) fn art_uv(art: &Art, tw: f32, th: f32, r: Rect) -> [f32; 4] {
    r.cover_uv(tw, th, art_crop(art))
}

/// Where [`art_uv`]'s crop keeps the picture: a person's photo rides high so the face survives a
/// portrait source going into a circle; every other variant is even.
pub(crate) fn art_crop(art: &Art) -> crate::ui::Crop {
    match art {
        Art::Person { .. } => crate::ui::Crop::Headshot,
        Art::Poster(_) | Art::Still(_) | Art::Thumb { .. } => crate::ui::Crop::Centre,
    }
}

/// A not-yet-loaded skeleton falls back to a rimmed fill (no shadow until the art arrives).
/// The source-facing half of the card primitive, also exercised without a GPU in
/// host admission tests. The same final rect then goes to the card composite below.
/// Text-only prewarming neither observes placement nor starts image work.
pub(crate) fn resolve_card_art(p: Painter, rect: Rect, art: &Art<'_>) -> (u32, f32, f32) {
    if p.is_recording() { return (0, 0.0, 0.0); }
    let _admission = art.motion_identity()
        .map(|id| crate::ui::card_motion::Scope::card(id, p.to_screen(rect).0));
    let image = card_art_request(art).map_or((0, 0.0, 0.0), |(srv, path, w, h)| resolve_tex_wh_on(srv, path, w, h, 0));
    #[cfg(feature = "devtriggers")]
    crate::ui::card_motion_metrics::draw(image.0 != 0);
    image
}

/// The `(server, path, width, height)` [`resolve_card_art`] asks the source for, for a screen that
/// must WARM exactly that key (the Library's at-rest lookahead). [`resolve_card_art`] resolves
/// through this, so the two cannot name different keys; `screens/library/lookahead_tests.rs::the_lookahead_asks_for_the_key_the_card_draw_resolves` pins it. `None` for an empty slot.
pub(crate) fn card_art_request<'a>(art: &Art<'a>) -> Option<(u16, &'a str, c_int, c_int)> {
    Some(match art {
        Art::Poster(m) => { let m = (*m)?; (m.src, m.thumb, POSTER_RES.0, POSTER_RES.1) }
        Art::Still(m) => { let m = (*m)?; (m.src, still_key(&m), STILL_RES.0, STILL_RES.1) }
        Art::Thumb { sid, key, res } | Art::Person { sid, key, res } => (*sid, *key, res.0, res.1),
    })
}

/// The name a poster card draws on the neutral collection tile, when it draws one: a collection
/// row whose server sent no `thumb`. A composite or custom thumb is artwork and draws as a poster.
fn neutral_collection_name<'a>(m: Option<&TileFacts<'a>>) -> Option<&'a str> {
    m.filter(|m| m.kind == TileKind::Collection && m.thumb.is_empty()).map(|m| m.title)
}

/// **The one art-tile draw op**: `art` resolved to a texture (or its loading fill) and drawn at
/// `frame`, scaled about its centre when `focused`, then its accent badges and its focus ring.
///
/// Litefin's card is flat (`docs/litefin-look.md` 2.6, 11.3): the picture, clipped to
/// [`theme::CARD_RING_RAD`], with no shadow, no edge sheen and no lit-glass glow, so it draws
/// through the plain [`Painter::tex_uv`] and never reaches the image shader's focus program. `f`
/// (0..1) is the [`theme::FOCUS_RING`] around the picture: the shelves pass 1 for the focused
/// tile and 0 for the rest, as Litefin's ring has no transition; the episode and chapter strips
/// pass their pop factor.
pub(crate) fn card(p: Painter, frame: Rect, art: Art, rad: f32, focused: bool, scale: f32, f: f32) {
    card_named(p, frame, art, None, rad, focused, scale, f)
}

/// [`card`] for a caller whose art is a bare path to a collection's thumb and who knows the
/// collection's name (the collection page's header): when the thumb is a server composite and so
/// resolves to our baked fan, `fan_name` is set over it (`collection_tile::draw_fan_name`), as the
/// poster arm does for a collection ROW by itself. `frame` is the RESTING rect in both.
///
/// **`fan_name` is `Some` only when the caller knows the thumb IS a composite.** Whether a path
/// is one is the application's fact (it is how the Plex layer names its generated art), so the
/// caller answers it and the library trusts the answer: `None` draws no name, which is what a
/// custom poster wants.
#[allow(clippy::too_many_arguments)]
pub(crate) fn card_named(p: Painter, frame: Rect, art: Art, fan_name: Option<&str>, rad: f32, focused: bool,
    scale: f32, f: f32) {
    // Text prewarming visits an offscreen page. This leaf has no text: starting
    // image work here would bypass on-screen admission, and pollute its history.
    if p.is_recording() { return; }
    let r = if focused { frame.scaled(scale) } else { frame };
    // All card variants resolve inside their final placement scope. Neither the
    // screen nor its springs can forget to report a new positional expression.
    let (t, tw, th) = resolve_card_art(p, r, &art);
    let picture = |p: Painter| {
        if t != 0 {
            p.tex_uv(t, art_uv(&art, tw, th, r), r, rad, theme::TINT_WHITE);
        } else {
            p.rrect(r, rad, rad, theme::CARD_PLACEHOLDER);
        }
    };
    match art {
        Art::Poster(m) => {
            // **The ROW's server, not the current one.** A `thumb` path is a key on the server that
            // issued it — image-transcode paths embed a server-local ratingKey — so the texture is
            // resolved on `m.src` (`card_art_request`), never on whichever server is current: both
            // servers number from 1, so a current-server fetch draws the wrong film for the
            // ratingKeys that collide.
            if let Some(name) = neutral_collection_name(m.as_ref()) {
                // A collection with no artwork of its own: nothing will ever resolve, so a loading
                // fill would read as loading forever. It wears its mark and name instead, and no
                // badge — a collection has no watch state (`poster_mark`).
                crate::ui::collection_tile::draw(p, frame, r, rad, name);
            } else {
                picture(p);
                // A collection whose thumb is the server's composite is drawn as our baked fan,
                // which leaves its name to be set live — on every tile, focused or not.
                if let Some(m) = m.filter(|m| t != 0 && m.kind == TileKind::Collection && m.composite_thumb) {
                    crate::ui::collection_tile::draw_fan_name(p, frame, r, m.title);
                }
                if let Some(m) = m { card_badges(p, r, &m); }
            }
        }
        // A landscape tile of a real catalog row: the item's own 16:9 art and the same badges a
        // poster wears. It carries the ROW rather than a path for exactly that reason (an
        // `Art::Thumb` is a path and a size, so nothing could derive a badge from it), and because
        // `still_key`'s fallback chain is a property of the item. Its labels sit under the card
        // like every other card's (`card_row`), never on the picture.
        Art::Still(m) => {
            picture(p);
            if let Some(m) = m { card_badges(p, r, &m); }
        }
        Art::Thumb { .. } => {
            picture(p);
            if let Some(name) = fan_name.filter(|_| t != 0) {
                crate::ui::collection_tile::draw_fan_name(p, frame, r, name);
            }
        }
        Art::Person { key, .. } => {
            picture(p);
            // Only for a person the server has NO headshot of — an unresolved texture with a key
            // behind it is merely still loading, and glyphing that would flash a "no photo" mark on
            // every tile of every page for the length of its fetch.
            if t == 0 && key.is_empty() {
                // quantize the glyph box to 4px so the focus-pop animation reuses a handful of
                // cached icon masks instead of rasterizing + uploading one per rounded pixel
                let d = ((r.w * PERSON_GLYPH_RATIO) / 4.0).round() * 4.0;
                crate::ui::icons::draw(
                    p,
                    crate::ui::icons::Icon::User,
                    Rect::new(r.cx() - d * 0.5, r.cy() - d * 0.5, d, d),
                    theme::TEXT_TERTIARY,
                );
            }
        }
    }
    focus_ring(p, r, rad, f);
}

/// **The artwork focus ring** (`docs/litefin-look.md` 2.6 Media cards): [`theme::FOCUS_RING_W`]
/// of [`theme::FOCUS_RING`] hugging the picture from OUTSIDE, its outer corner the picture's plus
/// the ring ([`theme::CARD_FOCUS_RING_RAD`] on a card, a circle on a circle). `f` is its opacity.
pub(crate) fn focus_ring(p: Painter, card: Rect, rad: f32, f: f32) {
    if f <= 0.0 { return; }
    let w = theme::FOCUS_RING_W;
    p.rring(card.inset(-w), rad + w, w, theme::with_a(theme::FOCUS_RING, theme::FOCUS_RING[3] * f.min(1.0)));
}

/// Which state mark a poster wears — the pure half of [`card`]'s corner, split out for exactly the
/// reason `detail::ep_state` is: the CHOICE is the behaviour, while drawing it needs a GL context no
/// host test has. Source-admission tests additionally exercise `card` with the host GL stubs.
///
/// | state | mark | drawn by |
/// |---|---|---|
/// | never started | nothing — and most of a server is here | — |
/// | in progress | the full-bleed resume BAR | the CALLER (`card_row::draw_cell`) |
/// | watched | the amber corner DISC | [`card`] |
///
/// [`PosterMark::InProgress`] is *defined* as "`PmsMovie::resume_frac` has a value" — precisely
/// when the caller draws the bar — so the two halves cannot disagree and put two marks on one tile.
/// That is also the precedence: a re-watch in flight outranks the watched flag, because PMS reports
/// both on a finished-then-restarted item and being part-way through the re-watch is what the viewer
/// is doing. Same answer as the still's resolver, on the same item.
///
/// The three states are the app's ONE watch-state vocabulary, not the poster's alone: the detail
/// hero's watch controls ([`crate::ui::detail`]'s `hero_watch_state`) and the tile context menu's
/// state rows ([`row_watch_state`], and `detail::ep_watch_state` for one episode of a loaded season)
/// all resolve into this same enum, so "what state is this item in" has one answer and one set of
/// names wherever it is asked.
///
/// There are four resolvers rather than one because the INPUTS differ — a catalog row, a loaded
/// `metadata::Detail`, one `metadata::Episode` — and, in exactly one place, because the QUESTION
/// does: a MARK describes while a CONTROL promises what its press delivers, so a container mid-run
/// wears no mark ([`poster_mark`]) and still offers both write verbs ([`row_watch_state`],
/// `hero_watch_state`). Each of the three names the other it departs from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum PosterMark {
    /// never started — and the `Default`, because most of a server is here
    #[default]
    None,
    InProgress,
    Watched,
}

/// Resolve [`PosterMark`] from a catalog row. Total and mutually exclusive by construction.
///
/// Keyed on the row's `watched` flag (`PmsMovie::watched`) — **not** on `!unwatched`, which is a
/// weaker claim for a container: a SHOW with one episode played is `!unwatched` but nowhere near done, and marking it watched is a
/// statement the viewer can see is false. Partly-watched shows therefore land on [`PosterMark::None`]
/// beside never-started ones; the true statement about a series mid-run is where its next episode
/// stands, which a poster in a grid is not the place for. **That is this function's rule and not the
/// enum's** — a MENU asking which verbs are reachable gets a different answer for the same show, and
/// asks [`row_watch_state`] instead.
pub(crate) fn poster_mark<T: crate::ui::tile::Tile + ?Sized>(m: &T) -> PosterMark {
    if m.progress().is_some() {
        return PosterMark::InProgress;
    }
    if m.watched() {
        PosterMark::Watched
    } else {
        PosterMark::None
    }
}

/// The same three states asked as the **write-verb** question: which ends of the watch range can
/// this row still be sent to — i.e. whether a menu offers *Mark as Watched*, *Mark as Unwatched*, or
/// BOTH ([`crate::screens::item_menu`]'s state group).
///
/// It is [`poster_mark`] plus one rule, and the split is the same one the detail hero draws
/// ([`crate::ui::detail`]'s `hero_watch_state`): **a mark DESCRIBES, a control PROMISES.** A poster
/// says nothing about a show three episodes into ten, because "where its next episode stands" is not
/// a statement a tile in a grid can make — so `poster_mark` sends a container mid-run to
/// [`PosterMark::None`], beside the never-started. A menu is asking something else entirely, and for
/// a show in the middle both verbs are reachable and both are true, so it is [`PosterMark::InProgress`]
/// and gets the pair. Anything else and the two surfaces the owner reaches this item through — the
/// hero's discs and the tile's menu — would disagree about one item.
///
/// The extra rule is exactly "**neither end**", which needs no `kind` test: for a leaf `unwatched`
/// and `watched` are complements (`viewCount == 0` vs `> 0`), so only a CONTAINER can be neither, and
/// a container that is neither is one with some leaves viewed and some not (`pms::parse_item`). The
/// one other row that lands here is a container the server sent no leaf counts for, where both flags
/// are false: that item's state is unknown, both verbs are legitimate, and neither LABEL claims a
/// state — each names the outcome its press produces.
///
/// Leaves are delegated rather than re-derived, so the resume-point edge cases (`resume_frac`'s "at
/// or past the end is finished, not in progress") stay in one place.
pub(crate) fn row_watch_state<T: crate::ui::tile::Tile + ?Sized>(m: &T) -> PosterMark {
    if !m.unwatched() && !m.watched() {
        return PosterMark::InProgress;
    }
    poster_mark(m)
}

/// The app's **two watch-state verbs**, written down once.
///
/// Two surfaces offer this pair of writes — the press-and-hold card menu's state group
/// ([`crate::screens::item_menu`]) and the detail hero's discs, which unfurl the verb on focus
/// ([`crate::ui::detail`]'s `watch_label`) — and they are the same two actions on the same item.
/// A menu row reading *Mark as Watched* beside a control reading *Watched* would read as two
/// different writes, so both take the words from here. They sit beside [`row_watch_state`] because
/// that function is the other half of the same vocabulary: it decides WHICH of these a surface may
/// offer, and these are what it offers.
///
/// Each names the OUTCOME its press produces, never the state the item is in — which is what lets
/// a part-watched item show both at once without either being a lie.
pub(crate) fn mark_watched_verb() -> &'static str { nj_platform::i18n::msg::widgets_action_mark_watched() }
pub(crate) fn mark_unwatched_verb() -> &'static str { nj_platform::i18n::msg::widgets_action_mark_unwatched() }

/// …and the third verb the same two surfaces share: **play this from 00:00, ignoring the resume
/// point.** The detail hero's disc and the card menu's row are one action, so they carry one WORD —
/// this one. Before 2026-08-21 they did not even do that: the row said this and the disc said
/// nothing at all, which is what the unfurl fixed.
///
/// **They deliberately carry two GLYPHS, and that is not the same mistake.** The row draws
/// [`crate::ui::icons::Icon::PlayStart`], the hero disc draws `Icon::Restart` — because the disc is
/// icon-only at rest and stands beside the Play pill, where a play-triangle-with-a-bar is that
/// pill's own mark plus a 3px stem, while the row's glyph sits next to the words above and nothing
/// resembling a play mark. The two were reconciled onto one glyph for a day and it was wrong;
/// `Icon::Restart`'s doc is the argument. One action, one word, and the mark chosen per surface for
/// what that surface has to tell apart.
pub(crate) fn play_from_start_verb() -> &'static str { nj_platform::i18n::msg::widgets_action_play_start() }
pub(crate) fn play_trailer_verb() -> &'static str { nj_platform::i18n::msg::widgets_action_play_trailer() }

// ---- Card badges: Litefin's tinted marks on the artwork (`docs/litefin-look.md` 4.2 Badges) ----
//
// The household's badge style resolves to `tinted` (spec 0): accent fill, lavender ink, no border.
// Three of Litefin's badges reach a card here: the episode code top-left, a show's or season's
// unplayed count top-right and the watched check bottom-right. Its favourite heart is off by
// default (`pref:showFavoriteBadges`, unset in the household).

/// Every badge's height and its inset from the picture's edges (base.css `.played-badge`,
/// `.episode-badge`: 40 tall, 12 in).
const CARD_BADGE_H: f32 = 40.0;
const CARD_BADGE_INSET: f32 = 12.0;
/// The episode code's and the unplayed count's side padding (`.episode-badge` `padding: 4px 8px`,
/// `.count-badge` `padding: 0 8px`).
const CARD_BADGE_PAD_X: f32 = 8.0;
/// The code's type, Litefin's 17.6px at 700 on the nearest rung.
const CARD_BADGE_SZ: c_int = theme::size::lf::S18;
/// The watched check's glyph box: Litefin's 18px stroked check.
const CARD_BADGE_CHECK: f32 = 18.0;

/// **An episode's code**, `S01E05`, for its badge: both numbers zero-padded to two digits, the
/// letters the catalog's (`cards.episode_code`). Litefin draws it on every episode card outside a
/// "latest" row (`useEpisodeBadges`, on by default); `None` for anything but a numbered episode.
pub(crate) fn episode_code(m: &TileFacts<'_>) -> Option<String> {
    (m.kind == TileKind::Episode && m.ep_index > 0).then(|| {
        nj_platform::i18n::msg::cards_episode_code(
            &format!("{:02}", m.ep_index),
            &format!("{:02}", m.season_index.max(0)),
        )
    })
}

/// The corner a card badge stands in.
#[derive(Clone, Copy)]
pub(crate) enum BadgeCorner {
    /// The episode code.
    TopLeft,
    /// A show's or season's unplayed count.
    TopRight,
    /// The watched check.
    BottomRight,
}

/// Where a `w`-wide badge sits on `card` (the picture as drawn, popped or not), 12 in from its
/// corner. Fixed-size, so the focus pop moves a badge with its corner without re-rasterizing its
/// glyph.
fn badge_rect(card: Rect, w: f32, corner: BadgeCorner) -> Rect {
    let (left, top) = (card.x + CARD_BADGE_INSET, card.y + CARD_BADGE_INSET);
    let right = card.x + card.w - CARD_BADGE_INSET - w;
    match corner {
        BadgeCorner::TopLeft => Rect::new(left, top, w, CARD_BADGE_H),
        BadgeCorner::TopRight => Rect::new(right, top, w, CARD_BADGE_H),
        BadgeCorner::BottomRight => Rect::new(right, card.y + card.h - CARD_BADGE_INSET - CARD_BADGE_H, w, CARD_BADGE_H),
    }
}

/// The unplayed count's pill width for a `text_w`-wide count: its padding either side, never
/// narrower than a square (`.count-badge` `min-width: 40px`).
fn count_badge_w(text_w: f32) -> f32 {
    (text_w + 2.0 * CARD_BADGE_PAD_X).max(CARD_BADGE_H)
}

/// A catalog card's badges. The code is measured through the live-font [`LegacyMeasure`]: `card`
/// has no measure in its draw chain (the collection fan's name is measured the same way).
pub(crate) fn card_badges(p: Painter, card: Rect, m: &TileFacts<'_>) {
    if let Some(code) = episode_code(m).and_then(|c| CString::new(c).ok()) {
        use nj_machine::machine::Measure;
        let w = LegacyMeasure.width(&code, CARD_BADGE_SZ, true) + 2.0 * CARD_BADGE_PAD_X;
        let r = badge_rect(card, w, BadgeCorner::TopLeft);
        p.rrect(r, theme::radius::BASE, theme::radius::BASE, theme::cards_lf::BADGE_FILL);
        let y = nj_gfx::text::text_vcenter_y(CARD_BADGE_SZ, 1, r.cy());
        p.text(code.as_ptr(), r.x + CARD_BADGE_PAD_X, y, CARD_BADGE_SZ, theme::cards_lf::BADGE_INK, 0, 1);
    }
    if let Some(count) = (m.unplayed > 0).then(|| CString::new(m.unplayed.to_string()).ok()).flatten() {
        use nj_machine::machine::Measure;
        let r = badge_rect(card, count_badge_w(LegacyMeasure.width(&count, CARD_BADGE_SZ, true)), BadgeCorner::TopRight);
        p.rrect(r, theme::radius::LG, theme::radius::LG, theme::cards_lf::BADGE_FILL);
        let y = nj_gfx::text::text_vcenter_y(CARD_BADGE_SZ, 1, r.cy());
        p.text(count.as_ptr(), r.cx(), y, CARD_BADGE_SZ, theme::cards_lf::BADGE_INK, 1, 1);
    }
    if poster_mark(m) == PosterMark::Watched {
        let r = badge_rect(card, CARD_BADGE_H, BadgeCorner::BottomRight);
        p.rrect(r, theme::radius::LG, theme::radius::LG, theme::cards_lf::BADGE_FILL);
        let g = CARD_BADGE_CHECK;
        crate::ui::icons::draw(
            p,
            crate::ui::icons::Icon::Check,
            Rect::new(r.cx() - g * 0.5, r.cy() - g * 0.5, g, g),
            theme::cards_lf::BADGE_INK,
        );
    }
}

/// Person-glyph box as a fraction of a headshot tile — the [`Art::Person`] fallback's one ratio.
/// Deliberately TIGHTER than [`DISC_ICON_RATIO`] (0.54, the ratio every disc *control* glyph uses,
/// and what the profile chip's own fallback works out to): a cast card is 228px wide, and 0.54 of
/// it is past `icons::tex_for`'s 112px rasterization clamp — the mask would be upscaled and soft. At
/// 0.44 the box lands at about 100px across the whole focus pop, inside the clamp and crisp.
const PERSON_GLYPH_RATIO: f32 = 0.44;

/// The scale a focused card pops to (shared by every animated card row): Litefin's effective
/// `.media-card.focused` scale once its lazy stylesheets have loaded (`docs/litefin-look.md` 2.6).
pub(crate) const CARD_FOCUS_SCALE: f32 = 1.02;

/// The scale a focused CONTROL face pops to — the design system's `--focus-scale-control`.
///
/// Every control face takes it: [`Button`], [`CircleButton`] and [`TransportButton`]. It is
/// deliberately the generic tile's number and no larger — the FILL already carries focus here, so
/// the pop is the second half of a signal rather than the whole of one, and a control that popped
/// like a poster (1.09) would out-shout the artwork it sits on.
///
/// **A control inside a TRACK does not take it.** The top tab bar's pills and the profile chip are
/// focused by their row's own motion — the capsule travelling, the chip unfurling — and a pill that
/// also grew would be two answers to one question. The season strip's BARE pills are not in a track
/// and do pop — through [`TabGround::Plated`], which carries the factor to the focus capsule that IS
/// the focused pill's face. That last sentence was here for months while nothing in the draw scaled
/// anything: a claim about another module cannot be compiled, so it read as a description and was
/// a specification nobody had executed.
///
/// Written as its own constant rather than an alias of [`CARD_FOCUS_SCALE`] because the two are
/// separate design decisions that happen to have landed on one number; either may move alone.
pub(crate) const CTRL_FOCUS_SCALE: f32 = 1.07;

/// The gap between two controls that form ONE GROUP — the design system's `--control-gap`, which
/// sits in `tokens/layout.css` under *Controls* rather than on the `theme::space` ladder, because
/// it is a property of the control family and not a rung of the vertical rhythm (20 is not on that
/// ladder at all: `SM` is 16 and `MD` is 24).
///
/// **It is deliberately smaller than a `space` rung, and that is the whole distinction.** A
/// spacing rung separates two things you read as SEPARATE — a primary action and a hint beside it,
/// a heading and its body. Two peer answers are one object with two faces, so they sit closer than
/// any block gap would put them; at `space::LG` 40 the pair reads as two unrelated controls that
/// happen to share a row. The decision alert's `Cancel`/`Delete` pair has always used this
/// distance (`decision_alert::BUTTON_GAP`, the design's `--alert-btn-gap`); the two are separate
/// tokens in the design system that happen to have landed on one number, so either may move alone.
pub(crate) const CONTROL_GAP: f32 = 20.0;

/// One control ROW's focus pop — a spring per control, so the arriving face grows while the one
/// being left behind shrinks.
///
/// A row rather than a widget owns this for the reason every animated row in this app does
/// ([`crate::ui::card_row`]): the pop is a property of a control's place in a row, the widgets are
/// immediate-mode and keep nothing between frames, and the leaving control still needs a value after
/// it has stopped being the focused one. A single global spring cannot express that — it would have
/// to snap the outgoing face to 1.0 the instant focus moved, which is the 4px jump this exists to
/// avoid.
///
/// The spring is **critically damped**, on [`K_SCALE`](crate::ui::consts::K_SCALE) — a control face
/// arriving at focus is the SAME motion a poster's focus pop is, and neither bounces. The design
/// system is explicit and states it twice: `tokens/motion.css` opens with "the only thing that
/// BOUNCES is the press release: focus ARRIVING is a calm grow, the CLICK is what rings", and the
/// `--ease-bounce` token's own comment says to use it "for the press spring-back and NOTHING else —
/// never a focus pop, a fade, a slide, or anything that carries text". `Button.jsx` spends the two
/// curves accordingly: `--ease-spring` on arrival, `--ease-bounce` only while `releasing`.
///
/// **This was underdamped until 2026-08-22**, on `press`'s own release constants, under a comment
/// claiming the design system named two bouncing things. It names one. The pop is the tile's spring
/// now — the same `K_SCALE` `card_row` steps every shelf with — so a capsule and a poster answer the
/// same event at the same rate, which is the thing that was actually worth sharing. What still
/// belongs to `press` is the click, and that is folded in by [`scale`](Self::scale) below.
///
/// [`scale`](Self::scale) folds in [`press::scale`](crate::ui::press::scale) for the focused control
/// only. The dip is a FACTOR on top of the focus scale — that is what makes a press read the same on
/// a 1.07 capsule as on a 1.09 poster — and it belongs to the control being pressed, which is always
/// the focused one.
pub struct CtlPop<const N: usize> {
    sp: [Spring; N],
    focused: Option<usize>,
}
impl<const N: usize> CtlPop<N> {
    /// Captured geometry needs the spring velocity as well as its current scale: the next
    /// input may land after another Tick. GPU resources and paint palettes are not encoded.
    pub(crate) fn write_motion(&self, c: &mut nj_machine::machine::Canon) {
        let Self { sp, focused } = self;
        c.seq(sp.len());
        for spring in sp { c.f32(spring.pos).f32(spring.vel); }
        c.option(*focused, |c, i| { c.u32(i as u32); });
    }

    pub const fn new() -> Self {
        Self {
            sp: [Spring::at(1.0); N],
            focused: None,
        }
    }
    /// Advance every control's spring toward its target for this frame. `focused` is the index of
    /// the control holding focus, or `None` when the row has none at all (the page scrolled away
    /// from it, a panel took over), which closes every pop.
    pub fn step(&mut self, focused: Option<usize>, dt: f32) {
        self.focused = focused;
        for (i, sp) in self.sp.iter_mut().enumerate() {
            let target = if focused == Some(i) {
                CTRL_FOCUS_SCALE
            } else {
                1.0
            };
            sp.step(target, crate::ui::consts::K_SCALE, dt);
        }
    }
    /// Control `i`'s drawn scale this frame, press dip included. Out-of-range asks answer 1.0 rather
    /// than panicking: a row whose control COUNT changes with the item (`detail::hero_ctls`) indexes
    /// this by a position that can outrun `N` for a frame while the set is being rebuilt.
    pub fn scale(&self, i: usize) -> f32 {
        self.scale_with(i, crate::ui::press::scale())
    }
    /// Captured-input counterpart for owned screens: paint and placement use the same frame's
    /// press value, without consulting the legacy global press machine. Zero means no press read.
    pub fn scale_with(&self, i: usize, press_scale: f32) -> f32 {
        let s = self.sp.get(i).map(|sp| sp.pos).unwrap_or(1.0);
        if self.focused == Some(i) {
            s * if press_scale > 0.0 { press_scale } else { 1.0 }
        } else {
            s
        }
    }
    /// Drop every pop to rest with no motion in between — for a page being torn down or re-mounted,
    /// so the next mount does not open on a control already popped (`detail::reset_view_state`).
    pub fn reset(&mut self) {
        self.focused = None;
        for sp in self.sp.iter_mut() {
            sp.jump(1.0);
        }
    }
}
impl<const N: usize> Default for CtlPop<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Icon box as a fraction of a round control's diameter — the ONE ratio every disc glyph uses
/// (transport CC/Audio buttons, CircleButton vector icons, the Continue-Watching play badge).
pub(crate) const DISC_ICON_RATIO: f32 = 0.54;

/// The air between one control of an ACTION ROW and the next, edge to edge — the sibling of
/// [`StatusOverlay::CTRL_H`] on the other axis, and the second half of what makes the home hero's
/// row and the detail hero's row one object rather than two that agree by inspection. Both wrote
/// out a private `20.0` (Home's `HERO_CTRL_GAP`, `detail::CGAP`), so the rhythm those rows are meant
/// to share was a number two files happened to hold the same copy of.
///
/// Deliberately NOT a [`theme::space`] rung: that ladder is the gap between stacked BLOCKS in a
/// page's vertical flow, and this is the air inside one horizontal control row, measured against
/// the 60px control it separates. Adding a 20 to the ladder to launder it would put a rung on the
/// scale that no block spacing may use, which is worse than an honest constant with a home.
pub(crate) const CTRL_GAP: f32 = 20.0;

/// A media card shared by the episode picker and the chapters strip so they resolve + animate
/// identically: the thumbnail (at `res`, or the loading fill), a focus scale-pop about the centre
/// and the accent focus ring when `focused` (the caller owns the `scale` spring).
pub(crate) fn draw_card(
    p: Painter,
    frame: Rect,
    sid: u16,
    thumb: &str,
    res: (c_int, c_int),
    radius: f32,
    focused: bool,
    scale: f32,
) {
    draw_card_peaked(p, frame, sid, thumb, res, radius, focused, scale, CARD_FOCUS_SCALE);
}

/// [`draw_card`] for a caller whose scale spring targets a peak OTHER than [`CARD_FOCUS_SCALE`] —
/// the episode filmstrip's own [`crate::ui::theme::EP_CARD_FOCUS_SCALE`] pop. The pop factor is
/// taken against the caller's own peak so the focus ring is fully drawn there.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_card_peaked(
    p: Painter,
    frame: Rect,
    sid: u16,
    thumb: &str,
    res: (c_int, c_int),
    radius: f32,
    focused: bool,
    scale: f32,
    peak_scale: f32,
) {
    // pop factor from the caller's scale spring (0 at rest → 1 at full focus scale) drives the ring
    let f = if focused { pop_factor(scale, peak_scale) } else { 0.0 };
    card(
        p,
        frame,
        Art::Thumb {
            sid,
            key: thumb,
            res,
        },
        radius,
        focused,
        scale,
        f,
    );
}

/// A pop spring's scale as a 0..1 focus factor against the `peak` it targets — what drives a
/// strip card's focus ring and an episode still's lift.
pub(crate) fn pop_factor(scale: f32, peak: f32) -> f32 {
    ((scale - 1.0) / (peak - 1.0)).clamp(0.0, 1.0)
}

/// **THE progress bar** — the app's one "how far into this am I" mark, on every tile shape: the bottom
/// band of the artwork itself, full-bleed edge to edge, square-ended fill, clipped to the card's own
/// rounded silhouette (`docs/litefin-look.md` 4.2 Progress: a 6px bar, track `rgba(0,0,0,.5)`, fill
/// a left-to-right gradient from the accent to its hover tint, under `overflow: hidden`).
///
/// One function for the Continue Watching card and the episode still, because it is meant to be the
/// SAME bar on every tile.
///
/// Three details are load-bearing:
///
/// * **The band is snapped to whole composited pixels.** `gfx::clip_set` truncates its scissor to
///   integer rows while the fill is antialiased at fractional coordinates, so an unsnapped band leaves
///   a hairline of either bare artwork or a doubled band.
/// * **Track and fill never share a pixel.** Two translucent fills compositing on the corner arc's
///   partial coverage came out brighter than either alone (a 1–2px fleck that pulsed with the pop),
///   so the band is split at the played fraction instead.
/// * **Only the corners need the card's silhouette.** The renderer's one horizontal gradient is the
///   square [`Painter::grad4`], so it fills the straight run of the bottom edge between the two
///   corner arcs, and the arcs themselves are the card's own rounded rect under a scissor, flat in
///   the gradient's colour at that end (9px of a card-wide ramp).
pub(crate) fn progress_bar(p: Painter, card: Rect, rad: f32, h: f32, frac: f32) {
    let snap = |y: f32| nj_gfx::gfx::snap(y + p.dy) - p.dy;
    let bottom = card.y + card.h;
    let top = snap(bottom - h.min(card.h));
    if bottom <= top {
        return;
    }
    let right = card.x + card.w;
    // the split is a whole pixel too, so the fill's square end is a clean edge rather than a column
    // of half-covered fill
    let split = (card.x + card.w * frac.clamp(0.0, 1.0))
        .round()
        .clamp(card.x, right);
    let band = |x0: f32, x1: f32| Rect::new(x0, top, x1 - x0, bottom - top);
    let arc = |x0: f32, x1: f32, ink: [f32; 4]| {
        if x1 > x0 {
            p.clip(band(x0, x1));
            p.rrect(card, rad, rad, ink);
        }
    };
    if split > card.x {
        let ink = |x: f32| {
            theme::mix(theme::RESUME_FILL, theme::RESUME_FILL_END, ((x - card.x) / (split - card.x)).clamp(0.0, 1.0))
        };
        // where the bottom edge runs straight, in whole pixels: between the two corner arcs
        let lo = (card.x + rad).ceil().min(split);
        let hi = (right - rad).floor().clamp(lo, split);
        arc(card.x, lo, ink(card.x));
        if hi > lo {
            p.clip(band(lo, hi));
            p.grad4(band(lo, hi), [ink(lo), ink(hi), ink(hi), ink(lo)]);
        }
        arc(hi, split, ink(hi));
    }
    arc(split, right, theme::RESUME_TRACK);
    p.clip_clear();
}

/// The corner radius of the Person bio and Collection summary `ui::text_lift` blocks.
pub(crate) const TEXT_BLOCK_HL_RAD: f32 = 18.0;

// ---- Keyline chip: the FINE-PRINT outlined chip — a hairline box round a very short label, sized to
// hug it. Its one job is the content rating beside an episode's air date (`18+` / `TV-MA`).
//
// Deliberately NOT `badge` + `BadgeStyle::Outlined`, which is the same shape two rungs up: that chip
// is built on `BADGE_H` (34) with 12px padding, a 2px keyline and a BOLD `CAPTION` label, because it
// exists to sit in a row of metadata chips and hold its own. Down here it has to sit beside a 22px
// air date as the dimmest thing on the tile, and at badge's weight it outweighed the episode TITLE
// above it. `BADGE_H` stays one band for every `BadgeStyle`, as documented — this is a different leaf,
// not a resized one.
const KEYLINE_PAD_X: f32 = 7.0;
const KEYLINE_PAD_Y: f32 = 6.0;
const KEYLINE_RAD: f32 = 5.0;
const KEYLINE_W: f32 = 1.5;
/// The chip's label weight — the mock's `font-weight:600`. See [`keyline_chip`] for why it is bold.
const KEYLINE_BOLD: std::os::raw::c_int = 1;

/// The width [`keyline_chip`] will occupy for `text` — the measure-first companion.
pub(crate) fn keyline_chip_w(text: &str, measure: &dyn nj_machine::machine::Measure) -> f32 {
    measure.width_str(text, theme::size::CAPTION, KEYLINE_BOLD != 0) + 2.0 * KEYLINE_PAD_X
}

/// Draw a fine-print keyline chip with its LEFT edge at `x`, centred on `cy`; returns its width.
///
/// **A real hollow ring** ([`Painter::rring`]), so whatever is behind it shows through the middle —
/// the mock's `box-shadow: inset 0 0 0 1.5px …` with no `background`. It used to be a KNOCKOUT
/// (stroke colour, then the interior repainted in a `bg` the caller named), which is exact on a
/// flat panel and wrong over artwork: on the detail hero's identity line the ground is a backdrop
/// plus two scrim ramps, so a chip claiming `SURFACE_APP` read as a dark box over a bright still
/// instead of a hairline. The parameter is gone rather than defaulted — there was no honest value
/// for it, which is the point.
/// The ring takes the LABEL'S OWN INK, and the mock's `rgba(255,255,255,.34)` deliberately does
/// not port — it cannot. `Painter::rring` is the SDF's rim band, whose coverage is the product of
/// two 1.5px-wide smoothsteps (`fs_src.frag`); at [`KEYLINE_W`] the window where both reach 1 is
/// about a QUARTER of a pixel, so no pixel centre lands in it and a thin rim never resolves to
/// more than a fraction of its stated alpha. Measured on the panel: `white .34` came out ~12
/// levels above the ground where the arithmetic says ~87 — an outline nobody can see. The opaque
/// ink is what makes a 1.5px ring exist at all here, and it is what shipped before this was
/// briefly "corrected" to the mock's literal value.
///
/// The label is BOLD for the same reason the mock sets `font-weight:600` on it: two or three caps
/// at `CAPTION` inside a ring have to hold their own against it, and regular weight is what made
/// this chip read as an empty frame in the first device photograph of the identity line.
pub(crate) fn keyline_chip(p: Painter, x: f32, cy: f32, text: &str, col: [f32; 4], measure: &dyn nj_machine::machine::Measure) -> f32 {
    let lc = match std::ffi::CString::new(text) {
        Ok(c) => c,
        Err(_) => return 0.0,
    };
    let w = keyline_chip_w(text, measure);
    let h = measure.cap_h(theme::size::CAPTION) + 2.0 * KEYLINE_PAD_Y; // hugs the label's cap band, not a fixed band
    p.rring(
        Rect::new(x, cy - h * 0.5, w, h),
        KEYLINE_RAD,
        KEYLINE_W,
        col,
    );
    p.text(
        lc.as_ptr(),
        x + KEYLINE_PAD_X,
        nj_gfx::text::text_vcenter_y(theme::size::CAPTION, KEYLINE_BOLD, cy),
        theme::size::CAPTION,
        col,
        0,
        KEYLINE_BOLD,
    );
    w
}

// ---- Key cap + key hint: a REMOTE BUTTON named in running prose --------------------------------
//
// "Press [BACK] to return" — the line every read-only alert panel closes with (`Alert Views.dc.html`
// 1A/1B/1C all carry it, 1E states the rule: the panels that hold no control close on BACK, so the
// line IS the whole affordance) and the line the player's failure read-out already carried.
//
// **The cap is the point, not decoration.** `BACK` set in the same prose as the words around it
// reads as a word; the same three letters inside a keyline read as the thing under your thumb. That
// distinction has to survive a PHOTOGRAPH of a television in an issue thread, which is the failure
// read-out's whole output format, and a keyline survives a phone camera's chroma subsampling where
// a colour or weight change does not.
//
// The idiom was written first as a private `draw_hint_with_keycap` in `player_hud.rs`, screen-local
// and centred on `SCR_W`. It is here because the alert panels want the same object RIGHT-aligned
// inside a panel, and the guide's rule 4 answers that: promote the widget, don't fork it. Three
// alert lanes reached for it independently and all three rejected the HUD's CONSTRUCTION for the
// same reason, below.
//
// **The cap is a genuinely hollow ring in the LABEL'S OWN INK**, which is `keyline_chip`'s
// construction and its measured reason — read that one before retuning either. The HUD's original
// drew the ring as a knockout (a white-at-.34 rounded rect, then the interior repainted OPAQUE
// BLACK) and got away with it because the video plane behind the read-out is black by construction.
// On a glass panel that interior is a black hole punched through the frost. Going hollow is
// therefore forced — and the alpha cannot come with it: at [`KEYCAP_W`] 1.5 the SDF's rim coverage
// is the product of two 1.5px smoothsteps, whose overlap is about a quarter of a pixel, so a
// stated `.34` resolves to roughly a tenth of that on screen. `keyline_chip` measured it (~12
// levels above ground where the arithmetic says ~87). Opaque ink is what makes a 1.5px ring exist.
//
// The player's failure read-out no longer draws a key cap at all: its actions are real buttons
// (`StatusOverlay`'s row, `player::failure_actions`), so this is the one cap construction left.
/// The cap's outer height — a fixed band, unlike [`keyline_chip`], which hugs its label's cap band.
/// A key cap stands for a physical button, so every cap in the app is the same size whatever word
/// is on it; only the WIDTH grows.
pub(crate) const KEYCAP_H: f32 = 36.0;
/// The narrowest a cap is ever drawn. `BACK` sets it; `OK` would otherwise come out as a stub, and
/// two caps of different widths on one line read as two different objects.
const KEYCAP_MIN_W: f32 = 74.0;
const KEYCAP_PAD_X: f32 = 12.0;
const KEYCAP_RAD: f32 = 8.0;
/// Stroke width — the design's 1.5, the same weight as [`keyline_chip`]'s and `ControlStyle::Keyline`'s.
const KEYCAP_W: f32 = 1.5;
/// A cap's label is BOLD [`theme::size::MICRO`]: it is a one-line de-emphasised LABEL in the rung's
/// own terms, and it must hold its own inside a ring at a size below the reading floor.
const KEYCAP_BOLD: c_int = 1;
/// Legacy-fragment spacing around a cap. Complete translated sentences retain their own spaces
/// and use no added gap. The design's `gap:12`, which every
/// alert footer in `Alert Views.dc.html` sets (§1A's and both of §1B's runs). It shipped at a
/// hand-tuned 14 in all three lanes that built one of these panels, none of which could read the
/// spec; two pixels either side of one cap is not a thing anyone would have found by looking.
const KEYCAP_GAP: f32 = 12.0;

/// The glyph box inside a [`CapFace::Glyph`] cap. A remote's arrow keys carry an arrow, not a
/// word, so their cap wears the design system's chevron; 20px lands the chevron's ink at about the
/// cap height of the bold `MICRO` label a word cap carries, so the two kinds of cap read as one family.
const KEYCAP_GLYPH: f32 = 20.0;

/// What is printed on a [`key_cap`]: the key's NAME (`BACK`, `OK`) or, for a key whose face is a
/// symbol (the remote's arrows), the design system's glyph for it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum CapFace<'a> {
    Label(&'a std::ffi::CStr),
    Glyph(crate::ui::icons::Icon),
}

/// The width [`key_cap`] will occupy for `face` — the measure-first companion, so a caller can
/// right-align or centre the whole line before drawing any of it.
pub(crate) fn key_cap_w(face: CapFace<'_>, measure: &dyn nj_machine::machine::Measure) -> f32 {
    let inner = match face {
        CapFace::Label(label) => measure.width(label, theme::size::MICRO, KEYCAP_BOLD != 0),
        CapFace::Glyph(_) => KEYCAP_GLYPH,
    };
    (inner + 2.0 * KEYCAP_PAD_X).max(KEYCAP_MIN_W)
}

/// Draw one key cap with its LEFT edge at `x`, centred on `cy`; returns its width.
pub(crate) fn key_cap(
    p: Painter,
    x: f32,
    cy: f32,
    face: CapFace<'_>,
    ink: [f32; 4],
    measure: &dyn nj_machine::machine::Measure,
) -> f32 {
    let w = key_cap_w(face, measure);
    p.rring(
        Rect::new(x, cy - KEYCAP_H * 0.5, w, KEYCAP_H),
        KEYCAP_RAD,
        KEYCAP_W,
        ink,
    );
    match face {
        CapFace::Label(label) => {
            let tw = measure.width(label, theme::size::MICRO, KEYCAP_BOLD != 0);
            p.text(
                label.as_ptr(),
                x + (w - tw) * 0.5,
                nj_gfx::text::text_vcenter_y(theme::size::MICRO, KEYCAP_BOLD, cy),
                theme::size::MICRO,
                ink,
                0,
                KEYCAP_BOLD,
            );
        }
        CapFace::Glyph(icon) => crate::ui::icons::draw(
            p,
            icon,
            Rect::new(
                x + (w - KEYCAP_GLYPH) * 0.5,
                cy - KEYCAP_GLYPH * 0.5,
                KEYCAP_GLYPH,
                KEYCAP_GLYPH,
            ),
            ink,
        ),
    }
    w
}

/// `{pre} [KEY] {post}` — one line of fine print with a [`key_cap`] set into the middle of it.
///
/// Measure-then-place, because a caller needs the width before it knows the x: an alert panel
/// right-aligns the line against its own padding edge (`right - hint.width()`), and the same
/// arithmetic centres it (`(w - hint.width()) * 0.5`) for whoever wants that. It places by x rather
/// than centring itself precisely so that both are the caller's to choose — the alignment is the
/// half that belongs to the screen, the assembled line is the half that does not.
///
/// `new` borrows the three C strings for the draw lifetime. `translated` owns its surrounding
/// prose so a whole sentence can move the borrowed keycap without risking a dangling string.
pub(crate) struct KeyHint<'a> {
    pre: std::borrow::Cow<'a, std::ffi::CStr>,
    key: CapFace<'a>,
    post: std::borrow::Cow<'a, std::ffi::CStr>,
    /// Legacy fragment callers supply no whitespace; complete translated sentences supply it.
    fragment_gap: f32,
}

#[derive(Debug, PartialEq)]
struct KeyHintLayout {
    key_x: f32,
    post_x: f32,
    width: f32,
}

impl<'a> KeyHint<'a> {
    pub(crate) fn new(
        pre: &'a std::ffi::CStr,
        key: &'a std::ffi::CStr,
        post: &'a std::ffi::CStr,
    ) -> Self {
        Self { pre: pre.into(), key: CapFace::Label(key), post: post.into(), fragment_gap: KEYCAP_GAP }
    }

    /// A complete translated sentence containing one object-replacement character for the key.
    /// Translators may move that placeholder to either end of the sentence without changing the
    /// layout code. The owned runs keep their UTF-8 C strings alive for the entire draw.
    pub(crate) fn translated(message: String, key: &'a std::ffi::CStr) -> Self {
        let (pre, post) = key_hint_parts(&message);
        Self { pre: pre.into(), key: CapFace::Label(key), post: post.into(), fragment_gap: 0.0 }
    }

    /// A legacy-fragment hint whose physical key is drawn as an icon.
    pub(crate) fn glyph(pre: &'a std::ffi::CStr, key: crate::ui::icons::Icon, post: &'a std::ffi::CStr) -> Self {
        Self { pre: pre.into(), key: CapFace::Glyph(key), post: post.into(), fragment_gap: KEYCAP_GAP }
    }

    /// A complete localized sentence around a physical key glyph, preserving punctuation.
    pub(crate) fn translated_glyph(message: String, key: crate::ui::icons::Icon) -> Self {
        let (pre, post) = key_hint_parts(&message);
        Self { pre: pre.into(), key: CapFace::Glyph(key), post: post.into(), fragment_gap: 0.0 }
    }

    /// Total width of the assembled line.
    pub(crate) fn width(&self, measure: &dyn nj_machine::machine::Measure) -> f32 {
        self.layout(measure).width
    }

    /// One set of advances for measuring and painting, including the catalog's own spaces.
    fn layout(&self, measure: &dyn nj_machine::machine::Measure) -> KeyHintLayout {
        let sz = theme::size::CAPTION;
        let key_x = measure.width(&self.pre, sz, false)
            + if self.pre.is_empty() { 0.0 } else { self.fragment_gap };
        let post_x = key_x + key_cap_w(self.key, measure)
            + if self.post.is_empty() { 0.0 } else { self.fragment_gap };
        KeyHintLayout { key_x, post_x, width: post_x + measure.width(&self.post, sz, false) }
    }

    /// The band the line occupies — the cap is taller than the prose's cap band, so a caller
    /// reserving flow for this must reserve the CAP's height, not the text's.
    pub(crate) const fn height() -> f32 {
        KEYCAP_H
    }

    /// **The air a panel leaves BELOW this line, which is deliberately not that panel's own
    /// padding.** One rung ([`theme::space::MD`]), matching the `space::MD` step every alert in the
    /// family puts between its closing hairline and this line — so the hint sits centred in the band
    /// the rule opens, instead of riding high in it.
    ///
    /// All three read-only alerts drew `PAD` 48 here, which is what the design states
    /// (`Alert Views.dc.html`'s content div is `padding:48px` all round, and nothing follows the
    /// footer row). Against the PANEL FRAME that is exactly symmetric — the eyebrow's cap top is 48
    /// below the top edge and the key cap's ring is 48 above the bottom one. Against the HAIRLINE,
    /// which is the edge the eye actually measures this line from, it is 24 over and 48 under.
    ///
    /// And it is worse than 2:1 on the ink, which is the number that settles it: the reserved band
    /// is the CAP's 36px, while the words either side are `size::CAPTION` with a ~17px cap band
    /// centred in it. Only the 1.5px keycap ring ever reaches the band's edges, so the READ line
    /// runs 33.5px below the hairline and 56.5px above the panel's floor. Half the visible air is
    /// under three words nobody is meant to look at twice.
    pub(crate) const fn pad_below() -> f32 {
        theme::space::MD
    }

    /// Draw with the line's LEFT edge at `x`, its cap band vertically centred on `cy`. The prose
    /// sits on its own cap band (rule 3 — never a magic y), the cap on the same centre line.
    pub(crate) fn draw(
        &self,
        p: Painter,
        x: f32,
        cy: f32,
        measure: &dyn nj_machine::machine::Measure,
    ) {
        let sz = theme::size::CAPTION;
        let ty = nj_gfx::text::text_vcenter_y(sz, 0, cy);
        let layout = self.layout(measure);
        p.text(self.pre.as_ptr(), x, ty, sz, theme::TEXT_TERTIARY, 0, 0);
        key_cap(p, x + layout.key_x, cy, self.key, theme::TEXT_SECONDARY, measure);
        p.text(
            self.post.as_ptr(),
            x + layout.post_x,
            ty,
            sz,
            theme::TEXT_TERTIARY,
            0,
            0,
        );
    }
}

/// Split the marked key from a localized sentence, keeping text order and UTF-8 intact.
pub(crate) fn key_hint_parts(message: &str) -> (std::ffi::CString, std::ffi::CString) {
    let (pre, post) = message.split_once('\u{fffc}').unwrap_or((message, ""));
    let run = |s: &str| std::ffi::CString::new(s).unwrap_or_default();
    (run(pre), run(post))
}

// ---- Dotted run: fine-print facts separated by `·` ----------------------------------------------

/// Draw `parts` left to right from `x` on the cap-band y `y`, joined by a **`·` in
/// [`theme::TEXT_SEPARATOR`]** with `pad` either side of it. Returns the total drawn width.
///
/// **Empty parts are ABSENT, not blank**: a run with no content contributes neither a draw nor a
/// separator, so a person with no birthplace gets "Actor · 1987" and never "Actor · 1987 · ". That
/// is the whole reason this is a component and not a `join(" · ")` — the dot is a *different ink*
/// from the runs it divides ([`theme::TEXT_SEPARATOR`], .45 of the words' own, which is what every
/// mock in the design project sets a separator to). A dot sharing the ink of the words either side
/// JOINS them instead of punctuating them, and one `p.text` call cannot express the difference.
///
/// Promoted out of `detail.rs`, where it was `dotted_run` and private; the detail facts row and the
/// bio panel's identity line are the same idiom one screen apart, and the second copy is where the
/// separator ink would have drifted.
/// The hairline divider's height. Published because a panel's flow has to MEASURE the rule as
/// well as draw it, and the two files that made it a private constant each called it something
/// else (`RULE_H`, `HAIRLINE_H`) while holding the same 1.0.
pub(crate) const HAIRLINE_H: f32 = 1.0;

/// One full-width HAIRLINE divider — the alert family's rule, in one place.
///
/// **A `theme::HAIRLINE` rect, never a 1px `rrect` with a radius nobody can see.** That sentence
/// was already written down, in `screens::tracks_panel`'s private copy of this function, while a third
/// panel drew exactly the `rrect` it forbids — which is what three private drawers of one line
/// buy you. The height is 1.0 and is not a parameter: a divider that is two pixels somewhere is a
/// different object, and both files that made it a named constant gave it the same value.
pub(crate) fn hairline(p: Painter, x: f32, y: f32, w: f32) {
    p.rect(
        Rect::new(x, y, w, HAIRLINE_H),
        0.0,
        theme::HAIRLINE,
        theme::HAIRLINE,
        0.0,
    );
}

pub(crate) fn dotted_run(
    p: Painter,
    parts: &[&str],
    x: f32,
    y: f32,
    sz: c_int,
    col: [f32; 4],
    pad: f32,
) -> f32 {
    let mut bx = x;
    for part in parts.iter().filter(|s| !s.is_empty()) {
        if bx > x {
            bx += pad;
            // `c"·"`, not `CString::new` — the separator is a compile-time constant, and this runs
            // once per gap per frame on the detail facts row and the bio identity line.
            bx += p.text(c"\u{b7}".as_ptr(), bx, y, sz, theme::TEXT_SEPARATOR, 0, 0);
            bx += pad;
        }
        if let Ok(pc) = CString::new(*part) {
            bx += p.text(pc.as_ptr(), bx, y, sz, col, 0, 0);
        }
    }
    bx - x
}

// ---- Scroll rail: how far through a paged viewport you are --------------------------------------

/// The rail's bar width, and the corner that makes it a pill. A **6px** bar is the design's, and the
/// radius is simply half of it — a "pill radius" is not a number to choose, it is the constraint
/// that the ends are semicircles.
pub(crate) const RAIL_W: f32 = 6.0;
const RAIL_RAD: f32 = RAIL_W * 0.5;

/// Where the rail's fill sits inside its track, as `(top, height)` FRACTIONS of the track — the
/// pure half of [`scroll_rail`], so the arithmetic is host-testable without a GL context.
///
/// **It is quantised to PAGES, not to pixels, and that is the design.** The viewport it indexes
/// moves a page at a time (there is no continuous scroll to track), so a fill sized by
/// `viewport/content` would sit at fractions the content can never rest at and would stop short of
/// the bottom on the last page. One page of travel moves the fill by exactly its own height, which
/// is what makes the rail readable as "3 of 5" rather than as an approximate position.
///
/// `pages == 0` cannot happen from [`scroll_rail`] (a viewport that fits is not railed at all), and
/// is folded to the full track rather than dividing by zero. `page` is 1-based and clamped, so a
/// caller that has not yet re-clamped after a resize draws a rail that is merely stale, never one
/// hanging off the end of its track.
pub(crate) fn rail_geom(page: usize, pages: usize) -> (f32, f32) {
    if pages <= 1 {
        return (0.0, 1.0);
    }
    let h = 1.0 / pages as f32;
    let page = page.clamp(1, pages);
    ((page - 1) as f32 * h, h)
}

/// Draw the paged scroll rail in `track` (the caller's 6px-wide column beside its viewport): the
/// full-height dim track, then the fill at [`rail_geom`]'s offset. Both are pill-ended.
///
/// Drawn only when there is more than one page — a rail on content that fits is a control saying
/// there is somewhere else to go when there is not.
pub(crate) fn scroll_rail(p: Painter, track: Rect, page: usize, pages: usize) {
    if pages <= 1 {
        return;
    }
    p.rrect(track, RAIL_RAD, RAIL_RAD, theme::RAIL_TRACK);
    let (top, h) = rail_geom(page, pages);
    p.rrect(
        Rect::new(track.x, track.y + top * track.h, track.w, h * track.h),
        RAIL_RAD,
        RAIL_RAD,
        theme::RAIL_FILL,
    );
}

/// Geometry for a continuously scrolling document.  Unlike [`rail_geom`], the thumb represents
/// the visible/content ratio and its travel represents the exact scroll range.
pub(crate) fn continuous_rail_geom(
    scroll: f32,
    content_h: f32,
    viewport_h: f32,
) -> Option<(f32, f32)> {
    if content_h <= viewport_h || content_h <= 0.0 || viewport_h <= 0.0 {
        return None;
    }
    let thumb = (viewport_h / content_h).clamp(0.08, 1.0);
    let max_scroll = (content_h - viewport_h).max(1.0);
    let top = (scroll / max_scroll).clamp(0.0, 1.0) * (1.0 - thumb);
    Some((top, thumb))
}

pub(crate) fn continuous_scroll_rail(
    p: Painter,
    track: Rect,
    scroll: f32,
    content_h: f32,
    viewport_h: f32,
) {
    let Some((top, h)) = continuous_rail_geom(scroll, content_h, viewport_h) else {
        return;
    };
    p.rrect(track, RAIL_RAD, RAIL_RAD, theme::RAIL_TRACK);
    p.rrect(
        Rect::new(track.x, track.y + top * track.h, track.w, h * track.h),
        RAIL_RAD,
        RAIL_RAD,
        theme::RAIL_FILL,
    );
}

// ---- Pending placeholders --------------------------------------------------------------------

/// **One light pass across a placeholder** — the pending sweep, and a deliberate addition to this
/// product's motion set (`Person Screen.dc.html`: "it does not pulse, scale or move the block
/// itself"). Nothing here pulses; arrival is the dip, which is `Xfade`'s job, not this one's.
pub const SKELETON_PERIOD_MS: u32 = 1500;
/// The resting fill of a TEXT-line placeholder, and the peak the sweep lifts it to.
const SKEL_BAR_A: f32 = 0.08;
const SKEL_SHEEN_A: f32 = 0.06;
/// How wide the moving highlight is, as a fraction of the block it crosses.
const SKEL_BAND: f32 = 0.35;
/// The sweep is drawn as a few flat strips rather than one interpolated quad. At six per cent alpha
/// the steps are invisible on the panel, and it keeps the whole thing on `Painter::rect` — no
/// `grad4` corner convention to get wrong, and no scissor to pair.
const SKEL_STRIPS: usize = 6;

/// Phase 0..1 from a millisecond clock, for a caller accumulating its own — [`Spinner::phase`]'s
/// pattern. Stepping the clock itself needs no `nj_machine::idle` report (see that module: the six phase
/// accumulators tick unconditionally); the report is [`skeleton_sheen`]'s, from the draw, exactly
/// as [`Spinner::draw`] does it and for the same reason recorded there.
pub(crate) fn skeleton_phase(ms: u32) -> f32 {
    (ms % SKELETON_PERIOD_MS) as f32 / SKELETON_PERIOD_MS as f32
}

/// The sweep, over whatever ground the caller has already laid down.
///
/// Reports to `nj_machine::idle` from HERE, not from the clock that feeds it — `Spinner::draw`'s rule:
/// only a placeholder actually ON SCREEN should hold the loop awake, and reporting from draw can
/// only latch the gate on for the next frame, never off, so it self-sustains for exactly as long as
/// something keeps drawing one.
fn skeleton_sheen(p: Painter, r: Rect, rad: f32, phase: f32) {
    nj_machine::idle::invalidate();
    let band = (r.w * SKEL_BAND).max(1.0);
    // travel from fully off the left edge to fully off the right, so the block is clean at both
    // ends of the cycle rather than starting mid-flash
    let centre = r.x - band * 0.5 + phase * (r.w + band);
    let step = band / SKEL_STRIPS as f32;
    for k in 0..SKEL_STRIPS {
        let sx = centre - band * 0.5 + step * k as f32;
        let (x0, x1) = (sx.max(r.x), (sx + step).min(r.x + r.w));
        if x1 <= x0 {
            continue;
        }
        // a linear tent, peaking at the band's centre
        let t = 1.0 - ((sx + step * 0.5 - centre).abs() / (band * 0.5)).clamp(0.0, 1.0);
        let c = theme::white(SKEL_SHEEN_A * t);
        p.rect(Rect::new(x0, r.y, x1 - x0, r.h), rad, c, c, 0.0);
    }
}

/// **A line of text that has not arrived** — a chip-radius bar at the run's own measured height, so
/// the block it stands in keeps its height and nothing reflows when the words land.
pub(crate) fn skeleton_bar(p: Painter, r: Rect, phase: f32) {
    let rad = r.h * 0.5; // a chip radius on a text-line bar IS its half height
    let base = theme::white(SKEL_BAR_A);
    p.rect(r, rad, base, base, 0.0);
    skeleton_sheen(p, r, rad, phase);
}

/// **Artwork that has not arrived** — the structural top→bottom sheet [`Art`] already draws for a
/// missing poster, plus the sweep. Same geometry and same resting card edge as the real tile, which
/// is the whole point: the placeholder is the card, without the picture.
pub(crate) fn skeleton_sheet(p: Painter, r: Rect, rad: f32, phase: f32) {
    p.rect(r, rad, theme::SKELETON_TOP, theme::SKELETON_BOT, 0.0);
    skeleton_sheen(p, r, rad, phase);
    // the resting card edge, so the placeholder holds its own boundary exactly as the real tile does
    p.rring(r, rad, theme::CARD_SHEEN_W, theme::CARD_SHEEN);
}

// ---- Tracked caps: a kicker drawn letter by letter -----------------------------------------------

/// Draw a letter-tracked kicker. The text backend has no letter-spacing, so tracking is always a
/// per-character pen advance, and the label is pre-split into `&CStr` literals so a kicker that
/// draws every frame costs no allocation — deliberately awkward, and only worth it for a CONSTANT
/// word, which every tracked kicker in this app is. Returns the run's width, so nothing has to
/// measure ahead (a measure-only half existed and never had a caller).
pub(crate) fn tracked_run(
    p: Painter,
    chars: &[&std::ffi::CStr],
    x: f32,
    y: f32,
    sz: c_int,
    col: [f32; 4],
    bold: c_int,
    track: f32,
) -> f32 {
    let mut bx = x;
    for c in chars {
        bx += p.text(c.as_ptr(), bx, y, sz, col, 0, bold);
        bx += track;
    }
    (bx - track - x).max(0.0)
}

// ---- Litefin's hero ground: the backdrop faded into the page by its two masks ----------------

/// One layer of [`hero_backdrop`]: a resolved texture and its size, the Ken Burns scale of its box
/// and its alpha (its reveal times its share of a crossfade).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HeroArt {
    pub(crate) tex: u32,
    pub(crate) size: (f32, f32),
    pub(crate) zoom: f32,
    pub(crate) a: f32,
}

/// **The hero ground** (`docs/litefin-look.md` 4.1, geometry in [`crate::ui::landing_hero`]): each
/// layer's picture covering its box from the top, then the box's two alpha masks laid over all of
/// it as PAGE GROUND at `1 - alpha`. The page is that ground, so ground at `1 - m` over the art is
/// the art at alpha `m` on the page: one ramp quad per mask segment rather than a masked program,
/// and no scrim colour of its own. A crossfade's two layers share the larger box's masks; the
/// smaller box sits inside it, so its edge stays under the mask's first few percent.
pub(crate) fn hero_backdrop(p: Painter, layers: &[HeroArt]) {
    use crate::ui::landing_hero::{art_box, HEIGHT, MASK_X, MASK_Y};
    // nothing of the art reaches the page below the vertical mask's last stop, so none is drawn
    let end = MASK_Y[MASK_Y.len() - 1].0 * HEIGHT;
    let mut zoom: Option<f32> = None;
    for l in layers.iter().filter(|l| l.tex != 0 && l.a > 0.01) {
        let b = art_box(l.zoom);
        let mut uv = b.cover_uv(l.size.0, l.size.1, crate::ui::Crop::Centre);
        uv[1] = 0.0; // the picture rides the top of its box (object-position: top)
        uv[3] *= (end - b.y) / b.h;
        p.tex_uv(l.tex, uv, Rect::new(b.x, b.y, b.w, end - b.y), 0.0, theme::with_a(theme::TINT_WHITE, l.a));
        zoom = Some(zoom.map_or(l.zoom, |z: f32| z.max(l.zoom)));
    }
    if let Some(zoom) = zoom {
        let b = art_box(zoom);
        let ground = |a: f32| theme::with_a(theme::SURFACE_APP, 1.0 - a);
        for w in MASK_X.windows(2) {
            let (l, r) = (ground(w[0].1), ground(w[1].1));
            p.grad4(Rect::new(b.x + w[0].0 * b.w, b.y, (w[1].0 - w[0].0) * b.w, end - b.y), [l, r, r, l]);
        }
        for w in MASK_Y.windows(2).filter(|w| w[0].1 < 1.0 || w[1].1 < 1.0) {
            let (t, d) = (ground(w[0].1), ground(w[1].1));
            p.grad4(Rect::new(b.x, w[0].0 * HEIGHT, b.w, (w[1].0 - w[0].0) * HEIGHT), [t, t, d, d]);
        }
    }
}

/// The focused hero's accent band at `band` (0..1) over the whole slide, its copy included:
/// transparent at the foot, peaking a fifth of the way up and gone at the top
/// (`.hero-carousel-container::after`, whose stacking context keeps it under the rows).
pub(crate) fn hero_band(p: Painter, band: f32) {
    if band <= 0.0 {
        return;
    }
    let ink = theme::home_lf::FOCUS_BAND;
    let (peak, clear) = (theme::with_a(ink, ink[3] * band), theme::with_a(ink, 0.0));
    let h = crate::ui::landing_hero::HEIGHT;
    let (top, foot) = (0.80 * h, 0.95 * h);
    p.grad4(Rect::new(0.0, 0.0, crate::ui::consts::SCR_W, top), [clear, clear, peak, peak]);
    p.grad4(Rect::new(0.0, top, crate::ui::consts::SCR_W, foot - top), [peak, peak, clear, clear]);
}

// ---- CircleButton: circular disc + centered glyph, same keyed/unkeyed ControlStyle family as
// Button / TransportButton. The hero + detail +/i/> circles. ----

/// The **UNFURL**: a focused disc grows rightward into a capsule carrying the verb its press
/// performs — icon-only at rest, LABELLED on focus.
///
/// It exists because a disc is the one control that cannot say what it does. At ten feet a bare
/// tick teaches nobody, and a television has no hover to hint with, so focus is the only moment the
/// user can be told. Resting geometry is unchanged: at `e == 0` every number below cancels and the
/// control is the same 60px disc it has always been, which is what lets the whole family opt in
/// without a second widget beside it.
///
/// The open capsule is `LEAD + icon + GAP + label + TAIL` — deliberately asymmetric, the icon side
/// tighter, because a round glyph reads as further from a capsule's end than a stem does. The
/// glyph's own inset slides from the disc's centring to [`DISC_LABEL_LEAD`] across the unfurl, so
/// both ENDS are exactly the shapes the design draws rather than only the open one.
///
/// Sibling of the profile chip's own unfurl (`chip_cap`), and the same two rules apply: the width
/// is the ONE expression both the painter and the hit-test read ([`CircleButton::cap_w`]), and the
/// label rides the TAIL of the widening so its glyphs land in a capsule that has already made room
/// for them instead of smearing across the grow.
///
/// The two differ on ONE policy and deliberately: what happens when the word does not fit. The chip
/// ELIDES to a fixed `CHIP_NAME_MAX` budget, because a profile name is user data of any length and
/// a shortened name is still a name. A verb is not — half of "Mark as Unwatched" teaches less than
/// the tick it was meant to explain — so a control with a hard bound to respect asks
/// [`CircleButton::label_budget`] and shows the whole word or none of it
/// (`detail::watch_cap_at`). Both are right for their content; neither is the default.
const DISC_LABEL_LEAD: f32 = 26.0;
const DISC_LABEL_GAP: f32 = 14.0;
const DISC_LABEL_TAIL: f32 = 34.0;
/// The unfurl's stiffness — brisk, a touch stiffer than the hero slide (it was the retired profile
/// chip's own rate, the same interaction: a focused control opening far enough to name itself).
pub(crate) const K_DISC_UNFURL: f32 = 300.0;

pub struct CircleButton {
    pub frame: Rect,
    pub glyph: *const c_char,
    pub icon: Option<crate::ui::icons::Icon>, // vector glyph; overrides the text glyph when set
    pub focused: bool,
    pub style: ControlStyle,
    /// WHICH GROUND this disc stands on — see [`ControlGround`].
    pub ground: ControlGround,
    /// Local page palette. It is copied in by a Hero row and ignored on an unkeyed ground.
    pub palette: ControlPalette,
    /// The FOCUS POP, as a factor on the frame — see [`CircleButton::scale`].
    pub scale: f32,
    /// The unfurled label and how far open it is (0..1) — see [`DISC_LABEL_LEAD`]. `None` is a
    /// plain disc, which is what every caller that has not opted in still gets.
    pub label: Option<(*const c_char, f32)>,
}
impl CircleButton {
    pub fn new(glyph: *const c_char) -> Self {
        Self {
            frame: Rect::new(0.0, 0.0, 60.0, 60.0),
            glyph,
            icon: None,
            focused: false,
            style: ControlStyle::Accent,
            ground: ControlGround::Keyed,
            palette: ControlPalette::default(),
            scale: 1.0,
            label: None,
        }
    }
    /// Move the disc, keeping its own diameter. Enough for a plain disc, and NOT enough for an
    /// unfurled one — see [`frame`](Self::frame).
    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.frame.x = x;
        self.frame.y = y;
        self
    }
    /// Place AND size the control from a frame the caller already owns.
    ///
    /// The unfurl's width is [`cap_w`](Self::cap_w), which a row that accumulates its controls has
    /// already had to compute — so it hands the whole rect over rather than an origin, and the
    /// shape drawn is by construction the shape that row reserved and the pointer grades against.
    /// [`at`](Self::at) would silently keep the bare 60, which draws a labelled capsule clipped to
    /// a disc.
    pub fn frame(mut self, r: Rect) -> Self {
        self.frame = r;
        self
    }
    /// Render a vector icon centred on the disc instead of the text glyph (e.g. a real
    /// chevron rather than a ">" character). Pass a bare-stroke icon — one that carries its
    /// own outline circle (Info) would double-ring against the disc face.
    pub fn icon(mut self, i: crate::ui::icons::Icon) -> Self {
        self.icon = Some(i);
        self
    }
    pub fn focused(mut self, f: bool) -> Self {
        self.focused = f;
        self
    }
    /// The [FOCUS POP](CTRL_FOCUS_SCALE), about the control's centre — normally
    /// [`CtlPop::scale`], which already folds the press dip in. `1.0` is the resting control, which
    /// is what every caller that has not opted in still gets.
    ///
    /// It scales the PLATE and the glyph box. It does not scale the unfurled label's TYPE: text in
    /// this app is sized off `theme::size`'s rungs and nothing may sit between them, so the word
    /// keeps its `BODY` 28 through the pop and only its placement moves. At 1.07 on a 60px control
    /// that is a two-pixel relative difference seen from three metres — the rung rule is worth more.
    pub fn scale(mut self, s: f32) -> Self {
        self.scale = s;
        self
    }
    pub fn style(mut self, s: ControlStyle) -> Self {
        self.style = s;
        self
    }
    /// Stand this disc on a named [`ControlGround`] — see [`Button::ground`].
    pub fn ground(mut self, g: ControlGround) -> Self {
        self.ground = g;
        self
    }
    /// Answer to a page-drawn ground with the palette it published for this control row.
    pub fn palette(mut self, palette: ControlPalette) -> Self {
        self.palette = palette;
        self
    }
    /// Give this disc the [UNFURL](DISC_LABEL_LEAD): `text` is the verb the press performs and `e`
    /// (0..1) is how far open the capsule is — normally a focus spring, so the control reads as one
    /// object opening rather than a label appearing beside it. `e == 0` draws the plain disc.
    ///
    /// The caller must size [`frame`](Self::frame)`.w` with [`cap_w`](Self::cap_w) at the same `e`:
    /// the drawn shape and the graded shape are one expression (`chip_cap`'s rule), which is what
    /// keeps a pointer clicking the capsule it can see. A frame that is NOT a capsule cancels the
    /// unfurl outright, glyph included — see [`cap_label_w`](Self::cap_label_w) — so a row that
    /// refused this control its room can keep passing `e` without drawing anything wrong.
    ///
    /// Three preconditions, none of which the builder can enforce:
    /// * **It needs [`icon`](Self::icon).** The label is drawn beside the vector glyph; the
    ///   text-glyph form has no run to sit next to, and silently ignores this.
    /// * **The label is drawn at [`theme::size::BODY`]**, the one control-label rung — measure with
    ///   the same size when sizing the frame, or the capsule and its word disagree.
    /// * **It clips, and the clip has no stack** (`gfx::clip_clear` is a bare `glDisable`). Drawing
    ///   an unfurled control INSIDE another scissor releases that outer clip for the rest of the
    ///   frame. Every caller today is unclipped; a scrolling list that adopts this is not.
    pub fn label(mut self, text: *const c_char, e: f32) -> Self {
        self.label = Some((text, e));
        self
    }

    /// The capsule width of a disc of diameter `d`, unfurled `e` (0..1) around a label measured
    /// `label_w` wide. The LAYOUT companion to `draw`, and the only place the open geometry is
    /// written down — a row accumulating this control's advance and the painter placing its glyphs
    /// read the very same number, at every phase of the animation.
    ///
    /// `label_w <= 0` is "no label", which collapses to the bare disc whatever `e` says.
    pub fn cap_w(d: f32, e: f32, label_w: f32) -> f32 {
        if label_w <= 0.0 {
            return d;
        }
        d + (Self::cap_open_w(d, label_w) - d) * e.clamp(0.0, 1.0)
    }

    /// The fully-open capsule — [`cap_w`](Self::cap_w) at `e == 1`, and the only place the open
    /// geometry is spelled out.
    fn cap_open_w(d: f32, label_w: f32) -> f32 {
        DISC_LABEL_LEAD + (d * DISC_ICON_RATIO).round() + DISC_LABEL_GAP + label_w + DISC_LABEL_TAIL
    }

    /// The widest label a disc of diameter `d` may unfurl when its row can spare only `room`
    /// beyond the bare disc — i.e. `cap_w(d, 1.0, label_budget(d, room)) - d == room`.
    ///
    /// The INVERSE of [`cap_w`](Self::cap_w), for a caller that has a fixed bound to respect and a
    /// label to decide about (`detail::watch_cap_at`). Having it here rather than re-derived at
    /// the call site is the same rule the width itself follows: the open geometry is written down
    /// once, so a budget and the shape it is a budget FOR cannot drift apart. Can come back
    /// negative, which means the disc cannot afford a label of any width at all.
    pub(crate) fn label_budget(d: f32, room: f32) -> f32 {
        room - (Self::cap_open_w(d, 0.0) - d)
    }

    /// The label width a capsule of DRAWN width `w` was sized for — the exact inverse of
    /// [`cap_w`](Self::cap_w) at the same `e`, so the widget can recover what its caller measured
    /// without being handed it a second time (two numbers that must agree are two numbers that can
    /// disagree; this way the frame is the single statement of the geometry, as `chip_cap`'s rule
    /// demands).
    ///
    /// `None` means **this frame is not a capsule**, and it is the widget's whole guard rather than
    /// a convenience: `w <= d` is a caller that asked for a label and was given a bare disc — a row
    /// that REFUSED the unfurl for want of room ([`label_budget`](Self::label_budget)), or a
    /// measurement that came back 0 because the font never opened — while its focus spring keeps
    /// climbing to 1. Answering with the arithmetic there returns a NEGATIVE label width, which
    /// then reads as "acres of tail air" to the fade ramp and lights the word up at full alpha; and
    /// the icon, slid by that same `e`, lands 26px into a 60px circle. Both are the disc drawn
    /// wrong, from a state the caller declared by handing over the frame.
    fn cap_label_w(d: f32, e: f32, w: f32) -> Option<f32> {
        if e <= 0.01 || w <= d {
            return None;
        }
        let lw = (w - d) / e - (Self::cap_open_w(d, 0.0) - d);
        (lw > 0.0).then_some(lw)
    }
}
impl View for CircleButton {
    fn draw(&self, _e: &Env, p: Painter) {
        let base = self.frame;
        let r = base.scaled(self.scale);
        let face = self.style.face(self.focused, self.ground, self.palette);
        let ink = face.ink;
        // The DISC is the frame's HEIGHT, not its width: an unfurled control is a capsule, and
        // taking the radius and the glyph box off `w` would swell both as it opened. At rest the
        // two are the same number, which is why every plain caller is unaffected.
        let d = r.h;
        // At REST the edge holds it; FOCUS lifts it (`control_cast`). An unfurled disc is a
        // capsule, and `face_box`/`face_outline` pick that up from the shape rather than from a
        // flag — at rest both are no-ops on a circle.
        let plate = face_box(r);
        if self.focused {
            control_cast(p, plate, plate.h * 0.5);
        }
        control_rim(
            p,
            plate,
            plate.h * 0.5,
            face.top,
            face.body,
            self.focused,
            self.ground,
        );
        // Resolve the unfurl from the FRAME, before anything is placed by it: `cap_label_w` answers
        // `None` for a frame that is not a capsule, and that verdict governs the GLYPH as well as
        // the word. Sliding the icon off the caller's raw `e` was a real defect — a control whose
        // row refused it the room still had a focus spring climbing to 1, and the check kicked 12px
        // right inside a disc that never grew.
        // …from the frame the CALLER sized (`cap_w`), not the popped one: the pop is a factor
        // applied after the row's layout, so asking the base frame is how the widget recovers the
        // label width its caller actually measured.
        let unfurl = self.label.and_then(|(text, e)| {
            Some((
                text,
                e.clamp(0.0, 1.0),
                Self::cap_label_w(base.h, e, base.w)?,
            ))
        });
        let e = unfurl.map(|(_, e, _)| e).unwrap_or(0.0);
        if let Some(icon) = self.icon {
            // vector glyph at the shared DISC_ICON_RATIO box, so every round control carries its
            // icon at one ratio — centred on the DISC, sliding to the open capsule's lead inset as
            // the unfurl opens (see `DISC_LABEL_LEAD`).
            let isz = (d * DISC_ICON_RATIO).round();
            let closed_inset = (d - isz) * 0.5;
            // the capsule's own air rides the pop with it, so a popped capsule is the same shape
            // scaled and not a wider one wearing the original padding
            let (lead, gap, tail) = (
                DISC_LABEL_LEAD * self.scale,
                DISC_LABEL_GAP * self.scale,
                DISC_LABEL_TAIL * self.scale,
            );
            let ix = r.x + closed_inset + (lead - closed_inset) * e;
            crate::ui::icons::draw(
                p,
                icon,
                Rect::new(ix, r.y + (r.h - isz) * 0.5, isz, isz),
                ink,
            );
            if let Some((text, _, label_w)) = unfurl {
                // The label rides the TAIL of the widening, and the ramp is the GEOMETRY rather
                // than a chosen fraction of it: it fades in exactly as fast as the capsule's tail
                // air ([`DISC_LABEL_TAIL`]) opens up behind the last glyph. So the word is at zero
                // the instant it would still be overhanging, and at full only once the capsule has
                // the whole run plus its own end air — which is what stops a half-cut glyph being
                // legible for a few frames of every focus move. A magic 0.55 could not do that: the
                // crossover moves with the LABEL's width, and this row's two differ by 34px.
                let tx = ix + isz + gap;
                let a = (((r.x + r.w) - (tx + label_w * self.scale)) / tail).clamp(0.0, 1.0);
                if a > 0.004 {
                    let ty = nj_gfx::text::text_vcenter_y(theme::size::BODY, 1, r.y + d * 0.5);
                    // …and the clip stays, as the backstop the ramp makes invisible: a caller whose
                    // frame does not come from `cap_w` would otherwise paint a label out over the
                    // page (the design's own `overflow:hidden`).
                    p.clip(r);
                    p.alpha(a).text(text, tx, ty, theme::size::BODY, ink, 0, 1);
                    p.clip_clear();
                }
            }
        } else {
            // text glyph centred on the disc by its cap band (layout ≠ paint), not a hand-tuned y
            crate::ui::label::Label::new(self.glyph, crate::ui::theme::size::HEADLINE, ink)
                .h(crate::ui::label::HAlign::Center)
                .draw(p, Rect::new(r.x, r.y, d, d));
        }
    }
}

// ---- PageDots: page indicators, Litefin's dots (`.indicator-dots .hero-dot`) ----
pub struct PageDots {
    pub count: usize,
    pub active: usize,
    pub x: f32,
    pub y: f32,
}
impl PageDots {
    const GAP: f32 = 10.0; // edge gap between neighbours
    pub(crate) const DOT: f32 = 8.0; // every dot's diameter: the current one is lit, not longer

    pub fn new(count: usize) -> Self {
        Self {
            count,
            active: 0,
            x: 0.0,
            y: 0.0,
        }
    }
    /// the lit dot (0-based); clamped into range so a stale index degrades to the last dot.
    pub fn active(mut self, i: usize) -> Self {
        self.active = if self.count == 0 {
            0
        } else {
            i.min(self.count - 1)
        };
        self
    }
    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.x = x;
        self.y = y;
        self
    }
    /// Place the row so its RIGHT edge is at `right` — the hero's bottom-right pager. Resolves to a
    /// left origin through [`width`](Self::width), so `draw` stays one left-to-right walk.
    pub fn right_at(self, right: f32, y: f32) -> Self {
        let w = self.width();
        self.at(right - w, y)
    }
    /// The row's drawn width — the SAME advance `draw` walks, so a placed row can't drift from the
    /// pixels.
    pub fn width(&self) -> f32 {
        if self.count == 0 {
            return 0.0;
        }
        self.count as f32 * Self::DOT + (self.count - 1) as f32 * Self::GAP
    }
}
impl View for PageDots {
    fn draw(&self, _e: &Env, p: Painter) {
        for d in 0..self.count {
            let col = if d == self.active { theme::home_lf::DOT_ACTIVE } else { theme::home_lf::DOT_IDLE };
            let x = self.x + d as f32 * (Self::DOT + Self::GAP);
            p.rect(Rect::new(x, self.y, Self::DOT, Self::DOT), Self::DOT * 0.5, col, col, 0.0);
        }
    }
}

// ---- Spinner: dots around a circle, the leading one bright and trailing into a fade. A loading/
// buffering indicator (e.g. the player HUD while a seek resolves). `phase` (ms) drives rotation. ----
pub struct Spinner {
    pub cx: f32,
    pub cy: f32,
    pub r: f32,
    pub phase: u32,
    pub col: [f32; 4],
    pub dots: usize,
    pub dot_r: f32,
}
impl Spinner {
    /// One full revolution, in ms. Public because a caller that accumulates its own phase clock
    /// can only wrap it losslessly on a whole period (see `home.rs`'s `status_ms`).
    pub const PERIOD_MS: u32 = 760;
    /// The **page** radius — a wait that owns a whole surface: the [`StatusOverlay`] read-out, and
    /// any screen whose content is simply not there yet. Sized to be read from the couch.
    ///
    /// Associated consts on the widget are the house pattern ([`PageDots::DOT`]/`PILL`/`GAP`): a
    /// spinner radius is neither a text size nor a gap, so it does not belong in `theme.rs`.
    pub const R_PAGE: f32 = 22.0;
    /// The **inline** radius — a mark that belongs to one line of text or one control, sized to sit
    /// on a `size::CAPTION` cap band. Anything that must sit *beside* something rather than *over*
    /// everything uses this.
    pub const R_INLINE: f32 = 12.0;
    /// Dot radius for a ring of radius `r` — the ONE place the ratio lives, so a layout can measure
    /// a spinner's real extent without re-deriving it (0.28·r ≈ the HUD spinner's original 3.4 at
    /// r=12, floored so a tiny ring still has visible dots).
    pub fn dot_r(r: f32) -> f32 {
        (r * 0.28).max(3.0)
    }
    /// The leading GUTTER an inline spinner takes before the line of text it belongs to — the
    /// ring's full extent (dots included) and an `XS` gap — so a line that is "on its way" moves
    /// over by this and nothing else. Read by [`StatusOverlay`]'s busy note and by any column of
    /// fine print that marks a line the same way.
    pub fn inline_gutter() -> f32 {
        2.0 * (Self::R_INLINE + Self::dot_r(Self::R_INLINE)) + theme::space::XS
    }
    /// An inline spinner in the leading gutter that starts at `x`, centred on `cy` — the middle
    /// of the cap band of the line it marks.
    pub fn leading(x: f32, cy: f32) -> Self {
        Self::new(x + Self::R_INLINE + Self::dot_r(Self::R_INLINE), cy, Self::R_INLINE)
    }
    pub fn new(cx: f32, cy: f32, r: f32) -> Self {
        // dot size scales WITH the ring radius, so a big spinner reads as a bigger spinner, not the
        // same tiny dots on a wider circle.
        Self {
            cx,
            cy,
            r,
            phase: 0,
            col: [1.0, 1.0, 1.0, 1.0],
            dots: 10,
            dot_r: Self::dot_r(r),
        }
    }
    pub fn phase(mut self, ms: u32) -> Self {
        self.phase = ms;
        self
    }
    pub fn tint(mut self, c: [f32; 4]) -> Self {
        self.col = c;
        self
    }
}
impl View for Spinner {
    fn draw(&self, _e: &Env, p: Painter) {
        // A spinner is driven by a CLOCK, not a spring, so `nj_machine::idle`'s spring instrumentation
        // cannot see it: before this line, a Home waiting on /hubs — the exact state the read-out
        // exists for — drew a STOPPED spinner. Reported here, in `draw`, and that is deliberate:
        // only a spinner actually ON SCREEN should hold the loop awake, whereas the six phase
        // accumulators that feed it tick unconditionally at the top of their screens' update.
        //
        // Reporting from a draw is sound in exactly one direction — it can only latch the gate ON
        // for the next frame, never off — and it self-sustains: the landing that started the load
        // presents frame 1, whose draw reports and so buys frame 2, until nothing draws a spinner.
        // It relies on `should_present` taking-and-clearing rather than `note_present` clearing
        // after the draw, which would destroy this report on the frame it is raised.
        //
        // Not while the screenshot pipeline holds the clocks (`stillclock`): the phase this draws
        // from is then a constant, so the next frame would be identical and a waiting screen
        // (the sign-in QR's "Waiting for you to sign in…") could never come to rest.
        if !nj_machine::motion::phase_clocks_held() {
            nj_machine::idle::invalidate();
        }
        let t = (self.phase % Self::PERIOD_MS) as f32 / Self::PERIOD_MS as f32;
        for i in 0..self.dots {
            let ang =
                i as f32 / self.dots as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            let lead = (t - i as f32 / self.dots as f32).rem_euclid(1.0);
            let a = (1.0 - lead) * 0.85 + 0.12; // bright at the leading dot, fading behind it
            let c = [self.col[0], self.col[1], self.col[2], self.col[3] * a];
            let (dx, dy) = (self.cx + self.r * ang.cos(), self.cy + self.r * ang.sin());
            let d = self.dot_r;
            p.rect(Rect::new(dx - d, dy - d, 2.0 * d, 2.0 * d), d, c, c, 0.0);
        }
    }
}

// ---- AmbientWash: the full-screen four-corner colour wash keyed to an item's artwork. ----

/// The **ambient wash** — a page-wide bilinear gradient taken from an item's PMS `UltraBlurColors`
/// corners, which is how home's backdrop, detail's below-hero ground and the person page all say
/// "this screen is about *this* artwork" without paying for a full-bleed image.
///
/// It exists as a component because the non-obvious part is not the gradient, it is the FADE.
/// [`Painter::ambient`](crate::ui::Painter::ambient) writes opaque pixels (its `dim` scales the
/// corners toward BLACK, which is a different thing), so a wash cannot be cross-faded from ONE
/// ITEM'S colours to ANOTHER'S by alpha at all — the only way is to spring each corner channel
/// toward its target and keep drawing at full strength. That is twelve springs, and a screen that
/// hand-rolls them ends up doing index arithmetic over a flat array. Fading a wash toward the app's
/// own GROUND is the one case the cascade *does* handle, and it belongs to the cascade: an alpha
/// below 1 mixes the corners toward [`theme::SURFACE_APP`] inside `Painter::ambient`, which is what
/// lets a whole page — wash included — dip for [`ui::nav`](crate::ui::nav)'s route transition.
///
/// Blend the corners toward a base surface with [`theme::mix`] before handing them over: a wash
/// keyed at full strength is a photograph, not a wash, and mixing toward [`theme::SURFACE_APP`]
/// means "no artwork" is simply the app's own flat ground with no special case.
#[derive(Clone, Copy)]
pub(crate) struct AmbientWash {
    /// corner-major, `Painter::ambient`'s order: top-left, top-right, bottom-right, bottom-left.
    corners: [[Spring; 3]; 4],
}

/// The luminance ceiling a GROUND colour is held to before it is mixed toward the surface.
/// UltraBlur corners are SAMPLED FROM THE ARTWORK, so a white poster hands us a near-white corner:
/// at any weight strong enough to see, that lifts the page off the palette's dark end and takes the
/// [`theme::TEXT_TERTIARY`] fine print sitting on it along — an uncapped white corner at
/// [`AmbientWash::GROUND_W`] puts `TEXT_TERTIARY`/`size::CAPTION` at **2.10:1**, under the 3:1
/// large-text floor; capped here the worst source (saturated green) is **3.67:1** and a white poster
/// **3.83:1**. A hard ceiling, not a tone map: only brightness is spent, the corner's hue and its
/// channel balance survive untouched, which is all a wash is saying. Rec.709 weights over the stored
/// display-encoded values — a ground-brightness knob, not colour management.
///
/// Why 0.42 and not higher: the legibility constraint only binds near **0.59** (green hits 3.0:1
/// there), so this is chosen design-first — a ceiling that let a white poster produce a mid-grey
/// page would betray the word "dark", and the contrast margin then falls out for free. Every number
/// above is MEASURED, by `a_ground_never_outshines_the_fine_print_that_sits_on_it`.
const GROUND_LUMA: f32 = 0.42;

/// One artwork corner, held under [`GROUND_LUMA`]. A scalar multiply (via [`theme::dim`]) rather
/// than a per-channel clamp, which would desaturate the corner instead of dimming it.
fn ground_capped(c: [f32; 3]) -> [f32; 4] {
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    theme::dim(
        [c[0], c[1], c[2], 1.0],
        if y > GROUND_LUMA {
            GROUND_LUMA / y
        } else {
            1.0
        },
    )
}

impl AmbientWash {
    /// The dissolve rate every page wash shares — deliberately slower than any focus spring (the
    /// person mock's `.5s ease`): the wash is atmosphere, and snapping it with the focus pop reads
    /// as the SCREEN changing rather than the subject. One rate, because two pages dissolving at two
    /// speeds are two different products.
    pub(crate) const K: f32 = 80.0;

    /// How far a GROUND leans from the app surface toward an item's own colour — the strength every
    /// item-keyed wash shares (the person page's focused-card state, the detail page's whole page).
    /// One value, because "this screen is about THIS artwork" should be the same statement wherever
    /// it is made; the per-corner SHAPE stays each screen's business (person tapers its bottom
    /// corners toward its shelves, detail keeps the artwork's own corner arrangement).
    ///
    /// The bound is what makes it safe to be the page's ground everywhere: with the source capped at
    /// [`GROUND_LUMA`], the brightest ground is ≈60/255 and the darkest (black corners) ≈33/255 —
    /// clear of the near-black 25/255 that [`theme::SURFACE_APP`]'s own doc rejects as too dark for
    /// a card shadow to read against. No floor constant is needed; `GROUND_W ≤ 0.26` IS the floor.
    pub(crate) const GROUND_W: f32 = 0.26;

    /// How close a wash must be to the colour it is drawn over before a screen stops drawing it —
    /// [`is_flat`](Self::is_flat)'s epsilon, ONE 8-bit code, below which the panel cannot show a
    /// difference. ([`theme::SURFACE_APP`] is snapped to exact 8-bit codes for the `GL_DITHER`
    /// reason in its own doc, so that is the unit this is reasoned in.)
    ///
    /// It lives beside the type rather than in a screen because the skip is worth ~2.07M fragments
    /// a frame on two different pages, and it was a private `home.rs` constant while `detail.rs`
    /// simply lacked the test — one value here is what stops the two from drifting apart again.
    pub(crate) const FLAT_EPS: f32 = 1.0 / 255.0;

    /// A dissolve target: corner `i` mixed from [`theme::SURFACE_APP`] toward `src[i]` by `w[i]`
    /// ("how much of this source shows through at that corner"). The mix is the TYPE's contract (see
    /// the docs above) rather than a loop each screen writes, so "no artwork" is the app's own flat
    /// ground on every page **by construction** — `w = 0` returns exactly the surface. Colours that
    /// came from ARTWORK go through [`keyed`](Self::keyed), which caps them first.
    pub(crate) fn target(src: [[f32; 4]; 4], w: [f32; 4]) -> [[f32; 4]; 4] {
        std::array::from_fn(|i| theme::mix(theme::SURFACE_APP, src[i], w[i]))
    }

    /// [`target`](Self::target) for corners taken from an item's `UltraBlurColors` envelope
    /// (`PmsMovie::blur` / `metadata::Detail::blur`), each held under [`GROUND_LUMA`] first. Every
    /// artwork-keyed wash goes through here; a palette token (the resting warm tint) does not need
    /// it, because we chose that value.
    pub(crate) fn keyed(blur: [[f32; 3]; 4], w: [f32; 4]) -> [[f32; 4]; 4] {
        std::array::from_fn(|i| Self::keyed_one(blur[i], w[i]))
    }

    /// **ONE artwork colour graded into a ground** — [`keyed`](Self::keyed) for a single sample,
    /// and the function `keyed` is now four of.
    ///
    /// It exists because the four-corner envelope is no longer the only shape a ground can have:
    /// [`ui::underlay`](crate::ui::underlay) grades 120 cells the same way, and "the same way" has
    /// to mean the same CODE or the two shapes drift into two palettes. The cap and the lean are
    /// the legibility contract `widgets_ambient_ground_tests.rs` grades; nothing may reach them by
    /// re-writing this line.
    pub(crate) fn keyed_one(c: [f32; 3], w: f32) -> [f32; 4] {
        theme::mix(theme::SURFACE_APP, ground_capped(c), w)
    }

    /// A wash resting flat on one colour — what a page opens as before any item keys it.
    pub(crate) const fn flat(c: [f32; 4]) -> Self {
        AmbientWash {
            corners: [[Spring::at(c[0]), Spring::at(c[1]), Spring::at(c[2])]; 4],
        }
    }
    /// Jump straight to `target` (no dissolve) — on mount, so the PREVIOUS item's colours never
    /// dissolve across a page that has just changed subject.
    pub(crate) fn jump(&mut self, target: [[f32; 4]; 4]) {
        for (c, t) in self.corners.iter_mut().zip(target) {
            for (sp, v) in c.iter_mut().zip(t) {
                sp.jump(v);
            }
        }
    }
    /// Dissolve toward `target` at rate `k`. Every channel shares one rate, so the corners move as
    /// one wash rather than twelve independent fades.
    pub(crate) fn step(&mut self, target: [[f32; 4]; 4], k: f32, dt: f32) {
        for (c, t) in self.corners.iter_mut().zip(target) {
            for (sp, v) in c.iter_mut().zip(t) {
                sp.step(v, k, dt);
            }
        }
    }
    /// Is this wash within `eps` of the flat colour `c` on every corner channel? A wash that has
    /// resolved to the app's own clear colour is a ~2M-fragment full-screen fill that changes
    /// nothing, and a screen must be able to skip it. (`eps` is naturally one 8-bit code —
    /// [`theme::SURFACE_APP`] is snapped to exact codes for the `GL_DITHER` reason in its own doc,
    /// so that is the unit this value is reasoned in.) Its own method because the corner springs are
    /// private.
    pub(crate) fn is_flat(&self, c: [f32; 4], eps: f32) -> bool {
        self.corners
            .iter()
            .all(|q| q.iter().zip(c).all(|(s, v)| (s.pos - v).abs() <= eps))
    }
    /// **The colour this wash puts at one point of `r`** — the same bilinear the shader evaluates
    /// (`fs_ambient.frag`: `mix(mix(tl,tr,u), mix(bl,br,u), v)`), on the CPU, for a caller that has
    /// to paint something the exact colour of the ground it is standing on.
    ///
    /// Its one caller is an EDGE FADE: a scissor-clipped strip is cut at a hard line, and the only
    /// way to dissolve that cut rather than disguise it is to lay the ground's own colour over it
    /// at a falling alpha. A guessed constant cannot do that here — this ground is keyed to the
    /// host's artwork and is a different colour on every page.
    pub(crate) fn sample(&self, r: Rect, x: f32, y: f32) -> [f32; 3] {
        let u = if r.w > 0.0 { ((x - r.x) / r.w).clamp(0.0, 1.0) } else { 0.0 };
        let v = if r.h > 0.0 { ((y - r.y) / r.h).clamp(0.0, 1.0) } else { 0.0 };
        let c = |i: usize, q: usize| self.corners[q][i].pos;
        std::array::from_fn(|i| {
            let top = c(i, 0) + (c(i, 1) - c(i, 0)) * u; // tl -> tr
            let bot = c(i, 3) + (c(i, 2) - c(i, 3)) * u; // bl -> br
            top + (bot - top) * v
        })
    }
    /// Paint it over `r`. Opaque — this REPLACES what is under it (see the type docs), so it belongs
    /// at the bottom of a screen's draw, standing in for the flat clear.
    ///
    /// Always dithered, on every screen and every frame — including under Home's diving hero and
    /// Detail's scrolling still (`gfx::draw_ambient` records the three gates that were tried and
    /// why each came back out).
    pub(crate) fn draw(&self, p: Painter, r: Rect) {
        p.ambient(r, 1.0, self.corners.map(|c| [c[0].pos, c[1].pos, c[2].pos]));
    }
    /// **Paint the whole GROUND over `r`: this wash, the photograph dissolving over it, and the
    /// screen's atmospheric ramp of scrim ink over both** — the same picture as
    /// [`draw`](Self::draw), then `p.tex_uv(art…)` at radius 0, then the ramp's full-width `rect`s,
    /// in as few passes as the geometry allows: ONE per pixel. Where the art covers the wash, wash,
    /// art and ramp are one fragment ([`Painter::ambient_art`]); everywhere else the wash carries
    /// the ramp per vertex ([`Painter::ambient_inked`]), which costs its fragment nothing.
    ///
    /// **Why.** Home's snap dive and Detail's scroll both fade their photograph while it moves, so
    /// on every frame of the motion the wash shows through it — and the layered ground was the
    /// wash, the art and the ramp as three full-width passes over most of the panel. On the dev
    /// television (Mali-T820, arithmetic-bound) Home's dive ran 17–24 ms a frame across its whole
    /// curve; `gfx::draw_art_wash` records what each fold was worth.
    ///
    /// The art is always drawn — in the wash's pass when it can be, as its own layer over the wash
    /// otherwise (a driver that refused the program, or art that misses `r`). Returns whether it
    /// TOOK THE RAMP: when it did not, the ramp is the caller's to draw as before, over all of
    /// this — which is also the answer whenever the ramp would have had to go UNDER an art that
    /// stayed a layer of its own. Every fallback is the layered picture, never a different one.
    pub(crate) fn draw_ground(
        &self,
        p: Painter,
        r: Rect,
        art: Option<WashArt>,
        ramp: Option<WashRamp>,
    ) -> bool {
        let art = art.filter(|a| a.tex != 0);
        let split = art.and_then(|a| art_wash_split(r, a.rect)).filter(|_| nj_gfx::gfx::art_wash_ok());
        let art_taken = split.is_some();
        // The ramp sits over the art: it can only join the wash if the art did (or there is none).
        let ramp = ramp.filter(|_| nj_gfx::gfx::wash_ink_ok() && (art.is_none() || art_taken));
        let (over, bands) = match split {
            Some((over, bands)) => (Some(over), bands),
            None => (None, [Some(r), None, None, None]),
        };
        let corners = |b: Rect| {
            [(b.x, b.y), (b.x + b.w, b.y), (b.x + b.w, b.y + b.h), (b.x, b.y + b.h)]
                .map(|(x, y)| self.sample(r, x, y))
        };
        let knees = ramp.map_or([None, None], |q| [Some(q.stops[0].0), Some(q.stops[1].0)]);
        let ink = |b: Rect| {
            ramp.map_or(([0.0; 4], [0.0; 2]), |q| (q.ink, [q.alpha(b.y), q.alpha(b.y + b.h)]))
        };
        for band in bands.into_iter().flatten() {
            for b in cut_at(band, knees) {
                if ramp.is_some() {
                    p.ambient_inked(b, corners(b), ink(b));
                } else {
                    p.ambient(b, 1.0, corners(b));
                }
            }
        }
        if let (Some(over), Some(a)) = (over, art) {
            for b in cut_at(over, knees) {
                p.ambient_art(b, corners(b), a.tex, a.rect, a.uv, a.tint, ink(b));
            }
        }
        if !art_taken {
            if let Some(a) = art {
                p.tex_uv(a.tex, a.uv, a.rect, 0.0, a.tint);
            }
        }
        ramp.is_some()
    }
}

/// A screen's ATMOSPHERIC RAMP as [`AmbientWash::draw_ground`] carries it: `ink` (a scrim colour;
/// its alpha is ignored) at no alpha above `stops[0].0`, then linear through the three
/// `(y, alpha)` stops, holding the last below it. Home's is two segments with a midpoint knee;
/// Detail's is one, which it writes by repeating its foot.
#[derive(Clone, Copy)]
pub(crate) struct WashRamp {
    pub ink: [f32; 4],
    pub stops: [(f32, f32); 3],
}

impl WashRamp {
    /// The ramp's alpha at `y` — the curve the layered `rect`s drew, stop to stop.
    pub(crate) fn alpha(&self, y: f32) -> f32 {
        let [s0, s1, s2] = self.stops;
        let seg = |(y0, a0): (f32, f32), (y1, a1): (f32, f32)| {
            if y1 - y0 <= 0.0 { a1 } else { a0 + (a1 - a0) * ((y - y0) / (y1 - y0)).clamp(0.0, 1.0) }
        };
        if y <= s0.0 {
            0.0
        } else if y <= s1.0 {
            seg(s0, s1)
        } else {
            seg(s1, s2)
        }
    }
}

/// `r` cut into horizontal strips at the ramp's knees, each on a pixel row (the same
/// `ceil(y - 0.5)` rule as [`art_wash_split`]), so the ramp is ONE straight segment within every
/// strip — which is what lets a strip carry it per vertex exactly.
pub(crate) fn cut_at(r: Rect, knees: [Option<f32>; 2]) -> impl Iterator<Item = Rect> {
    let (top, bottom) = (r.y, r.y + r.h);
    let mut ys = [top, bottom, bottom, bottom];
    for (i, k) in knees.into_iter().enumerate() {
        if let Some(k) = k {
            ys[i + 1] = (k - 0.5).ceil().clamp(top, bottom);
        }
    }
    ys[3] = bottom;
    ys.sort_by(f32::total_cmp);
    (0..3).filter_map(move |i| {
        (ys[i + 1] > ys[i]).then(|| Rect::new(r.x, ys[i], r.w, ys[i + 1] - ys[i]))
    })
}

/// The photograph [`AmbientWash::draw_ground`] lays over its wash: exactly the arguments the
/// layered path hands `Painter::tex_uv` (at radius 0).
#[derive(Clone, Copy)]
pub(crate) struct WashArt {
    pub tex: u32,
    /// The quad the art is drawn at — a `Rect::cover` of the panel, typically, so larger than it.
    pub rect: Rect,
    /// The texture window it samples (`gfx::UV_FULL` for the whole picture).
    pub uv: [f32; 4],
    /// Its tint, alpha included: the dissolve.
    pub tint: [f32; 4],
}

/// **Where the wash over `r` meets art at `art`**: the overlap, which [`AmbientWash::draw_ground`]
/// paints as one pass, and the up-to-four bands of `r` the art does not reach (above, below, then
/// left and right of the overlap's own rows), which it paints as wash alone. `None` when the art
/// misses `r` entirely.
///
/// **Every edge is on a PIXEL boundary**, at the row or column where the art's own quad would have
/// started or stopped covering pixel centres (`ceil(edge - 0.5)`) — so the one-pass region holds
/// exactly the pixels the layered art covered, and the bands and the overlap share their edges
/// as identical integer vertices, which a rasteriser fills with neither a crack nor a double row.
pub(crate) fn art_wash_split(r: Rect, art: Rect) -> Option<(Rect, [Option<Rect>; 4])> {
    let px = |v: f32| (v - 0.5).ceil();
    let (rx0, ry0, rx1, ry1) = (r.x, r.y, r.x + r.w, r.y + r.h);
    let x0 = px(art.x).clamp(rx0, rx1);
    let y0 = px(art.y).clamp(ry0, ry1);
    let x1 = px(art.x + art.w).clamp(rx0, rx1);
    let y1 = px(art.y + art.h).clamp(ry0, ry1);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let band = |x: f32, y: f32, x_end: f32, y_end: f32| {
        (x_end > x && y_end > y).then(|| Rect::new(x, y, x_end - x, y_end - y))
    };
    Some((
        Rect::new(x0, y0, x1 - x0, y1 - y0),
        [
            band(rx0, ry0, rx1, y0),
            band(rx0, y1, rx1, ry1),
            band(rx0, y0, x0, y1),
            band(x1, y0, rx1, y1),
        ],
    ))
}

// ---- PageGround: the item-keyed ground a BROWSING screen stands on. ----

/// **The page ground under a browsing surface** — one [`AmbientWash`] dissolving toward the colours
/// of whatever item is under the focus ring, so a screen full of other people's artwork says which
/// one you are standing on without drawing anything extra to say it.
///
/// [`AmbientWash`] is the gradient and the dissolve; this is the POLICY three screens were about to
/// write three times over. It owns exactly the parts that are not obvious and that a screen gets
/// wrong quietly:
///
/// * **An item with no `UltraBlurColors` envelope HOLDS the ground where it is** rather than
///   dissolving to the flat surface. One artless poster in a row of colour would otherwise flash the
///   whole page grey and back on the way past — the person page states this rule in prose and this
///   is it as code, which is why that screen now shares this type.
/// * **A ground that has resolved to the app's own clear colour is not drawn.** It is a
///   ~2M-fragment full-screen pass that changes nothing; `AmbientWash::is_flat` is the test and
///   forgetting it costs a whole screen's fill-rate headroom for an invisible gradient.
/// * **This ground always dithers** — as every wash does now, the two with artwork sliding over
///   them included (`gfx::draw_ambient`). A browsing ground has nothing over it, so there is no
///   frame on which it is not the thing being looked at. [`draw`](Self::draw) takes no flag.
///
/// What stays the screen's business is the per-corner SHAPE — how far each corner leans — for the
/// reason [`AmbientWash::GROUND_W`] gives: one strength, many arrangements. [`CARD_W`](Self::CARD_W)
/// is the arrangement every item-keyed page ground shares.
pub(crate) struct PageGround {
    wash: AmbientWash,
    /// The last target actually keyed from artwork — what [`key`](Self::key) holds when the item
    /// under the ring carries no envelope. Starts at the app's own surface, so a page whose first
    /// item has no colours is simply the flat ground it already was.
    target: [[f32; 4]; 4],
}

impl PageGround {
    pub(crate) const SHAPE: &'static str = "PageGround{target:[[f32;4];4],corners:[[Spring{pos:f32,vel:f32};3];4]}";

    /// Canonical animation state, not GL resources. A held target and spring velocity influence
    /// subsequent frames even when two grounds currently draw the same colours.
    pub(crate) fn write_motion(&self, c: &mut nj_machine::machine::Canon) {
        let Self { wash, target } = self;
        let AmbientWash { corners } = wash;
        for corner in target {
            for component in corner { c.f32(*component); }
        }
        for corner in corners {
            for spring in corner { c.f32(spring.pos).f32(spring.vel); }
        }
    }

    /// The corner arrangement every item-keyed page ground leans by: [`AmbientWash::GROUND_W`]
    /// across the top, nearly gone by the bottom. Strongest where the page's own subject is and
    /// faint under the content rows, which is what keeps it atmosphere rather than a tint.
    ///
    /// One constant rather than one per screen: the STRENGTH is already shared
    /// ([`AmbientWash::GROUND_W`]), and two browsing screens leaning it in two different directions
    /// would read as two products. Home's billboard has its own, deliberately stronger arrangement
    /// (`screens::home`'s `HERO_WASH_W`) because there the wash stands in for a missing photograph rather than
    /// grounding body text.
    pub(crate) const CARD_W: [f32; 4] = [
        AmbientWash::GROUND_W,
        AmbientWash::GROUND_W,
        0.08,
        0.08,
    ];

    /// The faint warm tint a page's HEADER leans when no focused card hands the ground colours —
    /// strongest top-left, where the page's title sits.
    pub(crate) const HEADER_W: [f32; 4] = [0.10, 0.06, 0.02, 0.03];

    /// A header-and-cards page's ground target: `focused`'s `UltraBlurColors` along
    /// [`CARD_W`](Self::CARD_W) when it carries any, else the warm [`HEADER_W`](Self::HEADER_W)
    /// tint. The Person and Collection pages share it.
    pub(crate) fn page_target(focused: Option<TileFacts<'_>>) -> [[f32; 4]; 4] {
        match focused.and_then(|m| m.blur) {
            Some(blur) => AmbientWash::keyed(blur, Self::CARD_W),
            None => AmbientWash::target([theme::WASH_WARM; 4], Self::HEADER_W),
        }
    }

    /// A ground resting on the app's own surface — what a screen mounts with, and what it stays
    /// until something focused hands it colours.
    pub(crate) const fn new() -> Self {
        PageGround {
            wash: AmbientWash::flat(theme::SURFACE_APP),
            target: [theme::SURFACE_APP; 4],
        }
    }

    /// Dissolve toward the focused item's colours. `src` is that item's `UltraBlurColors` corners
    /// (`PmsMovie::blur`) or `None` when nothing focused carries any — **which holds the ground
    /// rather than clearing it**, per the type's doc.
    ///
    /// Called once per frame from a screen's update, before anything reads the ground.
    pub(crate) fn key(&mut self, src: Option<[[f32; 3]; 4]>, w: [f32; 4], dt: f32) {
        if let Some(b) = src {
            self.target = AmbientWash::keyed(b, w);
        }
        self.wash.step(self.target, AmbientWash::K, dt);
    }

    /// Dissolve toward an ALREADY-MIXED target — for a screen whose no-artwork state is a palette
    /// token rather than the flat surface (the person page's warm header tint), so "hold the last
    /// colour" is not the right answer there and the screen resolves both states itself. Takes an
    /// [`AmbientWash::target`] / [`AmbientWash::keyed`] result.
    pub(crate) fn key_target(&mut self, target: [[f32; 4]; 4], dt: f32) {
        self.target = target;
        self.wash.step(self.target, AmbientWash::K, dt);
    }

    /// [`key_target`](Self::key_target) without the dissolve — for a page that has just changed
    /// SUBJECT, where the previous subject's colours must not wash across the new one. Mount-time
    /// only; a jump on a page already on screen is a hard colour cut.
    pub(crate) fn jump_target(&mut self, target: [[f32; 4]; 4]) {
        self.target = target;
        self.wash.jump(self.target);
    }

    /// Paint the ground under everything, standing in for the flat clear — **skipped when it has
    /// resolved to that clear anyway**, which is the whole fill-rate argument in the type's doc.
    ///
    /// **Dithered on every frame, unconditionally**, like every wash. A browsing screen's ground is
    /// directly visible in every gutter of the grid standing on it, at rest and while it dissolves,
    /// and an undithered gradient there bands in slow diagonal treads that CRAWL as the ground
    /// moves. The fill-rate answer
    /// this type DOES keep is the one above it: a ground that has resolved to the clear colour is
    /// not drawn at all, so the frames it costs nothing are the frames it shows nothing.
    pub(crate) fn draw(&self, p: Painter, r: Rect) {
        if self.wash.is_flat(theme::SURFACE_APP, AmbientWash::FLAT_EPS) {
            return;
        }
        self.wash.draw(p, r);
    }

    /// Has the ground resolved to the app's own surface? The screens' own tests ask this; the draw
    /// path asks it for itself.
    #[cfg(test)]
    pub(crate) fn is_flat(&self) -> bool {
        self.wash.is_flat(theme::SURFACE_APP, AmbientWash::FLAT_EPS)
    }

    /// The ground's live corners, for a test that wants to see WHERE it got to.
    #[cfg(test)]
    pub(crate) fn corners(&self) -> [[f32; 3]; 4] {
        self.wash.corners.map(|c| [c[0].pos, c[1].pos, c[2].pos])
    }
}

// ---- StatusOverlay: a centred "something is happening / something failed" read-out — a Spinner
// (or nothing, for a terminal state) above one line of copy, optionally a REASON under it and the
// ONE action that answers it. The player HUD renders it for the
// states where NO PICTURE IS ON THE PANEL (Resolving/Connecting/Buffering/Seeking before this
// session's first frame, and Error, which is what a black screen used to be) — a seek over a live
// picture belongs to the transport's inline spinner instead, and `player_hud::busy_surface` is the
// ONE place that division is written down. `kind` picks the treatment, not the words: the caller
// supplies the caption so the state machine stays the single source of that string.
//
// The `frame` is THE AREA THE WAIT IS ABOUT, and the read-out centres on it — pass the region whose
// content is missing, not a region carved out to dodge other chrome. Home's catalog is the whole
// screen (`Rect::FULL`); the person page's shelves are one band, so it passes the band; the player's
// picture is the whole panel, so it passes `Rect::FULL` too. Carving the frame down to "avoid"
// nearby chrome pushes the read-out OFF the optical centre, which is exactly what the player's
// deleted `OVERLAY_BOTTOM` did — it centred the block at y=370 on a 1080 panel. A `Failed`
// read-out that FILLS THE PAGE — the sign-in failure, Home's, a Library section's — is the other
// case: it says so with [`StatusOverlay::page`], which hangs its verdict from
// [`StatusOverlay::FULL_ANCHOR_TOP`] in screen space with the reason and the row stacked under it,
// so all three share one verdict line (and one row line when they carry the same copy). The Library's still leaves its chrome
// live above it (a section failing is not the app failing, `Shared Sources.dc.html` D); it just no
// longer centres in the region under that chrome, which dropped its block well below Home's.
//
// **Up to three blocks, and each answers one question**: the verdict (what happened), the reason
// (why, and what is NOT broken), the action (the one thing to press). Two of the three are
// optional, and an absent one is ABSENT — no gap, no empty band, no draw call — so the read-outs
// that carry only a caption are laid out exactly as they were before the other two existed. ----
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusKind {
    /// in flight — spinner + secondary copy
    Working,
    /// terminal failure — no spinner, and **no warning colour**: the design system's
    /// `StatusOverlay` contract ("the app does not scold") draws a failed verdict bold at
    /// `size::TITLE` in `TEXT_SECONDARY`, the reason regular at `size::BODY` in `TEXT_SECONDARY`.
    /// See [`StatusOverlay::verdict_face`].
    Failed,
    /// nothing to show, and that is the server's honest ANSWER rather than a fault — no spinner,
    /// de-emphasized copy. Distinct from `Failed` on purpose: an empty library is not an error, so
    /// it reads a rung quieter (tertiary ink) than a failure does.
    ///
    /// The TINT is this kind's; the ACTION is the caller's, and the two callers differ for a
    /// reason. The Library's empty section offers none — the server answered, and asking it the
    /// same question again gets the same answer. Home's empty hub offers *Refresh*, because there
    /// the honest reading of "nothing yet" is a server still scanning its first library, and a
    /// second look is what finds it. Neither is a retry of a failure.
    Empty,
}
/// The read-out's own type sizes. A `Working`/`Empty` verdict is reading text; its reason is one
/// rung down because it EXPLAINS rather than states. The action carries a control label, which is
/// `Button`'s rung everywhere else in the product — `STATUS_CAP_SZ`, and a `Failed` verdict's
/// larger rung never resizes a pill.
const STATUS_CAP_SZ: c_int = theme::size::BODY;
const STATUS_REASON_SZ: c_int = theme::size::CAPTION;
/// A `Failed` read-out's verdict and reason rungs (`StatusOverlay.jsx`: bold `--size-title`, then
/// regular `--size-body` in a reserved two-line slot).
const STATUS_FAILED_VERDICT_SZ: c_int = theme::size::TITLE;
const STATUS_FAILED_REASON_SZ: c_int = theme::size::BODY;
pub struct StatusOverlay<'a> {
    pub frame: Rect,
    /// The VERDICT — one line naming what happened.
    ///
    /// Borrowed rather than `&'static`: a section read-out interpolates a machine name ("Can’t
    /// reach &lt;machine&gt;"), which no `c"…"` literal can carry. `&'static CStr` still coerces, so the
    /// state-machine captions (`PlaybackState::caption()`, Home's hub states) are unchanged. The
    /// `Label` rule applies to a runtime one — keep the `CString` alive for the whole draw frame.
    pub caption: &'a core::ffi::CStr,
    /// The line UNDER the verdict, in de-emphasized ink: why it happened, and — the job it exists
    /// for — what is still fine. `None` = absent, not empty.
    pub reason: Option<&'a core::ffi::CStr>,
    /// The read-out's PRIMARY control, drawn below the copy and hit-tested by the screen through
    /// [`StatusOverlay::action_frame`], so the drawn pill and the rect a click lands in are one
    /// expression. `None` = no control: while a fetch is in flight the spinner IS the state, and an
    /// [`StatusKind::Empty`] answer has nothing to retry.
    pub action: Option<&'a core::ffi::CStr>,
    /// The SECONDARY control on the primary's row, after it — the sign-in failure's *Details*. It
    /// exists only beside a primary: a read-out whose one thing to press is secondary has no
    /// primary to be secondary to. `None` = absent, not empty. Frame slot 1.
    pub secondary: Option<&'a core::ffi::CStr>,
    /// Further controls on the same row, after the secondary — frame slots 2 and up. Like the
    /// secondary they exist only beside a primary. The player's failure read-out is the caller
    /// that needs them: its row is derived from `player::failure_actions`, which can offer a fix,
    /// a second recovery, *Details* and *Back* together. `None` = absent.
    pub extra: [Option<&'a core::ffi::CStr>; STATUS_ROW_MAX - 2],
    /// ONE line of fine print UNDER the control row, centred at `CAPTION` in `TEXT_TERTIARY` — the
    /// sign-in report's quiet status ("Sending report…", "Report sent"). It wraps (at most two
    /// lines, never shrinking) inside [`Self::REASON_W`], is absent when `None`, and cannot move
    /// the blocks above it. Anything longer than one quiet line — a Report ID, a support line —
    /// belongs in a card the screen opens, not on the read-out.
    pub note: Option<&'a core::ffi::CStr>,
    /// The note is still ON ITS WAY — a report being sent. It is drawn with an inline [`Spinner`]
    /// in a leading gutter, the pair centred together, turning on [`phase`](Self::phase).
    pub note_busy: bool,
    /// Placed on the PAGE rather than in its frame — see [`StatusOverlay::page`].
    pub page: bool,
    /// The glyph a page-placed `Failed` read-out draws above its verdict — see
    /// [`StatusOverlay::page`], the only way this is ever set. `None` for every read-out that is
    /// not page-placed AND `Failed`, which is exactly the set the design says carries no glyph
    /// (a container read-out, `Working`, `Empty`).
    pub glyph: Option<crate::ui::icons::Icon>,
    /// The lowest y some OTHER chrome already occupies on this page, if any — see
    /// [`StatusOverlay::glyph_ceiling`]. `None` (the default, and what Home and sign-in pass)
    /// means the glyph is free to draw at its natural size; the Library passes its live tab
    /// strip's bottom only while that strip is on screen.
    pub glyph_ceiling: Option<f32>,
    pub kind: StatusKind,
    pub phase: u32,
    /// which pill of the row holds focus — 0 the primary, 1 the `secondary`, 2.. the `extra`
    /// slots; `None` when focus is elsewhere. Ignored for a slot with no pill.
    pub focus: Option<usize>,
    /// the per-pill [FOCUS POP](CTRL_FOCUS_SCALE) the CALLER's `CtlPop` supplies — see the draw
    /// for why a lone action takes none.
    pub scales: [f32; STATUS_ROW_MAX],
}

/// How far a `Failed` read-out's action row sits below the end of its reserved two-line reason
/// slot, on top of [`space::LG`](theme::space::LG): one position for every reason, whatever its
/// line count, so a two-line reason keeps air above the pills and a one-line one just keeps its
/// empty second line. A `theme::space` rung.
const REASON_ROW_DROP: f32 = theme::space::MD;

/// The most controls one read-out row carries: a primary, a secondary and two `extra` slots.
pub const STATUS_ROW_MAX: usize = 4;

/// The stacked read-out's geometry — the ONE place its bands are placed, read by the draw and by
/// [`StatusOverlay::action_frames_measured`] so a screen's hit test cannot drift from the pills it
/// sees.
struct StatusBands {
    cap: Rect,
    reason: Option<Rect>,
    /// top of the action row (whether or not there is one)
    action_y: f32,
}

impl<'a> StatusOverlay<'a> {
    /// The action pill's height. The app's one action-control size — the hero rows' pill/disc
    /// diameter, which `home.rs` aliases rather than restating.
    pub const CTRL_H: f32 = 60.0;
    /// The fine print's rung — one step under the verdict, the reason's own rung.
    const NOTE_SZ: c_int = theme::size::CAPTION;
    /// The measure a reason in the two-line slot — and the note — wraps at: a centred line of
    /// reading text wider than this is a scan across the room rather than a sentence.
    pub const REASON_W: f32 = 960.0;
    /// The FULL-SCREEN top anchor: the player's failure glyph, and the verdict band of a
    /// [page-filling](Self::page) `Failed` read-out. Anchored from the top so the verdict stays
    /// put whatever grows below it.
    pub const FULL_ANCHOR_TOP: f32 = 540.0;
    /// A [page-placed](Self::page) `Failed` read-out's glyph — square, this side.
    pub const GLYPH_SIZE: f32 = 112.0;
    /// The air between the glyph's bottom edge and [`Self::FULL_ANCHOR_TOP`] — the verdict's cap
    /// top, so the glyph box's own top sits at `FULL_ANCHOR_TOP - GLYPH_GAP - GLYPH_SIZE` (384 on
    /// the 1920×1080 screen space every page-filling read-out shares).
    pub const GLYPH_GAP: f32 = 44.0;
    /// The air kept between the (possibly shrunk) glyph box's top edge and
    /// [`Self::glyph_ceiling`], when one is set. Chrome touching the glyph reads as crowded even
    /// where the two rects do not literally overlap, so the box stops short of the ceiling by
    /// this much rather than right at it.
    pub const GLYPH_CEILING_MARGIN: f32 = 12.0;
    /// The smallest square a page glyph is still drawn at. A mark shrunk past this reads as a
    /// blurry thumbnail rather than a considered smaller glyph, so [`Self::glyph_rect`] omits it
    /// entirely below this size instead of drawing one — half the natural [`Self::GLYPH_SIZE`],
    /// rounded to a size nanosvg still rasterizes cleanly.
    pub const GLYPH_MIN_SIZE: f32 = 56.0;

    pub fn new(frame: Rect, caption: &'a core::ffi::CStr, kind: StatusKind) -> Self {
        Self {
            frame,
            caption,
            reason: None,
            action: None,
            secondary: None,
            extra: [None; STATUS_ROW_MAX - 2],
            note: None,
            note_busy: false,
            page: false,
            glyph: None,
            glyph_ceiling: None,
            kind,
            phase: 0,
            focus: None,
            scales: [1.0; STATUS_ROW_MAX],
        }
    }
    /// ms clock driving the spinner's rotation (ignored by `Failed`)
    pub fn phase(mut self, ms: u32) -> Self {
        self.phase = ms;
        self
    }
    /// The line under the verdict — see [`StatusOverlay::reason`].
    pub fn reason(mut self, r: &'a core::ffi::CStr) -> Self {
        self.reason = Some(r);
        self
    }
    /// **This read-out FILLS THE PAGE** — a `Failed` one hangs its verdict from
    /// [`Self::FULL_ANCHOR_TOP`] in screen space, centred on the panel, with the reason and the
    /// control row stacked under it, rather than centring in `frame`. The sign-in failure, Home's
    /// hub failure and a Library section's source failure all say so, which is what puts them on
    /// one verdict line (and one row line whenever their copy is the same shape); a read-out bounded by a panel or a list region (the
    /// onboarding list, Search's results, the person page's shelves) does not, and keeps its
    /// container's centred layout. `Working` and `Empty` are unaffected: a spinner and a quiet
    /// answer stay centred in the region they are about.
    ///
    /// **Takes the glyph, not an optional builder** — a page-placed `Failed` read-out cannot be
    /// built without saying what failed (spec "1A"). Every caller reads it off the SAME typed
    /// cause its copy came from (`auth::incident::IncidentContext::readout_glyph`, or the
    /// fixed `Icon::ServerBadgeMinus` Home and the Library share for their own untyped "can't
    /// reach" verdict), so the glyph and the caption can never disagree. `Working`/`Empty` callers
    /// still pass one (the read-out is built once and shared across kinds in more than one
    /// screen), but it is discarded here: only a `Failed` verdict ever draws it.
    pub fn page(mut self, glyph: crate::ui::icons::Icon) -> Self {
        if self.kind == StatusKind::Failed {
            self.page = true;
            self.frame = Rect::FULL;
            self.glyph = Some(glyph);
        }
        self
    }
    /// **Some OTHER chrome on this page already occupies down to this y — shrink the glyph to
    /// clear it, rather than let the two overlap.** The Library keeps its tab strip live above a
    /// failed section's read-out (a section failing is not the app failing). That strip reaches
    /// y 254 and the glyph's natural box starts at 384, so today nothing collides; the ceiling is
    /// the guard for chrome that grows down into the box (at anchor 372 it was an ~38px overlap). Only the glyph box (and the air above the verdict it sits in)
    /// shrinks; [`Self::FULL_ANCHOR_TOP`] never moves, so the verdict, reason and action row are
    /// unaffected. Below [`Self::GLYPH_MIN_SIZE`] the glyph is dropped rather than drawn as a
    /// thumbnail. Home and sign-in pass no ceiling — they own the whole page above the verdict —
    /// so this is a no-op for both.
    pub fn glyph_ceiling(mut self, y: f32) -> Self {
        self.glyph_ceiling = Some(y);
        self
    }
    /// **The verdict's face — rung, weight and ink — by kind**, the design system's
    /// `StatusOverlay` contract in one place: a `Failed` verdict is bold `size::TITLE` in
    /// `TEXT_SECONDARY` ("the app does not scold": a failure is never tinted a warning colour);
    /// `Working` is `size::BODY` secondary and `Empty` `size::BODY` tertiary, as they always were.
    /// Every `Failed` read-out uses this face, the player's full-screen failure included: it is a
    /// page-placed read-out (`page`, spec "1A") like Home's and sign-in's, with the same 112px
    /// `TEXT_SECONDARY` glyph drawn by this widget.
    pub(crate) fn verdict_face(kind: StatusKind) -> (c_int, bool, [f32; 4]) {
        match kind {
            StatusKind::Working => (STATUS_CAP_SZ, false, theme::TEXT_SECONDARY),
            StatusKind::Failed => (STATUS_FAILED_VERDICT_SZ, true, theme::TEXT_SECONDARY),
            StatusKind::Empty => (STATUS_CAP_SZ, false, theme::TEXT_TERTIARY),
        }
    }
    /// The reason's rung and ink by kind: a `Failed` reason is regular `size::BODY` in
    /// `TEXT_SECONDARY` (the design system's), the others keep the quiet caption line.
    pub(crate) fn reason_face(kind: StatusKind) -> (c_int, [f32; 4]) {
        match kind {
            StatusKind::Failed => (STATUS_FAILED_REASON_SZ, theme::TEXT_SECONDARY),
            StatusKind::Working | StatusKind::Empty => (STATUS_REASON_SZ, theme::TEXT_TERTIARY),
        }
    }
    /// Whether the reason sits in the design system's RESERVED TWO-LINE slot — every `Failed`
    /// read-out's (`StatusOverlay.jsx`: "a one-line and a two-line reason leave everything below
    /// them in the same place"). The slot is two lines tall whatever the reason says, and the
    /// reason wraps inside it at [`Self::REASON_W`].
    fn reason_slotted(&self) -> bool {
        self.kind == StatusKind::Failed
    }
    /// The reason's view in the two-line slot: centred, wrapped, never more than two lines.
    fn reason_view(&self, r: &'a core::ffi::CStr) -> TextView<'a> {
        let (sz, ink) = Self::reason_face(self.kind);
        TextView::new(r.to_str().unwrap_or(""), sz, ink).h(HAlign::Center).max_lines(2)
    }
    /// **A reason with a deliberate line break** (`\n` in the message) as its lines: at most two,
    /// each trimmed and non-empty. `None` for a reason without one, which keeps wrapping freely
    /// inside the slot. A forced break is the message's own presentation (the sign-in failure's
    /// "Signed in as …." over "This Plex account has no server yet."), so each line stands on a
    /// row of its own and is never joined to its neighbour or wrapped onto a third. `TextView`
    /// collapses `\n` like any whitespace, which is why the split happens here, the same way
    /// `ui::qr` presents an address broken at a path separator.
    fn reason_segments(r: &str) -> Option<Vec<&str>> {
        if !r.contains('\n') {
            return None;
        }
        Some(r.split('\n').map(str::trim).filter(|l| !l.is_empty()).take(2).collect())
    }
    /// One line of a forced-break reason: the reason's face, never wrapped, ellipsized if it is
    /// still too wide (`screens::login`'s `signed_in_reason` pre-fit makes that a net, not the plan).
    fn reason_segment_view(&self, seg: &'a str) -> TextView<'a> {
        let (sz, ink) = Self::reason_face(self.kind);
        TextView::new(seg, sz, ink).h(HAlign::Center).max_lines(1)
    }
    /// Whether `reason` would be cut short in a `Failed` read-out's two-line slot [`Self::REASON_W`]
    /// wide, measured through the slot's own view, with `headroom` of the width to spare.
    #[cfg(test)]
    pub(crate) fn failed_reason_truncates(reason: &core::ffi::CStr, measure: &dyn nj_machine::machine::Measure, headroom: f32) -> bool {
        let o = StatusOverlay::new(Rect::FULL, c"", StatusKind::Failed);
        let width = Self::REASON_W * headroom;
        match Self::reason_segments(reason.to_str().unwrap_or("")) {
            Some(lines) => lines.iter().any(|l| o.reason_segment_view(l).with_measure(measure).truncates(width)),
            None => o.reason_view(reason).with_measure(measure).truncates(width),
        }
    }
    /// The two-line slot's height from one measured line: one line pitch plus the last line's box.
    fn reason_slot_h(&self, line_h: f32) -> f32 {
        self.reason_view(c"").line_h() + line_h
    }
    /// **The ONE place a page-placed `Failed` read-out's glyph box is computed** — square,
    /// centred horizontally on `frame`, its bottom edge above [`Self::FULL_ANCHOR_TOP`] by a gap,
    /// with no `ceiling`: [`Self::GLYPH_SIZE`] and [`Self::GLYPH_GAP`] exactly (384 on the shared
    /// 1920×1080 screen space `frame` is `Rect::FULL` for every page-placed read-out).
    ///
    /// With a `ceiling` (`glyph_ceiling`'s y), the size and the gap shrink TOGETHER by whatever
    /// factor makes the box's top edge land [`Self::GLYPH_CEILING_MARGIN`] below it, so the
    /// verdict never moves and the glyph keeps its proportions rather than the gap alone
    /// collapsing. Below [`Self::GLYPH_MIN_SIZE`] this returns `None` rather than a box — the
    /// caller draws nothing for it. A test asks this directly rather than re-deriving it, the
    /// same reason [`StatusBands`] exists for the blocks below it.
    fn glyph_rect(frame: Rect, ceiling: Option<f32>) -> Option<Rect> {
        let natural_span = Self::GLYPH_GAP + Self::GLYPH_SIZE;
        let (gap, size) = match ceiling {
            None => (Self::GLYPH_GAP, Self::GLYPH_SIZE),
            Some(ceiling) => {
                let available = Self::FULL_ANCHOR_TOP - (ceiling + Self::GLYPH_CEILING_MARGIN);
                if available >= natural_span {
                    (Self::GLYPH_GAP, Self::GLYPH_SIZE)
                } else {
                    let factor = (available / natural_span).max(0.0);
                    // `size` is quantized to a whole pixel — `icons.rs::icon_raster_px` rasterizes
                    // at an integer size, so a fractional box here would draw the exact right
                    // float rect over a texture rasterized at a ROUNDED size, forcing a rescale
                    // and going soft exactly where this shrink exists to stay crisp (`draw`'s own
                    // `r.w.max(r.h).round()` already floors/rounds `px` before `tex_for`, but the
                    // draw RECT itself stayed fractional). `floor`, not `round`: shrinking `size`
                    // alone, without also shrinking `gap`, only ever gives the ceiling MORE
                    // clearance than the un-quantized box already had, never less — `gap` stays
                    // at its exact proportional value.
                    (Self::GLYPH_GAP * factor, (Self::GLYPH_SIZE * factor).floor())
                }
            }
        };
        if size < Self::GLYPH_MIN_SIZE {
            return None;
        }
        Some(Rect::new(
            frame.cx() - size * 0.5,
            Self::FULL_ANCHOR_TOP - gap - size,
            size,
            size,
        ))
    }
    /// The glyph box a page-placed `Failed` read-out draws, or `None` — for a screen's own layout
    /// test (the Library's chrome-collision check) without duplicating the geometry.
    #[cfg(test)]
    pub(crate) fn glyph_frame(&self) -> Option<Rect> {
        self.glyph.filter(|_| self.page_placed()).and_then(|_| Self::glyph_rect(self.frame, self.glyph_ceiling))
    }
    /// Whether this read-out hangs from [`Self::FULL_ANCHOR_TOP`] — a `Failed` one its caller
    /// declared page-filling with [`Self::page`].
    fn page_placed(&self) -> bool {
        self.page && self.kind == StatusKind::Failed
    }
    /// The note's view: centred `CAPTION` tertiary, wrapping to two lines rather than shrinking.
    fn note_view(&self, line: &'a core::ffi::CStr) -> TextView<'a> {
        TextView::new(line.to_str().unwrap_or(""), Self::NOTE_SZ, theme::TEXT_TERTIARY)
            .h(HAlign::Center)
            .max_lines(2)
    }
    /// The read-out's primary control — see [`StatusOverlay::action`].
    pub fn action(mut self, label: &'a core::ffi::CStr) -> Self {
        self.action = Some(label);
        self
    }
    /// The row's secondary control — see [`StatusOverlay::secondary`].
    pub fn secondary(mut self, label: Option<&'a core::ffi::CStr>) -> Self {
        self.secondary = label;
        self
    }
    /// **The whole row at once**, in order: the first label is the primary, the second the
    /// secondary, the rest the [`extra`](Self::extra) slots. Labels past [`STATUS_ROW_MAX`] are a
    /// caller bug and are dropped (debug builds assert).
    pub fn row(mut self, labels: &[&'a core::ffi::CStr]) -> Self {
        debug_assert!(labels.len() <= STATUS_ROW_MAX, "a read-out row holds at most {STATUS_ROW_MAX} controls");
        let mut it = labels.iter().copied();
        self.action = it.next();
        self.secondary = it.next();
        for slot in self.extra.iter_mut() {
            *slot = it.next();
        }
        self
    }
    /// The quiet line under the row — see [`StatusOverlay::note`].
    pub fn note(mut self, line: Option<&'a core::ffi::CStr>) -> Self {
        self.note = line;
        self
    }
    /// Mark the note as on its way — see [`StatusOverlay::note_busy`].
    pub fn note_busy(mut self, busy: bool) -> Self {
        self.note_busy = busy;
        self
    }
    /// Focus on the PRIMARY, or on nothing — the lone-action read-outs' whole vocabulary.
    pub fn focused(mut self, f: bool) -> Self {
        self.focus = f.then_some(0);
        self
    }
    /// Focus on control `i` (0 the primary, 1 the secondary), or on nothing.
    pub fn focus(mut self, i: Option<usize>) -> Self {
        self.focus = i;
        self
    }
    /// The first two controls' focus pop, normally `[pop.scale(0), pop.scale(1)]`; any `extra`
    /// control keeps 1.0.
    pub fn scales(mut self, s: [f32; 2]) -> Self {
        self.scales[..2].copy_from_slice(&s);
        self
    }
    /// How far the read-out's ink reaches ABOVE the frame centre: the spinner ring plus its dots
    /// plus the `space::XS` that separates it from the caption. The caption half is deliberately NOT
    /// here — it needs `text::text_height`, which the host suite cannot link — and it is the SMALLER
    /// half, so this doubles as the conservative bound a layout test can assert against.
    pub fn above() -> f32 {
        Spinner::R_PAGE + theme::space::XS + Spinner::R_PAGE + Spinner::dot_r(Spinner::R_PAGE)
    }

    /// Where the blocks sit. Everything hangs off ONE anchor — the caption band, which keeps
    /// the exact position it has always had (`Working` straddles the frame centre with the spinner
    /// above it; a terminal state owns the centre alone) — and the optional blocks stack BELOW it on
    /// `theme::space` rungs. That ordering is deliberate: adding a reason or an action cannot move
    /// the read-outs that carry neither, so the player's and the person page's are untouched. A
    /// [page-placed](Self::page) `Failed` read-out is the exception: its verdict hangs from
    /// [`Self::FULL_ANCHOR_TOP`] in screen space, and the rest stacks under it as usual.
    fn bands(&self) -> StatusBands {
        self.bands_measured(&LegacyMeasure)
    }

    fn bands_measured(&self, measure: &dyn nj_machine::machine::Measure) -> StatusBands {
        let (cap_sz, _, _) = Self::verdict_face(self.kind);
        let (reason_sz, _) = Self::reason_face(self.kind);
        self.bands_from_heights(measure.line_h(cap_sz), self.reason_h(measure.line_h(reason_sz)))
    }

    /// How far the action row sits below the reserved slot's end plus `space::LG`
    /// ([`REASON_ROW_DROP`]). Only a slotted reason moves it: a read-out without one, and the
    /// `Working`/`Empty` kinds, stack exactly as they always did.
    fn row_drop(&self) -> f32 {
        if self.reason_slotted() && self.reason.is_some() { REASON_ROW_DROP } else { 0.0 }
    }

    /// The reason band's height from one reason line: absent, one line, or the reserved slot.
    fn reason_h(&self, line_h: f32) -> f32 {
        match (self.reason, self.reason_slotted()) {
            (None, _) => 0.0,
            (Some(_), false) => line_h,
            (Some(_), true) => self.reason_slot_h(line_h),
        }
    }

    fn bands_from_heights(&self, cap_h: f32, reason_h: f32) -> StatusBands {
        let cy = self.frame.cy();
        let cap_y = if self.kind == StatusKind::Working {
            cy + theme::space::XS
        } else if self.page_placed() {
            Self::FULL_ANCHOR_TOP
        } else {
            cy - cap_h * 0.5
        };
        let cap = Rect::new(self.frame.x, cap_y, self.frame.w, cap_h);
        let mut below = cap_y + cap_h;
        let reason = self.reason.map(|_| {
            let h = reason_h;
            let r = Rect::new(self.frame.x, below + theme::space::SM, self.frame.w, h);
            below = r.y + h;
            r
        });
        let action_y = below + theme::space::LG + self.row_drop();
        StatusBands { cap, reason, action_y }
    }

    /// The ROW's labels in draw order — the primary, then the secondary, then the `extra` slots —
    /// each with the slot index focus and the scales address it by. Empty without a primary.
    fn row_labels(&self) -> impl Iterator<Item = (usize, &'a core::ffi::CStr)> + '_ {
        let rest = core::iter::once(self.secondary)
            .chain(self.extra.iter().copied())
            .enumerate()
            .filter_map(|(i, l)| l.map(|l| (i + 1, l)))
            .filter(move |_| self.action.is_some());
        self.action.into_iter().map(|l| (0, l)).chain(rest)
    }

    /// Lay the row out from its pill widths: one centred run, `CONTROL_GAP` apart — the
    /// control-group distance a pair of answers uses everywhere else. A lone primary is exactly
    /// the centred pill it always was.
    fn row_rects(&self, widths: [Option<f32>; STATUS_ROW_MAX], bands: &StatusBands) -> [Option<Rect>; STATUS_ROW_MAX] {
        let present = widths.iter().flatten().count();
        let total = widths.iter().flatten().sum::<f32>()
            + CONTROL_GAP * present.saturating_sub(1) as f32;
        let mut x = self.frame.cx() - total * 0.5;
        let mut out = [None; STATUS_ROW_MAX];
        for (i, w) in widths.iter().enumerate() {
            if let Some(w) = w {
                out[i] = Some(Rect::new(x, bands.action_y, *w, Self::CTRL_H));
                x += w + CONTROL_GAP;
            }
        }
        out
    }

    /// The note's band: `space::MD` under the row when there is one, in the row's place when not.
    /// A note on its way is one line (its spinner sits on it); a settled one is its wrapped view's
    /// height inside [`Self::REASON_W`].
    fn note_band(&self, bands: &StatusBands, measure: &dyn nj_machine::machine::Measure) -> Option<Rect> {
        let line = self.note?;
        let top = if self.action.is_some() {
            bands.action_y + Self::CTRL_H + theme::space::MD
        } else {
            bands.action_y
        };
        let h = if self.note_busy {
            self.note_view(c"").line_h()
        } else {
            self.note_view(line).with_measure(measure).measure_h(Self::REASON_W.min(self.frame.w))
        };
        Some(Rect::new(self.frame.x, top, self.frame.w, h))
    }

    /// The primary pill's frame, or `None` when there is no action. The screen records this for its
    /// pointer hit test; the draw builds its `Button` from the same call.
    pub fn action_frame(&self) -> Option<Rect> {
        self.frames_live(&self.bands())[0]
    }

    /// The row through the live font — the legacy `View` path's twin of `action_frames_measured`.
    fn frames_live(&self, bands: &StatusBands) -> [Option<Rect>; STATUS_ROW_MAX] {
        let mut widths = [None; STATUS_ROW_MAX];
        for (i, l) in self.row_labels() {
            widths[i] = Some(Button::pill_w(l.as_ptr(), STATUS_CAP_SZ, false));
        }
        self.row_rects(widths, bands)
    }

    /// Owned-screen placement uses the same metrics capability as its draw, including replay.
    pub(crate) fn action_frame_measured(&self, measure: &dyn nj_machine::machine::Measure) -> Option<Rect> {
        self.action_frames_measured(measure)[0]
    }

    /// Every control, by slot (0 the primary, 1 the secondary), through the geometry the draw uses.
    pub(crate) fn action_frames_measured(&self, measure: &dyn nj_machine::machine::Measure) -> [Option<Rect>; 2] {
        let row = self.row_frames_measured(measure);
        [row[0], row[1]]
    }

    /// Every control of the row, by slot (0 the primary, 1 the secondary, 2.. the `extra`
    /// slots), through the geometry the draw uses.
    pub(crate) fn row_frames_measured(&self, measure: &dyn nj_machine::machine::Measure) -> [Option<Rect>; STATUS_ROW_MAX] {
        if self.action.is_none() {
            return [None; STATUS_ROW_MAX];
        }
        let mut widths = [None; STATUS_ROW_MAX];
        for (i, l) in self.row_labels() {
            widths[i] = Some(Button::pill_w_measured(l, STATUS_CAP_SZ, false, false, measure));
        }
        self.row_rects(widths, &self.bands_measured(measure))
    }

    /// The verdict band through the draw's own geometry — for a screen's test that two read-outs
    /// stand on one line.
    #[cfg(test)]
    pub(crate) fn verdict_band_measured(&self, measure: &dyn nj_machine::machine::Measure) -> Rect {
        self.bands_measured(measure).cap
    }

    #[cfg(test)]
    fn action_rect(&self, width: f32, bands: &StatusBands) -> Rect {
        self.row_rects([Some(width), None, None, None], bands)[0].expect("one pill")
    }

    /// Render through the very geometry used by `action_frames_measured`, without live font
    /// measurements deciding the hit target behind the host's measurement capability.
    pub(crate) fn draw_measured(&self, e: &Env, p: Painter, measure: &dyn nj_machine::machine::Measure) {
        let bands = self.bands_measured(measure);
        let frames = self.row_frames_measured(measure);
        self.draw_geometry(e, p, bands, frames, measure);
    }

    /// A busy note's band split into the spinner's gutter and the text: the pair — gutter, then
    /// `text_w` of text — centred on `band` as one group, so the line reads as centred as a
    /// settled one. Returns the gutter's left edge and the text's rect.
    fn busy_note_split(band: Rect, text_w: f32) -> (f32, Rect) {
        let gutter = Spinner::inline_gutter();
        let left = band.cx() - (gutter + text_w) / 2.0;
        (left, Rect::new(left + gutter, band.y, text_w, band.h))
    }

    fn draw_geometry(
        &self,
        e: &Env,
        p: Painter,
        b: StatusBands,
        frames: [Option<Rect>; STATUS_ROW_MAX],
        measure: &dyn nj_machine::machine::Measure,
    ) {
        // spinner above, caption below, the pair centred on the frame
        let cy = self.frame.cy();
        let (cap_sz, cap_bold, tint) = Self::verdict_face(self.kind);
        let working = self.kind == StatusKind::Working;
        // Both branches centre the caption the same way — by Label's cap band (VAlign::Middle,
        // the default). Working straddles the frame centre with the spinner above it; Failed owns
        // the centre alone. Using the cap band for one and a line-box metric for the other put the
        // two states on different baselines in the same frame.
        if working {
            Spinner::new(
                self.frame.cx(),
                cy - Spinner::R_PAGE - theme::space::XS,
                Spinner::R_PAGE,
            )
            .phase(self.phase)
            .tint(tint)
            .draw(e, p);
        }
        if let Some(icon) = self.glyph.filter(|_| self.page_placed()) {
            if let Some(rect) = Self::glyph_rect(self.frame, self.glyph_ceiling) {
                crate::ui::icons::draw(p, icon, rect, theme::TEXT_SECONDARY);
            }
        }
        let verdict = Label::new(self.caption.as_ptr(), cap_sz, tint).h(HAlign::Center);
        if cap_bold { verdict.bold() } else { verdict }.draw(p, b.cap);
        // The reason is in its own face (`reason_face`), never a severity colour: its job is to say
        // why, and what is still working.
        if let (Some(r), Some(band)) = (self.reason, b.reason) {
            if self.reason_slotted() {
                // Top-aligned in the slot: a one-line reason leaves the second line empty, and
                // nothing below the slot moves either way.
                let w = Self::REASON_W.min(band.w);
                let x = band.cx() - w * 0.5;
                match Self::reason_segments(r.to_str().unwrap_or("")) {
                    Some(lines) => {
                        let pitch = self.reason_view(c"").line_h();
                        for (i, line) in lines.into_iter().enumerate() {
                            self.reason_segment_view(line)
                                .with_measure(measure)
                                .draw(p, Rect::new(x, band.y + i as f32 * pitch, w, pitch));
                        }
                    }
                    None => {
                        self.reason_view(r).with_measure(measure).draw(p, Rect::new(x, band.y, w, band.h));
                    }
                }
            } else {
                let (sz, ink) = Self::reason_face(self.kind);
                Label::new(r.as_ptr(), sz, ink).h(HAlign::Center).draw(p, band);
            }
        }
        // **A lone action takes no [`CTRL_FOCUS_SCALE`] pop, deliberately**, and a row does. A
        // read-out's single action is the ONLY focusable thing on the region it owns —
        // `library::sync_readout_focus` lands the ring on it the moment the read-out appears and
        // there is nowhere else for it to go — so a pop would have no sibling to distinguish the
        // control from and would resolve to a constant 1.07, a slightly larger button with no
        // signal in it. The pop is a ROW's affordance: once the read-out offers two actions (the
        // sign-in failure's *Try again* / *Details*), the caller's `CtlPop` scales reach the pills.
        let row = self.row_labels().count();
        for (i, label) in self.row_labels() {
            if let Some(f) = frames[i] {
                let scale = if row > 1 { self.scales[i] } else { 1.0 };
                Button::new(label.as_ptr(), STATUS_CAP_SZ, f)
                    .focused(self.focus == Some(i))
                    .scale(scale)
                    .draw(e, p);
            }
        }
        if let (Some(line), Some(band)) = (self.note, self.note_band(&b, measure)) {
            let ink = theme::TEXT_TERTIARY;
            if self.note_busy {
                // Cap-top on the band's top edge, exactly where the settled note's `TextView` puts
                // its first line, so a report that lands does not hop; the ring sits on the cap band.
                let (gutter_x, text) = Self::busy_note_split(band, measure.width(line, Self::NOTE_SZ, false));
                Spinner::leading(gutter_x, band.y + measure.cap_h(Self::NOTE_SZ) * 0.5)
                    .phase(self.phase)
                    .tint(ink)
                    .draw(e, p);
                Label::new(line.as_ptr(), Self::NOTE_SZ, ink)
                    .h(HAlign::Left)
                    .v(VAlign::CapTop)
                    .draw(p, text);
            } else {
                let w = Self::REASON_W.min(band.w);
                self.note_view(line)
                    .with_measure(measure)
                    .draw(p, Rect::new(band.cx() - w * 0.5, band.y, w, band.h));
            }
        }
    }
}

impl View for StatusOverlay<'_> {
    fn draw(&self, e: &Env, p: Painter) {
        let bands = self.bands();
        let frames = self.frames_live(&bands);
        self.draw_geometry(e, p, bands, frames, &LegacyMeasure);
    }
}

// ---- TransportButton: the player OSD's icon-only circle (`docs/litefin-look.md` 10, Buttons) ---
// 58px circles (the play/pause one 86.4) holding a 33.6px icon (48 in the play circle). Idle is the
// white outline icon alone on the video plane; focused fills with the accent, swaps to the filled
// cut in lavender and pops 1.08, the icon a further 1.12. A toggle that is on wears its icon in the
// accent; a control that cannot act sits at .30.
pub struct TransportButton {
    pub frame: Rect,
    /// The player OSD's index for this control (`appkit::player_hud::BTN_*`): picks the glyph pair.
    pub which: i32,
    pub focused: bool,
    pub enabled: bool,
    /// a toggle that is on (Subtitles while a subtitle track plays)
    pub active: bool,
    /// WHICH GROUND this circle stands on — see [`ControlGround`]. It defaults to
    /// [`Keyed`](ControlGround::Keyed) like every other control face, the default belonging to the
    /// WIDGET: on a page it rests on Litefin's action fill (the details page's round buttons), on
    /// the video plane ([`Unkeyed`](ControlGround::Unkeyed), the OSD) it is the icon alone.
    pub ground: ControlGround,
    /// The FOCUS POP, as a factor on the frame — see [`CircleButton::scale`].
    pub scale: f32,
    /// The icon's own pop on top of [`Self::scale`] (Litefin's further 1.12).
    pub icon_scale: f32,
    /// The icon box at rest, px.
    pub icon_px: f32,
    /// (rest, focused) glyphs.
    pub glyph: (crate::ui::icons::Icon, crate::ui::icons::Icon),
}
impl TransportButton {
    pub fn new(which: i32, frame: Rect) -> Self {
        use crate::ui::icons::Icon;
        // the OSD's controls, by `player_hud::BTN_*`: outline at rest, the filled cut on focus
        let glyph = match which {
            1 => (Icon::Speaker, Icon::SpeakerFill),
            2 => (Icon::Settings, Icon::SettingsFill),
            3 => (Icon::SkipBack, Icon::SkipBackFill),
            4 => (Icon::SkipForward, Icon::SkipForwardFill),
            5 => (Icon::RewindStart, Icon::RewindStartFill),
            6 => (Icon::Replay10, Icon::Replay10),
            7 => (Icon::PlayLine, Icon::PlayFill),
            8 => (Icon::Forward10, Icon::Forward10),
            9 => (Icon::SpeedEnd, Icon::SpeedEndFill),
            10 => (Icon::BookOpen, Icon::BookOpenFill),
            _ => (Icon::ClosedCaptioning, Icon::ClosedCaptioningFill),
        };
        Self {
            frame,
            which,
            focused: false,
            enabled: true,
            active: false,
            ground: ControlGround::Keyed,
            scale: 1.0,
            icon_scale: 1.0,
            icon_px: theme::osd::BUTTON_ICON,
            glyph,
        }
    }
    pub fn enabled(mut self, enabled: bool) -> Self { self.enabled = enabled; self }
    pub fn focused(mut self, f: bool) -> Self {
        self.focused = f;
        self
    }
    pub fn active(mut self, a: bool) -> Self {
        self.active = a;
        self
    }
    /// Stand this circle on a named [`ControlGround`] — see [`Button::ground`].
    pub fn ground(mut self, g: ControlGround) -> Self {
        self.ground = g;
        self
    }
    pub fn scale(mut self, s: f32) -> Self {
        self.scale = s;
        self
    }
    pub fn icon_scale(mut self, s: f32) -> Self {
        self.icon_scale = s;
        self
    }
    pub fn icon_px(mut self, px: f32) -> Self {
        self.icon_px = px;
        self
    }
    /// Override the glyph pair — the play circle shows pause while playing.
    pub fn glyph(mut self, rest: crate::ui::icons::Icon, focused: crate::ui::icons::Icon) -> Self {
        self.glyph = (rest, focused);
        self
    }
}
impl View for TransportButton {
    fn draw(&self, _e: &Env, p: Painter) {
        use theme::osd;
        let r = self.frame.scaled(self.scale);
        let p = if self.enabled { p } else { p.alpha(osd::BUTTON_DISABLED_A) };
        let fill = if self.focused {
            Some(osd::BUTTON_FOCUS_FILL)
        } else {
            (self.ground == ControlGround::Keyed).then_some(theme::ACTION_FILL)
        };
        if let Some(fill) = fill {
            p.rrect(r, r.w * 0.5, r.w * 0.5, fill);
        }
        let (id, ink) = if self.focused {
            (self.glyph.1, osd::BUTTON_FOCUS_INK)
        } else if self.active {
            (self.glyph.0, osd::BUTTON_ACTIVE_INK)
        } else {
            (self.glyph.0, osd::BUTTON_IDLE_INK)
        };
        let s = (self.icon_px * self.scale * self.icon_scale).round();
        crate::ui::icons::draw(p, id, Rect::new(r.cx() - s * 0.5, r.cy() - s * 0.5, s, s), ink);
    }
}

// ---- OsdPill: Litefin's text button over the video (the Up Next card's pair, Skip Intro) ----
// `styles/upnext.css` `.upnext-btn`: 11.2 x 24 padding, radius 12, 18.4 bold, a white .10 fill with
// a 2.5px white .20 border and .80 ink; focus draws the border white and pops 1.05. A PRIMARY pill
// (Play now, Skip Intro) fills with the accent and inks lavender on focus; the other lifts to .22.
pub struct OsdPill {
    pub label: *const c_char,
    pub frame: Rect,
    pub focused: bool,
    pub primary: bool,
    pub scale: f32,
}
impl OsdPill {
    /// The pill's type: 18.4 on the nearest Litefin rung.
    pub const SZ: c_int = theme::size::lf::S18;
    const PAD_X: f32 = 24.0;
    const PAD_Y: f32 = 11.2;
    /// Litefin's focused size for these buttons.
    pub const FOCUS_SCALE: f32 = 1.05;
    pub fn new(label: *const c_char, frame: Rect) -> Self {
        Self { label, frame, focused: false, primary: false, scale: 1.0 }
    }
    pub fn focused(mut self, f: bool) -> Self {
        self.focused = f;
        self
    }
    pub fn primary(mut self, v: bool) -> Self {
        self.primary = v;
        self
    }
    pub fn scale(mut self, s: f32) -> Self {
        self.scale = s;
        self
    }
    /// The pill's size for `label`: the label plus padding and border.
    pub fn size(label: &str, measure: &dyn nj_machine::machine::Measure) -> (f32, f32) {
        let border = 2.0 * theme::osd::PILL_BORDER_W;
        (measure.width_str(label, Self::SZ, true) + 2.0 * Self::PAD_X + border,
            measure.line_h(Self::SZ) + 2.0 * Self::PAD_Y + border)
    }
}
impl View for OsdPill {
    fn draw(&self, _e: &Env, p: Painter) {
        use theme::osd;
        let r = self.frame.scaled(self.scale);
        let rad = theme::radius::BASE;
        let (fill, ink, border) = match (self.focused, self.primary) {
            (true, true) => (osd::BUTTON_FOCUS_FILL, osd::BUTTON_FOCUS_INK, osd::BUTTON_FOCUS_FILL),
            (true, false) => (osd::PILL_FOCUS_FILL, theme::TEXT_PRIMARY, osd::PILL_FOCUS_BORDER),
            _ => (osd::PILL_FILL, osd::PILL_INK, osd::PILL_BORDER),
        };
        p.rrect(r, rad, rad, fill);
        p.rring(r, rad, osd::PILL_BORDER_W, border);
        Label::new(self.label, Self::SZ, ink).bold().h(HAlign::Center).draw(p, r);
    }
}

// ---- figures: a read-out whose digits hold still --------------------------------------------
// Poppins (like Litefin, which ships it without `tnum`) has proportional figures, so a ticking time
// drawn as one run changes width every second. These draw each ASCII digit centred in a cell as
// wide as the widest digit and every other character at its own advance, so a read-out's width
// depends only on its SHAPE ("0:00:00") and its edges stay put while the digits tick.

const FIGURES: [&CStr; 10] = [c"0", c"1", c"2", c"3", c"4", c"5", c"6", c"7", c"8", c"9"];

/// The widest digit's advance at `sz`/`bold`: one figure cell.
fn figure_cell(sz: c_int, bold: bool, measure: &dyn nj_machine::machine::Measure) -> f32 {
    FIGURES.iter().map(|d| measure.width(d, sz, bold)).fold(0.0, f32::max)
}

/// `text`'s width drawn by [`draw_figures`].
pub fn figures_width(text: &str, sz: c_int, bold: bool, measure: &dyn nj_machine::machine::Measure) -> f32 {
    let cell = figure_cell(sz, bold, measure);
    let mut w = 0.0;
    for run in figure_runs(text) {
        w += match run {
            Ok(_) => cell,
            Err(s) => measure.width_str(s, sz, bold),
        };
    }
    w
}

/// `text` split into single digits (`Ok`) and the runs between them (`Err`).
fn figure_runs(text: &str) -> impl Iterator<Item = Result<usize, &str>> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let first = rest.chars().next()?;
        if let Some(d) = first.to_digit(10) {
            rest = &rest[1..];
            return Some(Ok(d as usize));
        }
        let end = rest.find(|c: char| c.is_ascii_digit()).unwrap_or(rest.len());
        let (run, tail) = rest.split_at(end);
        rest = tail;
        Some(Err(run))
    })
}

/// Draw `text` with figure cells, its cap band centred on `cy`; `align` 0 starts it at `x`, 1
/// centres it there, 2 ends it there (`Painter::text`'s convention). Returns the drawn width.
#[allow(clippy::too_many_arguments)]
pub fn draw_figures(p: Painter, text: &str, x: f32, cy: f32, sz: c_int, ink: [f32; 4], align: c_int, bold: bool,
    measure: &dyn nj_machine::machine::Measure) -> f32 {
    let w = figures_width(text, sz, bold, measure);
    let cell = figure_cell(sz, bold, measure);
    let b = c_int::from(bold);
    let y = nj_gfx::text::text_vcenter_y(sz, b, cy);
    let mut at = match align { 1 => x - w * 0.5, 2 => x - w, _ => x };
    for run in figure_runs(text) {
        match run {
            Ok(d) => {
                p.text(FIGURES[d].as_ptr(), at + cell * 0.5, y, sz, ink, 1, b);
                at += cell;
            }
            Err(s) => {
                if let Ok(cs) = CString::new(s) {
                    p.text(cs.as_ptr(), at, y, sz, ink, 0, b);
                }
                at += measure.width_str(s, sz, bold);
            }
        }
    }
    w
}

// ---- FieldList: a NON-INTERACTIVE key/value read-out ---------------------------------------
//
// The diagnostics overlay's list primitive (`app/diagnostics.rs`), and the reason it is not a
// `TableView`: that is a SELECTION widget. It paints an accent pill under row `sel` on every draw
// with no "nothing selected" mode, its rows are 60px so ~25 of them measure 1540 against a 1080
// panel and SCROLL behind a scissor, and a row is `label` + optional sub-line + badges — there is
// no right-hand value column at all. A read-out needs the opposite of all three: no selection, no
// scrolling (a panel the user must scroll is two photographs and a chance of missing the line that
// mattered), and a fixed value column. Nothing here was close, which is the condition ui/CLAUDE.md
// sets for a new component.
//
// It owns no state, no focus and no springs: hand it a slice and a frame and it draws.

/// A read-out value's severity. Carried by a WORD in the value text as well as by this tint —
/// a phone photograph of a television chroma-subsamples, so hue alone must never be the signal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Normal,
    /// something is wrong here, and this is the row to read first
    Fault,
}

/// One line of a [`FieldList`]. `val: None` makes it a SECTION heading rather than a pair.
///
/// Diagnostics are the one FieldList consumer that deliberately opts out of elision: its values
/// are support evidence, so they wrap to as many measured lines as they need. The diagnostic panel
/// owns the resulting height; every other FieldList caller continues to pass fixed frames.
pub struct Field {
    pub key: &'static str,
    pub val: Option<String>,
    pub tone: Tone,
    // Laid out ONCE when the 2 Hz diagnostics snapshot is built.  Pixel wrapping in `draw` would
    // turn ~20 values into hundreds of uncached TTF metric walks every second of playback.
    lines: Vec<String>,
}

impl Field {
    pub fn new(key: &'static str, val: impl Into<String>) -> Self {
        let val = val.into();
        let lines = value_lines(&val, FIELD_COL_W);
        Self {
            key,
            val: Some(val),
            tone: Tone::Normal,
            lines,
        }
    }
    /// mark the row as the fault — see [`Tone`]
    pub fn fault(mut self, bad: bool) -> Self {
        if bad {
            self.tone = Tone::Fault;
        }
        self
    }
    /// a group heading, drawn as a quiet caption above its rows
    pub fn section(key: &'static str) -> Self {
        Self {
            key,
            val: None,
            tone: Tone::Normal,
            lines: Vec::new(),
        }
    }
}

// -- Litefin's sign-in pieces (`docs/litefin-look.md` 8, modern layout) ---------------------------
// Shared by `screens::jf_login` and `screens::jf_users`: the brand pane, the 72px buttons and
// inputs, and the who's-watching tile.

/// The brand pane's width (8: 30vw of 1920) and its padding, held inside the safe area.
pub(crate) const LOGIN_BRAND_W: f32 = 576.0;
const LOGIN_BRAND_PAD: f32 = 80.0;
/// The app's mark (70x70, 20 above the name) and the tagline's measure (8).
const LOGIN_LOGO: f32 = 70.0;
const LOGIN_LOGO_GAP: f32 = 20.0;
const LOGIN_TAGLINE_W: f32 = 500.0;
/// The brand word: 80/700 in Litefin; sizes from 80 up do not exist here, so the HERO rung.
const LOGIN_BRAND_SZ: c_int = theme::size::lf::S72;
const LOGIN_TAGLINE_SZ: c_int = theme::size::lf::S22;
/// The right pane's side padding (8: padding 40 80).
const LOGIN_PANE_PAD_X: f32 = 80.0;
/// A button's height and its label (8: 72 tall, 25.6/600 -> `S26` bold).
pub(crate) const LOGIN_CTL_H: f32 = 72.0;
pub(crate) const LOGIN_CTL_SZ: c_int = theme::size::lf::S26;
/// An input: 21 above and below a 25.6px line, its 28px mark at 24 and its text at 69 (8).
pub(crate) const LOGIN_INPUT_H: f32 = 21.0 + 36.0 + 21.0;
const LOGIN_INPUT_MARK: f32 = 28.0;
const LOGIN_INPUT_MARK_X: f32 = 24.0;
pub(crate) const LOGIN_INPUT_TEXT_X: f32 = 69.0;
pub(crate) const LOGIN_INPUT_PAD_R: f32 = 21.0;
/// The focus pops: buttons 1.02, inputs 1.01, avatars 1.08 (8).
pub(crate) const LOGIN_BUTTON_POP: f32 = 1.02;
pub(crate) const LOGIN_INPUT_POP: f32 = 1.01;
pub(crate) const LOGIN_AVATAR_POP: f32 = 1.08;
/// A who's-watching tile: 215 wide with 20 either side, a 208 circle on [`theme::settings_lf::LOGIN_FILL`]
/// holding the 200 avatar, and the name 24 below it at 25.6/600 (8 User picker).
pub(crate) const LOGIN_PERSON_W: f32 = 215.0;
pub(crate) const LOGIN_PERSON_GAP: f32 = 40.0;
pub(crate) const LOGIN_AVATAR: f32 = 208.0;
const LOGIN_AVATAR_IMG: f32 = 200.0;
pub(crate) const LOGIN_NAME_GAP: f32 = 24.0;
pub(crate) const LOGIN_NAME_H: f32 = 36.0;
const LOGIN_SUB_H: f32 = 30.0;
/// A tile's full height: the circle, the name, and the one quiet line under it.
pub(crate) const LOGIN_PERSON_H: f32 = LOGIN_AVATAR + LOGIN_NAME_GAP + LOGIN_NAME_H + LOGIN_SUB_H;

/// The sign-in right pane: right of the brand pane, padded, inside the safe area.
pub(crate) fn login_pane() -> Rect {
    let safe = crate::ui::consts::SAFE;
    let x = LOGIN_BRAND_W + LOGIN_PANE_PAD_X;
    let right = (crate::ui::consts::SCR_W - LOGIN_PANE_PAD_X).min(safe.x + safe.w);
    Rect::new(x, safe.y, right - x, safe.h)
}

/// **Litefin's brand pane** (8 Left brand pane): flat black with a 1px divider down its right edge,
/// and at its foot the app's mark, its name and the tagline, the block bottom-aligned on the pane's
/// padding.
pub(crate) fn draw_login_brand(p: Painter, measure: &dyn nj_machine::machine::Measure) {
    let safe = crate::ui::consts::SAFE;
    let pane = Rect::new(0.0, 0.0, LOGIN_BRAND_W, crate::ui::consts::SCR_H);
    p.rect(pane, 0.0, theme::SURFACE_APP, theme::SURFACE_APP, 0.0);
    p.rect(Rect::new(pane.w - 1.0, 0.0, 1.0, pane.h), 0.0, theme::HAIRLINE, theme::HAIRLINE, 0.0);
    let x = LOGIN_BRAND_PAD.max(safe.x);
    // the left padding grows to the safe edge; the right keeps half of Litefin's 80 so the name
    // stays one line in Poppins' bold cut, which runs wider than Litefin's synthetic bold
    let w = pane.w - LOGIN_BRAND_PAD * 0.5 - x;
    let brand = TextView::new(nj_platform::i18n::msg::settings_login_brand(), LOGIN_BRAND_SZ, theme::TEXT_PRIMARY)
        .bold()
        .leading(LOGIN_BRAND_SZ as f32 * 1.05)
        .with_measure(measure);
    let tagline = TextView::new(nj_platform::i18n::msg::settings_login_tagline(), LOGIN_TAGLINE_SZ, theme::settings_lf::TAGLINE)
        .with_measure(measure);
    let (bw, tw) = (w, w.min(LOGIN_TAGLINE_W));
    let (bh, th) = (brand.measure_h(bw), tagline.measure_h(tw));
    let bottom = (pane.h - LOGIN_BRAND_PAD).min(safe.y + safe.h);
    let mut y = bottom - (LOGIN_LOGO + LOGIN_LOGO_GAP + bh + theme::space::SM + th);
    crate::ui::icons::draw(p, crate::ui::icons::Icon::BrandMark, Rect::new(x, y, LOGIN_LOGO, LOGIN_LOGO), theme::BRAND_FILL);
    y += LOGIN_LOGO + LOGIN_LOGO_GAP;
    brand.draw(p, Rect::new(x, y, bw, bh));
    y += bh + theme::space::SM;
    tagline.draw(p, Rect::new(x, y, tw, th));
}

/// Which sign-in control a face is (8 Buttons, Inputs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoginKind {
    /// Quick Connect's accent button: white label; focused, lavender with an accent label and a
    /// 4px lavender ring.
    Primary,
    /// `#0a0a0a` with the secondary ink; focused, `#121212` with a 4px accent ring and white ink.
    Secondary,
    /// A text input: `#0a0a0a`; focused, a 4px accent ring.
    Input,
}

impl LoginKind {
    /// The face's focus pop at full focus.
    pub(crate) fn pop(self) -> f32 {
        match self {
            LoginKind::Primary | LoginKind::Secondary => LOGIN_BUTTON_POP,
            LoginKind::Input => LOGIN_INPUT_POP,
        }
    }

    /// The ink of a label or mark on this face.
    pub(crate) fn ink(self, focused: bool) -> [f32; 4] {
        match (self, focused) {
            (LoginKind::Primary, false) => theme::TEXT_PRIMARY,
            (LoginKind::Primary, true) => theme::SELECTED_INK,
            (LoginKind::Secondary, false) => theme::TEXT_SECONDARY,
            (LoginKind::Secondary | LoginKind::Input, true) | (LoginKind::Input, false) => theme::TEXT_PRIMARY,
        }
    }
}

/// `rect` popped by `focus` (0 at rest, 1 fully focused) toward the kind's own scale.
pub(crate) fn login_popped(rect: Rect, kind: LoginKind, focus: f32) -> Rect {
    let k = 1.0 + (kind.pop() - 1.0) * focus.clamp(0.0, 1.0);
    Rect::new(rect.cx() - rect.w * k * 0.5, rect.cy() - rect.h * k * 0.5, rect.w * k, rect.h * k)
}

/// **A sign-in control's face** at `rect` (already popped): its fill and, focused, its 4px ring.
pub(crate) fn draw_login_face(p: Painter, rect: Rect, kind: LoginKind, focused: bool) {
    use theme::settings_lf::{LOGIN_FILL, LOGIN_RAD, LOGIN_RING, LOGIN_RING_W};
    let (fill, ring) = match (kind, focused) {
        (LoginKind::Primary, false) => (theme::BRAND_FILL, None),
        (LoginKind::Primary, true) => (theme::SELECTED_FILL, Some(theme::FOCUS_BORDER)),
        (LoginKind::Secondary, false) | (LoginKind::Input, _) => (LOGIN_FILL, (focused).then_some(LOGIN_RING)),
        (LoginKind::Secondary, true) => (theme::CARD_BG_HOVER, Some(LOGIN_RING)),
    };
    p.rrect(rect, LOGIN_RAD, LOGIN_RAD, fill);
    if let Some(ring) = ring {
        p.rring(rect, LOGIN_RAD, LOGIN_RING_W, ring);
    }
}

/// **A sign-in button**: [`draw_login_face`] with its label centred at `S26` bold, and an optional
/// leading mark beside it.
pub(crate) fn draw_login_button(p: Painter, rect: Rect, label: &CStr, kind: LoginKind, focused: bool,
    focus: f32, mark: Option<crate::ui::icons::Icon>, measure: &dyn nj_machine::machine::Measure) {
    let r = login_popped(rect, kind, focus);
    draw_login_face(p, r, kind, focused);
    let ink = kind.ink(focused);
    let text_w = measure.width(label, LOGIN_CTL_SZ, true);
    let gap = theme::space::SM;
    let mark_w = if mark.is_some() { LOGIN_INPUT_MARK + gap } else { 0.0 };
    let x = r.cx() - (text_w + mark_w) * 0.5;
    if let Some(icon) = mark {
        crate::ui::icons::draw(p, icon, Rect::new(x, r.cy() - LOGIN_INPUT_MARK * 0.5, LOGIN_INPUT_MARK, LOGIN_INPUT_MARK), ink);
    }
    Label::new(label.as_ptr(), LOGIN_CTL_SZ, ink).bold().v(VAlign::Middle).draw(p, Rect::new(x + mark_w, r.y, text_w + 1.0, r.h));
}

/// **An input's face and mark**; the caller draws its text from `rect.x + LOGIN_INPUT_TEXT_X`.
/// Returns the popped rect the text belongs in.
pub(crate) fn draw_login_input(p: Painter, rect: Rect, mark: crate::ui::icons::Icon, focused: bool, focus: f32) -> Rect {
    let r = login_popped(rect, LoginKind::Input, focus);
    draw_login_face(p, r, LoginKind::Input, focused);
    let ink = if focused { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY };
    crate::ui::icons::draw(p, mark, Rect::new(r.x + LOGIN_INPUT_MARK_X, r.cy() - LOGIN_INPUT_MARK * 0.5, LOGIN_INPUT_MARK, LOGIN_INPUT_MARK), ink);
    r
}

/// Who a who's-watching tile is: a person (their avatar), or an action (a mark on the bare circle).
pub(crate) enum LoginPerson<'a> {
    Person { name: &'a str, id: &'a str },
    Mark(crate::ui::icons::Icon),
}

/// The cell a tile's name (and quiet line) is drawn in: centred on the circle, the grid's `pitch`
/// wide less [`theme::space::SM`] of air, so neighbours' names never meet however tight the grid.
pub(crate) fn login_name_span(circle: Rect, pitch: f32) -> (f32, f32) {
    let w = (pitch - theme::space::SM).max(circle.w);
    (circle.cx() - w * 0.5, w)
}

/// **A who's-watching tile** (8 User picker): the [`LOGIN_AVATAR`] circle at `circle` (the rest
/// rect, popped by `scale`), the name under it and an optional quiet line under that, in a
/// [`login_name_span`] of the grid's `pitch`. Focused, the circle wears a 4px accent ring and the
/// name turns white.
pub(crate) fn draw_login_person(p: Painter, circle: Rect, pitch: f32, who: LoginPerson<'_>, label: &str,
    sub: Option<&str>, focused: bool, scale: f32) {
    use theme::settings_lf::{LOGIN_FILL, LOGIN_RING, LOGIN_RING_W};
    let r = Rect::new(circle.cx() - circle.w * scale * 0.5, circle.cy() - circle.h * scale * 0.5, circle.w * scale, circle.h * scale);
    let rad = r.w * 0.5;
    p.rrect(r, rad, rad, LOGIN_FILL);
    let inner = r.w * LOGIN_AVATAR_IMG / LOGIN_AVATAR;
    let img = Rect::new(r.cx() - inner * 0.5, r.cy() - inner * 0.5, inner, inner);
    let ink = if focused { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY };
    match who {
        LoginPerson::Person { name, id } => avatar_disc(p, img, name, id, false, theme::size::HERO),
        LoginPerson::Mark(icon) => {
            let m = img.w * 0.34;
            crate::ui::icons::draw(p, icon, Rect::new(img.cx() - m * 0.5, img.cy() - m * 0.5, m, m), ink);
        }
    }
    if focused {
        p.rring(r, rad, LOGIN_RING_W, LOGIN_RING);
    }
    let (tx, cell_w) = login_name_span(circle, pitch);
    let ty = circle.y + circle.h + (r.h - circle.h) * 0.5 + LOGIN_NAME_GAP;
    TextView::new(label, LOGIN_CTL_SZ, ink).bold().h(HAlign::Center).max_lines(1).draw(p, Rect::new(tx, ty, cell_w, LOGIN_NAME_H));
    if let Some(sub) = sub {
        TextView::new(sub, theme::size::lf::S19, theme::TEXT_TERTIARY)
            .h(HAlign::Center)
            .max_lines(1)
            .draw(p, Rect::new(tx, ty + LOGIN_NAME_H, cell_w, LOGIN_SUB_H));
    }
}

/// [`CtlPop`]'s focus progress for control `i`, 0 at rest and 1 fully popped, so a Litefin face can
/// pop to its own scale on the shared spring. `0` if the shared pop is ever retuned to none.
pub(crate) fn pop_progress<const N: usize>(pop: &CtlPop<N>, i: usize, press_scale: f32) -> f32 {
    let span = CTRL_FOCUS_SCALE - 1.0;
    if span <= 0.0 { 0.0 } else { (pop.scale_with(i, press_scale) - 1.0) / span }
}

/// Row pitch. Values are [`FIELD_VAL_SZ`]; this is that plus air, and it is what bounds how many
/// fields the overlay may carry — see `app::diagnostics`'s `LEFT_ROWS`/`RIGHT_ROWS`.
pub const FIELD_ROW_H: f32 = 26.0;
/// The diagnostics instrument's dense type.  It is intentionally smaller than product copy: a
/// fixed two-column schema is more useful than one large sentence wrapping under another, and the
/// owner explicitly prefers density as long as every run remains present and readable.
pub const FIELD_VAL_SZ: i32 = theme::size::DIAGNOSTIC;
/// Width of the key column inside a [`FieldList`] frame. Keys are right-aligned against it and
/// values start one `space::SM` later, so every value in a column shares an x (the digits are
/// proportional, so a column of numbers shares its left edge rather than its digit columns).
pub const FIELD_KEY_W: f32 = 132.0;
/// The value column used by [`FieldList`]. Exposed so an owner can measure wrapped rows against
/// exactly the width the list draws. Each diagnostics column is wide enough that every bounded
/// composed value normally stays on one row; exact wrapping remains the no-clipping fallback.
pub const FIELD_VAL_W: f32 = 676.0;
/// Width a [`FieldList`] column needs: the key gutter plus room for the longest value.
pub const FIELD_COL_W: f32 = FIELD_KEY_W + theme::space::SM + FIELD_VAL_W;

pub struct FieldList<'a> {
    pub fields: &'a [Field],
    pub frame: Rect,
}

impl<'a> FieldList<'a> {
    pub fn new(fields: &'a [Field], frame: Rect) -> Self {
        Self { fields, frame }
    }
    /// How tall this list draws — so a caller can size or split its columns without re-deriving
    /// the pitch.
    pub fn height(n: usize) -> f32 {
        n as f32 * FIELD_ROW_H
    }

    /// How many wrapped lines each value needs — the SUM of what [`value_lines`] would produce,
    /// so a measured height and a drawn one can never disagree.
    pub fn wrapped_line_count(fields: &[Field]) -> usize {
        fields
            .iter()
            .map(|field| match &field.val {
                None => 1,
                Some(_) => field.lines.len().max(1),
            })
            .sum()
    }
}

/// A value split into the exact lines this list will draw. Simulator/device builds use live glyph
/// advances; host tests, which deliberately do not link SDL_ttf, use a conservative character
/// budget. Both paths wrap rather than elide and split even an oversized opaque word, so no suffix
/// can disappear outside the value frame.
///
/// **It is shared by the measure and the paint, and that is the whole point.** The measure existed
/// alone until 2026-08-26 while `draw` emitted ONE `Label` per field and then advanced y by the
/// measured line COUNT — so any value long enough to wrap left a blank band the height of the lines
/// nobody drew, and pushed every row below it down until the last ones fell outside the panel
/// entirely, over the transport. Both halves of that were visible on screen and neither was visible
/// to a test, because the two rules were never compared.
pub fn value_lines(value: &str, width: f32) -> Vec<String> {
    diagnostic_lines(value, width - FIELD_KEY_W - theme::space::SM, false)
}

/// Full-width diagnostic prose, using the same no-elision wrapping as field values. The owner
/// caches these lines with its sampled data and uses their count to position every later block.
/// `width` is the actual text width, with no field-key gutter; `bold` matches the painted face.
pub(crate) fn diagnostic_lines(value: &str, width: f32, bold: bool) -> Vec<String> {
    let value_w = width.max(1.0);
    #[cfg(test)]
    let _ = bold;
    // Once SDL_ttf is live, use the exact same glyph metrics the draw path advances by.  Host
    // tests have no text runtime (`text_width` returns 0), so they fall back to a deliberately
    // conservative character budget.  Both paths preserve every character; neither elides.
    // `value_lines` is called from `Field::new`, which the diagnostics module runs while building
    // its 2 Hz snapshot — off the draw path entirely, so there is no `Cx`/`DrawFrame` in scope to
    // thread a real `Measure` from. `TtfMeasure` wraps the identical `text_width` this closure
    // called directly before, so the measured widths are unchanged.
    #[cfg(not(test))]
    let measure = |s: &str| {
        use nj_machine::machine::Measure;
        CString::new(s)
            .ok()
            .map(|c| nj_gfx::text::TtfMeasure.width(&c, FIELD_VAL_SZ, bold))
            .filter(|w| *w > 0.0)
    };
    // The library unit-test target deliberately does not link SDL_ttf.  Its conservative fallback
    // still proves that no byte is elided or dropped; simulator/device runs exercise exact glyph
    // advances once the live text runtime is present.
    #[cfg(test)]
    let measure = |_s: &str| -> Option<f32> { None };
    if measure(value).is_some() {
        let mut out: Vec<String> = Vec::new();
        for word in value.split_whitespace() {
            let joined = out.last().map(|line| format!("{line} {word}"));
            if joined
                .as_deref()
                .and_then(measure)
                .is_some_and(|w| w <= value_w)
            {
                let line = out.last_mut().expect("joined implies a line");
                line.push(' ');
                line.push_str(word);
                continue;
            }
            if measure(word).is_some_and(|w| w <= value_w) {
                out.push(word.to_string());
                continue;
            }
            // A long opaque token is still support evidence.  Split it on character boundaries
            // instead of cutting or ellipsising its suffix.
            let mut part = String::new();
            for ch in word.chars() {
                let mut candidate = part.clone();
                candidate.push(ch);
                if !part.is_empty() && measure(&candidate).is_some_and(|w| w > value_w) {
                    out.push(std::mem::take(&mut part));
                }
                part.push(ch);
            }
            if !part.is_empty() {
                out.push(part);
            }
        }
        return out;
    }

    let budget = (value_w / VAL_AVG_ADVANCE).max(1.0).floor() as usize;
    let mut out: Vec<String> = Vec::new();
    for word in value.split_whitespace() {
        let word_len = word.chars().count();
        if word_len > budget {
            // The normal formatter emits bounded words, but keeping this total makes FieldList safe
            // for an unexpected codec/tag as well.  Do not let one opaque token bypass wrapping.
            let chars: Vec<char> = word.chars().collect();
            for chunk in chars.chunks(budget) {
                out.push(chunk.iter().collect());
            }
            continue;
        }
        match out.last_mut() {
            // `+ 1` for the space this word would be joined with.
            Some(line) if line.chars().count() + 1 + word.chars().count() <= budget => {
                line.push(' ');
                line.push_str(word);
            }
            _ => out.push(word.to_string()),
        }
    }
    out
}

/// Conservative mean glyph advance at [`FIELD_VAL_SZ`], for [`value_lines`]' character budget. It
/// tracks the value size and nothing else: 15.0 was measured for `size::BODY` 28, and the dense
/// 20px diagnostic face keeps the same conservative ratio.
const VAL_AVG_ADVANCE: f32 = 11.0;

impl View for FieldList<'_> {
    fn draw(&self, _e: &Env, p: Painter) {
        let vx = self.frame.x + FIELD_KEY_W + theme::space::SM;
        let vw = (self.frame.w - FIELD_KEY_W - theme::space::SM).max(0.0);
        let mut y = self.frame.y;
        for f in self.fields.iter() {
            match &f.val {
                // a section heading spans the whole width and carries no value
                None => {
                    // A heading differs from a key by POSITION (full width, left-aligned, where a
                    // key is right-aligned into its gutter) and by weight — not by a separate size.
                    if let Ok(cs) = CString::new(f.key) {
                        Label::new(cs.as_ptr(), FIELD_VAL_SZ, theme::TEXT_SECONDARY)
                            .bold()
                            .draw(p, Rect::new(self.frame.x, y, self.frame.w, FIELD_ROW_H));
                    }
                }
                Some(_) => {
                    if let Ok(cs) = CString::new(f.key) {
                        Label::new(cs.as_ptr(), FIELD_VAL_SZ, theme::TEXT_TERTIARY)
                            .h(HAlign::Right)
                            .draw(p, Rect::new(self.frame.x, y, FIELD_KEY_W, FIELD_ROW_H));
                    }
                    let ink = if f.tone == Tone::Fault {
                        theme::DANGER
                    } else {
                        theme::TEXT_PRIMARY
                    };
                    // WRAP, never elide: this read-out is support evidence, and a hidden suffix
                    // can be the exact fact the photograph was taken to capture. Every line the
                    // measure reserved is drawn — see `value_lines`.
                    for (n, line) in f.lines.iter().enumerate() {
                        if let Ok(cs) = CString::new(line.as_str()) {
                            let mut l = Label::new(cs.as_ptr(), FIELD_VAL_SZ, ink);
                            if f.tone == Tone::Fault {
                                l = l.bold();
                            }
                            l.draw(
                                p,
                                Rect::new(vx, y + n as f32 * FIELD_ROW_H, vw, FIELD_ROW_H),
                            );
                        }
                    }
                }
            }
            y += match &f.val {
                None => FIELD_ROW_H,
                Some(_) => f.lines.len().max(1) as f32 * FIELD_ROW_H,
            };
        }
    }
}

// ---- TabPill: a rounded pill with a centered label. Focused = light pill + dark ink; idle =
// faint fill + dim ink. `TabPill::width_measured` sizes it from glyph advances — NOT `Button::pill_w`, which
// budgets a Button's icon box and air. ----
/// How a `TabPill` reads. The player Info/Chapters tabs are always-filled buttons; the detail season
/// tabs are a segmented control with two *independent* states — a **selected** segment (the active
/// one, whose content shows) and a **highlighted** one (where the remote focus is).
#[derive(Clone, Copy)]
enum TabStyle {
    /// Always a pill — focused → ACCENT, selected → quiet tint, idle → the ground's dark face
    /// (player Info/Chapters).
    Button,
    /// segmented control (detail season tabs): the focused segment is a bright ACCENT pill; the
    /// selected segment gets a subtle pill while focus is elsewhere; the rest are plain dim text.
    Segment { selected: bool },
}

/// The quiet annotation a [`TabPill`] can carry after its label — the detail page's season tabs use
/// it for "you have watched everything behind this tab". It is deliberately subordinate to the
/// label: it stands one type rung down and a step of alpha under the label's own ink, so a tab
/// still reads as its name first and its state second.
///
/// It carried an episode COUNT too, and no longer does (owner call, 2026-07-29: *"I don't like
/// unwatched episodes count in season tab selector. I think it's ok to leave just marks that we
/// watched."*). A count is filing data the season's own episode row already answers by simply
/// existing, and it made every tab wider for a number nobody was reading. The tick is the one fact
/// a tab strip can state that its content cannot: which seasons are behind you.
#[derive(Clone, Copy, Default)]
pub enum TabNote {
    #[default]
    None,
    /// everything behind this tab is watched — a small tick after the label.
    Done,
}

/// Type rung the trailing note is measured against: one below the label, at the couch legibility
/// floor. The tick is a glyph rather than text, but it stands in the same band a label at this rung
/// would, so the note reads as a peer of the name it follows.
const NOTE_SZ: c_int = theme::size::CAPTION;
/// Label → note air inside the pill.
const NOTE_GAP: f32 = theme::space::SM;
/// The watched tick stands as tall as the note's own rung.
const NOTE_TICK_D: f32 = NOTE_SZ as f32;
/// The note's ink is the pill's own ink one alpha step down — de-emphasis without a second colour
/// role, so it stays legible in every one of [`TabStyle`]'s ink states (including the already-dim
/// unselected segment) while never competing with the label.
const NOTE_INK_A: f32 = 0.75;

// ---- TabPill: a rounded pill with a centered label, in one of two state models (TabStyle). ----
pub struct TabPill {
    pub frame: Rect,
    pub label: *const c_char,
    pub sz: c_int,
    pub focused: bool,
    /// A standalone tab can name the open panel while that panel owns actionable focus.
    selected: bool,
    style: TabStyle,
    note: TabNote,
    /// see [`TabPill::plated`]
    plated: bool,
    /// see [`TabPill::mix`] — `None` = this pill owns its own state fill (the boolean model).
    mix: Option<(f32, f32)>,
    /// see [`TabPill::ground`] — reaches [`TabStyle::Button`] and nothing else.
    ground: ControlGround,
}
impl TabPill {
    /// Width from the actual bold glyph advances, shared by paint and hit geometry.
    pub(crate) fn width_measured(label: &str, sz: c_int, measure: &dyn nj_machine::machine::Measure) -> f32 {
        measure.width_str(label, sz, true) + 44.0
    }
    pub fn new(label: *const c_char, sz: c_int, frame: Rect) -> Self {
        Self {
            frame,
            label,
            sz,
            focused: false,
            selected: false,
            style: TabStyle::Button,
            note: TabNote::None,
            plated: false,
            mix: None,
            ground: ControlGround::Keyed,
        }
    }
    pub fn focused(mut self, f: bool) -> Self {
        self.focused = f;
        self
    }
    /// Keep the active standalone tab visible without borrowing its input-focus treatment.
    pub(crate) fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    /// Stand a STANDALONE pill ([`TabStyle::Button`]) on a named [`ControlGround`] — the player
    /// HUD's Info/Chapters, which is that style's only caller and stands on the video plane.
    ///
    /// It reaches no other style, and the omission is the argument: a SEGMENT is a tab inside a
    /// row, inked by its strip's travelling capsules and standing on that row's own ground (the
    /// tab-bar track, or the detail page's controlled one). Neither is video, and a segment has no
    /// control face to swap.
    pub fn ground(mut self, g: ControlGround) -> Self {
        self.ground = g;
        self
    }
    /// switch to the segmented-control look (detail season tabs); `selected` = the active segment.
    /// Give every segment its own faint plate, and the selected one a stronger plate
    /// (`Details Screen.dc.html`'s season tabs). **Opt-in**, because the TOP tab row draws its segments
    /// inside the tab-bar TRACK — that track already is the ground, and plating there would stack two.
    /// The detail page's season tabs have no track, so without this an unselected season was bare text
    /// with no indication it could be pressed.
    pub fn plated(mut self) -> Self {
        self.plated = true;
        self
    }
    /// The BOOLEAN segmented model. It has **no caller today**: both of the app's segmented rows —
    /// the shared top tab bar and the detail season tabs — are [`TabStrip`]s now, and a strip inks its
    /// pills through [`mix`](Self::mix) so the highlight can travel between them. It is kept, with its
    /// four arms below, because it is the right answer for a segmented control that is NOT in a strip
    /// (nothing there to travel), and because deleting it would take [`TabStyle::Segment`] with it and
    /// leave a one-variant enum. Do not reach for it inside a strip: two pills would each paint their
    /// own fill and the capsules would have nothing left to do.
    pub fn segment(mut self, selected: bool) -> Self {
        self.style = TabStyle::Segment { selected };
        self
    }
    /// Strip-driven ink: `focus`/`selected` as 0..1 **mixes** rather than booleans, because the fills
    /// they used to imply are now travelling capsules the STRIP draws ([`TabStrip`]). A mixed pill
    /// paints no state fill of its own — only a plated strip's idle ground, which is a constant per
    /// pill and not a state (and which retires as the opaque focus capsule arrives under it, so the
    /// composite at full focus is exactly the `ACCENT` it always was).
    ///
    /// Leave it unset for a STANDALONE pill — [`TabStyle::Button`], the player HUD's Info/Chapters —
    /// which owns its fill and has no strip to travel in; those keep the boolean model verbatim.
    pub fn mix(mut self, focus: f32, selected: f32) -> Self {
        self.mix = Some((focus, selected));
        self
    }
    /// The ink a strip-driven pill wears at mixes `(focus, selected)` — `TEXT_TERTIARY` →
    /// `TEXT_PRIMARY` as the selection capsule arrives, then all the way to `ACCENT_INK` as the focus
    /// capsule covers it. Nested in that order because focus OUTRANKS selection, which is the order
    /// the boolean arms below are written in.
    ///
    /// Pure and split out of the draw so the states no STILL capture can catch — the mid-travel ones,
    /// where a partly covered pill is inking its whole label — are an executable contract rather than
    /// an eyeball. It is linear in coverage on purpose: [`cap_cover`] is the one rule both the fill
    /// and the ink read, so they cannot disagree about where the capsule is. If a device capture ever
    /// says the labels "dim when I move", shaping this one call (`focus.powi(2)`, a smoothstep) is the
    /// whole fix — but do it HERE, not with a second spring, or the ink starts leading the fill.
    pub(crate) fn mixed_ink(focus: f32, selected: f32) -> [f32; 4] {
        // **The idle ink is `TEXT_READING`, one rung brighter than the `TEXT_TERTIARY` this row wore
        // for most of its life, and it is what pays for the whole material.** The scrim is solved so
        // this ink clears [`TRACK_INK_CONTRAST`]: a muted grey needs .61 of black over the Office
        // hero and .56 over a cyan sky, which is a band laid across the picture, while this one needs
        // the floor for both. The row is louder for it — idle and selected are now two rungs apart
        // rather than the full span — and the plate carries the difference, which is what the
        // reference does too.
        theme::mix(
            theme::mix(
                theme::TEXT_READING,
                theme::TEXT_PRIMARY,
                selected.clamp(0.0, 1.0),
            ),
            crate::ui::ACCENT_INK,
            focus.clamp(0.0, 1.0),
        )
    }
    /// attach a trailing [`TabNote`] (the watched tick).
    pub fn note(mut self, note: TabNote) -> Self {
        self.note = note;
        self
    }
    /// What the note adds to a pill's CONTENT width, gap included (0 for [`TabNote::None`]). The
    /// caller lays its tab strip out with this, so pill widths, the strip's x-advance and the note
    /// itself can never disagree about how wide a tab is.
    pub fn note_w(note: TabNote) -> f32 {
        match note {
            TabNote::None => 0.0,
            TabNote::Done => NOTE_GAP + NOTE_TICK_D,
        }
    }
    /// The pill's `(fill, ink, weight 0..1)` for its current state, with no painter in it — so
    /// both state models, and the GROUND the standalone pill answers to, can be graded on the host.
    ///
    /// Two state models, and the strip-driven one lerps between exactly
    /// the same three ink roles the boolean match picks from, so a mixed pill at 0/1 is the old
    /// look to the bit (see `the_ink_a_pill_wears_is_exactly_the_capsule_over_it`).
    ///
    /// Boolean model: a highlighted (focused) tab is a bright ACCENT pill; a selected-but-
    /// unfocused segment is a subtle pill; a plain segment is dim text with no pill at all.
    /// Legibility over art is the CONTAINER's job (the tab-bar track in `draw_tab_row`), not
    /// the segment's — the detail season tabs use this element bare on a controlled ground.
    fn face(&self) -> (Option<[f32; 4]>, [f32; 4], f32) {
        match self.mix {
            Some((fm, sm)) => {
                let (fm, sm) = (fm.clamp(0.0, 1.0), sm.clamp(0.0, 1.0));
                let ink = Self::mixed_ink(fm, sm);
                // The plated strip's idle ground stays the PILL's, because it is a constant per pill
                // rather than a state — but it must not lie on top of the OPAQUE focus capsule the
                // strip drew under it (0.08 of white over `ACCENT` is a ~2/255 lift and a bright rim
                // exactly where the capsule is brightest), so it retires as that capsule arrives. The
                // SELECTION capsule needs no such treatment: two whites composite to `a + b − ab`
                // whichever way round they are stacked, which is how `TAB_PLATE_SELECTED_OVER` lands
                // back on `TAB_PLATE_SELECTED`'s .20 from underneath.
                let plate_a = theme::TAB_PLATE_IDLE[3] * (1.0 - fm);
                let fill = (self.plated && plate_a > 0.002)
                    .then(|| theme::with_a(theme::TAB_PLATE_IDLE, plate_a));
                (fill, ink, sm.max(fm))
            }
            None => match self.style {
                TabStyle::Button if self.focused => {
                    (Some(crate::ui::ACCENT), crate::ui::ACCENT_INK, 1.0)
                }
                TabStyle::Button if self.selected => {
                    // Both standalone neighbours already have a resting plate. Use the shared
                    // plated-selection tint so the active mode stays visible beside that ground.
                    (Some(theme::TAB_PLATE_SELECTED), theme::TEXT_PRIMARY, 1.0)
                }
                // The standalone pill is a CONTROL FACE, so its idle fill is the one the GROUND
                // supplies — the light film over video, the dark plate on a page. See
                // [`ControlGround`], and the rim it is drawn with below.
                TabStyle::Button => (Some(self.ground.idle_fill()), theme::CONTROL_IDLE_INK, 1.0),
                TabStyle::Segment { .. } if self.focused => {
                    (Some(crate::ui::ACCENT), crate::ui::ACCENT_INK, 1.0)
                }
                TabStyle::Segment { selected: true } if self.plated => {
                    (Some(theme::TAB_PLATE_SELECTED), theme::TEXT_PRIMARY, 1.0)
                }
                TabStyle::Segment { .. } if self.plated => {
                    (Some(theme::TAB_PLATE_IDLE), theme::TEXT_TERTIARY, 0.0)
                }
                TabStyle::Segment { selected: true } => {
                    (Some(theme::OVERLAY_FOCUS_PILL), theme::TEXT_PRIMARY, 1.0)
                }
                // `TEXT_TERTIARY` and not the standing track's brighter `TEXT_READING`, on purpose:
                // this arm draws on a CONTROLLED ground (the detail page's season row) where nothing
                // is solved, so the muted idle ink costs nothing and is the design-system default.
                // See `a_settled_mixed_pill_is_the_boolean_look_it_replaced`.
                TabStyle::Segment { .. } => (None, theme::TEXT_TERTIARY, 0.0),
            },
        }
    }
}
impl View for TabPill {
    fn draw(&self, _e: &Env, p: Painter) {
        let r = self.frame;
        let (fill, ink, bold_mix) = self.face();
        if let Some(bg) = fill {
            let rad = r.h * 0.5;
            if matches!(self.style, TabStyle::Button) {
                let r = face_box(r);
                let rad = r.h * 0.5;
                if self.focused {
                    control_cast(p, r, rad);
                }
                // A STANDALONE pill is one of the app's control faces — the same object as the
                // transport disc beside it on the HUD — so it wears the same edge, [`control_rim`],
                // rather than a bare fill. That is not cosmetic on the unkeyed ground: the idle face
                // there is a 10% film, and the rim is the only thing separating it from what it
                // stands on. A SEGMENT keeps the bare fill: its edge is its strip's business.
                control_rim(p, r, rad, bg, bg, self.focused, self.ground);
            } else {
                p.rrect(r, rad, rad, bg);
            }
        }
        // The label centres in the pill MINUS the note group, so a tab carrying a note keeps
        // label+note centred as one unit rather than shoving the label off-centre. The note then
        // hugs the label's own PAINTED right edge (the width `Label::draw` hands back), which is
        // why this needs no knowledge of the caller's pill padding.
        //
        // Painted, deliberately, not the width the caller measured: a strip that sizes its tabs
        // with the BOLD label (as the season tabs and `draw_tab_row` both do, so an advance can't
        // move with focus) paints a NON-bold label a couple of px narrower. Anchoring off the
        // caller's number instead would pin the note while the label's right edge slid under it,
        // i.e. it would trade a constant label→note gap for a variable one. The gap is the
        // relationship the eye reads here; the few px of slack left inside the pill's own padding
        // on an unbolded tab is the same slack the label alone already had. The one exception is the
        // crossfade below, which has two painted widths and so no single answer — it falls back to
        // the caller's own bold advance rather than letting the tick slide as the weights swap.
        let nw = Self::note_w(self.note);
        let frame = Rect::new(r.x, r.y, r.w - nw, r.h);
        let run = |p: Painter, bold: bool| {
            let mut lab = Label::new(self.label, self.sz, ink).h(HAlign::Center);
            if bold {
                lab = lab.bold();
            }
            lab.draw(p, frame)
        };
        // WEIGHT crossfades; it does not tween. A bold run and a regular run are two rasterizations
        // — the same reason `theme.rs`'s size ladder says a SIZE is crossfaded rather than animated,
        // and the way `detail.rs` spells its hero → compact title. (`person.rs`'s band condense was
        // the other worked example until that band stopped condensing.) Two draws happen only while a capsule is
        // genuinely mid-travel over THIS pill: at most two pills, for ~200 ms. Both boolean states
        // land squarely in the single-draw branches, so nothing that exists today pays for this.
        let lw = if bold_mix > 0.98 {
            run(p, true)
        } else if bold_mix < 0.02 {
            run(p, false)
        } else {
            run(p.alpha(1.0 - bold_mix), false);
            run(p.alpha(bold_mix), true);
            // `TabPill` draws through the generic retui `View::draw(&self, env: &Env, p: Painter)`
            // — no `Measure` parameter, and that trait is the whole retained-leaf contract, not
            // something this lane's scope covers reshaping. `TtfMeasure` wraps the identical
            // `text_width` this line called directly.
            {
                use nj_machine::machine::Measure as _;
                LegacyMeasure.width(
                    unsafe { std::ffi::CStr::from_ptr(self.label) },
                    self.sz,
                    true,
                )
            }
        };
        let nx = r.x + (r.w - nw) * 0.5 + lw * 0.5 + NOTE_GAP;
        let note_ink = theme::with_a(ink, NOTE_INK_A);
        match self.note {
            TabNote::None => {}
            TabNote::Done => {
                let d = NOTE_TICK_D;
                crate::ui::icons::draw(
                    p,
                    crate::ui::icons::Icon::Check,
                    Rect::new(nx, r.y + (r.h - d) * 0.5, d, d),
                    note_ink,
                );
            }
        }
    }
}

// ---- Tab strip MOTION: the travelling capsules. ----
// A `TabPill` is a retui LEAF: it has no idea its neighbours exist, so it structurally cannot
// animate *between* pills — which is why the selection used to vanish from one pill and reappear on
// the next in a single frame. The highlight is therefore hoisted out of the pill and into the STRIP,
// which knows every pill's span and can carry one fill from one to another. The pill keeps only its
// label, its constant ground, and the ink it derives from how covered it is.
//
// Shared by the top tab bar (`draw_tab_row`) and the detail page's season tabs, per the owner
// directive recorded above [`TAB_PILL_H`]: the two rows are ONE control, and one control has one
// motion. The stiffnesses below are matched to springs that already exist rather than invented.

/// Stiffness of a tab strip's travelling capsules. Deliberately IDENTICAL to [`K_TAB_SCROLL`]: the
/// capsule rides *inside* the strip it marks, and a highlight that settles at a different rate from
/// the row under it reads as two objects instead of one control. (The detail season row springs its
/// own scroll at the same 240 — the directive above [`TAB_PILL_H`] covers their motion too.)
const K_TAB_CAP: f32 = K_TAB_SCROLL;
/// Stiffness of a capsule's ALPHA — the app's shared appear spring
/// ([`crate::ui::popover::K_APPEAR`]), because entering or leaving a row is a FADE, not a journey.
const K_TAB_CAP_A: f32 = crate::ui::popover::K_APPEAR;
/// Below this alpha a capsule is not on screen, so it LANDS on its next pill instead of gliding to
/// it (see [`Capsule::step`]). 0.02 is picked to be *invisibly* small rather than merely small: the
/// faintest capsule is [`theme::OVERLAY_FOCUS_PILL`] at .14, so at this alpha it composites to
/// .0028 of white — under one display code on the panel's 8-bit framebuffer — and a jump there
/// cannot be seen however far it travels.
const CAP_LAND_A: f32 = 0.02;

/// How much of pill `pill` (content-space `(x, w)`) a capsule spanning `cap` covers, 0..1 — a 1-D
/// [`Rect::intersect`]. This is the ONE rule a pill's ink is derived from, so the ink can never
/// disagree with the fill sliding under it: a dark label left behind on a pill the capsule has
/// already left is unreadable, and that is exactly what a separate per-pill ink spring would
/// produce. A zero-width pill covers nothing (and, more to the point, does not divide by zero).
fn cap_cover(pill: (f32, f32), cap: (f32, f32)) -> f32 {
    if pill.1 <= 0.0 {
        return 0.0;
    }
    let lo = pill.0.max(cap.0);
    let hi = (pill.0 + pill.1).min(cap.0 + cap.1);
    ((hi - lo) / pill.1).clamp(0.0, 1.0)
}

/// A tab highlight that TRAVELS between pills instead of being repainted onto a new one. Two springs
/// for its content-space geometry — left edge AND width, so a wide pill *morphs* into a narrow one
/// rather than teleporting — plus one for alpha, which is how it enters and leaves a row without
/// streaking across the strip from wherever it was last parked.
#[derive(Clone, Copy)]
pub(crate) struct Capsule {
    x: Spring,
    w: Spring,
    a: Spring,
    /// The pill index this capsule is bound to, or -1 for "nothing to mark". Tracked so a capsule
    /// that has never been placed can tell that apart from one resting at content x = 0 (which is a
    /// real position: it is pill 0).
    at: i32,
}

impl Capsule {
    pub(crate) const fn new() -> Self {
        Capsule {
            x: Spring::at(0.0),
            w: Spring::at(0.0),
            a: Spring::at(0.0),
            at: -1,
        }
    }
    /// One frame toward pill `i`'s content-space `(x, w)`. `None` = nothing to mark (focus left the
    /// row, or the index went stale after a section refetch): HOLD the position and fade out, so
    /// focus leaving and returning to the SAME pill is a fade, not a round trip.
    ///
    /// A capsule that is not on screen ([`CAP_LAND_A`]) LANDS rather than glides. Without this, the
    /// first frame of the Library screen — and every return of focus to the row — would start with a
    /// bright capsule flying in from the pill the user was on two screens ago.
    ///
    /// `land` forces that on every move, for a mark whose job is to say WHICH and not to draw a path
    /// between two answers — see [`SelMark::Lands`].
    fn step(&mut self, target: Option<(usize, (f32, f32))>, land: bool, dt: f32) {
        match target {
            Some((i, (x, w))) => {
                if land || self.at < 0 || self.a.pos < CAP_LAND_A {
                    self.x.jump(x);
                    self.w.jump(w);
                }
                self.at = i as i32;
                self.x.step(x, K_TAB_CAP, dt);
                self.w.step(w, K_TAB_CAP, dt);
                self.a.step(1.0, K_TAB_CAP_A, dt);
            }
            None => {
                self.at = -1;
                self.a.step(0.0, K_TAB_CAP_A, dt);
            }
        }
    }
    /// Content-space `(x, w)` as drawn this frame.
    #[inline]
    fn span(&self) -> (f32, f32) {
        (self.x.pos, self.w.pos)
    }
    #[inline]
    fn alpha(&self) -> f32 {
        self.a.pos.clamp(0.0, 1.0)
    }
    /// How strongly pill `pill` is wearing this capsule right now — coverage × alpha.
    #[inline]
    fn mix(&self, pill: (f32, f32)) -> f32 {
        self.alpha() * cap_cover(pill, self.span())
    }
}

// ---- a strip's PILL GEOMETRY, shared by every strip a [`TabStrip`] drives ---------------------
//
// **These lived in `ui/detail.rs`, private to the season strip, until the Library grew a strip of
// its own.** `TabStrip` and `TabPill` were already shared; the numbers that PLACE their pills were
// not, so a second consumer's only options were to import private constants or to hand-author a
// copy — and a copy is the fork the directive above [`TAB_PILL_H`] forbids, arrived at from
// underneath. One padding, one gap, one advance, one pill rect, one span function.
//
// The row HEIGHT stays the caller's, because the two strips genuinely differ there: the season
// strip stands as tall as the hero buttons beside it, and the Library's head as tall as the
// control row it heads.

/// A strip pill's padding either side of its CONTENT (`Details Screen.dc.html`'s `padding: 0 26px`,
/// up from 18). A 60px-tall pill wrapped on 18 read as a tall thin lozenge; at 26 the pill is the
/// capsule the mock draws, and it matches the Play pill's own label inset beside it.
pub(crate) const STRIP_PAD: f32 = 26.0;
/// Air BETWEEN two pills of a strip — the tight rung, for a strip of SHORT labels.
pub(crate) const STRIP_GAP: f32 = theme::space::SM;
/// …and the wide rung, for a strip whose pills carry PHRASES rather than words.
///
/// Owner call from the panel, on the Library's library row and the Filmography route's department
/// filter together. The padding is a property of the pill and never changes; the air between two
/// of them is a property of the ROW, and at `SM` two adjacent plates carrying several words each
/// read as one long run with a seam in it rather than as two controls. The season strip keeps
/// [`STRIP_GAP`]: "Season 3" is a word and a number, and it separates cleanly at the tight rung.
pub(crate) const STRIP_GAP_WIDE: f32 = theme::space::MD;
/// Per-pill horizontal advance past the CONTENT width — derived from the two above rather than
/// spelled, so a change to the padding cannot silently change the gap (which is what 52 vs 18 used
/// to hide).
pub(crate) const STRIP_ADVANCE: f32 = 2.0 * STRIP_PAD + STRIP_GAP;

/// ONE strip pill's resolved layout: its index, content-space label x, the pill's CONTENT width
/// (the label plus whatever trailing note it carries) and the label CString.
pub(crate) struct StripLay {
    pub(crate) i: usize,
    pub(crate) x: f32,
    pub(crate) w: f32,
    pub(crate) label: CString,
}

/// A strip pill's frame: padded [`STRIP_PAD`] either side of its content, standing the full row
/// height so the pills read as one control family with whatever sits beside them. `top` is the
/// row's local y for the draw and its screen y for the hit-test; any horizontal scroll is applied
/// by the caller, which is why this takes the layout entry rather than reaching for one.
pub(crate) fn strip_pill_rect(lay: &StripLay, top: f32, h: f32) -> Rect {
    Rect::new(lay.x - STRIP_PAD, top, lay.w + 2.0 * STRIP_PAD, h)
}

/// Content-space `(x, w)` of pill `i` — **the frame, not the label inside it.** The one span
/// function a [`TabStrip`] is placed from, so a capsule can only ever come to rest exactly on a
/// pill; `TabStrip::update`'s contract is that this IS the function the caller laid out with.
pub(crate) fn strip_span(lays: &[StripLay], i: usize, h: f32) -> Option<(f32, f32)> {
    lays.get(i).map(|l| {
        let r = strip_pill_rect(l, 0.0, h);
        (r.x, r.w)
    })
}

/// **Draw a whole strip — both capsules, then every on-screen pill — through ONE scroll offset.**
///
/// This is the one draw path every [`TabStrip`] consumer uses (the detail page's season tabs, the
/// Filmography department filter and the Library's library row), because the capsules and the pills
/// are two halves of one picture and must never be placed by two offsets. The Filmography route
/// drew them with two: its pills were laid out already scrolled while its capsules were drawn in
/// content space through an UN-translated painter, so the moment the row scrolled the focus plate
/// slid off its own label by exactly the scroll (issue 14). Here the translate is applied once and
/// both halves are drawn through it.
///
/// Pills wholly outside the screen are CULLED against the painter's own screen x (the scroll and
/// any page slide already folded in), so a long strip costs its visible pills, not its length.
/// `top` is the row's y in the painter's space; `scroll` is the strip's horizontal scroll.
pub(crate) fn draw_strip(
    p: Painter,
    strip: &TabStrip,
    lays: &[StripLay],
    top: f32,
    h: f32,
    scroll: f32,
    ground: TabGround,
) {
    let p = p.translate(-scroll, 0.0);
    strip.draw(p, top, h, ground);
    let env = Env::inert();
    for lay in lays {
        let pill = strip_pill_rect(lay, top, h);
        if !crate::ui::on_axis(pill.x + p.dx(), pill.w, crate::ui::consts::SCR_W, 0.0) {
            continue;
        }
        let (fm, sm) = strip.mixes((pill.x, pill.w));
        TabPill::new(lay.label.as_ptr(), theme::size::BODY, pill)
            .plated()
            .mix(fm, sm)
            .draw(&env, p);
    }
}

/// The same strip geometry using an owned screen's measurement capability.
///
/// **`gap` is a parameter and [`STRIP_GAP`] is its default**, not its only value. The padding is a
/// property of the PILL and is shared unconditionally; the air between two of them is a property of
/// the ROW, and the two strips want different amounts of it — a strip of two or three long library
/// names reads as one run at the season strip's 16px, where a row of short department names does
/// not. Pass [`STRIP_GAP`] to keep the shared rhythm.
///
/// A label that cannot be a `CString` (an interior NUL — never in practice) is skipped entirely:
/// not drawn, and no advance, so the gap it would have left cannot desynchronise the span function
/// from the draw.
///
/// This is the ONLY strip-layout entry point (phase 12, P8-H): a raw-`text_width` twin,
/// `strip_layout`, used to sit beside it and had acquired zero callers of its own — every caller
/// had already migrated to a threaded `Measure` — so it was deleted rather than converted.
pub(crate) fn strip_layout_measured(
    labels: impl Iterator<Item = String>,
    x0: f32,
    sz: c_int,
    gap: f32,
    measure: &dyn nj_machine::machine::Measure,
) -> Vec<StripLay> {
    strip_layout_by(labels, x0, gap, |label| measure.width(label, sz, true))
}

fn strip_layout_by(
    labels: impl Iterator<Item = String>,
    x0: f32,
    gap: f32,
    mut width: impl FnMut(&CStr) -> f32,
) -> Vec<StripLay> {
    let mut x = x0;
    let mut out = Vec::new();
    for (i, label) in labels.enumerate() {
        let Ok(lc) = CString::new(label) else { continue };
        let w = width(&lc);
        out.push(StripLay { i, x, w, label: lc });
        x += w + 2.0 * STRIP_PAD + gap;
    }
    out
}

/// The motion state of ONE tab strip: the subtle capsule marking the SELECTED tab and the bright one
/// marking the FOCUSED tab, both travelling. Two, not one, because they are independently placed —
/// on the Library screen the selected tab is the section you are browsing while focus walks the row
/// — and one capsule cannot be in two places. Held as a value, not a global: the top row keeps one
/// static instance, the detail page keeps one per `DetailView` (so a new item's strip starts clean).
#[derive(Clone, Copy)]
pub(crate) struct TabStrip {
    sel: Capsule,
    foc: Capsule,
}

/// How a strip's SELECTION plate answers a change of selection. The FOCUS capsule always travels —
/// it is the ring following your thumb, and the path between two pills is the information.
///
/// Selection is a different statement, and on one of the two strips the path is noise.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum SelMark {
    /// **It travels**, as the focus capsule does: the shared top tab bar and the sources popover's
    /// segmented control, where the plate marks a page you are ON and sliding it reads as the row
    /// re-pointing itself.
    Travels,
    /// **It lands** on the newly selected pill with no path in between — the detail page's season
    /// strip (owner, 2026-08-22: the sliding grey pill "does not fit").
    ///
    /// The case is worth stating because it is the one where the travel was invisible AND ugly at
    /// the same time. Selection only ever changes here by pressing a season, and the press leaves
    /// the bright focus capsule sitting on that very pill — so the grey plate's whole journey
    /// happens UNDER an accent capsule that has already answered the question, and the only part of
    /// it anyone sees is a grey edge crawling out from behind the white one. A mark that says
    /// "this one" has nothing to gain from drawing the route it took.
    Lands,
}

/// The ground a [`TabStrip`] is drawn on. The top tab bar's `Tracked` strip went with the bar
/// (the Litefin rail replaced it), which leaves the plated one: the detail page's season tabs,
/// the Filmography filter and the Library's library row, bare on artwork with each pill laying its
/// own ground. Its focused pill is a CONTROL FACE and wears the whole of one: the
/// `CTRL_FOCUS_SCALE` pop and the press dip, both folded into `pop` (normally a `CtlPop<1>`'s
/// [`scale`](CtlPop::scale), which is what puts `press::scale()` in it), plus the control rim. The
/// selection capsule still does not scale — see [`TabStrip::draw`].
#[derive(Clone, Copy)]
pub(crate) enum TabGround {
    Plated { pop: f32 },
}

impl TabStrip {
    pub(crate) const SHAPE: &'static str = "TabStrip{sel:Capsule{x:Spring{pos:f32,vel:f32},w:Spring{pos:f32,vel:f32},a:Spring{pos:f32,vel:f32},at:u32},foc:Capsule{x:Spring{pos:f32,vel:f32},w:Spring{pos:f32,vel:f32},a:Spring{pos:f32,vel:f32},at:u32}}";

    /// Held capsule geometry and velocity determine the next frame even before a new target.
    pub(crate) fn write_motion(&self, c: &mut nj_machine::machine::Canon) {
        let Self { sel, foc } = self;
        for capsule in [sel, foc] {
            let Capsule { x, w, a, at } = capsule;
            for spring in [x, w, a] { c.f32(spring.pos).f32(spring.vel); }
            c.u32(*at as u32);
        }
    }

    pub(crate) const fn new() -> Self {
        TabStrip {
            sel: Capsule::new(),
            foc: Capsule::new(),
        }
    }
    /// Step both capsules. `span(i)` resolves pill `i`'s content-space `(x, w)` and MUST be the same
    /// function the caller lays its pills out with — that is what keeps a capsule from ever landing
    /// off a pill. A negative or out-of-range index resolves to `None` (nothing to mark).
    pub(crate) fn update(
        &mut self,
        selected: c_int,
        focused: c_int,
        span: impl Fn(usize) -> Option<(f32, f32)>,
        sel: SelMark,
        dt: f32,
    ) {
        let pick = |i: c_int| -> Option<(usize, (f32, f32))> {
            let i = usize::try_from(i).ok()?;
            span(i).map(|s| (i, s))
        };
        let (sel_t, foc_t) = (pick(selected), pick(focused));
        // The focus capsule's REAL spring target, read before the step. Every other probe site
        // passes one, and it is the whole of what the diagnostic measures: with `pos` handed in as
        // its own target the overshoot and settle-frame numbers degenerate to nothing on every
        // frame, including the travelling ones the probe exists to look at. `None` means "hold
        // where you are" (see [`Capsule::step`]), so on those frames the target IS the position.
        let foc_x = foc_t.map(|(_, (x, _))| x).unwrap_or(self.foc.x.pos);
        self.sel.step(sel_t, sel == SelMark::Lands, dt);
        self.foc.step(foc_t, false, dt);
        crate::ui::anim::probe("tabstrip.foc", self.foc.x.pos, self.foc.x.vel, foc_x, dt);
    }
    /// Draw both capsules in the painter's own (already scroll-translated) CONTENT space, for a strip
    /// whose pills stand `h` tall at `top`. Selection first, focus over it — when they are on the same
    /// pill the bright one wins, exactly as the boolean match ordered its arms. [`TabGround`] says
    /// which of the two strips this is, and carries everything that differs between them.
    ///
    /// Call this BEFORE the pill loop: the capsules are the pills' ground, and a label drawn under an
    /// opaque focus capsule is a label nobody can read.
    pub(crate) fn draw(&self, p: Painter, top: f32, h: f32, ground: TabGround) {
        let cap = |c: &Capsule, col: [f32; 4], rim: Option<[f32; 4]>, scale: f32, cast: bool| {
            let (x, w) = c.span();
            let a = c.alpha();
            // sub-code alpha or a sub-pixel width is nothing on screen but still a full rrect pass
            if a > 0.004 && w > 0.5 {
                // The pop + press dip, about the capsule's own centre — 1.0 for a tracked strip,
                // which never scales. The RADIUS stays keyed to the drawn height so a scaled capsule
                // is still a capsule rather than a rounded rectangle.
                let r = Rect::new(x, top, w, h).scaled(scale);
                let h = r.h;
                // The lift every other focused control face wears (`control_cast`, via
                // `Button::plate`) — owner correction, 2026-09-06: a plated strip's focused pill IS
                // a control face (this fn's own doc says so, two paragraphs down) and a control face
                // that never casts reads as a sticker pasted on the artwork rather than a pressable
                // button, exactly the defect `linked_heading::Entry` had before its own fix. Never
                // for a TRACKED pill or the selection plate — see the call sites' own comments for
                // why each of those stays flat.
                if cast {
                    control_cast(p, r, h * 0.5);
                }
                match rim {
                    // On a GLASS track the selection plate is a piece of the same material, not a
                    // white wash: its own perimeter line and its own brighter top edge, exactly as
                    // the track wears them. Without an edge a translucent plate over a translucent
                    // band has nothing to be bounded BY — it lands differently on each side of
                    // itself and reads as a smudge, which is what the first device photograph of
                    // this bar showed and what no amount of fill alpha fixes.
                    // …and the boost to the shared lamp on the side facing it, derived from the
                    // perimeter's OWN alpha rather than named: the two callers pass two different
                    // perimeters (a glass track's `GLASS_RIM`, a control face's `CARD_SHEEN`) and
                    // both want the crown at `GLASS_RIM_LIGHT`. Written as one subtraction, that is
                    // the same expression `control_rim` makes.
                    Some(rc) => p.alpha(a).rect_rimmed(
                        r,
                        h * 0.5,
                        col,
                        col,
                        rc,
                        theme::GLASS_RIM_LIGHT[3] - rc[3],
                    ),
                    None => p.alpha(a).rrect(r, h * 0.5, h * 0.5, col),
                }
            }
        };
        let TabGround::Plated { pop } = ground;
        // The SELECTION capsule never scales, on either strip. It marks which season you are
        // browsing, which is a fact about the page and not about the control under your thumb — and
        // a plate that dipped with the press would say the selection had moved when it had not.
        cap(
            &self.sel,
            theme::TAB_PLATE_SELECTED_OVER,
            None,
            1.0,
            // No cast: it marks a PAGE fact (which department you are browsing), not the control
            // under your thumb, and only the thing focus is actually on lifts off the page.
            false,
        );
        // The FOCUS capsule keeps its near-white fill: it is the one thing on this row that must not
        // read as a material, because the material is what everything else here is and focus has to
        // be the exception. On a TRACKED strip it wore two stops of shadow for a while — near-white
        // on a near-white LIGHT track has no separation and had to be lifted instead — and that
        // went with the light track it was drawn for: the capsule TRAVELLING under a row that
        // encloses it is already the whole focus mark, so `cast: false` below.
        //
        // On the PLATED strip it is additionally a control FACE, which the design system states in
        // one sentence (`components/chrome/TabStrip.jsx`): a season pill "sits bare on artwork and
        // provides its own ground, and because nothing encloses it its focused pill wears the
        // control face: edge-sheen, top hairline, the `--focus-scale-control` pop, and a Button's
        // press — dip on the way down, ring on release." **Owner correction, 2026-09-06: that list
        // was missing the fifth thing a control face wears, the CAST** — `Button::plate`'s own
        // `control_cast` before the fill, which is what turns a flat coloured shape sitting flush
        // against the artwork into something that reads as a pressable control lifted off it. This
        // pill sits bare on artwork exactly the way `linked_heading::Entry` sits bare on the page,
        // and it had the identical defect for the identical reason, so `cast: true` below.
        cap(&self.foc, crate::ui::ACCENT, Some(theme::CARD_SHEEN), pop, true);
    }
    /// The `(focus, selected)` mixes pill `pill` (content-space `(x, w)`) should ink itself with —
    /// hand straight to [`TabPill::mix`].
    pub(crate) fn mixes(&self, pill: (f32, f32)) -> (f32, f32) {
        (self.foc.mix(pill), self.sel.mix(pill))
    }
}

// ---- WCAG contrast, the measure every legibility grade in this file uses -----------------------

fn linearize(c: f32) -> f32 {
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
/// WCAG relative luminance of an rgba token (alpha ignored — a ground is opaque).
fn rel_luma(c: [f32; 4]) -> f32 {
    0.2126 * linearize(c[0]) + 0.7152 * linearize(c[1]) + 0.0722 * linearize(c[2])
}
/// WCAG contrast ratio between two opaque colours, brighter over darker.
fn contrast(a: [f32; 4], b: [f32; 4]) -> f32 {
    let (x, y) = (rel_luma(a), rel_luma(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The scroll stiffness of a strip that outgrows its row (the detail page's season tabs, the
/// Filmography filter); [`K_TAB_CAP`] moves the capsules at the same rate.
const K_TAB_SCROLL: f32 = 240.0;

// ---- The Litefin rail: the left icon rail every page but the player wears ---------------------
//
// `docs/litefin-look.md` 3 (modern-collapsed): a transparent 100px rail, the logo at the top, the
// nav items as one block centred between the logo header and the footer, the account avatar at
// the foot, and the item's NAME only as a tooltip pill beside the focused icon. The Libraries item
// opens a floating popover listing the server's libraries. Geometry and inks are `theme::sidebar`;
// what the items ARE is the application's (`app::chrome`), which hands this module captured
// entries and keys, never a Browse or session type.

/// The overshoot every rail motion shares — the squircle's 1.08 pop, the label pill and the
/// popover opening — Litefin's 250 ms `cubic-bezier(0.34,1.56,0.64,1)` (2.5): ~10% past the
/// target at ~150 ms. That is an underdamped spring at zeta .59 peaking at 150 ms, so
/// `k = (pi / (0.15 * sqrt(1 - zeta^2)))^2`.
const K_RAIL: f32 = 672.0;
const ZETA_RAIL: f32 = 0.59;

/// The rail's colour channels follow the same spring, CLAMPED, so the overshoot moves the scale and
/// never pushes a colour past the focused ink.
fn rail_amount(f: f32) -> f32 {
    f.clamp(0.0, 1.0)
}

/// Both colours of a fill blended, alpha included (`theme::mix` keeps the first alpha, and the
/// squircle fades from nothing).
fn lerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

/// One control the rail draws: a nav item or a popover row. Owned by the application's capture
/// (the label is measured and elided there), borrowed here for paint.
pub(crate) struct RailEntry {
    /// The outline mark, drawn idle.
    pub(crate) icon: crate::ui::icons::Icon,
    /// The filled mark, drawn on the current page's item and under focus.
    pub(crate) icon_on: crate::ui::icons::Icon,
    pub(crate) label: CString,
    pub(crate) label_w: f32,
}

/// The account avatar at the foot of the rail, captured by the application.
#[derive(Clone, Copy)]
pub(crate) struct AvatarRead<'a> {
    /// The raw id of the server `thumb` is a path on.
    pub(crate) src: u16,
    pub(crate) thumb: &'a str,
    /// The tooltip: the account's name, or what its menu offers when nobody is signed in.
    pub(crate) name: &'a CStr,
    pub(crate) name_w: f32,
}

/// The borrowed shared-chrome publication used by both its normal paint and the account menu's
/// scrim lift ([`redraw_rail_avatar`]): the avatar and how far focus has lit it.
#[derive(Clone, Copy)]
pub(crate) struct ChromeRead<'a> {
    pub(crate) avatar: AvatarRead<'a>,
    pub(crate) avatar_focus: f32,
}

/// What the rail draws this frame.
#[derive(Clone, Copy)]
pub(crate) struct RailRead<'a> {
    pub(crate) items: &'a [RailEntry],
    /// The current page's item.
    pub(crate) active: Option<usize>,
    pub(crate) avatar: AvatarRead<'a>,
    /// The Libraries popover's rows; empty while it is closed.
    pub(crate) rows: &'a [RailEntry],
    /// The library on screen, if it is one of the rows.
    pub(crate) row_active: Option<usize>,
    /// The item the popover hangs from.
    pub(crate) opener: usize,
}

/// Where the remote is on the rail: a slot of the column (the items, then the avatar at
/// `items.len()`), or a popover row.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct RailFocus {
    pub(crate) slot: Option<usize>,
    pub(crate) row: Option<usize>,
}

/// The logo, centred at x=50, y=60 (3).
pub(crate) const RAIL_LOGO: Rect = Rect::new(
    (theme::sidebar::RAIL_W - theme::sidebar::LOGO) * 0.5,
    theme::sidebar::PAD_Y + (theme::sidebar::HEADER_H - theme::sidebar::LOGO) * 0.5,
    theme::sidebar::LOGO,
    theme::sidebar::LOGO,
);
/// The footer's avatar item, pinned 20 above the bottom edge (3).
pub(crate) const RAIL_AVATAR: Rect = Rect::new(
    0.0,
    crate::ui::consts::SCR_H - theme::sidebar::PAD_Y - theme::sidebar::ITEM_H,
    theme::sidebar::RAIL_W,
    theme::sidebar::ITEM_H,
);
/// The band the item block is centred in: under the logo header, above the footer and its padding
/// (3: y 120..968).
const RAIL_BAND_TOP: f32 = theme::sidebar::PAD_Y + theme::sidebar::HEADER_H + theme::sidebar::HEADER_GAP;
const RAIL_BAND_BOTTOM: f32 = RAIL_AVATAR.y - theme::sidebar::PAD_Y;

/// Nav item `i` of `n`: a 100x72 box, the block vertically centred in its band.
pub(crate) fn rail_item_rect(i: usize, n: usize) -> Rect {
    let block = theme::sidebar::ITEM_H * n as f32;
    let top = RAIL_BAND_TOP + (RAIL_BAND_BOTTOM - RAIL_BAND_TOP - block) * 0.5;
    Rect::new(0.0, top + theme::sidebar::ITEM_H * i as f32, theme::sidebar::RAIL_W, theme::sidebar::ITEM_H)
}

/// Slot `slot` of a rail with `n` nav items: an item, or the avatar at `n` (the geometry tests'
/// name for what `app::chrome` publishes).
#[cfg(test)]
pub(crate) fn rail_slot_rect(slot: usize, n: usize) -> Rect {
    if slot < n { rail_item_rect(slot, n) } else { RAIL_AVATAR }
}

/// A box of `side` centred in `r`.
fn centred(r: Rect, side: f32) -> Rect {
    Rect::new(r.cx() - side * 0.5, r.cy() - side * 0.5, side, side)
}

/// The label pill beside a slot: its left edge at x=92, vertically centred on the item, padded
/// 8x18 round a label at most 300 wide (3).
pub(crate) fn rail_label_rect(slot: Rect, label_w: f32) -> Rect {
    use theme::sidebar::{LABEL_LINE_H, LABEL_MAX_W, LABEL_PAD_X, LABEL_PAD_Y, LABEL_X};
    let h = LABEL_LINE_H + 2.0 * LABEL_PAD_Y;
    let w = (label_w + 2.0 * LABEL_PAD_X).min(LABEL_MAX_W);
    Rect::new(LABEL_X, slot.cy() - h * 0.5, w, h)
}

/// The widest a label may be inside its pill, for the application's elision.
pub(crate) const RAIL_LABEL_TEXT_MAX: f32 = theme::sidebar::LABEL_MAX_W - 2.0 * theme::sidebar::LABEL_PAD_X;

/// The Libraries popover hanging from `opener`: left edge x=92, its top 6 above the item, kept on
/// screen (3, `Sidebar.js:_toggleFloatingLibraries`).
pub(crate) fn rail_popover_rect(opener: Rect, rows: usize) -> Rect {
    use theme::sidebar::{POPOVER_EDGE_BOTTOM, POPOVER_EDGE_TOP, POPOVER_NUDGE, POPOVER_PAD, POPOVER_ROW_H, POPOVER_W, POPOVER_X};
    let h = 2.0 * POPOVER_PAD + POPOVER_ROW_H * rows as f32;
    let max = (crate::ui::consts::SCR_H - h - POPOVER_EDGE_BOTTOM).max(POPOVER_EDGE_TOP);
    let y = (opener.y - POPOVER_NUDGE).clamp(POPOVER_EDGE_TOP, max);
    Rect::new(POPOVER_X, y, POPOVER_W, h)
}

/// Row `j` of a popover.
pub(crate) fn rail_row_rect(popover: Rect, j: usize) -> Rect {
    use theme::sidebar::{POPOVER_PAD, POPOVER_ROW_H};
    Rect::new(
        popover.x + POPOVER_PAD,
        popover.y + POPOVER_PAD + POPOVER_ROW_H * j as f32,
        popover.w - 2.0 * POPOVER_PAD,
        POPOVER_ROW_H,
    )
}

/// Where a row's name starts, and the widest it may be (for the application's elision).
const RAIL_ROW_TEXT_X: f32 = theme::sidebar::POPOVER_ROW_PAD_X + theme::sidebar::POPOVER_ICON + theme::sidebar::POPOVER_ICON_GAP;
pub(crate) const RAIL_ROW_TEXT_MAX: f32 = theme::sidebar::POPOVER_W - 2.0 * theme::sidebar::POPOVER_PAD
    - RAIL_ROW_TEXT_X - theme::sidebar::POPOVER_ROW_PAD_X;

/// Label and row text: 16.8px, the nearest rung (2.2 Sidebar focus tooltip).
pub(crate) const RAIL_TEXT: c_int = theme::size::lf::S17;

/// `r` scaled by `s` about the point (`ox`, `oy`).
fn scaled_about(r: Rect, s: f32, ox: f32, oy: f32) -> Rect {
    Rect::new(ox + (r.x - ox) * s, oy + (r.y - oy) * s, r.w * s, r.h * s)
}

/// **The rail's motion — owned by the application bridge, never a static.** One underdamped
/// spring per column slot (its focus: the squircle's pop, its colour and its label pill), one per
/// popover row, and the popover's opening. The logo texture rides along because the rail is its
/// only user. None of it is logical state: focus is the engine's, and these only follow it.
pub(crate) struct RailRender {
    slots: Vec<Spring>,
    rows: Vec<Spring>,
    open: Spring,
    logo: std::cell::Cell<u32>,
}

impl RailRender {
    pub(crate) fn new() -> Self {
        Self { slots: Vec::new(), rows: Vec::new(), open: Spring::at(0.0), logo: std::cell::Cell::new(0) }
    }

    /// Step every spring toward this frame's focus. `slots` counts the column (items + avatar),
    /// `rows` the open popover's rows (0 while closed).
    pub(crate) fn update(&mut self, slots: usize, rows: usize, focus: RailFocus, dt: f32) {
        self.slots.resize(slots, Spring::at(0.0));
        for (i, s) in self.slots.iter_mut().enumerate() {
            s.step_zeta(if focus.slot == Some(i) { 1.0 } else { 0.0 }, K_RAIL, ZETA_RAIL, dt);
        }
        if rows == 0 {
            // A closed popover leaves nothing to animate out (Litefin hides it at once); the next
            // opening starts from its own scale again.
            self.rows.clear();
            self.open.jump(0.0);
            return;
        }
        self.rows.resize(rows, Spring::at(0.0));
        for (j, s) in self.rows.iter_mut().enumerate() {
            s.step_zeta(if focus.row == Some(j) { 1.0 } else { 0.0 }, K_RAIL, ZETA_RAIL, dt);
        }
        self.open.step_zeta(1.0, K_RAIL, ZETA_RAIL, dt);
    }

    /// Slot `i`'s focus amount this frame, overshoot included.
    pub(crate) fn slot_focus(&self, i: usize) -> f32 {
        self.slots.get(i).map_or(0.0, |s| s.pos)
    }

    /// How far the popover has opened, overshoot included.
    #[cfg(test)]
    pub(crate) fn open_amount(&self) -> f32 {
        self.open.pos
    }

    fn logo_tex(&self) -> u32 {
        if self.logo.get() == 0 {
            // JellyRust's own app icon, not Litefin's logo (the fork's rule); 80px, drawn at 40.
            const ICON: &[u8] = include_bytes!("../../../pkg/icon.png");
            if let Some((w, h, px)) = nj_gfx::img::img_decode_owned(ICON) {
                self.logo.set(nj_gfx::gfx::upload_rgba(0, w as c_int, h as c_int, px.as_ptr()));
            }
        }
        self.logo.get()
    }

    /// Paint the rail: the logo, the item column, the avatar and, while it is open, the popover.
    /// The ground is transparent — the page shows through (3).
    pub(crate) fn draw(&self, p: Painter, read: RailRead<'_>) {
        let tex = self.logo_tex();
        if tex != 0 {
            // The icon is an app tile on its own dark ground, so it keeps a tile's corner.
            p.tex(tex, RAIL_LOGO, theme::radius::BASE, theme::TINT_WHITE);
        }
        let n = read.items.len();
        let open = !read.rows.is_empty();
        for (i, item) in read.items.iter().enumerate() {
            let active = read.active == Some(i) || (open && i == read.opener);
            draw_rail_item(p, rail_item_rect(i, n), item, active, self.slot_focus(i));
        }
        let avatar_f = self.slot_focus(n);
        draw_rail_avatar(p, read.avatar, avatar_f);
        let f = rail_amount(avatar_f);
        if f > 0.004 {
            draw_rail_label(p, RAIL_AVATAR, read.avatar.name, read.avatar.name_w, avatar_f);
        }
        if open {
            self.draw_popover(p, read);
        }
    }

    fn draw_popover(&self, p: Painter, read: RailRead<'_>) {
        use theme::sidebar as sb;
        let n = read.items.len();
        let panel = rail_popover_rect(rail_item_rect(read.opener, n), read.rows.len());
        let o = self.open.pos;
        let a = rail_amount(o);
        if a <= 0.004 {
            return;
        }
        // It grows out of the edge it hangs from: about its left side, level with its opener.
        let s = sb::POPOVER_OPEN_FROM + (1.0 - sb::POPOVER_OPEN_FROM) * o;
        let (ox, oy) = (panel.x, panel.cy());
        let p = p.alpha(a);
        let face = scaled_about(panel, s, ox, oy);
        p.rect_rimmed_w(face, sb::POPOVER_RAD * s, sb::POPOVER_FILL, sb::POPOVER_FILL, sb::POPOVER_RIM, 0.0, 1.0);
        for (j, row) in read.rows.iter().enumerate() {
            let g = self.rows.get(j).map_or(0.0, |s| s.pos);
            let gf = rail_amount(g);
            let active = read.row_active == Some(j);
            let at = scaled_about(rail_row_rect(panel, j), s, ox, oy);
            let r = at.scaled(1.0 + (sb::POPOVER_FOCUS_SCALE - 1.0) * g);
            let rest = if active { sb::POPOVER_ROW_ACTIVE } else { [sb::POPOVER_ROW_FOCUS[0], sb::POPOVER_ROW_FOCUS[1], sb::POPOVER_ROW_FOCUS[2], 0.0] };
            let fill = lerp4(rest, sb::POPOVER_ROW_FOCUS, gf);
            if fill[3] > 0.004 {
                p.rrect(r, sb::POPOVER_ROW_RAD, sb::POPOVER_ROW_RAD, fill);
            }
            let ink = lerp4(sb::POPOVER_INK, sb::POPOVER_INK_FOCUS, gf);
            let icon = Rect::new(r.x + sb::POPOVER_ROW_PAD_X, r.cy() - sb::POPOVER_ICON * 0.5, sb::POPOVER_ICON, sb::POPOVER_ICON);
            crate::ui::icons::draw(p, if active || gf > 0.5 { row.icon_on } else { row.icon }, icon, ink);
            let ty = nj_gfx::text::text_vcenter_y(RAIL_TEXT, active as c_int, r.cy());
            p.text(row.label.as_ptr(), r.x + RAIL_ROW_TEXT_X, ty, RAIL_TEXT, ink, 0, active as c_int);
        }
    }
}

impl Default for RailRender {
    fn default() -> Self {
        Self::new()
    }
}

/// One nav item: idle no squircle and a grey outline mark; active a white .14 squircle and the
/// filled mark in white; focused the accent squircle, the filled mark in lavender, both popped
/// 1.08, and the label pill beside it (3 States).
fn draw_rail_item(p: Painter, slot: Rect, item: &RailEntry, active: bool, f: f32) {
    draw_rail_mark(p, slot, item.icon, item.icon_on, active, f);
    if rail_amount(f) > 0.004 {
        draw_rail_label(p, slot, &item.label, item.label_w, f);
    }
}

/// A slot's squircle and mark, in its idle, active or focused state.
fn draw_rail_mark(p: Painter, slot: Rect, icon: crate::ui::icons::Icon, icon_on: crate::ui::icons::Icon, active: bool, f: f32) {
    use theme::sidebar as sb;
    let fc = rail_amount(f);
    let s = 1.0 + (sb::FOCUS_SCALE - 1.0) * f;
    let clear = |c: [f32; 4]| [c[0], c[1], c[2], 0.0];
    let (rest_fill, rest_rim) = if active {
        (sb::SQUIRCLE_ACTIVE, sb::SQUIRCLE_ACTIVE_RIM)
    } else {
        (clear(sb::SQUIRCLE_FOCUS), clear(sb::SQUIRCLE_FOCUS_RIM))
    };
    let fill = lerp4(rest_fill, sb::SQUIRCLE_FOCUS, fc);
    if fill[3] > 0.004 {
        let sq = centred(slot, sb::SQUIRCLE).scaled(s);
        let rim = lerp4(rest_rim, sb::SQUIRCLE_FOCUS_RIM, fc);
        p.rect_rimmed_w(sq, sb::SQUIRCLE_RAD * s, fill, fill, rim, 0.0, 1.0);
    }
    let rest_ink = if active { sb::ICON_ACTIVE } else { sb::ICON_IDLE };
    let ink = lerp4(rest_ink, sb::ICON_FOCUS, fc);
    let mark = if active || fc > 0.5 { icon_on } else { icon };
    crate::ui::icons::draw(p, mark, centred(slot, sb::ICON).scaled(s), ink);
}

/// The focused slot's name: an accent pill at x=92, growing from 0.9 and fading in with focus.
fn draw_rail_label(p: Painter, slot: Rect, label: &CStr, label_w: f32, f: f32) {
    use theme::sidebar as sb;
    let a = rail_amount(f);
    let rest = rail_label_rect(slot, label_w);
    let s = sb::LABEL_OPEN_FROM + (1.0 - sb::LABEL_OPEN_FROM) * f;
    let pill = scaled_about(rest, s, rest.x, rest.cy());
    let p = p.alpha(a);
    p.rect_rimmed_w(pill, sb::LABEL_RAD * s, sb::LABEL_FILL, sb::LABEL_FILL, sb::LABEL_RIM, 0.0, 1.0);
    let ty = nj_gfx::text::text_vcenter_y(RAIL_TEXT, 1, pill.cy());
    p.text(label.as_ptr(), pill.x + sb::LABEL_PAD_X * s, ty, RAIL_TEXT, sb::LABEL_INK, 0, 1);
}

/// The account at the foot of the rail: the user's picture as a 40px circle with a 2px lavender
/// ring on focus, or — with no picture — the account mark, styled like any nav item (3,
/// `Sidebar.js:_renderUserAvatar`).
fn draw_rail_avatar(p: Painter, data: AvatarRead<'_>, f: f32) {
    use theme::sidebar as sb;
    let r = centred(RAIL_AVATAR, sb::AVATAR);
    let d = r.w;
    let (t, tw, th) = if data.thumb.is_empty() { (0, 0.0, 0.0) } else { resolve_tex_wh_on(data.src, data.thumb, 128, 128, 0) };
    if t == 0 {
        draw_rail_mark(p, RAIL_AVATAR, crate::ui::icons::Icon::Account, crate::ui::icons::Icon::AccountFill, false, f);
        return;
    }
    p.rrect(r, d * 0.5, d * 0.5, sb::AVATAR_FILL);
    p.tex_uv(t, r.cover_uv(tw, th, crate::ui::Crop::Centre), r, d * 0.5, theme::TINT_WHITE);
    let a = rail_amount(f);
    if a > 0.004 {
        let w = sb::AVATAR_RING_W;
        let ring = Rect::new(r.x - w, r.y - w, d + 2.0 * w, d + 2.0 * w);
        p.alpha(a).rring(ring, ring.w * 0.5, w, theme::FOCUS_BORDER);
    }
}

/// The global clock, top 32 / right 50, 24 h `HH:MM` (4.2). The application formats the text.
pub(crate) fn draw_clock(p: Painter, text: &CStr) {
    use theme::sidebar as sb;
    let sz = theme::size::lf::S22;
    let ty = nj_gfx::text::text_vcenter_y(sz, 0, sb::CLOCK_TOP + sb::CLOCK_LINE_H * 0.5);
    p.text(text.as_ptr(), crate::ui::consts::SCR_W - sb::CLOCK_RIGHT, ty, sz, sb::CLOCK_INK, 2, 0);
}

/// `HH:MM` for a minute of the day, zero padded, 24 h (`player:timeFormat=24h`, 4.2).
pub(crate) fn clock_text(minute_of_day: u32) -> String {
    let m = minute_of_day % (24 * 60);
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// [`ChromeRead`]'s avatar, re-drawn over the account menu's scrim — the `Scrim::lift` contract
/// (`ui::screen::Scrim`, drawn by `ModalStack::draw_scrims`) for the one surface that hangs off a
/// piece of shared chrome: the menu opened from the avatar keeps the avatar out of its own dim.
/// On `nav::chrome_alpha`, the alpha the rail is drawn on.
pub(crate) fn redraw_rail_avatar(read: crate::ui::screen::ScrimLiftRead<'_>) {
    let Some(chrome) = read.chrome else { return };
    crate::ui::guard(|| draw_rail_avatar(
        Painter::root().alpha(crate::ui::nav::chrome_alpha()),
        chrome.avatar,
        chrome.avatar_focus,
    ));
}

// ---- PageError: Litefin's `.page-error` box, a page's "nothing to show" ------------------------
// A rimmed box (1px border, 16 padding, corner `--jf-border-radius`) with one line centred in it at
// 17.6px on a 1.5 line (`styles/components.css`). The caller places it; the box is one line tall.
pub struct PageError<'a> {
    pub frame: Rect,
    pub text: &'a CStr,
}

impl PageError<'_> {
    /// The text's rung (17.6px).
    pub const TEXT: c_int = theme::size::lf::S18;
    /// Border plus padding, on every side.
    pub const INSET: f32 = 1.0 + 16.0;
    /// The box's height: the inset round one 26.4px line.
    pub const H: f32 = 2.0 * Self::INSET + 26.4;
}

impl View for PageError<'_> {
    fn draw(&self, _e: &Env, p: Painter) {
        use theme::page_error::{FILL, INK};
        p.rect_rimmed_w(self.frame, theme::radius::BASE, FILL, FILL, INK, 0.0, 1.0);
        Label::new(self.text.as_ptr(), Self::TEXT, INK).h(HAlign::Center).v(VAlign::Middle).draw(p, self.frame);
    }
}

/// Which colour treatment a control (Button / CircleButton) wears. One control widget, four looks —
/// the focus-driven default (every pill and disc in the app, the hero Play button included), a
/// caller-coloured one-off, the keyline pill for a secondary action over video, and the
/// destructive face.
///
/// There used to be a fourth, `Primary`: an always-filled cool-white CTA. It went with
/// `theme::FILL_PRIMARY` in the 2026-08-13 palette sync — **nothing is filled by rank, only by
/// focus**, so a control that lights up while the remote is elsewhere is a lie about where you are,
/// and at ten feet its white was indistinguishable from [`theme::ACCENT`] anyway. It had no callers
/// by then; the hero Play pill has been `Accent` for months.
#[derive(Clone, Copy)]
pub enum ControlStyle {
    /// Focus-driven: on a keyed page both states take the local [`ControlPalette`] hue (a bright
    /// two-stop focused face and a dark idle face); over video focus is flat ACCENT and idle is a
    /// white film. Dark ink on focus, white ink at rest in both cases.
    Accent,
    /// caller supplies the exact fill + ink.
    Custom { fill: [f32; 4], ink: [f32; 4] },
    /// The **keyline** pill — idle, a hairline outline ([`theme::PILL_KEYLINE`]) knocked out over
    /// a translucent near-black interior ([`theme::PILL_KEYLINE_BG`]), for a secondary action
    /// sitting on SCRIMMED VIDEO, where the KEYED [`Accent`] idle plate reads as a hole in the
    /// picture. Focused it takes the standard Accent treatment, so focus reads identically across
    /// every control in the family.
    ///
    /// **It has no caller, and the problem it was built for is now solved one level up.** Its one
    /// user was the post-play card's *Watch credits*, which lost it at `123cfd89` when that card
    /// was rebuilt as a corner tile; the doc line naming that call site outlived it. The hole in
    /// the picture is what [`ControlGround::Unkeyed`] answers, and it answers it the other way
    /// round — a light FILM over the HUD's ramp rather than a darker plate cut into the frame — for
    /// every control on that ground rather than for one style a call site has to remember to ask
    /// for. Kept because it is the one construction in this family that draws a stroke, and
    /// deleting it would take `Button::plate`'s knockout branch with it; reach for the ground, not
    /// for this, when a control lands on video.
    Keyline,
    /// The **destructive** face — the control that ends something (a decision alert's destructive
    /// answer, e.g. consent's *Delete all local data*).
    ///
    /// Idle it states the hue TWICE: the plate is [`Accent`]'s local idle face carrying
    /// [`theme::DANGER_IDLE_TINT`] of [`theme::DANGER`] (the static fallback is
    /// [`theme::CONTROL_DANGER_IDLE_FILL`]) and
    /// the label is drawn in the danger ink itself. That is not belt-and-braces — at ten feet a
    /// 16% tint on a dark plate is a shade of grey, and the point of the pair is that the action
    /// is NAMED as destructive before the remote ever reaches it. Focused, the hue takes the whole
    /// face: [`theme::DANGER`] fill under [`theme::TEXT_PRIMARY`] ink.
    ///
    /// **There is no keyline, and that is a decision rather than an omission.** A danger-tinted
    /// perimeter was a third signal saying what the plate and the label already say, and the
    /// perimeter sheen every control wears ([`control_rim`]) is a card CONSTANT, not a state — a
    /// style that made it a state would be the one control in the app whose edge means something.
    ///
    /// The invariant to preserve: **the colour is a property of the ACTION, the FILL is still a
    /// property of FOCUS.** Every other control in this family lights up for exactly one reason,
    /// and this one must not become a button that is filled by rank (the reason `Primary` went).
    Danger,
    /// **Litefin's action control** (`docs/litefin-look.md` 5 Actions row, 2.6 Primary buttons):
    /// idle a [`theme::ACTION_FILL`] film inside a [`theme::ACTION_BORDER_W`] [`theme::ACTION_BORDER`]
    /// ring under a white glyph, focused the accent [`theme::BRAND_FILL`] under a
    /// [`theme::BRAND_INK`] glyph with no ring. `active` is a toggle that is ON (watched,
    /// favourite): its idle glyph is the accent instead ([`theme::BRAND_ACTIVE_INK`]).
    ///
    /// Flat on purpose: Litefin's focus is fill plus scale, never a sheen, a glow or a cast (2.6),
    /// so this face skips [`control_rim`] and [`control_cast`] and ignores the sampled palette and
    /// the ground. A square frame draws the 72px circle, a wide one the Resume pill; the glyph is
    /// [`ACTION_ICON`] whatever the label size.
    Action { active: bool },
}
/// WHICH GROUND a control is standing on — the second axis beside [`ControlStyle`], and the one
/// thing about a control that the widget itself cannot work out.
///
/// Everything [`ControlStyle`] does assumes the ground can be SAMPLED: the page drew its own
/// artwork, so a control knows what it is sitting on and can be solved against it. The player
/// cannot. The picture lives on the hardware VIDEO PLANE — it never enters our framebuffer, no
/// scrim of ours can dim it, and it is a different image every frame — so nothing about the ground
/// under a HUD control is knowable at draw time except one fact the HUD itself supplies: it laid a
/// black ramp over the video first, so that ground is DARK.
///
/// The design system states this as a CSS scope (`[data-ground="unkeyed"]`, `tokens/colors.css`):
/// a surface standing on video sets it and every control inside switches. There is no scope to
/// inherit through here, so it rides the widget as a builder — [`Button::ground`],
/// [`CircleButton::ground`], [`TransportButton::ground`] — and the player HUD is the only caller
/// that sets it.
///
/// It changes the parts that require KNOWING the ground: `Keyed` accepts a [`ControlPalette`] for
/// both face levels, while `Unkeyed` drops that palette, uses the HUD's light film and stronger
/// idle edge at rest, flattens focus back to [`theme::ACCENT`] and raises its rim to pure white.
/// Focus still means the same
/// thing on both grounds — bright face, dark ink, pop and cast — but video cannot borrow a hue from
/// pixels this process never sees.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ControlGround {
    /// The app's own pages — a ground we drew and can therefore answer to. Every control that is
    /// not in the player HUD, and the default.
    Keyed,
    /// The VIDEO PLANE, under the HUD's own ramp: ambient tint is disabled, idle becomes the light
    /// film ([`theme::CONTROL_IDLE_FILL_UNKEYED`]) with its visible idle edge
    /// ([`theme::CONTROL_RIM_IDLE_UNKEYED`]); focus is flat [`theme::ACCENT`] and its edge is the
    /// pure-white line ([`theme::CONTROL_RIM_FOCUS_UNKEYED`]).
    Unkeyed,
}

/// The colours a keyed control derives from its page's local ground sample.
///
/// Home and Detail read the pixels they already rendered under the row — artwork AFTER its scrims,
/// not the PMS artwork envelope. The sample's LIGHTNESS never reaches the control: it contributes
/// only OKLCH hue and a fraction of chroma. That is the difference between a white object catching
/// the scene's light and a white chip averaged toward a dark photograph (which turns muddy). A
/// screen computes this once for its control row and copies it into every face; the player may
/// receive one accidentally, but [`ControlGround::Unkeyed`] ignores it by construction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlPalette {
    focus_top: [f32; 4],
    focus_body: [f32; 4],
    idle: [f32; 4],
    spent: [f32; 4],
}

impl Default for ControlPalette {
    fn default() -> Self {
        Self::ambient([
            theme::SURFACE_APP[0],
            theme::SURFACE_APP[1],
            theme::SURFACE_APP[2],
        ])
    }
}

impl ControlPalette {
    /// Resolve the design system's local `--ambient-key` into the focused top/body, idle face and
    /// countdown-spent roles. `key` is display-encoded sRGB, matching the renderer's plain 888
    /// framebuffer.
    pub fn ambient(key: [f32; 3]) -> Self {
        let lab = srgb_to_oklab([key[0], key[1], key[2], 1.0]);
        let chroma = lab[1].hypot(lab[2]);
        let (ha, hb) = if chroma > 1e-6 {
            (lab[1] / chroma, lab[2] / chroma)
        } else {
            (0.0, 0.0)
        };
        let keyed = |l: f32, scale: f32, alpha: f32| {
            oklab_to_srgb([l, ha * chroma * scale, hb * chroma * scale], alpha)
        };
        let focus_top = keyed(
            theme::CONTROL_FOCUS_FACE_L,
            theme::CONTROL_FOCUS_AMBIENT_C,
            1.0,
        );
        let focus_body = keyed(
            theme::CONTROL_FOCUS_FACE_L - theme::CONTROL_FOCUS_BODY_STEP,
            theme::CONTROL_FOCUS_AMBIENT_C,
            1.0,
        );
        let idle = keyed(
            theme::CONTROL_IDLE_FACE_L,
            theme::CONTROL_IDLE_AMBIENT_C,
            theme::CONTROL_IDLE_FACE_A,
        );
        let spent = theme::mix(
            focus_top,
            theme::SURFACE_APP,
            1.0 - theme::CONTROL_SPENT_FOCUS_W,
        );
        Self {
            focus_top,
            focus_body,
            idle,
            spent,
        }
    }

    fn focus_face(self) -> ([f32; 4], [f32; 4]) {
        (self.focus_top, self.focus_body)
    }
}

/// sRGB → OKLab, following the CSS Color 4 matrices used by relative `oklch(from …)` colors.
fn srgb_to_oklab(c: [f32; 4]) -> [f32; 3] {
    let lin = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (lin(c[0]), lin(c[1]), lin(c[2]));
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// OKLab → display-encoded sRGB. If the requested tint falls outside sRGB, reduce CHROMA along the
/// same hue until it fits instead of clipping a channel: clipping would also move the authored
/// lightness, precisely the part of the focus contract artwork is not allowed to change.
fn oklab_to_srgb(c: [f32; 3], alpha: f32) -> [f32; 4] {
    let linear = |lab: [f32; 3]| {
        let l = lab[0] + 0.396_337_78 * lab[1] + 0.215_803_76 * lab[2];
        let m = lab[0] - 0.105_561_346 * lab[1] - 0.063_854_17 * lab[2];
        let s = lab[0] - 0.089_484_18 * lab[1] - 1.291_485_5 * lab[2];
        let (l, m, s) = (l * l * l, m * m * m, s * s * s);
        [
            4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
            -1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s,
            -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
        ]
    };
    let in_gamut = |rgb: [f32; 3]| rgb.iter().all(|v| *v >= 0.0 && *v <= 1.0);
    let mut rgb = linear(c);
    if !in_gamut(rgb) {
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..12 {
            let k = (lo + hi) * 0.5;
            let candidate = linear([c[0], c[1] * k, c[2] * k]);
            if in_gamut(candidate) {
                lo = k;
                rgb = candidate;
            } else {
                hi = k;
            }
        }
    }
    let encode = |v: f32| {
        (if v <= 0.003_130_8 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        })
        .clamp(0.0, 1.0)
    };
    [encode(rgb[0]), encode(rgb[1]), encode(rgb[2]), alpha]
}

impl ControlGround {
    /// The idle control face on this ground. The ONE place the two fills are chosen between, so a
    /// widget can never hand-pick the wrong one.
    fn idle_fill(self) -> [f32; 4] {
        match self {
            ControlGround::Keyed => theme::CONTROL_IDLE_FILL,
            ControlGround::Unkeyed => theme::CONTROL_IDLE_FILL_UNKEYED,
        }
    }
}

#[derive(Clone, Copy)]
struct ControlFace {
    top: [f32; 4],
    body: [f32; 4],
    ink: [f32; 4],
    spent: [f32; 4],
}

/// A control's EDGE — the 1px perimeter the design system has always asked every control to wear
/// and which none of them wore.
///
/// **It is a rim and nothing else: no blur, no region, no budget.** A glass FACE was built and
/// looked at on the panel first, and it is not worth its price here — the app's controls sit in the
/// darkest quarter of the frame by construction (the hero wedge is what buys their labels their
/// contrast), so there is nothing behind them to bend. Measured on the dev set's own Home: the
/// blurred face moves a control's pixels by at most 7/255 while this rim moves them by up to 74.
/// Dropping the backdrop also drops the thing that made controls unaffordable — a glass surface is
/// charged for its rectangle unioned with every other, and Home's action row unioned with a glass
/// tab track priced at 3.8x the frame budget. A rim costs one extra fragment op on the perimeter.
///
/// **The weights are the TILE's, plus the container's lamp**: [`theme::CARD_SHEEN`] .22 round the
/// whole perimeter — a card's own edge, because a control face has a card's problem, it sits over
/// artwork nobody chose — and the boost to [`theme::GLASS_RIM_LIGHT`] .28 on the side facing the
/// light, weighted by the surface normal, so a disc's highlight sits on its crown and fades to
/// nothing at its equator. One lamp, the one every card shadow in this app is already cast from.
///
/// The perimeter used to be [`theme::GLASS_RIM`] .14, the GLASS container's line, which was a
/// category error by the design system's own rule: `tokens/glass.css` scopes that pair to a surface
/// you read THROUGH and says in the same breath that every control is flat. A flat control wears the
/// card constant. The crown stays the glass hairline because that is what it is — a specular line
/// from the shared lamp, not a second perimeter — so the edge now runs .22 → .28 instead of
/// .14 → .28, and this rim is the ONLY thing separating a control from its ground since the drop
/// shadow went (`Button::draw`).
///
/// Unconditional on focus, which is the point: the design's rule is that the card constants are not
/// states and the FILL is what focus owns. On the focused near-white `ACCENT` face a white rim is
/// invisible by construction, which is the sign it is an edge and not a fill.
///
/// **[`ControlGround::Unkeyed`] is the one exception, and it is the ground asking, not the state.**
/// Over the video plane both states use the 1.25px unkeyed geometry. Idle takes
/// [`theme::CONTROL_RIM_IDLE_UNKEYED`] because the keyed 1px/.22 card edge disappeared between the
/// light film and the panel's video composition; focus raises that same geometry to
/// [`theme::CONTROL_RIM_FOCUS_UNKEYED`] — pure white, with no top boost because nothing is brighter
/// than white. Keyed controls keep the same card edge in both states.
fn control_rim(
    p: Painter,
    r: Rect,
    rad: f32,
    top_face: [f32; 4],
    body_face: [f32; 4],
    focused: bool,
    ground: ControlGround,
) {
    let (rim, top, w) = control_rim_spec(focused, ground);
    // the ÉTLIV — light spilling inward off that line. It belongs to the one edge that is a STATE,
    // so it arrives and leaves with it.
    let glow = (focused && ground == ControlGround::Unkeyed)
        .then_some(theme::CONTROL_RIM_FOCUS_UNKEYED_GLOW);
    p.face_rimmed(
        r,
        rad,
        face_outline(r).as_ref(),
        top_face,
        body_face,
        rim,
        top,
        w,
        glow,
    );
}

/// **The outline a control face of this shape draws to** — the blended capsule
/// ([`crate::ui::pill`]) for anything wider than it is tall, and `None` for a DISC, which this
/// design keeps a circle.
///
/// The test is the SHAPE and not the widget, which is what makes an unfurled [`CircleButton`] come
/// out right for free: at rest it is a circle and takes none of this, and as it opens into a capsule
/// carrying its verb it becomes the same object a [`Button`] is. `None` is also the honest answer
/// for a box that cannot carry the outline at all — the caller has already been handed the stadium
/// radius, which is the design's own fallback.
fn face_outline(r: Rect) -> Option<crate::ui::pill::Pill> {
    (r.w > r.h + 0.5)
        .then(|| crate::ui::pill::solve(r.w, r.h))
        .flatten()
}

/// **The BOX a control face is drawn in, which is not the frame it was laid out with.**
///
/// A capsule's end circle is under half its box ([`theme::PILL_END_R`]), so a box the size of the
/// frame would draw its ends smaller than the control's stated height and a capsule would stop
/// matching a disc beside it. The box carries that deficit instead: every layout number in the app
/// stays exactly what it was, the ends come out at the frame's own height, and the only thing that
/// leaves the frame is the middle of the top and bottom edges — by a quarter of a pixel at this
/// app's control sizes ([`crate::ui::pill`]'s note has the measurement). A DISC is returned
/// untouched.
fn face_box(r: Rect) -> Rect {
    if r.w <= r.h + 0.5 {
        return r;
    }
    let h = crate::ui::pill::box_h(r.h);
    Rect::new(r.x, r.y - (h - r.h) * 0.5, r.w, h)
}

/// **THE FOCUS CAST** — the soft lift a control face takes once the remote is on it, and nothing it
/// wears at rest.
///
/// This file argued for a long time that a control face casts nothing at all, and half of that
/// argument stands: it is held by its EDGE, and two elevations for one control family is what the
/// design system removed. What it got wrong is that FOCUS is an elevation. A fill alone cannot say
/// "the remote is here" over a ground nobody chose — the measured case is an `ACCENT` capsule over
/// a white frame at ~1.2:1 — and the design's answer is [`theme::CONTROL_CAST_FOCUS`], two stops of
/// black under the focused face only. Drawn to the stadium radius rather than to the solved
/// capsule: at a 32px blur the half pixel between the two outlines is far below anything the
/// penumbra resolves.
fn control_cast(p: Painter, r: Rect, rad: f32) {
    for (dy, blur, a) in theme::CONTROL_CAST_FOCUS {
        p.shadow(r, rad, blur, dy, theme::with_a(theme::CARD_SHADOW, a));
    }
}

/// [`control_rim`]'s decision, as `(colour, extra weight on the up-facing edge, stroke width)` and
/// with no painter in it — the one place the two edges are chosen between, so a test can grade the
/// choice without a GL context (nothing on the host draws a pixel).
fn control_rim_spec(focused: bool, ground: ControlGround) -> ([f32; 4], f32, f32) {
    if ground == ControlGround::Unkeyed {
        if focused {
            // No top boost: the perimeter is already pure white and there is nothing brighter to
            // crown it with — the design's two soft inner glows carry its volume inward instead.
            return (
                theme::CONTROL_RIM_FOCUS_UNKEYED,
                0.0,
                theme::CONTROL_RIM_FOCUS_UNKEYED_W,
            );
        }
        // Preserve the shared lamp's .06 crown step while giving the film enough base edge to
        // survive the TV's separate-plane video composition.
        return (
            theme::CONTROL_RIM_IDLE_UNKEYED,
            theme::GLASS_RIM_LIGHT[3] - theme::CARD_SHEEN[3],
            theme::CONTROL_RIM_FOCUS_UNKEYED_W,
        );
    }
    (
        theme::CARD_SHEEN,
        theme::GLASS_RIM_LIGHT[3] - theme::CARD_SHEEN[3],
        theme::CARD_SHEEN_W,
    )
}

impl ControlStyle {
    /// Full face material for this style and ground. A keyed Accent has two focused stops and a
    /// palette-derived idle stop; an unkeyed Accent deliberately flattens back to the fixed video
    /// treatment because no colour can be sampled from that plane.
    fn face(self, focused: bool, ground: ControlGround, palette: ControlPalette) -> ControlFace {
        let flat = |fill, ink, spent| ControlFace {
            top: fill,
            body: fill,
            ink,
            spent,
        };
        match self {
            ControlStyle::Accent if ground == ControlGround::Unkeyed && focused => {
                flat(theme::ACCENT, theme::ACCENT_INK, theme::CONTROL_SPENT_FILL)
            }
            ControlStyle::Accent if ground == ControlGround::Unkeyed => flat(
                theme::CONTROL_IDLE_FILL_UNKEYED,
                theme::CONTROL_IDLE_INK,
                theme::CONTROL_SPENT_FILL,
            ),
            ControlStyle::Accent if focused => ControlFace {
                top: palette.focus_top,
                body: palette.focus_body,
                ink: theme::ACCENT_INK,
                spent: palette.spent,
            },
            ControlStyle::Accent => flat(palette.idle, theme::CONTROL_IDLE_INK, palette.spent),
            ControlStyle::Custom { fill, ink } => flat(fill, ink, theme::CONTROL_SPENT_FILL),
            ControlStyle::Keyline if focused && ground == ControlGround::Keyed => ControlFace {
                top: palette.focus_top,
                body: palette.focus_body,
                ink: theme::ACCENT_INK,
                spent: palette.spent,
            },
            ControlStyle::Keyline if focused => {
                flat(theme::ACCENT, theme::ACCENT_INK, theme::CONTROL_SPENT_FILL)
            }
            ControlStyle::Keyline => flat(
                theme::PILL_KEYLINE_BG,
                theme::TEXT_HEADING,
                theme::CONTROL_SPENT_FILL,
            ),
            ControlStyle::Danger if focused => {
                flat(theme::DANGER, theme::TEXT_PRIMARY, theme::DANGER)
            }
            ControlStyle::Danger => {
                let idle = if ground == ControlGround::Keyed {
                    theme::mix(palette.idle, theme::DANGER, theme::DANGER_IDLE_TINT)
                } else {
                    theme::CONTROL_DANGER_IDLE_FILL
                };
                flat(idle, theme::CONTROL_DANGER_IDLE_INK, theme::DANGER)
            }
            ControlStyle::Action { .. } if focused => {
                flat(theme::BRAND_FILL, theme::BRAND_INK, theme::BRAND_FILL_ACTIVE)
            }
            ControlStyle::Action { active } => flat(
                theme::ACTION_FILL,
                if active { theme::BRAND_ACTIVE_INK } else { theme::CONTROL_IDLE_INK },
                theme::BRAND_FILL_ACTIVE,
            ),
        }
    }

    /// Compatibility projection of the default shelf palette to one `(top fill, ink)` pair. New
    /// painting code uses [`ControlStyle::face`] so it cannot silently discard the focused body's
    /// second stop; this remains for pure state tests and callers that genuinely need one colour.
    pub(crate) fn colors(self, focused: bool, ground: ControlGround) -> ([f32; 4], [f32; 4]) {
        let f = self.face(focused, ground, ControlPalette::default());
        (f.top, f.ink)
    }
}

// ---- Button: a pill with a label, an optional leading icon and an optional TRAILING accessory,
// centered together as one group (icon + gap + label + gap + accessory is centered in the pill).
// Colour per `ControlStyle` (default Accent). The one reusable action button — hero Play (Primary),
// detail/info actions (Accent), etc. ----
pub struct Button {
    pub frame: Rect,
    pub label: *const c_char,
    pub sz: c_int,
    pub icon: Option<crate::ui::icons::Icon>,
    /// The TRAILING accessory glyph — a chevron saying the press opens a list rather than acting
    /// ([`crate::screens::alt_sources`]'s *Also available*). Deliberately its own slot rather than a
    /// second use of [`Button::icon`]: the leading icon is part of the label's own statement (the
    /// Play triangle IS "play"), while this one is a disclosure mark about what the control DOES,
    /// and the two are read in opposite directions. It is the same `›`-family mark
    /// [`crate::ui::table::Row::ticon`] puts at a row's trailing edge, for the same reason.
    pub trailing: Option<crate::ui::icons::Icon>,
    pub focused: bool,
    pub style: ControlStyle,
    /// WHICH GROUND this pill stands on — see [`ControlGround`]. [`Keyed`](ControlGround::Keyed)
    /// unless the caller says otherwise, which is every screen but the player HUD.
    pub ground: ControlGround,
    /// Local page palette. Hero rows publish one from their artwork; flat pages keep the default
    /// shelf key and the video ground ignores it.
    pub palette: ControlPalette,
    /// The FOCUS POP, as a factor on the frame — see [`Button::scale`].
    pub scale: f32,
    /// 0..1 left-to-right FILL sweep across the pill; None = an ordinary button.
    pub progress: Option<f32>,
}
/// [`Button`]'s icon box, as a multiple of its type size, and the icon→label gap. Named because
/// [`Button::pill_w`] measures the same run `Button::draw` lays out — a literal in each would let
/// the two drift.
const BTN_ICON_RATIO: f32 = 1.15;
const BTN_ICON_GAP: f32 = 12.0;
/// Total horizontal air a pill carries around its icon+label run.
pub(crate) const BTN_PILL_AIR: f32 = 68.0;
/// [`ControlStyle::Keyline`]'s stroke width (the design's 1.5 — same weight as [`keyline_chip`]'s).
const BTN_KEYLINE_W: f32 = 1.5;

/// Litefin's action control geometry (`docs/litefin-look.md` 5 Actions row): 72px circles 16
/// apart, a 28px glyph, and the Resume pill's 22px side padding.
pub(crate) const ACTION_D: f32 = 72.0;
pub(crate) const ACTION_GAP: f32 = 16.0;
pub(crate) const ACTION_ICON: f32 = 28.0;
pub(crate) const ACTION_PILL_PAD: f32 = 22.0;
/// The focus pop: 1.15 for an icon circle, 1.05 for Play and the Resume pill (5 Actions row).
pub(crate) const ACTION_FOCUS_SCALE: f32 = 1.15;
pub(crate) const ACTION_PLAY_FOCUS_SCALE: f32 = 1.05;

/// [`CtlPop::scale`]'s live value (1 at rest, [`CTRL_FOCUS_SCALE`] focused, below 1 in the press
/// dip) carried onto an action control's own peak, so the shared spring and press dip drive
/// Litefin's larger pop without a second spring.
pub(crate) fn action_scale(pop: f32, peak: f32) -> f32 {
    1.0 + (pop - 1.0) * (peak - 1.0) / (CTRL_FOCUS_SCALE - 1.0)
}

/// The Resume pill's width for a label `label_w` wide: padding, glyph, gap, label, padding.
pub(crate) fn action_pill_w(label_w: f32) -> f32 {
    2.0 * ACTION_PILL_PAD + ACTION_ICON + BTN_ICON_GAP + label_w
}

impl Button {
    pub fn new(label: *const c_char, sz: c_int, frame: Rect) -> Self {
        Self {
            frame,
            label,
            sz,
            icon: None,
            trailing: None,
            focused: false,
            style: ControlStyle::Accent,
            ground: ControlGround::Keyed,
            palette: ControlPalette::default(),
            scale: 1.0,
            progress: None,
        }
    }
    /// The [FOCUS POP](CTRL_FOCUS_SCALE), about the capsule's centre — normally [`CtlPop::scale`],
    /// which already folds the press dip in. `1.0` is the resting control.
    ///
    /// The pill's TYPE does not scale with it, for the reason [`CircleButton::scale`] gives: the
    /// size ladder has no rung between 28 and 32 and text may not sit between rungs. So the plate
    /// and its icon grow and the label keeps its measured width, which is also what keeps the
    /// centred `[icon + gap + label]` run centred through the pop.
    pub fn scale(mut self, s: f32) -> Self {
        self.scale = s;
        self
    }
    pub fn icon(mut self, i: crate::ui::icons::Icon) -> Self {
        self.icon = Some(i);
        self
    }
    /// Give this pill a trailing accessory — see [`Button::trailing`].
    pub fn trailing_icon(mut self, i: crate::ui::icons::Icon) -> Self {
        self.trailing = Some(i);
        self
    }
    pub fn focused(mut self, f: bool) -> Self {
        self.focused = f;
        self
    }
    pub fn style(mut self, s: ControlStyle) -> Self {
        self.style = s;
        self
    }
    /// Stand this pill on a named [`ControlGround`] — [`Unkeyed`](ControlGround::Unkeyed) for the
    /// player HUD, whose ground is the video plane and cannot be sampled. Nothing else needs it.
    pub fn ground(mut self, g: ControlGround) -> Self {
        self.ground = g;
        self
    }
    /// Answer to a page-drawn ground with the palette it published for this control row.
    pub fn palette(mut self, palette: ControlPalette) -> Self {
        self.palette = palette;
        self
    }

    /// The width this button wants for `label` at type size `sz`: its own content run (icon box +
    /// gap + label, per [`BTN_ICON_RATIO`]/[`BTN_ICON_GAP`]) plus one air budget. The LAYOUT
    /// companion to `draw`, which only ever centres that same run in the frame it is handed — so
    /// the two read the same constants and cannot drift.
    ///
    /// ONE formula for every pill in the product, because they all relabel from state and a fixed
    /// frame that fits the short word crams the long one against its own capsule ends: both hero
    /// rows ("Play"/"Continue", "Play"/"Resume") pass `icon: true`, and Home's status-screen Retry
    /// control passes `false`. That flag is the whole reason this takes one — an icon-less pill
    /// measured with an icon box gets a slug of air it never fills, which is why the earlier
    /// icon-only version had to send such callers off to `text::text_width` on their own. A second
    /// sizing path is exactly the drift this file exists to prevent.
    pub fn pill_w(label: *const c_char, sz: c_int, icon: bool) -> f32 {
        Self::pill_w_full(label, sz, icon, false)
    }

    /// [`Button::pill_w`] with the TRAILING accessory counted too — the same one formula, taking
    /// both of the button's optional slots rather than growing a second sizing path beside it (the
    /// drift this file exists to prevent, and the exact reason `pill_w` gained its `icon` flag).
    /// An accessory occupies one more icon box and one more gap, which is what [`Button::draw`]
    /// lays out below.
    pub fn pill_w_full(label: *const c_char, sz: c_int, icon: bool, trailing: bool) -> f32 {
        // The `Measure`-threaded twin is `pill_w_measured`, just below. This raw form has to stay
        // measure-less: its own caller `StatusOverlay::action_frame` is reached from
        // `impl View for StatusOverlay` (`View::draw` takes no `Measure`, and reshaping that shared
        // retained-leaf trait is out of this lane's scope) as well as from a host test that
        // deliberately compares this exact formula against `RawTextMeasure`. `TtfMeasure` wraps the
        // identical `text_width` this line called directly.
        use nj_machine::machine::Measure as _;
        let advance = LegacyMeasure.width(
            unsafe { std::ffi::CStr::from_ptr(label) },
            sz,
            true,
        );
        Self::pill_w_from_advance(advance, sz, icon, trailing)
    }

    pub(crate) fn pill_w_measured(label: &core::ffi::CStr, sz: c_int, icon: bool, trailing: bool,
        measure: &dyn nj_machine::machine::Measure) -> f32 {
        Self::pill_w_from_advance(measure.width(label, sz, true), sz, icon, trailing)
    }

    fn pill_w_from_advance(advance: f32, sz: c_int, icon: bool, trailing: bool) -> f32 {
        let (isz, gap) = if icon {
            (sz as f32 * BTN_ICON_RATIO, BTN_ICON_GAP)
        } else {
            (0.0, 0.0)
        };
        let (tsz, tgap) = if trailing {
            (sz as f32 * BTN_ICON_RATIO, BTN_ICON_GAP)
        } else {
            (0.0, 0.0)
        };
        isz + gap + advance + tgap + tsz + BTN_PILL_AIR
    }

    /// Turn the pill into its own countdown: `frac` of its width is filled with
    /// [`theme::CONTROL_SPENT_FILL`], the rest with the button's normal face, so time reads as a
    /// sweep across the control itself instead of a separate rail beside it.
    ///
    /// **Progress and focus are separate channels, deliberately.** The first version drew the
    /// filled part as the FOCUSED face and the rest as the idle one, which collapsed the two: a
    /// focused counting button was pixel-identical to an unfocused idle one at t=0, and the label's
    /// ink flipped at the sweep line — bisecting a word with a hard edge, which from a couch reads
    /// as a torn glyph atlas rather than a timer. Now the face is drawn once, at its true focus
    /// state, and only the FILL BEHIND the label changes — the ink never inverts.
    pub fn progress(mut self, frac: f32) -> Self {
        self.progress = Some(frac);
        self
    }

    /// The pill's filled background, including the countdown sweep when one is set. Takes the rect
    /// rather than reading `self.frame`, because the drawn plate is the POPPED one
    /// ([`Button::scale`]) and the sweep has to ride it.
    fn plate(&self, p: Painter, r: Rect, face: ControlFace) {
        // The DRAWN box, which is not the laid-out frame: a capsule's ends are under half its box,
        // so the box carries the deficit and the ends come out at the frame's own height
        // (`face_box`). A stadium and a disc are returned untouched.
        let b = face_box(r);
        let rad = b.h * 0.5;
        if matches!(self.style, ControlStyle::Action { .. }) {
            // A plain stadium (CSS border-radius 50% / 36px): the film under the whole box, the
            // ring over its edge, exactly how the browser composites a translucent border.
            let rad = r.h * 0.5;
            p.rrect(r, rad, rad, face.top);
            if !self.focused {
                p.rring(r, rad, theme::ACTION_BORDER_W, theme::ACTION_BORDER);
            }
            return;
        }
        if matches!(self.style, ControlStyle::Keyline) && !self.focused {
            let rad = r.h * 0.5;
            // the knockout: stroke colour first, then the interior inset by it — the SDF has no
            // stroke-only mode (`keyline_chip` / `pass_capsule`'s construction); `bg` here is
            // `colors()`'s translucent interior, not a repaint of the ground (there is none over
            // live video — see `theme::PILL_KEYLINE_BG`)
            p.rrect(r, rad, rad, theme::PILL_KEYLINE);
            let s = BTN_KEYLINE_W;
            p.rrect(
                Rect::new(r.x + s, r.y + s, r.w - 2.0 * s, r.h - 2.0 * s),
                rad - s,
                rad - s,
                face.top,
            );
        } else {
            // FOCUS is an elevation as well as a fill — see `control_cast`. Under the face, so the
            // near-opaque focused plate covers everything the shadow's own interior cut leaves.
            if self.focused {
                control_cast(p, b, rad);
            }
            control_rim(p, b, rad, face.top, face.body, self.focused, self.ground);
        }
        let Some(frac) = self.progress else { return };
        let w = b.w * frac.clamp(0.0, 1.0);
        if w <= 0.0 {
            return;
        }
        // Scissor so the sweep inherits the capsule's rounded ends instead of a square edge; it is
        // GLOBAL GL state, so it is set and cleared inside this one draw and never left armed. The
        // sweep is drawn to the same OUTLINE with no edge of its own: a stadium here would spill
        // outside the capsule at the ends, which is precisely where a countdown is watched.
        p.clip(Rect::new(b.x, b.y, w, b.h));
        let spent = face.spent;
        p.face_rimmed(
            b,
            rad,
            face_outline(b).as_ref(),
            spent,
            spent,
            [0.0; 4],
            0.0,
            0.0,
            None,
        );
        p.clip_clear();
    }
}
/// **A standalone control face for a caller outside this module** — the same plate
/// [`Button::plate`] draws (the capsule outline, the edge sheen, and the focus cast), for a control
/// whose layout does not fit `Button`'s one-label-two-icons shape and so cannot be a `Button`
/// itself. [`super::linked_heading`] uses it for its entry and heading presentations: independent
/// text runs (label, separator, count) and a trailing chevron, drawn by hand rather than through
/// `Button::icon`/`label`/`trailing_icon`. It used to fill itself with a bare `Painter::rrect`/
/// `rrect_sheened` — a plain rounded rect with no capsule outline, no edge sheen and no focus cast,
/// the one pill-shaped control in the app that skipped the construction [`control_rim`] gives every
/// other one (`CircleButton`, `TransportButton`, `TabPill`'s standalone `ground()` style, `Button`
/// itself). Flat fill only — `top` and `body` the same colour — because that caller has never
/// needed the gradient the two-tone [`ControlFace`] exists for; hand it a real `ControlFace` if one
/// ever does.
pub(crate) fn draw_control_face(
    p: Painter,
    r: Rect,
    fill: [f32; 4],
    focused: bool,
    ground: ControlGround,
) {
    let b = face_box(r);
    let rad = b.h * 0.5;
    if focused {
        control_cast(p, b, rad);
    }
    control_rim(p, b, rad, fill, fill, focused, ground);
}
impl View for Button {
    fn draw(&self, _e: &Env, p: Painter) {
        let r = self.frame.scaled(self.scale);
        let face = self.style.face(self.focused, self.ground, self.palette);
        let ink = face.ink;
        // **Nothing at REST, a lift on FOCUS** — `plate` draws [`control_cast`] under the face and
        // only when the remote is on it. A control face is held by its EDGE ([`control_rim`], the
        // card's own .22 sheen) and that is the whole of its resting elevation; what it wears when
        // focused is fill, pop AND cast.
        //
        // This file used to argue for neither, and the measured case it kept written down is the one
        // that decided it: an ACCENT capsule over a WHITE frame measures ~1.2:1 against its
        // surround, so the plate disappears and only the dark label is left floating. The rim's own
        // weight was the answer offered, and a rim cannot be one — a near-white line on a near-white
        // ground has nothing to separate. `theme::CONTROL_CAST_FOCUS` is what the design system
        // asks for, and it is still ONE elevation for the family: the resting control casts
        // nothing, which is the half of that rule worth keeping.
        self.plate(p, r, face);
        // center the [icon + gap + label] group in the pill; the label sits on the pill centre by
        // its cap band, so descenders (the g's in "From Beginning") don't drag the caps upward
        let ty = nj_gfx::text::text_vcenter_y(self.sz, 1, r.y + r.h * 0.5);
        // `Button` draws through the generic retui `View::draw` (no `Measure` parameter; see the
        // identical note on `TabPill::draw` above). `TtfMeasure` wraps the same `text_width`.
        let tw = {
            use nj_machine::machine::Measure as _;
            LegacyMeasure.width(
                unsafe { std::ffi::CStr::from_ptr(self.label) },
                self.sz,
                true,
            )
        };
        let (isz, gap) = match (self.icon.is_some(), self.style) {
            (false, _) => (0.0, 0.0),
            // Litefin scales the whole control, glyph included; an icon-only circle centres it alone.
            (true, ControlStyle::Action { .. }) => {
                (ACTION_ICON * self.scale, if tw > 0.0 { BTN_ICON_GAP } else { 0.0 })
            }
            (true, _) => (self.sz as f32 * BTN_ICON_RATIO, BTN_ICON_GAP),
        };
        let (asz, agap) = if self.trailing.is_some() {
            (self.sz as f32 * BTN_ICON_RATIO, BTN_ICON_GAP)
        } else {
            (0.0, 0.0)
        };
        // the WHOLE run is centred — accessory included — which is why `pill_w_full` measures it:
        // sizing the pill without the chevron and then drawing one would push the label off-centre
        let gl = r.cx() - (isz + gap + tw + agap + asz) * 0.5;
        if let Some(icon) = self.icon {
            crate::ui::icons::draw(
                p,
                icon,
                Rect::new(gl, r.y + (r.h - isz) * 0.5, isz, isz),
                ink,
            );
        }
        p.text(self.label, gl + isz + gap, ty, self.sz, ink, 0, 1); // left-aligned after the icon
        if let Some(acc) = self.trailing {
            let ax = gl + isz + gap + tw + agap;
            crate::ui::icons::draw(
                p,
                acc,
                Rect::new(ax, r.y + (r.h - asz) * 0.5, asz, asz),
                ink,
            );
        }
    }
}

// ---- Badge: the small rounded metadata chip (CC / SDH / AD / FORCED / codec tags), with an
// OPTIONAL leading glyph. ONE leaf for the track-menu rows, the Info card meta line, the detail
// About column and the episode filmstrip's duration pill, so the chip look can't drift. Cap-band-
// centred bold CAPTION label; width hugs the label with a floor so short tags (CC) still read as a
// chip. Returns the drawn width so callers can flow chips inline. ----
pub(crate) enum BadgeStyle {
    /// 2px border + knockout interior: label in `col`, ring in `border`, interior filled `bg` (the
    /// surface behind the chip — keeps the outline clean over a light focus pill or a dark panel).
    ///
    /// The ring is its OWN colour because the design system makes it one: a table row's chip is
    /// `border: 2px solid (focused ? --accent-ink : --overlay-border)` with `color: ink`, i.e. the
    /// stroke is a keyline and only the label carries the row's ink. Both callers passed `col` for
    /// both, so every chip off a focus pill outlined at full [`theme::TEXT_PRIMARY`] — nearly twice
    /// the ink the contract gives it, which is what made a FORCED/SDH tag read as loud as the track
    /// name beside it. [`theme::OVERLAY_BORDER`] is the value, and its own doc has said
    /// "outlined-badge / meta-badge border" the whole time it had no consumer.
    Outlined {
        col: [f32; 4],
        border: [f32; 4],
        bg: [f32; 4],
    },
    /// solid translucent fill ([`theme::BADGE_FILL`]), label in [`theme::TEXT_HEADING`] — the
    /// About column's accessibility chips.
    Filled,
    /// A CAPSULE that rides on ARTWORK: the idle-control pair ([`theme::CONTROL_IDLE_FILL`] face,
    /// [`theme::CONTROL_IDLE_INK`] ink) and fully rounded ends — the same surface
    /// [`watched_badge`]'s disc wears, so a chip and a disc laid over the same still read as one
    /// family. Deliberately NOT [`BadgeStyle::Filled`]: that chip's translucent light fill is
    /// legible in a dark text column and disappears over a bright thumbnail.
    ///
    /// Its ink is NEUTRAL on purpose. Amber (`RESUME_*`) is the app's one watched-STATE hue, and a
    /// chip in that hue over a tile that already carries a state mark would be a second, competing
    /// claim about the same item (see `ui/CLAUDE.md`'s one-vocabulary rule).
    OverArt,
}
/// The chip's height — one band for every style, so a row mixing them stays on one line. Public
/// because a caller that pins a chip to an edge (the episode still's duration pill) needs to know
/// how tall the thing it is placing is.
pub(crate) const BADGE_H: f32 = 34.0;
/// A leading glyph's box, at the label's own type size — a touch over its cap height, the same
/// relationship [`rating_group`]'s verdict mark has to its score. Deliberately smaller than
/// [`Button`]'s `sz * BTN_ICON_RATIO`: this chip is half a button's height, so a button-proportioned
/// glyph would fill it edge to edge.
const BADGE_ICON: f32 = theme::size::CAPTION as f32;
/// Glyph → label air. They are one run, so the tightest rung (as in [`rating_group`]).
const BADGE_ICON_GAP: f32 = theme::space::XS;

/// pixel width [`badge`] will occupy for `text` (+ `icon`) — the layout companion (e.g. reserving
/// the inline-chip run so a row label elides before it). The icon's band is added OUTSIDE the
/// short-tag floor, so a bare "CC" still measures its minimum and a glyphed chip still fits both.
// ---- The PLEX PASS capsule (`Details Screen.dc.html` / `Player Screen.dc.html`) --------------
//
// The name set in type — deliberately NO logo artwork: this is an unofficial client, and the
// badge is a referential use of the words alone (the same reasoning `plex::identity` documents
// for the product name). Height matches [`BADGE_H`] so it shares a badge row's optical line.
// **Two product surfaces, both places the name changes what the user does next** (see
// `theme::PASS_GOLD`'s doc for the docs-derived rule): FILLED in the playback-failed read-out
// (pure black ground), OUTLINE in the detail facts row's Pass-gated states. Non-interactive in
// both; it is [`theme::PASS_GOLD`]'s only consumer.
//
// **Re-spec'd 2026-08-12** to the geometry BOTH mock files now carry (which is what makes it a
// decision rather than a drift): an 8px rounded rect at a 2px stroke, `size::CAPTION` bold at
// `.06em` — up from a full-pill silhouette, a 1.5px stroke and `size::MICRO` at `.12em`. The
// silhouette is no longer what separates it from a technical chip; the GOLD is, and the label
// now sits on the couch-legibility floor instead of below it. (The Player Screen's comment
// beside the filled form still says "full pill radius" against its own `border-radius:8px` —
// the CSS is the artifact and the comment is stale.)

/// Letter-tracking for the capsule label: the design's `.06em` of [`theme::size::CAPTION`]. The
/// text renderer has no letter-spacing, so the label is drawn per character.
const PASS_TRACK: f32 = theme::size::CAPTION as f32 * 0.06;
/// The label inset. The design states `padding: 0 13px 0 15px` and **that asymmetry does not
/// port** — honouring the reasoning, not the literal. The mock is asymmetric because CSS emits a
/// letter-space after the LAST character too, so it takes that trailing space off the right pad to
/// keep the label optically centred; its own comment says exactly that. Our renderer tracks
/// BETWEEN characters only ([`pass_label_w`] counts `n − 1` gaps), so there is no trailing space to
/// compensate for, and copying 15/13 would push the ink 1px right of centre — off-centre in the
/// opposite direction from the thing the design was correcting. 14/14 keeps the SUM, and therefore
/// the drawn width and every layout measured from it, byte-identical.
const PASS_PAD_X: f32 = 14.0;
/// Corner radius and stroke — `border-radius: 8px`, `inset 0 0 0 2px`.
const PASS_RAD: f32 = 8.0;
const PASS_STROKE: f32 = 2.0;
/// The label, "PLEX PASS", **pre-split into per-character `CStr` literals** — the tracking is
/// applied by advancing the pen between them, so the label is nine one-character draws.
/// Compile-time constants rather than nine `CString::new` allocations per call: `pass_capsule_w`
/// alone walks them, and the capsule is now on the detail hero for every converting item on a
/// proven-Pass-less server (not only an HDR one) as well as in the failure read-out, so this ran
/// ~18 small allocations a frame for a string that never changes.
const PASS_CHARS: [&std::ffi::CStr; 9] = [c"P", c"L", c"E", c"X", c" ", c"P", c"A", c"S", c"S"];

/// The label's own drawn width, **memoised** — the pens below re-measure per character anyway, so
/// only the total is worth holding. Main-thread only, like every other layout memo here.
///
/// A safe atomic (bits of the `f32` held in a `u32`) rather than `static mut` — the same
/// `AtomicU32` this file already uses for the diagnostic counters and
/// [`VEIL_TEX`], applied to a float memo.
static PASS_W: AtomicU32 = AtomicU32::new(0);

fn pass_label_w(measure: &dyn nj_machine::machine::Measure) -> f32 {
    // The width reads 0 until `init_text` has run (a live `TtfMeasure`) — never cache a pre-init
    // measurement (the same guard `ctrl_slot`'s width memo keeps, and for the same reason). Under
    // replay the threaded `Measure` is a `TableMeasure`, which answers from the recorded table
    // rather than 0, so the memo is populated on its first call there too.
    let memo = f32::from_bits(PASS_W.load(Relaxed));
    if memo > 0.0 {
        return memo;
    }
    let mut w = 0.0;
    for c in PASS_CHARS {
        w += measure.width(c, theme::size::CAPTION, true);
    }
    if w <= 0.0 {
        return 0.0;
    }
    w += PASS_TRACK * (PASS_CHARS.len() - 1) as f32;
    PASS_W.store(w.to_bits(), Relaxed);
    w
}

/// Layout width of the capsule — for right-anchoring and row flow.
pub(crate) fn pass_capsule_w(measure: &dyn nj_machine::machine::Measure) -> f32 {
    pass_label_w(measure) + 2.0 * PASS_PAD_X
}

/// Draw the capsule with its LEFT edge at `x`, centred on `cy`; returns its width.
///
/// `filled: false` is the OUTLINE form — a [`PASS_STROKE`] ring of pass-gold with a gold label and
/// **nothing inside it** ([`Painter::rring`]), which is the mock's `box-shadow: inset 0 0 0 2px
/// var(--pass-gold)` with no `background`. It is the default everywhere a surface sits behind it,
/// and it no longer has to be told what that surface is: the knockout it replaces painted the
/// interior in a `bg` the caller named, which over the detail hero's backdrop meant a gold-ringed
/// dark BOX rather than a hairline.
///
/// `filled: true` is the FILLED form — pass-gold fill, near-black label — used in exactly one
/// place, the playback-failed read-out, where the ground is pure black and an outline would read
/// as a hole.
pub(crate) fn pass_capsule(
    p: Painter,
    x: f32,
    cy: f32,
    filled: bool,
    measure: &dyn nj_machine::machine::Measure,
) -> f32 {
    let w = pass_capsule_w(measure);
    let r = Rect::new(x, cy - BADGE_H * 0.5, w, BADGE_H);
    let ink = if filled {
        p.rrect(r, PASS_RAD, PASS_RAD, theme::PASS_GOLD);
        theme::PASS_GOLD_INK
    } else {
        p.rring(r, PASS_RAD, PASS_STROKE, theme::PASS_GOLD);
        theme::PASS_GOLD
    };
    let ty = nj_gfx::text::text_vcenter_y(theme::size::CAPTION, 1, cy);
    let mut cx = x + PASS_PAD_X;
    for c in PASS_CHARS {
        p.text(c.as_ptr(), cx, ty, theme::size::CAPTION, ink, 0, 1);
        cx += measure.width(c, theme::size::CAPTION, true) + PASS_TRACK;
    }
    w
}

pub(crate) fn badge_w(
    text: &str,
    icon: Option<crate::ui::icons::Icon>,
    measure: &dyn nj_machine::machine::Measure,
) -> f32 {
    const PAD: f32 = 12.0;
    const MIN_W: f32 = 56.0;
    let lead = if icon.is_some() {
        BADGE_ICON + BADGE_ICON_GAP
    } else {
        0.0
    };
    (measure.width_str(text, theme::size::CAPTION, true) + 2.0 * PAD).max(MIN_W) + lead
}
/// Draw one chip with its LEFT edge at `x`, vertically centred on `cy`; returns its width.
pub(crate) fn badge(
    p: Painter,
    x: f32,
    cy: f32,
    text: &str,
    icon: Option<crate::ui::icons::Icon>,
    style: BadgeStyle,
    measure: &dyn nj_machine::machine::Measure,
) -> f32 {
    let lc = match std::ffi::CString::new(text) {
        Ok(c) => c,
        Err(_) => return 0.0,
    };
    let sz = theme::size::CAPTION;
    let w = badge_w(text, icon, measure);
    let r = Rect::new(x, cy - BADGE_H * 0.5, w, BADGE_H);
    let ink = match style {
        BadgeStyle::Outlined { col, border, bg } => {
            let bw = 2.0f32;
            p.rrect(r, 6.0, 6.0, border); // keyline
            p.rrect(
                Rect::new(r.x + bw, r.y + bw, r.w - 2.0 * bw, r.h - 2.0 * bw),
                5.0,
                5.0,
                bg,
            );
            col
        }
        BadgeStyle::Filled => {
            p.rrect(r, 7.0, 7.0, theme::BADGE_FILL);
            theme::TEXT_HEADING
        }
        BadgeStyle::OverArt => {
            let rad = r.h * 0.5;
            p.rrect(r, rad, rad, theme::CONTROL_IDLE_FILL);
            theme::CONTROL_IDLE_INK
        }
    };
    let ty = nj_gfx::text::text_vcenter_y(sz, 1, cy);
    // [glyph + gap + label] centred in the chip as ONE run — the same composition `Button::draw`
    // uses, so a chip and a pill put their icon in the same optical place. With no icon `lead` is 0
    // and this collapses to the label centred on its own, which is what it always did.
    let lead = if icon.is_some() {
        BADGE_ICON + BADGE_ICON_GAP
    } else {
        0.0
    };
    let tw = measure.width(&lc, sz, true);
    let gl = r.cx() - (lead + tw) * 0.5;
    if let Some(i) = icon {
        crate::ui::icons::draw(
            p,
            i,
            Rect::new(gl, cy - BADGE_ICON * 0.5, BADGE_ICON, BADGE_ICON),
            ink,
        );
    }
    p.text(lc.as_ptr(), gl + lead, ty, sz, ink, 0, 1); // left-aligned after the glyph
    w
}

// ---------------------------------------------------------------------------------------
// ---- Rating row: one PROVIDER's scores under the provider's name in words.
//
// Rewritten 2026-08-02 from `Details Screen.dc.html`. It used to draw one badge per score, each
// behind that provider's own brand mark — Rotten Tomatoes' fruit and popcorn tub as tinted
// silhouettes, IMDb and TMDB as logotype chips in their brand colours. All of that is gone:
//
//   * the RT marks had no licensing route (see `ui/icons.rs`), and
//   * the chips were reproducing two more brands' logotypes to solve a problem — "whose score is
//     this?" — that a WORD solves for free, and that naming the provider solves *lawfully*, since
//     referential use needs no licence where a mark does.
//
// So a group is: the provider's name as a quiet MICRO caption in TEXT_TERTIARY, then its score or
// scores. That inverts what carried the colour. Before, four saturated brand marks competed with
// the hero art and with each other; now the captions recede to caption weight and the ONLY colour
// left in the row is the verdict — a red or gold or hollow tomato, a green or drained crowd. The
// row reads as one rhythm instead of four logos.
//
// Rotten Tomatoes is ONE group with two scores under one caption, because critics and audience are
// two readings from one source; IMDb and TMDB are one score each. That is also why this draws a
// GROUP rather than a badge: the caption is shared, so the unit that knows how to lay itself out
// is the provider, not the score.
// ----

/// Mark box (px). A little over the meta line's cap height so a 26-unit silhouette still resolves
/// at couch distance — these marks carry the VERDICT, so legibility here is not cosmetic.
pub(crate) const RATING_MARK_D: f32 = 30.0;
/// Glyph → its score. They are one unit, so it stays tight.
const RATING_GAP: f32 = 10.0;
/// Provider caption → the first score under it.
const RATING_CAPTION_GAP: f32 = 12.0;
/// Score → the next glyph in the SAME group (Rotten Tomatoes' critic → audience). Wider than
/// [`RATING_GAP`] so the two pairs read as two readings rather than one run of four things.
const RATING_PAIR_GAP: f32 = 14.0;

/// One colour layer of a rating mark — a mask and the tint it is painted in. Marks are two-tone
/// (body + calyx), and the rasterizer renders a MASK, so a mark is a slice rather than one icon.
pub(crate) type MarkLayer = (crate::ui::icons::Icon, [f32; 4]);

/// One score inside a provider group: the mark that carries its verdict, and the score as text.
/// `mark` is empty for a provider that has no verdict to draw (IMDb, TMDB) — their number IS the
/// whole statement, and inventing a glyph for them is what put a meaningless star here before.
pub(crate) struct RatingCell<'a> {
    pub(crate) mark: &'a [MarkLayer],
    pub(crate) value: &'a str,
    /// Trailing unit set a rung down in tertiary ink — IMDb's "/10". A percentage carries its own
    /// "%" inside `value`, because there the unit is part of the number rather than a scale note.
    pub(crate) suffix: &'a str,
}

/// Width [`rating_group`] will occupy. Measure before drawing so a row can stop at a margin
/// instead of running a group off the panel (same contract as `badge`/`badge_w`).
pub(crate) fn rating_group_w(
    caption: &str,
    cells: &[RatingCell],
    measure: &dyn nj_machine::machine::Measure,
) -> f32 {
    if caption.contains('\0') {
        return 0.0;
    }
    let mut w = measure.width_str(caption, theme::size::MICRO, true) + RATING_CAPTION_GAP;
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            w += RATING_PAIR_GAP;
        }
        if !cell.mark.is_empty() {
            w += RATING_MARK_D + RATING_GAP;
        }
        if !cell.value.contains('\0') {
            w += measure.width_str(cell.value, theme::size::LABEL, true);
        }
        if !cell.suffix.contains('\0') {
            w += measure.width_str(cell.suffix, theme::size::MICRO, true);
        }
    }
    w
}

/// Draw one provider's group with its LEFT edge at `x`, centred on `cy`; returns its width.
pub(crate) fn rating_group(
    p: Painter,
    x: f32,
    cy: f32,
    caption: &str,
    cells: &[RatingCell],
    measure: &dyn nj_machine::machine::Measure,
) -> f32 {
    let Ok(cap) = std::ffi::CString::new(caption) else {
        return 0.0;
    };
    let mut bx = x;
    // The caption sits on the SCORE's baseline, not on its own centre: the design aligns the row
    // by baseline (`align-items:baseline`), so a MICRO caption beside a LABEL number must share
    // the number's baseline or it floats. `text::baseline_y` is that rule, shared.
    let base = nj_gfx::text::baseline_y(
        theme::size::MICRO,
        1,
        theme::size::LABEL,
        1,
        nj_gfx::text::text_vcenter_y(theme::size::LABEL, 1, cy),
    );
    p.text(
        cap.as_ptr(),
        bx,
        base,
        theme::size::MICRO,
        theme::TEXT_TERTIARY,
        0,
        1,
    );
    bx += measure.width(&cap, theme::size::MICRO, true) + RATING_CAPTION_GAP;

    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            bx += RATING_PAIR_GAP;
        }
        if !cell.mark.is_empty() {
            // every layer rides the SAME rect, so all of them rasterize at one size from one
            // viewBox and register exactly — see `ui/icons.rs`'s note on the layered marks
            let r = Rect::new(bx, cy - RATING_MARK_D * 0.5, RATING_MARK_D, RATING_MARK_D);
            for (mask, tint) in cell.mark.iter() {
                crate::ui::icons::draw(p, *mask, r, *tint);
            }
            bx += RATING_MARK_D + RATING_GAP;
        }
        if let Ok(v) = std::ffi::CString::new(cell.value) {
            let ty = nj_gfx::text::text_vcenter_y(theme::size::LABEL, 1, cy);
            p.text(
                v.as_ptr(),
                bx,
                ty,
                theme::size::LABEL,
                theme::TEXT_PRIMARY,
                0,
                1,
            );
            bx += measure.width(&v, theme::size::LABEL, true);
        }
        if let Ok(s) = std::ffi::CString::new(cell.suffix) {
            p.text(
                s.as_ptr(),
                bx,
                base,
                theme::size::MICRO,
                theme::TEXT_TERTIARY,
                0,
                1,
            );
            bx += measure.width(&s, theme::size::MICRO, true);
        }
    }
    // Every advance above already used the supplied metric source. Return that extent rather
    // than measuring every run a second time; the host test compares it with rating_group_w.
    bx - x
}

// ---- Litefin details plates (`docs/litefin-look.md` 5): the official rating badge, a tech pill
// and a rich-metadata chip. Flat white-alpha plates, no sheen. Each draws with its LEFT edge at
// `x`, vertically centred on `cy`, and returns its width; [`LfPlate::w`] measures the same run so a
// row can lay out before it draws. Heights are the CSS box: line-height 1.5 plus the padding. ----

/// Which tint a tech pill wears: plain, HDR's warm wash or Dolby Vision's violet one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PillTone {
    Plain,
    Hdr,
    DolbyVision,
}

/// One of the three plates, carrying its type size, weight, padding and radius.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum LfPlate {
    /// The official rating badge: 19.2/600 `#e0e0e0` on white .15, padding 4x10, radius 12.
    Rating,
    /// A tech pill: 20/600 on white .09 in a 1px white .16 border, padding 4x12, radius 12.
    Tech(PillTone),
    /// A rich-metadata chip: 17.6/500 on white .05 in a 2px white .10 border, padding 6x10,
    /// radius 16; focused, the accent under lavender text.
    Chip { focused: bool },
}

impl LfPlate {
    fn sz(self) -> c_int {
        match self {
            LfPlate::Rating => theme::size::lf::S19,
            LfPlate::Tech(_) => theme::size::lf::S20,
            LfPlate::Chip { .. } => theme::size::lf::S18,
        }
    }
    fn bold(self) -> bool {
        !matches!(self, LfPlate::Chip { .. })
    }
    fn pad(self) -> (f32, f32) {
        match self {
            LfPlate::Rating => (10.0, 4.0),
            LfPlate::Tech(_) => (12.0, 4.0),
            LfPlate::Chip { .. } => (10.0, 6.0),
        }
    }
    fn border(self) -> f32 {
        match self {
            LfPlate::Rating => 0.0,
            LfPlate::Tech(_) => 1.0,
            LfPlate::Chip { .. } => 2.0,
        }
    }
    /// The plate's outer height.
    pub(crate) fn h(self) -> f32 {
        self.sz() as f32 * 1.5 + 2.0 * (self.pad().1 + self.border())
    }
    /// The plate's outer width for `text`.
    pub(crate) fn w(self, text: &str, measure: &dyn nj_machine::machine::Measure) -> f32 {
        measure.width_str(text, self.sz(), self.bold()) + 2.0 * (self.pad().0 + self.border())
    }
    /// Draw `text` on this plate; returns its width.
    pub(crate) fn draw(
        self,
        p: Painter,
        x: f32,
        cy: f32,
        text: &str,
        measure: &dyn nj_machine::machine::Measure,
    ) -> f32 {
        let Ok(c) = std::ffi::CString::new(text) else { return 0.0 };
        use theme::details_lf as lf;
        let (w, h) = (self.w(text, measure), self.h());
        let r = Rect::new(x, cy - h * 0.5, w, h);
        let rad = match self {
            LfPlate::Chip { .. } => theme::radius::LG,
            _ => theme::radius::BASE,
        };
        let ink = match self {
            LfPlate::Rating => {
                p.rrect(r, rad, rad, lf::RATING_BADGE_FILL);
                lf::RATING_BADGE_INK
            }
            LfPlate::Tech(tone) => {
                p.rrect(r, rad, rad, lf::TECH_PILL_FILL);
                let tint = match tone {
                    PillTone::Plain => None,
                    PillTone::Hdr => Some(lf::TECH_PILL_HDR),
                    PillTone::DolbyVision => Some(lf::TECH_PILL_DOVI),
                };
                if let Some((top, bot)) = tint {
                    p.rect(r, rad, top, bot, 0.0);
                }
                p.rring(r, rad, self.border(), lf::TECH_PILL_BORDER);
                theme::TEXT_PRIMARY
            }
            LfPlate::Chip { focused: true } => {
                p.rrect(r, rad, rad, theme::BRAND_FILL);
                theme::BRAND_INK
            }
            LfPlate::Chip { focused: false } => {
                p.rrect(r, rad, rad, lf::CHIP_FILL);
                p.rring(r, rad, self.border(), lf::CHIP_BORDER);
                theme::TEXT_PRIMARY
            }
        };
        let ty = nj_gfx::text::text_vcenter_y(self.sz(), c_int::from(self.bold()), cy);
        p.text(c.as_ptr(), x + self.pad().0 + self.border(), ty, self.sz(), ink, 0, c_int::from(self.bold()));
        w
    }
}

#[cfg(test)]
#[test]
fn rating_group_measures_each_run_once_and_returns_its_drawn_width() {
    use nj_machine::machine::Measure;
    use std::cell::Cell;
    let _serial = nj_base::testlock::serial();
    struct Counting(Cell<usize>);
    impl Measure for Counting {
        fn width(&self, s: &CStr, sz: i32, _: bool) -> f32 {
            self.0.set(self.0.get() + 1);
            s.to_bytes().len() as f32 * sz as f32 * 0.5
        }
        fn cap_h(&self, sz: i32) -> f32 { sz as f32 }
        fn line_h(&self, sz: i32) -> f32 { sz as f32 }
    }
    let measure = Counting(Cell::new(0));
    let cells = [
        RatingCell { mark: &[], value: "8.1", suffix: "/10" },
        RatingCell { mark: &[], value: "92%", suffix: "" },
    ];
    let expected = rating_group_w("Provider", &cells, &measure);
    let runs = measure.0.replace(0);
    let drawn = rating_group(Painter::recording(), 64.0, 100.0, "Provider", &cells, &measure);
    assert!((drawn - expected).abs() < 0.001);
    assert_eq!(measure.0.get(), runs, "the draw must not repeat its entire measurement walk");
    nj_gfx::text::take_measure_fault();
}

#[cfg(test)]
#[path = "widgets_test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "widgets_tab_strip_geometry_tests.rs"]
mod tab_strip_geometry_tests;

#[cfg(test)]
#[path = "widgets_control_face_tests.rs"]
mod control_face_tests;

#[cfg(test)]
#[path = "widgets_measure_tests.rs"]
mod measure_tests;

#[cfg(test)]
#[path = "widgets_glass_budget_tests.rs"]
mod glass_budget_tests;

#[cfg(test)]
#[path = "widgets_tab_capsule_motion_tests.rs"]
mod tab_capsule_motion_tests;

#[cfg(test)]
#[path = "widgets_poster_mark_tests.rs"]
mod poster_mark_tests;

#[cfg(test)]
#[path = "widgets_ambient_ground_tests.rs"]
mod ambient_ground_tests;

#[cfg(test)]
#[path = "widgets_ground_tests.rs"]
mod ground_tests;

#[cfg(test)]
#[path = "widgets_hero_text_tests.rs"]
mod hero_text_tests;

#[cfg(test)]
#[path = "widgets_art_crop_tests.rs"]
mod art_crop_tests;
