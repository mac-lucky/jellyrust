//! **Litefin's Favorites page** (`pages/FavoritesPage.js`, the rail's second item): the user's
//! favourites as the browsed server lists them, one classic row per item type in Litefin's order
//! (films, series, seasons, episodes, people), each headed by its type's name.
//!
//! Geometry is Litefin's (`styles/home.css`): the page is NOT pulled under the rail the way Home
//! is (`.home-page:not(.favorites-page)`), so it starts at x=100; `.home-rows` pads 24 above and
//! 60 below; a row is its `.favorites-row-header` (15 margin, 5 padding, a 30px weight-800 title
//! inside a button with 3px border and 6x16 padding, 5 below) over the classic card track
//! (`card_row`'s HOME and EPISODE presets, the track's 24 above the cards and 38 below their
//! labels) and 28 to the next row. Focus walks a row with LEFT/RIGHT, steps rows with UP/DOWN, and
//! LEFT off a row's first card enters the rail; the focused row rests 40 below the top
//! (`ui/ScrollController.js`). With no favourites, or no answer, the page shows Litefin's
//! `.page-error` box and the rail takes focus.
//!
//! The lists are read on a worker (`AppFx::Favorites`, `jf::favorites`) when the page mounts and
//! again whenever it is uncovered, as Litefin re-renders the page on return; until the first
//! answer the page shows the loading spinner. A card is keyed by its item, so a re-read keeps
//! focus on the card the reader left from.
//!
//! Not copied: the header is not focusable here. Litefin's header button opens the library
//! filtered to favourites, and this app's Library has no favourites filter to open.
use std::borrow::Cow;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::catalog::ServerId;
use crate::catalog_fetch::PmsMovie;
use crate::jf::favorites::Favorite;
use crate::screens::registry::{tile_facts, AppFx, ContentArg, ContentLike, ContentReq};
use crate::ui::card_row::{self, CardRow, RowStyle, TileLabel};
use crate::ui::consts::{CARD_H, CARD_INSET, CARD_W, CONTENT_LEFT, GLOW_PAD, K_SCROLL, SCR_H, SCR_W};
use crate::ui::frame::Budget;
use crate::ui::label::{Label, VAlign};
use crate::ui::screen::{At, AxisMask, Dir, DrawFrame, EdgeRule, ElemKind, Enter, FocusTarget,
    Focusable, GroupKind, GroupSpec, Link, Placed, RenderStrategy, Screen, ScreenEvent, Seat, Step};
use crate::ui::widgets::{Art, PageError};
use crate::ui::{theme, Env, Rect, Spring, View};
use nj_machine::machine::{Canon, Cx, Delivery, Edge, Effects, EntryId, FocusKey, Fx, GroupId,
    Handled, InputKind, InstanceId, Key, LogicalState, Machine, MachineId, Tick};
use nj_machine::present::Provenance;

pub(crate) const SHAPE: &str = "FavoritesScreen{entry:u32,instance:u32,load:u32,pending:bool,scroll:Spring,scroll_target:f32,next_elem:u32,keys:[{kind:u32,sid:u32,id:str,elem:u32}],rows:[{kind:u32,elems:[u32],motion:CardRow}]}";

/// `.home-rows` padding: 24 above the first row, 60 under the last.
const TOP_PAD: f32 = 24.0;
const BOTTOM_PAD: f32 = 60.0;
/// The header button's box: `.favorites-row-header` margin-top 15 and padding-top 5 above it,
/// its 3px border and 6px padding round a 30px line at line-height 1.5, and 5 below it.
const HEADER_TOP: f32 = 15.0 + 5.0;
const HEADER_BOX: f32 = 2.0 * (3.0 + 6.0) + 45.0;
const HEADER_BELOW: f32 = 5.0;
/// The title's cap top inside its 45px line: Poppins' half-leading (45 - 1.4 x 30) / 2 plus its
/// ascender above the cap (1.05 - .70) x 30.
const TITLE_CAP_DY: f32 = HEADER_TOP + 3.0 + 6.0 + 1.5 + 10.5;
/// The title's left edge: the header's 44 margin, the button's 3 border and 16 padding.
const TITLE_X: f32 = CONTENT_LEFT + 44.0 + 3.0 + 16.0;
const TITLE_SZ: std::os::raw::c_int = theme::size::lf::S30;
/// Row top to its pictures: the header, the track's 24 above the cards, the card's border and ring.
const PICTURE_DY: f32 = HEADER_TOP + HEADER_BOX + HEADER_BELOW + 24.0 + CARD_INSET;
/// Picture bottom to the next row: the label block, the card's bottom border, the track's 38 and
/// the row's 28.
const ROW_TAIL: f32 = card_row::UNDER_LABEL_H + 3.0 + 38.0 + 28.0;
/// Where the focused row's top rests (`ScrollController.js`).
const SNAP_Y: f32 = 40.0;
/// The `.page-error` box, with 40 margin round it inside the rows.
const NOTICE: Rect = Rect::new(CONTENT_LEFT + 40.0, TOP_PAD + 40.0, SCR_W - CONTENT_LEFT - 80.0, PageError::H);
fn ordinal(kind: Favorite) -> u32 {
    Favorite::ALL.iter().position(|k| *k == kind).unwrap_or(0) as u32
}

fn group(kind: Favorite) -> GroupId {
    GroupId(0x4641_5600 + ordinal(kind))
}

/// Episodes are the one landscape row (`isLandscape: type === 'episode'`).
fn style(kind: Favorite) -> RowStyle {
    if kind == Favorite::Episodes { RowStyle::EPISODE } else { RowStyle::HOME }
}

fn title(kind: Favorite) -> &'static std::ffi::CStr {
    use nj_platform::i18n::msg;
    match kind {
        Favorite::Movies => msg::favorites_movies_c(),
        Favorite::Series => msg::favorites_series_c(),
        Favorite::Seasons => msg::favorites_seasons_c(),
        Favorite::Episodes => msg::favorites_episodes_c(),
        Favorite::People => msg::favorites_people_c(),
    }
}

/// A row's height, top to the next row's top.
fn row_h(kind: Favorite) -> f32 {
    PICTURE_DY + style(kind).h + ROW_TAIL
}

/// One favourite as its card reads it.
enum Tile {
    Media(PmsMovie),
    /// `kind` is the item's `Type` as the server sends it, which `CardRenderer` puts under a
    /// person's name when there is no role: server text, like the name above it.
    Person { sid: ServerId, key: String, guid: String, name: String, thumb: String, kind: String },
}

impl Tile {
    fn of(kind: Favorite, it: &crate::jf::models::BaseItemDto, sid: ServerId) -> Self {
        if kind == Favorite::People {
            return Tile::Person { sid, key: crate::jf::ids::intern(&it.id).to_string(),
                guid: crate::jf::ids::normalize(&it.id), name: it.name.clone(),
                thumb: crate::jf::images::person_thumb(it), kind: it.kind.clone() };
        }
        Tile::Media(crate::catalog_fetch::jf_row::row(it, sid, 0))
    }

    /// The identity a card's element is keyed by.
    fn id(&self) -> (ServerId, &str) {
        match self {
            Tile::Media(m) => (m.sid, &m.rk),
            Tile::Person { sid, key, .. } => (*sid, key),
        }
    }
}

struct Row {
    kind: Favorite,
    tiles: Vec<Tile>,
    elems: Vec<u32>,
    motion: CardRow,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Load {
    Loading,
    Ready,
    Failed,
}

type Reply = Option<(ServerId, Vec<(Favorite, Vec<crate::jf::models::BaseItemDto>)>)>;

pub(crate) struct FavoritesScreen {
    entry: EntryId,
    instance: InstanceId,
    load: Load,
    pending: Option<Receiver<Reply>>,
    rows: Vec<Row>,
    /// Every card identity an element was minted for; an element is never handed to another.
    keys: Vec<(u32, ServerId, String, u32)>,
    next_elem: u32,
    scroll: Spring,
    scroll_target: f32,
}

impl FavoritesScreen {
    pub(crate) fn new(entry: EntryId, instance: InstanceId) -> Self {
        Self { entry, instance, load: Load::Loading, pending: None, rows: Vec::new(), keys: Vec::new(),
            next_elem: 1, scroll: Spring::at(0.0), scroll_target: 0.0 }
    }

    fn key(&self, elem: u32) -> FocusKey<u32> {
        FocusKey { entry: self.entry, elem }
    }

    fn request<H: ContentLike>(&mut self, fx: &mut Effects<'_, H>) {
        if self.pending.is_some() { return; }
        let (reply, rx) = mpsc::channel();
        self.pending = Some(rx);
        fx.push(Fx::App(AppFx::Favorites { reply }));
    }

    fn intern(&mut self, kind: Favorite, sid: ServerId, id: &str) -> u32 {
        let k = ordinal(kind);
        if let Some(&(_, _, _, elem)) = self.keys.iter().find(|(o, s, i, _)| *o == k && *s == sid && i == id) {
            return elem;
        }
        let elem = self.next_elem;
        self.next_elem = self.next_elem.checked_add(1).expect("Favorites element space exhausted");
        self.keys.push((k, sid, id.to_owned(), elem));
        elem
    }

    /// The lists as they landed: a row per non-empty list, each row keeping its scroll.
    fn land(&mut self, sid: ServerId, lists: Vec<(Favorite, Vec<crate::jf::models::BaseItemDto>)>) {
        let mut previous = std::mem::take(&mut self.rows);
        for (kind, items) in lists {
            let tiles: Vec<Tile> = items.iter()
                .filter(|it| kind == Favorite::People || crate::catalog_fetch::jf_row::listable(it))
                .map(|it| Tile::of(kind, it, sid)).collect();
            if tiles.is_empty() { continue; }
            let elems = tiles.iter().map(|t| { let (s, id) = t.id(); (s, id.to_owned()) }).collect::<Vec<_>>()
                .into_iter().map(|(s, id)| self.intern(kind, s, &id)).collect();
            let motion = previous.iter().position(|r| r.kind == kind).map(|i| previous.remove(i).motion)
                .unwrap_or_else(CardRow::new);
            self.rows.push(Row { kind, tiles, elems, motion });
        }
        self.scroll_target = self.scroll_target.min(self.max_scroll());
    }

    /// Take the worker's answer, if it is in. The first answer seats focus on the first card, or
    /// the rail when there is nothing to focus (`FavoritesPage.js`), wherever the engine parked
    /// it while the page had nothing. A later one leaves a card that left to the end-of-frame
    /// [`Focusable::reconcile`] (the same slot of its row), and seats only a focus that names
    /// nothing this page minted.
    fn drain<H: ContentLike>(&mut self, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        let Some(rx) = &self.pending else { return };
        let reply = match rx.try_recv() {
            Ok(reply) => reply,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => None,
        };
        self.pending = None;
        let first = self.load == Load::Loading;
        match reply {
            Some((sid, lists)) => { self.land(sid, lists); self.load = Load::Ready; }
            // A re-read that fails keeps the rows on screen.
            None if self.load == Load::Loading => self.load = Load::Failed,
            None => {}
        }
        fx.invalidate(Provenance::Landing(MachineId::Instance(self.instance)));
        let focus = cx.focus.current.filter(|k| k.entry == self.entry);
        let on_card = focus.is_some_and(|k| self.index(k.elem).is_some());
        let on_rail = focus.is_some_and(|k| k.elem >= crate::ui::dispatch::STRIP_BASE);
        let was_card = focus.is_some_and(|k| self.keys.iter().any(|key| key.3 == k.elem));
        let target = match self.rows.first() {
            Some(_) if on_card || (!first && (on_rail || was_card)) => return,
            Some(row) => FocusTarget::ContainerGroup(group(row.kind)),
            None if on_rail => return,
            None => FocusTarget::ContainerGroup(crate::ui::containers::tabs::STRIP),
        };
        fx.push(Fx::Deliver(MachineId::Instance(self.instance), Delivery::Screen(ScreenEvent::Enter(Enter::Fresh { focus: target }))));
    }

    fn row_top(&self, row: usize) -> f32 {
        TOP_PAD + self.rows[..row].iter().map(|r| row_h(r.kind)).sum::<f32>()
    }

    fn max_scroll(&self) -> f32 {
        (self.row_top(self.rows.len()) + BOTTOM_PAD - SCR_H).max(0.0)
    }

    fn index(&self, elem: u32) -> Option<(usize, usize)> {
        self.rows.iter().enumerate().find_map(|(r, row)| row.elems.iter().position(|e| *e == elem).map(|c| (r, c)))
    }

    /// The first card of row `row` and its group: the dev `favorites=<row>` trigger's seat.
    pub(crate) fn first_card(&self, row: usize) -> Option<(u32, GroupId)> {
        let r = self.rows.get(row)?;
        Some((r.elems[0], group(r.kind)))
    }

    fn row_of_group(&self, g: GroupId) -> Option<usize> {
        self.rows.iter().position(|r| group(r.kind) == g)
    }

    fn tile_rect(&self, row: usize, col: usize, at: At) -> Rect {
        let r = &self.rows[row];
        let sty = style(r.kind);
        let scroll = if at == At::Drawn { self.scroll.pos } else { self.scroll_target };
        card_row::tile_rect(col, sty.margin_x, sty.w + sty.gap, r.motion.scroll_x(),
            self.row_top(row) + PICTURE_DY - scroll, (sty.w, sty.h))
    }

    fn reveal(&mut self, elem: u32) {
        if let Some((row, _)) = self.index(elem) {
            self.scroll_target = (self.row_top(row) - SNAP_Y).clamp(0.0, self.max_scroll());
        }
    }

    fn open<H: ContentLike>(&self, elem: u32, fx: &mut Effects<'_, H>) -> Handled {
        let Some((row, col)) = self.index(elem) else { return Handled::No };
        let arg = match &self.rows[row].tiles[col] {
            Tile::Media(m) => ContentArg::Detail { sid: m.sid, rk: m.rk.clone() },
            Tile::Person { sid, key, guid, name, thumb, .. } => ContentArg::Person { sid: *sid, key: key.clone(),
                guid: guid.clone(), name: name.clone(), thumb: thumb.clone() },
        };
        fx.push(Fx::App(AppFx::Content(ContentReq::Push(arg))));
        Handled::Yes
    }

    fn tick<H: ContentLike>(&mut self, t: Tick, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        self.drain(cx, fx);
        let focus = cx.focus.current.filter(|k| k.entry == self.entry).map(|k| k.elem);
        // The scroll follows the focused row whoever put focus there.
        if let Some(elem) = focus { self.reveal(elem); }
        for row in &mut self.rows {
            let focused = focus.and_then(|e| row.elems.iter().position(|x| *x == e));
            if focused.is_none() && row.motion.at_exact_rest() { continue; }
            row.motion.update(row.elems.len(), focused, &style(row.kind), t.dt());
        }
        let mut present = fx.present();
        nj_machine::motion::spring(&mut self.scroll.pos, &mut self.scroll.vel, self.scroll_target, K_SCROLL, t, &mut present);
    }

    fn draw_tile<H: ContentLike>(&self, row: usize, col: usize, focused: bool, f: &mut DrawFrame<'_, '_, H>, p: crate::ui::Painter) {
        let r = &self.rows[row];
        let sty = style(r.kind);
        let rest = self.tile_rect(row, col, At::Drawn);
        let press = if focused && f.press.scale > 0.0 { f.press.scale } else { 1.0 };
        let scale = r.motion.scale(col) * press;
        let rect = rest.scaled(scale);
        if !crate::ui::on_axis(rect.x, rect.w, SCR_W, GLOW_PAD) { return; }
        let (art, resume, label) = match &r.tiles[col] {
            Tile::Media(m) if r.kind == Favorite::Episodes => (Art::Still(Some(tile_facts::of(m))), m.resume_frac(), TileLabel::default()),
            Tile::Media(m) => (Art::Poster(Some(tile_facts::of(m))), m.resume_frac(), TileLabel::default()),
            Tile::Person { sid, name, thumb, kind, .. } => (Art::Person { sid: sid.raw(), key: thumb, res: (CARD_W as i32, CARD_H as i32) },
                None, TileLabel::titled(name, kind)),
        };
        if focused {
            let label = label.revealed(r.motion.band_reveal()).settling(r.motion.settle_lag(r.elems.len(), col, &sty));
            card_row::draw_focused(p, art, rect, scale, &sty, resume, &label, f.measure);
        } else {
            card_row::draw_cell(p, art, rect, scale, &sty, resume, &label, false, f.measure);
        }
        if f.records_stops() {
            use crate::ui::screen::{Activate, Hover, Stop};
            f.stop(p, Stop { key: self.key(r.elems[col]), rect, rest_rect: rest,
                clip: Rect::FULL, hover: Hover::Focus, activate: Activate::Press });
        }
    }
}

impl<H: ContentLike> Machine<H> for FavoritesScreen {
    type Ev = ScreenEvent<H>;
    fn step(&mut self, ev: &Self::Ev, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> Handled {
        match ev {
            ScreenEvent::Mount | ScreenEvent::Uncover => self.request(fx),
            ScreenEvent::Tick(t) => self.tick(*t, cx, fx),
            ScreenEvent::FocusMoved { to, .. } => {
                self.reveal(to.elem);
                fx.invalidate(Provenance::Input);
            }
            ScreenEvent::Activate(elem) => return self.open(*elem, fx),
            ScreenEvent::PressCommit(_) | ScreenEvent::PressHold(_) => {
                let Some(key) = cx.focus.current.filter(|k| k.entry == self.entry) else { return Handled::No };
                return self.open(key.elem, fx);
            }
            ScreenEvent::Input(input) => match input.kind {
                InputKind::Key { key: Key::Back, edge: Edge::Down, .. } => {
                    fx.push(Fx::App(AppFx::Content(ContentReq::Back)));
                }
                _ => return Handled::No,
            },
            _ => return Handled::No,
        }
        Handled::Yes
    }
}

impl<H: ContentLike> Focusable<H> for FavoritesScreen {
    fn groups(&self, _cx: &Cx<'_, H>, out: &mut Vec<GroupSpec>) {
        for (i, row) in self.rows.iter().enumerate() {
            out.push(GroupSpec { id: group(row.kind), kind: GroupKind::Row { wrap: false }, seat: Seat::Projected,
                reachable: AxisMask::BOTH, edge: [EdgeRule::Stop; 4], extent: self.tile_rect(i, 0, At::SpringTarget),
                len: row.elems.len(), elem: ElemKind::Control });
        }
    }
    fn group_of(&self, elem: &u32, _: &Cx<'_, H>) -> Option<GroupId> {
        self.index(*elem).map(|(r, _)| group(self.rows[r].kind))
    }
    fn neighbour(&self, from: FocusKey<u32>, dir: Dir, _: &Cx<'_, H>) -> Step<u32> {
        let Some((row, col)) = self.index(from.elem) else { return Step::Edge };
        let next = match dir {
            Dir::Left => col.checked_sub(1),
            Dir::Right => Some(col + 1).filter(|c| *c < self.rows[row].elems.len()),
            _ => None,
        };
        next.map_or(Step::Edge, |c| Step::Move(self.key(self.rows[row].elems[c])))
    }
    fn place(&self, elem: &u32, _: &Cx<'_, H>, at: At) -> Option<Placed> {
        let (row, col) = self.index(*elem)?;
        let rest = self.tile_rect(row, col, at);
        let rect = if at == At::Drawn { rest.scaled(self.rows[row].motion.scale(col)) } else { rest };
        Some(Placed { rect, rest_rect: rest, clip: Rect::FULL, index: Some(col as u32) })
    }
    fn reconcile(&self, want: FocusKey<u32>, _: &Cx<'_, H>) -> FocusKey<u32> {
        if want.entry == self.entry && self.index(want.elem).is_some() { return want; }
        // A card that left: the same slot of its row, else the first card of the first row.
        let slot = self.keys.iter().find(|k| k.3 == want.elem).and_then(|&(kind, ..)| {
            let row = self.rows.iter().find(|r| ordinal(r.kind) == kind)?;
            let col = self.keys.iter().filter(|k| k.0 == kind).position(|k| k.3 == want.elem).unwrap_or(0);
            row.elems.get(col.min(row.elems.len() - 1)).copied()
        });
        slot.or_else(|| self.rows.first().map(|r| r.elems[0])).map_or(want, |e| self.key(e))
    }
    fn seat(&self, g: GroupId, from: Placed, cx: &Cx<'_, H>) -> FocusKey<u32> {
        let Some(row) = self.row_of_group(g) else { return self.reconcile(self.key(0), cx) };
        let r = &self.rows[row];
        let sty = style(r.kind);
        let col = card_row::column_near_x(from.rect.cx(), sty.margin_x, sty.w + sty.gap, sty.w,
            r.motion.scroll_x(), r.elems.len(), from.index.unwrap_or(0) as usize);
        self.key(r.elems[col])
    }
}

impl<H: ContentLike> Screen<H> for FavoritesScreen {
    fn name(&self) -> &'static str { crate::screens::registry::word::FAVORITES }
    fn state(&self) -> &dyn LogicalState { self }
    fn crumb(&self, _: &Cx<'_, H>) -> Option<Cow<'_, str>> { None }
    fn prepare(&mut self, _: &mut Budget, _: &Cx<'_, H>) {}
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, H>) {
        nj_gfx::gfx::frame_clear(theme::CLEAR_RGB.0, theme::CLEAR_RGB.1, theme::CLEAR_RGB.2);
        let p = f.painter.alpha(f.page_alpha);
        match self.load {
            Load::Loading => crate::ui::widgets::Spinner::new(SCR_W * 0.5, SCR_H * 0.5, crate::ui::widgets::Spinner::R_PAGE)
                .phase(f.cx.tick.ms).draw(&Env::inert(), p),
            Load::Failed => PageError { frame: NOTICE, text: nj_platform::i18n::msg::favorites_failed_c() }.draw(&Env::inert(), p),
            Load::Ready if self.rows.is_empty() =>
                PageError { frame: NOTICE, text: nj_platform::i18n::msg::favorites_empty_c() }.draw(&Env::inert(), p),
            Load::Ready => {}
        }
        let focus = f.cx.focus.current.filter(|k| k.entry == self.entry).and_then(|k| self.index(k.elem));
        for i in 0..self.rows.len() {
            let top = self.row_top(i) - self.scroll.pos;
            if !crate::ui::on_axis(top, row_h(self.rows[i].kind), SCR_H, 0.0) { continue; }
            Label::new(title(self.rows[i].kind).as_ptr(), TITLE_SZ, theme::TEXT_PRIMARY).bold().v(VAlign::CapTop)
                .draw(p, Rect::new(TITLE_X, top + TITLE_CAP_DY, 0.0, 0.0));
            let focused = focus.filter(|(r, _)| *r == i).map(|(_, c)| c);
            for col in (0..self.rows[i].elems.len()).filter(|c| Some(*c) != focused) {
                self.draw_tile(i, col, false, f, p);
            }
            if let Some(col) = focused { self.draw_tile(i, col, true, f, p); }
        }
    }
    fn render(&self) -> RenderStrategy { RenderStrategy::Page }
    fn links(&self, out: &mut Vec<Link>) {
        for pair in self.rows.windows(2) {
            out.push(Link { from: group(pair[0].kind), dir: Dir::Down, to: group(pair[1].kind) });
            out.push(Link { from: group(pair[1].kind), dir: Dir::Up, to: group(pair[0].kind) });
        }
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> { Some(self) }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> { Some(self) }
}

impl LogicalState for FavoritesScreen {
    fn write(&self, c: &mut Canon) {
        c.u32(self.entry.0).u32(self.instance.0);
        c.u32(match self.load { Load::Loading => 0, Load::Ready => 1, Load::Failed => 2 });
        c.bool(self.pending.is_some());
        c.f32(self.scroll.pos).f32(self.scroll.vel).f32(self.scroll_target).u32(self.next_elem);
        c.seq(self.keys.len());
        for (kind, sid, id, elem) in &self.keys { c.u32(*kind).u32(u32::from(sid.raw())).str(id).u32(*elem); }
        c.seq(self.rows.len());
        for row in &self.rows {
            c.u32(ordinal(row.kind)).seq(row.elems.len());
            for elem in &row.elems { c.u32(*elem); }
            row.motion.write_motion(c);
        }
    }
    fn probe(&self, out: &mut String) {
        out.push_str(&format!("favorites entry={} load={:?} rows={}", self.entry.0, self.load, self.rows.len()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jf::models::BaseItemDto;
    use crate::screens::registry::{AppMsg, PageMemory};
    use crate::ui::fixture::FixtureMeasure;
    use nj_machine::machine::{FocusRead, Host, InputOwner, PressRead, Stamped};
    use nj_machine::present::Present;

    #[derive(Clone)]
    struct Arg;
    impl LogicalState for Arg {
        fn write(&self, _: &mut Canon) {}
        fn probe(&self, _: &mut String) {}
    }
    impl crate::ui::screen::ScreenArg for Arg {
        fn chrome(&self) -> nj_machine::machine::Chrome { nj_machine::machine::Chrome::TabBar }
        fn id(&self) -> nj_machine::machine::ScreenId { nj_machine::machine::ScreenId(1) }
        fn title(&self) -> Option<&str> { None }
        fn same_instance(&self, _: &Self) -> bool { true }
    }
    struct Fixture;
    impl Host for Fixture {
        type Arg = Arg;
        type Fx = AppFx;
        type Msg = AppMsg;
        type Elem = u32;
        type Views<'a> = ();
        type Init = Arg;
        type Memory = PageMemory;
    }

    const ENTRY: EntryId = EntryId(40);
    const INSTANCE: InstanceId = InstanceId(41);

    fn cx(focus: Option<FocusKey<u32>>) -> Cx<'static, Fixture> {
        Cx { views: (), tick: Tick::default(), measure: &FixtureMeasure,
            focus: FocusRead { current: focus, ..Default::default() }, press: PressRead::default(),
            owner: InputOwner::Entry(ENTRY) }
    }

    fn deliver(s: &mut FavoritesScreen, focus: Option<FocusKey<u32>>, ev: ScreenEvent<Fixture>) -> (Vec<Stamped<Fixture>>, bool) {
        let mut out = Vec::new();
        let mut present = Present::new();
        present.take(0);
        {
            let mut fx = Effects::new(&mut out, MachineId::Instance(INSTANCE), &mut present);
            Machine::<Fixture>::step(s, &ev, &cx(focus), &mut fx);
        }
        (out, present.take(1))
    }

    fn tick(i: u32) -> Tick { Tick { ms: i * 16, dt_us: 16_667 } }

    fn item(kind: &str, n: u32) -> BaseItemDto {
        // The app keys a Jellyfin item by its id's first 13 hex digits, so those differ.
        BaseItemDto { id: format!("{:013x}{}", 0xfa000 + n, "0".repeat(19)), name: format!("Synthetic {n}"), kind: kind.into(), ..Default::default() }
    }

    fn lists(movies: u32, episodes: u32, people: u32) -> Vec<(Favorite, Vec<BaseItemDto>)> {
        vec![(Favorite::Movies, (0..movies).map(|i| item("Movie", i)).collect()),
            (Favorite::Series, Vec::new()), (Favorite::Seasons, Vec::new()),
            (Favorite::Episodes, (0..episodes).map(|i| item("Episode", 100 + i)).collect()),
            (Favorite::People, (0..people).map(|i| item("Person", 200 + i)).collect())]
    }

    /// Mount, answer the request the mount made with `reply`, and land it.
    fn mounted(reply: Reply) -> (FavoritesScreen, Vec<Stamped<Fixture>>) {
        let mut s = FavoritesScreen::new(ENTRY, INSTANCE);
        let (out, _) = deliver(&mut s, None, ScreenEvent::Mount);
        let sender = out.into_iter().find_map(|st| match st.fx { Fx::App(AppFx::Favorites { reply }) => Some(reply), _ => None })
            .expect("the mount asks for the lists");
        sender.send(reply).unwrap();
        let (out, _) = deliver(&mut s, None, ScreenEvent::Tick(tick(1)));
        (s, out)
    }

    /// The group a landing re-seated focus on, if it did.
    fn reseat(out: &[Stamped<Fixture>]) -> Option<GroupId> {
        out.iter().find_map(|st| match &st.fx {
            Fx::Deliver(_, Delivery::Screen(ScreenEvent::Enter(Enter::Fresh { focus: FocusTarget::ContainerGroup(g) }))) => Some(*g),
            _ => None,
        })
    }

    /// Litefin's rows, top to bottom, at its measurements: the title at x=163 with its cap 41
    /// below the row top, the first picture at x=178, 118 below it, the rows 24 below the top and
    /// stacked by their own heights, the episodes landscape, and only the non-empty lists.
    #[test]
    fn the_rows_stand_where_litefins_favorites_page_puts_them() {
        let _serial = nj_base::testlock::serial();
        let (s, _) = mounted(Some((ServerId::UNSET, lists(3, 2, 1))));
        assert_eq!(s.rows.iter().map(|r| r.kind).collect::<Vec<_>>(), [Favorite::Movies, Favorite::Episodes, Favorite::People]);
        assert_eq!((TITLE_X, TOP_PAD + TITLE_CAP_DY), (163.0, 65.0));
        let first = s.tile_rect(0, 0, At::SpringTarget);
        assert_eq!((first.x, first.y, first.w, first.h), (178.0, 142.0, CARD_W, CARD_H));
        assert_eq!(s.tile_rect(0, 1, At::SpringTarget).x, 178.0 + 264.0, "the classic 264 pitch");
        let poster_row = 118.0 + CARD_H + card_row::UNDER_LABEL_H + 69.0;
        assert_eq!(s.row_top(1), 24.0 + poster_row);
        let still = s.tile_rect(1, 0, At::SpringTarget);
        assert_eq!((still.w, still.h), (RowStyle::EPISODE.w, RowStyle::EPISODE.h), "episodes are the landscape row");
        assert_eq!(s.tile_rect(1, 1, At::SpringTarget).x, 178.0 + 424.0, "the landscape 424 pitch");
        assert_eq!(s.row_top(2), s.row_top(1) + 118.0 + RowStyle::EPISODE.h + card_row::UNDER_LABEL_H + 69.0);
        // Litefin's box runs to 40 from the right edge; its centred text stays inside the safe
        // area (the fit test below holds it there).
        assert!(NOTICE.x >= crate::ui::consts::SAFE.x && NOTICE.y >= crate::ui::consts::SAFE.y);
    }

    /// The focused row rests 40 below the top, the first one at the top of the page, and the page
    /// never scrolls past its end.
    #[test]
    fn the_focused_row_rests_forty_below_the_top() {
        let _serial = nj_base::testlock::serial();
        let (mut s, _) = mounted(Some((ServerId::UNSET, lists(3, 2, 1))));
        let card = |s: &FavoritesScreen, row: usize| FocusKey { entry: ENTRY, elem: s.rows[row].elems[0] };
        let to = card(&s, 1);
        deliver(&mut s, None, ScreenEvent::FocusMoved { from: None, to, by: nj_machine::machine::By::Dir });
        assert_eq!(s.scroll_target, s.row_top(1) - 40.0);
        let to = card(&s, 0);
        deliver(&mut s, None, ScreenEvent::FocusMoved { from: None, to, by: nj_machine::machine::By::Dir });
        assert_eq!(s.scroll_target, 0.0, "the first row is 24 down: the page does not scroll for it");
        let to = card(&s, 2);
        deliver(&mut s, None, ScreenEvent::FocusMoved { from: None, to, by: nj_machine::machine::By::Dir });
        assert_eq!(s.scroll_target, s.max_scroll(), "the last row is held at the page's end");
        assert_eq!(s.max_scroll(), s.row_top(3) + 60.0 - SCR_H);
    }

    /// LEFT and RIGHT walk a row and stop at its ends (LEFT off the first card is the rail's);
    /// UP and DOWN step between rows through the links, and a card opens its own page.
    #[test]
    fn focus_walks_the_rows_and_a_card_opens_its_page() {
        let _serial = nj_base::testlock::serial();
        let (mut s, out) = mounted(Some((ServerId::UNSET, lists(2, 1, 1))));
        assert_eq!(reseat(&out), Some(group(Favorite::Movies)), "the first card takes focus");
        let c = cx(None);
        let at = |row: usize, col: usize, s: &FavoritesScreen| FocusKey { entry: ENTRY, elem: s.rows[row].elems[col] };
        assert!(matches!(Focusable::<Fixture>::neighbour(&s, at(0, 0, &s), Dir::Left, &c), Step::Edge));
        assert!(matches!(Focusable::<Fixture>::neighbour(&s, at(0, 0, &s), Dir::Right, &c), Step::Move(k) if k == at(0, 1, &s)));
        assert!(matches!(Focusable::<Fixture>::neighbour(&s, at(0, 1, &s), Dir::Right, &c), Step::Edge));
        let mut groups = Vec::new();
        Focusable::<Fixture>::groups(&s, &c, &mut groups);
        assert!(groups.iter().all(|g| g.edge == [EdgeRule::Stop; 4]), "every edge is the rail's or a link's");
        let mut links = Vec::new();
        Screen::<Fixture>::links(&s, &mut links);
        assert_eq!(links, vec![
            Link { from: group(Favorite::Movies), dir: Dir::Down, to: group(Favorite::Episodes) },
            Link { from: group(Favorite::Episodes), dir: Dir::Up, to: group(Favorite::Movies) },
            Link { from: group(Favorite::Episodes), dir: Dir::Down, to: group(Favorite::People) },
            Link { from: group(Favorite::People), dir: Dir::Up, to: group(Favorite::Episodes) }]);
        let episode = at(1, 0, &s).elem;
        let (out, _) = deliver(&mut s, None, ScreenEvent::Activate(episode));
        assert!(out.iter().any(|st| matches!(&st.fx, Fx::App(AppFx::Content(ContentReq::Push(ContentArg::Detail { rk, .. })))
            if *rk == crate::jf::ids::rating_key(&item("Episode", 100).id))), "an episode opens its detail page");
        let person = at(2, 0, &s).elem;
        let (out, _) = deliver(&mut s, None, ScreenEvent::Activate(person));
        assert!(out.iter().any(|st| matches!(&st.fx, Fx::App(AppFx::Content(ContentReq::Push(ContentArg::Person { name, .. })))
            if name == "Synthetic 200")), "a person opens the person page");
    }

    /// No favourites, or no answer: Litefin's notice and the rail takes focus. A re-read on return
    /// that fails keeps the rows on screen, and one that lands keeps focus on the same card.
    #[test]
    fn nothing_to_show_hands_focus_to_the_rail_and_a_reread_keeps_the_card() {
        let _serial = nj_base::testlock::serial();
        let rail = Some(crate::ui::containers::tabs::STRIP);
        let (empty, out) = mounted(Some((ServerId::UNSET, lists(0, 0, 0))));
        assert_eq!((empty.load, empty.rows.len(), reseat(&out)), (Load::Ready, 0, rail));
        let (failed, out) = mounted(None);
        assert_eq!((failed.load, reseat(&out)), (Load::Failed, rail));

        let (mut s, _) = mounted(Some((ServerId::UNSET, lists(3, 0, 0))));
        let kept = FocusKey { entry: ENTRY, elem: s.rows[0].elems[2] };
        let (out, _) = deliver(&mut s, Some(kept), ScreenEvent::Uncover);
        let reply = out.into_iter().find_map(|st| match st.fx { Fx::App(AppFx::Favorites { reply }) => Some(reply), _ => None })
            .expect("returning to the page reads the lists again");
        drop(reply);
        deliver(&mut s, Some(kept), ScreenEvent::Tick(tick(2)));
        assert_eq!((s.load, s.rows[0].elems.len()), (Load::Ready, 3), "a failed re-read keeps the rows");
        let (out, _) = deliver(&mut s, Some(kept), ScreenEvent::Uncover);
        let reply = out.into_iter().find_map(|st| match st.fx { Fx::App(AppFx::Favorites { reply }) => Some(reply), _ => None }).unwrap();
        let mut fewer = lists(3, 0, 0);
        fewer[0].1.remove(0);
        reply.send(Some((ServerId::UNSET, fewer))).unwrap();
        let (out, _) = deliver(&mut s, Some(kept), ScreenEvent::Tick(tick(3)));
        assert_eq!(s.rows[0].elems[1], kept.elem, "the card keeps its element when a neighbour leaves");
        assert_eq!(reseat(&out), None, "focus stays where the reader is");
    }

    /// A re-read that lands without the focused card (the reader unmarked it on its page) leaves
    /// the seat to the page's reconcile, the same slot of its row, instead of re-seating on the
    /// first card at the top of the page.
    #[test]
    fn a_reread_without_the_focused_card_keeps_its_slot() {
        let _serial = nj_base::testlock::serial();
        let (mut s, _) = mounted(Some((ServerId::UNSET, lists(4, 2, 0))));
        let gone = FocusKey { entry: ENTRY, elem: s.rows[0].elems[2] };
        let (out, _) = deliver(&mut s, Some(gone), ScreenEvent::Uncover);
        let reply = out.into_iter().find_map(|st| match st.fx { Fx::App(AppFx::Favorites { reply }) => Some(reply), _ => None }).unwrap();
        let mut fewer = lists(4, 2, 0);
        fewer[0].1.remove(2);
        reply.send(Some((ServerId::UNSET, fewer))).unwrap();
        let (out, _) = deliver(&mut s, Some(gone), ScreenEvent::Tick(tick(2)));
        assert_eq!(reseat(&out), None, "the landing does not re-seat on the first card");
        assert_eq!(Focusable::<Fixture>::reconcile(&s, gone, &cx(Some(gone))).elem, s.rows[0].elems[2],
            "the reconcile keeps the slot: the film after the one that left");
    }

    /// The engine parks focus on the rail while the page has nothing to focus; the first answer
    /// still seats the first card, as Litefin does after its load, and a later one leaves the rail
    /// to the reader who went there.
    #[test]
    fn the_first_answer_takes_focus_off_the_rail_and_a_later_one_does_not() {
        let _serial = nj_base::testlock::serial();
        let rail = Some(FocusKey { entry: ENTRY, elem: crate::ui::dispatch::STRIP_BASE });
        let mut s = FavoritesScreen::new(ENTRY, INSTANCE);
        let (out, _) = deliver(&mut s, rail, ScreenEvent::Mount);
        let answer = |out: Vec<Stamped<Fixture>>| out.into_iter()
            .find_map(|st| match st.fx { Fx::App(AppFx::Favorites { reply }) => Some(reply), _ => None }).unwrap();
        answer(out).send(Some((ServerId::UNSET, lists(2, 0, 0)))).unwrap();
        let (out, _) = deliver(&mut s, rail, ScreenEvent::Tick(tick(1)));
        assert_eq!(reseat(&out), Some(group(Favorite::Movies)));
        let (out, _) = deliver(&mut s, rail, ScreenEvent::Uncover);
        answer(out).send(Some((ServerId::UNSET, lists(2, 0, 0)))).unwrap();
        let (out, _) = deliver(&mut s, rail, ScreenEvent::Tick(tick(2)));
        assert_eq!(reseat(&out), None);
    }

    /// Every row title and both notices fit on one line in every shipped language: titles from
    /// x=163 to the safe edge at 30px weight 800, notices inside the box's padding and, centred,
    /// inside the safe area.
    #[test]
    fn the_favorites_pages_text_fits_in_every_language() {
        use crate::ui::fit::HEADROOM;
        use nj_base::fontcov::advances::ShippedMeasure;
        use nj_machine::machine::Measure;
        use nj_platform::i18n::{language_on_this_thread_for_test, msg, SHIPPED};
        let m = ShippedMeasure;
        let title_room = (SCR_W - crate::ui::consts::MARGIN_X - TITLE_X) * HEADROOM;
        let safe_right = crate::ui::consts::SAFE.x + crate::ui::consts::SAFE.w;
        let notice_room = (NOTICE.w - 2.0 * PageError::INSET).min(2.0 * (safe_right - NOTICE.cx())) * HEADROOM;
        let mut out = Vec::new();
        for language in SHIPPED {
            let _guard = language_on_this_thread_for_test(language);
            for kind in Favorite::ALL {
                let t = title(kind).to_str().unwrap();
                let w = m.width_str(t, TITLE_SZ, true);
                if w > title_room { out.push(format!("{}: {t:?} is {w:.0}px", language.tag())); }
            }
            for t in [msg::favorites_empty(), msg::favorites_failed()] {
                let w = m.width_str(t, PageError::TEXT, false);
                if w > notice_room { out.push(format!("{}: {t:?} is {w:.0}px", language.tag())); }
            }
        }
        assert!(out.is_empty(), "{out:#?}");
    }

    /// The page's own motion — the scroll to a focused row — keeps the gate awake while it
    /// travels and asks for nothing once it has arrived.
    #[test]
    fn the_scroll_reports_while_it_runs_and_goes_quiet_at_rest() {
        let _serial = nj_base::testlock::serial();
        let (mut s, _) = mounted(Some((ServerId::UNSET, lists(3, 2, 1))));
        let card = FocusKey { entry: ENTRY, elem: s.rows[0].elems[0] };
        for i in 2..200 { deliver(&mut s, Some(card), ScreenEvent::Tick(tick(i))); }
        assert!(!deliver(&mut s, Some(card), ScreenEvent::Tick(tick(200))).1, "a settled page asks for nothing");
        let down = FocusKey { entry: ENTRY, elem: s.rows[2].elems[0] };
        deliver(&mut s, Some(down), ScreenEvent::FocusMoved { from: Some(card), to: down, by: nj_machine::machine::By::Dir });
        assert!(deliver(&mut s, Some(down), ScreenEvent::Tick(tick(201))).1, "the scroll keeps the panel awake");
        let mut frames = 0;
        while deliver(&mut s, Some(down), ScreenEvent::Tick(tick(202 + frames))).1 && frames < 600 { frames += 1; }
        assert!(frames < 600, "the scroll arrives");
        assert!((s.scroll.pos - s.scroll_target).abs() < 0.25);
        assert!(!deliver(&mut s, Some(down), ScreenEvent::Tick(tick(900))).1, "…and the page goes quiet");
    }
}
