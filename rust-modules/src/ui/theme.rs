//! `theme` — the single palette for the whole UI, in TWO LAYERS.
//!
//! **Primitives** (private, at the top of this file) are the palette itself: the only place a colour
//! CODE is written down, but for the two stops the renderer paints with, which `gfx::tokens` holds. **Roles** (`pub`, everything after them) are the JOBS — [`TEXT_PRIMARY`],
//! [`ACCENT`], [`HAIRLINE`] — and every role resolves to a primitive, never to a fresh literal. A
//! screen may only ever name a role; that is the other end of `ui/CLAUDE.md`'s "never write a raw
//! colour literal" rule. The design project mirrors this split exactly (`tokens/primitives.css` +
//! `tokens/colors.css`), so a palette decision is one edit here and one there.
//!
//! Two roles may resolve to the SAME primitive and still stay two roles — [`ACCENT`] and
//! [`TEXT_PRIMARY`] are both `COOL_0`, [`TEXT_TERTIARY`] and [`RATING_MUTED`] both `COOL_400` —
//! because sharing a value is not sharing a job: retuning the focus fill must not restyle every
//! title on the screen. They are values on one stop for that reason, not `pub use` aliases of each
//! other.
//!
//! The values began as the player screen's literals promoted to canonical (it was the one screen
//! with a coherent design language); the two *lossy* collapses (several near-whites → `COOL_0`;
//! several dim greys → [`TEXT_SECONDARY`]/[`TEXT_TERTIARY`]) are deliberate and only visible
//! on-device — see `docs/ui-system-migration.md` §A. Colors are `[f32; 4]` (r,g,b,a); scrims supply
//! their alpha per call via [`scrim`]/[`scrim_black`].
//!
//! `ACCENT`/`ACCENT_INK` live here and are re-exported from `ui` (`mod.rs`) so the existing
//! `crate::ui::ACCENT` call sites keep compiling unchanged.
//!
//! **Litefin.** This fork wears Litefin's black theme (`docs/litefin-look.md`, cited by section
//! at each value): the ground, text inks, surfaces and status colours are repointed in place, and
//! the accent `#6b6ede` arrives as NEW roles ([`BRAND_FILL`], [`FOCUS_RING`], the `sidebar` and
//! `osd` modules) rather than as a new [`ACCENT`] - see [`ACCENT`] for why that one stays white.
#![allow(dead_code)]

// ── PRIMITIVES — the palette. PRIVATE: nothing outside this module may name a stop ───────────
// Families are by hue, stops by lightness (0 = lightest). Every stop is an EXACT 8-bit code, which
// is load-bearing rather than tidy: the panel is plain 888 with no sRGB framebuffer anywhere in the
// tree, so a token value IS an sRGB code — and a FRACTIONAL one hands `GL_DITHER` (on by default in
// GLES2) a half-code to alternate on across a large flat fill, which banded the app ground visibly
// before it was snapped. So write the code and let `rgb8` do the division; never a decimal guess.
// `rgb8`, and the two stops the renderer itself paints with (`NEUTRAL_500`, the app ground, and
// `NEUTRAL_1000`, the scrim ink behind `SCRIM_INK`), are written down in `gfx::tokens` (module-layers
// step L5) and imported here, so a code still exists in exactly one place.
use nj_gfx::gfx::tokens::{rgb8, NEUTRAL_500};

// Cool — blue-leaning: everything that is text, and artwork that has not loaded.
const COOL_0: [f32; 4] = rgb8(0xf7, 0xfa, 0xfc);
const COOL_150: [f32; 4] = rgb8(0xdb, 0xe0, 0xeb);
const COOL_250: [f32; 4] = rgb8(0xbc, 0xbe, 0xbf);
const COOL_400: [f32; 4] = rgb8(0x94, 0x99, 0xa3);
const COOL_870: [f32; 4] = rgb8(0x18, 0x19, 0x1c);

// Neutral — achromatic: the shelf, panels, control plates, inks.
const NEUTRAL_600: [f32; 4] = rgb8(0x25, 0x25, 0x27);
const NEUTRAL_950: [f32; 4] = rgb8(0x08, 0x08, 0x0a);
const WHITE: [f32; 4] = rgb8(0xff, 0xff, 0xff);
const BLACK: [f32; 4] = rgb8(0x00, 0x00, 0x00);

// Grey - Litefin's black theme (`docs/litefin-look.md` 2.1): pure achromatic codes, named by their
// 8-bit value because the spec states them as codes. Three inks over the #000 page, then the
// surfaces just off it. `GREY_CC` is the spec's `rgba(255,255,255,.8)` text resolved over that
// page, an opaque code so a contrast floor can grade it.
const GREY_CC: [f32; 4] = rgb8(0xcc, 0xcc, 0xcc);
const GREY_66: [f32; 4] = rgb8(0x66, 0x66, 0x66);
const GREY_44: [f32; 4] = rgb8(0x44, 0x44, 0x44);
const GREY_2A: [f32; 4] = rgb8(0x2a, 0x2a, 0x2a);
const GREY_22: [f32; 4] = rgb8(0x22, 0x22, 0x22);
const GREY_18: [f32; 4] = rgb8(0x18, 0x18, 0x18);
const GREY_12: [f32; 4] = rgb8(0x12, 0x12, 0x12);
const GREY_0A: [f32; 4] = rgb8(0x0a, 0x0a, 0x0a);
const GREY_08: [f32; 4] = rgb8(0x08, 0x08, 0x08);

// Violet - Litefin's one accent, "Litefin Purple Blue" `#6b6ede`, and the variants its
// `ThemeUtils.getAccentVariants` / `getContrastColor` derive from it (`docs/litefin-look.md` 2.1):
// hover (lightness +10), active (-10), the light lavender every ink on the accent is drawn in, and
// the dark rgb its tinted panels use.
const VIOLET_200: [f32; 4] = rgb8(0xd4, 0xd5, 0xf7);
const VIOLET_400: [f32; 4] = rgb8(0x95, 0x97, 0xe7);
const VIOLET_500: [f32; 4] = rgb8(0x6b, 0x6e, 0xde);
const VIOLET_600: [f32; 4] = rgb8(0x41, 0x45, 0xd5);
const VIOLET_950: [f32; 4] = rgb8(0x11, 0x12, 0x22);

// Caption — the achromatic ladder under `WHITE`, and it has exactly one job: the subtitle tones.
// Not more Neutral stops, because Neutral is the app's own dark SURFACES and these are light
// INKS that happen to have no hue. Stops are named by their sRGB level, in percent of `WHITE`.
// The panel's response is a power curve, so the light they emit falls much faster than the
// names do: 85 / 70 / 55 / 40 / 28 percent of the code is roughly 70 / 46 / 27 / 13 / 6 percent
// of the light, which is the range the ladder exists to cover (white over an HDR picture is the
// complaint; see `SUBTITLE_INKS`).
const CAPTION_85: [f32; 4] = rgb8(0xd9, 0xd9, 0xd9);
const CAPTION_70: [f32; 4] = rgb8(0xb3, 0xb3, 0xb3);
const CAPTION_55: [f32; 4] = rgb8(0x8c, 0x8c, 0x8c);
const CAPTION_40: [f32; 4] = rgb8(0x66, 0x66, 0x66);
const CAPTION_28: [f32; 4] = rgb8(0x47, 0x47, 0x47);

// Sand — one warm off-white, and it has exactly one job: the ambient wash's resting cast.
const SAND_100: [f32; 4] = rgb8(0xe9, 0xe6, 0xe0);

// Atmosphere — deliberately dark, warm light for a pre-content route. These are not state colours
// and never ink a control; together they stand in for an artwork UltraBlur envelope before the
// catalog has supplied one. The range stays on PlxNative's graphite/amber axis so first-run does
// not invent a rainbow identity before any actual artwork exists.
//
// **Corrected 2026-09-02.** The first cut of these four stops sat within ~15 8-bit codes of
// `SURFACE_APP` (0x2c,0x2c,0x2e) in BOTH hue and luminance — three of the four were, to the eye,
// the app's own flat ground with one warm corner — which is exactly what a device capture of
// first-run consent showed: "no ambient light on the privacy onboarding screen", not a wash. A
// real keyed hero ground (`AmbientWash::keyed`, capped at `GROUND_LUMA` 0.42 and mixed toward the
// surface at `GROUND_W` 0.26) can and does land in a similarly narrow band when the source artwork
// itself is flat — the difference is that a `RouteGround` fallback bypasses that mix entirely
// (`underlay::Grade::Dim` is the identity grade `RouteGround` latches this quad through — unmixed,
// unlike the `Grade::Ground` a real seed or live frame is capped and leaned through) and has no
// artwork to fall back on if it reads as flat, so it has to carry its OWN contrast rather than borrow the keying
// pipeline's. These four now spread across a ~3x luminance range (see
// `route_screen::tests::the_pre_home_fallback_reads_as_a_directional_wash`) on the same diagonal
// an authored key light would use — bright near one corner, dark at its opposite — while every
// corner still clears 3:1 against `TEXT_TERTIARY` and 7:1 against `TEXT_PRIMARY`, the same floors
// `AmbientWash::keyed` holds a real hero to.
const ATMOS_CHARCOAL: [f32; 4] = rgb8(0x18, 0x18, 0x1a);
const ATMOS_WARM_GREY: [f32; 4] = rgb8(0x4a, 0x46, 0x40);
const ATMOS_UMBER: [f32; 4] = rgb8(0x5e, 0x3e, 0x24);
const ATMOS_ASH: [f32; 4] = rgb8(0x42, 0x42, 0x4a);

// Semantic — amber / red / green: state and verdict, never decoration.
const AMBER_300: [f32; 4] = rgb8(0xfa, 0xb8, 0x2e);
const AMBER_400: [f32; 4] = rgb8(0xf0, 0xb4, 0x29);
const AMBER_500: [f32; 4] = rgb8(0xe5, 0xa0, 0x0d);
const AMBER_950: [f32; 4] = rgb8(0x1a, 0x12, 0x04);
const RED_400: [f32; 4] = rgb8(0xeb, 0x52, 0x4a);
const GREEN_400: [f32; 4] = rgb8(0x3e, 0xc9, 0x6b);
const GREEN_500: [f32; 4] = rgb8(0x2f, 0xae, 0x5b);
// Litefin's own state and mark codes (`docs/litefin-look.md` 2.1), stated exactly by the spec, so
// they are stops of their own rather than a nudge of the ones above.
const RED_430: [f32; 4] = rgb8(0xff, 0x3b, 0x30);
const RED_450: [f32; 4] = rgb8(0xf4, 0x43, 0x36);
const RED_550: [f32; 4] = rgb8(0xfa, 0x32, 0x0a);
const RED_300: [f32; 4] = rgb8(0xff, 0x6b, 0x6b);
const RED_600: [f32; 4] = rgb8(0xdc, 0x35, 0x45);
const GREEN_450: [f32; 4] = rgb8(0x4c, 0xaf, 0x50);
const ORANGE_500: [f32; 4] = rgb8(0xff, 0x98, 0x00);
const BLUE_500: [f32; 4] = rgb8(0x21, 0x96, 0xf3);
const GOLD_400: [f32; 4] = rgb8(0xf5, 0xc5, 0x18);

// The two ALPHA ramps are `WHITE`/`BLACK` at a measured weight, spelled `with_a(WHITE, .20)` at the
// role: a new overlay is a weight on that ramp, never a new hue. `NEUTRAL_900` (#0c0c0d) is not a
// primitive here because its one role — the tab track — is a translucent GRADIENT in this renderer;
// see `TAB_TRACK_TOP`.

// ── Text ────────────────────────────────────────────────────────────────────
/// Primary reading text / high-emphasis title. Litefin's text primary `#ffffff`
/// (`docs/litefin-look.md` 2.1); upstream collapsed its 3-4 near-whites onto `COOL_0` here.
pub const TEXT_PRIMARY: [f32; 4] = WHITE;
/// Section headings ("Related", "Cast & Crew", About headings). Litefin draws its row titles in
/// `#fff` (`docs/litefin-look.md` 2.2 Home row title), so this is the primary stop; upstream sat it
/// a touch below primary.
pub const TEXT_HEADING: [f32; 4] = WHITE;
/// Secondary text: metadata lines, idle row titles. Litefin's text secondary,
/// `rgba(255,255,255,.8)` over the page (`docs/litefin-look.md` 2.1).
pub const TEXT_SECONDARY: [f32; 4] = GREY_CC;
/// **Reading copy** — a multi-line paragraph someone actually reads through, one step brighter than
/// the label grey above it (`#cdd3dd`, `Details Screen.dc.html`). Its one job today is the detail
/// hero's synopsis, which is the longest run of text on the page and sits over the backdrop scrim;
/// at [`TEXT_SECONDARY`] it read as fine print rather than as the blurb. A one-line metadata VALUE
/// stays secondary — the distinction is paragraph-vs-label, not importance.
///
/// Litefin draws both its synopses at the secondary ink, white .8 (`docs/litefin-look.md` 2.2 Hero
/// synopsis, Details overview), so the two roles share [`TEXT_SECONDARY`]'s stop here.
pub const TEXT_READING: [f32; 4] = GREY_CC;
/// Tertiary text: runtime, kickers, inactive tabs, About labels, dim/empty states.
///
/// **Not Litefin's `#666666`** (`docs/litefin-look.md` 2.1): that reads 3.66:1 on the bare black
/// page but this role sits on keyed washes, route grounds and panels too, where it falls to 1.8 and
/// 2.7:1 under the 3:1 floor `widgets_ambient_ground_tests`, `route_screen` and `underlay_tests`
/// grade it at. Copy that only ever sits on the page itself takes [`TEXT_FAINT`].
pub const TEXT_TERTIARY: [f32; 4] = COOL_400;
/// **Litefin's tertiary ink, `#666666`** (`docs/litefin-look.md` 2.1, the Quick Connect
/// instructions and status in 8) - legal ONLY on the bare page ([`SURFACE_APP`], 3.66:1), never on
/// artwork, a wash or a panel; [`TEXT_TERTIARY`] is the dim ink for those.
pub const TEXT_FAINT: [f32; 4] = GREY_66;
/// Litefin's text disabled, `#444444` (`docs/litefin-look.md` 2.1): a label that cannot be acted on.
pub const TEXT_DISABLED: [f32; 4] = GREY_44;
/// The `·` between fine-print facts, and **only** that — a step below the words it separates.
/// Both mocks set every separator dot `opacity:.45` against the run around it, and the reason is
/// worth keeping: a dot at the full ink of its neighbours joins the list instead of punctuating
/// it, so a three-fact row reads as five things. Derived from [`TEXT_TERTIARY`] rather than
/// spelled out, because it is that ink quietened, not a colour of its own.
///
/// Only reachable where the separator is its OWN draw run. A dot baked into a joined string
/// (`"a   ·   b"`) is one run at one colour by construction; those are unchanged, and converting
/// them is a per-site decision about whether the extra draw call is worth it.
pub const TEXT_SEPARATOR: [f32; 4] = with_a(TEXT_TERTIARY, 0.45);

/// **The ink a control shows when it has reached a hard limit** — the Timing capsule's chevron on
/// the side already at its range's edge (`appkit::timing_capsule`, plan `subtitle-menu-capsule` §4,
/// `player.html`'s `leftInk`/`rightInk` at `.22`). A control-specific alpha, not a text rung: it
/// answers "can this direction do anything right now", the same question `TEXT_TERTIARY` answers
/// for words.
pub const INK_DISABLED: [f32; 4] = with_a(WHITE, 0.22);

/// **The inks a client-rendered subtitle may be drawn in**, lightest first — one per rung of
/// `plex::session::SubtitleTone::LADDER`, indexed by `SubtitleTone::index` (a host test pins the
/// two lengths together). The caption is media chrome over the video plane rather than app text,
/// which is why it is `WHITE` and not [`TEXT_PRIMARY`] at the top; the greys under it exist
/// because an HDR picture maps graphics white far brighter than an SDR one does, and the only
/// cure for a searing caption is less light. Text subtitles are inked with the rung; image
/// subtitles (PGS/VobSub) are TINTED by it, which scales their authored colours by the same
/// amount. The dark outline is untouched — it is what keeps the darkest rung legible over a
/// bright scene.
pub const SUBTITLE_INKS: [[f32; 4]; 6] =
    [WHITE, CAPTION_85, CAPTION_70, CAPTION_55, CAPTION_40, CAPTION_28];

// ── Type scale ───────────────────────────────────────────────────────────────
// The ladder is DEFINED in `gfx::tokens` (module-layers step L5): `text` warms exactly these faces and
// the `gfx` layer may not name `ui`. Re-exported as a module, so `theme::size::BODY` and
// `use theme::size::*` hold, and `tools/font-hint-audit.py` reads the rungs from there.
pub use nj_gfx::gfx::tokens::size;

/// The **spacing scale** — the vertical/horizontal *gap* axis of the design system, the sibling of
/// [`size`]. Gaps between stacked elements come from a named rung, never a hand-tuned pixel offset,
/// so vertical rhythm stays consistent instead of drifting per-screen (the same reason colours and
/// sizes are tokenised). Rungs step ~1.6×; a gap picks the nearest rung by role. Positions still
/// flow off *measured* element heights — a rung is the gap *between* blocks, not an absolute Y.
pub mod space {
    /// Hairline gap — an icon and its label, chips in a row.
    pub const XS: f32 = 8.0;
    /// Tight — closely related lines (a meta line under its title).
    pub const SM: f32 = 16.0;
    /// Default — a heading and its body text, label→value pairs.
    pub const MD: f32 = 24.0;
    /// Block gap — one content block to the next (synopsis → action row).
    pub const LG: f32 = 40.0;
    /// Major gap — separates whole regions (a title band from the metadata beneath it).
    pub const XL: f32 = 64.0;
}

/// **Panel geometry tokens** — widths a whole class of popover shares, so no panel carries a width
/// literal of its own.
pub mod layout {
    /// Narrowest an in-player popover (Tracks, More) is drawn, at 1080p. A small page (Style, a
    /// Size / Position / Color picker, the More menu) would otherwise shrink to its labels and wrap
    /// its note into a column too narrow to read; this floor keeps it a panel. A FLOOR only: wider
    /// content still grows the panel up to `ui::table::MENU_MAX_W`.
    pub const PLAYER_MENU_MIN_W: f32 = 440.0;
    /// Bottom edge of an in-player popover: ~28px above the transport buttons, at 1080p.
    pub const PLAYER_MENU_BOTTOM: f32 = crate::ui::consts::SCR_H - 316.0;
}

/// The **logo presence ladder** — how big a clearLogo is DRAWN. The third size axis of the design
/// system, beside [`size`] (type) and [`space`] (gaps), and it exists for the same reason: a logo
/// used to be sized by a per-screen literal (96 on the home hero, 120 on the detail hero, 54 on the
/// scrolled compact title), so the SAME mark was three sizes in one app.
///
/// **The rule is constant AREA, not constant height** ([`crate::ui::hero_logo::fit`]). A clearLogo's
/// aspect runs from ~1:1 (an emblem) to ~10:1 (a long wordmark); under a height clamp the drawn area
/// is LINEAR in aspect, so a 5:1 wordmark covered five times the ink of a 1:1 emblem — which is
/// exactly why square logos read as an afterthought. Solving for a target AREA instead makes a
/// square grow TALL and a wordmark grow WIDE, so both carry the same weight. The floor then stops a
/// very wide mark thinning to a pinstripe, the ceiling stops a taller-than-wide asset swallowing the
/// hero, and the caller's column contains the result last.
pub mod logo {
    /// HERO rung — the home hero's title band AND the detail hero's title band (one mark on two
    /// screens, one value; the user directive is "mutual logic for logo size for all hero"). The
    /// target ink in px², anchored on the shape this rule must NOT move: a 5:1 wordmark at the
    /// height floor, i.e. 600×120 — the detail hero's on-device-tuned size today. Every other
    /// aspect is solved from it, so the commonest logo shape is unchanged and only squarer ones grow.
    pub const HERO_AREA: f32 = 72_000.0;
    /// The shortest a hero logo is ever drawn, and the height everything ~5:1 and wider lands on.
    /// The detail hero's tuned 120; home's old 96 is retired, because two heroes drawing the same
    /// clearLogo must draw it the same size. Deliberately a step ABOVE one line of [`super::size::HERO`]
    /// text (72 × 1.32 ≈ 95): the logo is the brand mark and outranks the text title it stands in
    /// for. ALSO the LAYOUT height of the title band ([`crate::ui::hero_logo::band_h`]) — a taller
    /// logo spills upward as paint, never as layout.
    pub const HERO_H_MIN: f32 = 120.0;
    /// The tallest. = √[`HERO_AREA`], i.e. precisely where the area rule lands a 1:1 logo — so the
    /// ceiling never touches a real wordmark and bites only on a taller-than-wide asset. This is the
    /// ONE knob to pull down (to ~200) if a device capture says a square emblem shouts or crowds the
    /// detail page's compact title mid-scroll; do not reach for the area, which is what keeps
    /// wordmarks where they are.
    pub const HERO_H_MAX: f32 = 268.0;
    /// COMPACT rung — the pinned title the detail page scrolls up to. The hero rung scaled by
    /// ([`COMPACT_H_MIN`]/[`HERO_H_MIN`])² = 0.45², so the two rungs cannot drift apart: a 5:1
    /// wordmark lands on the floor at BOTH rungs, by construction.
    pub const COMPACT_AREA: f32 = 14_580.0;
    /// One line of [`super::size::TITLE`] text (40 × 1.32 ≈ 53) — the compact title's own text
    /// fallback, which is what this rung stands in for. Unchanged from the literal 54 it replaces,
    /// so a wordmark in the pinned strip is pixel-identical to today.
    pub const COMPACT_H_MIN: f32 = 54.0;
    /// Set by the BAND, not by the area rule (which would want 121 here): the pinned strip has to
    /// clear the screen top and stop short of `detail::TOP_MARGIN`, where the first scrolled
    /// section lifts to.
    ///
    /// Since 2026-08-23 the strip is solved from this number rather than fitted around it — "clear
    /// the screen top" now means clear the OVERSCAN frame (`consts::MARGIN_Y` 54), so
    /// `detail::COMPACT_TITLE_BOT` is `54 + this` and `TOP_MARGIN` is that plus 26px of air. Pulling
    /// this down (still the one knob to pull if a device capture says a square emblem crowds the
    /// pinned title) now lifts the whole strip with it instead of only shrinking the mark.
    pub const COMPACT_H_MAX: f32 = 72.0;
}

// ── Accent / control ─────────────────────────────────────────────────────────
/// **The focus fill — and the only fill a control can light up with.** `COOL_0`: the same near-white
/// [`TEXT_PRIMARY`] is, and the same the tab bar inks a selected label in, because a control lights
/// up for exactly one reason — the remote is on it. Nothing is filled by RANK, so there is no
/// always-filled primary CTA: the hero Play pill sits idle like everything else until focus reaches
/// it.
///
/// It was the mockup's warm "Snow" `#e9e6e0` until the 2026-08-13 palette sync, alongside a second,
/// cooler control white for the "primary" CTA (`FILL_PRIMARY`/`INK_ON_PRIMARY`, and
/// `ControlStyle::Primary` with them). At ten feet nobody could tell the two whites apart, so the
/// pair collapsed onto this one. Both survivors of that collapse are elsewhere and neither is a
/// control: the warm off-white is [`WASH_WARM`], a page ground; the cool plate is
/// [`SURFACE_QR_PLATE`], a scan surface.
///
/// **It stays white in the Litefin fork, on purpose.** Litefin fills a focused control with its
/// accent and inks it `#d4d5f7` (`docs/litefin-look.md` 2.6), but this role cannot carry that pair:
/// a KEYED focused face is solved near-white in OKLCH ([`CONTROL_FOCUS_FACE_L`] .972) and inked
/// [`ACCENT_INK`], so a lavender ink would vanish on it; [`ROW_VALUE_INK_ON`] is black on this
/// fill; and the unkeyed rim must be brighter than it ([`CONTROL_RIM_FOCUS_UNKEYED`]). Litefin's
/// pair is also 2.99:1 where this one is 18:1. So the accent is a NEW family - [`BRAND_FILL`] /
/// [`BRAND_INK`] for filled controls, [`FOCUS_RING`] for artwork - that a screen adopts per control,
/// with its own face variant (`widgets::ControlStyle`), rather than a repaint of every focus at once.
pub const ACCENT: [f32; 4] = COOL_0;
/// Near-black ink/glyphs drawn over [`ACCENT`].
pub const ACCENT_INK: [f32; 4] = NEUTRAL_950;
/// Alias for [`ACCENT_INK`] read from the "ink over accent" intent. The one legitimate alias in this
/// file: it is the same ROLE under a second name, not a second role that happens to share a stop.
pub const INK_ON_ACCENT: [f32; 4] = ACCENT_INK;
/// Fixed OKLCH lightness of a keyed focused control's lit top. The rendered ground contributes hue
/// and a measured fraction of chroma only; it may never make the focus face darker.
pub const CONTROL_FOCUS_FACE_L: f32 = 0.972;
/// Fraction of the ambient key's OKLCH chroma carried by a focused face. Above roughly .45 the
/// control reads as a coloured button instead of white material catching the page's light.
pub const CONTROL_FOCUS_AMBIENT_C: f32 = 0.36;
/// Pure OKLCH-lightness step from the focused face's lit top to its body. Both stops keep the same
/// hue and chroma, so the gradient reads as glare rather than grey laid over the tint.
pub const CONTROL_FOCUS_BODY_STEP: f32 = 0.06;
/// Fixed OKLCH lightness of an idle control face on a keyed page.
pub const CONTROL_IDLE_FACE_L: f32 = 0.215;
/// Fraction of the ambient key's OKLCH chroma carried by an idle keyed face.
pub const CONTROL_IDLE_AMBIENT_C: f32 = 0.45;
/// Opacity of an idle keyed face. Named separately because its RGB is derived from the local key.
pub const CONTROL_IDLE_FACE_A: f32 = 0.92;
/// Share of the focused top retained by the spent half of a countdown control; the balance is the
/// shelf ground, in sRGB just like the design system's `color-mix`.
pub const CONTROL_SPENT_FOCUS_W: f32 = 0.76;
/// The *spent* portion of a control that is counting down ([`Button::progress`](crate::ui::widgets::Button::progress)).
/// [`ACCENT`] mixed a quarter of the way to the shelf ([`SURFACE_APP`]), NOT a different hue: the
/// control stays focused-looking end to end, so the sweep reads as time passing rather than as the
/// focus state changing. Deliberately close enough to `ACCENT` that [`ACCENT_INK`] stays legible on
/// BOTH sides — the first version flipped the ink at the sweep line and cut the label in half
/// mid-word.
pub const CONTROL_SPENT_FILL: [f32; 4] = mix(COOL_0, NEUTRAL_500, 1.0 - CONTROL_SPENT_FOCUS_W);

/// A text field's **editing** ink while the television's keyboard is up: the sign-in inputs'
/// (`screens::jf_login`). Lands on `COOL_0`, the brightest ink stop short of pure white.
pub const FIELD_EDITING_INK: [f32; 4] = COOL_0;

/// Idle (unfocused) control disc/pill fill - solid dark, faintly translucent. Litefin's black theme
/// button bg `#222222` (`docs/litefin-look.md` 2.1), at the `.92` the destructive plate mixes from.
pub const CONTROL_IDLE_FILL: [f32; 4] = with_a(GREY_22, 0.92);
/// White glyph/label over an idle control. Both grounds ink it the same: the design system states
/// one `--control-idle-ink`, and the UNKEYED face below is a white film over a black ramp, which is
/// still a dark surface — see [`CONTROL_IDLE_FILL_UNKEYED`].
pub const CONTROL_IDLE_INK: [f32; 4] = WHITE;

// -- Litefin accent: brand fill, focus ring and border ------------------------
// `docs/litefin-look.md` 2.1 (values) and 2.6 (where they go). New roles beside [`ACCENT`] (see
// its doc for why that one stays white); every screen slice reads these and never adds its own.

/// **The accent fill** a Litefin control lights up with - the focused primary button, tab, chip,
/// settings category, OSD button and sidebar squircle (2.6 "Primary buttons"), and the details Play
/// button at rest. `#6b6ede`, "Litefin Purple Blue" (2.1 Accent).
pub const BRAND_FILL: [f32; 4] = VIOLET_500;
/// The accent under a pointer hover, lightness +10 (2.1 Accent hover). Also the far stop of the
/// card progress gradient ([`RESUME_FILL_END`]).
pub const BRAND_FILL_HOVER: [f32; 4] = VIOLET_400;
/// The accent while pressed, lightness -10 (2.1 Accent active).
pub const BRAND_FILL_ACTIVE: [f32; 4] = VIOLET_600;
/// Every glyph and label drawn ON [`BRAND_FILL`]: Litefin's `getContrastColor` lavender
/// `#d4d5f7` (2.1 Text on accent). The pair measures 2.99:1 - Litefin's own choice, used only for
/// the short bold labels and icons it puts on the accent.
pub const BRAND_INK: [f32; 4] = VIOLET_200;
/// The accent at .20 (2.1 Accent light): a tint, never a fill a label sits on alone.
pub const BRAND_TINT: [f32; 4] = with_a(VIOLET_500, 0.20);
/// A focused SETTINGS row: the accent at .15, no scale (2.6 "Rows of settings", 9 Setting row).
pub const BRAND_ROW_FOCUS: [f32; 4] = with_a(VIOLET_500, 0.15);
/// Litefin's accent-dark panel, `rgba(17,18,34,.65)` (2.1 Accent dark rgb): the season page's
/// episode rows and the focused rich-metadata table (5).
pub const BRAND_PANEL: [f32; 4] = with_a(VIOLET_950, 0.65);
/// [`BRAND_PANEL`] under focus, `.85` (5 Season page episodes).
pub const BRAND_PANEL_FOCUS: [f32; 4] = with_a(VIOLET_950, 0.85);
/// The SELECTED-but-not-focused segment - the active library tab, the active settings category, the
/// focused Quick Connect button: the lavender as the fill (6 Tabs, 8 Buttons, 9 Left category list).
pub const SELECTED_FILL: [f32; 4] = VIOLET_200;
/// The label on [`SELECTED_FILL`]: the accent as ink (same sections).
pub const SELECTED_INK: [f32; 4] = VIOLET_500;
/// An "active" toggle's icon at rest - watched, favourite, an OSD toggle that is on: the accent
/// as ink on an idle control (5 Actions row, 10 Buttons).
pub const BRAND_ACTIVE_INK: [f32; 4] = VIOLET_500;

/// **The artwork focus ring** - a media card's 3px ring turns `#6b6ede` on focus (2.1 Card focus
/// ring, 2.6 Media cards), drawn with `Painter::rring` around the image. No glow, no shadow.
pub const FOCUS_RING: [f32; 4] = VIOLET_500;
/// [`FOCUS_RING`]'s width (2.6 Media cards: "a permanent 3px transparent border").
pub const FOCUS_RING_W: f32 = 3.0;
/// **The generic focus border** - `.btn`, text inputs and episode cards turn their border lavender
/// `#d4d5f7` on focus (2.1 Focus border, 2.6 Generic `.btn`); also the sidebar avatar's 2px ring (3).
pub const FOCUS_BORDER: [f32; 4] = VIOLET_200;
/// [`FOCUS_BORDER`]'s width on a button or input (2.6).
pub const FOCUS_BORDER_W: f32 = 3.0;
/// [`FOCUS_BORDER`]'s width on an episode card and on the login buttons and inputs (2.6, 8).
pub const FOCUS_BORDER_W_WIDE: f32 = 4.0;

/// Litefin's corner pair, `--jf-border-radius` / `--jf-border-radius-lg` with rounded corners on
/// (2.3). Cards themselves are [`CARD_RING_RAD`] / [`CARD_FOCUS_RING_RAD`].
pub mod radius {
    /// Cards' outer box, setting rows, small chips, the sidebar label pill, tech pills (2.3).
    pub const BASE: f32 = 12.0;
    /// Buttons, pills, badges and menus (2.3).
    pub const LG: f32 = 16.0;
}

/// **Litefin's left rail** (`docs/litefin-look.md` 3, modern-collapsed): a transparent 100px icon
/// rail on every page, the label shown only as a tooltip pill beside the focused item. Geometry
/// and inks the sidebar slice draws with; [`crate::ui::consts::CONTENT_LEFT`] is the content edge
/// it leaves.
pub mod sidebar {
    use super::{with_a, COOL_250, COOL_870, VIOLET_200, VIOLET_500, WHITE};

    /// The visible rail, x 0..100 (2.4 Sidebar rail width, 3 Geometry).
    pub const RAIL_W: f32 = 100.0;
    /// Padding above the logo header and below the footer (3).
    pub const PAD_Y: f32 = 20.0;
    /// The logo header's height and the gap under it (3: 80 tall + 20 margin).
    pub const HEADER_H: f32 = 80.0;
    pub const HEADER_GAP: f32 = 20.0;
    /// The logo, drawn 40x40 centred at x=50 (3).
    pub const LOGO: f32 = 40.0;
    /// One nav item's height; the block of items is centred between header and footer (3).
    pub const ITEM_H: f32 = 72.0;
    /// The item icon, 28x28 centred at x=50 (3).
    pub const ICON: f32 = 28.0;
    /// The squircle behind an active or focused icon: 56x56, radius 18 (2.3, 3 States).
    pub const SQUIRCLE: f32 = 56.0;
    pub const SQUIRCLE_RAD: f32 = 18.0;
    /// The focused icon and squircle pop, 250 ms overshoot (3, 2.5).
    pub const FOCUS_SCALE: f32 = 1.08;
    /// The footer avatar: a 40x40 circle in a 72-tall item, a 2px [`super::FOCUS_BORDER`] ring on
    /// focus (3).
    pub const AVATAR: f32 = 40.0;
    pub const AVATAR_RING_W: f32 = 2.0;

    /// Idle icon `#bcbebf`, no squircle (3 States, 2.1 Sidebar unselected icon).
    pub const ICON_IDLE: [f32; 4] = COOL_250;
    /// The current page's icon, filled white (3, 2.1 Sidebar selected icon).
    pub const ICON_ACTIVE: [f32; 4] = WHITE;
    /// The focused icon, filled lavender on the accent squircle (3).
    pub const ICON_FOCUS: [f32; 4] = VIOLET_200;
    /// The current page's squircle, white .14 with a 1px white .08 edge (3).
    pub const SQUIRCLE_ACTIVE: [f32; 4] = with_a(WHITE, 0.14);
    pub const SQUIRCLE_ACTIVE_RIM: [f32; 4] = with_a(WHITE, 0.08);
    /// The focused squircle, the accent with a 1px white .25 edge (3).
    pub const SQUIRCLE_FOCUS: [f32; 4] = VIOLET_500;
    pub const SQUIRCLE_FOCUS_RIM: [f32; 4] = with_a(WHITE, 0.25);

    /// The focused item's label pill: left edge at x=92, centred on the item, padding 8x18,
    /// radius 12, at most 300 wide, one line (3). Its text is 16.8px weight 600 in Litefin, which
    /// is below `size::CAPTION`; the slice picks the rung.
    pub const LABEL_X: f32 = 92.0;
    pub const LABEL_PAD_X: f32 = 18.0;
    pub const LABEL_PAD_Y: f32 = 8.0;
    pub const LABEL_RAD: f32 = 12.0;
    pub const LABEL_MAX_W: f32 = 300.0;
    /// The label pill's fill, edge and ink: accent, white .25, lavender (3).
    pub const LABEL_FILL: [f32; 4] = VIOLET_500;
    pub const LABEL_RIM: [f32; 4] = with_a(WHITE, 0.25);
    pub const LABEL_INK: [f32; 4] = VIOLET_200;

    /// The Libraries popover: left edge x=92, 270 wide, radius 18, padding 8, rows 48 tall at
    /// radius 12 (3).
    pub const POPOVER_X: f32 = 92.0;
    pub const POPOVER_W: f32 = 270.0;
    pub const POPOVER_RAD: f32 = 18.0;
    pub const POPOVER_PAD: f32 = 8.0;
    pub const POPOVER_ROW_H: f32 = 48.0;
    pub const POPOVER_ROW_RAD: f32 = 12.0;
    /// Its fill `#18191c`, 1px white .18 edge, the active row's white .10 (3).
    pub const POPOVER_FILL: [f32; 4] = COOL_870;
    pub const POPOVER_RIM: [f32; 4] = with_a(WHITE, 0.18);
    pub const POPOVER_ROW_ACTIVE: [f32; 4] = with_a(WHITE, 0.10);
    /// Its row text, white .90 (3).
    pub const POPOVER_INK: [f32; 4] = with_a(WHITE, 0.90);
    /// The focused row: accent fill, lavender ink, a 1.02 pop (3).
    pub const POPOVER_ROW_FOCUS: [f32; 4] = VIOLET_500;
    pub const POPOVER_INK_FOCUS: [f32; 4] = VIOLET_200;
    pub const POPOVER_FOCUS_SCALE: f32 = 1.02;
    /// A row's side padding, its 28px library icon and the gap to the name (3).
    pub const POPOVER_ROW_PAD_X: f32 = 14.0;
    pub const POPOVER_ICON: f32 = 28.0;
    pub const POPOVER_ICON_GAP: f32 = 12.0;
    /// Its top sits 6 above the Libraries item, kept 10 from the top and 20 from the bottom of
    /// the screen (`Sidebar.js:_toggleFloatingLibraries`).
    pub const POPOVER_NUDGE: f32 = 6.0;
    pub const POPOVER_EDGE_TOP: f32 = 10.0;
    pub const POPOVER_EDGE_BOTTOM: f32 = 20.0;
    /// It opens from scale 0.92, and the label pill from 0.9, both with the overshoot (3).
    pub const POPOVER_OPEN_FROM: f32 = 0.92;
    pub const LABEL_OPEN_FROM: f32 = 0.9;
    /// The label pill's line box: 16.8px text at Litefin's 1.5 line height (2.2).
    pub const LABEL_LINE_H: f32 = 25.2;
    /// The avatar's ground while no picture has landed: the action fill white .10 (2.1).
    pub const AVATAR_FILL: [f32; 4] = with_a(WHITE, 0.10);

    /// The global clock, top 32 / right 50, 22.4px at white .90 on a 1.5 line box (4.2).
    pub const CLOCK_TOP: f32 = 32.0;
    pub const CLOCK_RIGHT: f32 = 50.0;
    pub const CLOCK_LINE_H: f32 = 33.6;
    pub const CLOCK_INK: [f32; 4] = with_a(WHITE, 0.90);
}

/// **Litefin's `.page-error` box** (`styles/components.css`), drawn by `widgets::PageError` where a
/// page has nothing to show (the Favorites page today): `rgba(220,53,69,.1)` inside a 1px
/// `#ff6b6b` border, the text `#ff6b6b`. Litefin's look, so it is red where upstream's failure
/// read-outs are not.
pub mod page_error {
    use super::{with_a, RED_300, RED_600};

    pub const FILL: [f32; 4] = with_a(RED_600, 0.10);
    /// The border and the text.
    pub const INK: [f32; 4] = RED_300;
}

/// **Litefin's player OSD** (`docs/litefin-look.md` 10): two black gradients, icon-only circle
/// buttons that fill with the accent on focus, and an accent seek fill.
pub mod osd {
    use super::{rgb8, scrim_black, with_a, VIOLET_200, VIOLET_500, WHITE};

    /// The header and bottom gradients start at black .75 and run to transparent, downward from
    /// the top edge and upward from the bottom edge (10).
    pub const GRADIENT_FROM: [f32; 4] = scrim_black(0.75);
    pub const GRADIENT_TO: [f32; 4] = scrim_black(0.0);
    /// Header padding 38.4 a side, 76.8 below (10).
    pub const HEADER_PAD: f32 = 38.4;
    pub const HEADER_PAD_BOTTOM: f32 = 76.8;
    /// Bottom area padding: 160 above the controls, 38.4 a side, 48 below (10).
    pub const BOTTOM_PAD_TOP: f32 = 160.0;
    pub const BOTTOM_PAD_X: f32 = 38.4;
    pub const BOTTOM_PAD_BOTTOM: f32 = 48.0;

    /// A control: a 58px circle with a 33.6px icon, 11.52 apart (10 Buttons, Controls row).
    pub const BUTTON_D: f32 = 58.0;
    pub const BUTTON_ICON: f32 = 33.6;
    pub const BUTTON_GAP: f32 = 11.52;
    /// Play/pause: an 86.4px circle with a 48px icon, 11.52 extra margin each side (10).
    pub const PLAY_D: f32 = 86.4;
    pub const PLAY_ICON: f32 = 48.0;
    pub const PLAY_MARGIN: f32 = 11.52;
    /// Focus pops the circle 1.08 and the icon inside it a further 1.12 (10).
    pub const FOCUS_SCALE: f32 = 1.08;
    pub const ICON_FOCUS_SCALE: f32 = 1.12;
    /// An idle button is the white icon alone (icon-only, 10).
    pub const BUTTON_IDLE_INK: [f32; 4] = WHITE;
    /// A focused button: accent fill, lavender icon, no border (10).
    pub const BUTTON_FOCUS_FILL: [f32; 4] = VIOLET_500;
    pub const BUTTON_FOCUS_INK: [f32; 4] = VIOLET_200;
    /// A toggle that is on shows its icon in the accent (10).
    pub const BUTTON_ACTIVE_INK: [f32; 4] = VIOLET_500;
    /// A disabled chapter button sits at .30 (10).
    pub const BUTTON_DISABLED_A: f32 = 0.30;

    /// The seek track: 8 tall, white .30, radius 12, 20 from the times (10 Seek row).
    pub const SEEK_TRACK_H: f32 = 8.0;
    pub const SEEK_TRACK: [f32; 4] = with_a(WHITE, 0.30);
    pub const SEEK_RAD: f32 = 12.0;
    pub const SEEK_TIME_GAP: f32 = 20.0;
    /// The played part, the accent (10 Seek row; 0 OSD progress accent).
    pub const SEEK_FILL: [f32; 4] = VIOLET_500;
    /// The thumb, 8x8 white; a 10x28 white bar while the seek bar has focus (10).
    pub const THUMB: f32 = 8.0;
    pub const THUMB_FOCUS_W: f32 = 10.0;
    pub const THUMB_FOCUS_H: f32 = 28.0;
    pub const THUMB_INK: [f32; 4] = WHITE;
    /// The time read-outs: at least 80 wide, 36 tall (10).
    pub const TIME_MIN_W: f32 = 80.0;
    pub const TIME_H: f32 = 36.0;
    /// The track menus' full-screen dim, black .85 (10 Track menus, `osdTrackMenuBgOpacity`).
    pub const TRACK_MENU_DIM: f32 = 0.85;
    /// The track menus' list: centred, at least 750 wide, at most 90 % of the width and 70 % of
    /// the height (10 Track menus).
    pub const TRACK_MENU_MIN_W: f32 = 750.0;
    pub const TRACK_MENU_MAX_W: f32 = crate::ui::consts::SCR_W * 0.90;
    pub const TRACK_MENU_MAX_H: f32 = crate::ui::consts::SCR_H * 0.70;

    /// The title's secondary run, the clock and "Ends at": white at .90 (10 Header, Controls row).
    pub const TEXT_SOFT: [f32; 4] = with_a(WHITE, 0.90);
    /// Chapter ticks, 4 x 13 in the track colour (10 Seek row).
    pub const TICK_W: f32 = 4.0;
    pub const TICK_H: f32 = 13.0;
    /// The seek tooltip: the gradient's black at .90 inside a 2px white .20 border, 14 above the
    /// seek row, its trickplay still clipped at radius 6 (10 Seek row, `.osd-seek-tooltip`).
    pub const TOOLTIP_FILL: [f32; 4] = scrim_black(0.90);
    pub const TOOLTIP_BORDER: [f32; 4] = with_a(WHITE, 0.20);
    pub const TOOLTIP_BORDER_W: f32 = 2.0;
    pub const TOOLTIP_GAP: f32 = 14.0;
    pub const TOOLTIP_THUMB_RAD: f32 = 6.0;

    /// The Up Next card's and Skip Intro's text buttons at rest: a white .10 fill, a 2.5px white
    /// .20 border, .80 ink (10 Up Next, `styles/upnext.css` `.upnext-btn`).
    pub const PILL_FILL: [f32; 4] = with_a(WHITE, 0.10);
    pub const PILL_INK: [f32; 4] = with_a(WHITE, 0.80);
    pub const PILL_BORDER: [f32; 4] = with_a(WHITE, 0.20);
    pub const PILL_BORDER_W: f32 = 2.5;
    /// A focused secondary pill (Hide): white .22 under a white border.
    pub const PILL_FOCUS_FILL: [f32; 4] = with_a(WHITE, 0.22);
    pub const PILL_FOCUS_BORDER: [f32; 4] = WHITE;

    /// The Up Next card: `rgba(12,12,18,.92)` behind a 1px white .15 border (10 Up Next).
    const CARD_INK: [f32; 4] = rgb8(12, 12, 18);
    pub const CARD_FILL: [f32; 4] = with_a(CARD_INK, 0.92);
    pub const CARD_BORDER: [f32; 4] = with_a(WHITE, 0.15);
    /// The still's ground while it loads, and the episode badge over it.
    pub const CARD_THUMB_FILL: [f32; 4] = with_a(WHITE, 0.06);
    pub const CARD_BADGE_FILL: [f32; 4] = scrim_black(0.70);
    /// "UP NEXT" in the accent at .90; the countdown in white .55.
    pub const CARD_LABEL: [f32; 4] = with_a(VIOLET_500, 0.90);
    pub const CARD_COUNTDOWN: [f32; 4] = with_a(WHITE, 0.55);
}

// ── The UNKEYED ground: a control standing on the VIDEO PLANE ────────────────
// Everything above assumes the ground can be SAMPLED — the page read its own artwork and a control
// answers to it. The player cannot: the picture lives on the hardware video plane, never enters our
// framebuffer, cannot be dimmed by a scrim and changes every frame. So the player HUD declares
// itself `ControlGround::Unkeyed` (`widgets::ControlGround`) and the roles below replace those the
// keyed ground supplies. The design system says the same thing as a CSS scope
// (`[data-ground="unkeyed"]` in `tokens/colors.css`), which is why these are tokens and not
// call-site literals.

/// The idle control face on the **unkeyed** ground — a LIGHT FILM where [`CONTROL_IDLE_FILL`] is a
/// dark plate, and the polarity is settled the hard way round.
///
/// A control over video does NOT float on a bare frame: the HUD lays its own black ramp over the
/// picture first, so the ground under it is reliably DARK even though the picture itself is
/// unreadable. A white film at a low alpha is therefore always LIGHTER than the scrim beneath it —
/// the chip reads as an object catching light rather than a hole punched in the frame, and the
/// picture stays faintly alive through it. The dark plate cannot promise either: over a snow frame
/// it is the hole, and the ramp is the only thing that makes any of this decidable at all.
///
/// **Legal only under that ramp.** A control on bare video would have to bring the scrim itself.
///
/// Same stop as [`HAIRLINE`] and deliberately its own role — retuning a divider must not restyle
/// every control the player draws.
pub const CONTROL_IDLE_FILL_UNKEYED: [f32; 4] = with_a(WHITE, 0.10);
/// The idle film's edge on the unkeyed ground. The keyed card constant ([`CARD_SHEEN`], .22 at
/// 1px) left only a faint alpha step over the .10 film and disappeared in the TV panel's video
/// composition. This edge keeps the same white material and 1.25px geometry as unkeyed focus, but
/// at .34 remains clearly subordinate to focus's pure-white perimeter.
pub const CONTROL_RIM_IDLE_UNKEYED: [f32; 4] = with_a(WHITE, 0.34);
/// The **focused** control's edge on the unkeyed ground — the one place in the app a rim goes to
/// pure white instead of [`CARD_SHEEN`]'s .22.
///
/// The focused face is [`ACCENT`], a near-white capsule, and the brightest ground the app can be
/// asked to sit on is a snow frame: the line has to stay brighter than BOTH, and light is the only
/// thing the two polarities of an unknown frame agree on. It is not an ink line — an even dark ring
/// reads as a moulded plastic edge, i.e. volume drawn ON the control rather than the control's own
/// boundary.
///
pub const CONTROL_RIM_FOCUS_UNKEYED: [f32; 4] = WHITE;
/// The unkeyed stroke width — a quarter px over [`CARD_SHEEN_W`], the design's
/// `inset 0 0 0 1.25px`. Both states keep one geometry; state changes brightness, not shape.
/// Nothing brighter than white is available to boost the crown with, which is why the unkeyed rim
/// asks for no top weight and carries its light INWARD instead — see
/// [`CONTROL_RIM_FOCUS_UNKEYED_GLOW`].
pub const CONTROL_RIM_FOCUS_UNKEYED_W: f32 = 1.25;
/// **The unkeyed focused face's ÉTLIV** — the light that spills inward off that pure-white line, as
/// `[top depth px, top weight, bottom depth px, bottom weight]`.
///
/// The design states it as two inset shadows either side of the perimeter —
/// `inset 0 5px 6px -3px white` and `inset 0 -3px 5px -3px white .55` — i.e. a strong fall from the
/// top edge and a weaker one from the bottom, over a face that is near-white already. It is what
/// stops the capsule reading as a flat chip when the frame behind it is unknown: an edge alone says
/// where the object stops, and this says the object has a TOP. The weights are the design's; the
/// depths are its blur radii read as how far the light reaches in, which is what an SDF falloff can
/// express and a CSS spread cannot be transcribed into.
pub const CONTROL_RIM_FOCUS_UNKEYED_GLOW: [f32; 4] = [5.0, 1.0, 2.5, 0.55];

/// **THE FOCUS CAST** — what a control face casts once the remote is on it, as two stops of
/// `(dy, blur, alpha)` over [`CARD_SHADOW`]'s black.
///
/// At REST a control face casts NOTHING: it is held by its edge (`widgets::control_rim`), and this
/// file's history is emphatic that two elevations for one control family is what the design system
/// removed. That rule survives — what changed is that FOCUS is an elevation. "The remote is here"
/// is then fill, scale AND lift, none of which depends on the ground being readable, which is the
/// property the player needed and the one a pure fill cannot have over an unknown frame.
///
/// The design writes it as `drop-shadow(0 5px 16px black .34) drop-shadow(0 1px 3px black .20)`.
/// **A drop-shadow's blur is a standard deviation where a box-shadow's is a diameter**, and this
/// renderer's is the latter, so 16 and 3 arrive here doubled — transcribing the CSS numbers
/// literally would draw a shadow half the size the design asks for.
pub const CONTROL_CAST_FOCUS: [(f32, f32, f32); 2] = [(5.0, 32.0, 0.34), (1.0, 6.0, 0.20)];

// ── Surfaces / backgrounds ───────────────────────────────────────────────────
/// Flat shelf/app base (both gradient stops) - Litefin's black theme page `#000000`
/// (`docs/litefin-look.md` 2.1; also its sidebar and navbar background). Upstream's was Apple TV's
/// shelf gray `#2C2C2E`, chosen so a focused card's drop shadow had something to darken; Litefin
/// focuses with a ring instead and draws no shadow at all (2.6).
pub const SURFACE_APP: [f32; 4] = NEUTRAL_500;
/// Opaque sheet drawn over the hardware video plane when a detail page scrolls the hero away.
/// Same stop as [`SURFACE_APP`] so the rising page and the cover are one surface. Different role:
/// this hides the plane. It is not atmosphere, so [`scrim`] is the wrong token.
pub const PLANE_COVER: [f32; 4] = SURFACE_APP;
// GL clear color — 3-float (`frame_clear` takes r,g,b, no alpha): [`SURFACE_APP`] itself (both are
// the `NEUTRAL_500` stop), defined in `gfx::tokens` because `gfx` clears with it.
pub use nj_gfx::gfx::tokens::CLEAR_RGB;
/// Opaque menu panel / fade mask / badge knockout interior. Litefin's one floating panel in the black
/// theme, the sidebar's libraries popover `#18191c` (`docs/litefin-look.md` 3); flat, because the
/// black theme has no gradient surface anywhere (2.1).
pub const SURFACE_PANEL: [f32; 4] = COOL_870;
/// Near-opaque sheet/card gradient — top stop. [`SURFACE_PANEL`]'s own stop at .985, so the sheet
/// and its opaque twin are one material rather than two greys a hair apart.
pub const PANEL_TOP: [f32; 4] = with_a(COOL_870, 0.985);
/// Near-opaque sheet gradient - bottom stop. The same stop as the top in the Litefin fork: the
/// black theme's surfaces are flat fills (`docs/litefin-look.md` 2.1).
pub const PANEL_BOT: [f32; 4] = with_a(COOL_870, 0.985);
/// **The material scale, named rather than numbered — SwiftUI's `Material` is the vocabulary.**
///
/// Every glass surface in this app used to be described by two unrelated numbers written at its own
/// call site: a frost alpha in `theme` and, once the bar's blur was lightened, a sample radius in
/// `gfx`. Two numbers for one idea drift, and the drift is invisible — a panel can end up dense and
/// crisp, or thin and soft, neither of which is a material anybody chose.
///
/// So it is one named thing with two halves, and the halves move together. The names mean what they
/// mean in SwiftUI: how much of the material there is between the eye and the page.
///
/// **Why a menu is not a bar.** The blur chain produces ONE snapshot per frame, shared by every
/// surface, so lightening it so the tab bar can pass a television's picture through lightened the
/// menus with it — the wrong direction for a menu. The reference settles which way each goes: in one
/// screenshot of iOS 26's TV app, the posters behind the tab BAR are readable and behind the context
/// MENU directly above it almost nothing is. A menu is a surface you read and act on and earns its
/// opacity; a bar is chrome you look past.
///
/// The second half is a wider RE-SAMPLE of the shared snapshot — four extra bilinear fetches on a
/// plus-cross, confined to the surface's own fragments — rather than a second chain, which would
/// mean another pair of targets and another set of passes for a surface that is on screen a few
/// seconds at a time. `UltraThin`'s zero restores the single fetch exactly, which is what the tab
/// track takes.
///
/// **The track's frost is NOT read from here** and cannot be: it is solved every frame against the
/// ground so its labels clear their contrast (`widgets::track_alpha_for`). The track takes this
/// scale only for its sample radius. Everything else — panels, sheets, the loading capsule — takes
/// both halves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Material {
    /// The bar's own: no extra sample at all, so the page comes through as sharp as the chain left it.
    UltraThin,
    Thin,
    /// A sheet: readable through, but no longer a window.
    Regular,
    /// A menu — what a popover takes. Dense enough to read against, soft enough not to compete.
    Thick,
    UltraThick,
}

impl Material {
    /// The frost alpha a surface that draws its own material over the backdrop uses.
    pub const fn frost(self) -> f32 {
        match self {
            Self::UltraThin => 0.28,
            Self::Thin => 0.45,
            Self::Regular => 0.60,
            Self::Thick => 0.72,
            Self::UltraThick => 0.85,
        }
    }

    /// The extra sample radius in authored px — see the note above.
    pub const fn deep(self) -> f32 {
        match self {
            Self::UltraThin => 0.0,
            Self::Thin => 1.5,
            Self::Regular => 3.0,
            Self::Thick => 5.0,
            Self::UltraThick => 8.0,
        }
    }

    /// `/tmp/nativejelly-material=<ultrathin|thin|regular|thick|ultrathick>` — the panel's material,
    /// swept. Absent, [`PANEL_MATERIAL`] stands.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "ultrathin" => Self::UltraThin,
            "thin" => Self::Thin,
            "regular" => Self::Regular,
            "thick" => Self::Thick,
            "ultrathick" => Self::UltraThick,
            _ => return None,
        })
    }
}

/// What a popover is made of. The bar is [`Material::UltraThin`] by construction — it takes no extra
/// sample — and solves its own frost; everything else states its material here.
///
/// **`UltraThick`, chosen on a real poster rather than a synthetic ground**, because the thing a
/// menu has to survive is exactly a busy one: laddered over a shelf of artwork, the row ink runs
/// 7.2 : 8.7 : 10.3 : 11.8 : 13.5 to one across the five, and what the eye reads is where the
/// artwork stops competing with the words. At `Thick` a poster's title lettering still comes
/// through under "Remove from Deck"; at `UltraThick` the rows are clean while the artwork's shapes
/// are still there, so it reads as glass rather than as paint. The reference agrees — iOS's own
/// context menu passes almost nothing of the page behind it.
///
/// **A panel spends only the `frost()` half now.** Its ground is the latched underlay field
/// (`widgets::panel_ground`), not a backdrop blur, so `deep()` has nothing to re-sample under it;
/// the density over the field is this material's frost, and it is what keeps the words clean.
pub const PANEL_MATERIAL: Material = Material::UltraThick;
// The five `MODAL_*` tokens that stood here — `MODAL_SAMPLE_MATERIAL`, `MODAL_FROST_ALPHA`,
// `MODAL_BLUR_TAPS`, `MODAL_BLUR_TINT`, `MODAL_BLUR_SATURATION` — were the material of a
// full-screen BLURRED modal ground (`Glass::modal_ground` → `Painter::backdrop_blur_flat` →
// `gfx::draw_blur_snapshot_flat` → `shaders/fs_modal_ground.frag`). That chain had no live caller
// left and was deleted whole; a route ground is an `AmbientWash` (see `ui::route_screen`) and,
// from PR2, an `ui::underlay::UnderlayField`, neither of which is a blur. Do not re-add the tokens
// without the surface that reads them.

#[cfg(test)]
mod material_tests {
    use super::Material::*;
    use super::*;

    /// **The scale has to be a scale.** Both halves are hand-picked numbers in a `match`, and a
    /// `Thin` denser than `Regular` — or softer than `UltraThin` — is a silent design error: nothing
    /// fails, the name simply stops meaning what it says, and every call site that reads as a
    /// deliberate choice becomes a lie. The ORDER is the contract; the values are taste.
    #[test]
    fn the_material_scale_only_ever_thickens() {
        let ladder = [UltraThin, Thin, Regular, Thick, UltraThick];
        for w in ladder.windows(2) {
            assert!(
                w[1].frost() > w[0].frost(),
                "{:?} must be denser than {:?} ({} vs {})",
                w[1],
                w[0],
                w[1].frost(),
                w[0].frost()
            );
            assert!(
                w[1].deep() > w[0].deep(),
                "{:?} must be softer than {:?} ({} vs {})",
                w[1],
                w[0],
                w[1].deep(),
                w[0].deep()
            );
        }
        // The bar's end of the scale takes NO extra sample: `fs_glass.frag` skips the four extra
        // fetches entirely at zero, which is what makes "the tab track costs nothing for this"
        // true rather than approximately true.
        assert_eq!(
            UltraThin.deep(),
            0.0,
            "the thinnest material must be the single-fetch one"
        );
    }

    /// Every name the sweep accepts round-trips, so `nativejelly-material` cannot silently fall back
    /// to the default on a typo it half-recognises.
    #[test]
    fn every_material_parses_from_its_own_name() {
        for m in [UltraThin, Thin, Regular, Thick, UltraThick] {
            let name = format!("{m:?}").to_ascii_lowercase();
            assert_eq!(Material::parse(&name), Some(m), "{name}");
        }
        assert_eq!(
            Material::parse("  ThIcK  "),
            Some(Thick),
            "trimmed and case-folded"
        );
        assert_eq!(
            Material::parse("thickish"),
            None,
            "a near miss is a refusal, not the default"
        );
    }

}
/// The FROSTED sheet's gradient — top stop: the same two greys as [`PANEL_TOP`]/[`PANEL_BOT`] at
/// a lower alpha, and the same material as far as the palette is concerned. The alpha actually
/// drawn is [`PANEL_MATERIAL`]'s `frost()` (`widgets::panel_frost`); this pair supplies the hue.
///
/// It is a separate pair rather than a lower alpha passed at the call site because the two are not
/// interchangeable: `.985` over an unknown background is the panel *being* its own ground, and this
/// one is only legal over an OPAQUE ground of its own — the latched underlay field
/// (`underlay::UnderlayField::draw_panel`), or on the chrome a live backdrop blur. A panel that
/// draws it over neither is a translucent hole onto whatever the page had there;
/// [`widgets::panel_ground`](crate::ui::widgets::panel_ground) is what keeps the two paired, and
/// falls back to [`PANEL_TOP`]/[`PANEL_BOT`] when the field is not latched.
///
/// The floor is legibility, not taste: the fine print's contrast over the composite at a white and
/// a black underlay is graded in `underlay_tests.rs`
/// (`text_contrast_floors_hold_over_a_bright_and_a_dark_underlay`).
pub const PANEL_FROST_TOP: [f32; 4] = with_a(COOL_870, 0.72);
/// The frosted sheet's bottom stop - see [`PANEL_FROST_TOP`] (flat, as [`PANEL_BOT`] is).
pub const PANEL_FROST_BOT: [f32; 4] = with_a(COOL_870, 0.72);
/// The sign-in **QR card's** quiet-zone plate (`login.rs`) — a bright cool-white whose job is SCAN
/// CONTRAST for a phone camera, not focus. It is a SURFACE, not a control fill, which is the whole
/// reason it outlived the second control white it used to be (`FILL_PRIMARY`): a plate under a
/// black-tinted QR texture, and the only place in the app where the app paints something this
/// bright on purpose.
pub const SURFACE_QR_PLATE: [f32; 4] = COOL_0;
/// Poster/thumb skeleton flat placeholder: Litefin's `--jf-surface` `#0a0a0a`, the card image's
/// background while it loads (`docs/litefin-look.md` 2.1, 4.2).
pub const CARD_PLACEHOLDER: [f32; 4] = GREY_0A;
/// Loading-skeleton gradient - top. The same stop as [`CARD_PLACEHOLDER`]: a card whose artwork is
/// on the way and one with none to load are the same object at rest.
pub const SKELETON_TOP: [f32; 4] = GREY_0A;
/// Loading-skeleton gradient - bottom. Flat like the top: Litefin's loading fill is one code (4.2).
pub const SKELETON_BOT: [f32; 4] = GREY_0A;
/// A card's own box under its image - the details poster's loading fill and any card body: Litefin's
/// `--jf-card-bg` `#080808` (`docs/litefin-look.md` 2.1, 5 Poster).
pub const CARD_BG: [f32; 4] = GREY_08;
/// [`CARD_BG`] under a pointer hover, `#121212`; also the focused secondary login button (2.1, 8).
pub const CARD_BG_HOVER: [f32; 4] = GREY_12;

// Litefin's flat control surfaces (`docs/litefin-look.md` 2.1), for the slices that restyle buttons,
// inputs and toggles. Overlays stay weights on the white ramp, as everywhere in this file.
/// The black theme's border `#222222` (2.1 Border); also an input's border (2.1 Input bg / border).
pub const BORDER: [f32; 4] = GREY_22;
/// The lighter border `#2a2a2a` (2.1 Border light); also a solid button's hover fill (2.1 Button
/// bg / hover).
pub const BORDER_LIGHT: [f32; 4] = GREY_2A;
/// A text input's fill, `#181818` (2.1 Input bg).
pub const INPUT_FILL: [f32; 4] = GREY_18;
/// An action button at rest - the details icon circles, library tabs and controls, the search
/// field: white .10 with a 2px white .20 border and a white label (2.1 Action button, 5, 6, 7).
pub const ACTION_FILL: [f32; 4] = with_a(WHITE, 0.10);
pub const ACTION_BORDER: [f32; 4] = with_a(WHITE, 0.20);
/// The border [`ACTION_BORDER`] is drawn at (5 Actions row, 7 Input: 2px).
pub const ACTION_BORDER_W: f32 = 2.0;
/// A toggle's track while off, white .15 (2.1 Switch track, 9 Toggle).
pub const SWITCH_TRACK_OFF: [f32; 4] = with_a(WHITE, 0.15);
/// A full-screen overlay's black, `rgba(0,0,0,.95)` (2.1 Overlay bg), as a dim weight.
pub const OVERLAY_DIM_A: f32 = 0.95;

// ── Scrims (near-black; alpha supplied per call) ─────────────────────────────
// Hero/scroll scrim ink; use via [`scrim`]. Defined in `gfx::tokens` (the renderer paints with it).
pub use nj_gfx::gfx::tokens::SCRIM_INK;
/// Pure-black scrim ink (HUD bottom, subtitle outline, modal); use via [`scrim_black`].
pub const SCRIM_BLACK_INK: [f32; 3] = [BLACK[0], BLACK[1], BLACK[2]];

/// The **text-legibility floor** for copy drawn straight onto ARTWORK: the scrim alpha at which
/// [`TEXT_PRIMARY`]/[`TEXT_SECONDARY`] hold ≥4.5:1 over a bright backdrop (an encoded 0.85
/// highlight, brighter than essentially any fanart pixel) and ≥3:1 over a blown-white one.
///
/// MEASURED, not chosen. The panel is plain 888 with no sRGB framebuffer anywhere in the tree, so a
/// token value IS an sRGB code and GL's blend is a straight lerp in that space — which makes the
/// whole derivation arithmetic, so brightening a text token means re-deriving this.
///
/// The same weight the tab bar's own track capsule already uses for the same job over the same
/// artwork ([`TAB_TRACK_A_TOP`]) — one value per role, arrived at independently twice. An alpha
/// rather than a colour, like [`CARD_SHADOW_REST_A`]: the ink is [`SCRIM_INK`], only the weight is
/// the decision. **This is the one knob** if the treatment reads heavy — 0.60 still clears 3:1
/// everywhere.
pub const SCRIM_TEXT_A: f32 = 0.72;

/// **White overlay at alpha `a`** — the ramp every overlay on this palette rides.
///
/// The palette's rule is that an overlay is a WEIGHT, never a new hue (`with_a(WHITE, a)`), and
/// `WHITE` is a private primitive; this is that expression for callers outside this module which
/// genuinely need a weight rather than a named role. Reach for a named token first — a shade with
/// a JOB belongs in the role layer above, not at a call site.
pub const fn white(a: f32) -> [f32; 4] {
    with_a(WHITE, a)
}

/// Near-black scrim at alpha `a` — hero/scroll dimming.
pub const fn scrim(a: f32) -> [f32; 4] {
    [SCRIM_INK[0], SCRIM_INK[1], SCRIM_INK[2], a]
}

/// Pure-black scrim at alpha `a` — HUD/modal dimming.
pub const fn scrim_black(a: f32) -> [f32; 4] {
    [
        SCRIM_BLACK_INK[0],
        SCRIM_BLACK_INK[1],
        SCRIM_BLACK_INK[2],
        a,
    ]
}

/// **The modal DIM — the one weight table every overlay's dim is read from.**
///
/// A dim is not a black sheet here: it is the page's own light, pushed down. Every surface that
/// dims its host paints it through ONE field (`ui::underlay::UnderlayField`, owned by the
/// container — `ModalStack`'s underlay), as `Role::Dim { weight: TINT }` at the alpha below times
/// the surface's appear spring and the route dip. So green under a panel stays green, and stays
/// where it was. What a surface chooses is only its ROLE's row in this table; there is no
/// per-screen scrim number anywhere else (`containers::tests::no_surface_states_its_own_dim_weight` greps for one).
///
/// The rows are roles, not screens, and each alpha is the value the role already shipped at:
/// moving them here changed which file states a number, not what any panel looks like at
/// [`TINT`] `= 0`.
pub mod underlay {
    /// A compact menu beside the thing it is about (the card menu, the Library's Sort/Filter
    /// menu): most of the page stays readable, so the dim only separates the panel from it.
    pub const DIM_COMPACT: f32 = 0.34;
    /// A read-only or picker panel in the middle of the frame (*Also available*, *About*,
    /// *Track information*, Settings' own ground dim). Was `alert::SCRIM_A` — the design's
    /// `scrimStill`.
    pub const DIM_PANEL: f32 = 0.46;
    /// A sheet that takes over a side of the screen (the profile menu, the player's `…` menu).
    pub const DIM_SHEET: f32 = 0.50;
    /// A decision alert: the page is not what is being asked about, so it recedes further than
    /// behind a read-only panel.
    pub const DIM_DECISION: f32 = 0.55;
    /// The player's track menu over moving video: its rows sit on the busiest ground in the app.
    pub const DIM_PLAYER: f32 = 0.58;
    /// A panel of PROSE over artwork (the person bio): the text-legibility floor
    /// [`super::SCRIM_TEXT_A`] derives, restated as this role's weight rather than borrowed.
    pub const DIM_PROSE: f32 = 0.72;
    /// **How much of the inherited field survives the dim's black ink** — the `weight` of
    /// `Role::Dim`, `mix(SCRIM_BLACK_INK, field, TINT)`. `0.0` is the flat
    /// [`super::scrim_black`] rect exactly, to the bit (`underlay::plan` owns that contract), so
    /// this is the one knob that turns the whole family's inheritance down or off.
    pub const TINT: f32 = 0.35;
    /// **How much of the inherited field a popover PANEL carries under its frost** — the
    /// multiply `widgets::panel_ground` draws the field's window with before
    /// [`super::PANEL_MATERIAL`]'s frost goes over it. It stands in for what the backdrop blur it
    /// replaced used to see: the page with the panel's own dim already on it, i.e.
    /// `1 - DIM_PANEL * (1 - TINT)` = 0.70 for the middle-of-the-frame role. One number for every
    /// role, because the frost on top (`PANEL_MATERIAL`, .85) is what makes the panel read as the
    /// panel material; this only decides how much of the page's hue shows through it, and where.
    /// `/tmp/nativejelly-paneltint=<w>` sweeps it on a devtriggers build.
    pub const PANEL_TINT: f32 = 0.70;
    /// **The brightest the field may be under a panel**, Rec.709 over display codes — the same
    /// measure and the same number as the page ground's ceiling (`widgets::GROUND_LUMA`), and for
    /// the same reason: a white poster under a menu must not turn the menu grey. Spent as a SCALAR
    /// over the panel's whole window (`underlay::UnderlayField::panel_plan`), so hue and the
    /// field's shape survive and only brightness is given up. The fine print's contrast floors
    /// over a white and a black underlay are graded in `underlay_tests.rs`.
    pub const PANEL_LUMA_MAX: f32 = 0.42;
}
// `with_a` is defined in `gfx::tokens` (module-layers step L5) beside the card constants that use it;
// it is how a role spells a stop on the white/black alpha ramps: `with_a(WHITE, 0.20)`.
pub use nj_gfx::gfx::tokens::with_a;
/// Blend `a` toward `b` by `t` (rgb only; keeps `a`'s alpha) — for a token that is a *mix* of two
/// roles rather than one of them, e.g. an ambient wash sitting `t` of the way from [`SURFACE_APP`]
/// to an item's artwork colour. A screen that lerps channels in a loop wants this instead. `const`
/// so a ROLE can be spelled as a mix of two primitives ([`CONTROL_SPENT_FILL`]) — the design
/// project's `color-mix(in srgb, …)`.
pub const fn mix(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3],
    ]
}
/// [`mix`], **alpha included** — for CROSS-FADING one role into another over time, as opposed to
/// spelling a token that sits between two of them.
///
/// The distinction is not pedantry, it is the reason both exist. `mix` keeps `a`'s alpha because a
/// tint has one opacity and two hues; a cross-fade has two of each, and the pairs this app actually
/// animates between differ in both — `CONTROL_IDLE_FILL` is `.92` where `ACCENT` is opaque, and
/// `TEXT_TERTIARY` is opaque where `ROW_VALUE_INK_ON_DIM` is `.38`. Reaching for `mix` there lands
/// the destination hue at the SOURCE's opacity, which on the search field's own hint run is a
/// near-black at full strength over white.
pub const fn cross(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}
/// Scale a token's rgb by `k` toward black, keeping its alpha — e.g. folding a scroll-darken into a
/// layer tint. Pair with [`with_a`] when the alpha also changes.
pub const fn dim(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

// ── Rails / overlays / accents ───────────────────────────────────────────────
/// Unfilled progress/scrubber track.
pub const RAIL_TRACK: [f32; 4] = with_a(WHITE, 0.20);
/// Buffered-ahead / resume-track band.
pub const RAIL_BUFFERED: [f32; 4] = with_a(WHITE, 0.28);
/// Played/filled portion of a rail.
pub const RAIL_FILL: [f32; 4] = with_a(WHITE, 0.95);
/// An intro/credits SEGMENT on the player scrubber — a band lit a step above the track, drawn
/// between the track and the fill so the played part of a segment reads as played. Stays inside the
/// rail's monochrome language on purpose: a hue here would compete with the amber resume fill for
/// "this bit is special", and the rail sits straight over moving video.
// (RAIL_MARKER, white @ 0.42, was the intro/credits band on the scrubber. Removed 2026-08-04 with
// the band itself — at twice the track's opacity it read as a rendering artifact rather than as
// information. See the "the rail carries NO marks" note in appkit/player_hud.rs.)
/// The **ambient wash's** resting tint — the faint warm cast a page carries when no artwork is
/// keying it (the person page's header state, `Person Screen.dc.html`'s
/// `rgba(233,230,224,.10)`). The palette's one warm stop, and the only survivor of the warm "Snow"
/// [`ACCENT`] used to be: a page GROUND, never a control. Its own role rather than a borrowed one,
/// so retuning a focus colour can never silently restyle a page background.
pub const WASH_WARM: [f32; 4] = SAND_100;
/// Frozen ambient envelope for a route that intentionally appears before Home has fetched any
/// artwork (Shared Sources).  It is a DESIGN-SYSTEM fallback, not a screen colour: the graphite
/// and amber stops create the low-frequency wallpaper light of an UltraBlur envelope without
/// pretending there is a poster behind a pre-Home screen.  Once real artwork exists,
/// [`crate::ui::route_screen::RouteGround`] uses that instead.
pub const ROUTE_GROUND_FALLBACK: [[f32; 4]; 4] =
    [ATMOS_WARM_GREY, ATMOS_CHARCOAL, ATMOS_UMBER, ATMOS_ASH];
/// **A caution that is not an error** — a server reached without encryption, marked where it is
/// named (the sign-in's server card). Litefin's warning `#ff9800` (`docs/litefin-look.md` 2.1),
/// distinct in lightness from `DANGER`'s red so the two never rely on hue alone.
pub const CAUTION: [f32; 4] = ORANGE_500;
/// **A person's avatar disc** (who's watching, *Add a user*): the person's initial on one of these,
/// chosen by their id so one person keeps one colour on every screen. Drawn from the pre-Home
/// atmosphere those screens stand on, so a disc reads as part of that ground rather than a badge.
pub const AVATAR_TONES: [[f32; 4]; 4] = [ATMOS_WARM_GREY, ATMOS_ASH, ATMOS_UMBER, NEUTRAL_600];
/// Continue-Watching progress fill: the left stop of Litefin's card progress gradient, the accent
/// `#6b6ede` (`docs/litefin-look.md` 4.2 Progress). Upstream's was a warm amber `#fab82e`.
pub const RESUME_FILL: [f32; 4] = VIOLET_500;
/// The progress gradient's right stop, the accent hover `#9597e7` (4.2 Progress).
pub const RESUME_FILL_END: [f32; 4] = VIOLET_400;
/// The card progress bar's height, at the very bottom inside the image (4.2 Progress: 6px).
pub const RESUME_BAR_H: f32 = 6.0;
/// Plex's own "Plex Pass" gold, `#e5a00d` — a BRAND REFERENCE, not a palette member
/// (`Plex Pass Awareness.dc.html`, "the amber decision"). Two ambers exist ON PURPOSE and stay
/// two tokens: [`RESUME_FILL`] is ours and tunable; this one names somebody else's colour and
/// must not drift when the first is retuned. They never meet: progress fill lives on card art;
/// pass-gold appears on exactly TWO surfaces, both derived from Plex's own Pass docs (the owner's
/// directive — design as visual baseline, logic from the docs): the failure read-out's filled
/// capsule, and the detail facts row's two Pass-gated states (`detail::play_note`, the docs-derived
/// truth table — hardware conversion and HDR tone mapping are both Pass-gated server features, and
/// the warning outranks the soft note because a wrong picture outranks a slower one). Every use of
/// the name is load-bearing,
/// which is the trademark position. [`crate::ui::widgets::pass_capsule`] is its only consumer
/// besides ink; a line carries at most one gold thing, and warning severity is never amber — it
/// is carried by glyph, stroke and contrast.
pub const PASS_GOLD: [f32; 4] = AMBER_500;
/// Near-black ink over a [`PASS_GOLD`] fill — the error read-out's FILLED capsule, the one place
/// the capsule fills (on pure black an outline has no ground and reads as a hole). `#1a1204`.
pub const PASS_GOLD_INK: [f32; 4] = AMBER_950;
/// Unfilled track behind the Continue-Watching resume bar - the full-bleed card-bottom rail.
/// Litefin's `rgba(0,0,0,.5)` (`docs/litefin-look.md` 4.2 Progress): a darkening, so the accent
/// reads against a bright poster.
pub const RESUME_TRACK: [f32; 4] = with_a(BLACK, 0.5);
/// Ink over [`RESUME_FILL`] when the amber is a SURFACE rather than a mark. Distinct from
/// [`ACCENT_INK`]: that one is tuned against the near-white [`ACCENT`], and at that value it
/// disappeared into the amber rather than reading as a knockout.
///
/// **No consumer today** — its one user was the amber watched DISC, which the 2026-08-13 evening
/// sync replaced with a bare tick over a veil ([`TILE_MARK_INK`]). Kept because the design system
/// still carries the role. With the fill now the accent, its ink is the accent's own lavender -
/// what Litefin's accent-filled badges are inked in (`docs/litefin-look.md` 4.2 Badges).
pub const INK_ON_RESUME: [f32; 4] = VIOLET_200;

// ── A list row's trailing VALUE read-out ─────────────────────────────────────
/// The word at a row's trailing edge that says what it is SET to (`On`/`Off`, "English"), over the
/// FOCUSED row's near-white pill. One step behind the label so the label is what you read and the
/// value is what you check — and an ALPHA of the pill's own ink rather than a grey, because a grey
/// over near-white goes muddy where black at a weight stays clean. Unfocused rows use
/// [`TEXT_SECONDARY`] (and [`TEXT_TERTIARY`] when quietened), which need no token of their own,
/// and [`ROW_VALUE_INK_DIM`] on a dim row.
pub const ROW_VALUE_INK_ON: [f32; 4] = with_a(BLACK, 0.60);
/// The same read-out a further step back — see [`ROW_VALUE_INK_ON`]. Derived from [`ACCENT_INK`]
/// rather than black: at .38 the ink is thin enough that its HUE starts to show, and the pill's own
/// near-black is the one this must not look tinted against.
pub const ROW_VALUE_INK_ON_DIM: [f32; 4] = with_a(ACCENT_INK, 0.38);
/// An unfocused **dim** row's read-out (`table::Row::dim` — an unavailable step, a limit reached).
/// The dim row's label is already [`TEXT_TERTIARY`], and the value keeps its one step behind the
/// label; a read-out at the live row's ink made the row look half-available. That same ink
/// quietened, as [`TEXT_SEPARATOR`] is, rather than a grey of its own.
pub const ROW_VALUE_INK_DIM: [f32; 4] = with_a(TEXT_TERTIARY, 0.45);

// ── The watched TICK on artwork (the tile's one state mark) ──────────────────
/// The tick's ink: `COOL_0`, the same near-white a title is set in. **No disc, no plate** — the
/// artwork stays visible and only the veil below touches it. (The mark was an amber disc with a
/// knocked-out check for one day, 2026-08-13; the amber is now the resume bar's alone, so the two
/// states cannot be mistaken for each other at a glance across a shelf.)
pub const TILE_MARK_INK: [f32; 4] = COOL_0;
/// The **veil**: a corner falloff under the tick, and the only thing that touches the picture. A
/// white tick has no contrast of its own over a light frame — a snowfield, a white title card, the
/// office's white backdrop — so this guarantees it reads without putting a plate on the artwork.
/// Peak strength at the corner itself, gone by ~half the veil's box (`widgets::tile_mark_veil`).
pub const TILE_MARK_VEIL: [f32; 4] = with_a(BLACK, 0.40);
/// The tick's own soft drop shadow, INSIDE the veil — belt and braces on a bright frame, and what
/// keeps the mark from dissolving into a busy one (the design's `drop-shadow(0 2px 7px …)`).
pub const TILE_MARK_SHADOW: [f32; 4] = with_a(BLACK, 0.60);
/// The ink of a collection's NAME set on its baked fan (`Collections.dc.html` G1:
/// `.art b { color: rgba(255,255,255,.94) }`), drawn live by `ui::collection_tile::draw_fan_name`.
pub const FAN_NAME_INK: [f32; 4] = with_a(WHITE, 0.94);
/// The drop under that name (the mock's `text-shadow: 0 2px 8px rgba(0,0,0,.5)`), set as an offset
/// copy of the name: the glyph path has no blur, and the fan's own scrim carries the legibility.
pub const FAN_NAME_SHADOW: [f32; 4] = with_a(BLACK, 0.50);
/// Error/destructive signal ink - the wrong-PIN dot flash. Litefin's error `#f44336`
/// (`docs/litefin-look.md` 2.1).
pub const DANGER: [f32; 4] = RED_450;
/// Litefin's success `#4caf50` (`docs/litefin-look.md` 2.1).
pub const SUCCESS: [f32; 4] = GREEN_450;
/// Litefin's info `#2196f3` (`docs/litefin-look.md` 2.1).
pub const INFO: [f32; 4] = BLUE_500;
/// How much [`DANGER`] a destructive control's IDLE plate carries — the design's 16%.
///
/// Named rather than written into [`CONTROL_DANGER_IDLE_FILL`] because it is the one number that
/// decides whether the tint is a *hue* or an *alarm*: the plate has to read as tinted at ten feet
/// and still not compete with the focused face, which is the whole fill. Retuning the destructive
/// family is this line.
pub const DANGER_IDLE_TINT: f32 = 0.16;
/// The idle plate of a **destructive** control ([`ControlStyle::Danger`](crate::ui::widgets::ControlStyle::Danger)):
/// the app's ordinary [`CONTROL_IDLE_FILL`] carrying [`DANGER_IDLE_TINT`] of [`DANGER`].
///
/// A derived token and not a palette stop, deliberately — the destructive plate IS the neutral one
/// with a hue leaned into it, so retuning either the neutral or the danger stop moves this with
/// them instead of leaving a third value behind. [`mix`] keeps the neutral's own `.92` alpha, which
/// is what keeps the two idle plates the same MATERIAL and makes the hue the only difference
/// between them.
///
/// The colour is a property of the ACTION and is worn while nothing is focused, so the label is
/// named before the remote reaches it; the FILL is still a property of FOCUS
/// ([`DANGER`] whole, under [`TEXT_PRIMARY`]). That split is what keeps a destructive control a
/// PlxNative control rather than a red button borrowed from somewhere else.
pub const CONTROL_DANGER_IDLE_FILL: [f32; 4] = mix(CONTROL_IDLE_FILL, DANGER, DANGER_IDLE_TINT);
/// The label over [`CONTROL_DANGER_IDLE_FILL`] — the danger ink itself. The hue is stated twice on
/// an idle destructive control (a tinted plate AND a tinted label) because at ten feet the plate
/// alone is a shade of grey; the label is what names the action.
pub const CONTROL_DANGER_IDLE_INK: [f32; 4] = DANGER;
/// Dev counter's two human-visible buffer phases. The renderer holds each for 30 returned swaps,
/// so motion is visible instead of red/green frames blending into yellow at panel refresh rate.
pub const DIAG_FLIP_A: [f32; 4] = GREEN_400;
pub const DIAG_FLIP_B: [f32; 4] = RED_400;
/// Stats-for-Nerds delivery budget that covers the current media demand. This is not a generic
/// success colour: the diagnostic plot uses it only for the literal `budget >= demand` state.
pub const DIAG_LINK_SUSTAINS: [f32; 4] = GREEN_400;
/// The same plot when its conservative delivery budget is below current media demand.
pub const DIAG_LINK_DEFICIT: [f32; 4] = RED_400;
/// Bytes delivered during one diagnostics sampling cell. Kept independent of the budget colour:
/// activity can be high while the probabilistic budget still does not cover demand.
pub const DIAG_NETWORK_ACTIVITY: [f32; 4] = COOL_150;
/// Arithmetic mean across the diagnostics plot's visible network-activity window. Amber separates
/// the statistic from the cool instantaneous bars; the adjacent `mean` label prevents it being
/// mistaken for an independent connection-speed test.
pub const DIAG_NETWORK_MEAN: [f32; 4] = AMBER_300;
/// The fixed-slot plot's overwrite cursor. Near-white survives every state colour and a phone
/// camera's chroma subsampling, which is the output format of this surface.
pub const DIAG_SWEEP_CURSOR: [f32; 4] = COOL_0;
/// Section divider hairline: Litefin's `--jf-divider`, white .05 (`docs/litefin-look.md` 2.1).
pub const HAIRLINE: [f32; 4] = with_a(WHITE, 0.05);
/// A glass container's **perimeter line** — the design system's `--glass-rim`, one notch under the
/// tiles' own [`CARD_SHEEN`] (.22) because a sheet's edge is quieter than a card's.
///
/// It goes ON TOP of the material, not under it, and that ordering is the whole reason it is a
/// token here rather than a shader term. The design states the track as two inset shadows —
/// `inset 0 0 0 1px var(--glass-rim), inset 0 1px 0 var(--glass-rim-light)` — and an inset shadow
/// composites over the background it is declared with. `fs_glass.frag`'s own specular hairline is
/// part of the BACKDROP, so the darkening lands on top of it and its weight is whatever the scrim
/// leaves; drawing the line here puts it where the design puts it and makes .14 mean .14.
pub const GLASS_RIM: [f32; 4] = with_a(WHITE, 0.14);
/// The same line on the TOP edge only, at double weight — `--glass-rim-light`. One light, from
/// above, the direction every card shadow in this system already falls in.
pub const GLASS_RIM_LIGHT: [f32; 4] = with_a(WHITE, 0.28);

/// The lit edge's weight where the surface has the BRIGHTEST ground it will ever sit on.
///
/// [`GLASS_RIM`] and [`GLASS_RIM_LIGHT`] are the design system's constants and they are right for
/// the ground the design was drawn over — a dark one. They are constants, though, and the edge they
/// draw is not a colour but a RELATIONSHIP: a lit edge has to be brighter than the light it is
/// catching. Measured on the panel against bright artwork, the .28 top line lands **34.9 L\* below
/// its own ground**, which stops it being a local maximum across the edge at all — the profile runs
/// 220 → 196 → 150 → 73 straight down, and an edge that is only a step on a monotone ramp reads as
/// blur, not as a bevel. Reported as "the rim is harder to see than the buttons'", which is exactly
/// right and is a comparison worth keeping: a control sits in the darkest quarter of the frame by
/// construction, so its rim is the brightest thing at its boundary and never had this problem.
///
/// So the two constants become the FLOOR of a ramp that ends here, travelled with the ground — the
/// same ground, and the same eased signal, the scrim density already follows. A dark ground still
/// gets exactly `.14`/`.28` and nothing about that case moves.
///
/// **It was 1.0, on the argument that the rim is a LERP** — `fs_glass.frag` mixes the surface TOWARD
/// this colour, so a ceiling of one means "at the brightest ground this bar will ever sit on, the
/// line is the line's own colour", a 1px white hairline. That is sound about the arithmetic and
/// wrong about the material, and the reference says so in one number.
///
/// A lit edge is lit BY something, so it must move with what is behind it. Measured against the
/// macOS 26 system tab bar over a hard stripe ground — the top rim's brightness regressed on the
/// page directly above it, column by column:
///
/// | | line | line/face | its own swing | slope vs ground |
/// |---|---|---|---|---|
/// | macOS `.regular` | 124 | 1.39 | **23.9%** | 0.25 |
/// | ours at 1.0 | 213 | 4.23 | 10.5% | 0.23 |
/// | ours at 0.6 | 184 | 3.67 | 17.4% | 0.33 |
/// | **ours at 0.4** | 167 | 3.33 | **22.9%** | 0.39 |
/// | ours at 0.28 (no ramp) | 156 | 3.10 | 27.1% | 0.43 |
///
/// The standing suspicion was that our edge did not depend on the ground at all. It always did —
/// the SLOPE was 0.23 against their 0.25 before anything changed. What was wrong is the LEVEL: at
/// 1.0 the line sits so near white that the ground's swing is 10% of its own brightness, and a line
/// that bright and that even is read as a stroke drawn round a shape rather than as an edge
/// catching a lamp. Lowering the ceiling does not add the dependence, it stops DROWNING it.
///
/// 0.4 is where the relative swing meets the reference (22.9% against 23.9%). 0.28 — the floor,
/// where the ramp flattens into [`GLASS_RIM_LIGHT`] and there is no travel at all — matches the
/// look slightly better and the number slightly worse; the extra tenth is kept because this is a
/// television seen from three metres, and it is the case where the ground is BRIGHT that the ramp
/// was built for. `/tmp/nativejelly-rimmax` sweeps it without a rebuild.
///
/// (`line/face` stays far from the reference at every rung, and that is NOT this constant's to fix:
/// our face is 50 where theirs is 89, which is the density policy, not the edge.)
pub const GLASS_RIM_MAX: f32 = 0.4;

// ---- The standing track has ONE material -----------------------------------------------------
//
// It had two for a while: a black pair for dark artwork and a white pair for bright, with a
// crossover between them. That is Apple's answer and it is a good one — over bright content their
// chrome goes light and flips its ink — but it is a DISCRETE second state, and a discrete state has
// to be decided every time the ground moves. Deciding it produced four reported bugs in one week: a
// bar flipping back and forth on one unchanging hero, the same hero settling dark in one run and
// light in the next, 1.6 s between the press and the material changing on a route change, and a
// visible wrong-direction excursion while the decision was pending. Guarding it took a hysteresis
// band, a commit window, a two-independent-readings rule, an override that woke the whole screen so
// those readings could happen at all, a page-swap fast path and a frozen draw weight.
//
// A judging panel priced that against what the second polarity actually bought, on synthetic grounds
// (`ui::testpat`) rather than on whatever poster happened to be up, and three of four lenses said
// delete it. The measurements that decided it:
//
//   * with a brighter idle ink (`TEXT_READING` rather than `TEXT_TERTIARY`) the dark material sits
//     at or near its floor for every ground up to L* 73, so the light one is not rescuing the common
//     case — the two are BIT-IDENTICAL on 8 of 14 graded grounds;
//   * on the real heroes it differs on, the light material wins only where the hero is uniformly
//     bright, and wins there BY DISSOLVING: L* 97.4 against a L* 95.7 ground is 1.7 L* of
//     separation, i.e. four words and a rim with no container under them;
//   * on a hero whose ground is MIXED — half a dark building, half bright sky, which a census of a
//     real library's rotation found is the median case, span 26.8 L* — the light material lands
//     34.9 L* off the local ground on the dark half where the dark material lands 3.4 off. The
//     Apple property it was imitating (sit close to your backdrop) is honoured by the material that
//     replaced it.
//
// What remains is one black pair, solved per frame, and an ink bright enough that it rarely has to
// ask for much. `docs/glass-hardware-budget.md` prices the surface; the deleted polarity is in the
// history if it is ever wanted back.

// The light polarity's own tokens stood here — `PILL_RIM_ON_LIGHT`, `PILL_RIM_LIT_ON_LIGHT` and
// the `COOL_600` ink above — and they went with it. They were live `pub` colour codes that nothing
// read, which in a file whose whole claim is "the only place a colour code is written down" reads
// as a role still in the system rather than as one that was measured and removed. The account of
// WHY it was removed is the block above, and it is the part worth keeping.

/// Faint focus pill (pre-`TabPill`-adoption tab highlight).
pub const OVERLAY_FOCUS_PILL: [f32; 4] = with_a(WHITE, 0.14);
/// The shared top tab bar's **TRACK** — the recessed near-black capsule the tab pills sit in, and
/// the one thing that owns their legibility over bright hero art (inside it a plain segment can stay
/// bare [`TEXT_TERTIARY`] text). Dark-material weight — light enough to keep a hint of the
/// artwork, dark enough that tertiary ink holds over near-white art — and a GRADIENT: this top stop
/// plus [`TAB_TRACK_BOT`].
///
/// The design project flattens the pair to one opaque code (`--tab-track-fill`, Neutral 900
/// `#0c0c0d`) because CSS cannot say "black at .72 over whatever happens to be behind it". That code
/// IS this stop composited over [`SURFACE_APP`] (44 × 0.28 ≈ 12), which is what makes the two
/// descriptions one material rather than two decisions.
pub const TAB_TRACK_TOP: [f32; 4] = scrim_black(TAB_TRACK_A_TOP);
/// The track's bottom stop — see [`TAB_TRACK_TOP`].
pub const TAB_TRACK_BOT: [f32; 4] = scrim_black(TAB_TRACK_A_BOT);
/// The tab track's stops when it is drawn as GLASS — the same near-black ink at roughly half the
/// weight, since a real backdrop behind it is doing the work the flat material had to do alone.
/// Paired with `Painter::backdrop_blur`: without one they are a translucent hole onto the page.
///
/// **The track DARKENS where a panel frosts, and that is the difference between a container you
/// read THROUGH and one you read ON.** A sheet is a neutral frost because it holds a page's worth
/// of copy; this band is 76px tall over moving artwork and its idle labels are [`TEXT_TERTIARY`],
/// which a neutral frost leaves swimming. So it takes a black scrim pair rather than the panel
/// family's greys — the design system's `--glass-track-top`/`-bot`, the two stops on the existing
/// ramp nearest the .38/.46 this was first built with.
///
/// **These are the FLOOR of a range, not a fixed pair** — [`crate::ui::widgets::track_alpha_for`]
/// solves the weight per frame from what is actually behind the bar, and this is where it starts.
///
/// They were a fixed pair for one afternoon and could not be: both the .38/.46 they were derived
/// from and the .34/.50 recorded here were tuned against a render that darkened TWICE — the direct
/// source pass drew the flat track into the glass track's own backdrop (`widgets::draw_tab_row`
/// holds the account of it), so a nominal .42 was landing at an effective ~.87 and every legibility
/// judgement was made on the wrong picture. With that fixed the material became honest and the
/// numbers stopped being enough. Against the worst artwork a hero can be — white — the ground is
/// `1 - a`, and [`TEXT_TERTIARY`] over it:
///
/// | a | contrast | | a | contrast |
/// |---|---|---|---|---|
/// | .34 | 1.21 | | .62 | 2.17 |
/// | .50 | 1.39 | | .67 | 2.64 |
/// | .56 | 1.73 | | **.72** | **3.23** |
///
/// The bar is **3:1** — the same one `widgets`' ground test holds an ambient wash to, and the same
/// arithmetic that put [`SCRIM_TEXT_A`] and [`TAB_TRACK_A_TOP`] at .72 in the first place. Read as a
/// CONSTANT that table says the first legal density is the flat track's own, because **a blur
/// removes DETAIL, not brightness**: a quarter-res Kawase of a white poster is still white. That was
/// the verdict for a day, and it is the right verdict for a constant.
///
/// **The premise was the mistake.** A hero is one picture at a time, not every picture at once, so
/// the density does not have to survive the worst one — it has to survive THIS one. Solved per
/// frame it sits here on a dark backdrop and walks up the table only as far as the ground makes it,
/// which on a bright hero measured .562 and on most heroes is this floor. The ink never moves,
/// which is what the row's hierarchy is made of.
pub const TAB_GLASS_TOP: [f32; 4] = scrim_black(0.20);
/// **The material's LIGHT FLOOR — the amount of itself the glass shows where the page shows
/// nothing.** Two numbers, both measured against the real thing rather than chosen.
///
/// A black scrim can only ever subtract, and at the bottom of the range there is nothing left to
/// subtract from: over a page at L\*0 the bar's face measures exactly the page — 0% Weber — and the
/// whole container is carried by one pixel of rim. Over L\*20 it is 28% and darker. The system
/// container this material is answering does the opposite: measured on the real macOS 26 tab bar
/// over a near-black page, .071 → .098, **+38% Weber and LIGHTER**. Its edge, meanwhile, is one
/// antialiased pixel with no rim at all on any ground — it does not need one, because the material
/// separates itself.
///
/// So the material gets a diffuse component, and this is the one number behind it: **the darkest
/// the glass itself may be, in absolute sRGB, however dark the page gets.** It is a floor, not a
/// target — the scrim keeps doing all the separating it can, and this only catches the bottom. A
/// page brighter than the floor takes no lift at all, so every ordinary hero is untouched bit for
/// bit; `crate::ui::widgets::track_lift` is the rule and records the two shapes that were tried
/// first and why they were worse.
///
/// **This is not the light polarity coming back.** That was a second MATERIAL — a white scrim with
/// dark ink and a crossover to hunt — and it was deleted with the argument that a bar is one
/// surface. This is one surface still: same ink, same solve, same scrim, with a floor under how
/// dark the glass itself is allowed to get.
pub const TAB_GLASS_LIFT_FLOOR: f32 = 0.045;
/// The glass track's bottom stop — see [`TAB_GLASS_TOP`].
pub const TAB_GLASS_BOT: [f32; 4] = scrim_black(0.36);
/// The track material's two weights, exposed as alphas because the focused **profile chip** wears
/// the same material and FADES it in with its unfurl (`scrim_black(A * e)`) — one material and one
/// pair of weights whether it is painted at full strength or on the way in, which is what keeps the
/// chip's capsule and the tab track reading as one band on the top chrome line.
/// **The ceiling is a LIMIT, not a policy, and the ladder that went looking for a policy here found
/// nothing to change.** Swept on the television over five grounds (.72/.55/.42/.32) and then read
/// back out of the app's own `track_ground` line rather than off the pixels, which is what settled
/// it: fully eased, the solve asks for **.252** over a flat L\*55 ground, **.550** over L\*85 and
/// **.603** over the brightest real hero in the rotation (L\*95.7, span 69.8). It does not reach .72
/// on anything, so lowering the ceiling toward it changes nothing at all until the ceiling drops
/// BELOW what the solve wants — at which point what is being cut is the labels' contrast, not the
/// material's weight.
///
/// **Two measurements of this bar were wrong before this one and are worth naming**, because both
/// are easy to repeat. Reading the page from BELOW the bar samples a brighter part of the hero and
/// reported −52% Weber; capturing 3 s after a `pat:` change catches the density mid-travel and
/// reported swings of 30 codes between rungs that are in truth identical. Settled and referenced to
/// the page directly above it, this bar sits at **−15% Weber** on that hero, against the macOS 26
/// tab bar's −27% on its own page — i.e. ours is CLOSER to its backdrop than the reference is to
/// its own, which is the opposite of the story these numbers were first told to support.
///
/// So the constant stays where it was. `/tmp/nativejelly-trackmax` sweeps it, and the honest way to
/// make this bar lighter is [`super::widgets::TRACK_INK_CONTRAST`] — the 4:1 promise is what puts
/// the density where it is, and `the_lift_never_spends_the_labels_contrast` marks the boundary: the
/// guarantee survives a ceiling of .62 and dies at .60.
pub const TAB_TRACK_A_TOP: f32 = 0.72;
/// See [`TAB_TRACK_A_TOP`].
pub const TAB_TRACK_A_BOT: f32 = 0.82;
/// A PLATED tab segment's fill — the detail page's season tabs, which sit bare on the backdrop rather
/// than inside the top bar's track and so provide their own ground
/// ([`crate::ui::widgets::TabPill::plated`], `Details Screen.dc.html`). The selected one is a step up
/// from [`OVERLAY_FOCUS_PILL`]: at .14 against a plated neighbour at .08 the two read as the same pill.
pub const TAB_PLATE_SELECTED: [f32; 4] = with_a(WHITE, 0.20);
/// The selected-segment plate as a **travelling capsule** over a plated strip
/// ([`crate::ui::widgets::TabStrip`]). It is deliberately not [`TAB_PLATE_SELECTED`]: that value
/// assumed the selected plate REPLACED the idle one, whereas a capsule *slides over* the idle plates
/// that stay put underneath it — and .20 over .08 composites to .26, a selected season visibly
/// heavier than it was. 0.13 over [`TAB_PLATE_IDLE`] lands back on .20 (`a + b − ab` = .1996), and
/// because both layers are the same white that arithmetic is order-independent, so it holds whether
/// the plate is painted under or over the capsule.
///
/// The top tab row needs no such value: its pills sit bare inside the tab-bar track (which already
/// is their ground) and its selection capsule uses [`OVERLAY_FOCUS_PILL`] unchanged.
pub const TAB_PLATE_SELECTED_OVER: [f32; 4] = with_a(WHITE, 0.13);
/// An unselected plated segment — present, but only just: it says "this is a control" and nothing more.
pub const TAB_PLATE_IDLE: [f32; 4] = with_a(WHITE, 0.08);
/// Softer selection panel — the lightest weight on the overlay ramp, and deliberately NOT
/// [`TAB_PLATE_IDLE`]'s .08 one step up. The design project briefly resolved this role to that .08
/// stop while its own specimen card and readme both said .07; holding the product at .07 was the
/// right call, and the design has since added a .07 stop of its own and points here again.
pub const OVERLAY_FOCUS_SOFT: [f32; 4] = with_a(WHITE, 0.07);
/// Outlined-badge / meta-badge border.
pub const OVERLAY_BORDER: [f32; 4] = with_a(WHITE, 0.55);
/// The keyline PILL's stroke — a secondary action outlined over scrimmed video (the post-play
/// card's "Watch credits"; `Plex Pass Awareness.dc.html` deliverable D: stroke 1.5, white .38).
/// Deliberately a step quieter than [`OVERLAY_BORDER`]: that value is tuned for a 2px chip border
/// on a panel, and at .55 a 60px capsule outline over a ~.85 black scrim becomes the row's
/// brightest object, outshining the filled primary beside it.
pub const PILL_KEYLINE: [f32; 4] = with_a(WHITE, 0.38);
/// The keyline pill's knockout interior — and here the knockout is the DESIGN, not a limitation.
/// (`Painter::rring` draws a genuinely hollow outline now, which is what `keyline_chip` and the
/// PLEX PASS capsule use.) This pill sits over LIVE VIDEO, where a hollow ring would leave the
/// label on whatever frame happens to be under it; the interior is a translucent near-black that
/// keeps the scrimmed credits part of the surface instead of punching an opaque hole in them (and
/// doubles as the quiet plate [`TEXT_HEADING`] ink needs).
pub const PILL_KEYLINE_BG: [f32; 4] = scrim_black(0.55);
/// Filled metadata chip (the About column's CC/SDH/AD accessibility badges). A COOL tint rather than
/// plain white at a weight, so a chip reads as a plate rather than as a gap in the panel.
pub const BADGE_FILL: [f32; 4] = with_a(COOL_150, 0.20);
/// No-op texture tint (structural: draw an RGBA texture unmodified).
pub const TINT_WHITE: [f32; 4] = WHITE;

// ── Review-score marks (detail hero ratings row) ─────────────────────────────
// These used to be the ONE place the palette borrowed someone else's colour, justified by the
// badge being a BRAND MARK. That justification is gone with the marks: Rotten Tomatoes' fruit,
// seal and popcorn tub were removed 2026-08-02 (no licensing route exists, and a redraw is the
// infringement pattern, not a defence), and every provider is NAMED in text instead. So these are
// now our own semantic colours for a VERDICT, and only two still sit near a brand's hue because a
// ripe tomato that isn't red has stopped being a tomato.
//
// They tint an icon MASK only — never text, never a surface — so they cannot leak into the rest of
// the UI. The row's captions and scores use the ordinary TEXT_* tokens, which is the point: the
// only colour left in the row is the verdict.
/// The ripe tomato's body - Litefin's critic colour `#fa320a` (`docs/litefin-look.md` 2.1).
pub const RATING_FRESH: [f32; 4] = RED_550;
/// The star before a community rating - Litefin's `#f5c518` (`docs/litefin-look.md` 2.1, 4.1).
pub const RATING_STAR: [f32; 4] = GOLD_400;
/// The favourite heart - Litefin's `#ff3b30` (`docs/litefin-look.md` 2.1).
pub const FAVORITE: [f32; 4] = RED_430;
/// The Certified body — `#f0b429`. The SAME fruit struck in gold rather than a separate seal:
/// a rarer bar reads as a richer version of the thing, and a seal was the one shape here that was
/// unmistakably somebody's award rather than a piece of fruit.
pub const RATING_CERTIFIED: [f32; 4] = AMBER_400;
/// The audience crowd when the verdict is good — `#3ec96b`. Note the polarity across the row is
/// deliberately NOT uniform: critics go red-for-good (a ripe tomato), audience goes green-for-good
/// (a healthy crowd). Each mark is read against itself, not against its neighbour.
pub const RATING_AUDIENCE: [f32; 4] = GREEN_400;
/// The **calyx** green — the tomato's leaf-and-stem, on a fresh or certified body. `#2fae5b`, and
/// deliberately not [`RATING_AUDIENCE`]: they are two marks' greens that land close, and folding
/// them would let an audience tweak silently restyle a leaf.
pub const RATING_LEAF: [f32; 4] = GREEN_500;
/// The drained state, for both a hollow tomato and a negated crowd — `COOL_400`, the same stop
/// [`TEXT_TERTIARY`] resolves to, because the negation is literally "this has the weight of a
/// caption now". Its own role ON that stop rather than an alias OF the text token: a verdict mark
/// and a runtime label are two jobs, and retuning one must not restyle the other.
pub const RATING_MUTED: [f32; 4] = COOL_400;

// ── Card-glow geometry (the glow *color* is shader-baked in gfx.rs's FS_SRC/FS_IMG; only geometry
// is tunable). The hero-grid card's wide glow pad is `consts::GLOW_PAD` (shared with off-screen
// culling, so it has one home there).
/// Card corner radius (tiles/shelves — `CardRow`'s rounded rect + its baked focus glow follow it).
/// Litefin's card IMAGE corner, 9 = the card's 12 less its 3px focus ring (`docs/litefin-look.md`
/// 2.3 Card image inner); the ring around it is [`CARD_FOCUS_RING_RAD`]. Upstream's was 14.
pub const CARD_RING_RAD: f32 = 9.0;
/// The outer corner of a card's [`FOCUS_RING`]: the image's [`CARD_RING_RAD`] plus the ring's
/// [`FOCUS_RING_W`], Litefin's `--jf-border-radius` 12 (`docs/litefin-look.md` 2.3).
pub const CARD_FOCUS_RING_RAD: f32 = CARD_RING_RAD + FOCUS_RING_W;

// ── Episode filmstrip focus treatment ───────────────────────────────────────────────────────────
// The episode filmstrip's labels sit BELOW the tile, so a focus change has to read from the text as
// well as the still: the still lifts and grows, and the label block gets a `ui::text_lift` plate.

/// The episode still's focus pop. Its own constant rather than [`crate::ui::widgets::CARD_FOCUS_SCALE`]
/// (1.07): on a 420×236 still that overlaps the neighbouring slot, and 1.04 reads as a lift rather
/// than a zoom.
pub const EP_CARD_FOCUS_SCALE: f32 = 1.04;

/// How far the focused episode still rises, in px, at full focus — with [`EP_CARD_FOCUS_SCALE`] it
/// separates the focused card from the row instead of merely enlarging it in place.
pub const EP_CARD_FOCUS_LIFT: f32 = 6.0;

/// The episode label block's inks. They never change with focus (Apple dims nothing, and the
/// dimmed variants were rejected on the TV); focus is marked by the `ui::text_lift` plate instead.
pub const EP_TITLE_INK: [f32; 4] = TEXT_PRIMARY;

/// The episode summary's ink — [`TEXT_SECONDARY`] mixed slightly toward [`TEXT_PRIMARY`].
pub const EP_SUMMARY_INK: [f32; 4] = mix(TEXT_SECONDARY, TEXT_PRIMARY, 0.03);

/// The episode number / air date / rating chip's ink.
pub const EP_META_INK: [f32; 4] = TEXT_TERTIARY;

/// **The ALERT PANEL corner** — the full-frame glass sheet a block opens with OK
/// (`Alert Views.dc.html`), as opposed to the anchored menus a chip or a button drops. Those keep
/// their own smaller corner (`screens::item_menu` 20, `screens::alt_sources` 20,
/// `screens::account_menu` 24, `glassload` 28):
/// a menu hangs off a control and reads as part of it, while an alert owns the middle of the frame
/// and is its own object.
///
/// **32, in the design's own words** — quoted rather than paraphrased, because all four alert
/// panels were built against this one paragraph and three of them re-derived it independently:
///
/// > "Panel corner is 32px, one step over `--radius-panel` 24. It was tried at 60 — one whole
/// > `--control-h`, so the panel would read as round as the pills inside it — and that is the
/// > value that made the modal look like it came from somewhere else: at 14px on every tile and
/// > 18px on a `TableView` row pill, this page has a restrained corner vocabulary and a 60px arc
/// > leaves it. 32 stays inside that vocabulary while still reading as softer than a card, and
/// > clears the 28px glass bevel so the chamfer runs inside the arc."
///
/// The tile's 14 was [`CARD_RING_RAD`] (9 in the Litefin fork) and the row pill's 18 is
/// `table::PILL_RAD`, so that sentence was checkable against this tree rather than only the mock. Two consequences are
/// load-bearing rather than decorative. The **bevel clause**: below the 28px glass bevel the
/// chamfer would run OUTSIDE the corner arc and the rim would break at each corner. And the
/// **padding clause**, which the design leaves implicit and the panels make explicit — every one of
/// them pads 48, and 32 is the largest corner whose arc still clears a 48px pad, so the eyebrow's
/// cap-top and the footer's keycap sit beside the curve rather than inside it. That is why a panel
/// quotes its radius and its padding together.
///
/// Deliberately ONE token rather than a literal per panel: the alert panels are a FAMILY, and a
/// corner that drifted between them is exactly the drift `theme.rs` exists to kill. It very nearly
/// did — this shipped as four constants under four names (`ALERT_CORNER_RAD`, `ALERT_PANEL_RAD`,
/// `ALERT_RAD`, `RADIUS_PANEL`) before the four panels were merged into one tree.
///
/// One independent corroboration is worth keeping, because it was derived from the app rather than
/// from the mock and it agrees: a decision alert's two answers are fully-rounded capsules at
/// [`StatusOverlay::CTRL_H`](crate::ui::widgets::StatusOverlay::CTRL_H)`/2` = 30, so 32 sits a hair
/// OVER the pills it contains — enough that the sheet is unmistakably the outer shape, not enough
/// for it to become one. That is the design's "as round as the pills inside it" objection to 60,
/// measured on the one panel that actually holds pills.
pub const ALERT_PANEL_RAD: f32 = 32.0;

/// **The alert panels' HEAD ladder** — eyebrow → title → subtitle, in the design's own `margin-top`s
/// (`Alert Views.dc.html` §1B and §1C, which spell the identical four numbers).
///
/// It is a module rather than four loose constants for the reason [`ALERT_PANEL_RAD`] is one token:
/// the panels are a FAMILY, and this ladder had already drifted. §1B and §1A each named the pair
/// locally and agreed by luck; §1C spelled it a third way — `cap_h(CAPTION) + space::SM` and
/// `cap_h(TITLE) + space::SM` — which is not the same arithmetic at all. Measured on the simulator
/// at 1920×1080, the person panel's eyebrow sat **26px** above its name where the track panel's sat
/// 38, and its name **45px** above its identity line where the spec says 56. That is what "PERSON is
/// too close to the name" was, and no amount of looking at one panel on its own would have found it:
/// the bug is only visible as the difference between two sheets nobody sees side by side.
///
/// **A rung ladder is the wrong tool here and that is why the drift happened.** `space::SM` is 16 —
/// the nearest rung to both 14 and 12 — so spelling this in rungs rounds two DIFFERENT gaps to one
/// number and inverts them: a kicker belongs to the title under it and must sit tighter than the
/// title sits to its own subtitle, which 14-then-12… does not do either. The design's own answer is
/// that both are sub-rung and neither is the other: 14 over a 24px eyebrow band, 12 under a 44px
/// title band. Those two BANDS are half the ladder, which is why the leads live here too — a caller
/// that advanced by measured cap heights instead is exactly how §1C ended up 12px tight.
pub mod alert {
    /// Titles and reading text share the panel's left padding edge in every alert.
    /// Button labels and paired trailing values keep their own control/column alignment.
    pub const TEXT_ALIGN: crate::ui::label::HAlign = crate::ui::label::HAlign::Left;
    /// The alert panel's inset — the pad every read-only alert lays its head ladder and its body
    /// against. **One number, because [`super::ALERT_PANEL_RAD`]'s own argument depends on it**:
    /// "32 is the largest corner whose arc still clears a 48px pad" cannot be checked while the pad
    /// is four private constants in four files. It was exactly that, and `widgets::KeyHint` had to
    /// reason about "the PAD 48 all three read-only alerts drew" with no name to say it with.
    pub const PAD: f32 = 48.0;
    /// The scrolled-viewport edge dissolve (`ui::text_view::TextView::edge_fade`, since
    /// 2026-09-02 — was `widgets::edge_feather`'s opaque gradient) — how far the crossing text
    /// fades at the top and bottom of a scissor-clipped body.
    pub const FEATHER: f32 = 88.0;
    /// `line-height: 1` on the eyebrow — a caps run has no descenders to clear.
    pub const EYEBROW_LEAD: f32 = super::size::CAPTION as f32; // 24
    /// The eyebrow's `margin-bottom`, as the title's `margin-top:14`.
    pub const GAP_EYEBROW_TITLE: f32 = 14.0;
    /// `1.1` — tight, because a panel's title is one line by construction.
    pub const TITLE_LEAD: f32 = super::size::TITLE as f32 * 1.1; // 44
    /// The title's `margin-bottom`, as the subtitle's `margin-top:12`. A subtitle here is §1B's file
    /// path or §1C's dot-separated identity line.
    pub const GAP_TITLE_SUB: f32 = 12.0;
}

// ── Card treatment (Home Screen.dc.html): every tile = a soft drop shadow that GROWS with the focus
// pop + a 1px perimeter edge-sheen, both FOLDED into the tile's own draw pass (FS_IMG for textured
// tiles, FS_SRC for skeleton/chip fills), replacing the old glow ring. Applies to circles too. ──
// The edge-sheen is a single thin rounded-rect stroke flush around the whole perimeter (following the
// corner radius), CONSTANT strength on every tile — NOT a gloss/wash over the card face.
/// The 1px perimeter edge-highlight on a tile (white, faintly translucent).
pub const CARD_SHEEN: [f32; 4] = with_a(WHITE, 0.22);
/// Stroke width (px) of the perimeter edge-highlight.
pub const CARD_SHEEN_W: f32 = 1.0;

// ── THE FOCUSED TILE'S LIT-GLASS EDGE (ArtTile component, Claude Design) ────────────────────────
// `gfx::image_focus_geometry` and the `fs_img.frag` literals it mirrors read these, and the `gfx`
// layer may not name `ui`, so they are DEFINED in `gfx::tokens` (module-layers step L5) with their
// full documentation, and re-exported here unchanged for every other caller. (Only `gfx`, its
// shader test and the design-system mirror read them today, so the re-export has no user in a
// non-test build — hence the allow.)
#[allow(unused_imports)]
pub use nj_gfx::gfx::tokens::{
    CARD_GLARE_A, CARD_GLARE_EASE, CARD_GLARE_PX, CARD_GLOSS_A, CARD_GLOSS_DIR, CARD_GLOSS_FADE,
    CARD_GLOW_A, CARD_GLOW_BAND_PX, CARD_GLOW_BOT_A, CARD_GLOW_BOT_PX, CARD_GLOW_TOP_A,
    CARD_GLOW_TOP_PX,
};

// ── THE CAPSULE OUTLINE ──────────────────────────────────────────────────────
// The two FREE numbers of the control capsule's shape; everything else about it is solved from
// them and the box (`ui::pill`, and `tokens/shape.css` for the full construction). A capsule here
// is not a stadium: it has no straight side and no ellipse, only circular arcs joined by blends.

/// The end circle's radius over the box HEIGHT. Tangency forces it strictly below .5 — a circle of
/// exactly half the height could only touch the big arc at its apex, and that shape is the stadium,
/// which has no solution here. The leftover band `(.5 − this) × height` is both how far the ends sit
/// inside the box and how far the top and bottom bow, so this is **the dial between a stadium and a
/// pillow**: .45 shows the side's curve plainly on a wide button, .492 is 0.8% of the height and
/// reads as almost a stadium — which is the shipped value, and which is why the visible change at
/// this app's 60px controls is the blended CORNER rather than a bowed side.
pub const PILL_END_R: f32 = 0.492;
/// How much of that band the blend arc may use, which is what fixes the big arc's radius. More
/// slack → a bigger big arc → a flatter top; at 1 it runs away and the shape returns to a stadium
/// (`ui::pill::solve` returns `None`, and the caller draws the stadium it already had).
pub const PILL_BLEND_SLACK: f32 = 0.86;
/// Drop-shadow ink under a raised card — pure black (the design's `rgba(0,0,0,…)`), alpha supplied per
/// call (× the resting→lifted focus ramp). Only the alpha/rgb matter for the folded card shadow (the
/// rgb is used by the chip's standalone shadow); its own token (not `scrim_black`) so it can be tuned
/// alone. This is the ART TILE's focused ceiling (`0 18px 44px black .50`, [`CARD_SHADOW_BLUR`]/
/// [`CARD_SHADOW_DY`]/this alpha) — the profile chip's own real, always-offset shadow (never a folded
/// card composite) keeps a shallower [`CARD_SHADOW_CHIP_A`] instead, since chip-sized art never
/// approaches the blur/offset ceilings that would otherwise cap its own alpha too.
pub const CARD_SHADOW: [f32; 4] = with_a(BLACK, 0.50);
/// The profile chip's own focused-shadow alpha ceiling ([`Painter::focus_shadow`](crate::ui::Painter::focus_shadow)) —
/// kept at the art tile's PRE-lift depth (.40) because the chip's real, always-offset shadow was
/// tuned at that depth and the tile's own bump to .50 answers a bigger, further-falling shadow the
/// chip never draws.
pub const CARD_SHADOW_CHIP_A: f32 = 0.40;
/// Focused (RISEN) card drop-shadow penumbra (px) and downward offset (px) — the CAPS on the
/// tile-scaled values, reached by a large poster. A lifted tile's shadow falls BELOW it, not around
/// it evenly — `0 18px 44px` — reading as a card actually risen off the shelf rather than glowing in
/// place.
pub const CARD_SHADOW_BLUR: f32 = 44.0;
pub const CARD_SHADOW_DY: f32 = 18.0;
/// RESTING (unfocused) drop-shadow caps — every tile carries a small, tight shadow so it sits CLOSE
/// to the shelf; on focus the blur/alpha lerp UP to the lifted values above (the folded card
/// composite's own downward shift is computed separately, see `fs_img.frag`'s shadow SDF, and is
/// exactly 0 at this resting end so a resting tile's shader takes its unchanged, symmetric path).
/// Caps on the tile-scaled resting values.
pub const CARD_SHADOW_REST_BLUR: f32 = 11.0;
pub const CARD_SHADOW_REST_DY: f32 = 4.0;
/// Resting shadow ink alpha (unfocused); lerps up to the caller's own focused alpha ceiling
/// ([`CARD_SHADOW`]'s for the art tile, [`CARD_SHADOW_CHIP_A`] for the chip).
pub const CARD_SHADOW_REST_A: f32 = 0.34;

// A prose block's focus shadow has no constants of its own: `ui::text_lift` reuses the card ramp
// (`shadow_ramp`) from a resting alpha of 0.

// Purple — developer runtime issues, distinct from product failure states.
#[cfg(feature = "threadcheck")]
const PURPLE_700: [f32; 4] = rgb8(0x70, 0x30, 0xa0);
/// Opaque main-thread checker warning surface (developer builds only).
#[cfg(feature = "threadcheck")]
pub(crate) const RUNTIME_WARNING: [f32; 4] = PURPLE_700;

/// Litefin's Home (`docs/litefin-look.md` 4.1): the hero's meta line and rating badge, its pager
/// dots and the focused hero's accent band.
pub mod home_lf {
    use super::{with_a, BRAND_FILL, WHITE};
    /// The hero meta line (`.hero-meta-row`, white at 0.9).
    pub const META_INK: [f32; 4] = with_a(WHITE, 0.90);
    /// The rating badge's fill (`.hero-meta-badge`).
    pub const BADGE_FILL: [f32; 4] = with_a(WHITE, 0.20);
    /// Pager dots: the other slides, and the one on screen.
    pub const DOT_IDLE: [f32; 4] = with_a(WHITE, 0.28);
    pub const DOT_ACTIVE: [f32; 4] = WHITE;
    /// The focused hero's accent band where it peaks (`.hero-carousel-container.focused::after`).
    pub const FOCUS_BAND: [f32; 4] = with_a(BRAND_FILL, 0.15);
}

/// **Litefin's library and search surfaces** (`docs/litefin-look.md` 6, 7) that no shared role
/// names. The browse slice reads these; every other control there takes the shared Litefin roles
/// ([`ACTION_FILL`], [`BRAND_FILL`], [`SELECTED_FILL`] and their inks).
pub mod browse_lf {
    use super::{with_a, VIOLET_500, WHITE};

    /// The alphabet bar's pill, white .05 (6 Alphabet bar; 2.1's 0.05 chip/surface weight).
    pub const ALPHA_BAR: [f32; 4] = with_a(WHITE, 0.05);
    /// The alphabet bar's corner, 36 (2.3 Alphabet bar).
    pub const ALPHA_BAR_RAD: f32 = 36.0;
    /// A genre row's focused header: white .15 behind the title (6 Genres tab).
    pub const GENRE_HEADER_FOCUS: [f32; 4] = with_a(WHITE, 0.15);
    /// The focused genre header's 3px accent border (6 Genres tab).
    pub const GENRE_HEADER_RING: [f32; 4] = VIOLET_500;
    /// The search field's border while it holds focus: the accent (7 Input, `.search-input:focus`).
    /// Litefin's own field outranks the upstream "ink only" Search focus verdict in this fork.
    pub const SEARCH_BORDER_FOCUS: [f32; 4] = VIOLET_500;
}

/// **The card slice's roles** (`docs/litefin-look.md` 4.2 Badges): Litefin's `tinted` badge skin,
/// the household's (spec 0), is the accent with its lavender ink and no border. Roles of their own
/// beside [`BRAND_FILL`]/[`BRAND_INK`], which light a CONTROL: a badge is a mark on artwork.
pub mod cards_lf {
    /// A card badge's fill - the episode code and the watched check - `#6b6ede`.
    pub const BADGE_FILL: [f32; 4] = super::VIOLET_500;
    /// The badge's text and glyph, `#d4d5f7` (2.1 Text on accent).
    pub const BADGE_INK: [f32; 4] = super::VIOLET_200;
}

/// **The Litefin details page's own roles** (`docs/litefin-look.md` 5, posterLeft): the fixed
/// backdrop's black ramp and the flat white-alpha plates of the meta row, the tech pills and the
/// rich-metadata table. Kept apart from the shared roles because only the details slice reads them.
pub mod details_lf {
    use super::{rgb8, with_a, AMBER_300, VIOLET_500, WHITE};

    /// The backdrop gradient, top to bottom, as `(fraction of the screen height, black alpha)`:
    /// .35 at the top, .60 at 45%, .90 at 75%, solid from 88% (5 Backdrop, `.backdrop-gradient`).
    pub const BACKDROP_RAMP: [(f32, f32); 5] =
        [(0.0, 0.35), (0.45, 0.60), (0.75, 0.90), (0.88, 1.0), (1.0, 1.0)];
    /// The official rating badge: white .15 under `#e0e0e0` text (5 Meta row).
    pub const RATING_BADGE_FILL: [f32; 4] = with_a(WHITE, 0.15);
    pub const RATING_BADGE_INK: [f32; 4] = rgb8(0xe0, 0xe0, 0xe0);
    /// A tech pill: white .09 inside a 1px white .16 border (5 Tech pills row, `.tech-pill`).
    pub const TECH_PILL_FILL: [f32; 4] = with_a(WHITE, 0.09);
    pub const TECH_PILL_BORDER: [f32; 4] = with_a(WHITE, 0.16);
    /// The HDR pill's warm tint and the Dolby Vision pill's violet one (`.tech-pill-hdr`,
    /// `.tech-pill-dovi`), each a two-stop wash over the plain pill.
    pub const TECH_PILL_HDR: ([f32; 4], [f32; 4]) = (with_a(AMBER_300, 0.30), with_a(AMBER_300, 0.12));
    pub const TECH_PILL_DOVI: ([f32; 4], [f32; 4]) = (with_a(VIOLET_500, 0.40), with_a(VIOLET_500, 0.16));
    /// A rich-metadata chip: white .05 inside a 2px white .10 border (5 Rich metadata table).
    pub const CHIP_FILL: [f32; 4] = with_a(WHITE, 0.05);
    pub const CHIP_BORDER: [f32; 4] = with_a(WHITE, 0.10);
    /// The 1px rule between the table's rows, white .05 (`--jf-divider`).
    pub const DIVIDER: [f32; 4] = with_a(WHITE, 0.05);
}

/// **Litefin's sign-in screens** (`docs/litefin-look.md` 8, modern layout): the roles its brand
/// pane, inputs, buttons and who's-watching tiles need that no shared role covers.
pub mod settings_lf {
    use super::{with_a, GREY_0A, VIOLET_500, WHITE};

    /// An input's and a secondary button's fill, and the ground under a who's-watching avatar:
    /// Litefin's `--jf-surface` `#0a0a0a` (8 Buttons, Inputs, User picker).
    pub const LOGIN_FILL: [f32; 4] = GREY_0A;
    /// The 4px ring of a focused input, secondary button or avatar: the accent (8).
    pub const LOGIN_RING: [f32; 4] = VIOLET_500;
    /// The ring's width (8: "focused accent 4px border").
    pub const LOGIN_RING_W: f32 = 4.0;
    /// The corner of every sign-in button and input (2.3 Login buttons / inputs).
    pub const LOGIN_RAD: f32 = 14.0;
    /// The brand pane's tagline, white at .5 (8 Left brand pane).
    pub const TAGLINE: [f32; 4] = with_a(WHITE, 0.5);
}
