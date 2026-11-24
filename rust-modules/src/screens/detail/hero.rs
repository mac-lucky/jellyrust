//! Detail hero policy: the dynamic action controls and the info column's meta row and tech pills
//! (Litefin's posterLeft, `docs/litefin-look.md` 5). Painting orchestration stays in
//! [`super::DetailScreen`]; the decisions here are explicit-value helpers so host tests need no
//! global focus state.
//!
//! Control selection and geometry take explicit `Detail`/`HeroSet` values. The tech pills also
//! consult cached playback policy, and `Ends at` the wall clock; the screen owns the metadata
//! identity and supplies the current item.
//!
//! These helpers are orchestrated by the completed screen's direct `Focusable`/`Screen` impls;
//! they do not own an independent focus cursor or screen lifetime.
//!
//! # The `ElemKey` numbering convention this package establishes (spec §13's own instruction:
//! "reserve a range so the next package can pick a disjoint one")
//!
//! Hero controls occupy the `u32` elem range **`[0, HERO_ELEM_RANGE_END)` = `[0, 64)`**. Every
//! constant below is an IDENTITY, not a POSITION — `ELEM_MARK_WATCHED`/`ELEM_MARK_UNWATCHED` are
//! two DIFFERENT ids for the two faces one control wears, never both present at once, which is
//! what lets [`DetailScreen::reconcile`](super::DetailScreen) carry the focus across a press that
//! flips the toggle (see that impl's doc — the engine calls `reconcile` with whatever key focus
//! currently holds, every frame, so identity-as-elem plus a membership test reproduces
//! the legacy `hero_col` correction without a second cursor). Section helpers use disjoint LOCAL
//! slot ranges starting at [`HERO_ELEM_RANGE_END`]; `DetailScreen` projects repeated items to stable
//! engine identities instead of exposing those positional slots as focus keys.
use std::ffi::{CStr, CString};
use std::os::raw::c_int;

use crate::metadata::{Detail, Episode, Extra};
use nj_machine::machine::{GroupId, Measure};
use crate::ui::widgets::PosterMark;
use crate::ui::{theme, Painter, Rect};

/// The hero's whole `u32` elem namespace. A later Detail package's own range must start here.
pub(crate) const HERO_ELEM_RANGE_END: u32 = 64;

pub(crate) const ELEM_PLAY: u32 = 0;
pub(crate) const ELEM_RESTART: u32 = 1;
pub(crate) const ELEM_ALT: u32 = 2;
pub(crate) const ELEM_MARK_WATCHED: u32 = 3;
pub(crate) const ELEM_MARK_UNWATCHED: u32 = 4;
pub(crate) const ELEM_TRAILER: u32 = 5;
pub(crate) const ELEM_VERSION: u32 = 6;
pub(crate) const ELEM_TRACKS: u32 = 7;
pub(crate) const ELEM_FAVORITE: u32 = 8;
pub(crate) const ELEM_UNFAVORITE: u32 = 9;

/// The hero's one focus group. Local to this screen — nothing outside `screens::detail` ever names
/// it — so, like `screens::profiles`'s `ROSTER_GROUP`/`FOOTER_GROUP`, any small integer would do;
/// `0` matches this family's own convention of seating group `GroupId(0)` as a screen's primary
/// group.
pub(crate) const HERO_GROUP: GroupId = GroupId(0);

pub(crate) fn alt_label() -> &'static CStr { nj_platform::i18n::msg::browse_detail_also_available_c() }
pub(crate) fn version_label() -> &'static CStr { nj_platform::i18n::msg::browse_detail_version_c() }
pub(crate) fn tracks_label() -> &'static CStr { nj_platform::i18n::msg::browse_detail_tracks_c() }

fn mark_watched_label() -> &'static CStr { nj_platform::i18n::msg::browse_detail_mark_watched_c() }
fn mark_unwatched_label() -> &'static CStr { nj_platform::i18n::msg::browse_detail_mark_unwatched_c() }
fn mark_show_watched_label() -> &'static CStr { nj_platform::i18n::msg::browse_detail_mark_show_watched_c() }
fn mark_show_unwatched_label() -> &'static CStr { nj_platform::i18n::msg::browse_detail_mark_show_unwatched_c() }
fn play_from_start_label() -> &'static CStr { nj_platform::i18n::msg::browse_detail_play_start_c() }
fn trailer_label() -> &'static CStr { nj_platform::i18n::msg::browse_detail_trailer_c() }
fn favorite_label() -> &'static CStr { nj_platform::i18n::msg::details_favorite_c() }

/// A control in the hero action row, named rather than numbered — ported verbatim from
/// `ui/detail.rs::HeroCtl`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum HeroCtl {
    /// the Play/Resume pill — always present, always first
    Play,
    /// the ↺ disc, present only while there is a resume point for it to ignore
    Restart,
    /// the Trailer disc, present only while the loaded item has a playable extra
    Trailer,
    /// the *Version* pill, present only while the item has more than one version
    Version,
    /// the *Audio & Subtitles* pill, present only while the leaf has a track choice to make
    /// (`Detail::has_track_choice`)
    Tracks,
    /// the *Also available* pill, present only while a second pinned source holds this item
    Alt,
    /// the ✓ face of the watched TOGGLE — worn while the item is not watched (part-watched
    /// included)
    MarkWatched,
    /// the − face of that same toggle — worn once the item IS watched
    MarkUnwatched,
    /// the outline heart of the favourite TOGGLE — worn while the item is not a favourite
    Favorite,
    /// its filled face, in the accent — worn once the item IS a favourite
    Unfavorite,
}

impl HeroCtl {
    /// This control's fixed `u32` identity (never its position — see the module doc).
    pub(crate) const fn elem(self) -> u32 {
        match self {
            HeroCtl::Play => ELEM_PLAY,
            HeroCtl::Restart => ELEM_RESTART,
            HeroCtl::Trailer => ELEM_TRAILER,
            HeroCtl::Version => ELEM_VERSION,
            HeroCtl::Tracks => ELEM_TRACKS,
            HeroCtl::Alt => ELEM_ALT,
            HeroCtl::MarkWatched => ELEM_MARK_WATCHED,
            HeroCtl::MarkUnwatched => ELEM_MARK_UNWATCHED,
            HeroCtl::Favorite => ELEM_FAVORITE,
            HeroCtl::Unfavorite => ELEM_UNFAVORITE,
        }
    }
    /// The inverse of [`elem`](Self::elem) — `None` for any `u32` outside the seven identities
    /// above, which is every elem a NON-hero group can mint (the completed sections use
    /// their own disjoint range, per the module doc).
    pub(crate) fn of_elem(e: u32) -> Option<Self> {
        match e {
            ELEM_PLAY => Some(HeroCtl::Play),
            ELEM_RESTART => Some(HeroCtl::Restart),
            ELEM_TRAILER => Some(HeroCtl::Trailer),
            ELEM_VERSION => Some(HeroCtl::Version),
            ELEM_TRACKS => Some(HeroCtl::Tracks),
            ELEM_ALT => Some(HeroCtl::Alt),
            ELEM_MARK_WATCHED => Some(HeroCtl::MarkWatched),
            ELEM_MARK_UNWATCHED => Some(HeroCtl::MarkUnwatched),
            ELEM_FAVORITE => Some(HeroCtl::Favorite),
            ELEM_UNFAVORITE => Some(HeroCtl::Unfavorite),
            _ => None,
        }
    }
    /// Is `self` the watched toggle, wearing either face?
    pub(crate) fn is_watch(self) -> bool {
        matches!(self, HeroCtl::MarkWatched | HeroCtl::MarkUnwatched)
    }
    /// Is `self` the favourite toggle, wearing either face?
    pub(crate) fn is_favorite(self) -> bool {
        matches!(self, HeroCtl::Favorite | HeroCtl::Unfavorite)
    }
}

/// Which of the conditional controls the row is showing: the four independent bits (restart,
/// trailer, Version, Also available), plus the item's watch state, which decides which FACE the watched
/// toggle wears.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct HeroSet {
    pub(crate) restart: bool,
    pub(crate) trailer: bool,
    pub(crate) version: bool,
    /// the *Audio & Subtitles* pill
    pub(crate) tracks: bool,
    pub(crate) alt: bool,
    pub(crate) mark: PosterMark,
    /// the item is the viewer's favourite: which face the favourite toggle wears, `None` where the
    /// item's server keeps no favourites (Plex), which offers no toggle at all
    pub(crate) favorite: Option<bool>,
}

/// The row's controls, in drawn order, for a given set. A fixed 8-slot array (Play + Restart +
/// Trailer + Version + Audio & Subtitles + Alt + one watch face + one favourite face is the widest
/// the row ever gets) plus a live count, so no per-frame allocation.
pub(crate) fn hero_ctls(set: HeroSet) -> ([HeroCtl; 8], usize) {
    let mut v = [HeroCtl::Play; 8];
    let mut n = 1;
    if set.restart {
        v[n] = HeroCtl::Restart;
        n += 1;
    }
    if set.trailer {
        v[n] = HeroCtl::Trailer;
        n += 1;
    }
    if set.version {
        v[n] = HeroCtl::Version;
        n += 1;
    }
    if set.tracks {
        v[n] = HeroCtl::Tracks;
        n += 1;
    }
    if set.alt {
        v[n] = HeroCtl::Alt;
        n += 1;
    }
    v[n] = if set.mark == PosterMark::Watched {
        HeroCtl::MarkUnwatched
    } else {
        HeroCtl::MarkWatched
    };
    n += 1;
    if let Some(on) = set.favorite {
        v[n] = if on { HeroCtl::Unfavorite } else { HeroCtl::Favorite };
        n += 1;
    }
    (v, n)
}

/// Whether `ctl` may hold — or be enumerated as holding — focus right now, given whether
/// full-trailer mode owns the screen. The ONE predicate [`visible_ctls`] (group extent) and
/// `DetailScreen::reconcile`/`valid` (focus legitimacy) all gate on, so the row cannot offer a
/// control the mode has taken away — the exact class of bug this predicate replaced (two
/// independent "what's visible" checks that had silently drifted apart).
///
/// **Play survives as the ANCHOR, not as a reachable control.** Full-trailer mode fades the whole
/// row out together (`DetailScreen::draw_buttons` draws every control from [`hero_ctls`]
/// regardless of this predicate — see its own doc — and lets the caller's `chrome`/`preview_chrome`
/// alpha carry the fade; `screens::detail::trailer` puts the trailer's transport up instead) — but
/// the engine still needs somewhere legitimate for the focus to stand while the mode is up, and an
/// EMPTY hero group would push it into the sections below, scrolling the page out from under a
/// trailer nobody asked to leave. Keys never reach the row anyway: the mode answers them itself,
/// before the engine.
pub(crate) fn focusable(ctl: HeroCtl, full_trailer: bool) -> bool {
    !full_trailer || ctl == HeroCtl::Play
}

/// [`hero_ctls`], filtered through [`focusable`] for full-trailer mode's FOCUS/hit-testing extent
/// only — the transient UI state where the row narrows to its Play anchor alone and everything
/// else (Restart/Trailer/Alt/the watch toggle) stops being reachable the instant `full_trailer()`
/// flips. **This is not what gets drawn.** `DetailScreen::draw_buttons` draws every control
/// [`hero_ctls`] returns, full_trailer or not, so the row fades out together with the rest of the
/// chrome instead of four pills hard-cutting a frame ahead of the one that fades (2026-09-17
/// fix — a control losing its focus must not also lose its paint on the same frame). Deliberately
/// NOT a flag on [`HeroSet`]: that struct describes what the ITEM offers, not a screen's transient
/// presentation mode. Every call site that enumerates the row for focus/hit-testing extent must go
/// through this rather than `hero_ctls` directly, or the two can disagree about which controls
/// exist right now.
pub(crate) fn visible_ctls(set: HeroSet, full_trailer: bool) -> ([HeroCtl; 8], usize) {
    let (all, n) = hero_ctls(set);
    if !full_trailer {
        return (all, n);
    }
    let mut v = [HeroCtl::Play; 8];
    let mut count = 0;
    for &c in &all[..n] {
        if focusable(c, full_trailer) {
            v[count] = c;
            count += 1;
        }
    }
    (v, count)
}

/// The control at index `i` in `set`, or `None` for an index the set does not have.
pub(crate) fn ctl_at(set: HeroSet, i: usize) -> Option<HeroCtl> {
    let (v, n) = hero_ctls(set);
    (i < n).then(|| v[i])
}

/// Where `ctl` sits in `set`, or `None` when the set does not include it.
pub(crate) fn index_of(set: HeroSet, ctl: HeroCtl) -> Option<usize> {
    let (v, n) = hero_ctls(set);
    v[..n].iter().position(|&c| c == ctl)
}

/// Where the watched toggle sits in `set` — always present, so `Some` for every set the row can
/// be in. Keyed on the control's JOB rather than either of its two faces, which is what lets
/// [`DetailScreen::reconcile`](super::DetailScreen) carry the focus across a press that flips it.
pub(crate) fn watch_index(set: HeroSet) -> Option<usize> {
    let (v, n) = hero_ctls(set);
    v[..n].iter().position(|&c| c.is_watch())
}

/// Where the favourite toggle sits in `set`, keyed on its job like [`watch_index`], so a press that
/// flips it keeps the focus on it; `None` where the item's server keeps no favourites.
pub(crate) fn favorite_index(set: HeroSet) -> Option<usize> {
    let (v, n) = hero_ctls(set);
    v[..n].iter().position(|&c| c.is_favorite())
}

/// Has this show been started AT ALL? PMS puts `S1E1 off=0` on deck for a show nobody has
/// touched, so "there is something on deck" is not the same question as "this show is underway" —
/// ported verbatim from `ui/detail.rs::show_started`.
pub(crate) fn show_started(d: &Detail) -> bool {
    d.on_deck.as_ref().map(|e| e.resume_ms > 0).unwrap_or(false)
        || d.seasons.iter().any(|s| s.viewed_leaf_count > 0)
}

/// The episode a show's hero is ABOUT — the server's on-deck episode, once the show has actually
/// been started. `None` means the hero presents the SERIES instead (finished, or never opened).
/// Ported verbatim from `ui/detail.rs::hero_episode`.
pub(crate) fn hero_episode(d: &Detail) -> Option<&Episode> {
    let ep = d.on_deck.as_ref()?;
    show_started(d).then_some(ep)
}

/// The resume position (ns) the hero's Play control would ACTUALLY apply. Ported verbatim from
/// `ui/detail.rs::hero_resume_ns`.
pub(crate) fn hero_resume_ns(d: &Detail) -> i64 {
    if d.is_show {
        hero_episode(d)
            .map(|e| crate::metadata::resume_ns(e.resume_ms, e.dur_ms))
            .unwrap_or(0)
    } else {
        crate::metadata::resume_ns(d.resume_ms, d.dur_ms)
    }
}

/// Is there a resume point for the restart disc to ignore?
pub(crate) fn has_restart(resume_ns: i64) -> bool {
    resume_ns > 0
}

/// The loaded item's watch state, in the app's one watch-state vocabulary. Ported verbatim from
/// `ui/detail.rs::hero_watch_state`.
pub(crate) fn hero_watch_state(
    is_show: bool,
    show_started: bool,
    watched: bool,
    resume_ns: i64,
) -> PosterMark {
    let mid_run = is_show && show_started && !watched;
    if resume_ns > 0 || mid_run {
        PosterMark::InProgress
    } else if watched {
        PosterMark::Watched
    } else {
        PosterMark::None
    }
}

/// [`hero_watch_state`] for `d`. Ported verbatim from `ui/detail.rs::hero_mark`.
pub(crate) fn hero_mark(d: &Detail) -> PosterMark {
    hero_watch_state(d.is_show, show_started(d), d.watched, hero_resume_ns(d))
}

/// Does the toggle's subject differ from the one the rest of the hero is about — true exactly
/// while the hero is showing an on-deck EPISODE. Ported verbatim from
/// `ui/detail.rs::watch_names_show`.
pub(crate) fn watch_names_show(d: &Detail) -> bool {
    hero_episode(d).is_some()
}

/// The Trailer disc's play fields — extra identity and HUD title, always from start.
/// `None` when the loaded item has no playable trailer (no button, no-op activate).
pub(crate) fn trailer_play(d: &Detail) -> Option<(&Extra, &str)> {
    let extra = d.trailer().filter(|e| e.playable())?;
    Some((extra, extra.hud_title(d.title.as_str())))
}

/// The Play pill's label — the word the press will actually perform.
pub(crate) fn hero_pill_label(has_restart: bool) -> &'static CStr {
    if has_restart {
        nj_platform::i18n::msg::browse_detail_resume_c()
    } else {
        nj_platform::i18n::msg::browse_detail_play_c()
    }
}

// ---- Litefin's actions row (`docs/litefin-look.md` 5 Actions row) ---------------------------
// Every control is a 72px circle 16 apart from the info column's left edge, icon only, its name
// floating under it while focused; Play wears the Resume pill (icon + label, radius 36, padding
// 22) while there is a resume point. The look is `widgets::ControlStyle::Action`.

/// The row's control height and circle diameter.
pub(crate) const CD: f32 = crate::ui::widgets::ACTION_D;
/// The Resume pill's label size.
pub(crate) const PILL_SZ: c_int = theme::size::lf::S20;
/// The floating name under the focused control: 18.4/600 white (`.action-btn-tooltip-text`).
pub(crate) const NAME_SZ: c_int = theme::size::lf::S18;

/// The Play control's width: the 72px circle, or the Resume pill while there is a resume point.
pub(crate) fn hero_pill_w(measure: &dyn Measure, has_restart: bool) -> f32 {
    if has_restart {
        crate::ui::widgets::action_pill_w(measure.width(hero_pill_label(true), PILL_SZ, true))
    } else {
        CD
    }
}

/// The drawn frame of control `i` in `set`, the row starting at the info column's left edge.
/// PURE: `pill` is [`hero_pill_w`]'s answer, the only width in the row that is not [`CD`].
pub(crate) fn hero_btn_rect_at(set: HeroSet, i: usize, y: f32, pill: f32) -> Rect {
    let (v, n) = hero_ctls(set);
    let mut x = crate::ui::detail_layout::INFO_X;
    for c in &v[..i.min(n)] {
        x += if *c == HeroCtl::Play { pill } else { CD } + crate::ui::widgets::ACTION_GAP;
    }
    let w = if v.get(i) == Some(&HeroCtl::Play) { pill } else { CD };
    Rect::new(x, y, w, CD)
}

/// The glyph each control wears. The watched toggle wears the check on both faces: the face that
/// says the item IS watched is the same check in the accent ([`ctl_active`]). The favourite toggle
/// is Litefin's heart, outline until the item is a favourite and filled (in the accent) after.
pub(crate) fn ctl_icon(ctl: HeroCtl) -> crate::ui::icons::Icon {
    use crate::ui::icons::Icon;
    match ctl {
        HeroCtl::Play => Icon::Play,
        HeroCtl::Restart => Icon::Restart,
        HeroCtl::Trailer => Icon::Trailer,
        HeroCtl::Version => Icon::Collection,
        HeroCtl::Tracks => Icon::Audio,
        HeroCtl::Alt => Icon::Server,
        HeroCtl::MarkWatched | HeroCtl::MarkUnwatched => Icon::Check,
        HeroCtl::Favorite => Icon::Heart,
        HeroCtl::Unfavorite => Icon::HeartFill,
    }
}

/// An "active" toggle shows its idle glyph in the accent: the watched toggle once the item is
/// watched, the favourite toggle once it is a favourite (5 Actions row).
pub(crate) fn ctl_active(ctl: HeroCtl) -> bool {
    matches!(ctl, HeroCtl::MarkUnwatched | HeroCtl::Unfavorite)
}

/// The focus pop's peak: Play and its Resume pill 1.05, every circle 1.15.
pub(crate) fn ctl_peak(ctl: HeroCtl) -> f32 {
    if ctl == HeroCtl::Play {
        crate::ui::widgets::ACTION_PLAY_FOCUS_SCALE
    } else {
        crate::ui::widgets::ACTION_FOCUS_SCALE
    }
}

/// The name that floats under the focused control — the verb its press performs.
pub(crate) fn ctl_name(ctl: HeroCtl, has_restart: bool, name_show: bool) -> &'static CStr {
    match (ctl, name_show) {
        (HeroCtl::Play, _) => hero_pill_label(has_restart),
        (HeroCtl::Restart, _) => play_from_start_label(),
        (HeroCtl::Trailer, _) => trailer_label(),
        (HeroCtl::Version, _) => version_label(),
        (HeroCtl::Tracks, _) => tracks_label(),
        (HeroCtl::Alt, _) => alt_label(),
        (HeroCtl::MarkWatched, false) => mark_watched_label(),
        (HeroCtl::MarkWatched, true) => mark_show_watched_label(),
        (HeroCtl::MarkUnwatched, false) => mark_unwatched_label(),
        (HeroCtl::MarkUnwatched, true) => mark_show_unwatched_label(),
        (HeroCtl::Favorite | HeroCtl::Unfavorite, _) => favorite_label(),
    }
}

// ---- The info column's text rows (5 Info column) ---------------------------------------------

/// The episode line in the accent: the series and the episode's address for an episode page, the
/// on-deck episode a show's Play would start for a show page that has one.
/// The original title line (5 Info column), only when it says something the title does not.
pub(crate) fn original_title(d: &Detail) -> Option<&str> {
    Some(d.original_title.as_str()).filter(|t| !t.is_empty() && *t != d.title)
}

pub(crate) fn episode_line(d: &Detail) -> Option<String> {
    if d.kind == "episode" {
        let ordinal = (d.season > 0 && d.index > 0)
            .then(|| crate::ui::fmt::episode_ordinal(d.season, d.index))
            .unwrap_or_default();
        let parts: Vec<&str> = [d.show_title.as_str(), ordinal.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        return (!parts.is_empty()).then(|| parts.join(" \u{b7} "));
    }
    let ep = hero_episode(d)?;
    Some(crate::ui::fmt::episode_kicker(ep.season, ep.index, &ep.title))
}

/// `2h 37m`, or `52m` under an hour (5 Meta row).
pub(crate) fn runtime(ms: i64) -> Option<String> {
    let minutes = (ms + 30_000) / 60_000;
    (minutes > 0).then(|| match (minutes / 60, minutes % 60) {
        (0, m) => nj_platform::i18n::msg::details_runtime_m(m),
        (h, m) => nj_platform::i18n::msg::details_runtime_hm(h, m),
    })
}

/// What the item still has to run, in ms, when it is something the viewer can finish: a movie or
/// an episode, or a show's on-deck episode.
fn remaining_ms(d: &Detail) -> Option<i64> {
    let (dur, at) = match hero_episode(d) {
        Some(ep) if d.is_show => (ep.dur_ms, ep.resume_ms),
        _ if d.is_show => return None,
        _ => (d.dur_ms, d.resume_ms),
    };
    (dur > 0).then(|| (dur - at.clamp(0, dur)).max(0))
}

/// `HH:MM` on the 24-hour clock, `ahead_ms` from now in the TV's local time zone — the wall clock
/// the viewer reads, the one presentation read of it on this page. `None` under a replay or a
/// controlled boot, which must render the same frame on every run.
fn clock_after(ahead_ms: i64) -> Option<String> {
    if crate::stores::tape::active() {
        return None;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    let at = (now + ahead_ms / 1000) as libc::time_t;
    // SAFETY: `localtime_r` writes only the `tm` it is handed and reads `at`; both outlive the call.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&at, &mut tm) }.is_null() {
        return None;
    }
    Some(format!("{:02}:{:02}", tm.tm_hour, tm.tm_min))
}

/// One run of the meta row, in its drawn order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MetaRun {
    /// Plain text in an ink: year, runtime, extent, Ends at, the dates.
    Text(String, [f32; 4]),
    /// The official rating on its badge.
    Rating(String),
    /// The community score behind the gold star, one decimal.
    Star(String),
    /// The critic score behind the tomato.
    Critic(String, &'static [crate::ui::widgets::MarkLayer]),
}

const META_ITEM_GAP: f32 = 20.0;
const META_SZ: c_int = theme::size::lf::S22;
const META_MARK_D: f32 = META_SZ as f32;
const META_MARK_GAP: f32 = 6.0;

/// How the screen reads a review score: its units and the critic mark it wears.
pub(crate) struct ScoreArt {
    pub(crate) scale: fn(crate::metadata::RatingArt) -> crate::ui::fmt::RatingScale,
    pub(crate) mark: fn(crate::metadata::RatingArt) -> &'static [crate::ui::widgets::MarkLayer],
}

/// The meta row's runs (5 Meta row): year, runtime (or a show's extent), the official rating
/// badge, the star and critic scores, `Ends at HH:MM`, the added and aired dates (one run joined by
/// a middot, as Litefin's `meta-item-dates`) and, for a borrowed item, whose server it is on — the
/// last three secondary. `ends_at` is the clock text the caller
/// resolved, so the runs stay a pure function of the item.
///
/// One star and one critic score, as Litefin shows: the critic is the first critic score, and the
/// star the first audience score from ANOTHER provider when there is one (Rotten Tomatoes'
/// popcorn is the critic's own source talking twice; IMDb or TMDB is the community).
pub(crate) fn meta_runs(d: &Detail, ends_at: Option<&str>, art: &ScoreArt) -> Vec<MetaRun> {
    let mut out = Vec::new();
    if d.year > 0 {
        out.push(MetaRun::Text(d.year.to_string(), theme::TEXT_PRIMARY));
    }
    if d.is_show {
        let seasons = d.seasons.len() as i64;
        if seasons > 0 {
            out.push(MetaRun::Text(nj_platform::i18n::msg::browse_detail_seasons(seasons), theme::TEXT_PRIMARY));
        }
    } else if let Some(rt) = runtime(d.dur_ms) {
        out.push(MetaRun::Text(rt, theme::TEXT_PRIMARY));
    }
    if !d.rating.is_empty() {
        out.push(MetaRun::Rating(d.rating.clone()));
    }
    let critic = d.ratings.iter().find(|r| r.critic);
    let star = d
        .ratings
        .iter()
        .filter(|r| !r.critic)
        .min_by_key(|r| critic.is_some_and(|c| c.art.provider() == r.art.provider()));
    if let Some(r) = star {
        out.push(MetaRun::Star(crate::ui::fmt::decimal(r.value, 1)));
    }
    if let Some(r) = critic {
        out.push(MetaRun::Critic(crate::ui::fmt::rating_score((art.scale)(r.art), r.value), (art.mark)(r.art)));
    }
    if let Some(at) = ends_at {
        out.push(MetaRun::Text(nj_platform::i18n::msg::details_ends_at(at), theme::TEXT_SECONDARY));
    }
    let aired = match hero_episode(d) {
        Some(ep) if d.is_show => crate::ui::fmt::pretty_date(&ep.aired, 0),
        _ => crate::ui::fmt::pretty_date(&d.aired, 0),
    };
    let added = crate::ui::fmt::pretty_date(&d.added, 0);
    let dates: Vec<String> = [
        (!added.is_empty()).then(|| nj_platform::i18n::msg::details_added(&added)),
        (!aired.is_empty()).then(|| nj_platform::i18n::msg::details_aired(&aired)),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !dates.is_empty() {
        out.push(MetaRun::Text(dates.join(" · "), theme::TEXT_SECONDARY));
    }
    if let Some(credit) = crate::ui::fmt::shared_by(&d.source()) {
        out.push(MetaRun::Text(credit, theme::TEXT_SECONDARY));
    }
    out
}

/// The `Ends at` clock text for `d` right now, or `None` when the item has no end to reach.
pub(crate) fn ends_at(d: &Detail) -> Option<String> {
    remaining_ms(d).and_then(clock_after)
}

fn meta_run_w(run: &MetaRun, measure: &dyn Measure) -> f32 {
    match run {
        MetaRun::Text(s, _) => measure.width_str(s, META_SZ, false),
        MetaRun::Rating(s) => crate::ui::widgets::LfPlate::Rating.w(s, measure),
        MetaRun::Star(s) | MetaRun::Critic(s, _) => {
            META_MARK_D + META_MARK_GAP + measure.width_str(s, META_SZ, false)
        }
    }
}

/// Draw the meta row centred on `cy` from `x`, items 20 apart, dropping whole runs that would
/// cross `right`. Returns how many runs it drew.
pub(crate) fn draw_meta(p: Painter, runs: &[MetaRun], x: f32, cy: f32, right: f32, measure: &dyn Measure) -> usize {
    let ty = nj_gfx::text::text_vcenter_y(META_SZ, 0, cy);
    let mut bx = x;
    let mut drawn = 0;
    for run in runs {
        let w = meta_run_w(run, measure);
        if bx + w > right {
            break;
        }
        let text = |p: Painter, s: &str, at: f32, ink| {
            if let Ok(c) = CString::new(s) {
                p.text(c.as_ptr(), at, ty, META_SZ, ink, 0, 0);
            }
        };
        let mark = Rect::new(bx, cy - META_MARK_D * 0.5, META_MARK_D, META_MARK_D);
        match run {
            MetaRun::Text(s, ink) => text(p, s, bx, *ink),
            MetaRun::Rating(s) => {
                crate::ui::widgets::LfPlate::Rating.draw(p, bx, cy, s, measure);
            }
            MetaRun::Star(s) => {
                crate::ui::icons::draw(p, crate::ui::icons::Icon::Star, mark, theme::RATING_STAR);
                text(p, s, bx + META_MARK_D + META_MARK_GAP, theme::TEXT_PRIMARY);
            }
            MetaRun::Critic(s, layers) => {
                for &(icon, ink) in layers.iter() {
                    crate::ui::icons::draw(p, icon, mark, ink);
                }
                text(p, s, bx + META_MARK_D + META_MARK_GAP, theme::TEXT_PRIMARY);
            }
        }
        bx += w + META_ITEM_GAP;
        drawn += 1;
    }
    drawn
}

/// The tech pills (5 Tech pills row): resolution, HDR or Dolby Vision, the video codec, the
/// primary audio track's codec and layout, CC when the file has subtitles, and how the server will
/// deliver it (the HDR-to-SDR warning wears the warm tint). Leaf items only: a show's borrowed
/// technicals describe one episode, not the show.
pub(crate) fn tech_pills(d: &Detail) -> Vec<(String, crate::ui::widgets::PillTone)> {
    use crate::ui::widgets::PillTone;
    let mut out = Vec::new();
    if d.is_show || d.part.is_empty() {
        return out;
    }
    if let Some(res) = crate::ui::fmt::resolution(&d.video_resolution, d.width, d.height) {
        out.push((res, PillTone::Plain));
    }
    if d.dovi.present {
        out.push(("Dolby Vision".to_string(), PillTone::DolbyVision));
    } else if d.hdr {
        out.push(("HDR".to_string(), PillTone::Hdr));
    }
    let vcodec = match d.vcodec.to_ascii_lowercase().as_str() {
        "" => None,
        "hevc" | "h265" => Some("HEVC".to_string()),
        "h264" | "avc" => Some("H.264".to_string()),
        other => Some(other.to_ascii_uppercase()),
    };
    out.extend(vcodec.map(|c| (c, PillTone::Plain)));
    if let Some(a) = d.audio.iter().find(|a| a.default).or_else(|| d.audio.first()) {
        let layout = match a.channels {
            1 => "1.0",
            2 => "2.0",
            6 => "5.1",
            8 => "7.1",
            _ => "",
        };
        let codec = a.codec.to_ascii_uppercase();
        let label = [codec.as_str(), layout].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ");
        if !label.is_empty() {
            out.push((label, PillTone::Plain));
        }
    }
    if !d.subs.is_empty() {
        out.push((nj_platform::i18n::msg::widgets_badge_cc().to_string(), PillTone::Plain));
    }
    if let Some(preview) = crate::route::playback_preview(d) {
        let converts = preview == crate::route::Preview::Converts;
        let mode = match preview {
            crate::route::Preview::DirectPlay => nj_platform::i18n::msg::browse_detail_direct_play(),
            crate::route::Preview::Remux => nj_platform::i18n::msg::browse_detail_direct_stream(),
            crate::route::Preview::Converts => crate::ui::fmt::converts_on_server(),
        };
        out.push((mode.to_string(), PillTone::Plain));
        if converts && d.hdr && crate::catalog::serverinfo::subscription_of(d.sid) == crate::catalog::serverinfo::Subscription::No {
            out.push(("HDR \u{2192} SDR".to_string(), PillTone::Hdr));
        }
    }
    out
}

const PILL_GAP: f32 = 10.0;

/// Draw the tech pills centred on `cy` from `x`, 10 apart, dropping the pills that would cross
/// `right`. Returns how many it drew.
pub(crate) fn draw_pills(
    p: Painter,
    pills: &[(String, crate::ui::widgets::PillTone)],
    x: f32,
    cy: f32,
    right: f32,
    measure: &dyn Measure,
) -> usize {
    let mut bx = x;
    let mut drawn = 0;
    for (label, tone) in pills {
        let plate = crate::ui::widgets::LfPlate::Tech(*tone);
        let w = plate.w(label, measure);
        if bx + w > right {
            break;
        }
        plate.draw(p, bx, cy, label, measure);
        bx += w + PILL_GAP;
        drawn += 1;
    }
    drawn
}

#[cfg(test)]
mod tests {
    use super::*;

    // TEST ONLY: this module's few call sites still read/write metadata as free functions, a
    // shape written for the old crate-global statics. A thread-confined store (same pattern as
    // `screens::detail::tests`'s `TEST_METADATA`) gives them a real, per-owner `MetadataStore`
    // without threading one through every helper here.
    thread_local! {
        static TEST_METADATA: std::cell::UnsafeCell<crate::stores::metadata::MetadataStore> =
            std::cell::UnsafeCell::new(crate::stores::metadata::MetadataStore::default());
    }

    fn test_store() -> &'static mut crate::stores::metadata::MetadataStore {
        TEST_METADATA.with(|cell| unsafe { &mut *cell.get() })
    }

    fn set(restart: bool, alt: bool, mark: PosterMark) -> HeroSet {
        set_full(restart, alt, mark, false)
    }

    fn set_full(restart: bool, alt: bool, mark: PosterMark, trailer: bool) -> HeroSet {
        HeroSet {
            restart,
            trailer,
            version: false,
            tracks: false,
            alt,
            mark,
            favorite: Some(false),
        }
    }

    /// **The row offers exactly ONE watched toggle, never both faces at once** — the property
    /// `ui/detail.rs`'s own module doc calls out by name ("this row briefly drew the two faces
    /// side by side for a part-watched item, and that was wrong").
    #[test]
    fn the_row_offers_exactly_one_watched_toggle() {
        for mark in [
            PosterMark::None,
            PosterMark::InProgress,
            PosterMark::Watched,
        ] {
            for restart in [false, true] {
                for alt in [false, true] {
                    for trailer in [false, true] {
                        let s = set_full(restart, alt, mark, trailer);
                        let (v, n) = hero_ctls(s);
                        let watch_ctls: Vec<_> = v[..n].iter().filter(|c| c.is_watch()).collect();
                        assert_eq!(watch_ctls.len(), 1, "set={s:?}");
                    }
                }
            }
        }
    }

    /// **Full-trailer mode always narrows the row's FOCUS/pointer extent to exactly `[Play]`,
    /// whatever the item's own facts offer** — swept over every `HeroSet` this row can take. The
    /// property that keeps the mode's focus anchor single and legitimate. Drawing is the OPPOSITE:
    /// `DetailScreen::draw_buttons` always paints the full `hero_ctls` row, full-trailer or not
    /// (2026-09-17 fix — the row fades out together with the rest of the chrome instead of four
    /// pills hard-cutting a frame ahead of Play), so this also pins that the two predicates
    /// genuinely diverge whenever the set has more than Play alone.
    #[test]
    fn full_trailer_mode_collapses_focus_to_play_only_but_leaves_drawing_alone() {
        for mark in [
            PosterMark::None,
            PosterMark::InProgress,
            PosterMark::Watched,
        ] {
            for restart in [false, true] {
                for alt in [false, true] {
                    for trailer in [false, true] {
                        let s = set_full(restart, alt, mark, trailer);
                        let (v, n) = visible_ctls(s, true);
                        assert_eq!(n, 1, "set={s:?}");
                        assert_eq!(v[0], HeroCtl::Play, "set={s:?}");
                        // And the non-full-trailer path must be byte-identical to `hero_ctls` —
                        // `visible_ctls` is a strict narrowing, never a second row model.
                        assert_eq!(visible_ctls(s, false), hero_ctls(s), "set={s:?}");
                        // Drawing does not narrow with `visible_ctls` — `draw_buttons` iterates
                        // `hero_ctls` directly — so drawing always sees strictly more than
                        // full-trailer mode makes focusable/pointer-reachable: the watch toggle
                        // alone (unconditional in `hero_ctls`) already outnumbers the `[Play]`
                        // focus set, before `restart`/`alt`/`trailer` add anything further.
                        let (_, drawn) = hero_ctls(s);
                        assert!(
                            drawn > n,
                            "set={s:?}: drawing must stay decoupled from the focus narrowing"
                        );
                    }
                }
            }
        }
    }

    /// The watched toggle is carried by its JOB, not by either face — `watch_index` must answer
    /// `Some` for a set wearing EITHER face, at the SAME index.
    #[test]
    fn the_watch_toggle_is_found_by_its_job_whichever_face_it_wears() {
        let unwatched = set(false, false, PosterMark::None);
        let watched = set(false, false, PosterMark::Watched);
        let i_unwatched = watch_index(unwatched).expect("always present");
        let i_watched = watch_index(watched).expect("always present");
        assert_eq!(
            i_unwatched, i_watched,
            "the toggle sits at the row's tail in both sets"
        );
        assert_eq!(ctl_at(unwatched, i_unwatched), Some(HeroCtl::MarkWatched));
        assert_eq!(ctl_at(watched, i_watched), Some(HeroCtl::MarkUnwatched));
    }

    /// A finished-then-restarted item reads `InProgress`, never `Watched` — the viewer is
    /// part-way through a re-watch, and "not watched" is the honest state for the toggle to read.
    #[test]
    fn a_restarted_finished_item_reads_in_progress_not_watched() {
        // `watched=true` but a live resume point beats it.
        assert_eq!(
            hero_watch_state(false, false, true, 4_000_000_000),
            PosterMark::InProgress
        );
    }

    /// A show mid-run (started, not finished) reads `InProgress` even with no resume point of its
    /// own — `mid_run` is `is_show && show_started && !watched`.
    #[test]
    fn a_show_mid_run_reads_in_progress_with_no_resume_point_of_its_own() {
        assert_eq!(
            hero_watch_state(true, true, false, 0),
            PosterMark::InProgress
        );
        assert_eq!(
            hero_watch_state(true, true, true, 0),
            PosterMark::Watched,
            "…but a FINISHED show is simply watched"
        );
        assert_eq!(
            hero_watch_state(true, false, false, 0),
            PosterMark::None,
            "an unstarted show is simply unstarted"
        );
    }

    /// **`hero_col`'s reconciliation, restated as identity-membership**: a restart control that
    /// vanishes from the set (a resume point was just consumed) is no longer found by
    /// `index_of`, which is exactly the signal `DetailScreen::reconcile` uses to know the focus
    /// must move off it.
    #[test]
    fn restart_disc_tracks_the_resume_the_play_would_actually_apply() {
        assert!(!has_restart(0), "no resume point, no restart disc");
        assert!(has_restart(1), "any positive resume position offers one");
        let with = set(true, false, PosterMark::None);
        let without = set(false, false, PosterMark::None);
        assert_eq!(index_of(with, HeroCtl::Restart), Some(1));
        assert_eq!(
            index_of(without, HeroCtl::Restart),
            None,
            "the disc that a completed restart just consumed is gone from the new set"
        );
    }

    /// The row's ACCUMULATED x positions never overlap and advance strictly left to right, 16
    /// apart from the info column's edge, at every set size the row can take (the property the
    /// pointer hit-test and the draw both rely on agreeing about).
    #[test]
    fn the_row_s_controls_never_overlap_at_any_set_size() {
        for restart in [false, true] {
            for alt in [false, true] {
                for trailer in [false, true] {
                    for mark in [PosterMark::None, PosterMark::Watched] {
                        let s = set_full(restart, alt, mark, trailer);
                        let (_, n) = hero_ctls(s);
                        let pill = if restart { 200.0 } else { CD };
                        let mut prev_right = crate::ui::detail_layout::INFO_X - crate::ui::widgets::ACTION_GAP;
                        for i in 0..n {
                            let r = hero_btn_rect_at(s, i, 0.0, pill);
                            assert_eq!(r.x, prev_right + crate::ui::widgets::ACTION_GAP, "set={s:?} i={i}");
                            assert_eq!(r.w, if i == 0 { pill } else { CD });
                            prev_right = r.x + r.w;
                        }
                        assert!(prev_right <= crate::ui::detail_layout::INFO_R, "set={s:?}");
                    }
                }
            }
        }
    }

    /// `ctl_at`/`index_of` are exact inverses over every index the set actually has.
    #[test]
    fn ctl_at_and_index_of_round_trip() {
        for restart in [false, true] {
            for alt in [false, true] {
                for trailer in [false, true] {
                    for mark in [
                        PosterMark::None,
                        PosterMark::InProgress,
                        PosterMark::Watched,
                    ] {
                        let s = set_full(restart, alt, mark, trailer);
                        let (_, n) = hero_ctls(s);
                        for i in 0..n {
                            let ctl = ctl_at(s, i).unwrap();
                            assert_eq!(index_of(s, ctl), Some(i));
                        }
                        assert_eq!(
                            ctl_at(s, n),
                            None,
                            "one past the end is None, never a panic"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn restart_and_the_watch_tail_are_independent() {
        let set = HeroSet {
            restart: true,
            trailer: false,
            version: false,
            tracks: false,
            alt: false,
            mark: PosterMark::Watched,
            favorite: Some(false),
        };
        let (controls, n) = hero_ctls(set);
        assert_eq!(
            &controls[..n],
            &[HeroCtl::Play, HeroCtl::Restart, HeroCtl::MarkUnwatched, HeroCtl::Favorite]
        );
        let (controls, n) = hero_ctls(HeroSet { favorite: Some(true), ..set });
        assert_eq!(controls[n - 1], HeroCtl::Unfavorite, "the favourite face follows the item");
        let (controls, n) = hero_ctls(HeroSet { favorite: None, ..set });
        assert_eq!(controls[n - 1], HeroCtl::MarkUnwatched, "a server with no favourites offers no heart");
    }

    #[test]
    fn the_actions_row_grows_an_also_available_control_only_for_a_second_source() {
        let without = HeroSet {
            restart: false,
            trailer: false,
            version: false,
            tracks: false,
            alt: false,
            mark: PosterMark::None,
            favorite: Some(false),
        };
        let with = HeroSet {
            alt: true,
            ..without
        };
        assert!(index_of(without, HeroCtl::Alt).is_none());
        assert!(index_of(with, HeroCtl::Alt).is_some());
    }

    #[test]
    /// The row carries no track INFORMATION control (that lives in the About footer's Languages
    /// column), and its one track control — the *Audio & Subtitles* pill — appears only while the
    /// item offers a track choice.
    fn the_hero_row_carries_no_track_information_disc() {
        for alt in [false, true] {
            for trailer in [false, true] {
                for tracks in [false, true] {
                    let set = HeroSet {
                        restart: true,
                        trailer,
                        version: false,
                        tracks,
                        alt,
                        mark: PosterMark::InProgress,
                        favorite: Some(false),
                    };
                    let (controls, n) = hero_ctls(set);
                    assert!(controls[..n].iter().all(|ctl| matches!(
                        ctl,
                        HeroCtl::Play
                            | HeroCtl::Restart
                            | HeroCtl::Trailer
                            | HeroCtl::Tracks
                            | HeroCtl::Alt
                            | HeroCtl::MarkWatched
                            | HeroCtl::MarkUnwatched
                            | HeroCtl::Favorite
                            | HeroCtl::Unfavorite
                    )));
                    assert_eq!(controls[..n].contains(&HeroCtl::Tracks), tracks);
                }
            }
        }
    }

    #[test]
    fn a_shows_hero_is_about_the_servers_on_deck_episode_or_the_series() {
        let mut d = Detail {
            is_show: true,
            seasons: vec![crate::metadata::Season {
                rk: "s1".into(),
                index: 1,
                title: "Season 1".into(),
                leaf_count: 3,
                viewed_leaf_count: 0,
            }],
            on_deck: Some(Episode {
                rk: "e1".into(),
                resume_ms: 0,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(
            hero_episode(&d).is_none(),
            "an untouched show is about the series"
        );
        d.seasons[0].viewed_leaf_count = 1;
        assert_eq!(hero_episode(&d).map(|ep| ep.rk.as_str()), Some("e1"));
    }

    #[test]
    fn the_watch_state_resolver_answers_leaf_and_container_by_their_own_rules() {
        assert_eq!(hero_watch_state(false, false, false, 0), PosterMark::None);
        assert_eq!(hero_watch_state(false, false, true, 0), PosterMark::Watched);
        assert_eq!(
            hero_watch_state(true, true, false, 0),
            PosterMark::InProgress
        );
        assert_eq!(hero_watch_state(true, true, true, 0), PosterMark::Watched);
    }

    #[test]
    fn the_optimistic_flip_settles_a_leaf_at_once_and_a_container_a_round_trip_late() {
        let _guard = nj_base::testlock::serial();
        let sid = crate::catalog::ServerId::UNSET;
        crate::metadata::set_current_for_test(test_store().state_mut(), Some(Detail {
            sid,
            rk: "movie".into(),
            resume_ms: 1_800_000,
            dur_ms: 7_200_000,
            ..Default::default()
        }));
        assert!(test_store().run(
            crate::stores::metadata::MetadataCmd::SetWatchedLocal { sid, rk: "movie".into(), on: true }
        ));
        let movie = test_store().view().current().unwrap();
        assert_eq!(hero_mark(movie), PosterMark::Watched);
        assert_eq!(
            movie.resume_ms, 0,
            "a leaf's restart disc disappears immediately"
        );

        crate::metadata::set_current_for_test(test_store().state_mut(), Some(Detail {
            sid,
            rk: "show".into(),
            is_show: true,
            seasons: vec![crate::metadata::Season {
                rk: "s1".into(),
                index: 1,
                title: "Season 1".into(),
                leaf_count: 2,
                viewed_leaf_count: 1,
            }],
            on_deck: Some(Episode {
                resume_ms: 600_000,
                dur_ms: 2_700_000,
                ..Default::default()
            }),
            ..Default::default()
        }));
        assert!(test_store().run(
            crate::stores::metadata::MetadataCmd::SetWatchedLocal { sid, rk: "show".into(), on: true }
        ));
        assert_eq!(
            hero_mark(test_store().view().current().unwrap()),
            PosterMark::InProgress,
            "container progress remains server evidence until the re-read lands"
        );
        crate::metadata::set_current_for_test(test_store().state_mut(), Some(Detail {
            sid,
            rk: "show".into(),
            is_show: true,
            watched: true,
            ..Default::default()
        }));
        assert_eq!(
            hero_mark(test_store().view().current().unwrap()),
            PosterMark::Watched
        );
        crate::metadata::set_current_for_test(test_store().state_mut(), None);
    }

    #[test]
    fn hero_indices_mean_different_actions_in_the_two_control_sets() {
        let compact = HeroSet {
            restart: false,
            trailer: false,
            version: false,
            tracks: false,
            alt: false,
            mark: PosterMark::None,
            favorite: Some(false),
        };
        let wide = HeroSet {
            restart: true,
            trailer: false,
            version: false,
            tracks: false,
            alt: true,
            mark: PosterMark::None,
            favorite: Some(false),
        };
        assert_eq!(ctl_at(compact, 1), Some(HeroCtl::MarkWatched));
        assert_eq!(ctl_at(wide, 1), Some(HeroCtl::Restart));
        assert_eq!(ctl_at(wide, 2), Some(HeroCtl::Alt));
    }

    /// Every control names the verb its press performs, the watched toggle by its face and by
    /// whether the hero is naming the show; Play is a circle until there is something to resume,
    /// then the Resume pill; the watched face wears the accent.
    #[test]
    fn each_control_floats_its_own_verb_and_wears_its_own_glyph() {
        assert_eq!(ctl_name(HeroCtl::Play, false, false), hero_pill_label(false));
        assert_eq!(ctl_name(HeroCtl::Play, true, false), hero_pill_label(true));
        assert_eq!(ctl_name(HeroCtl::Restart, false, false), play_from_start_label());
        assert_eq!(ctl_name(HeroCtl::Trailer, false, true), trailer_label());
        assert_eq!(ctl_name(HeroCtl::MarkWatched, false, false), mark_watched_label());
        assert_eq!(ctl_name(HeroCtl::MarkWatched, false, true), mark_show_watched_label());
        assert_eq!(ctl_name(HeroCtl::MarkUnwatched, false, false), mark_unwatched_label());
        assert_eq!(ctl_name(HeroCtl::MarkUnwatched, false, true), mark_show_unwatched_label());
        assert_eq!(ctl_name(HeroCtl::Tracks, false, false), tracks_label());
        let m = crate::ui::fixture::FixtureMeasure;
        assert_eq!(hero_pill_w(&m, false), CD);
        assert!(hero_pill_w(&m, true) > CD);
        assert!(ctl_active(HeroCtl::MarkUnwatched) && !ctl_active(HeroCtl::MarkWatched));
        assert_eq!(ctl_icon(HeroCtl::MarkWatched), ctl_icon(HeroCtl::MarkUnwatched));
        assert!(ctl_active(HeroCtl::Unfavorite) && !ctl_active(HeroCtl::Favorite), "a favourite wears the accent");
        assert_eq!((ctl_icon(HeroCtl::Favorite), ctl_icon(HeroCtl::Unfavorite)), (crate::ui::icons::Icon::Heart, crate::ui::icons::Icon::HeartFill));
        assert_eq!(ctl_name(HeroCtl::Favorite, false, false), ctl_name(HeroCtl::Unfavorite, true, true));
        assert!(ctl_peak(HeroCtl::Play) < ctl_peak(HeroCtl::Restart));
    }

    /// `2h 37m`, `52m`, nothing for no runtime, rounded to the minute.
    #[test]
    fn the_runtime_reads_hours_and_minutes() {
        let _locale = nj_platform::i18n::language_on_this_thread_for_test(nj_platform::i18n::Preference::En);
        assert_eq!(runtime(157 * 60_000).as_deref(), Some("2h 37m"));
        assert_eq!(runtime(52 * 60_000 + 20_000).as_deref(), Some("52m"));
        assert_eq!(runtime(0), None);
    }

    /// The meta row reads Litefin's order: year, runtime, rating, star, critic, Ends at, aired.
    /// One star and one critic, the star from another provider than the critic's when there is one.
    #[test]
    fn the_meta_row_reads_litefins_order_with_one_star_and_one_critic() {
        use crate::metadata::{Rating, RatingArt};
        let _locale = nj_platform::i18n::language_on_this_thread_for_test(nj_platform::i18n::Preference::En);
        let art = ScoreArt { scale: |_| crate::ui::fmt::RatingScale::Percent, mark: |_| &[] };
        let d = Detail {
            kind: "movie".into(),
            year: 2012,
            dur_ms: 12 * 60_000,
            rating: "PG-13".into(),
            aired: "2012-09-26".into(),
            ratings: vec![
                Rating { art: RatingArt::TomatoFresh, value: 7.3, critic: true },
                Rating { art: RatingArt::PopcornUpright, value: 8.0, critic: false },
                Rating { art: RatingArt::Tmdb, value: 7.1, critic: false },
            ],
            ..Default::default()
        };
        let runs = meta_runs(&d, Some("21:40"), &art);
        assert_eq!(runs[0], MetaRun::Text("2012".into(), theme::TEXT_PRIMARY));
        assert_eq!(runs[1], MetaRun::Text("12m".into(), theme::TEXT_PRIMARY));
        assert_eq!(runs[2], MetaRun::Rating("PG-13".into()));
        assert_eq!(runs[3], MetaRun::Star(crate::ui::fmt::decimal(7.1, 1)));
        assert!(matches!(&runs[4], MetaRun::Critic(s, _) if s == "73%"));
        assert_eq!(runs[5], MetaRun::Text("Ends at 21:40".into(), theme::TEXT_SECONDARY));
        assert!(matches!(&runs[6], MetaRun::Text(s, ink) if s.starts_with("Aired ") && *ink == theme::TEXT_SECONDARY));
        // the household shows the added date before the aired one, in one run (spec 0)
        let added = meta_runs(&Detail { added: "2025-11-20".into(), ..d.clone() }, Some("21:40"), &art);
        assert!(matches!(&added[6], MetaRun::Text(s, _) if s.starts_with("Added ") && s.contains(" · Aired ")), "{:?}", added[6]);
        assert_eq!(added.len(), runs.len(), "one dates run, not two");
        assert_eq!(runs.len(), 7);
    }

    /// A show's Play runs out on the on-deck episode, so its end is that episode's; an untouched
    /// show has nothing to end.
    #[test]
    fn only_something_the_viewer_can_finish_has_an_end() {
        assert_eq!(remaining_ms(&Detail { dur_ms: 100_000, resume_ms: 40_000, ..Default::default() }), Some(60_000));
        assert_eq!(remaining_ms(&Detail { is_show: true, ..Default::default() }), None);
        assert_eq!(remaining_ms(&Detail::default()), None);
    }

    /// The tech pills name the file a leaf plays and stay off a show page.
    #[test]
    fn the_tech_pills_describe_a_leafs_own_file() {
        use crate::ui::widgets::PillTone;
        let d = Detail {
            part: "/p".into(),
            video_resolution: "1080".into(),
            vcodec: "hevc".into(),
            dovi: crate::metadata::Dovi { present: true, ..Default::default() },
            audio: vec![crate::metadata::Stream { codec: "eac3".into(), channels: 6, ..Default::default() }],
            ..Default::default()
        };
        let pills = tech_pills(&d);
        assert_eq!(pills[0], ("1080p".to_string(), PillTone::Plain));
        assert_eq!(pills[1], ("Dolby Vision".to_string(), PillTone::DolbyVision));
        assert_eq!(pills[2], ("HEVC".to_string(), PillTone::Plain));
        assert_eq!(pills[3], ("EAC3 5.1".to_string(), PillTone::Plain));
        assert!(tech_pills(&Detail { is_show: true, ..d }).is_empty());
    }

    #[test]
    fn trailer_false_matches_today_s_four_control_sets() {
        let set = set(true, true, PosterMark::None);
        let (controls, n) = hero_ctls(set);
        assert_eq!(
            &controls[..n],
            &[
                HeroCtl::Play,
                HeroCtl::Restart,
                HeroCtl::Alt,
                HeroCtl::MarkWatched,
                HeroCtl::Favorite
            ]
        );
    }

    #[test]
    fn trailer_true_inserts_between_restart_and_alt() {
        let set = set_full(true, true, PosterMark::None, true);
        let (controls, n) = hero_ctls(set);
        assert_eq!(
            &controls[..n],
            &[
                HeroCtl::Play,
                HeroCtl::Restart,
                HeroCtl::Trailer,
                HeroCtl::Alt,
                HeroCtl::MarkWatched,
                HeroCtl::Favorite
            ]
        );
    }

    #[test]
    fn growing_a_trailer_disc_keeps_watch_identity() {
        let before = set(false, false, PosterMark::None);
        let after = set_full(false, false, PosterMark::None, true);
        assert_eq!(index_of(before, HeroCtl::Play), Some(0));
        assert_eq!(index_of(after, HeroCtl::Play), Some(0));
        assert_eq!(index_of(before, HeroCtl::Trailer), None);
        assert_eq!(index_of(after, HeroCtl::Trailer), Some(1));
        let watch = index_of(after, HeroCtl::MarkWatched).unwrap();
        assert_eq!(watch, index_of(before, HeroCtl::MarkWatched).unwrap() + 1);
        assert_eq!(ctl_at(after, watch).unwrap().elem(), ELEM_MARK_WATCHED);
    }

    #[test]
    fn losing_the_trailer_disc_drops_its_identity() {
        let with = set_full(true, false, PosterMark::None, true);
        let without = set(true, false, PosterMark::None);
        assert_eq!(index_of(with, HeroCtl::Trailer), Some(2));
        assert_eq!(index_of(without, HeroCtl::Trailer), None);
    }

    fn trailer_extra(rk: &str, part: &str, title: &str) -> Extra {
        Extra {
            rk: rk.into(),
            part: part.into(),
            title: title.into(),
            vcodec: "h264".into(),
            acodec: "aac".into(),
            subtype: "trailer".into(),
            extra_type: 1,
            dur_ms: 120_000,
            bitrate: 2_500,
            thumb: String::new(),
        }
    }

    #[test]
    fn trailer_play_uses_the_extra_not_the_parent_or_on_deck() {
        let extra = trailer_extra("9", "/library/parts/trailer", "Official Trailer");
        let movie = Detail {
            rk: "m".into(),
            title: "Movie".into(),
            part: "/library/parts/movie".into(),
            extras: vec![extra.clone()],
            ..Default::default()
        };
        let (got, title) = trailer_play(&movie).unwrap();
        assert_eq!(got.rk, "9");
        assert_eq!(got.part, "/library/parts/trailer");
        assert_eq!(title, "Official Trailer");
        assert_ne!(got.part, movie.part);

        let show = Detail {
            is_show: true,
            title: "Show".into(),
            on_deck: Some(Episode {
                rk: "ep".into(),
                part: "/library/parts/ep".into(),
                ..Default::default()
            }),
            extras: vec![extra],
            ..Default::default()
        };
        let (got, title) = trailer_play(&show).unwrap();
        assert_eq!(got.rk, "9");
        assert_eq!(got.part, "/library/parts/trailer");
        assert_eq!(title, "Official Trailer");
        assert_ne!(got.rk, show.on_deck.as_ref().unwrap().rk);
        assert!(trailer_play(&Detail::default()).is_none());
        assert!(
            trailer_play(&Detail {
                extras: vec![trailer_extra("", "/p", "T")],
                ..Default::default()
            })
            .is_none(),
            "a part without rk is not playable"
        );
        assert!(
            trailer_play(&Detail {
                extras: vec![trailer_extra("9", "", "T")],
                ..Default::default()
            })
            .is_none(),
            "an rk without part is not playable"
        );
        let untitled = Detail {
            title: "Parent".into(),
            extras: vec![trailer_extra("9", "/p", "")],
            ..Default::default()
        };
        assert_eq!(trailer_play(&untitled).unwrap().1, "Parent");
    }

}
