//! Paint from the instance and its retained frame views. No live Search/roster/focus reads.
use super::*;
use crate::screens::registry::tile_facts;
use crate::search::scope::{ScopeSource, SourceScopeSnapshot};
use crate::ui::card_row::{self, TileLabel};
use crate::ui::consts::{SCR_H, SCR_W};
use crate::ui::label::{HAlign, Label, VAlign};
use crate::ui::widgets::{Art, Button, StatusKind, StatusOverlay};
use crate::ui::{theme, Env, Painter, View};
use std::ffi::{CStr, CString};
use std::os::raw::c_int;

const CARET_W: f32 = 3.0;
const CARET_GAP: f32 = 4.0;
/// The field's text: Litefin's 22.4px in a 72-tall box, 32 in from either side (7 Input).
const FIELD_TEXT: c_int = theme::size::lf::S22;
const FIELD_PAD: f32 = 32.0;
const GHOST_GAP: f32 = theme::space::SM;
/// The empty state's outline search icon and the air under it (7 Empty state).
const EMPTY_ICON: f32 = 80.0;
const EMPTY_ICON_GAP: f32 = theme::space::MD;
/// Its line, at the caption floor (Litefin's `p` there is unsized).
const EMPTY_TEXT: c_int = theme::size::CAPTION;
const COUNT_GAP: f32 = theme::space::SM;
const SOURCE_PAD: f32 = theme::space::XS;

#[derive(Default)]
pub(super) struct Resources {
    query: String,
    caret: usize,
    run: CString,
    head: CString,
    run_w: f32,
    head_w: f32,
    scope: Option<SourceScopeSnapshot>,
    scope_line: Option<CString>,
    recents: Option<crate::search::recents::RecentsSnapshot>,
    recent_runs: Vec<CString>,
    titles: [CString; 5],
    count_keys: [Option<(Kind, usize)>; 5],
    counts: [CString; 5],
    owner: CString,
}

impl Resources {
    pub(super) fn prepare<H: SearchLike>(
        &mut self,
        query: &str,
        caret: usize,
        owner: &str,
        cx: &Cx<'_, H>,
    ) {
        let q = query.split('\0').next().unwrap_or("");
        let mut caret = caret.min(q.len());
        while !q.is_char_boundary(caret) {
            caret -= 1;
        }
        if self.query != q || self.caret != caret || self.run.is_empty() {
            self.query = q.into();
            self.caret = caret;
            self.run = if q.trim().is_empty() {
                nj_platform::i18n::msg::browse_search_placeholder_c().into()
            } else {
                cstring(q)
            };
            let (_, head) = run_and_head(q, caret);
            self.head = cstring(head);
            self.run_w = cx.measure.width(&self.run, FIELD_TEXT, false);
            self.head_w = if q.trim().is_empty() {
                0.0
            } else {
                cx.measure.width(&self.head, FIELD_TEXT, false)
            };
        }
        let scope = H::search(cx).scope();
        if self
            .scope
            .as_ref()
            .is_none_or(|old| !old.same_publication(scope))
        {
            self.scope_line = scope_text(scope.sources()).map(|line| {
                cstring(&elide(
                    &line,
                    layout::FIELD.w,
                    theme::size::CAPTION,
                    false,
                    cx.measure,
                ))
            });
            self.scope = Some(scope.clone());
        }
        let recents = H::search(cx).recents();
        if self
            .recents
            .as_ref()
            .is_none_or(|old| !old.same_publication(recents))
        {
            self.recent_runs = recents
                .terms()
                .iter()
                .map(|term| {
                    cstring(&elide(
                        term,
                        820.0 - 2.0 * crate::ui::table::CONTENT_X,
                        theme::size::HEADLINE,
                        true,
                        cx.measure,
                    ))
                })
                .collect();
            self.recents = Some(recents.clone());
        }
        let keys = count_keys(H::search(cx).shelves());
        for (i, kind) in crate::search::KINDS.iter().enumerate() {
            if self.titles[i].is_empty() {
                self.titles[i] = cstring(kind.title());
            }
            let key = keys[i];
            if self.count_keys[i] != key {
                self.count_keys[i] = key;
                self.counts[i] = key.map_or_else(CString::default, |(kind, n)| {
                    cstring(&kind.count_label(n))
                });
            }
        }
        if self.owner.to_bytes() != owner.as_bytes() {
            self.owner = cstring(owner);
        }
    }
}

/// Each shelf's count, filed under its KIND's slot — the slot `draw` reads the heading from.
///
/// `shelves()` is compact: a kind with no results has no shelf at all. Filing counts by position in
/// that list put every count after a missing kind one slot early, so a search with no TV Shows
/// headed its Episodes shelf with the Cast & Crew count ("Episodes 2 people").
fn count_keys(shelves: &[crate::search::Shelf]) -> [Option<(Kind, usize)>; 5] {
    let mut keys = [None; 5];
    for shelf in shelves {
        keys[layout::ordinal(shelf.kind) as usize] = Some((shelf.kind, shelf.items.len()));
    }
    keys
}

pub(super) fn draw<H: SearchLike>(screen: &SearchScreen, f: &mut DrawFrame<'_, '_, H>) {
    nj_gfx::gfx::frame_clear(theme::CLEAR_RGB.0, theme::CLEAR_RGB.1, theme::CLEAR_RGB.2);
    let p = f.painter.alpha(f.page_alpha);
    field(screen, f, p);
    let p = p.alpha(screen.fade.alpha());
    if !screen.recents.is_empty() {
        recents(screen, f, p);
    } else if screen.rows.is_empty() {
        empty(screen, f, p);
    }
    for (i, row) in screen.rows.iter().enumerate() {
        let (kinds, n) = screen.kinds();
        let top = layout::top(&kinds[..n], i, |j| screen.rows[j].motion.band_expand())
            - screen.scroll.pos;
        if !crate::ui::on_axis(
            top,
            layout::block_h(row.kind, row.motion.band_expand()),
            SCR_H,
            0.0,
        ) {
            continue;
        }
        let slot = layout::ordinal(row.kind) as usize;
        let title = &screen.render.titles[slot];
        let cap = f.cx.measure.cap_h(theme::size::HEADLINE);
        let source = if screen.owner_row == Some(i) && screen.owner_alpha.pos > OWNER_FLOOR {
            screen.render.owner.as_c_str()
        } else {
            c""
        };
        heading_flow(
            title,
            &screen.render.counts[slot],
            source,
            |run, dx, size, bold, ink, faded| {
                let mut label = Label::new(run.as_ptr(), size, ink).v(VAlign::Baseline);
                if bold != 0 {
                    label = label.bold();
                }
                label.draw(
                    if faded {
                        p.alpha(screen.owner_alpha.pos.clamp(0.0, 1.0))
                    } else {
                        p
                    },
                    Rect::new(layout::ROW_X + dx, top - row.motion.lift(), 0.0, cap),
                );
                f.cx.measure.width(run, size, bold != 0)
            },
        );
        let focused =
            f.cx.focus
                .current
                .and_then(|key| row.elems.iter().position(|elem| *elem == key.elem));
        for col in 0..row.elems.len() {
            if focused != Some(col) {
                tile(screen, i, col, false, f, p);
            }
        }
        if let Some(col) = focused {
            tile(screen, i, col, true, f, p);
        }
    }
}

/// Litefin's search input (7 Input): the action fill inside a 2px border, radius 16, the border
/// turning the accent while the field holds focus - this fork's answer to the upstream "focus by
/// ink alone" verdict - and the query in white, the placeholder in the tertiary ink.
fn field<H: SearchLike>(screen: &SearchScreen, f: &mut DrawFrame<'_, '_, H>, p: Painter) {
    let data = &screen.render;
    let rect = Rect::new(
        layout::FIELD.x,
        layout::FIELD.y - screen.scroll.pos,
        layout::FIELD.w,
        layout::FIELD.h,
    );
    let blank = data.query.trim().is_empty();
    // Two draws: a rim in the same pass as this white film would be added to its white ink.
    p.rect(rect, theme::radius::LG, theme::ACTION_FILL, theme::ACTION_FILL, 0.0);
    p.rring(rect, theme::radius::LG, theme::ACTION_BORDER_W, field_border(screen.hot.pos));
    let ink = if blank { theme::TEXT_TERTIARY } else { theme::TEXT_PRIMARY };
    let text = Rect::new(rect.x + FIELD_PAD, rect.y, rect.w - 2.0 * FIELD_PAD, rect.h);
    let (run_dx, caret_dx) = run_layout(
        data.run_w,
        run_caret_w(blank, data.head_w),
        text.w,
        screen.editing,
    );
    let (cap_top, cap_base) = nj_gfx::text::text_cap_band(FIELD_TEXT, 0);
    {
        let _clip = f.clip(p, text);
        Label::new(data.run.as_ptr(), FIELD_TEXT, ink)
            .draw(p, Rect::new(text.x + run_dx, text.y, text.w, text.h));
        let text_y = nj_gfx::text::text_vcenter_y(FIELD_TEXT, 0, rect.cy());
        if caret_shown(screen.editing, screen.blink_us < super::BLINK_US) {
            p.rect(
                Rect::new(
                    text.x + caret_dx,
                    text_y + cap_top,
                    CARET_W,
                    cap_base - cap_top,
                ),
                0.0,
                theme::TEXT_PRIMARY,
                theme::TEXT_PRIMARY,
                0.0,
            );
        }
        if ghost_shown(&data.query) {
            p.text(
                nj_platform::i18n::msg::browse_search_one_more_c().as_ptr(),
                text.x + caret_dx + CARET_W + GHOST_GAP,
                text_y,
                FIELD_TEXT,
                theme::TEXT_TERTIARY,
                0,
                0,
            );
        }
    }
    if let Some(line) = &data.scope_line {
        Label::new(line.as_ptr(), theme::size::CAPTION, theme::TEXT_TERTIARY).draw(
            p,
            Rect::new(
                rect.x,
                layout::SCOPE_Y - screen.scroll.pos,
                rect.w,
                layout::SCOPE_H,
            ),
        );
    }
    stop(screen, FIELD, ElemKind::Bare, f, p);
}

fn recents<H: SearchLike>(screen: &SearchScreen, f: &mut DrawFrame<'_, '_, H>, p: Painter) {
    let env = Env::inert();
    Label::new(
        nj_platform::i18n::msg::browse_search_recents_c().as_ptr(),
        theme::size::CAPTION,
        theme::TEXT_TERTIARY,
    )
    .draw(
        p,
        Rect::new(
            layout::ROW_X,
            layout::CONTENT_TOP - screen.scroll.pos,
            0.0,
            crate::ui::table::HDR_H,
        ),
    );
    let shown = screen.recents.len().min(layout::RECENT_CAP);
    for (index, elem) in screen.recents.iter().take(shown).enumerate() {
        let Some(key) = screen.keys.iter().find(|key| key.elem == *elem) else {
            continue;
        };
        let Identity::Recent(_) = &key.identity else {
            continue;
        };
        let rect = layout::recent(index, screen.scroll.pos);
        let focused = f.cx.focus.current == Some(screen.key(*elem));
        if focused {
            p.rrect(
                Rect::new(
                    rect.x + crate::ui::table::SIDE,
                    rect.y + crate::ui::table::PILL_INSET,
                    rect.w - 2.0 * crate::ui::table::SIDE,
                    rect.h - 2.0 * crate::ui::table::PILL_INSET,
                ),
                crate::ui::table::PILL_RAD,
                crate::ui::table::PILL_RAD,
                theme::ACCENT,
            );
        }
        let Some(run) = screen.render.recent_runs.get(index) else {
            continue;
        };
        Label::new(
            run.as_ptr(),
            theme::size::HEADLINE,
            if focused {
                theme::ACCENT_INK
            } else {
                theme::TEXT_PRIMARY
            },
        )
        .bold()
        .draw(
            p,
            Rect::new(rect.x + crate::ui::table::CONTENT_X, rect.y, 0.0, rect.h),
        );
        stop(screen, *elem, ElemKind::Bare, f, p);
    }
    let rect = layout::clear(shown, screen.scroll.pos, f.cx.measure);
    Button::new(nj_platform::i18n::msg::browse_search_clear_c().as_ptr(), theme::size::BODY, rect)
        .focused(f.cx.focus.current == Some(screen.key(CLEAR)))
        .draw(&env, p);
    stop(screen, CLEAR, ElemKind::Control, f, p);
}

fn empty<H: SearchLike>(screen: &SearchScreen, f: &DrawFrame<'_, '_, H>, p: Painter) {
    let state = H::search(f.cx).state();
    if screen.draft.pending() {
        return;
    }
    let Some(empty) = empty_state(state, false) else {
        return;
    };
    let mut rect = layout::empty_band(screen.editing);
    rect.y -= screen.scroll.pos;
    if empty == EmptyState::Fault {
        StatusOverlay::new(rect, nj_platform::i18n::msg::browse_search_failed_c(), StatusKind::Failed)
            .reason(nj_platform::i18n::msg::browse_search_failed_detail_c())
            .draw(&Env::inert(), p);
        return;
    }
    // Litefin's empty state: the 80px outline search icon over "Start typing to search"; no
    // results is the statement alone (7 Empty state).
    let statement = if empty == EmptyState::NoResults {
        let shell =
            f.cx.measure
                .width_str(&nj_platform::i18n::msg::browse_search_no_results(""), EMPTY_TEXT, false);
        no_results_line(&elide(
            screen.draft.query().trim(),
            1200.0 - shell,
            EMPTY_TEXT,
            false,
            f.cx.measure,
        ))
    } else {
        nj_platform::i18n::msg::browse_litefin_start_typing().into()
    };
    let statement = cstring(&statement);
    let text_h = f.cx.measure.cap_h(EMPTY_TEXT);
    let icon = (empty == EmptyState::NotYet).then_some(EMPTY_ICON + EMPTY_ICON_GAP).unwrap_or(0.0);
    let top = rect.y + (rect.h - icon - text_h) * 0.5;
    if empty == EmptyState::NotYet {
        crate::ui::icons::draw(p, crate::ui::icons::Icon::Search,
            Rect::new(rect.cx() - EMPTY_ICON * 0.5, top, EMPTY_ICON, EMPTY_ICON), theme::TEXT_PRIMARY);
    }
    Label::new(statement.as_ptr(), EMPTY_TEXT, theme::TEXT_PRIMARY)
        .h(HAlign::Center)
        .v(VAlign::CapTop)
        .draw(p, Rect::new(rect.x, top + icon, rect.w, 0.0));
}

pub(super) fn tile<H: SearchLike>(
    screen: &SearchScreen,
    row: usize,
    col: usize,
    focused: bool,
    f: &mut DrawFrame<'_, '_, H>,
    p: Painter,
) {
    let view = H::search(f.cx);
    let Some(shelf) = view.shelves().get(row) else {
        return;
    };
    let Some(item) = shelf.items.get(col) else {
        return;
    };
    let model = &screen.rows[row];
    let style = layout::style(model.kind);
    let rest = screen.row_rect(row, col, At::Drawn);
    let pop = model.motion.scale(col);
    let press = if focused && f.press.scale > 0.0 {
        f.press.scale
    } else {
        1.0
    };
    let scale = pop * press;
    let rect = rest.scaled(scale);
    if !crate::ui::on_axis(rect.x, rect.w, SCR_W, 32.0) {
        return;
    }
    let resume = match item {
        Item::Media(media) => media.resume_frac(),
        _ => None,
    };
    let sid = match item {
        Item::Media(media) => media.sid,
        Item::Tag(tag) => tag.sid,
        Item::Collection(hit) => hit.item.sid,
    };
    let handle = view
        .scope()
        .sources()
        .iter()
        .find(|source| source.sid == sid && !source.household)
        .map_or("", |source| source.handle.as_str());
    // Every card wears its label at rest too (spec 7: Home-style rows); a catalog row names
    // itself in `draw_cell`, so this text is drawn only for a person or a tag.
    let mut label = TileLabel::titled(item.title(), &subtitle(model.kind, item, handle));
    if focused {
        label = label.settling(model.motion.settle_lag(model.elems.len(), col, &style));
    }
    card_row::draw_cell(p, tile_art(model.kind, item), rect, scale, &style, resume, &label, focused, f.measure);
    stop(
        screen,
        model.elems[col],
        if matches!(model.kind, Kind::Movie | Kind::Show | Kind::Episode) {
            ElemKind::Card
        } else {
            ElemKind::Bare
        },
        f,
        p,
    );
}

/// What a result tile draws. A collection always draws through `Art::Poster` with its row, so the
/// shared card composite decides between its poster (custom, or #274's fan for the server's
/// composite) and the shared neutral collection tile (`ui::collection_tile`, for a row with no
/// thumb) — this screen draws no collection face of its own. A tag-shaped collection hit is
/// already a thumb-less collection row by the time it lands (`search::CollectionHit::from_tag`).
fn tile_art(kind: Kind, item: &Item) -> Art<'_> {
    match (kind, item) {
        (Kind::Episode, Item::Media(media)) => Art::Still(Some(tile_facts::of(media))),
        (_, Item::Media(media)) => Art::Poster(Some(tile_facts::of(media))),
        (_, Item::Collection(hit)) => Art::Poster(Some(tile_facts::of(&hit.item))),
        (Kind::Person, Item::Tag(tag)) => Art::Person {
            sid: tag.sid.raw(),
            key: &tag.thumb,
            res: (300, 300),
        },
        (_, Item::Tag(tag)) if tag.thumb.is_empty() => Art::Poster(None),
        (_, Item::Tag(tag)) => Art::Thumb {
            sid: tag.sid.raw(),
            key: &tag.thumb,
            res: (250, 375),
        },
    }
}

fn stop<H: SearchLike>(
    screen: &SearchScreen,
    elem: u32,
    kind: ElemKind,
    f: &mut DrawFrame<'_, '_, H>,
    p: Painter,
) {
    if !f.records_stops() {
        return;
    }
    use crate::ui::screen::{Activate, Hover, Stop};
    let Some(placed) = <SearchScreen as Focusable<H>>::place(screen, &elem, f.cx, At::Drawn) else {
        return;
    };
    f.stop(
        p,
        Stop {
            key: screen.key(elem),
            rect: placed.rect,
            rest_rect: placed.rest_rect,
            clip: placed.clip,
            hover: Hover::Focus,
            activate: if kind == ElemKind::Bare {
                Activate::Immediate
            } else {
                Activate::Press
            },
        },
    );
}

fn cstring(text: &str) -> CString {
    CString::new(text).unwrap_or_default()
}
fn elide(
    text: &str,
    width: f32,
    size: i32,
    bold: bool,
    measure: &dyn nj_machine::machine::Measure,
) -> String {
    nj_gfx::text::elide_by(text, width, false, |s| {
        measure.width(&cstring(s), size, bold)
    })
}
fn run_layout(run: f32, caret: f32, width: f32, editing: bool) -> (f32, f32) {
    let available = (width - if editing { CARET_W + CARET_GAP } else { 0.0 }).max(0.0);
    let overflow = (caret - available).max(0.0).min((run - available).max(0.0));
    (
        -overflow,
        (caret - overflow + CARET_GAP).min((width - CARET_W).max(0.0)),
    )
}

fn run_and_head(q: &str, caret: usize) -> (&str, &str) {
    let run = &q[..q.find('\0').unwrap_or(q.len())];
    let mut caret = caret.min(run.len());
    while caret > 0 && !run.is_char_boundary(caret) {
        caret -= 1;
    }
    (run, &run[..caret])
}

fn run_caret_w(blank: bool, head_w: f32) -> f32 {
    if blank {
        0.0
    } else {
        head_w
    }
}

fn ghost_shown(q: &str) -> bool {
    let n = q.trim().chars().count();
    n > 0 && n + 1 == crate::search::MIN_QUERY
}

fn caret_shown(editing: bool, phase_on: bool) -> bool {
    editing && phase_on
}

/// The field's border at focus heat `hot`: the action border at rest, the accent focused.
fn field_border(hot: f32) -> [f32; 4] {
    theme::cross(theme::ACTION_BORDER, theme::browse_lf::SEARCH_BORDER_FOCUS, hot)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmptyState {
    NotYet,
    NoResults,
    Fault,
}

fn empty_state(state: crate::search::State, has_shelves: bool) -> Option<EmptyState> {
    use crate::search::State;
    match state {
        State::Idle => Some(EmptyState::NotYet),
        State::Searching => None,
        State::Ready => (!has_shelves).then_some(EmptyState::NoResults),
        State::Failed => Some(EmptyState::Fault),
    }
}

fn no_results_line(q: &str) -> String {
    nj_platform::i18n::msg::browse_search_no_results(q)
}

fn heading_flow(
    title: &CStr,
    count: &CStr,
    source: &CStr,
    mut run: impl FnMut(&CStr, f32, c_int, c_int, [f32; 4], bool) -> f32,
) -> f32 {
    let mut dx = run(
        title,
        0.0,
        theme::size::HEADLINE,
        1,
        theme::TEXT_HEADING,
        false,
    );
    if !count.is_empty() {
        dx += COUNT_GAP;
        dx += run(count, dx, theme::size::BODY, 0, theme::TEXT_TERTIARY, false);
    }
    if source.is_empty() {
        return dx;
    }
    dx += SOURCE_PAD;
    dx += run(c"·", dx, theme::size::BODY, 0, theme::TEXT_SEPARATOR, true);
    dx += SOURCE_PAD;
    dx += run(source, dx, theme::size::BODY, 0, theme::TEXT_TERTIARY, true);
    dx
}

fn subtitle(kind: Kind, item: &Item, handle: &str) -> String {
    let mut parts = Vec::new();
    match item {
        Item::Media(media) if kind == Kind::Episode => {
            let date = crate::ui::fmt::pretty_date(&media.aired, media.year as i64);
            if !date.is_empty() {
                parts.push(date);
            }
        }
        Item::Media(media) if media.year > 0 => parts.push(media.year.to_string()),
        // a collection is its size, in the one formatter every collection surface shares
        Item::Collection(hit) => parts.push(crate::ui::fmt::item_count(hit.item.child_count)),
        _ => {}
    }
    if !handle.is_empty() {
        parts.push(handle.into());
    }
    parts.join(" · ")
}

fn source_label(source: &ScopeSource) -> String {
    if source.household {
        return if source.name.is_empty() {
            nj_platform::i18n::msg::browse_search_your_server().into()
        } else {
            source.name.clone()
        };
    }
    let named: Vec<_> = source
        .libraries
        .iter()
        .filter(|name| !name.is_empty())
        .collect();
    if named.len() == 1 {
        named[0].clone()
    } else if !source.handle.is_empty() {
        source.handle.clone()
    } else {
        nj_platform::i18n::msg::browse_search_shared_server().into()
    }
}
fn join(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => nj_platform::i18n::msg::browse_search_join(a, b),
        _ => nj_platform::i18n::msg::browse_search_join(&names[..names.len() - 1].join(", "), names.last().unwrap()),
    }
}
fn name_set(sources: &[&ScopeSource]) -> String {
    let shares: Vec<_> = sources.iter().filter(|source| !source.household).collect();
    if shares.len() <= 2 {
        return join(
            &sources
                .iter()
                .map(|source| source_label(source))
                .collect::<Vec<_>>(),
        );
    }
    let libraries = shares
        .iter()
        .map(|source| {
            source
                .libraries
                .iter()
                .filter(|name| !name.is_empty())
                .count()
        })
        .sum::<usize>();
    let mut names: Vec<_> = sources
        .iter()
        .filter(|source| source.household)
        .map(|source| source_label(source))
        .collect();
    names.push(if libraries > 0 {
        nj_platform::i18n::msg::browse_search_shared_libraries(libraries as i64)
    } else {
        nj_platform::i18n::msg::browse_search_shared_sources(shares.len() as i64)
    });
    join(&names)
}
fn scope_text(sources: &[ScopeSource]) -> Option<String> {
    // Litefin's search names no scope (7): one answering server is the whole library, so only a
    // second source or a silent one earns the line.
    if sources.is_empty() || (sources.len() == 1 && sources[0].live) {
        return None;
    }
    let (live, down): (Vec<_>, Vec<_>) = sources.iter().partition(|source| source.live);
    if down.is_empty() {
        let mut line = nj_platform::i18n::msg::browse_search_searching(&name_set(&live));
        let mut shares = live
            .iter()
            .filter(|source| !source.household && !source.handle.is_empty());
        if let (2, Some(source), None) = (live.len(), shares.next(), shares.next()) {
            if source_label(source) != source.handle {
                line.push_str(" · ");
                line.push_str(&source.handle);
            }
        }
        Some(line)
    } else if live.is_empty() {
        Some(nj_platform::i18n::msg::browse_search_unreachable(&name_set(&down)))
    } else {
        Some(nj_platform::i18n::msg::browse_search_partial(&name_set(&live), &name_set(&down)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translated_count_messages_follow_belarusian_grammar_with_separate_number_formatting() {
        use nj_platform::i18n::{LocaleContext, Preference, msg};
        let be = LocaleContext::resolve(Preference::Be, None, Some("en-US"), None, None);
        for (count, expected) in [(0, "0 вынікаў"), (1, "1 вынік"), (2, "2 вынікі"),
            (11, "11 вынікаў"), (21, "21 вынік"), (22, "22 вынікі"), (12345, "12,345 вынікаў")] {
            assert_eq!(msg::browse_search_count_in(&be, count), expected);
        }
        assert_eq!(msg::browse_search_people_in(&be, 2), "2 асобы");
        assert_eq!(msg::browse_search_items_in(&be, 11), "11 элементаў");
        let es = LocaleContext::resolve(Preference::Es, None, None, None, None);
        assert_eq!(msg::browse_search_count_in(&es, 1), "1 resultado");
        assert_eq!(msg::browse_search_count_in(&es, 2), "2 resultados");
    }

    #[test]
    fn translated_search_templates_preserve_user_values_and_sentence_order() {
        use nj_platform::i18n::{LocaleContext, Preference, msg};
        let be = LocaleContext::resolve(Preference::Be, None, None, None, None);
        assert_eq!(msg::browse_search_no_results_in(&be, "Кіна {query}"), "Няма вынікаў для «Кіна {query}»");
        let es = LocaleContext::resolve(Preference::Es, None, None, None, None);
        assert_eq!(msg::browse_search_partial_in(&es, "Biblioteca A", "Servidor B"),
            "No se puede conectar con Servidor B · solo hay resultados de Biblioteca A");
        assert_eq!(msg::browse_search_placeholder_c_in(&be).to_str().unwrap(), "Пошук у вашай бібліятэцы");
    }

    use crate::search::{Item, Kind, State, TagHit};

    fn own(name: &str) -> ScopeSource {
        ScopeSource {
            sid: crate::catalog::ServerId::UNSET,
            name: name.into(),
            libraries: Vec::new(),
            handle: String::new(),
            owned: true,
            home: false,
            owner_id: 0,
            household: true,
            live: true,
        }
    }
    fn share(lib: &str, handle: &str) -> ScopeSource {
        ScopeSource {
            sid: crate::catalog::ServerId::UNSET,
            name: "a-hostname".into(),
            libraries: vec![lib.into()],
            handle: handle.into(),
            owned: false,
            home: false,
            owner_id: 0,
            household: false,
            live: true,
        }
    }
    /// A managed/Guest profile's own household server: plex.tv sends it `owned: false` (the
    /// account does not own it) but it is the viewer's own family server, not a stranger's share —
    /// `is_household` (`plex/servers.rs:272`) is true and `owner_credit` deliberately returns no
    /// handle for it (`plex/servers.rs:253-280`'s doc table). It must take the same branch `own`
    /// does: named when it has a name, "your server" when it does not.
    fn household(name: &str) -> ScopeSource {
        ScopeSource {
            sid: crate::catalog::ServerId::UNSET,
            name: name.into(),
            libraries: Vec::new(),
            handle: String::new(),
            owned: false,
            home: true,
            owner_id: 111_111,
            household: true,
            live: true,
        }
    }
    fn down(mut source: ScopeSource) -> ScopeSource {
        source.live = false;
        source
    }
    fn cs(s: &str) -> CString {
        CString::new(s).expect("test literal")
    }

    #[test]
    fn a_count_sits_under_its_own_kind_when_an_earlier_kind_found_nothing() {
        let shelf = |kind, n| crate::search::Shelf {
            kind,
            items: (0..n).map(|_| Item::Media(Default::default())).collect(),
        };
        // No TV Shows: the compact shelf list skips that kind entirely.
        let keys = count_keys(&[
            shelf(Kind::Movie, 5),
            shelf(Kind::Episode, 4),
            shelf(Kind::Person, 2),
        ]);
        assert_eq!(keys[layout::ordinal(Kind::Movie) as usize], Some((Kind::Movie, 5)));
        assert_eq!(keys[layout::ordinal(Kind::Show) as usize], None);
        assert_eq!(keys[layout::ordinal(Kind::Episode) as usize], Some((Kind::Episode, 4)));
        assert_eq!(keys[layout::ordinal(Kind::Person) as usize], Some((Kind::Person, 2)));
        assert_eq!(keys[layout::ordinal(Kind::Collection) as usize], None);
    }

    #[test]
    fn the_scope_block_is_one_line_because_the_minimum_hint_lives_in_the_field() {
        assert_eq!(layout::SCOPE_H, theme::size::CAPTION as f32 * 1.35);
        assert!(ghost_shown("a"));
        assert!(!ghost_shown(""));
        assert!(!ghost_shown("ab"));
    }

    /// Litefin's field focus is its border turning the accent (7 Input), from the action border
    /// at rest, on the same `hot` heat the field already springs.
    #[test]
    fn focus_turns_the_fields_border_from_the_action_border_to_the_accent() {
        let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1.0e-6);
        assert!(near(field_border(0.0), theme::ACTION_BORDER));
        assert!(near(field_border(1.0), theme::browse_lf::SEARCH_BORDER_FOCUS));
        assert!(!near(field_border(0.5), field_border(0.0)));
    }

    #[test]
    fn the_caret_is_visible_only_during_the_on_phase_of_an_editing_field() {
        assert!(caret_shown(true, true));
        assert!(!caret_shown(true, false));
        assert!(!caret_shown(false, true));
        assert!(!caret_shown(false, false));
    }

    #[test]
    fn a_run_that_fits_starts_at_the_edge_and_the_caret_trails_it() {
        let (run, caret) = run_layout(100.0, 100.0, 756.0, true);
        assert_eq!(run, 0.0);
        assert_eq!(caret, 100.0 + CARET_GAP);
    }

    #[test]
    fn an_overlong_run_slides_left_so_the_caret_lands_on_the_boxs_right_edge() {
        let box_w = 756.0;
        let (run, caret) = run_layout(900.0, 900.0, box_w, true);
        assert!(run < 0.0);
        assert_eq!(run + 900.0, box_w - CARET_GAP - CARET_W);
        assert_eq!(caret + CARET_W, box_w);
    }

    #[test]
    fn with_no_caret_the_run_uses_the_whole_box() {
        let box_w = 756.0;
        let (run, _) = run_layout(900.0, 900.0, box_w, false);
        assert_eq!(run + 900.0, box_w);
    }

    #[test]
    fn an_empty_run_keeps_the_designs_caret_gap() {
        let (run, caret) = run_layout(0.0, 0.0, 756.0, true);
        assert_eq!(run, 0.0);
        assert_eq!(caret, CARET_GAP);
    }

    #[test]
    fn the_caret_follows_the_insertion_point_whether_or_not_the_run_has_slid() {
        for run_w in [0.0, 1.0, 400.0, 753.0, 900.0, 5000.0] {
            let (run, caret) = run_layout(run_w, run_w, 756.0, true);
            assert_eq!(caret, run + run_w + CARET_GAP, "run_w={run_w}");
        }
    }

    #[test]
    fn the_run_scrolls_to_the_caret_rather_than_to_the_tail() {
        let box_w = 756.0;
        let avail = box_w - CARET_GAP - CARET_W;
        assert_eq!(run_layout(2000.0, 0.0, box_w, true), (0.0, CARET_GAP));
        assert_eq!(
            run_layout(2000.0, avail - 10.0, box_w, true),
            (0.0, avail - 10.0 + CARET_GAP)
        );
        let (run, caret) = run_layout(2000.0, 1000.0, box_w, true);
        assert_eq!(run, -(1000.0 - avail));
        assert_eq!(caret + CARET_W, box_w);
        for c in [0.0, 1.0, 500.0, 1999.0, 2000.0] {
            let (_, caret) = run_layout(2000.0, c, box_w, true);
            assert!(
                (0.0..=box_w - CARET_W).contains(&caret),
                "caret at {c} drew at {caret}"
            );
        }
    }

    #[test]
    fn the_head_is_the_text_before_the_insertion_point() {
        assert_eq!(run_and_head("wallace", 0), ("wallace", ""));
        assert_eq!(run_and_head("wallace", 3), ("wallace", "wal"));
        assert_eq!(run_and_head("wallace", 7), ("wallace", "wallace"));
        assert_eq!(run_and_head("wallace ", 8), ("wallace ", "wallace "));
        assert_eq!(run_and_head("", 0), ("", ""));
    }

    #[test]
    fn a_caret_inside_a_multi_byte_character_never_splits_it() {
        for q in ["суббота", "千と千尋", "Amélie", "🎬🎬", "aé千🎬z"] {
            for c in 0..=q.len() + 4 {
                let (run, head) = run_and_head(q, c);
                assert_eq!(run, q);
                assert!(q.starts_with(head), "the head must be a prefix: {head:?}");
                assert!(head.len() <= c.min(q.len()));
                assert!(c.min(q.len()) - head.len() < 4);
            }
        }
    }

    #[test]
    fn a_nul_cuts_the_run_and_the_caret_lands_inside_what_is_left() {
        assert_eq!(run_and_head("wal\0lace", 3), ("wal", "wal"));
        assert_eq!(run_and_head("wal\0lace", 8), ("wal", "wal"));
        assert_eq!(run_and_head("\0wallace", 4), ("", ""));
        assert_eq!(run_and_head("су\0ббота", 99), ("су", "су"));
    }

    #[test]
    fn a_box_too_narrow_for_the_caret_does_not_go_negative() {
        assert_eq!(run_layout(0.0, 0.0, 2.0, true), (0.0, 0.0));
    }

    #[test]
    fn a_blank_field_puts_the_caret_at_the_field_start_not_after_the_placeholder() {
        assert_eq!(run_caret_w(true, 999.0), 0.0);
        assert_eq!(run_caret_w(false, 42.0), 42.0);
        let (run, caret) = run_layout(480.0, run_caret_w(true, 0.0), 756.0, true);
        assert_eq!(run, 0.0);
        assert_eq!(caret, CARET_GAP);
        let (buggy_run, buggy_caret) = run_layout(480.0, 480.0, 756.0, true);
        assert_eq!(buggy_run, 0.0);
        assert_eq!(buggy_caret, 480.0 + CARET_GAP);
        assert!(caret < buggy_caret);
    }

    #[test]
    fn one_answering_source_is_silent_and_a_silent_one_is_named() {
        assert_eq!(scope_text(&[own("nas-home")]), None, "Litefin's search names no scope");
        assert_eq!(scope_text(&[share("Film Club", "friend")]), None);
        assert_eq!(
            scope_text(&[down(own("nas-home"))]).unwrap(),
            "nas-home unreachable"
        );
        assert_eq!(scope_text(&[]), None);
    }

    /// A managed/Guest profile's household server was the whole bug: `source_label`/`name_set` read
    /// raw `owned`, which plex.tv sends `false` for a household grant, so with no other source in
    /// scope the line fell through to the library-count branch and, with an empty handle, read the
    /// literal "a shared server" — describing the viewer's own family server as a stranger's. The
    /// fix reads `household` instead, so a lone household source takes the same branch `own` does.
    #[test]
    fn a_household_profiles_only_source_is_named_not_read_as_a_shared_server() {
        assert_eq!(
            scope_text(&[down(household("nas-home"))]).unwrap(),
            "nas-home unreachable"
        );
        assert_eq!(
            scope_text(&[down(household(""))]).unwrap(),
            "your server unreachable"
        );
    }

    /// With a household source AND a genuine friend's share in scope, only the friend is a "share":
    /// the friend keeps its handle attribution and its place in the shared-library count, while the
    /// household source is named exactly like an owned server and never counted as shared. This is
    /// the case the doc table at `plex/servers.rs:253-280` draws the line on — household and a
    /// friend's share are graded differently — and the wording must not blur it.
    #[test]
    fn a_household_source_is_named_while_a_friends_share_stays_credited_and_counted() {
        assert_eq!(
            scope_text(&[household("nas-home"), share("Film Club", "friend")]).unwrap(),
            "Searching nas-home and Film Club · friend"
        );
        let many = vec![
            household("nas-home"),
            share("A", "ann"),
            share("B", "bob"),
            share("C", "cat"),
        ];
        assert_eq!(
            scope_text(&many).unwrap(),
            "Searching nas-home and 3 shared libraries",
            "the household source must not inflate — or hide inside — the shared-library count"
        );
    }

    /// An unnamed, unhandled friend's share must still read as "a shared server" even when a
    /// household source is also in scope — the household branch must not accidentally swallow the
    /// external-share fallback wording.
    #[test]
    fn an_unnamed_friends_share_still_reads_as_a_shared_server_beside_a_household_source() {
        assert_eq!(
            scope_text(&[household("nas-home"), share("", "")]).unwrap(),
            "Searching nas-home and a shared server"
        );
    }

    #[test]
    fn two_live_sources_name_both_and_attribute_the_share() {
        assert_eq!(
            scope_text(&[own("nas-home"), share("Film Club", "friend")]).unwrap(),
            "Searching nas-home and Film Club · friend"
        );
    }

    #[test]
    fn a_share_is_named_by_its_library_and_never_by_its_hostname() {
        let t = scope_text(&[own("nas-home"), share("Film Club", "friend")]).unwrap();
        assert!(t.contains("Film Club"));
        assert!(!t.contains("a-hostname"));
        assert!(t.contains("nas-home"));
        let mut two = share("Film Club", "friend");
        two.libraries = vec!["Film Club".into(), "Archive".into()];
        let t = scope_text(&[own("nas-home"), two]).unwrap();
        assert_eq!(t, "Searching nas-home and friend");
        assert_eq!(t.matches("friend").count(), 1);
    }

    #[test]
    fn many_shares_collapse_to_a_count_rather_than_becoming_a_list() {
        let mk = |lib: &str, handle: &str| share(lib, handle);
        let many = vec![
            own("nas-home"),
            mk("A", "ann"),
            mk("B", "bob"),
            mk("C", "cat"),
        ];
        assert_eq!(
            scope_text(&many).unwrap(),
            "Searching nas-home and 3 shared libraries"
        );
        let mut blank = vec![
            own("nas-home"),
            share("", "ann"),
            share("", "bob"),
            share("", "cat"),
        ];
        for source in blank.iter_mut().skip(1) {
            source.libraries.clear();
        }
        assert_eq!(
            scope_text(&blank).unwrap(),
            "Searching nas-home and 3 shared sources"
        );
    }

    #[test]
    fn a_share_with_no_handle_is_named_but_not_attributed() {
        assert_eq!(
            scope_text(&[own("nas-home"), share("Film Club", "")]).unwrap(),
            "Searching nas-home and Film Club"
        );
    }

    #[test]
    fn an_unreachable_source_names_itself_and_what_is_left() {
        assert_eq!(
            scope_text(&[own("nas-home"), down(share("Film Club", "friend"))]).unwrap(),
            "Film Club unreachable · results from nas-home only"
        );
    }

    #[test]
    fn your_own_server_going_quiet_reads_the_same_way() {
        assert_eq!(
            scope_text(&[down(own("nas-home")), share("Film Club", "friend")]).unwrap(),
            "nas-home unreachable · results from Film Club only"
        );
    }

    #[test]
    fn with_nothing_answering_there_is_no_results_from_clause_to_write() {
        assert_eq!(
            scope_text(&[down(own("nas-home")), down(share("Film Club", "friend"))]).unwrap(),
            "nas-home and Film Club unreachable"
        );
    }

    #[test]
    fn three_sources_list_plainly_and_attribute_none_of_them() {
        assert_eq!(
            scope_text(&[
                own("nas-home"),
                share("Film Club", "friend"),
                share("Archive", "pal")
            ])
            .unwrap(),
            "Searching nas-home, Film Club and Archive"
        );
    }

    #[test]
    fn two_shares_are_named_but_neither_is_attributed() {
        assert_eq!(
            scope_text(&[share("Film Club", "friend"), share("Archive", "pal")]).unwrap(),
            "Searching Film Club and Archive"
        );
    }

    #[test]
    fn an_unnamed_source_still_produces_a_sentence() {
        assert_eq!(
            scope_text(&[own(""), share("Film Club", "friend")]).unwrap(),
            "Searching your server and Film Club · friend"
        );
        assert_eq!(
            scope_text(&[own("nas-home"), share("", "")]).unwrap(),
            "Searching nas-home and a shared server"
        );
    }

    #[test]
    fn an_undescribed_roster_does_not_call_every_machine_yours() {
        assert_eq!(
            scope_text(&[own(""), share("", "")]).unwrap(),
            "Searching your server and a shared server"
        );
    }

    #[test]
    fn the_empty_state_preserves_state_and_region_copy() {
        assert_eq!(
            empty_state(State::Ready, false),
            Some(EmptyState::NoResults)
        );
        assert_eq!(empty_state(State::Failed, false), Some(EmptyState::Fault));
        assert_eq!(empty_state(State::Idle, false), Some(EmptyState::NotYet));
        assert_eq!(empty_state(State::Searching, false), None);
        assert_eq!(empty_state(State::Ready, true), None);
        assert_eq!(empty_state(State::Failed, true), Some(EmptyState::Fault));
        assert_eq!(empty_state(State::Idle, true), Some(EmptyState::NotYet));
    }

    #[test]
    fn the_statement_centres_in_the_space_the_user_can_actually_see() {
        let up = layout::empty_band(true);
        let down = layout::empty_band(false);
        assert_eq!(up.y, layout::CONTENT_TOP);
        assert_eq!(down.y, layout::CONTENT_TOP);
        assert_eq!(down.h - up.h, layout::KEYBOARD_H);
        assert!(up.y + up.h <= SCR_H - layout::KEYBOARD_H);
        assert_eq!(down.y + down.h, SCR_H);
        assert_eq!((up.x, up.w), (0.0, SCR_W));
    }

    #[test]
    fn the_query_is_quoted_back_typographically_and_the_closing_quote_survives() {
        assert_eq!(
            no_results_line("wallace"),
            "No results for \u{201C}wallace\u{201D}"
        );
        assert_eq!(
            no_results_line("wal…"),
            "No results for \u{201C}wal…\u{201D}"
        );
        assert!(!no_results_line("q").contains('"'));
    }

    #[test]
    fn the_heading_states_its_count_and_annotates_only_a_borrowed_source() {
        let w = |s: &str, sz: c_int| s.chars().count() as f32 * sz as f32 * 0.5;
        type Run = (String, f32, c_int, c_int, [f32; 4], bool);
        let runs = |title: &str, count: &str, source: &str| {
            let (t, c, s) = (cs(title), cs(count), cs(source));
            let mut out: Vec<Run> = Vec::new();
            heading_flow(&t, &c, &s, |run, dx, size, bold, ink, faded| {
                let text = run.to_str().unwrap_or_default().to_string();
                out.push((text.clone(), dx, size, bold, ink, faded));
                w(&text, size)
            });
            out
        };
        let bare = runs("Movies", "12 results", "");
        assert_eq!(bare.len(), 2);
        assert_eq!((bare[0].2, bare[0].3), (theme::size::HEADLINE, 1));
        assert_eq!(bare[0].4, theme::TEXT_HEADING);
        assert_eq!((bare[1].2, bare[1].3), (theme::size::BODY, 0));
        assert_eq!(bare[1].4, theme::TEXT_TERTIARY);
        assert_eq!(bare[1].1, w("Movies", theme::size::HEADLINE) + COUNT_GAP);
        let shared = runs("Movies", "12 results", "friend");
        assert_eq!(shared.len(), 4);
        assert_eq!(shared[..2], bare[..2]);
        assert_eq!(shared[2].0, "·");
        assert_eq!(shared[2].4, theme::TEXT_SEPARATOR);
        assert_eq!(
            shared[2].1,
            bare[1].1 + w("12 results", theme::size::BODY) + SOURCE_PAD
        );
        assert_eq!(shared[3].0, "friend");
        assert_eq!(shared[3].4, theme::TEXT_TERTIARY);
        assert_eq!((shared[3].2, shared[3].3), (theme::size::BODY, 0));
        assert!(shared[2].5 && shared[3].5);
        assert!(!shared[0].5 && !shared[1].5);
        assert_eq!(runs("Collections", "", "").len(), 1);
    }

    #[test]
    fn a_caption_identifies_the_result_and_names_a_borrowed_source_last() {
        let film = Item::Media(crate::catalog_fetch::PmsMovie {
            title: "Wallace & Gromit".into(),
            year: 2005,
            ..Default::default()
        });
        assert_eq!(subtitle(Kind::Movie, &film, ""), "2005");
        assert_eq!(subtitle(Kind::Movie, &film, "friend"), "2005 · friend");
        let ep = Item::Media(crate::catalog_fetch::PmsMovie {
            kind: 3,
            title: "A Grand Day Out".into(),
            show_title: "Wallace & Gromit".into(),
            season_index: 1,
            ep_index: 3,
            aired: "1989-11-04".into(),
            ..Default::default()
        });
        assert_eq!(subtitle(Kind::Episode, &ep, ""), "11/4/1989");
        assert_eq!(
            subtitle(Kind::Episode, &ep, "friend"),
            "11/4/1989 · friend"
        );
        let undated = Item::Media(crate::catalog_fetch::PmsMovie {
            kind: 3,
            title: "A Grand Day Out".into(),
            season_index: 1,
            ep_index: 3,
            ..Default::default()
        });
        assert_eq!(subtitle(Kind::Episode, &undated, ""), "");
        assert_eq!(subtitle(Kind::Episode, &undated, "friend"), "friend");
        let bare = Item::Media(crate::catalog_fetch::PmsMovie {
            title: "Untitled".into(),
            ..Default::default()
        });
        assert_eq!(subtitle(Kind::Movie, &bare, ""), "");
        assert_eq!(subtitle(Kind::Movie, &bare, "friend"), "friend");
        let person = Item::Tag(TagHit {
            name: "Peter Sallis".into(),
            count: 9,
            ..Default::default()
        });
        assert_eq!(subtitle(Kind::Person, &person, ""), "");
        // a tag-shaped collection lands as a collection hit carrying the tag's count
        let tagged = |count| Item::Collection(crate::search::CollectionHit::from_tag(&TagHit {
            count,
            ..Default::default()
        }));
        assert_eq!(subtitle(Kind::Collection, &tagged(4), ""), "4 items");
        assert_eq!(subtitle(Kind::Collection, &tagged(1), ""), "1 item");
        // a collection that arrived as a full row reads its childCount through the shared
        // formatter — an empty collection is "0 items", as on its own page
        let full = |child_count| Item::Collection(crate::search::CollectionHit {
            item: crate::catalog_fetch::PmsMovie { child_count, kind: crate::catalog_fetch::KIND_COLLECTION, ..Default::default() },
            ..Default::default()
        });
        assert_eq!(subtitle(Kind::Collection, &full(12), ""), "12 items");
        assert_eq!(subtitle(Kind::Collection, &full(1), "friend"), "1 item · friend");
        assert_eq!(subtitle(Kind::Collection, &full(0), ""), "0 items");
    }

    /// Every collection result reaches the shared card as `Art::Poster` with a kind-4 row, which
    /// is what makes the card draw the shared neutral tile (`ui::collection_tile`) when the row has
    /// no thumb and the poster (or the fan) when it has one. A tag-shaped hit is one of these by
    /// the time it lands (`search::CollectionHit::from_tag`).
    #[test]
    fn a_thumbless_collection_hit_reaches_the_card_as_a_neutral_collection_row() {
        let full = Item::Collection(crate::search::CollectionHit {
            item: crate::catalog_fetch::PmsMovie { title: "Shorts".into(), kind: crate::catalog_fetch::KIND_COLLECTION,
                ..Default::default() },
            tag: 7,
        });
        assert!(matches!(tile_art(Kind::Collection, &full),
            Art::Poster(Some(m)) if m.kind == crate::ui::tile::TileKind::Collection && m.thumb.is_empty()
                && m.title == "Shorts"));
        let with_art = Item::Collection(crate::search::CollectionHit {
            item: crate::catalog_fetch::PmsMovie { thumb: "/library/metadata/50007/thumb/1".into(),
                kind: crate::catalog_fetch::KIND_COLLECTION, ..Default::default() },
            tag: 7,
        });
        assert!(matches!(tile_art(Kind::Collection, &with_art),
            Art::Poster(Some(m)) if !m.thumb.is_empty()));
    }
}
