//! The player OSD, as Litefin draws it (`docs/litefin-look.md` 10): a header with the title and
//! the 24-hour clock, the controls row (transport on the left, "Ends at", tracks / chapters /
//! settings on the right) above the seek row, each on its own black gradient. Composed from retui
//! widgets (`TransportButton`, `OsdPill`, `Spinner`, the figure read-outs) drawn through a
//! `Painter`, reading the live playback state via crate::player (TX + playpos_ns/duration_ns). The
//! video-overlay subtitle draws below stay on the raw text/tex primitives (they composite directly
//! over the video plane, outside the transport HUD).
#![allow(dead_code)]
mod seek_jump;
pub(crate) use seek_jump::SeekBurst;

use nj_gfx::gfx::{delete_tex, upload_rgba};
use crate::ui::consts::{SCR_H, SCR_W};
use crate::ui::theme;
use crate::ui::theme::osd;
use crate::ui::widgets::{ControlGround, Spinner, StatusKind, StatusOverlay, TransportButton};
use crate::ui::{Env, Painter, Rect, View};
use std::ffi::CString;
use std::os::raw::{c_int, c_uint};
use std::sync::atomic::Ordering::Relaxed;

/// The HUD widgets draw purely from their own fields and ignore their Env.
fn hud_env() -> Env {
    Env::inert()
}

// The trailer transport's title (`draw_title`) is a HUD *display* title — deliberately larger than
// `theme::size::TITLE`, so it sits OUTSIDE the shared type scale as a documented carve-out (like the
// subtitle caption in `draw_subtitles`). Both are media chrome with their own legibility contract and
// both are already well above the couch floor; they're named here rather than left as bare literals.
const HUD_TITLE_SZ: i32 = 54;

/// The position the playbar SHOWS: a loading seek's target, else the live scrub preview, else the
/// real playhead. One function because the draw ([`Playbar::live`]) and the clock prewarm
/// ([`ClockWarm::queue_live`]) must resolve the same value, or the prewarm warms the wrong string.
fn display_pos_ns(loading: bool, seekdisp: i64, scrub: i64, livepos: i64) -> i64 {
    if loading && seekdisp >= 0 {
        seekdisp
    } else if scrub >= 0 {
        scrub
    } else {
        livepos
    }
}

/// **The seek row's elapsed time, rasterised AHEAD of the draw's back-buffer wait.**
///
/// The elapsed read-out is a new string once a second (proportional digits through `osd_text`, not
/// figure cells), and the draw met it cold: `FRAMEDROP … clear:12.4 … textx2:1.1` on the TV (1 to
/// 3.5 ms by the digits), once a second while the transport is up, which under an open
/// Audio/Subtitles menu (`HudPolicy::Lifted`) is the whole time. Rasterised after the frame's first
/// framebuffer command it adds to the commit; queued here it overlaps the wait and costs nothing.
///
/// So the page's `prepare` hands the text queue (drained by `app::run::prepare_window` before the
/// clear) this second's string and the NEXT one's, so a playhead crossing the boundary between
/// `prepare` and the draw still finds it resident. Formatted only when the displayed second
/// changes; offered every prepared frame, because the draw empties the queue. The total, the clock
/// and "Ends at" change at most once a minute and are not warmed.
#[derive(Default)]
pub(crate) struct ClockWarm {
    /// The (elapsed second, duration) `strings` were formatted for.
    key: Option<(i64, i64)>,
    /// Exactly as [`draw_seek_row`] hands them to `Painter::text` (`TIME_SZ`, bold).
    strings: Vec<CString>,
}

impl ClockWarm {
    /// Queue the elapsed time for the playhead the seek row would draw right now.
    pub(crate) fn queue_live(&mut self, ps: &crate::route::PlaybackSession) {
        let pos = display_pos_ns(
            crate::player::loading(ps),
            crate::player::seek_display_ns(),
            crate::player::TX.scrub_ns.load(Relaxed),
            crate::player::playpos_ns(),
        );
        self.queue(pos, crate::player::duration_ns());
    }

    /// Queue the elapsed string [`draw_seek_row`] draws at `pos_ns` of `dur_ns`, and one second on.
    fn queue(&mut self, pos_ns: i64, dur_ns: i64) {
        const SEC_NS: i64 = 1_000_000_000;
        let key = ((pos_ns / SEC_NS).max(0), dur_ns); // `osd_time` floors and clamps the same way
        if self.key != Some(key) {
            self.key = Some(key);
            let el = key.0;
            self.strings = [el, el + 1].into_iter()
                .filter_map(|t| CString::new(osd_time(t * SEC_NS, dur_ns)).ok())
                .collect();
        }
        for s in &self.strings {
            nj_gfx::text::queue_prewarm(s.as_ptr(), TIME_SZ, 1);
        }
    }
}

/// naive word-wrap to `max` chars/line (on word boundaries)
fn wrap(s: &str, max: usize) -> Vec<String> {
    if s.chars().count() <= max {
        return vec![s.to_string()];
    }
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > max {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// client-rendered subtitle line(s), bottom-center, synced to the video clock. Drawn
/// every frame independent of the transport HUD; hidden when subtitles are off or no
/// cue is active at the current position.
///
/// Subtitle download notices use the HUD's ordinary text layout. Media captions
/// themselves are rendered by the shared libass worker (or the image decoder).
pub(crate) fn draw_subtitles(hud_up: bool, suppressed: bool) {
    let cue = crate::player::sidecar::active(0, suppressed);
    let text = match cue {
        Some(t) if !t.trim().is_empty() => t,
        _ => return,
    };
    draw_subtitle_message(&text, hud_up);
}

/// Caption placement for a download or native-renderer failure message. Text size and
/// vertical placement follow the viewer's Settings > Playback picks (`route::subtitle_size`,
/// `route::subtitle_position`) — native ASS/SSA keeps its authored style instead
/// (`docs/ass-subtitles.md`).
pub(crate) fn draw_subtitle_message(text: &str, hud_up: bool) {
    let scale = subtitle_size_scale(crate::route::subtitle_size());
    let max_chars = ((42.0 / scale).round() as usize).max(20);
    let mut lines: Vec<String> = Vec::new();
    for seg in text.split('\n') {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        for l in wrap(seg, max_chars) {
            if lines.len() < MAX_CAPTION_LINES {
                lines.push(l);
            }
        }
    }
    if lines.is_empty() {
        return;
    }
    // subtitle caption: media chrome, a documented carve-out from theme::size (see HUD_TITLE_SZ)
    let sz = (36.0 * scale).round() as i32;
    let lh = CAPTION_LINE_PITCH * scale;
    let n = lines.len() as f32;
    let cx = SCR_W * 0.5;
    // sit near the bottom normally; lift above the scrubber/tabs while the HUD is up; the viewer's
    // Position pick lifts it further still, same direction, on top of that
    let baseline = (if hud_up { SUB_CEIL_Y } else { SUB_BASE_Y })
        - subtitle_position_lift(crate::route::subtitle_position());
    let block_top = baseline - n * lh;
    let ink = subtitle_ink(); // white unless the viewer picked a dimmer tone (track menu)
    let outline = theme::scrim_black(0.85);
    let p = Painter::root();
    for (i, ln) in lines.iter().enumerate() {
        let top = block_top + i as f32 * lh;
        if let Ok(cs) = CString::new(ln.as_str()) {
            // dark outline (4 offsets) then the bold caption in its tone — legible over any scene
            for (dx, dy) in [(-2.0f32, 0.0f32), (2.0, 0.0), (0.0, -2.0), (0.0, 2.0)] {
                p.text(cs.as_ptr(), cx + dx, top + 4.0 + dy, sz, outline, 1, 1);
            }
            p.text(cs.as_ptr(), cx, top + 4.0, sz, ink, 1, 1);
        }
    }
}

/// The ink both subtitle draws use this frame: the viewer's tone ([`crate::player::subtitle_tone`])
/// resolved on [`theme::SUBTITLE_INKS`]. A rung the table does not have is white, never a
/// neighbour.
pub(crate) fn subtitle_ink() -> [f32; 4] {
    subtitle_ink_for(crate::player::subtitle_tone())
}

fn subtitle_ink_for(tone: crate::catalog::session::SubtitleTone) -> [f32; 4] {
    theme::SUBTITLE_INKS
        .get(tone.index() as usize)
        .copied()
        .unwrap_or(theme::SUBTITLE_INKS[0])
}

/// The face-size multiplier for the viewer's Settings > Playback pick, applied to the plain-text
/// caption's 36 px base face ([`draw_subtitle_message`]) and its line pitch/wrap width together,
/// so a larger caption still wraps before it runs off either edge.
fn subtitle_size_scale(size: crate::catalog::session::SubtitleSize) -> f32 {
    crate::player::text_style::size_scale(size)
}

/// How far ABOVE [`SUB_BASE_Y`]/[`SUB_CEIL_Y`] the viewer's Position pick lifts the plain-text
/// caption ([`draw_subtitle_message`]) — Low is the baseline every build before this preference
/// drew, so it lifts nothing.
fn subtitle_position_lift(position: crate::catalog::session::SubtitlePosition) -> f32 {
    crate::player::text_style::position_lift(position)
}

/// Subtitle baseline: where the caption block sits with the transport DOWN, and the ceiling it
/// lifts to with the transport UP (clear of the scrubber, buttons and tabs). Both paths share
/// them: text lifts by moving its baseline, images lift by however much they overhang the
/// ceiling — otherwise a bottom-positioned PGS cue sits behind the scrim while the user seeks.
const SUB_BASE_Y: f32 = crate::player::text_style::SUB_BASE_Y;
const SUB_CEIL_Y: f32 = crate::player::text_style::SUB_CEIL_Y;

/// The most caption lines [`draw_subtitle_message`] ever stacks (the rest of a longer cue is
/// dropped), and the reason the Timing capsule's y is fixed rather than following the caption
/// block (`appkit::timing_capsule`, plan `subtitle-menu-capsule` §4): a live cue's line count changes
/// frame to frame, and a capsule that tracked it would jump under a viewer's thumb mid-hold.
const MAX_CAPTION_LINES: usize = 3;
/// The vertical pitch of one caption line in [`draw_subtitle_message`] — its 36 px face plus
/// leading.
const CAPTION_LINE_PITCH: f32 = crate::player::text_style::CAPTION_LINE_PITCH;
/// The Timing capsule's bottom y — [`SUB_BASE_Y`] cleared by the tallest caption block this app
/// ever draws ([`MAX_CAPTION_LINES`] at [`CAPTION_LINE_PITCH`]), so the capsule never overlaps
/// even a three-line cue.
pub(crate) const CAPSULE_BOTTOM_Y: f32 = SUB_BASE_Y - MAX_CAPTION_LINES as f32 * CAPTION_LINE_PITCH;

/// Map a decoded image-subtitle rect from the stream's `cw`×`ch` authoring canvas onto the video
/// rect — which is always the full panel here (the video track is authored 1920×1080; see the
/// `docs/agent-reference.md`). A subtitle canvas is the picture's own storage grid, so this is exactly the
/// stretch the TV applies to the video itself: identity for 1080p PGS, 2.67×/2.25 for a 720×480
/// NTSC VobSub, 0.5× for a 4K PGS track. `cw`/`ch` of 0 means the decoder never declared a canvas
/// — then the rect is used 1:1, which is what this path did unconditionally before.
///
/// Deliberately NOT snapped to whole pixels: this is scaled content, and the crispness contract
/// (`docs/agent-reference.md`) snaps 1:1-texel content only.
fn sub_screen_rect(r: (i32, i32, i32, i32), cw: i32, ch: i32) -> Rect {
    let sx = if cw > 0 { SCR_W / cw as f32 } else { 1.0 };
    let sy = if ch > 0 { SCR_H / ch as f32 } else { 1.0 };
    Rect::new(
        r.0 as f32 * sx,
        r.1 as f32 * sy,
        r.2 as f32 * sx,
        r.3 as f32 * sy,
    )
}

/// How far UP to shift the rects of a display set that overhang the transport, so they clear it.
/// Nothing moves while the HUD is down, and the shift never exceeds the overhanging group's own
/// top, so a set too tall to lift is clamped rather than pushed off the top of the screen.
///
/// The lift is measured over the OVERHANGING rects only, and (per [`overhangs`]) applies only to
/// them. Measuring the whole set breaks the case multi-rect adds: a set carrying a sign at
/// y≈100 and dialogue at y≈950 would clamp the lift to 100 — sliding the sign for no reason
/// while leaving the dialogue still behind the scrim. Measuring the group and moving it as ONE
/// unit is also why this is not a per-rect lift: two-line dialogue is two rects with different
/// overhangs, and lifting each by its own would collapse them onto each other.
fn hud_lift<I: Iterator<Item = Rect>>(rects: I, hud_up: bool) -> f32 {
    if !hud_up {
        return 0.0;
    }
    let (mut top, mut bottom) = (f32::MAX, f32::MIN);
    for r in rects.filter(|r| overhangs(*r)) {
        top = top.min(r.y);
        bottom = bottom.max(r.y + r.h);
    }
    if top > bottom {
        return 0.0; // nothing overhangs
    }
    (bottom - SUB_CEIL_Y).max(0.0).min(top.max(0.0))
}

/// Does this rect reach below the transport ceiling? Only such rects take the lift.
fn overhangs(r: Rect) -> bool {
    r.y + r.h > SUB_CEIL_Y
}

/// Client-rendered IMAGE subtitles (PGS/VobSub): composite the active decoded display set over
/// the video, scaled from the subtitle stream's own authoring canvas into the video rect
/// (`sub_screen_rect`) and lifted clear of the transport when `hud_up` (`hud_lift`). EVERY rect
/// of the set is drawn — two-line dialogue and sign-plus-dialogue are authored as separate rects.
/// Caches one GL texture per rect and re-uploads only when the active cue changes (every few
/// seconds); the cached rects are the unlifted screen rects, so the lift can follow the HUD
/// frame by frame without re-uploading. Main-thread only (GL). This is the image counterpart to
/// draw_subtitles — a selected track is either text or image, so at most one of the two draws a
/// cue at a time.
pub(crate) fn draw_subtitle_bitmap(cache: &mut SubtitleBitmaps, hud_up: bool) {
    let set = &mut cache.set;
    let sel = crate::player::desired_sub_idx();
    if sel < 0 {
        for (t, _) in set.drain(..) {
            delete_tex(t);
        }
        cache.bytes = 0;
        cache.key = i64::MIN;
        cache.sel = i32::MIN; // reset BOTH halves of the key — neither should prop the other up
        return;
    }
    match crate::player::active_bitmap_key(crate::player::playpos_ns()) {
        None => cache.key = i64::MIN, // gap between cues — draw nothing this frame
        Some(k) => {
            if k != cache.key || sel != cache.sel {
                let (cw, ch, rects) = match crate::player::bitmap_by_key(k) {
                    Some(v) => v,
                    None => return, // cue evicted between key lookup and fetch
                };
                // retire the surplus first, then re-spec the ids we keep: upload_rgba reuses
                // a non-zero id, so a steady 1-rect stream never allocates a texture twice.
                for (t, _) in set.drain(rects.len().min(set.len())..) {
                    delete_tex(t);
                }
                for (i, r) in rects.iter().enumerate() {
                    let dst = sub_screen_rect((r.x, r.y, r.w, r.h), cw, ch);
                    let prev = set.get(i).map_or(0, |(t, _)| *t);
                    // the store holds the set indexed; expand it once, here, per cue change
                    let rgba = r.to_rgba();
                    let tex = upload_rgba(prev, r.w, r.h, rgba.as_ptr());
                    match set.get_mut(i) {
                        Some(slot) => *slot = (tex, dst),
                        None => set.push((tex, dst)),
                    }
                }
                // The whole set is re-uploaded above, so its residency is the new set's, not a
                // running total: `set.drain(rects.len()..)` already retired any surplus and every
                // surviving id was re-spec'd to a rect of THIS display set.
                cache.bytes = rects
                    .iter()
                    .map(|r| r.w.max(0) as usize * r.h.max(0) as usize * 4)
                    .sum();
                cache.key = k;
                cache.sel = sel;
            }
            let lift = hud_lift(set.iter().map(|(_, r)| *r), hud_up);
            // the tone TINTS a bitmap: the image shader multiplies by it, so a PGS cue keeps its
            // authored colours and outline and only gives up light (white is the identity)
            let tint = subtitle_ink();
            let p = Painter::root();
            for (tex, r) in set.iter() {
                let dy = if overhangs(*r) { lift } else { 0.0 };
                p.tex(*tex, Rect::new(r.x, r.y - dy, r.w, r.h), 0.0, tint);
            }
        }
    }
}

/// **The image-subtitle display set on screen, and the two halves of its cache key** — a RENDER
/// resource of the player's instance (`PlayerScreen::render`), not module state.
///
/// It was three `static mut`s inside [`draw_subtitle_bitmap`] until restructure phase 9. The key is
/// `(track, start_ns)` and not `start_ns` alone: two image tracks of the same file routinely start
/// a display set on the SAME pts, so keying on the timestamp alone leaves the outgoing track's
/// bitmap on screen after a switch between two image tracks.
///
/// The GL names in `set` are OWNED: [`Self::release`] deletes them, and the player's instance calls
/// it when the screen is unmounted — a static could never be told that a playback had ended.
pub(crate) struct SubtitleBitmaps {
    set: Vec<(c_uint, Rect)>,
    /// The SOURCE pixels behind `set`, in bytes — `sum(w * h * 4)` over the display set's rects as
    /// they were uploaded, which the screen rects in `set` cannot answer (those are where each
    /// bitmap LANDS, scaled into the video rect, not how large the texture is).
    ///
    /// It exists so `PlayerScreen` can state what it holds of the frame's render residency
    /// (`Screen::render_report`, §8.3 rule (c)) instead of the frame plan guessing. It moves with
    /// the textures in every path that uploads, retires or releases them.
    bytes: usize,
    key: i64,
    sel: i32,
}

impl SubtitleBitmaps {
    pub(crate) const fn new() -> Self {
        Self {
            set: Vec::new(),
            bytes: 0,
            key: i64::MIN,
            sel: i32::MIN,
        }
    }
    /// What this display set holds of the frame's render residency: one texture per rect of the
    /// set, and the pixels behind them.
    pub(crate) fn render_report(&self) -> crate::ui::frame::RenderReport {
        crate::ui::frame::RenderReport {
            textures: self.set.len() as u32,
            bytes: self.bytes,
        }
    }
    /// A display set of the given SOURCE pixel sizes, without GL — the seam the residency tests
    /// use, since a host test uploads nothing (`gfx::delete_tex` no-ops under `cfg(test)`, so the
    /// stand-in ids are safe to release). The ids are stand-ins; only the count and the bytes are
    /// what anything reads off this.
    #[cfg(test)]
    pub(crate) fn stub(sizes: &[(i32, i32)]) -> Self {
        Self {
            set: sizes
                .iter()
                .enumerate()
                .map(|(i, _)| (i as c_uint + 1, Rect::new(0.0, 0.0, 0.0, 0.0)))
                .collect(),
            bytes: sizes.iter().map(|(w, h)| *w as usize * *h as usize * 4).sum(),
            key: 1,
            sel: 0,
        }
    }
    /// Retire every uploaded texture. Main thread only (GL), like every other call in this module.
    pub(crate) fn release(&mut self) {
        for (t, _) in self.set.drain(..) {
            delete_tex(t);
        }
        self.bytes = 0;
        self.key = i64::MIN;
        self.sel = i32::MIN;
    }
}

// ---- OSD geometry (`docs/litefin-look.md` 10), shared by draw_hud and the hit rects ----------
/// The OSD fade's spring constant: critically damped at ω = √250 ≈ 15.8/s, within 5 % of its
/// target after 4.74/ω ≈ 300 ms — Litefin's fade (`.osd-main` opacity transition, 10).
const FADE_K: f32 = 250.0;
/// The OSD's side edge. Litefin pads 38.4 ([`osd::BOTTOM_PAD_X`]); the overscan keep-out is wider
/// and wins, so every control stays inside [`crate::ui::consts::SAFE`] (LG checklist item 2).
const OSD_X: f32 = crate::ui::consts::MARGIN_X;
const OSD_RIGHT: f32 = SCR_W - OSD_X;
/// The seek row: [`osd::TIME_H`] tall, its foot on the safe frame (Litefin's 48 bottom pad sits
/// inside the 54 keep-out, so the keep-out wins here too).
const SEEK_Y: f32 = SCR_H - crate::ui::consts::MARGIN_Y - osd::TIME_H;
const SEEK_CY: f32 = SEEK_Y + osd::TIME_H * 0.5;
/// Controls row foot to seek row top: the row's 9.6 bottom margin plus the bottom area's 16 between
/// its children (`player-osd.css` `.osd-controls-row`, `.osd-bottom > * + *`).
const ROW_SEEK_GAP: f32 = 25.6;
/// The controls row: one play circle tall, its items centred on [`ROW_CY`].
const ROW_Y: f32 = SEEK_Y - ROW_SEEK_GAP - osd::PLAY_D;
const ROW_CY: f32 = ROW_Y + osd::PLAY_D * 0.5;
/// The bottom gradient's top edge: 160 above the controls row (10 Bottom area).
const GRADIENT_TOP: f32 = ROW_Y - osd::BOTTOM_PAD_TOP;
/// The time read-outs' side padding (10 Seek row, `padding: 0 9.6`).
const TIME_PAD_X: f32 = 9.6;
/// "Ends at" starts this far after the left group (10 Controls row).
const ENDS_AT_GAP: f32 = 22.4;
/// The OSD's type: title 28.8, clock 22.4, times and "Ends at" 19.2, seek tooltip 26.88 (2.2), on
/// the nearest Litefin rungs.
const TITLE_SZ: c_int = theme::size::lf::S29;
const CLOCK_SZ: c_int = theme::size::lf::S22;
const TIME_SZ: c_int = theme::size::lf::S19;
const TOOLTIP_SZ: c_int = theme::size::lf::S27;

// The control row, exported for the panels that align to it (`track_menu`, `more_menu`) and for the
// stand-ins (`skip_pill`, `up_next`), which register their own rects through `ControlSlot::item_rect`.
/// control-row height (the play circle's diameter)
pub(crate) const CTRL_H: f32 = osd::PLAY_D;
/// control-row top
pub(crate) const CTRL_Y: f32 = ROW_Y;
/// The OSD's right edge — the controls' and the panels' (`track_menu`/`more_menu`) one right edge.
pub(crate) const CTRL_RIGHT: f32 = OSD_RIGHT;

/// The controls row's items, by index. 0..=4 keep the numbers `app::playback::activate_player_row`
/// dispatches on (Subtitles, Audio, Settings, previous and next item); 5.. are Litefin's transport
/// and chapter controls, which `PlayerScreen` performs itself.
pub(crate) const BTN_SUBTITLES: i32 = 0;
pub(crate) const BTN_AUDIO: i32 = 1;
/// The Settings disc: Litefin's gear, which opens the quality / stats menu (`more_menu`).
pub(crate) const BTN_MORE: i32 = 2;
pub(crate) const BTN_PREVIOUS: i32 = 3;
pub(crate) const BTN_NEXT: i32 = 4;
pub(crate) const BTN_PREV_CHAPTER: i32 = 5;
pub(crate) const BTN_REWIND: i32 = 6;
pub(crate) const BTN_PLAY: i32 = 7;
pub(crate) const BTN_FORWARD: i32 = 8;
pub(crate) const BTN_NEXT_CHAPTER: i32 = 9;
pub(crate) const BTN_CHAPTERS: i32 = 10;
/// how many items the controls row holds
const BTN_N: i32 = 11;
/// Left group, left to right (10 Controls row).
const LEFT_GROUP: [i32; 7] =
    [BTN_PREVIOUS, BTN_PREV_CHAPTER, BTN_REWIND, BTN_PLAY, BTN_FORWARD, BTN_NEXT_CHAPTER, BTN_NEXT];
/// Right group, left to right. Litefin's Queue button is not here: this app has no queue panel.
const RIGHT_GROUP: [i32; 4] = [BTN_SUBTITLES, BTN_AUDIO, BTN_CHAPTERS, BTN_MORE];

/// The CONTROL ROW's focus pop — one spring per item ([`crate::ui::widgets::CtlPop`]), so the
/// control being left shrinks while the arriving one grows instead of both snapping.
///
/// **One array for all three slot occupants**, not one per module. The OSD buttons, the Skip
/// button and the Up Next pair are the one row the cursor `hud_nav.btn` walks ([`ControlSlot::items`]),
/// only ever one occupant at a time, so the pop is a property of that row and not of whichever
/// module happens to draw it this frame. `up_next` and `skip_pill` read theirs through
/// [`TransportRow::pop`].
///
/// A field of [`TransportRow`], which the player's instance owns: the HUD keeps no view struct
/// — it is drawn entirely from the caller's arguments — so this and the resume clock beside it are
/// the whole of the route's retained motion state.
pub(crate) type RowPop = crate::ui::widgets::CtlPop<{ BTN_N as usize }>;

/// **Everything the transport row remembers between frames**: the control row's per-item focus pop
/// and the OSD's fade.
///
/// The pop was a `static mut` (`ROW_POP`) until restructure phase 9. It is LOGICAL state, not a
/// render cache — a spring whose position a viewer can see — so it lives on `PlayerScreen` and is
/// stepped from its `Tick`, exactly where [`step`](Self::step)'s doc says it must be.
pub(crate) struct TransportRow {
    pop: RowPop,
    /// The OSD's opacity: Litefin fades it in and out over 300 ms (`.osd-main`, 10).
    fade: crate::ui::Spring,
}

impl Default for TransportRow {
    fn default() -> Self {
        Self::new()
    }
}

impl TransportRow {
    pub(crate) fn new() -> Self {
        Self { pop: RowPop::new(), fade: crate::ui::Spring::at(0.0) }
    }

    /// Step the control row's focus pop — once per FRAME, from the player instance's `Tick`.
    ///
    /// **Not from [`draw_hud`]**, though that is where every other number this row draws comes
    /// from: a spring advanced inside a draw advances once per DRAW, and this row is not drawn on
    /// every frame of the route (a failure read-out owns the frame, the Chapters strip takes the
    /// transport away). The pop would then run at a rate that depended on which overlay was open.
    ///
    /// `focus == 1` is the control row; anything else closes every pop. The index is bounded by
    /// the CURRENT occupant's item count, so a stale `btn` left over from a wider slot cannot pop a
    /// control the narrower one does not have.
    pub(crate) fn step(&mut self, slot: ControlSlot, focus: i32, btn: i32, dt: f32) {
        let f = (focus == 1)
            .then(|| usize::try_from(btn).ok())
            .flatten()
            .filter(|&i| i < slot.items().max(0) as usize);
        self.pop.step(f, dt);
    }

    /// Step the OSD's fade toward shown (1) or hidden (0) — once per frame, from the same `Tick`.
    /// A critically damped spring on [`FADE_K`] settles in Litefin's 300 ms, and like every spring
    /// it reports its motion to the frame gate while it moves and nothing once it rests. It LANDS on
    /// its target: the spring alone only approaches 0, and the draw runs while the fade is above it.
    pub(crate) fn fade_step(&mut self, shown: bool, dt: f32) {
        let target = if shown { 1.0 } else { 0.0 };
        if !self.fade.land(target, dt) {
            self.fade.step(target, FADE_K, dt);
        }
    }

    /// The OSD's opacity this frame, 0..1.
    pub(crate) fn fade(&self) -> f32 {
        self.fade.pos.clamp(0.0, 1.0)
    }

    /// Control `i`'s focus pop toward a control's own focused scale `to` — Litefin pops each
    /// control family to its own size (OSD buttons 1.08, the Up Next buttons 1.05) — with the
    /// press dip folded in. The row's springs travel 1.0 → [`crate::ui::widgets::CTRL_FOCUS_SCALE`];
    /// this re-reads that travel as a fraction and lands it on `to`.
    pub(crate) fn pop(&self, i: c_int, to: f32) -> f32 {
        let i = i.max(0) as usize;
        let rest = self.pop.scale_with(i, 0.0);
        let dip = self.pop.scale(i) / rest;
        (1.0 + self.focus_t(i) * (to - 1.0)) * dip
    }

    /// How far control `i`'s pop has travelled toward focus, 0 at rest and 1 settled.
    fn focus_t(&self, i: usize) -> f32 {
        let s = self.pop.scale_with(i, 0.0);
        ((s - 1.0) / (crate::ui::widgets::CTRL_FOCUS_SCALE - 1.0)).clamp(0.0, 1.0)
    }

}

/// What the previous/next-item and chapter controls can do for the item on screen, sampled with
/// the row once per frame. Every control keeps its place either way (Litefin dims a control it
/// cannot use rather than closing the gap); only the Chapters button leaves the row without
/// chapters, as Litefin's does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Controls {
    pub(crate) previous: bool,
    pub(crate) next: bool,
    pub(crate) restart: bool,
    pub(crate) chapters: bool,
}
impl Controls {
    pub(crate) const NONE: Self = Self { previous: false, next: false, restart: false, chapters: false };
    /// `enabled` is the Previous/Next buttons preference; previous restarts the item once it is
    /// 5 s in (`activate_player_row`), so `restart` alone enables it.
    pub(crate) fn for_item(enabled: bool, previous: bool, next: bool, restart: bool, chapters: bool) -> Self {
        Self { previous: enabled && (previous || restart), next: enabled && next,
            restart: enabled && restart, chapters }
    }
    fn live(ps: &crate::route::PlaybackSession, chapters: bool) -> Self {
        let (previous, next) = crate::route::episode_neighbours(ps);
        Self::for_item(crate::route::episode_navigation() == crate::route::EpisodeNavigation::On,
            previous, next, crate::player::intended_pos_ns(ps) >= 5_000_000_000, chapters)
    }
    pub(crate) fn enabled(self, index: i32) -> bool {
        match index {
            BTN_SUBTITLES | BTN_AUDIO | BTN_MORE | BTN_REWIND | BTN_PLAY | BTN_FORWARD => true,
            BTN_PREVIOUS => self.previous,
            BTN_NEXT => self.next,
            BTN_PREV_CHAPTER | BTN_NEXT_CHAPTER | BTN_CHAPTERS => self.chapters,
            _ => false,
        }
    }
    pub(crate) fn fingerprint(self) -> u64 {
        u64::from(self.previous) | u64::from(self.next) << 1 | u64::from(self.restart) << 2
            | u64::from(self.chapters) << 3
    }
}

/// What currently occupies the transport's control row.
///
/// This is the one concept the skip / Up Next feature introduces, and it exists as a TYPE because
/// the alternative — re-deriving the three-way choice at each of the draw, the OK handler, the
/// pointer handler, the LEFT/RIGHT walk and the hit rects — encodes its precedence in the order of
/// five separate if-chains, with nothing to keep them agreeing and nothing to test.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlSlot {
    /// the OSD's buttons: the transport group on the left, Subtitles / Audio / Chapters / Settings
    /// on the right
    Discs(Controls),
    /// a marker segment is under the playhead: the Skip button takes the row
    Skip(crate::appkit::skip_pill::Prompt),
    /// …and the show has another episode queued, which outranks skipping the credits: the Up Next
    /// card takes the row. Carries the segment for the same reason `Skip` does — so the row has a
    /// stable IDENTITY.
    UpNext(crate::metadata::Marker),
}

impl ControlSlot {
    /// How many items the row holds (a disabled control keeps its index).
    /// `enabled` is the common focus/hit eligibility predicate.
    pub(crate) fn items(self) -> c_int {
        match self {
            ControlSlot::Discs(_) => BTN_N,
            // Hide + Play now — the one stand-in with a pair
            ControlSlot::UpNext(_) => 2,
            ControlSlot::Skip(_) => 1,
        }
    }
    /// Where the focus ring parks when this row APPEARS: play/pause on the OSD, item 0 on the Skip
    /// button. Up Next's primary is not item 0: the countdown's cancel rule is a steady state
    /// (`up_next::countdown_may_run`), so parking on item 0 there would disarm the timer on the
    /// frame after it armed.
    pub(crate) fn primary_btn(self) -> c_int {
        match self {
            ControlSlot::UpNext(_) => crate::appkit::up_next::PRIMARY_BTN,
            ControlSlot::Discs(_) => BTN_PLAY,
            ControlSlot::Skip(_) => 0,
        }
    }
    pub(crate) fn enabled(self, index: i32) -> bool {
        index >= 0 && index < self.items() && match self { Self::Discs(c) => c.enabled(index), _ => true }
    }
    /// LEFT/RIGHT walk the drawn order, skipping a control that cannot be used, and stop at the ends.
    pub(crate) fn neighbour(self, from: i32, forward: bool) -> i32 {
        const ORDER: [i32; BTN_N as usize] = [BTN_PREVIOUS, BTN_PREV_CHAPTER, BTN_REWIND, BTN_PLAY,
            BTN_FORWARD, BTN_NEXT_CHAPTER, BTN_NEXT, BTN_SUBTITLES, BTN_AUDIO, BTN_CHAPTERS, BTN_MORE];
        let order: &[i32] = match self {
            ControlSlot::Discs(_) => &ORDER,
            ControlSlot::UpNext(_) => &[crate::appkit::up_next::BTN_NEXT, crate::appkit::up_next::BTN_CREDITS],
            ControlSlot::Skip(_) => &[0],
        };
        let Some(at) = order.iter().position(|&i| i == from) else { return self.primary_btn(); };
        let next = if forward { order[at + 1..].iter().copied().find(|&i| self.enabled(i)) }
            else { order[..at].iter().rev().copied().find(|&i| self.enabled(i)) };
        next.unwrap_or(from)
    }
    /// whether the OSD buttons are the current occupant
    pub(crate) fn is_discs(self) -> bool {
        matches!(self, ControlSlot::Discs(_))
    }
    /// Which SEGMENT this row is offering, if any — the row's identity for "have I already offered
    /// this?". Deliberately not the `ControlSlot` itself: `active_marker` is gated on `is_playing`,
    /// so a momentary drop out of Playing mid-segment reads as "no segment" and flips the slot to
    /// `Discs` and back. Keyed on the segment, that round trip is not a new offer; keyed on the
    /// slot, it was — and every flicker re-raised the HUD over an intro the user was just watching.
    pub(crate) fn offer(self) -> Option<(crate::metadata::MarkerKind, i64)> {
        match self {
            ControlSlot::Discs(_) => None,
            ControlSlot::Skip(pr) => Some((pr.marker.kind, pr.marker.start_ms)),
            ControlSlot::UpNext(m) => Some((m.kind, m.start_ms)),
        }
    }
    /// **Item `idx` of whatever occupies the row, at the rect it is DRAWN at** — the one
    /// geometry `PlayerScreen`'s `Focusable::place` answers with, and therefore the rect its stop
    /// is registered at, for all three occupants and every item of each (`0..items()`). `None`
    /// past the occupant's last item, and for a control the row does not draw (Chapters on an item
    /// without chapters).
    pub(crate) fn item_rect(self, idx: c_int, measure: &dyn nj_machine::machine::Measure) -> Option<Rect> {
        if idx < 0 || idx >= self.items() {
            return None;
        }
        match self {
            ControlSlot::Discs(c) => btn_rect(c, idx),
            ControlSlot::Skip(pr) => Some(crate::appkit::skip_pill::rect(pr, measure)),
            ControlSlot::UpNext(_) => {
                let l = crate::appkit::up_next::layout(measure);
                Some(if idx == crate::appkit::up_next::BTN_NEXT { l.next } else { l.hide })
            }
        }
    }
    /// The row GROUP's extent: the union of every item it draws.
    pub(crate) fn extent(self, measure: &dyn nj_machine::machine::Measure) -> Rect {
        (0..self.items()).filter_map(|i| self.item_rect(i, measure)).reduce(|a, b| a.union(b))
            .unwrap_or(Rect::new(OSD_X, ROW_Y, OSD_RIGHT - OSD_X, CTRL_H))
    }
}

/// PURE precedence: given the segment under the playhead and whether the queue has a successor,
/// which control owns the row. Host-testable, and the ONLY place the ordering is written down.
///
/// Up Next outranks Skip Credits deliberately: with somewhere to go, "next episode" is the better
/// offer, and Skip Credits stays for the last episode of a show, where there is nowhere to go.
/// Only a `final` credits marker (one that runs to the end of the item) qualifies: a mid-item
/// credits segment is followed by more of the episode, so it keeps the Skip Credits button.
///
/// The Next episode preference only changes the offer for that final-credits-with-a-successor
/// case: `AfterCredits` and `Off` offer nothing there (the buttons stay), because the successor is
/// not announced until the stream ends, if then. The last episode and films keep Skip Credits.
pub(crate) fn slot_for(marker: Option<crate::metadata::Marker>, has_next: bool, mode: crate::route::NextEpisodeMode) -> ControlSlot {
    match marker {
        Some(m) => {
            let pr = crate::appkit::skip_pill::prompt_for(m);
            if has_next && m.kind == crate::metadata::MarkerKind::Credits && m.final_seg {
                match mode {
                    crate::route::NextEpisodeMode::Countdown => ControlSlot::UpNext(m),
                    crate::route::NextEpisodeMode::AfterCredits | crate::route::NextEpisodeMode::Off => ControlSlot::Discs(Controls::NONE),
                }
            } else {
                ControlSlot::Skip(pr)
            }
        }
        None => ControlSlot::Discs(Controls::NONE),
    }
}

/// Sample the live globals ONCE and resolve the row. Call this once per frame and pass the result
/// around: `playpos_ns` is written by LG's media thread and `player::pump` runs between the input
/// handlers and the draw, so re-deriving per call site let a keypress dispatch to a control that
/// the same frame then declined to draw.
pub(crate) fn slot(ps: &crate::route::PlaybackSession, meta: crate::metadata::MetadataView<'_>) -> ControlSlot {
    let has_next = crate::route::up_next(ps).is_some();
    let mode = crate::route::next_episode_mode();
    // The data layer does not read the player: the playhead is sampled here, once, and handed in.
    let head = crate::metadata::Playhead {
        playing: crate::player::is_playing(ps),
        pos_ns: crate::player::playpos_ns(),
        dur_ns: crate::player::duration_ns(),
    };
    // Server marker first; the synthesized tail only exists where credits DETECTION does not
    // (a Plex Pass server feature) — see `metadata::synthesized_tail_marker` — and only to feed
    // the Up Next countdown, so it is not even computed in the other modes.
    let m = meta.active_marker(head).or_else(|| {
        (mode == crate::route::NextEpisodeMode::Countdown)
            .then(|| meta.synthesized_tail_marker(head, has_next))
            .flatten()
    });
    match slot_for(m, has_next, mode) {
        ControlSlot::Discs(_) => ControlSlot::Discs(Controls::live(ps, crate::appkit::chapters_panel::has_chapters(meta))),
        slot => slot,
    }
}

/// PURE edge: has a stand-in just vanished OUT FROM UNDER the focus ring, so the ring has to go
/// back to the scrubber? `focused` is whether the control row currently holds focus. `was_standin`
/// is the occupant on the last OVERLAY-FREE frame, not simply the last frame — the caller only
/// samples it while the transport is bare, so an edge that happens behind an open track menu / Info
/// card is held and fires on the frame the overlay closes, rather than being lost.
///
/// It exists because the row swapping from a Skip button back to the OSD buttons leaves focus on
/// the row with `btn` still 0, so the next OK opens the SUBTITLES menu instead of toggling pause.
///
/// **It is an EDGE, not a steady state**, and that is the whole reason it is a named function with
/// a test. Written inline as `is_discs() && focused` it is also true on every frame of a user who
/// deliberately walked UP to the buttons — the ring is then yanked back to the scrubber the same
/// frame it arrives, so focus can never rest on a button and OK on one is unreachable by remote.
pub(crate) fn standin_left_the_ring(was_standin: bool, now: ControlSlot, focused: bool) -> bool {
    was_standin && now.is_discs() && focused
}

/// The bottom gradient's height — the transport's dark ground, transparent at its top edge and
/// [`osd::GRADIENT_FROM`] at the panel bottom. Named because the read-out's placement is graded
/// against it (see [`readout_frame`] and its test).
const SCRIM_H: f32 = SCR_H - GRADIENT_TOP;

/// Which surface owns the "the pipeline is working" signal. There is exactly ONE, ever.
///
/// Two of them used to fire together for the whole of every load and every seek: the centred
/// [`StatusOverlay`] was gated on `is_busy() && frames() == 0`, but `pump` zeroes `frames` as part
/// of APPLYING a seek — so the guard that was meant to say "we have never had a picture" was true
/// for the entire seek, and the transport's own spinner (gated on plain `is_busy()`) was up beside
/// it. Two indicators for one fact read as two facts, which is exactly how it was reported.
///
/// It carries the caption rather than letting each draw re-read `state().caption()`: `Resolving` is
/// derived from `route::play_pending()`, an off-thread flag, so a second read could disagree with
/// the kind this value was chosen for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Busy {
    /// nobody — frames are on the panel, or there is no session
    None,
    /// the centred [`StatusOverlay`] at [`readout_frame`], carrying its treatment + its caption
    Readout(StatusKind, &'static core::ffi::CStr),
    /// the inline spinner beside the elapsed clock
    Transport,
}

/// PURE: which surface owns the busy signal, given the playback state and whether this SESSION has
/// ever presented a frame (`player::seen_frame` — NOT `frames() > 0`, see [`Busy`]).
///
/// **The rule: whichever surface owns the thing being waited for.** No picture yet → the wait is
/// about the whole panel, so the whole panel says so. A picture is up and only the POSITION is in
/// flight → the wait is about the playhead, so the mark goes on the playhead, which the HUD has
/// already frozen at the seek target. `Error` → the centred read-out either way; there is no
/// picture and no position, only a message.
///
/// It takes `seen_frame` rather than the state alone because `Buffering` is genuinely ambiguous —
/// it is both the cold-start tail AND the 1-3 frame tail of every seek (between prime→Play, where
/// `engine` clears `seeking`, and the first presented frame). Keyed on the state alone, one of
/// those two flashes the wrong surface every time.
pub(crate) fn busy_surface(ps: &crate::route::PlaybackSession, st: crate::player::PlaybackState, seen_frame: bool) -> Busy {
    use crate::player::PlaybackState;
    match st {
        // not `st.caption()`: the Error caption is shaped by WHY (an audio-only stream names the
        // server; see `player::error_shape`), which a method on the bare state cannot know.
        PlaybackState::Error => Busy::Readout(StatusKind::Failed, crate::player::error_caption(ps)),
        s if s.is_busy() && !seen_frame => Busy::Readout(StatusKind::Working, st.caption()),
        s if s.is_busy() => Busy::Transport,
        _ => Busy::None,
    }
}

/// PURE: does the read-out own the WHOLE frame, so [`draw_hud`] draws nothing at all?
///
/// **Only a failure**, and the asymmetry is the point. A `Failed` read-out is terminal: there is no
/// picture and no position, so every piece of transport under it is a lie — `Player Screen.dc.html`
/// hides both the transport (`hudDisplay:none`) and the bottom tabs (`tabsDisplay:none`) on that
/// variant. A `Working` read-out is the opposite: the pipeline is mid-flight, the position is real,
/// and the transport is what the user reads the moment the first frame lands. Hiding it there would
/// blank the HUD through every cold start and every pre-roll — which is why this asks the KIND and
/// not merely "is a read-out up".
pub(crate) fn readout_owns_frame(busy: Busy) -> bool {
    matches!(busy, Busy::Readout(StatusKind::Failed, _))
}

/// The same question for [`crate::screens::player::PlayerScreen::handle_key`], which asks it
/// FIRST, before any transport arm: is the transport (and every panel over it) absent from the
/// frame right now?
///
/// It was `app/run.rs`'s own guard, high in that ladder's chain, until restructure phase 12
/// (PX-PLAYER) retired the loop's player input path — so the precedence that used to be an arm's
/// HEIGHT is one condition at the top of one function.
///
/// A control that is not drawn must not be activatable — the rule `PlayerScreen::record_stops` keeps for
/// the pointer, at the row's own altitude. Without this the failure was hidden but
/// still drivable: `start_playback` stamps a ~4.5 s HUD linger on the way in, so a `/decision`
/// refusal lands with `hud_visible` true and focus parked on the scrubber, and two blind presses
/// (DOWN, OK) opened the Info card over the read-out from a tab row nothing had painted.
///
/// It resamples [`busy`] rather than taking one, because the event loop runs before the frame's
/// single resolve exists; both reads are of the same main-thread state within one iteration.
pub(crate) fn transport_hidden(ps: &crate::route::PlaybackSession) -> bool {
    readout_owns_frame(busy(ps))
}

/// Sample the live globals ONCE and resolve the owner. Call this once per frame and pass the result
/// to both [`draw_hud`] and [`draw_readout`] — the same discipline [`slot`] keeps, and for the same
/// reason: two independent derivations of one three-way choice is how the two indicators drifted
/// apart in the first place.
pub(crate) fn busy(ps: &crate::route::PlaybackSession) -> Busy {
    // dev: `/tmp/nativejelly-failtest` forces the failure read-out — the other half of
    // `player::failtest_arm`, which shapes WHICH failure. It is forced HERE, on the one impure
    // sampler, rather than in `player::state()`: the pump acts on that state, and a dev switch
    // that made the engine believe it had failed would be testing a different thing than the
    // screen. `busy_surface` stays pure and ungated, so what draws is still the real rule.
    if nj_base::devtrig::flag("failtest") {
        return Busy::Readout(StatusKind::Failed, crate::player::error_caption(ps));
    }
    busy_surface(ps, crate::player::state(ps), crate::player::seen_frame())
}

// ---- the seek row's wait mark ------------------------------------------------------------------

/// PURE: does the seek row show the inline [`Spinner`] in the thumb's place? Only while the
/// pipeline is being waited on with NO travel to show: `Busy::Transport` is the seek the user asked
/// for but also a re-buffer nobody asked for, and a seek still travelling (its target, `travel_ns`,
/// away from the published playhead `pos_ns`) is already said by the thumb and the seek tooltip.
/// What is left is a mid-play re-buffer and the prime→first-frame tail of a landed seek. Litefin's
/// OSD says the rest of the transport state with its own controls (the play/pause circle).
pub(crate) fn seek_waiting(busy: Busy, travel_ns: i64, pos_ns: i64) -> bool {
    busy == Busy::Transport && travel_ns == pos_ns
}

/// The read-out's frame: **the whole panel**, because the wait is about the whole picture.
///
/// It is deliberately NOT carved down to dodge the transport. The block [`StatusOverlay`] builds is
/// only ~91 px tall, so centred on the panel it spans y≈482…573 — clear of the scrim's top edge at
/// `SCR_H - SCRIM_H` (610) and of the transport's highest ink (the Up Next still at ≈597). The
/// carve-out this replaces (`SCR_H - 340`) centred it at y=370, visibly high, and bought nothing:
/// at 370 the block was already 200 px above a scrim that is fully TRANSPARENT at its top edge
/// anyway. Every "sit above the transport" framing pushes it UP — the un-scrimmed band's own centre
/// is 305 — which is the opposite of the fix the report asked for.
fn readout_frame() -> Rect {
    Rect::FULL
}

/// The load / seek / failure read-out. Drawn EVERY player frame, independent of the transport's
/// auto-hide, because its visibility is the PLAYBACK STATE and not the HUD timer.
///
/// It used to live inside [`draw_hud`], and two things came of that. Historically: `PlaybackState`
/// was published every frame and NOTHING read it, so the initial load drew a live-looking transport
/// at 0:00 / -0:00 and a dead producer drew a fully black screen with no message and no hint that
/// BACK is the way out. Then, once it did read it: in `Error` — which is deliberately not
/// `is_busy()`, hence not covered by `app.rs`'s `|| player::loading()` HUD pin — "Playback failed"
/// disappeared 4.5 s in with the HUD linger, leaving exactly the silent black screen the read-out
/// exists to prevent. A read-out is not transport chrome.
pub(crate) fn draw_readout(
    ps: &crate::route::PlaybackSession,
    busy: Busy,
    now: u32,
    failure_focus: usize,
    measure: &dyn nj_machine::machine::Measure,
) {
    let Busy::Readout(kind, caption) = busy else {
        return;
    };
    if kind == StatusKind::Failed {
        // The failure is a page read-out (`FailureReadout`): the shared page placement, a
        // constant verdict, one reason, a row of controls. The caption is not drawn here: the
        // verdict line is constant by design, so a viewer who has seen it once recognises it
        // before reading, and the caption's suffix is re-derived as the reason.
        draw_failed_readout(ps, Painter::root(), failure_focus, measure);
        return;
    }
    StatusOverlay::new(readout_frame(), caption, kind)
        .phase(now)
        .draw(&hud_env(), Painter::root());
}

// ---- the failure read-out -------------------------------------------------------------------
//
// **The shared page read-out, not a layout of its own.** A failed playback is a page that failed,
// exactly as Home's, a Library section's and the sign-in read-out are, so it is the same
// component at the same placement: `StatusOverlay::page` hangs the verdict from
// `FULL_ANCHOR_TOP` (540) with its 112 px glyph 44 px above it, the reason in the reserved
// two-line slot, and a row of real controls stacked directly under it. Until 2026-09-28 this was
// a bespoke composition (a 96 px glyph, a primary-ink verdict, three centred prose lines and key
// caps drawn into the sentences, a support line at the same weight as the reason); the owner's
// photograph of it is why it is not any more.
//
// What each block says is decided in ONE place per question:
//  * the controls — `player::failure_actions`, the table that offers an action only if it can
//    change the outcome;
//  * the words — `player::error_now`'s `ErrorShape`, resolved to one reason sentence and at most
//    one quiet note here (`FailureReadout::of`);
//  * the support facts (version · firmware · set · failure code) — one dim footer line at the
//    foot of the screen, always; there is no *Details* control.

/// The subscription's name in the one note that states it ("This server has no Plex Pass"). A
/// product name, so it is not translated.
const PLEX_PASS: &str = "Plex Pass";

/// The support line's cap top this far above the canvas bottom — inside the safe area's
/// bottom margin band's upper edge, clear of the row by construction, and
/// `the_diagnostics_footer_sits_below_the_row_inside_the_safe_bottom_band` fails if a taller row or
/// note ever meets it.
const FOOTER_TOP: f32 = SCR_H - crate::ui::consts::MARGIN_Y - theme::space::XL;

/// Everything the failure read-out draws, resolved once from the live failure. The draw, the
/// pointer stops and the key ladder all build it the same way, so the row a click lands on is the
/// row that was drawn.
pub(crate) struct FailureReadout {
    glyph: crate::ui::icons::Icon,
    reason: CString,
    note: Option<CString>,
    actions: Vec<crate::player::FailureAction>,
    footer: CString,
}

/// A control's label. Sentence case, as every `StatusOverlay` row is (the sign-in read-out's *Try
/// again*): these are capsule buttons, not the ALL-CAPS clickable TEXT marks.
pub(crate) fn failure_action_label(a: crate::player::FailureAction) -> &'static std::ffi::CStr {
    use nj_platform::i18n::msg;
    use crate::player::FailureAction as A;
    match a {
        A::PlayAutomatically => msg::widgets_failure_play_auto_c(),
        A::TryAgain => msg::browse_action_retry_c(),
        A::ChangeQuality => msg::widgets_failure_change_quality_c(),
        A::Repair => msg::widgets_repair_action_c(),
        A::Back => msg::settings_back_c(),
    }
}

/// The glyph above the verdict, by cause — read off the same `FailureKind` the words come from, so
/// the two cannot disagree (the rule `StatusOverlay::page` states for its callers).
fn failure_glyph(kind: crate::player::FailureKind) -> crate::ui::icons::Icon {
    use crate::player::FailureKind as K;
    use crate::ui::icons::Icon;
    match kind {
        K::DecisionRefused | K::NoVideoTranscodeTarget => Icon::ServerBadgeXmark,
        K::JailMissingRtkmem => Icon::LockBadgeAlert,
        K::LoadTimeout | K::ReleaseTimeout => Icon::ClockBadgeAlert,
        _ => Icon::Alert,
    }
}

impl FailureReadout {
    /// The read-out for the failure on screen now (main thread).
    pub(crate) fn now(ps: &crate::route::PlaybackSession) -> Self {
        let mut e = crate::player::error_now(ps);
        if e.kind == crate::player::FailureKind::JailMissingRtkmem {
            use nj_platform::tv::sandbox::State;
            match ps.repair_status {
                State::Idle => {}
                State::Running => {
                    e.readout = nj_platform::i18n::msg::widgets_repair_running();
                    e.detail = nj_platform::i18n::msg::widgets_repair_wait().into();
                }
                State::Repaired => {
                    e.readout = nj_platform::i18n::msg::widgets_repair_completed();
                    e.detail = nj_platform::i18n::msg::widgets_repair_reopen().into();
                }
                State::Failed(reason) => {
                    e.readout = nj_platform::i18n::msg::widgets_repair_failed();
                    e.detail = repair_failure_message(reason).into();
                }
            }
        }
        let actions = crate::player::failure_actions(e.kind, crate::player::failure_context(ps));
        Self::of(&e, actions, crate::player::support_line(e.kind))
    }

    /// PURE: the read-out for one failure shape.
    ///
    /// **Say it once.** The reason is ONE sentence: what happened and why. Under Force Direct
    /// Play that is the route's own verdict ("Force Direct Play is on, and this video format can't
    /// play without conversion"), which already names both — the old generic "Force Direct Play
    /// could not play this stream" above it said the same thing a second time. The shape's
    /// `detail` then goes to exactly one place, the note under the row (the server's own sentence,
    /// the sandbox's remedy, a repair's progress) — unless it is the Force verdict already said as
    /// the reason. The support facts are always the footer.
    pub(crate) fn of(
        e: &crate::player::ErrorShape,
        actions: Vec<crate::player::FailureAction>,
        support: String,
    ) -> Self {
        use crate::player::FailureKind;
        let policy = e.kind == FailureKind::PlaybackPolicy && !e.detail.is_empty();
        let reason: &str = if policy { &e.detail } else { e.readout };
        let note: Option<String> = if e.no_pass {
            Some(nj_platform::i18n::msg::widgets_failure_no_pass(PLEX_PASS))
        } else if policy || e.detail.is_empty() {
            None
        } else {
            Some(e.detail.to_string())
        };
        Self {
            glyph: failure_glyph(e.kind),
            reason: CString::new(reason).unwrap_or_default(),
            note: note.and_then(|n| CString::new(n).ok()),
            actions,
            footer: CString::new(support).unwrap_or_default(),
        }
    }

    /// The row, in draw order.
    pub(crate) fn actions(&self) -> &[crate::player::FailureAction] {
        &self.actions
    }

    #[cfg(test)]
    pub(crate) fn reason(&self) -> &str {
        self.reason.to_str().unwrap_or("")
    }

    #[cfg(test)]
    pub(crate) fn note(&self) -> Option<&str> {
        self.note.as_deref().and_then(|n| n.to_str().ok())
    }

    #[cfg(test)]
    pub(crate) fn footer(&self) -> &str {
        self.footer.to_str().unwrap_or("")
    }

    fn overlay(&self, labels: &[&'static std::ffi::CStr], focus: Option<usize>) -> StatusOverlay<'_> {
        StatusOverlay::new(Rect::FULL, nj_platform::i18n::msg::widgets_status_failed_c(), StatusKind::Failed)
            .page(self.glyph)
            .reason(&self.reason)
            .row(labels)
            .note(self.note.as_deref())
            .focus(focus)
    }

    fn labels(&self) -> Vec<&'static std::ffi::CStr> {
        self.actions.iter().map(|a| failure_action_label(*a)).collect()
    }

    /// Each control's rect, by row index — the geometry the draw uses.
    pub(crate) fn frames(&self, measure: &dyn nj_machine::machine::Measure) -> [Option<Rect>; crate::ui::widgets::STATUS_ROW_MAX] {
        let labels = self.labels();
        self.overlay(&labels, None).row_frames_measured(measure)
    }

    fn draw(&self, p: Painter, focus: usize, measure: &dyn nj_machine::machine::Measure) {
        let labels = self.labels();
        self.overlay(&labels, Some(focus.min(labels.len().saturating_sub(1))))
            .draw_measured(&hud_env(), p, measure);
        crate::ui::text_view::TextView::new(self.footer.to_str().unwrap_or(""), theme::size::CAPTION, theme::TEXT_TERTIARY)
            .h(crate::ui::label::HAlign::Center)
            .max_lines(1)
            // the full safe width, not the reason's: the failure code is the line's LAST field
            // and the one support needs, so it must not be the part an ellipsis eats
            .draw(p, Rect::new(crate::ui::consts::MARGIN_X, FOOTER_TOP, SCR_W - 2.0 * crate::ui::consts::MARGIN_X, 0.0));
    }
}

// ---- element addresses for the Engine's hit map / `Focusable` groups (restructure phase 12) --
//
// `PlayerScreen` (`screens/player/mod.rs`) registers one `ui::screen::Stop` per element of its
// `Focusable` groups (`record_stops`, in z-order, at `place`'s rect), keyed on these addresses.
// Kept as plain `u32` constants, not an enum, so `PlayerScreen` can address a control-row ITEM by
// `BASE + index`.
pub(crate) const ELEM_SCRUB: u32 = 0;
/// `+ 0..items()` for whatever occupies the row — the OSD buttons, the Skip button, or both Up
/// Next buttons, each at its own rect ([`ControlSlot::item_rect`]).
pub(crate) const ELEM_ROW_BASE: u32 = 10;
/// `+ 0..actions().len()` — the failure read-out's row, one element per
/// [`crate::player::FailureAction`] in [`FailureReadout::actions`] order: the only controls a
/// FAILED playback draws.
pub(crate) const ELEM_FAILURE_BASE: u32 = 30;
/// One past the failure row's last possible element.
pub(crate) const ELEM_FAILURE_END: u32 = ELEM_FAILURE_BASE + crate::ui::widgets::STATUS_ROW_MAX as u32;

/// The seek row's time boxes. `.osd-time`'s `min-width: 5em` resolves against the read-out's own
/// 19.2px, so it is 96 (the spec's 80, [`osd::TIME_MIN_W`], is that 5em at the 16px root); that holds
/// `H:MM:SS` in the bold face, so the boxes never grow and the track never moves.
const TIME_BOX_W: f32 = osd::TIME_MIN_W * 1.2;

/// The seek track: between the two time boxes, [`osd::SEEK_TIME_GAP`] from each, centred on the row.
pub(crate) fn seek_track() -> Rect {
    let x = OSD_X + TIME_BOX_W + osd::SEEK_TIME_GAP;
    Rect::new(x, SEEK_CY - osd::SEEK_TRACK_H * 0.5, OSD_RIGHT - TIME_BOX_W - osd::SEEK_TIME_GAP - x, osd::SEEK_TRACK_H)
}

/// The scrubber's GRAB band: the whole seek row, times included, a little taller than the row — a
/// pointer grab zone, placed by `PlayerScreen::place` and registered as the LOWEST stop so every
/// control over the band outranks it (issue #162). It stays clear of the controls row above.
pub(crate) fn scrub_hit_rect() -> Rect {
    const GRAB: f32 = 12.0;
    Rect::new(OSD_X, SEEK_Y - GRAB, OSD_RIGHT - OSD_X, osd::TIME_H + 2.0 * GRAB)
}

/// The diameter of control `idx`: the play circle, or an ordinary button.
fn btn_d(idx: i32) -> f32 {
    if idx == BTN_PLAY { osd::PLAY_D } else { osd::BUTTON_D }
}

/// The advance one left-group item takes, its gap to the next included: the play circle carries
/// [`osd::PLAY_MARGIN`] on each side.
fn left_advance(idx: i32) -> f32 {
    let margin = if idx == BTN_PLAY { 2.0 * osd::PLAY_MARGIN } else { 0.0 };
    btn_d(idx) + margin + osd::BUTTON_GAP
}

/// The left group's right edge, where "Ends at" starts [`ENDS_AT_GAP`] later.
fn left_group_right() -> f32 {
    OSD_X + LEFT_GROUP.iter().map(|&i| left_advance(i)).sum::<f32>() - osd::BUTTON_GAP
}

/// Control `idx`'s circle: the left group runs from [`OSD_X`], the right group is right-anchored on
/// [`OSD_RIGHT`] (10 Controls row, `osd-controls-left` / spacer / `osd-controls-right`). `None` for
/// the Chapters button on an item without chapters, which Litefin takes out of the row.
pub(crate) fn btn_rect(c: Controls, idx: i32) -> Option<Rect> {
    let circle = |x: f32| {
        let d = btn_d(idx);
        Rect::new(x, ROW_CY - d * 0.5, d, d)
    };
    if let Some(at) = LEFT_GROUP.iter().position(|&i| i == idx) {
        let before: f32 = LEFT_GROUP[..at].iter().map(|&i| left_advance(i)).sum();
        let margin = if idx == BTN_PLAY { osd::PLAY_MARGIN } else { 0.0 };
        return Some(circle(OSD_X + before + margin));
    }
    let drawn = |i: &&i32| **i != BTN_CHAPTERS || c.chapters;
    let at = RIGHT_GROUP.iter().filter(drawn).position(|&i| i == idx)?;
    let after = RIGHT_GROUP.iter().filter(drawn).count() - at - 1;
    Some(circle(OSD_RIGHT - osd::BUTTON_D - after as f32 * (osd::BUTTON_D + osd::BUTTON_GAP)))
}

/// Is the failure read-out's row ON SCREEN — a failure owns the frame. The row always holds at
/// least *Back*, so a failure always has something to press.
pub(crate) fn failure_row_drawn(busy: Busy) -> bool {
    readout_owns_frame(busy)
}

/// Product copy is resolved here; the worker's technical error identity stays unchanged.
fn repair_failure_message(reason: nj_platform::tv::sandbox::Failure) -> &'static str {
    use nj_platform::tv::sandbox::Failure;
    match reason {
        Failure::StartFailed => nj_platform::i18n::msg::widgets_repair_start_failed(),
        Failure::HbcUnavailable => nj_platform::i18n::msg::widgets_repair_hbc_unavailable(),
        Failure::NotRoot => nj_platform::i18n::msg::widgets_repair_not_root(),
        Failure::CommandFailed => nj_platform::i18n::msg::widgets_repair_command_failed(),
        Failure::Timeout => nj_platform::i18n::msg::widgets_repair_timeout(),
        Failure::Unreadable => nj_platform::i18n::msg::widgets_repair_unreadable(),
        Failure::Unsupported => nj_platform::i18n::msg::widgets_repair_unsupported(),
    }
}

fn draw_failed_readout(
    ps: &crate::route::PlaybackSession,
    p: Painter,
    focus: usize,
    measure: &dyn nj_machine::machine::Measure,
) {
    // The GROUND, first: a full-bleed opaque black, one quad. Without it the read-out stood on
    // whatever the video plane happened to be holding: `app.rs` clears the graphics plane to alpha
    // 0 on the player route, and the transport is not drawn at all under a failure. "The plane is
    // black by the time an error is up" is usually true and not always — `busy_surface` resolves
    // Error to Failed for BOTH values of `seen_frame`, i.e. a mid-playback failure over a held
    // frame is a state the code contemplates.
    p.rrect(Rect::FULL, 0.0, 0.0, theme::scrim_black(1.0));
    FailureReadout::now(ps).draw(p, focus, measure);
}

/// **Where along the track a pointer at `mx` is pointing**, `0..1`.
///
/// X only, on purpose: an engaged drag tracks the pointer even when it wanders off the band
/// vertically, which is why `PlayerScreen::handle_drag` never re-tests the hit once a click has
/// seated the gesture.
pub(crate) fn scrub_frac_x(mx: f32) -> f32 {
    let t = seek_track();
    ((mx - t.x) / t.w).clamp(0.0, 1.0)
}

/// The OSD's time read-out (10 Seek row): `MM:SS`, or `H:MM:SS` once the item `of_ns` runs an hour
/// or longer — both read-outs take the total's shape, so the pair never changes width mid-item.
pub(crate) fn osd_time(ns: i64, of_ns: i64) -> String {
    let s = (ns / 1_000_000_000).max(0);
    if of_ns >= 3_600_000_000_000 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// `HH:MM`, 24-hour (`player:timeFormat=24h` on both TVs, `docs/litefin-look.md` 0).
fn clock_24h(minutes: u32) -> String {
    format!("{:02}:{:02}", minutes / 60 % 24, minutes % 60)
}

/// **Seconds past local midnight on the television's clock** — the OSD clock and "Ends at".
/// A wall-clock READ-OUT: drawn, folded into the page's clock fingerprint so a new minute presents,
/// and never part of any logical state (`ci/allow/wall.txt`). `None` before 1970 or without a zone.
pub(crate) fn local_seconds() -> Option<u32> {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let t = libc::time_t::try_from(secs).ok()?;
    // SAFETY: `localtime_r` writes only the `tm` it is handed and reads the process's zone.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return None;
    }
    Some(tm.tm_hour as u32 * 3600 + tm.tm_min as u32 * 60 + tm.tm_sec as u32)
}

/// When the item ends at the current rate, as minutes past midnight: now plus what is left, summed
/// in seconds before the minute is taken (two floored minutes read up to a minute early). `None`
/// until the duration is known: with nothing left to add it would only repeat the clock.
pub(crate) fn ends_at_minutes(now_secs: u32, pos_ns: i64, dur_ns: i64) -> Option<u32> {
    (dur_ns > 0).then(|| {
        let left_s = ((dur_ns - pos_ns).max(0) / 1_000_000_000) as u64;
        ((u64::from(now_secs) + left_s) / 60 % (24 * 60)) as u32
    })
}

/// How long a transport lingers after the input that raised it — Litefin's 3.5 s
/// (`OSDController autoHideDelay`, 10) — the player HUD's (`screens::player::input::HUD_LINGER_MS`)
/// and the detail page's full-trailer transport alike, so a trailer's controls leave the screen on
/// the same beat a film's do.
pub(crate) const LINGER_MS: u32 = 3500;

// ---- scrub tuning, shared by the player HUD's scrubber and the trailer transport's own --------
//
// Both hold-to-scrub gestures want the same feel — a press jumps `scrub_step_ns()`; holding engages
// a continuous scrub ramping `SCRUB_BASE`→`SCRUB_MAX` (playback-seconds per real-second) — but the
// STATE (`screens::player::input::Scrub`, and `screens::detail::trailer::Transport`'s own fields)
// cannot live in one shared type: `ci/check-deps.sh`'s `sibling` gate forbids a file under
// `screens/detail/` from naming `crate::screens::player` at all, the same rule that keeps
// `LINGER_MS` above defined here rather than in `screens::player::input` for `trailer.rs` to
// import from a sibling. `screens::player::input::Scrub::clamp_target` is a thin call-through to
// [`scrub_clamp_target`] below so its own many call sites are untouched.
/// What one fresh press hops: the Skip interval preference (10 s unless Settings changed it),
/// read per press so a pick takes effect in a film already playing.
pub(crate) fn scrub_step_ns() -> i64 {
    crate::route::skip_interval().ns()
}
pub(crate) const SCRUB_BASE: f32 = 10.0;
pub(crate) const SCRUB_ACCEL: f32 = 45.0; // added per second of hold
pub(crate) const SCRUB_MAX: f32 = 140.0;
// tap released → commit after this (further taps accumulate); see `screens::player::input`'s own
// doc for why this debounce exists (coalescing a rapid tap burst into one Load/reload).
pub(crate) const TAP_COMMIT_MS: u32 = 450;
pub(crate) const SCRUB_LOST_MS: u32 = 400; // holding but no repeat this long → lost keyup → commit

/// Sets the Skip interval for one test and puts the prior value back on drop. The caller holds
/// `testlock::serial()`: the live value is a process global, like every preference.
#[cfg(test)]
pub(crate) struct SkipIntervalGuard(crate::route::SkipInterval);
#[cfg(test)]
impl SkipIntervalGuard {
    pub(crate) fn set(interval: crate::route::SkipInterval) -> Self {
        let prior = crate::route::skip_interval();
        crate::route::restore_skip_interval(interval);
        Self(prior)
    }
}
#[cfg(test)]
impl Drop for SkipIntervalGuard {
    fn drop(&mut self) {
        crate::route::restore_skip_interval(self.0);
    }
}

/// Where a scrub target may legally land: never before zero, and never inside the last three
/// seconds, which is a seek past the point the pipeline can prime from.
pub(crate) fn scrub_clamp_target(ns: i64, duration_ns: i64) -> i64 {
    let cap = duration_ns - 3_000_000_000;
    let ns = ns.max(0);
    if cap > 0 && ns > cap {
        cap
    } else {
        ns
    }
}

/// The bottom gradient: [`osd::GRADIENT_FROM`] at the panel's foot to transparent at
/// [`GRADIENT_TOP`], 160 above the controls row (`docs/litefin-look.md` 10, Bottom area). Drawn
/// under every transport, the player's and the trailer's.
pub(crate) fn draw_scrim(p: Painter) {
    p.rect(Rect::new(0.0, GRADIENT_TOP, SCR_W, SCRIM_H), 0.0, osd::GRADIENT_TO, osd::GRADIENT_FROM, 0.0);
}

/// The header's content band. Litefin's 38.4 top pad lifts to the safe frame's 54; the title box
/// is 60 tall (`.osd-title-main`), and the gradient runs 76.8 past it (10 Header).
const HEADER_TOP: f32 = crate::ui::consts::MARGIN_Y;
const HEADER_LINE_H: f32 = 60.0;
const HEADER_CY: f32 = HEADER_TOP + HEADER_LINE_H * 0.5;
const HEADER_H: f32 = HEADER_TOP + HEADER_LINE_H + osd::HEADER_PAD_BOTTOM;
/// Litefin caps the title at 60 % of the width (`.osd-title` max-width 60vw)…
const TITLE_MAX_W: f32 = SCR_W * 0.6;
/// …and spaces its secondary run .6rem after it (`.osd-title-wrap` gap).
const TITLE_GAP: f32 = 9.6;

/// The header's two runs, Litefin's `_getFormattedTitle`: the name in bold, then a regular
/// secondary run — `(Year)` for a film, `- S01E03 - Name (Year)` after a show's name. A film's name
/// is the route's own title (set with the play, before the metadata lands), and an item without a
/// year keeps the route's context line (an extra's kind) in the secondary place.
fn header_title(ps: &crate::route::PlaybackSession, meta: crate::metadata::MetadataView<'_>) -> (String, String) {
    let text = |p: *const std::os::raw::c_char| if p.is_null() { String::new() }
        else { unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned() };
    header_runs(meta.now_playing(), &text(crate::route::title_cptr(ps)), &text(crate::route::ctxline_cptr(ps)))
}

/// PURE: [`header_title`]'s two runs.
fn header_runs(n: Option<&crate::metadata::NowPlaying>, title: &str, context: &str) -> (String, String) {
    let year = match n {
        Some(n) if n.year > 0 => format!("({})", n.year),
        _ => String::new(),
    };
    let Some(n) = n.filter(|n| n.is_real_episode) else {
        return (title.into(), if year.is_empty() { context.into() } else { year });
    };
    let name = if n.ep_title.is_empty() { String::new() } else { format!("- {}", n.ep_title) };
    let ep = [format!("S{:02}E{:02}", n.season, n.index), name, year]
        .into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ");
    let secondary = if n.title.is_empty() { ep } else { format!("- {ep}") };
    (n.title.clone(), secondary)
}

/// The header: gradient, title runs on the left, the 24-hour clock on the right. No back button
/// (`osdHideBackButton` on TV) and the text title, not the logo (`osdShowLogo` off).
fn draw_header(p: Painter, main: &str, secondary: &str, clock: Option<&str>, measure: &dyn nj_machine::machine::Measure) {
    p.rect(Rect::new(0.0, 0.0, SCR_W, HEADER_H), 0.0, osd::GRADIENT_FROM, osd::GRADIENT_TO, 0.0);
    let right = clock.map_or(OSD_RIGHT, |c| OSD_RIGHT - measure.width_str(c, CLOCK_SZ, false) - theme::space::MD);
    let mut x = OSD_X;
    if !main.is_empty() {
        let fitted = measure.fit_line(main, (right - x).min(TITLE_MAX_W).max(0.0), TITLE_SZ, true);
        x += p.text(fitted.as_ptr(), x, nj_gfx::text::text_vcenter_y(TITLE_SZ, 1, HEADER_CY), TITLE_SZ, theme::TEXT_PRIMARY, 0, 1) + TITLE_GAP;
    }
    if !secondary.is_empty() && right > x {
        let fitted = measure.fit_line(secondary, right - x, TITLE_SZ, false);
        p.text(fitted.as_ptr(), x, nj_gfx::text::text_vcenter_y(TITLE_SZ, 0, HEADER_CY), TITLE_SZ, osd::TEXT_SOFT, 0, 0);
    }
    if let Some(c) = clock.and_then(|c| CString::new(c).ok()) {
        p.text(c.as_ptr(), OSD_RIGHT, nj_gfx::text::text_vcenter_y(CLOCK_SZ, 0, HEADER_CY), CLOCK_SZ, osd::TEXT_SOFT, 2, 0);
    }
}

/// One line of OSD text with its cap band centred on `cy`; `align` as `Painter::text`'s. The
/// anchored read-outs (the clock on the right edge, the times at the seek row's ends, "Ends at"
/// after the buttons) keep Litefin's proportional digits: their anchor holds still however the
/// digits change. The centred seek tooltip is the one that needs figure cells.
fn osd_text(p: Painter, text: &str, x: f32, cy: f32, sz: c_int, ink: [f32; 4], align: c_int, bold: bool) {
    if let Ok(cs) = CString::new(text) {
        let b = c_int::from(bold);
        p.text(cs.as_ptr(), x, nj_gfx::text::text_vcenter_y(sz, b, cy), sz, ink, align, b);
    }
}

/// The line over the trailer transport's display title. Non-owning `*const c_char`, the `Label`
/// rule (`ui/CLAUDE.md`): keep the `CString` (or the session's fixed buffer) alive across the draw.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Kicker {
    /// an episode's own address and name ("S1, E1 · Pilot"): primary ink, bold
    Episode(*const std::os::raw::c_char),
    /// a context label (an extra's kind): secondary ink, regular — it only says what the display
    /// title is
    Context(*const std::os::raw::c_char),
}

/// The trailer transport's title block, standing on its seek row (it has no controls row).
pub(crate) fn draw_title(p: Painter, kicker: Kicker, title: *const std::os::raw::c_char) {
    let (text, ink, bold) = match kicker {
        Kicker::Episode(text) => (text, theme::TEXT_PRIMARY, 1),
        Kicker::Context(text) => (text, theme::TEXT_SECONDARY, 0),
    };
    p.text(text, OSD_X, SEEK_Y - 114.0, theme::size::CAPTION, ink, 0, bold);
    p.text(title, OSD_X, SEEK_Y - 80.0, HUD_TITLE_SZ, theme::TEXT_PRIMARY, 0, 1);
}

/// How the playhead is drawn — see [`draw_playbar`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Knob {
    /// the scrubber holds the focus ring: the 10x28 bar
    Focused,
    /// a scrub preview is being dragged with the ring elsewhere: the bar too
    Held,
    /// neither: the 8x8 dot
    Tick,
}

/// Everything [`draw_playbar`] draws from, resolved by its caller. A value rather than a read of
/// the transport's globals inside the draw, because TWO surfaces draw this bar and they resolve it
/// differently: the player route from its own scrub gesture, seek target and focus ring
/// ([`Playbar::live`]), and the detail page's full-trailer transport from a preview session that
/// has none of the three.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Playbar {
    /// the DISPLAYED playhead (a seek target, a scrub preview or the published position)
    pub(crate) pos_ns: i64,
    pub(crate) dur_ns: i64,
    pub(crate) knob: Knob,
    /// [`seek_waiting`]: a spinner stands in the thumb's place
    pub(crate) waiting: bool,
    /// drives the spinner's rotation
    pub(crate) now: u32,
}

impl Playbar {
    /// The player route's bar: the live scrub preview and seek target over the published
    /// playhead, the knob from the HUD's own focus ring, and the state read-out.
    fn live(
        ps: &crate::route::PlaybackSession,
        busy: Busy,
        focus: i32,
        now: u32,
    ) -> Self {
        let scrub = crate::player::TX.scrub_ns.load(Relaxed);
        // while a seek is loading, freeze the playhead at the target (no wobble through the reopen);
        // else follow the live scrub preview, else the real playhead. ONE sample of each live
        // atomic: the condition and the value could otherwise straddle a tick.
        let loading = crate::player::loading(ps);
        let livepos = crate::player::playpos_ns();
        let seekdisp = crate::player::seek_display_ns();
        let dispos = display_pos_ns(loading, seekdisp, scrub, livepos);
        let knob = if focus == 0 {
            Knob::Focused
        } else if scrub >= 0 {
            Knob::Held
        } else {
            Knob::Tick
        };
        Self {
            pos_ns: dispos,
            dur_ns: crate::player::duration_ns(),
            knob,
            waiting: seek_waiting(busy, dispos, livepos),
            now,
        }
    }

    fn frac(self) -> f32 {
        if self.dur_ns > 0 { (self.pos_ns as f64 / self.dur_ns as f64).clamp(0.0, 1.0) as f32 } else { 0.0 }
    }
}

/// The seek row (10): current time left, total right, the track between them with its accent fill,
/// the thumb, and — the player's — chapter ticks. The player HUD and the detail page's full-trailer
/// transport both draw it, so the two cannot drift.
pub(crate) fn draw_playbar(p: Painter, bar: Playbar, _measure: &dyn nj_machine::machine::Measure) {
    draw_seek_row(p, bar, &[]);
}

/// [`draw_playbar`] with chapter ticks at `ticks` (fractions of the item).
fn draw_seek_row(p: Painter, bar: Playbar, ticks: &[f32]) {
    let t = seek_track();
    let rad = t.h * 0.5;
    p.rrect(t, rad, rad, osd::SEEK_TRACK);
    let fw = t.w * bar.frac();
    if fw > 0.0 {
        // the fill's right end is square; the track's own rounding masks it near either end
        p.rrect(Rect::new(t.x, t.y, fw, t.h), rad.min(fw * 0.5), (rad - (t.w - fw)).max(0.0), osd::SEEK_FILL);
    }
    for f in ticks {
        p.rrect(Rect::new(t.x + t.w * f - osd::TICK_W * 0.5, SEEK_CY - osd::TICK_H * 0.5, osd::TICK_W, osd::TICK_H),
            0.0, 0.0, osd::SEEK_TRACK);
    }
    let hx = t.x + fw;
    if bar.waiting {
        Spinner::new(hx, SEEK_CY, Spinner::R_INLINE).phase(bar.now).tint(osd::THUMB_INK).draw(&hud_env(), p);
    } else {
        let (w, h) = if bar.knob == Knob::Tick { (osd::THUMB, osd::THUMB) } else { (osd::THUMB_FOCUS_W, osd::THUMB_FOCUS_H) };
        p.rrect(Rect::new(hx - w * 0.5, SEEK_CY - h * 0.5, w, h), w * 0.5, w * 0.5, osd::THUMB_INK);
    }
    osd_text(p, &osd_time(bar.pos_ns, bar.dur_ns), OSD_X + TIME_PAD_X, SEEK_CY, TIME_SZ, theme::TEXT_PRIMARY, 0, true);
    osd_text(p, &osd_time(bar.dur_ns, bar.dur_ns), OSD_RIGHT - TIME_PAD_X, SEEK_CY, TIME_SZ, theme::TEXT_PRIMARY, 2, true);
}

/// The seek tooltip (10): the scrub target's time — and the trickplay still above it when the
/// server has one — in a black box with a 2px white .20 border, radius 16, 14 above the seek row,
/// centred on the thumb and kept on the panel.
fn draw_seek_tooltip(p: Painter, bar: Playbar, still: Option<(std::os::raw::c_uint, f32, f32)>, measure: &dyn nj_machine::machine::Measure) {
    use crate::ui::widgets::{draw_figures, figures_width};
    const THUMB_W: f32 = 320.0;
    const THUMB_H: f32 = 180.0;
    let text = osd_time(bar.pos_ns, bar.dur_ns);
    let text_w = figures_width(&text, TOOLTIP_SZ, true, measure);
    let text_h = measure.line_h(TOOLTIP_SZ);
    // the still at its tile's aspect inside 320 x 180; the box hugs it with 8 / 8 / 12 padding,
    // else the text alone with 9.6 x 19.2 (`.osd-seek-tooltip`, `.has-trickplay`)
    let thumb = still.map(|(tex, w, h)| {
        let k = (THUMB_W / w.max(1.0)).min(THUMB_H / h.max(1.0));
        (tex, w * k, h * k)
    });
    let (inner_w, pad_x, pad_top, pad_bottom, gap) = match thumb {
        Some((_, w, _)) => (w.max(text_w), 8.0, 8.0, 12.0, 8.0),
        None => (text_w, 19.2, 9.6, 9.6, 0.0),
    };
    let h = pad_top + thumb.map_or(0.0, |(_, _, h)| h + gap) + text_h + pad_bottom;
    let w = inner_w + 2.0 * pad_x;
    let cx = (seek_track().x + seek_track().w * bar.frac()).clamp(OSD_X + w * 0.5, OSD_RIGHT - w * 0.5);
    let r = Rect::new(cx - w * 0.5, SEEK_Y - osd::TOOLTIP_GAP - h, w, h);
    p.rrect(r, theme::radius::LG, theme::radius::LG, osd::TOOLTIP_FILL);
    p.rring(r, theme::radius::LG, osd::TOOLTIP_BORDER_W, osd::TOOLTIP_BORDER);
    if let Some((tex, tw, th)) = thumb {
        p.tex(tex, Rect::new(cx - tw * 0.5, r.y + pad_top, tw, th), osd::TOOLTIP_THUMB_RAD, theme::TINT_WHITE);
    }
    draw_figures(p, &text, cx, r.y + r.h - pad_bottom - text_h * 0.5, TOOLTIP_SZ, theme::TEXT_PRIMARY, 1, true, measure);
}

/// The OSD buttons (10 Controls row): the transport group from the left edge, "Ends at" after it,
/// and the track / chapters / settings group right-anchored. Icon-only circles; the focused one
/// fills with the accent and pops 1.08, its icon a further 1.12.
fn draw_controls(
    p: Painter,
    row: &TransportRow,
    c: Controls,
    focused: Option<i32>,
    paused: bool,
    subtitles_on: bool,
    ends_at: Option<&str>,
) {
    use crate::ui::icons::Icon;
    let e = hud_env();
    for i in 0..BTN_N {
        let Some(rect) = btn_rect(c, i) else { continue };
        let t = row.pop(i, osd::FOCUS_SCALE);
        let mut b = TransportButton::new(i, rect)
            .focused(c.enabled(i) && focused == Some(i))
            .enabled(c.enabled(i))
            .active(i == BTN_SUBTITLES && subtitles_on)
            .ground(ControlGround::Unkeyed)
            .scale(t)
            .icon_scale(1.0 + (t - 1.0) / (osd::FOCUS_SCALE - 1.0) * (osd::ICON_FOCUS_SCALE - 1.0));
        if i == BTN_PLAY {
            b = b.icon_px(osd::PLAY_ICON);
            if !paused {
                b = b.glyph(Icon::PauseLine, Icon::PauseLine);
            }
        }
        b.draw(&e, p);
    }
    if let Some(text) = ends_at {
        osd_text(p, text, left_group_right() + ENDS_AT_GAP, ROW_CY, TIME_SZ, osd::TEXT_SOFT, 0, true);
    }
}

/// The transport HUD, composed from retui widgets through a root `Painter`.
/// `focus`: 0 = seek bar, 1 = the controls row. `btn` is the focused item within the row (only
/// meaningful when `focus` selects it). `now` drives the spinners. `transport`: draw the controls
/// and the seek row; false while the Chapters strip has the middle, which leaves the header and
/// the gradients. `busy`: the caller's single resolve of who owns the working signal — see
/// [`busy_surface`]; the seek row draws its spinner only when it is [`Busy::Transport`], and the
/// centred read-out is [`draw_readout`]'s, drawn by the caller AFTER this.
pub(crate) fn draw_hud(
    ps: &crate::route::PlaybackSession,
    alpha: f32,
    row: &TransportRow,
    preview: &mut crate::appkit::scrub_preview::ScrubPreview,
    up: &crate::appkit::up_next::Countdown,
    slot: ControlSlot,
    busy: Busy,
    focus: i32,
    btn: i32,
    now: u32,
    transport: bool,
    measure: &dyn nj_machine::machine::Measure,
    meta: crate::metadata::MetadataView<'_>,
) {
    // A FAILURE owns the frame, and it outranks every branch below — including the Up Next card,
    // which cannot coexist with one but must not be the arm that decides so: with no picture and
    // no position a live-looking seek bar at 0:00 is the historical bug the read-out exists to end.
    // Gated on the resolved `busy`, the ONE place the surfaces are divided ([`busy_surface`]), and
    // deliberately not on `transport`.
    if readout_owns_frame(busy) {
        return;
    }
    let p = Painter::root().alpha(alpha);
    draw_scrim(p);
    let secs = local_seconds();
    let (main, secondary) = header_title(ps, meta);
    draw_header(p, &main, &secondary, secs.map(|s| clock_24h(s / 60)).as_deref(), measure);
    if !transport {
        preview.release();
        return;
    }
    let bar = Playbar::live(ps, busy, focus, now);
    // The control row, from the slot the CALLER resolved — so what is drawn and what a keypress
    // activates are the same value, not two derivations of it.
    match slot {
        ControlSlot::UpNext(_) => crate::appkit::up_next::draw(ps, row, up, p, focus == 1, btn, now, measure),
        ControlSlot::Skip(pr) => crate::appkit::skip_pill::draw(row, p, pr, focus == 1, measure),
        ControlSlot::Discs(c) => {
            let ends_at = secs.and_then(|s| ends_at_minutes(s, bar.pos_ns, bar.dur_ns))
                .map(|m| nj_platform::i18n::msg::player_osd_ends_at(&clock_24h(m)));
            draw_controls(p, row, c, (focus == 1).then_some(btn), crate::player::TX.paused.load(Relaxed),
                crate::route::cur_sub_sid(ps) != 0, ends_at.as_deref());
        }
    }
    let ticks: Vec<f32> = if bar.dur_ns > 0 {
        crate::appkit::chapters_panel::chapter_starts(meta).into_iter()
            .map(|ms| (ms as f64 * 1e6 / bar.dur_ns as f64) as f32)
            .filter(|f| *f > 0.0 && *f < 1.0)
            .collect()
    } else {
        Vec::new()
    };
    draw_seek_row(p, bar, &ticks);
    let scrub = crate::player::TX.scrub_ns.load(Relaxed);
    if scrub >= 0 {
        let still = preview.still(ps, scrub, now);
        draw_seek_tooltip(p, bar, still, measure);
    } else {
        preview.release();
    }
}

/// **The transport's outermost drawn chrome, for the overscan audit**
/// ([`crate::ui::consts::SAFE`]). Private geometry, so the rects are built where they are drawn
/// rather than restated in the module that grades them. Measured labels (the title, the clock,
/// "Ends at") are entered degenerate, with the edge that matters.
#[cfg(test)]
pub(crate) fn overscan_rects(out: &mut Vec<(&'static str, Rect)>) {
    out.push(("player seek row", Rect::new(OSD_X, SEEK_Y, OSD_RIGHT - OSD_X, osd::TIME_H)));
    let all = Controls { previous: true, next: true, restart: true, chapters: true };
    let row = (0..BTN_N).filter_map(|i| btn_rect(all, i)).reduce(|a, b| a.union(b)).expect("a row");
    out.push(("player controls row", row));
    out.push(("player title", Rect::new(OSD_X, HEADER_CY, 0.0, 0.0)));
    out.push(("player clock", Rect::new(OSD_RIGHT, HEADER_CY, 0.0, 0.0)));
    out.push(("player ends at", Rect::new(left_group_right() + ENDS_AT_GAP, ROW_CY, 0.0, 0.0)));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Litefin's read-outs: `MM:SS`, `H:MM:SS` once the item runs an hour, both times in the total's
    /// shape, nothing below zero; the clock and "Ends at" in 24 hours, wrapping past midnight.
    #[test]
    fn the_times_take_the_totals_shape_and_the_clock_is_24_hour() {
        let s = 1_000_000_000i64;
        assert_eq!(osd_time(446 * s, 888 * s), "07:26");
        assert_eq!(osd_time(888 * s, 888 * s), "14:48");
        assert_eq!(osd_time(1_955 * s, 9_010 * s), "0:32:35");
        assert_eq!(osd_time(9_010 * s, 9_010 * s), "2:30:10");
        assert_eq!(osd_time(-5 * s, 60 * s), "00:00");
        assert_eq!(clock_24h(0), "00:00");
        assert_eq!(clock_24h(21 * 60 + 5), "21:05");
        assert_eq!(ends_at_minutes((23 * 60 + 50) * 60, 0, 30 * 60 * s), Some(20), "past midnight");
        assert_eq!(ends_at_minutes(600 * 60, 50 * 60 * s, 30 * 60 * s), Some(600), "a finished item ends now");
        let at = |h: u32, m: u32, sec: u32| (h * 60 + m) * 60 + sec;
        assert_eq!(ends_at_minutes(at(20, 59, 50), 0, 620 * s), Some(21 * 60 + 10),
            "20:59:50 with 10:20 left ends 21:10:10, not 21:09");
        assert_eq!(ends_at_minutes(at(20, 59, 50), 0, 0), None, "no duration yet: no Ends at");
    }

    /// The controls row (10): the left group from the OSD's edge, 11.52 apart with the play circle
    /// carrying 11.52 more each side, every circle centred on one line; the right group ends on the
    /// right edge; Chapters leaves the row without chapters and the others close up.
    #[test]
    fn the_controls_row_is_litefins_two_groups() {
        let all = Controls { previous: true, next: true, restart: false, chapters: true };
        let r = |i| btn_rect(all, i).expect("drawn");
        assert_eq!(r(BTN_PREVIOUS).x, OSD_X);
        for pair in LEFT_GROUP.windows(2) {
            let gap = r(pair[1]).x - (r(pair[0]).x + r(pair[0]).w);
            let want = osd::BUTTON_GAP + if pair.contains(&BTN_PLAY) { osd::PLAY_MARGIN } else { 0.0 };
            assert!((gap - want).abs() < 1e-3, "gap {pair:?}: {gap}");
        }
        assert_eq!(r(BTN_PLAY).w, osd::PLAY_D);
        assert_eq!(r(BTN_REWIND).w, osd::BUTTON_D);
        for i in 0..BTN_N {
            assert!((r(i).cy() - ROW_CY).abs() < 1e-3, "button {i} off the row's line");
        }
        assert_eq!(r(BTN_MORE).x + r(BTN_MORE).w, OSD_RIGHT, "Settings ends the row");
        for pair in RIGHT_GROUP.windows(2) {
            assert!((r(pair[1]).x - (r(pair[0]).x + r(pair[0]).w) - osd::BUTTON_GAP).abs() < 1e-3);
        }
        assert!(left_group_right() + ENDS_AT_GAP < r(BTN_SUBTITLES).x - 200.0, "room for Ends at");
        let bare = Controls { chapters: false, ..all };
        assert_eq!(btn_rect(bare, BTN_CHAPTERS), None);
        assert_eq!(btn_rect(bare, BTN_AUDIO).map(|r| r.x + r.w + osd::BUTTON_GAP), btn_rect(bare, BTN_MORE).map(|r| r.x));
        // the row sits above the seek row, inside the bottom gradient
        assert!(r(BTN_PLAY).y + r(BTN_PLAY).h + ROW_SEEK_GAP <= SEEK_Y + 1e-3);
        assert!(GRADIENT_TOP < ROW_Y);
    }

    /// LEFT/RIGHT walk the row in its drawn order across both groups, skip what cannot act (the
    /// chapter controls without chapters, previous/next with nowhere to go) and stop at the ends;
    /// the ring parks on play/pause.
    #[test]
    fn the_row_walks_its_drawn_order_and_skips_what_cannot_act() {
        let movie = ControlSlot::Discs(Controls::NONE);
        assert_eq!(movie.primary_btn(), BTN_PLAY);
        assert_eq!(movie.neighbour(BTN_PLAY, true), BTN_FORWARD);
        assert_eq!(movie.neighbour(BTN_FORWARD, true), BTN_SUBTITLES, "past the dimmed chapter and next");
        assert_eq!(movie.neighbour(BTN_AUDIO, true), BTN_MORE, "Chapters is not drawn");
        assert_eq!(movie.neighbour(BTN_MORE, true), BTN_MORE, "no wrap");
        assert_eq!(movie.neighbour(BTN_REWIND, false), BTN_REWIND, "nothing usable before rewind");
        assert!(!movie.enabled(BTN_PREV_CHAPTER) && !movie.enabled(BTN_CHAPTERS));
        let ep = ControlSlot::Discs(Controls::for_item(true, true, true, false, true));
        assert_eq!(ep.neighbour(BTN_REWIND, false), BTN_PREV_CHAPTER);
        assert_eq!(ep.neighbour(BTN_PREV_CHAPTER, false), BTN_PREVIOUS);
        assert_eq!(ep.neighbour(BTN_AUDIO, true), BTN_CHAPTERS);
        assert_eq!(ep.items(), BTN_N);
        let up = ControlSlot::UpNext(Marker { kind: MarkerKind::Credits, start_ms: 0, end_ms: 1, final_seg: true });
        assert_eq!(up.neighbour(crate::appkit::up_next::BTN_NEXT, true), crate::appkit::up_next::BTN_CREDITS,
            "Next Episode is drawn first");
    }

    /// The seek row (10): the two time boxes at the OSD's edges, the track between them 20 from
    /// each, its grab band clear of the controls row above; a pointer reads the TRACK.
    #[test]
    fn the_seek_track_runs_between_the_times() {
        let t = seek_track();
        assert_eq!(t.x, OSD_X + TIME_BOX_W + osd::SEEK_TIME_GAP);
        assert_eq!(t.x + t.w, OSD_RIGHT - TIME_BOX_W - osd::SEEK_TIME_GAP);
        assert_eq!(t.h, osd::SEEK_TRACK_H);
        assert!((t.cy() - SEEK_CY).abs() < 1e-3);
        assert_eq!(scrub_frac_x(t.x - 50.0), 0.0);
        assert_eq!(scrub_frac_x(t.x + t.w * 0.25), 0.25);
        assert_eq!(scrub_frac_x(t.x + t.w + 50.0), 1.0);
        let band = scrub_hit_rect();
        let all = Controls { previous: true, next: true, restart: true, chapters: true };
        assert!(band.y > btn_rect(all, BTN_PLAY).map_or(0.0, |r| r.y + r.h), "the grab band clears the row");
        assert!(band.y + band.h <= SCR_H);
    }

    /// Figure cells (the tabular-digit stand-in): a read-out's width is its SHAPE's, whatever the
    /// digits, with the shipped proportional Poppins.
    #[test]
    fn figures_hold_one_width_while_the_digits_tick() {
        use nj_machine::machine::Measure;
        let m = &nj_base::fontcov::advances::ShippedMeasure;
        let w = |t: &str| crate::ui::widgets::figures_width(t, TIME_SZ, true, m);
        assert_eq!(w("11:11"), w("00:00"));
        assert_eq!(w("0:32:35"), w("1:59:08"));
        assert!(w("00:00") > 0.0);
        let plain = |t: &str| m.width_str(t, TIME_SZ, true);
        assert_ne!(plain("11:11"), plain("00:00"), "the shipped digits are proportional");
        assert!(w("0:00:00") + 2.0 * TIME_PAD_X <= TIME_BOX_W, "H:MM:SS fits its box");
    }

    /// **The seek row's elapsed time is resident before the draw, this second's and the next.**
    /// `ClockWarm::queue` is what `PlayerScreen::prepare` calls; the drain is the presenting side's
    /// (`app::run::prepare_window`), ahead of the clear.
    #[test]
    fn the_elapsed_time_is_rasterised_before_the_draw_and_one_second_ahead() {
        let _g = nj_base::testlock::serial();
        nj_gfx::text::reset_prewarm_for_test();
        let mut warm = ClockWarm::default();
        let (pos, dur) = (83_400_000_000, 120_000_000_000); // 1:23.4 into a 2:00 item
        warm.queue(pos, dur);
        assert!(nj_gfx::text::prewarm_pending(), "prepare queued the elapsed time");
        assert!(!nj_gfx::text::prewarm_resident_for_test(b"01:23", TIME_SZ, 1), "prepare itself uploaded");
        while nj_gfx::text::prewarm_pending() {
            crate::ui::panel_motion::PanelMotion::drain_queued_text_for_test();
        }
        for text in ["01:23", "01:24"] {
            assert!(nj_gfx::text::prewarm_resident_for_test(text.as_bytes(), TIME_SZ, 1),
                "{text:?} was left for the draw to rasterise after the back-buffer wait");
        }
        // The draw empties the queue every frame, so the same second is offered again.
        nj_gfx::text::reset_prewarm_for_test();
        warm.queue(pos + 16_000_000, dur);
        assert!(nj_gfx::text::prewarm_pending(), "the same second was not offered again");
        // An hour-long item takes the total's H:MM:SS shape, as the draw does.
        nj_gfx::text::reset_prewarm_for_test();
        warm.queue(pos, 3_600_000_000_000);
        crate::ui::panel_motion::PanelMotion::drain_queued_text_for_test();
        assert!(nj_gfx::text::prewarm_resident_for_test(b"0:01:23", TIME_SZ, 1));
        nj_gfx::text::reset_prewarm_for_test();
    }

    /// **The OSD's fade animates, so it owes two answers**: it reports while it fades in (and out)
    /// and is silent once settled; it settles within Litefin's 300 ms.
    #[test]
    fn the_osd_fade_reports_while_moving_and_is_silent_at_rest() {
        let _g = nj_base::testlock::serial();
        let mut row = TransportRow::new();
        let (_, moving) = nj_machine::idle::scoped_motion(|| row.fade_step(true, 1.0 / 60.0));
        assert!(moving, "the fade-in must report motion");
        for _ in 0..18 {
            row.fade_step(true, 1.0 / 60.0);
        }
        assert!(row.fade() > 0.95, "within 5 % after about 300 ms: {}", row.fade());
        for _ in 0..240 {
            row.fade_step(true, 1.0 / 60.0);
        }
        let (_, moving) = nj_machine::idle::scoped_motion(|| row.fade_step(true, 1.0 / 60.0));
        assert!(!moving, "a shown OSD at rest must not ask for frames");
        let (_, moving) = nj_machine::idle::scoped_motion(|| row.fade_step(false, 1.0 / 60.0));
        assert!(moving, "the fade-out reports too");
        let mut ticks = 1;
        while row.fade() > 0.0 && ticks < 120 {
            row.fade_step(false, 1.0 / 60.0);
            ticks += 1;
        }
        assert_eq!(row.fade(), 0.0, "the fade-out lands on 0 within 2 s, so the hidden OSD stops drawing");
        let (_, moving) = nj_machine::idle::scoped_motion(|| row.fade_step(false, 1.0 / 60.0));
        assert!(!moving, "a landed fade-out is silent");
        let mut idle = TransportRow::new();
        let (_, moving) = nj_machine::idle::scoped_motion(|| idle.fade_step(false, 1.0 / 60.0));
        assert!(!moving, "a hidden OSD at rest is silent");
    }

    /// **The focus pop animates, so it owes two answers** (`ui/CLAUDE.md`): it reports while the
    /// arriving button grows to Litefin's 1.08 (icon 1.12 on top), and it goes quiet once settled.
    #[test]
    fn the_buttons_focus_pop_reports_while_moving_and_is_silent_at_rest() {
        let _g = nj_base::testlock::serial();
        let slot = ControlSlot::Discs(Controls::NONE);
        let mut row = TransportRow::new();
        let (_, moving) = nj_machine::idle::scoped_motion(|| row.step(slot, 1, BTN_PLAY, 1.0 / 60.0));
        assert!(moving, "the arriving pop must report motion");
        for _ in 0..240 {
            row.step(slot, 1, BTN_PLAY, 1.0 / 60.0);
        }
        assert!((row.pop(BTN_PLAY, osd::FOCUS_SCALE) - osd::FOCUS_SCALE).abs() < 1e-3, "settles at 1.08");
        assert!((row.pop(BTN_REWIND, osd::FOCUS_SCALE) - 1.0).abs() < 1e-3, "the others rest at 1.0");
        let (_, moving) = nj_machine::idle::scoped_motion(|| row.step(slot, 1, BTN_PLAY, 1.0 / 60.0));
        assert!(!moving, "a settled row must not ask for frames");
        let (_, moving) = nj_machine::idle::scoped_motion(|| row.step(slot, 0, BTN_PLAY, 1.0 / 60.0));
        assert!(moving, "leaving the row shrinks the button back");
    }

    use crate::metadata::{Marker, MarkerKind};
    use crate::route::NextEpisodeMode;
    use crate::appkit::skip_pill::SkipAction;

    /// **The image-subtitle display set is a render, and it says how much of one** (§8.3 rule (c)):
    /// one texture per rect, the SOURCE pixels behind them, and nothing left claimed once the set
    /// is released. The bytes cannot be derived from the screen rects the cache keeps — those are
    /// where each bitmap lands, scaled into the video rect — which is why the count is carried.
    #[test]
    fn the_image_subtitle_set_reports_its_own_textures_and_bytes() {
        let mut subs = SubtitleBitmaps::stub(&[(720, 120), (300, 80)]);
        let r = subs.render_report();
        assert_eq!(r.textures, 2, "a two-rect display set is two textures");
        assert_eq!(r.bytes, (720 * 120 + 300 * 80) * 4);
        subs.release();
        assert_eq!(
            subs.render_report(),
            crate::ui::frame::RenderReport::NONE,
            "a released set holds nothing and must stop claiming bytes"
        );
    }

    fn marker(kind: MarkerKind, final_seg: bool) -> Marker {
        Marker {
            kind,
            start_ms: 1_000,
            end_ms: 2_000,
            final_seg,
        }
    }

    fn seg(start_ms: i64, end_ms: i64, final_seg: bool) -> Marker {
        Marker {
            kind: MarkerKind::Credits,
            start_ms,
            end_ms,
            final_seg,
        }
    }

    /// The control row's precedence, which used to live in the ORDER of five separate if-chains
    /// across `player_hud` and `app.rs` — the draw, the OK handler, the pointer handler, the
    /// LEFT/RIGHT clamp and `icon_hit` — with nothing keeping them in step and nothing to test.
    #[test]
    fn the_control_row_picks_the_most_specific_occupant() {
        // no segment under the playhead → the ordinary Subtitles + Audio pair
        assert!(slot_for(None, false, NextEpisodeMode::Countdown).is_discs());
        assert!(
            slot_for(None, true, NextEpisodeMode::Countdown).is_discs(),
            "a queued successor alone changes nothing"
        );
        assert_eq!(slot_for(None, true, NextEpisodeMode::Countdown).items(), BTN_N);
        assert_eq!(
            slot_for(None, true, NextEpisodeMode::Countdown).primary_btn(),
            BTN_PLAY,
            "the OSD opens on play/pause"
        );

        // an intro is always Skip, successor or not — "what's next" is an end-of-episode idea
        for has_next in [false, true] {
            let slot = slot_for(Some(marker(MarkerKind::Intro, false)), has_next, NextEpisodeMode::Countdown);
            assert!(matches!(slot, ControlSlot::Skip(p) if p.kind == MarkerKind::Intro));
            assert_eq!(slot.items(), 1, "a Skip pill is the row's only item");
            assert_eq!(
                slot.primary_btn(),
                0,
                "…so it is also the one focus lands on"
            );
        }

        // credits WITH somewhere to go → Up Next outranks Skip Credits…
        let up = slot_for(Some(marker(MarkerKind::Credits, true)), true, NextEpisodeMode::Countdown);
        assert!(matches!(up, ControlSlot::UpNext(_)));
        // …and it is the ONE stand-in with a pair, whose primary is not item 0. Asserted
        // together because they are one fact: an `items()` of 2 with a `primary_btn()` of 0 would
        // park the ring on Watch Credits, and `up_next::countdown_may_run` would then disarm the
        // countdown on the frame after it armed — a timer that visibly never runs.
        assert_eq!(up.items(), 2, "Watch Credits + Next Episode");
        assert_eq!(up.primary_btn(), crate::appkit::up_next::BTN_NEXT);
        // …and WITHOUT (a show's last episode) it stays Skip Credits, which is the whole reason
        // both still exist
        assert!(matches!(
            slot_for(Some(marker(MarkerKind::Credits, true)), false, NextEpisodeMode::Countdown),
            ControlSlot::Skip(p) if p.kind == MarkerKind::Credits
        ));
    }

    /// Up Next is only offered for a credits marker that runs to the end of the item (`final`).
    /// A mid-item credits segment with a successor queued (a post-credits scene follows) is a
    /// plain Skip Credits pill that seeks past it, not a countdown into the next episode.
    #[test]
    fn up_next_needs_a_final_credits_marker() {
        let mid = slot_for(Some(marker(MarkerKind::Credits, false)), true, NextEpisodeMode::Countdown);
        assert!(
            matches!(mid, ControlSlot::Skip(p) if p.kind == MarkerKind::Credits
                && p.action == SkipAction::Seek(2_000 * 1_000_000)),
            "a non-final credits marker offers Skip Credits even with a successor queued"
        );
        assert!(matches!(slot_for(Some(marker(MarkerKind::Credits, true)), true, NextEpisodeMode::Countdown), ControlSlot::UpNext(_)));
    }

    /// **Next episode: After credits / Off.** During the credits of an episode with a successor the
    /// control row offers nothing — no Up Next tile (so no countdown and no HUD raise, both of
    /// which key off the slot), and no Skip Credits pill. Everything else keeps its occupant.
    #[test]
    fn after_credits_and_off_offer_nothing_during_the_credits() {
        for mode in [NextEpisodeMode::AfterCredits, NextEpisodeMode::Off] {
            assert!(slot_for(Some(marker(MarkerKind::Credits, true)), true, mode).is_discs(), "{mode:?}");
            // a show's last episode and a movie keep the Skip Credits pill, which finishes the item
            assert!(matches!(
                slot_for(Some(marker(MarkerKind::Credits, true)), false, mode),
                ControlSlot::Skip(p) if p.action == SkipAction::Finish
            ));
            // a mid-item credits segment and an intro are skippable as ever
            assert!(matches!(
                slot_for(Some(marker(MarkerKind::Credits, false)), true, mode),
                ControlSlot::Skip(p) if p.action == SkipAction::Seek(2_000 * 1_000_000)
            ));
            assert!(matches!(slot_for(Some(marker(MarkerKind::Intro, false)), true, mode), ControlSlot::Skip(_)));
        }
        assert!(matches!(
            slot_for(Some(marker(MarkerKind::Credits, true)), true, NextEpisodeMode::Countdown),
            ControlSlot::UpNext(_)
        ));
    }

    /// A `final` credits segment runs to the end of the item, so skipping it FINISHES rather than
    /// seeks — seeking to its stated end would race the decoder against its own last frames.
    #[test]
    fn only_a_final_credits_segment_finishes_the_item() {
        let fin = match slot_for(Some(marker(MarkerKind::Credits, true)), false, NextEpisodeMode::Countdown) {
            ControlSlot::Skip(p) => p.action,
            _ => unreachable!(),
        };
        assert_eq!(fin, SkipAction::Finish);

        // a mid-item credits segment (a post-credits scene follows) seeks past it and plays on
        let mid = match slot_for(Some(marker(MarkerKind::Credits, false)), false, NextEpisodeMode::Countdown) {
            ControlSlot::Skip(p) => p.action,
            _ => unreachable!(),
        };
        assert_eq!(mid, SkipAction::Seek(2_000 * 1_000_000));
        // …as does an intro, `final` flag or not (PMS only sets it on credits)
        let intro = match slot_for(Some(marker(MarkerKind::Intro, true)), false, NextEpisodeMode::Countdown) {
            ControlSlot::Skip(p) => p.action,
            _ => unreachable!(),
        };
        assert_eq!(intro, SkipAction::Seek(2_000 * 1_000_000));
    }

    /// The ring only goes back to the scrubber on the EDGE where a stand-in vanished under it.
    /// As a steady state (`is_discs() && focused`, which is how it shipped) it fired on every
    /// frame the user had walked UP to the Subtitles/Audio discs, so focus could not rest there
    /// and OK on a disc never reached the track menu — invisible to the pointer path, and so to
    /// the on-device suite.
    #[test]
    fn only_a_vanishing_standin_takes_the_focus_ring_back() {
        let discs = slot_for(None, false, NextEpisodeMode::Countdown);
        let skip = slot_for(Some(marker(MarkerKind::Intro, false)), false, NextEpisodeMode::Countdown);

        // the edge it exists for: the pill was there last frame, the discs are back, ring on the row
        assert!(standin_left_the_ring(true, discs, true));

        // the steady state it must NOT fire on: the discs were already there, so nothing vanished
        assert!(
            !standin_left_the_ring(false, discs, true),
            "walking UP to the discs is not a lost stand-in"
        );

        // nothing to take back if the ring is elsewhere, or if a stand-in still owns the row
        assert!(!standin_left_the_ring(true, discs, false));
        assert!(!standin_left_the_ring(true, skip, true));
    }
    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }
    fn assert_rect(r: Rect, x: f32, y: f32, w: f32, h: f32) {
        assert!(
            close(r.x, x) && close(r.y, y) && close(r.w, w) && close(r.h, h),
            "got ({}, {}, {}, {}), want ({x}, {y}, {w}, {h})",
            r.x,
            r.y,
            r.w,
            r.h
        );
    }

    /// **Every tone has an ink, the first is the white every earlier build drew, and each rung
    /// under it is strictly darker** — the ladder's whole promise is "down is dimmer", and the
    /// table is indexed by position, so a rung added to `SubtitleTone::LADDER` without an ink
    /// here would silently draw white. Opaque and achromatic throughout: a tone gives up light,
    /// never coverage (the outline depends on it) and never hue (it tints image subtitles).
    #[test]
    fn the_subtitle_tones_are_a_strictly_darkening_opaque_grey_ladder_from_white() {
        use crate::catalog::session::SubtitleTone;
        assert_eq!(theme::SUBTITLE_INKS.len(), SubtitleTone::LADDER.len());
        assert_eq!(subtitle_ink_for(SubtitleTone::White), [1.0, 1.0, 1.0, 1.0]);
        let mut prev = f32::MAX;
        for tone in SubtitleTone::LADDER {
            let ink = subtitle_ink_for(tone);
            assert!(ink[0] == ink[1] && ink[1] == ink[2], "{tone:?} has a hue");
            assert_eq!(ink[3], 1.0, "{tone:?} is not opaque");
            assert!(ink[0] < prev, "{tone:?} is not darker than the rung above it");
            prev = ink[0];
        }
        // the darkest rung still has to be READ over the 0.85 black outline it is drawn on
        assert!(prev > 0.25, "the darkest tone has sunk into its own outline");
    }

    /// A 1080p-authored PGS rect must land EXACTLY where it always did — the scale is identity,
    /// not merely close to it, or every Blu-ray subtitle in the library moves a pixel.
    #[test]
    fn image_sub_1080p_canvas_is_identity() {
        assert_rect(
            sub_screen_rect((300, 900, 1320, 96), 1920, 1080),
            300.0,
            900.0,
            1320.0,
            96.0,
        );
    }

    /// The bug this unit exists for: a DVD VobSub rip is authored on a 720×480 canvas, so drawn
    /// 1:1 it is a postage stamp covering the top-left third of the panel. Scaled from its own
    /// canvas it spans the picture and sits at the bottom, exactly like the PGS case above.
    #[test]
    fn image_sub_dvd_canvas_fills_the_panel() {
        // a full-width, near-bottom VobSub line
        assert_rect(
            sub_screen_rect((0, 400, 720, 60), 720, 480),
            0.0,
            900.0,
            1920.0,
            135.0,
        );
        // and an inset one keeps its position within the picture
        let r = sub_screen_rect((180, 240, 360, 48), 720, 480);
        assert_rect(r, 480.0, 540.0, 960.0, 108.0);
        assert!(
            close(r.x + r.w * 0.5, SCR_W * 0.5),
            "a canvas-centred rect stays centred"
        );
    }

    /// PAL (720×576) and 4K PGS (3840×2160) are the other two canvases in the wild; the 4K one
    /// scales DOWN, which the old 1:1 path drew at double size and half off-screen.
    #[test]
    fn image_sub_pal_and_4k_canvases() {
        assert_rect(
            sub_screen_rect((0, 480, 720, 72), 720, 576),
            0.0,
            900.0,
            1920.0,
            135.0,
        );
        assert_rect(
            sub_screen_rect((600, 1800, 2640, 192), 3840, 2160),
            300.0,
            900.0,
            1320.0,
            96.0,
        );
    }

    /// A decoder that never declares a canvas (0) must fall back to 1:1 — the behaviour of the
    /// whole path before this change — rather than divide by zero or blank the cue.
    #[test]
    fn image_sub_unknown_canvas_stays_1to1() {
        assert_rect(
            sub_screen_rect((300, 900, 1320, 96), 0, 0),
            300.0,
            900.0,
            1320.0,
            96.0,
        );
        assert_rect(
            sub_screen_rect((300, 900, 1320, 96), -1, 1080),
            300.0,
            900.0,
            1320.0,
            96.0,
        );
    }

    /// The HUD lift: image cues rise just enough to clear the transport, and only then.
    #[test]
    fn image_sub_lifts_only_over_the_transport() {
        let dialogue = Rect::new(300.0, 900.0, 1320.0, 135.0); // bottom 1035, overhangs
                                                               // transport down: nothing moves, ever
        assert_eq!(hud_lift([dialogue].into_iter(), false), 0.0);
        // transport up: a bottom-anchored cue rises exactly its overhang, landing ON the ceiling
        let lift = hud_lift([dialogue].into_iter(), true);
        assert!(
            close(1035.0 - lift, SUB_CEIL_Y),
            "lifted bottom should sit at the ceiling"
        );
        // a sign at the top of frame already clears it and must stay put
        let sign = Rect::new(100.0, 100.0, 200.0, 100.0);
        assert_eq!(hud_lift([sign].into_iter(), true), 0.0);
        assert!(!overhangs(sign) && overhangs(dialogue));
        // a set too tall to lift is clamped instead of being pushed off the top of the screen
        assert_eq!(
            hud_lift([Rect::new(0.0, 0.0, 1920.0, SCR_H)].into_iter(), true),
            0.0
        );
        assert_eq!(
            hud_lift(
                [Rect::new(0.0, 50.0, 1920.0, SCR_H - 50.0)].into_iter(),
                true
            ),
            50.0
        );
    }

    /// The multi-rect case the lift has to get right, and the reason it is measured over the
    /// OVERHANGING rects rather than the whole set: a sign plus dialogue must lift the dialogue
    /// clear WITHOUT dragging the sign (whose top would otherwise clamp the whole shift), and
    /// two-line dialogue must move as one unit or the two lines collapse onto each other.
    #[test]
    fn image_sub_lift_spans_only_the_rects_that_overhang() {
        let sign = Rect::new(100.0, 100.0, 200.0, 100.0);
        let dialogue = Rect::new(300.0, 900.0, 1320.0, 135.0);
        let lift = hud_lift([sign, dialogue].into_iter(), true);
        assert!(
            close(lift, 1035.0 - SUB_CEIL_Y),
            "the sign's top must not clamp the lift"
        );
        assert!(
            close(dialogue.y + dialogue.h - lift, SUB_CEIL_Y),
            "dialogue clears the transport"
        );
        assert!(!overhangs(sign), "and the sign is not shifted at all");

        // two-line dialogue: different overhangs, ONE shift, so the gap between them survives
        let l1 = Rect::new(300.0, 900.0, 1320.0, 50.0); // bottom 950
        let l2 = Rect::new(300.0, 960.0, 1320.0, 50.0); // bottom 1010
        let two = hud_lift([l1, l2].into_iter(), true);
        assert!(
            close(two, 1010.0 - SUB_CEIL_Y),
            "measured over the group, not per rect"
        );
        assert!(overhangs(l1) && overhangs(l2), "both take the same shift");
        assert!(
            close((l2.y - two) - (l1.y + l1.h - two), 10.0),
            "their 10px gap is preserved"
        );
    }

    // ---- who owns the "the pipeline is working" signal -----------------------------------------

    use crate::player::PlaybackState as S;

    /// Every state the enum has, so a state added later cannot quietly escape the table below.
    const ALL_STATES: [S; 7] = [
        S::Idle,
        S::Resolving,
        S::Connecting,
        S::Buffering,
        S::Seeking,
        S::Playing,
        S::Error,
    ];

    /// The whole point of routing both surfaces through one function: for any state, and either
    /// answer to "has this session shown a picture", there is exactly ONE indicator — never two
    /// (the reported bug) and never none while the pipeline is working.
    ///
    /// Driven off `is_busy()` itself rather than a hand-copied list, so a FUTURE busy state cannot
    /// silently lose its indicator; the membership guard underneath pins what `is_busy()` means, so
    /// the coverage claim can't be satisfied by quietly narrowing it.
    #[test]
    fn exactly_one_surface_owns_the_busy_signal() {
        let ps = crate::route::PlaybackSession::IDLE;
        for st in ALL_STATES {
            for seen in [false, true] {
                let b = busy_surface(&ps, st, seen);
                if st.is_busy() || st == S::Error {
                    assert_ne!(b, Busy::None, "{st:?}/seen={seen} lost its indicator");
                } else {
                    assert_eq!(b, Busy::None, "{st:?}/seen={seen} must show nothing");
                }
            }
        }
        for st in ALL_STATES {
            let want = matches!(st, S::Resolving | S::Connecting | S::Buffering | S::Seeking);
            assert_eq!(st.is_busy(), want, "{st:?} changed sides of is_busy()");
        }
    }

    /// The regression, named after the bug. The shipped guard was `is_busy() && frames() == 0`, and
    /// `pump` zeroes `frames` as part of APPLYING a seek — so the centred read-out fired on every
    /// seek, on top of the transport's own spinner. Keyed on the SESSION bit instead, a seek over a
    /// live picture is the transport's alone.
    #[test]
    fn a_seek_over_a_live_picture_belongs_to_the_transport() {
        let ps = crate::route::PlaybackSession::IDLE;
        assert_eq!(busy_surface(&ps, S::Seeking, true), Busy::Transport);
        assert!(!matches!(busy_surface(&ps, S::Seeking, true), Busy::Readout(..)));
    }

    /// The 1-3 frame window between `engine`'s prime→Play clear of `seeking` and the first
    /// presented frame, which the pump publishes as `Buffering`. Keyed on the state alone this
    /// flashes the centred block at the END of every seek; it is precisely why the rule takes
    /// `seen_frame`, and without the assertion someone will "simplify" it back.
    #[test]
    fn the_post_seek_buffering_tail_does_not_flash_the_centre_readout() {
        let ps = crate::route::PlaybackSession::IDLE;
        assert_eq!(busy_surface(&ps, S::Buffering, true), Busy::Transport);
        assert_eq!(
            busy_surface(&ps, S::Resolving, true),
            Busy::Transport,
            "auto-advance over a live picture"
        );
        assert_eq!(busy_surface(&ps, S::Connecting, true), Busy::Transport);
    }

    /// No picture yet → the wait is about the whole panel. With the captions, which locks the
    /// kind↔caption pairing the enum carries (both are chosen once, by this function).
    #[test]
    fn a_cold_start_and_a_reload_both_own_the_whole_panel() {
        let ps = crate::route::PlaybackSession::IDLE;
        assert_eq!(
            busy_surface(&ps, S::Resolving, false),
            Busy::Readout(StatusKind::Working, c"Preparing\u{2026}")
        );
        assert_eq!(
            busy_surface(&ps, S::Connecting, false),
            Busy::Readout(StatusKind::Working, c"Connecting\u{2026}")
        );
        assert_eq!(
            busy_surface(&ps, S::Buffering, false),
            Busy::Readout(StatusKind::Working, c"Buffering\u{2026}")
        );
        // tapping RIGHT during pre-roll (or `/tmp/nativejelly-autoseek`): no picture to mark up
        assert_eq!(
            busy_surface(&ps, S::Seeking, false),
            Busy::Readout(StatusKind::Working, c"Seeking\u{2026}")
        );
    }

    /// A dead producer has no picture and no position, only a message — so it is the centred
    /// read-out either way. And it is a FAULT, never `Empty`, which is the "this is an answer"
    /// treatment.
    #[test]
    fn a_dead_producer_reads_out_whether_or_not_a_picture_was_up() {
        let ps = crate::route::PlaybackSession::IDLE;
        for seen in [false, true] {
            assert_eq!(
                busy_surface(&ps, S::Error, seen),
                Busy::Readout(StatusKind::Failed, c"Playback failed")
            );
        }
    }

    fn readout_of(e: &crate::player::ErrorShape) -> FailureReadout {
        let cx = crate::player::FailureContext { forced: true, can_retry: true, repair_idle: true };
        FailureReadout::of(e, crate::player::failure_actions(e.kind, cx), "0.7.0 · webOS 4.5 · code".into())
    }

    /// **Say it once.** Under Force the reason IS the route's verdict (what happened and why), no
    /// note repeats it, the row is *Switch to Auto and play* · *Back* (no *Details*), and the
    /// support facts are the dim footer — which the draw really paints, below the row.
    #[test]
    fn the_forced_readout_says_what_and_why_once_and_draws_the_diagnostics_as_the_footer() {
        use crate::player::FailureAction as A;
        let verdict = crate::route::PlayVerdict::Forced(crate::route::ForcedFailure::Video).text().to_owned();
        let e = crate::player::failtest_policy_shape_for_test(&verdict);
        let r = readout_of(&e);
        assert_eq!(r.reason(), verdict.as_str());
        assert_eq!(r.note(), None, "the verdict already names the cause");
        assert_eq!(r.actions(), &[A::PlayAutomatically, A::Back]);
        assert_eq!(r.footer(), "0.7.0 · webOS 4.5 · code");
        let log = crate::ui::draw_census::capture(|| {
            r.draw(crate::ui::Painter::recording(), 0, &crate::ui::fixture::FixtureMeasure);
        });
        let texts: Vec<Rect> = log.iter().filter(|(tag, _)| *tag == 100).map(|(_, r)| *r).collect();
        assert!(texts.iter().any(|t| (t.y - FOOTER_TOP).abs() < theme::size::CAPTION as f32 * 2.0),
            "the footer line is drawn at the foot of the screen: {texts:?}");
    }

    /// The read-out's row, note and footer stack top to bottom inside the safe band, and the
    /// footer never meets the row.
    #[test]
    fn the_diagnostics_footer_sits_below_the_row_inside_the_safe_bottom_band() {
        use nj_base::fontcov::advances::ShippedMeasure;
        let verdict = crate::route::PlayVerdict::Forced(crate::route::ForcedFailure::Video).text().to_owned();
        let e = crate::player::failtest_policy_shape_for_test(&verdict);
        let r = readout_of(&e);
        let row_bottom = r.frames(&ShippedMeasure).iter().flatten().map(|f| f.y + f.h).fold(0.0, f32::max);
        assert!(row_bottom > 0.0, "the row was laid out");
        // a two-line CAPTION note sits between them, with room to spare
        let note_box = theme::size::CAPTION as f32 * 1.5 * 2.0;
        assert!(FOOTER_TOP > row_bottom + note_box + theme::space::MD,
            "footer top {FOOTER_TOP} must clear the row ({row_bottom}) and its note");
        let line_box = theme::size::CAPTION as f32 * 1.5;
        assert!(FOOTER_TOP + line_box < SCR_H - theme::space::XL + 1.0);
    }

    /// **Every row fits the screen, in every shipped language**: the longest row each kind can
    /// draw, measured with the device's advances, stays inside the read-out's width.
    #[test]
    fn every_failure_row_fits_the_screen_in_every_language() {
        use nj_base::fontcov::advances::ShippedMeasure;
        use crate::ui::fit::HEADROOM;
        use nj_platform::i18n::{language_on_this_thread_for_test, Preference};
        use crate::player::FailureAction as A;
        let rows: [&[A]; 3] = [
            &[A::PlayAutomatically, A::TryAgain, A::Back],
            &[A::TryAgain, A::ChangeQuality, A::Back],
            &[A::Repair, A::Back],
        ];
        let mut out = Vec::new();
        for language in [Preference::En, Preference::Es, Preference::Be, Preference::Ru] {
            let _guard = language_on_this_thread_for_test(language);
            for row in rows {
                let labels: Vec<&'static std::ffi::CStr> = row.iter().map(|a| failure_action_label(*a)).collect();
                let o = StatusOverlay::new(Rect::FULL, c"", StatusKind::Failed).page(crate::ui::icons::Icon::Alert).reason(c"x").row(&labels);
                let frames = o.row_frames_measured(&ShippedMeasure);
                let lo = frames.iter().flatten().map(|f| f.x).fold(f32::MAX, f32::min);
                let hi = frames.iter().flatten().map(|f| f.x + f.w).fold(0.0, f32::max);
                let limit = SCR_W - 2.0 * crate::ui::consts::MARGIN_X;
                if (hi - lo) > limit * HEADROOM || lo < crate::ui::consts::MARGIN_X {
                    out.push(format!("{}: {row:?} spans {:.0}px of {limit:.0}", language.tag(), hi - lo));
                }
            }
        }
        assert!(out.is_empty(), "rows wider than the screen:\n  {}", out.join("\n  "));
    }

    /// Every Force verdict fits the read-out's two-line reason slot, in every shipped language.
    #[test]
    fn every_forced_verdict_fits_the_reason_slot_in_every_language() {
        use nj_base::fontcov::advances::ShippedMeasure;
        use crate::ui::fit::HEADROOM;
        use nj_platform::i18n::{language_on_this_thread_for_test, msg, Preference};
        let mut out = Vec::new();
        for language in [Preference::En, Preference::Es, Preference::Be, Preference::Ru] {
            let _guard = language_on_this_thread_for_test(language);
            for s in [msg::widgets_verdict_forced_no_original(), msg::widgets_verdict_forced_container(),
                msg::widgets_verdict_forced_video(), msg::widgets_verdict_forced_audio(),
                msg::widgets_verdict_forced_unauthorized(), msg::widgets_verdict_forced_open(),
                msg::widgets_verdict_forced_audio_conversion(), msg::widgets_reason_forced_playback_help()] {
                let c = std::ffi::CString::new(s).unwrap();
                if StatusOverlay::failed_reason_truncates(&c, &ShippedMeasure, HEADROOM) {
                    out.push(format!("{}: {s}", language.tag()));
                }
            }
        }
        assert!(out.is_empty(), "verdicts cut short:\n  {}", out.join("\n  "));
    }

    /// …and when it does, it owns the WHOLE frame: `draw_hud` draws nothing — no scrim, no
    /// scrubber, no clocks, no bottom tabs (`Player Screen.dc.html` sets `hudDisplay:none` and
    /// `tabsDisplay:none` on the failed variant, because a live-looking scrubber at 0:00 over a
    /// black panel is the bug the read-out exists to end).
    ///
    /// The other half is the one that would be easy to break by simplifying this to "a read-out is
    /// up": every WORKING read-out must leave the transport alone, or the HUD blanks through every
    /// cold start, every reconnect and every pre-roll seek — states where the position is real and
    /// the transport is what the user reads the instant the first frame lands.
    #[test]
    fn only_a_failure_takes_the_frame_away_from_the_transport() {
        let ps = crate::route::PlaybackSession::IDLE;
        for seen in [false, true] {
            assert!(
                readout_owns_frame(busy_surface(&ps, S::Error, seen)),
                "a failure hides the transport"
            );
        }
        for st in [S::Resolving, S::Connecting, S::Buffering, S::Seeking] {
            let b = busy_surface(&ps, st, false);
            assert!(
                matches!(b, Busy::Readout(StatusKind::Working, _)),
                "{st:?} is a read-out"
            );
            assert!(
                !readout_owns_frame(b),
                "{st:?} is mid-flight — the transport stays up under it"
            );
        }
        // and the two non-read-out surfaces are never a reason to blank the frame
        assert!(!readout_owns_frame(busy_surface(&ps, S::Playing, true)));
        assert!(
            !readout_owns_frame(busy_surface(&ps, S::Seeking, true)),
            "a seek over a live picture"
        );
    }

    /// The geometry regression, guarding against a new `OVERLAY_BOTTOM`: the read-out centres on
    /// the PANEL, and doing so still clears the transport — the constraint the carve-out claimed to
    /// be enforcing and was not.
    ///
    /// The spinner half of the block is pure geometry (the caption half needs `text_height`, which
    /// the host suite cannot link) and is the LARGER half, so `above()` is a conservative stand-in
    /// for the whole block. Numerically 540 + 58.16 = 598.16 < 610: deliberately tight, so it fails
    /// if someone bumps `R_PAGE`, thickens the scrim, or re-carves the frame.
    #[test]
    fn the_readout_is_centred_and_still_clears_the_transport() {
        let f = readout_frame();
        assert_eq!(
            f.cy(),
            SCR_H * 0.5,
            "the wait is about the whole picture, so it centres on it"
        );
        assert_eq!(f.cx(), SCR_W * 0.5);
        assert!(
            f.cy() + StatusOverlay::above() < SCR_H - SCRIM_H,
            "the read-out must not reach into the transport scrim"
        );
        assert!(
            f.cy() - StatusOverlay::above() > 0.0,
            "nor off the top of the panel"
        );
    }

    /// The spinner keeps only the busy frames with no direction in them: a re-buffer and the
    /// tail of a landed seek. A seek still travelling shows the thumb, and neither a scrub preview
    /// (not busy: the pipeline still plays the old position) nor the centred read-out's states
    /// reach this slot.
    #[test]
    fn the_spinner_keeps_only_the_busy_frames_with_no_direction() {
        const S: i64 = 1_000_000_000;
        assert!(seek_waiting(Busy::Transport, 42 * S, 42 * S));
        assert!(!seek_waiting(Busy::Transport, 600 * S, 42 * S));
        assert!(!seek_waiting(Busy::Transport, 5 * S, 42 * S));
        assert!(!seek_waiting(Busy::None, 52 * S, 42 * S));
        assert!(!seek_waiting(Busy::None, 42 * S, 42 * S));
        assert!(!seek_waiting(Busy::Readout(StatusKind::Working, c"Buffering…"), 0, 0));
    }
}

#[cfg(test)]
#[path = "player_hud/title_tests.rs"]
mod title_tests;
