//! Library instance: document motion and transactions belong here; focus belongs to Input.
mod identity;
mod bookmark;
mod layout;
mod lookahead;
mod parts;
mod rail;
mod transactions;
mod draw;
mod toolbar;
mod status;
pub(crate) mod menu;
#[cfg(test)]
mod tests;
#[cfg(test)]
#[cfg(test)]
mod labels_tests;
#[cfg(test)]
mod deferred_tests;
#[cfg(test)]
mod navigation_tests;
#[cfg(test)]
mod shelf_action_tests;
#[cfg(test)]
mod foreign_replacement_tests;
#[cfg(test)]
mod genre_tests;

use std::borrow::Cow;
use crate::browse::{SecFetch, SecKind};
use crate::screens::registry::{
    AppFx, AppMsg, LibraryCmd, LibraryIdentity, LibraryLike, LibraryMemory,
    LibraryReq, LibrarySectionIdentity, LibraryViewport, PageMemory,
};
use crate::stores::{StoreCmd, StoreId, StoreWork};
use crate::stores::browse::{BrowseCmd, LibraryWork, SectionAddress};
use crate::ui::card_row::{CardRow, RowStyle};
use crate::ui::consts::{MARGIN_X, SCR_W, SCR_H, K_SCROLL, CARD_DY};
use layout::LEFT;
use crate::ui::frame::Budget;
use nj_machine::machine::{
    Canon, Cx, Delivery, Edge, Effects, EntryId, FocusKey, Fx, GroupId, Handled, InputKind,
    InstanceId, Key, LogicalState, Machine, MachineId,
};
use crate::ui::master_detail::{
    Follow, MasterDetail, MasterDetailGroups, MasterDetailLayout, MasterDetailPolicy, MasterSide,
    Region,
};
use nj_machine::present::Provenance;
use crate::ui::screen::{
    At, AxisMask, By, Dir, DrawFrame, EdgeRule, ElemKind, Enter, FocusTarget, Focusable,
    GroupKind, GroupSpec, Link, Part, Placed, RenderStrategy, Screen, ScreenEvent, Seat, Step,
};
use crate::ui::{Rect, Spring};
use crate::ui::xfade::Xfade;
use identity::{KeyRegistry, KeyRegion, region_of_elem};
use layout::{Layout, Block, CONTENT_TOP, MAX_SHELVES};
/// The poster grid's columns, which the tests address cells by.
#[cfg(test)]
use crate::ui::poster_grid::COLS;
use parts::{GridPart, RailPart, GRID_GROUP, RAIL_GROUP};
use transactions::{PendingTransactions, GridTarget, GridAction};

/// The heading row's group before a section is known. A known section's heading is its own
/// group (`LibraryScreen::toolbar_group`), like its grid and alphabet bar: Movies and Shows are one page
/// and one entry, so a shared id carried Movies' heading cursor into Shows as a "restore".
pub(crate) const TOOLBAR_GROUP: GroupId = GroupId(0x4c49_4211);
const STATUS_GROUP: GroupId = GroupId(0x4c49_4212);
const SORT: u32 = 1;
const FILTER: u32 = 2;
const RETRY: u32 = 3;
/// "Connect without encryption?" over a failed source's read-out (`screens::plaintext_question`).
const PLAINTEXT_GROUP: GroupId = GroupId(0x4c49_4213);
const PLAINTEXT_CANCEL: u32 = 5;
const PLAINTEXT_CONNECT: u32 = 6;
const TYPE: u32 = 7;
const SHUFFLE: u32 = 8;
/// The tab pills' elements, `TAB_BASE + index` in [`LibraryScreen::tabs`] order.
const TAB_BASE: u32 = 16;
/// The tab row's group before a section is known; a known section's tabs are their own group,
/// like its heading row ([`TOOLBAR_GROUP`]).
pub(crate) const TABS_GROUP: GroupId = GroupId(0x4c49_4214);
const STRIP: GroupId = crate::ui::containers::tabs::STRIP;

/// Which of Litefin's library views is up: a listing type's grid, the Genres tab's rows, or one
/// genre's grid (its interned Jellyfin genre id), entered from its row's heading. The page's
/// memory keeps it, so a remount from memory reopens the view the reader left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum GenreView { #[default] Off, Rows, Drill(String) }

impl GenreView {
    pub(crate) fn write(&self, c: &mut Canon) {
        match self {
            GenreView::Off => { c.u32(0); }
            GenreView::Rows => { c.u32(1); }
            GenreView::Drill(id) => { c.u32(2).str(id); }
        }
    }
}

pub(crate) const SHAPE: [&str; 7] = [
    "LibraryScreen{entry:u32,instance:u32,wanted:Option<{sid:u32,key:u64}>,head:LibraryHead,scroll:{pos:f32,vel:f32},scroll_target:f32,restore_scroll:Option<f32>,live:bool,initial:bool,provisional:Option<u32>,placed:[(group:u32,elem:u32)],sweep_down:bool,epoch:Option<u32>,query:Option<u32>,grid_reset_pending:bool,shelf_publication:Option<(HubsId{epoch:u32,sid:u32,section:u64},revision:u64)>,page_fade:Xfade{phase:u8,t:f32},grid_fade:Xfade{phase:u8,t:f32},pair:MasterDetailState{side:u32,follow:u32,band:u32,door:Option<u32>},pending:PendingTransactions,chrome:LibraryChrome,memory:PageMemory::Library,viewport_cache:[LibraryViewport],shelves:[{id:str,hub:u32,group:u32,heading:Option<(group:u32,elem:u32)>,landscape:bool,elems:[u32],motion:CardRow}],readout:u32,layout:LibraryLayout,target_layout:LibraryLayout,grid:LibraryGrid,rail:LibraryRail}",
    transactions::SHAPE,
    "LibraryHead{genres:Off|Rows|Drill{id:str},pop:CtlPop{sp:[Spring{pos:f32,vel:f32}],focused:Option<u32>}}",
    "LibraryChrome{pair_groups:{master:u32,detail:u32}}",
    Layout::SHAPE,
    GridPart::SHAPE,
    RailPart::SHAPE,
];

struct Regions;
impl<H: LibraryLike> crate::ui::master_detail::KeyRegion<H> for Regions {
    fn region(&self, elem: &u32, _: &Cx<'_, H>) -> Option<Region> {
        match region_of_elem(*elem) {
            Some(KeyRegion::Grid) => Some(Region::Detail),
            Some(KeyRegion::Rail) => Some(Region::Master),
            _ => None,
        }
    }
}

struct Shelf {
    id: String,
    /// The published hub this row draws: its index in the store's shelves, which the page
    /// filters by view (genre rows on the Genres tab, every other shelf above a grid).
    hub: usize,
    group: GroupId,
    /// A promoted collection shelf's linked heading (`ui::linked_heading`): its one-element group
    /// and element, registered by the hub's identity like the shelf's own group. `None` for every
    /// other hub, which keeps its plain heading.
    heading: Option<(GroupId, u32)>,
    elems: Vec<u32>,
    landscape: bool,
    motion: CardRow,
    // Exact inputs to key registration. Metadata is drawn from the current store view.
    basis: ShelfBasis,
}

#[derive(Default)]
struct ShelfBasis {
    valid: bool,
    query: u32,
    items: Vec<(crate::catalog::ServerId, String)>,
}
impl ShelfBasis {
    fn of(shelf: &crate::browse::section_hubs::Shelf, query: u32) -> Self {
        Self { valid: true, query, items: shelf.items.iter().map(|item| (item.sid, item.rk.clone())).collect() }
    }
    fn matches(&self, shelf: &crate::browse::section_hubs::Shelf, query: u32) -> bool {
        self.valid && self.items.len() == shelf.items.len() && self.items.iter().zip(&shelf.items)
            .all(|((sid, rk), item)| *sid == item.sid && *rk == item.rk && (!rk.is_empty() || self.query == query))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Readout { Loading, Empty, Failed, Grid }

fn readout(table: SecFetch, sections: usize, fetch: SecFetch, total: i64) -> Readout {
    if total < 0 {
        match table {
            SecFetch::Failed => return Readout::Failed,
            SecFetch::Ready if sections == 0 => return Readout::Empty,
            _ => {}
        }
    }
    match fetch {
        SecFetch::Loading => Readout::Loading,
        SecFetch::Failed if total < 0 => Readout::Failed,
        SecFetch::Failed => Readout::Grid,
        SecFetch::Ready if total == 0 => Readout::Empty,
        SecFetch::Ready => Readout::Grid,
    }
}

/// Is the shelf run above the grid final? `Fetching` (the first answer is still inside its
/// bounded first-paint window) and `Staged` (an answer held until the page may move — which a
/// hidden page always may, so it commits on the next frame) both mean the grid's offset is about
/// to change. A publication with no identity has no section behind it and nothing to wait for.
fn shelves_settled(hubs: crate::stores::browse::HubsView<'_>) -> bool {
    hubs.id().is_none() || hubs.publication() == crate::browse::section_hubs::Publication::Committed
}

/// Section `i`'s identity, when the directory knows its server.
fn section_identity(directory: crate::stores::browse::DirectoryView<'_>, i: usize) -> Option<LibrarySectionIdentity> {
    let section = directory.sections().get(i)?;
    Some(LibrarySectionIdentity { sid: section.sid?, key: section.key })
}

/// Where the directory lists `wanted`, if it does.
fn wanted_index(directory: crate::stores::browse::DirectoryView<'_>, wanted: &LibrarySectionIdentity) -> Option<usize> {
    (0..directory.sections().len()).find(|&i| section_identity(directory, i).as_ref() == Some(wanted))
}

/// A retained page body, also remountable from its entry-owned LibraryMemory.
pub(crate) struct LibraryScreen {
    entry: EntryId,
    instance: InstanceId,
    /// The library this page was asked to show and is still waiting for the store to commit;
    /// `None` once it is the current one (or when the page adopted whatever was current).
    wanted: Option<LibrarySectionIdentity>,
    /// The type of the library on screen — a projection of the store's current section, refreshed
    /// by `sync` like `listed`, so not logical state of its own (the TYPE menu's titles read it).
    kind: SecKind,
    /// Is a library on screen at all: the store has a current section and the page is not waiting
    /// for another. Projected by `sync` beside `kind`.
    on_library: bool,
    /// What the committed listing lists — a projection of the store's publication, refreshed by
    /// `sync` like the layout's episode geometry, so not logical state of its own. The toolbar
    /// reads it: collections take no filters, so FILTER leaves the row while they are listed.
    listed: crate::browse::LibraryType,
    keys: KeyRegistry,
    pair: MasterDetail<RailPart, GridPart, Regions>,
    /// The heading row's group: per section and epoch, registered beside the pair's.
    toolbar: GroupId,
    /// The tab row's group, registered the same way.
    tabs: GroupId,
    genres: GenreView,
    /// The head pills' focus pop: tabs, then the controls row ([`toolbar::HEAD_POPS`]).
    head_pop: crate::ui::widgets::CtlPop<{ toolbar::HEAD_POPS }>,
    shelves: Vec<Shelf>,
    section: Option<LibrarySectionIdentity>,
    epoch: Option<u32>,
    query: Option<u32>,
    grid_reset_pending: bool,
    shelf_publication: Option<(crate::browse::section_hubs::HubsId, u64)>,
    layout: Layout,
    target_layout: Layout,
    scroll: Spring,
    scroll_target: f32,
    restore_scroll: Option<f32>,
    viewports: Vec<LibraryViewport>,
    pending: PendingTransactions,
    page_fade: Xfade,
    grid_fade: Xfade,
    readout: Readout,
    live: bool,
    initial: bool,
    /// The head group the page seated its own focus in, while that seat is still the page's own
    /// choice: nobody has moved since. Such a seat follows the head when a landing changes what the
    /// head is — the shelves arrive async, usually after a prepared grid, and commit above it.
    provisional: Option<GroupId>,
    /// Remembered cursors the PAGE wrote by seating itself, which the reader has not confirmed.
    /// The engine remembers every seat alike, so its memory alone cannot tell a restore from the
    /// page's own earlier seat in the same section.
    placed: Vec<(GroupId, u32)>,
    sweep_down: bool,
    /// The failed source's server, when discovery offers the question for it
    /// (`plex::grant::offers`) — the grant table's, not logical state.
    plaintext: super::plaintext_question::OfferWatch,
    /// Why key mode cannot help (`net::keypin::blocked`), held between ticks so the draw and the
    /// *Try again* hit rect of one frame read ONE value — the reason line moves the action row.
    clock: super::clock_readout::ClockWatch,
    /// The question, asked from the failed read-out's *Connect*.
    plaintext_alert: super::plaintext_question::PlaintextAlert,
}

impl LibraryScreen {
    /// A page of ONE library: `wanted` is the section it opens on, `None` for whatever the store
    /// has current.
    pub(crate) fn new(entry: EntryId, instance: InstanceId, wanted: Option<LibrarySectionIdentity>) -> Self {
        let layout = Layout::new(&[], 0, false);
        Self {
            entry, instance, wanted, kind: SecKind::Movie, on_library: false, listed: Default::default(), keys: KeyRegistry::default(),
            pair: MasterDetail::new(
                RailPart::new(entry, RAIL_GROUP), GridPart::new(entry, GRID_GROUP),
                MasterDetailLayout { master: Rect::FULL, detail: Rect::FULL },
                MasterDetailGroups { master: RAIL_GROUP, detail: GRID_GROUP },
                MasterDetailPolicy::new(MasterSide::Right, Follow::OnCommit), Regions,
            ),
            toolbar: TOOLBAR_GROUP, tabs: TABS_GROUP, genres: GenreView::Off,
            head_pop: crate::ui::widgets::CtlPop::new(),
            shelves: Vec::new(), section: None, epoch: None, query: None, grid_reset_pending: false,
            shelf_publication: None, layout, target_layout: layout,
            scroll: Spring::at(0.0), scroll_target: 0.0, restore_scroll: None,
            viewports: Vec::new(),
            pending: PendingTransactions::default(), page_fade: Xfade::new(), grid_fade: Xfade::new(),
            readout: Readout::Loading, live: true, initial: true, provisional: None, placed: Vec::new(), sweep_down: true,
            plaintext: Default::default(),
            clock: Default::default(),
            plaintext_alert: super::plaintext_question::PlaintextAlert::new(PLAINTEXT_GROUP, PLAINTEXT_CANCEL, PLAINTEXT_CONNECT),
        }
    }

    pub(crate) fn restore(&mut self, memory: &LibraryMemory) {
        self.wanted = None;
        self.keys = KeyRegistry::restore(memory);
        // A projection over the replaced registry cannot be retained, even if its basis matches.
        // Retain row motion while invalidating every key projection, as on a normal rebuild.
        for shelf in &mut self.shelves { shelf.basis.valid = false; }
        self.shelf_publication = None;
        self.pair.detail.restore_keys(&self.keys);
        self.section = memory.section.clone();
        self.epoch = memory.epoch;
        self.query = memory.query;
        self.grid_reset_pending = memory.grid_reset_pending;
        if self.grid_reset_pending { self.grid_fade.mount(); }
        self.genres = memory.genres.clone();
        self.viewports = memory.viewports.clone();
        self.restore_scroll = Some(memory.scroll);
        self.scroll.jump(memory.scroll);
        self.scroll_target = memory.scroll;
        self.initial = false;
        self.provisional = None;
    }

    fn key(&self, elem: u32) -> FocusKey<u32> { FocusKey { entry: self.entry, elem } }

    /// What the committed listing lists — the TYPE menu's value as the page last synced it.
    pub(crate) fn listed(&self) -> crate::browse::LibraryType { self.listed }

    /// The heading row's group for the section on the page.
    #[cfg(test)]
    pub(crate) fn toolbar_group(&self) -> GroupId { self.toolbar }

    fn reseat<H: LibraryLike>(&self, focus: FocusTarget<u32>, fx: &mut Effects<'_, H>) {
        fx.push(Fx::Deliver(MachineId::Instance(self.instance),
            Delivery::Screen(ScreenEvent::Enter(Enter::Fresh { focus }))));
    }

    fn address<H: LibraryLike>(&self, cx: &Cx<'_, H>) -> Option<SectionAddress> {
        let id = H::listing(cx).id()?;
        Some(SectionAddress { epoch: id.epoch, sid: id.sid, section: id.section })
    }



    fn store<H: LibraryLike>(&self, target: SectionAddress, work: LibraryWork, fx: &mut Effects<'_, H>) {
        fx.push(Fx::App(AppFx::Store(StoreId::Browse,
            StoreCmd::Browse(BrowseCmd::Addressed { target, work }))));
    }

    fn sync<H: LibraryLike>(&mut self, cx: &Cx<'_, H>) {
        let listing = H::listing(cx);
        self.listed = listing.library_type();
        self.layout = self.layout.with_episodes(self.listed == crate::browse::LibraryType::Episodes);
        let directory = H::directory(cx);
        let identity = listing.id().map(|id| LibrarySectionIdentity { sid: id.sid, key: id.section });
        let epoch = listing.id().map(|id| id.epoch).or(directory.epoch());
        let changed = self.section != identity || self.epoch != epoch;
        if changed { self.grid_reset_pending = false; }
        else if self.query.is_some() && self.query != listing.id().map(|id| id.query) {
            self.grid_reset_pending = true;
            self.restore_scroll = None;
            if !self.grid_fade.is_swapping() { self.grid_fade.mount(); }
        }
        if let (Some(section), Some(epoch)) = (&identity, epoch) {
            let mut group = |kind: &str| GroupId(self.keys.register(LibraryIdentity::Control {
                section: section.clone(), kind: kind.into(), key: epoch.to_string(),
            }, GroupId(0), 0));
            let groups = MasterDetailGroups { master: group("rail-group"), detail: group("grid-group") };
            self.toolbar = group("toolbar-group");
            self.tabs = group("tabs-group");
            if self.pair.groups_config() != groups {
                self.pair = MasterDetail::new(
                    RailPart::new(self.entry, groups.master), GridPart::new(self.entry, groups.detail),
                    MasterDetailLayout { master: Rect::FULL, detail: Rect::FULL }, groups,
                    MasterDetailPolicy::new(MasterSide::Right, Follow::OnCommit), Regions,
                );
                self.pair.detail.restore_keys(&self.keys);
            }
        }
        if changed {
            if self.section.is_some() {
                self.initial = true;
                if !self.page_fade.is_swapping() {
                    // This was not our page transaction: the outgoing publication is already gone.
                    self.page_fade.mount();
                    self.grid_fade = Xfade::new();
                    self.pending.cancel();
                }
            }
            if let Some(viewport) = self.current_viewport() {
                self.viewports.retain(|old| old.epoch != viewport.epoch || old.section != viewport.section);
                self.viewports.push(viewport);
            }
            self.shelves.clear();
            self.shelf_publication = None;
            self.section = identity.clone();
            self.epoch = epoch;
            let saved = self.viewports.iter().find(|view| Some(view.epoch) == epoch && Some(&view.section) == identity.as_ref());
            self.restore_scroll = saved.map(|view| view.scroll).or_else(||
                cx.focus.remembered(self.pair.groups_config().detail).is_none()
                    .then(|| listing.cursor().map(|cursor| cursor.scroll)).flatten());
            self.scroll.jump(self.restore_scroll.unwrap_or(0.0));
            self.scroll_target = self.scroll.pos;
        }
        self.query = listing.id().map(|id| id.query);
        let table = if directory.sections().is_empty() { directory.discovery() } else { directory.source_fetch() };
        self.readout = readout(table, directory.sections().len(), listing.fetch(), listing.total());
        if directory.current().and_then(|i| section_identity(directory, i)) == self.wanted {
            self.wanted = None;
        }
        if let (None, Some(current)) = (&self.wanted, directory.current()) {
            self.kind = directory.sections()[current].kind;
        }
        self.on_library = self.wanted.is_none() && directory.current().is_some();
        if let Some(wanted) = self.wanted.clone() {
            self.shelves.clear();
            self.shelf_publication = None;
            self.pair.detail.clear_projection();
            self.pair.master.clear_projection();
            self.grid_fade = Xfade::new();
            self.readout = if wanted_index(directory, &wanted).is_some() { Readout::Loading }
                else { readout(directory.discovery(), 0, directory.discovery(), -1) };
            self.relayout(cx.focus.current);
            return;
        }
        let hubs = H::section_hubs(cx);
        let publication = hubs.id().zip(hubs.revision());
        if self.shelf_publication != publication {
            // The Genres tab draws the genre rows; every other view the remaining shelves.
            let rows = self.genres == GenreView::Rows;
            let wanted: Vec<_> = hubs.shelves().iter().enumerate()
                .filter(|(_, shelf)| shelf.genre.is_some() == rows).take(MAX_SHELVES).collect();
            let mut old = super::registry::RowSeats::new(std::mem::take(&mut self.shelves), wanted.len());
            if let Some(section) = &identity {
                let mut identities = std::collections::HashSet::new();
                let duplicate = wanted.iter().any(|(_, shelf)| !identities.insert(&shelf.id));
                for (index, &(hub, shelf)) in wanted.iter().enumerate() {
                    let linked = shelf.link.is_some() || shelf.genre.is_some();
                    let same = old.at(index).is_some_and(|row: &Shelf| row.id == shelf.id);
                    if same && !duplicate && old.at(index).is_some_and(|row|
                        row.landscape == shelf.landscape && row.heading.is_some() == linked
                            && row.basis.matches(shelf, self.query.unwrap_or(0))) {
                        let mut row = old.keep(index);
                        row.hub = hub;
                        self.shelves.push(row);
                        continue;
                    }
                    let from = old.locate(index, same, &shelf.id, |row| row.id.clone());
                    let group = GroupId(self.keys.register(
                        LibraryIdentity::Control { section: section.clone(), kind: "shelf-group".into(), key: shelf.id.clone() },
                        GroupId(0), index));
                    let elems = shelf.items.iter().enumerate().map(|(col, item)| {
                        let identity = if item.rk.is_empty() {
                            LibraryIdentity::ShelfSlot { section: section.clone(), hub: shelf.id.clone(), publication: self.query.unwrap_or(0), slot: col as u32 }
                        } else {
                            LibraryIdentity::Shelf { section: section.clone(), hub: shelf.id.clone(), sid: item.sid, rk: item.rk.clone() }
                        };
                        self.keys.register(identity, group, col)
                    }).collect();
                    let motion = from.and_then(|at| old.take(at)).map(|row| row.motion).unwrap_or_else(|| {
                            let mut motion = CardRow::new();
                            let saved = self.viewports.iter().find(|view| Some(view.epoch) == epoch && view.section == *section)
                                .and_then(|view| view.shelves.iter().find(|(id, _)| *id == shelf.id));
                            if let Some((_, x)) = saved {
                                motion.restore_scroll(*x, shelf.items.len(), shelf_style(shelf.landscape));
                            }
                            motion
                        });
                    let heading = linked
                        .then(|| {
                            let control = |kind: &str| LibraryIdentity::Control {
                                section: section.clone(), kind: kind.into(), key: shelf.id.clone() };
                            let heading_group = GroupId(self.keys.register(control("shelf-heading-group"), GroupId(0), index));
                            let elem = self.keys.register(control("shelf-heading"), heading_group, 0);
                            (heading_group, elem)
                        });
                    old.rebuilt(from);
                    self.shelves.push(Shelf { id: shelf.id.clone(), hub, group, heading, elems,
                        landscape: shelf.landscape, motion, basis: ShelfBasis::of(shelf, self.query.unwrap_or(0)) });
                }
            }
            self.shelf_publication = publication;
        }
        self.pair.detail.refresh(cx, &mut self.keys);
        self.relayout(cx.focus.current);
        self.pair.master.refresh(cx, &mut self.keys);
    }

    fn relayout(&mut self, focus: Option<FocusKey<u32>>) {
        let grid_focus = focus.filter(|key| key.entry == self.entry)
            .and_then(|key| self.pair.detail.index_of(key.elem));
        self.pair.detail.focus_row(grid_focus.map(|index| index / self.layout.cols()), false);
        let pitches: Vec<_> = self.shelves.iter().map(|row| layout::shelf_pitch(row.landscape, row.motion.band_expand())).collect();
        let targets: Vec<_> = self.shelves.iter().map(|row| layout::shelf_pitch(row.landscape,
            f32::from(focus.is_some_and(|key| row.elems.contains(&key.elem))))).collect();
        let rows = self.pair.detail.elems.len().div_ceil(self.layout.cols());
        // A grid transaction's commit empties the store (`browse::requery`) while the grid is
        // dark and its answer is in flight. The shelves, heading and TYPE chip around it are
        // NOT dark: sizing the document from the empty interim would shrink `max_scroll` and
        // slide the whole page to the clamp and back when the answer lands. Until the answer
        // says otherwise, the dark grid keeps reserving the extent it had.
        let rows = if rows == 0 && self.grid_fade.is_swapping() && self.readout == Readout::Loading {
            self.layout.rows
        } else { rows };
        // The Genres tab's rows replace the grid; one genre's grid stands under its title alone.
        let rows = if self.grid_shown() { rows } else { 0 };
        let drill = matches!(self.genres, GenreView::Drill(_));
        let grid_head = self.genres == GenreView::Off && (rows > 0 || self.grid_fade.is_swapping()
            // An empty answer keeps the heading row: its View Mode is how the reader leaves an
            // empty listing (no collections, no episodes) for one that has something in it.
            || (self.section.is_some() && self.on_library && self.readout == Readout::Empty));
        let episodes = self.layout.is_episodes();
        self.layout = if self.readout == Readout::Failed {
            Layout::failed(&pitches)
        } else { Layout::new(&pitches, rows, grid_head) }.with_episodes(episodes)
            .with_drill(drill).with_grid_bands(self.pair.detail.band_geometry());
        self.target_layout = Layout::new(&targets, rows, grid_head).with_episodes(episodes)
            .with_drill(drill).with_grid_focus(grid_focus.map(|index| index / self.layout.cols()));
        if self.grid_reset_pending {
            self.scroll_target = self.target_layout.row_reveal(0);
            self.scroll.jump(self.scroll_target);
        }
        self.layout.status = self.readout == Readout::Failed;
        self.target_layout.status = self.layout.status;
        self.layout.empty = self.readout == Readout::Empty;
        self.target_layout.empty = self.layout.empty;
        self.pair.detail.set_geometry(self.layout, self.scroll.pos, self.target_layout, self.scroll_target);
        self.pair.master.set_geometry(self.layout.alpha_y(self.scroll.pos), self.target_layout.alpha_y(self.scroll_target),
            self.layout.grid_head && self.readout != Readout::Empty);
    }

    /// The listing's grid is on the page: everywhere but the Genres tab's rows.
    fn grid_shown(&self) -> bool { self.genres != GenreView::Rows }

    /// Show `view`: the shelves are rebuilt for it (genre rows or the others) and the document
    /// re-laid. Leaving one genre clears its filter; the Genres tab lists the section's own type,
    /// the only listing a genre filters.
    fn set_genres<H: LibraryLike>(&mut self, view: GenreView, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        if self.genres == view { return; }
        let leaving = std::mem::replace(&mut self.genres, view);
        let edit = match (&leaving, &self.genres) {
            (GenreView::Drill(_), _) => Some(GridAction::Drill { id: None }),
            (_, GenreView::Drill(id)) => Some(GridAction::Drill { id: Some(id.clone()) }),
            (_, GenreView::Rows) if H::listing(cx).library_type() != crate::browse::LibraryType::Primary =>
                Some(GridAction::LibraryType(crate::browse::LibraryType::Primary)),
            _ => None,
        };
        if let (Some(action), Some(target)) = (edit, GridTarget::from_view(H::listing(cx))) {
            self.pending.request_grid(target, action);
            self.grid_fade.reload();
        }
        // Another document, not a move within this one: it opens at its head.
        self.shelf_publication = None;
        self.restore_scroll = None;
        self.scroll.jump(0.0);
        self.scroll_target = 0.0;
        self.sync(cx);
        fx.invalidate(Provenance::Input);
    }

    /// Carry the page's own seat onto a new head block: its first card or control, wherever the
    /// seat it leaves stood (the tabs are centred, so the nearest card would be mid-row).
    fn follow_head<H: LibraryLike>(&self, group: GroupId, fx: &mut Effects<'_, H>) {
        let first = if group == self.pair.groups_config().detail { self.pair.detail.elem_at(0) }
            else { self.row_elems(Some(group)).first().copied() };
        match first {
            Some(elem) => self.reseat(FocusTarget::Elem(self.key(elem)), fx),
            None => self.reseat(FocusTarget::ContainerGroup(group), fx),
        }
    }

    /// The document's top block's group: the strip's door into the page, BACK's seat.
    fn first_group(&self) -> GroupId { self.block_group(self.layout.first()) }

    /// The group the page seats its own focus in, and that a provisional seat follows.
    fn head_group(&self) -> GroupId { self.block_group(self.layout.seat()) }

    fn block_group(&self, block: Option<Block>) -> GroupId {
        match block {
            Some(Block::Shelf(index)) => self.shelves[index].entry_group(),
            Some(Block::Status) => STATUS_GROUP,
            Some(Block::Toolbar) => self.toolbar,
            Some(Block::Grid(_)) => self.pair.groups_config().detail,
            Some(Block::Tabs) => self.tabs,
            None => STRIP,
        }
    }

    fn current_viewport(&self) -> Option<LibraryViewport> {
        let shelves = if self.shelf_publication.is_some() {
            self.shelves.iter().map(|row| (row.id.clone(), row.motion.scroll_x())).collect()
        } else {
            self.viewports.iter().find(|view| Some(view.epoch) == self.epoch && Some(&view.section) == self.section.as_ref())
                .map(|view| view.shelves.clone()).unwrap_or_default()
        };
        Some(LibraryViewport { epoch: self.epoch?, section: self.section.clone()?, scroll: self.scroll.pos,
            shelves })
    }

    fn page_memory(&self) -> LibraryMemory {
        let mut memory = self.keys.remember(self.section.clone(), self.scroll.pos,
            self.shelves.iter().map(|row| (row.id.clone(), row.motion.scroll_x())).collect());
        memory.epoch = self.epoch;
        memory.query = self.query;
        memory.grid_reset_pending = self.grid_reset_pending;
        memory.genres = self.genres.clone();
        memory.viewports = self.viewports.clone();
        if let Some(viewport) = self.current_viewport() {
            memory.viewports.retain(|old| old.epoch != viewport.epoch || old.section != viewport.section);
            memory.viewports.push(viewport);
        }
        memory
    }

    fn reveal<H: LibraryLike>(&mut self, key: FocusKey<u32>, by: By, cx: &Cx<'_, H>) {
        self.relayout(Some(key));
        // A hover moves focus, never the page: the pointer scrolls by the screen's edges (and a
        // click by what it clicked, the `Input` arm).
        let want = if by == By::Pointer {
            None
        } else if let Some(index) = self.pair.detail.index_of(key.elem) {
            Some(self.target_layout.row_reveal(index / self.layout.cols()))
        } else if let Some(index) = self.shelves.iter().position(|row|
            row.elems.contains(&key.elem) || row.heading_elem() == Some(key.elem)) {
            Some(self.target_layout.shelf_reveal(index))
        } else if matches!(key.elem, SHUFFLE | TYPE | SORT | FILTER) || region_of_elem(key.elem) == Some(KeyRegion::Rail) {
            // The controls and the alphabet keep the whole head in view while the document's
            // top has room for it: they lead the page's first block.
            let top = if self.shelves.is_empty() { 0.0 } else { self.target_layout.grid_block_top() };
            Some(top.clamp(0.0, self.target_layout.max_scroll()))
        } else if key.elem == RETRY || self.tab_of(key.elem).is_some() {
            Some(0.0)
        } else { None };
        if let Some(want) = want {
            self.scroll_target = want;
            if by == By::Restore {
                if let Some(saved) = self.restore_scroll.take() {
                    self.scroll_target = saved.clamp(0.0, self.target_layout.max_scroll());
                }
                self.scroll.jump(self.scroll_target);
            }
        } else {
            self.scroll_target = self.scroll_target.clamp(0.0, self.target_layout.max_scroll());
        }
        self.relayout(Some(key));
        self.pair.master.refresh(cx, &mut self.keys);
    }

    fn follow<H: LibraryLike>(&mut self, elem: u32, fx: &mut Effects<'_, H>) {
        let Some(index) = self.pair.master.start_for_elem(elem) else { return };
        let Some(target) = self.pair.detail.elem_at(index) else { return };
        fx.remember(self.pair.groups_config().detail, target);
        self.scroll_target = self.target_layout.with_grid_focus(None).row_reveal(index / self.layout.cols());
        fx.invalidate(Provenance::Input);
    }

    pub(crate) fn focused_item<'a, H: LibraryLike>(&self, focus: Option<FocusKey<u32>>, cx: &Cx<'a, H>) -> Option<&'a crate::catalog_fetch::PmsMovie> {
        let key = focus.filter(|key| key.entry == self.entry)?;
        if let Some(index) = self.pair.detail.index_of(key.elem) { return H::listing(cx).item(index); }
        let (row, col) = self.shelves.iter().enumerate().find_map(|(row, shelf)|
            shelf.elems.iter().position(|elem| *elem == key.elem).map(|col| (row, col)))?;
        self.hub(row, cx)?.items.get(col)
    }

    /// The published hub page row `row` draws.
    fn hub<'a, H: LibraryLike>(&self, row: usize, cx: &Cx<'a, H>) -> Option<&'a crate::browse::section_hubs::Shelf> {
        H::section_hubs(cx).shelves().get(self.shelves.get(row)?.hub)
    }

    pub(crate) fn grid_position(&self, focus: Option<FocusKey<u32>>) -> Option<(usize, usize)> {
        let key = focus.filter(|key| key.entry == self.entry)?;
        self.pair.detail.index_of(key.elem).map(|index| (index / self.layout.cols(), index % self.layout.cols()))
    }

    /// The grid's shape as the scenes that drive it see it: `(rows, rows_per_screen)`, once the
    /// listing has answered with at least one row.
    #[cfg(feature = "devtriggers")]
    pub(crate) fn grid_extent(&self) -> Option<(usize, usize)> {
        (self.layout.rows > 0).then(|| (self.layout.rows, self.layout.rows_per_screen()))
    }

    /// The focused hub-shelf card, `(shelf, col)`, if focus is on a shelf above the grid.
    pub(crate) fn shelf_position(&self, focus: Option<FocusKey<u32>>) -> Option<(usize, usize)> {
        let key = focus.filter(|key| key.entry == self.entry)?;
        self.shelves.iter().enumerate()
            .find_map(|(row, shelf)| shelf.elems.iter().position(|elem| *elem == key.elem).map(|col| (row, col)))
    }

    pub(crate) fn probe_viewport(&self, focus: Option<FocusKey<u32>>) -> (&'static str, i32, i32, f32, f32) {
        if let Some((row, col)) = self.grid_position(focus) { return ("grid", row as i32, col as i32, 0.0, self.scroll.pos); }
        let elem = focus.filter(|key| key.entry == self.entry).map(|key| key.elem);
        for (row, shelf) in self.shelves.iter().enumerate() {
            if let Some(col) = elem.and_then(|elem| shelf.elems.iter().position(|key| *key == elem)) {
                return ("shelf", row as i32, col as i32, shelf.motion.scroll_x(), self.scroll.pos);
            }
        }
        let region = match elem {
            Some(SHUFFLE | TYPE | SORT | FILTER) => "toolbar", Some(RETRY) => "status",
            Some(elem) if self.tab_of(elem).is_some() => "tabs",
            Some(elem) if region_of_elem(elem) == Some(KeyRegion::Rail) => "rail",
            Some(elem) if elem >= crate::ui::dispatch::STRIP_BASE => "strip",
            Some(_) => "library", None => "none",
        };
        (region, -1, -1, 0.0, self.scroll.pos)
    }

    fn from_deck<H: LibraryLike>(&self, elem: u32, cx: &Cx<'_, H>) -> bool {
        self.shelves.iter().position(|row| row.elems.contains(&elem))
            .and_then(|row| self.hub(row, cx)).is_some_and(|row| row.is_continue)
    }

    fn activate<H: LibraryLike>(&mut self, elem: u32, held: bool, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> Handled {
        // Every activation path (OK, a click, a hold, a scripted menu) is a choice made AT the seat,
        // and none of them moves focus, so no `FocusMoved` will release it.
        self.provisional = None;
        self.placed.retain(|(_, placed)| *placed != elem);
        if [TYPE, SORT, FILTER].contains(&elem) {
            if let (Some(target), Some(placed)) = (self.address(cx), <Self as Focusable<H>>::place(self, &elem, cx, At::SpringTarget)) {
                let r = placed.rest_rect;
                fx.push(Fx::App(AppFx::Library(LibraryReq::Menu {
                    kind: if elem == SORT { crate::screens::registry::LibraryMenuKind::Sort }
                        else if elem == TYPE { crate::screens::registry::LibraryMenuKind::Type }
                        else { crate::screens::registry::LibraryMenuKind::Filter },
                    anchor: [r.x.to_bits(), r.y.to_bits(), r.w.to_bits(), r.h.to_bits()], target,
                })));
            }
            return Handled::Yes;
        }
        if let Some(tab) = self.tab_of(elem) {
            match tab {
                toolbar::Tab::Genres => self.set_genres(GenreView::Rows, cx, fx),
                toolbar::Tab::Type(kind) => {
                    self.set_genres(GenreView::Off, cx, fx);
                    if self.active_tab(cx) != tab { self.command(LibraryCmd::SetType(kind), cx, fx); }
                }
            }
            return Handled::Yes;
        }
        if elem == SHUFFLE {
            self.shuffle(cx, fx);
            return Handled::Yes;
        }
        if region_of_elem(elem) == Some(KeyRegion::Rail) {
            // A letter jumps the grid to its first title and goes there (Litefin 6).
            self.follow(elem, fx);
            self.reseat(FocusTarget::ContainerGroup(self.pair.groups_config().detail), fx);
            return Handled::Yes;
        }
        if let Some(index) = self.shelves.iter().position(|row| row.heading_elem() == Some(elem)) {
            // OK on a linked heading opens what its shelf lists: a collection, or one genre's grid.
            if let Some(link) = self.hub(index, cx).and_then(|shelf| shelf.link.as_ref()) {
                fx.push(Fx::App(AppFx::Content(crate::screens::registry::ContentReq::Push(
                    crate::screens::registry::ContentArg::Collection(link.clone())))));
            } else if let Some(genre) = self.hub(index, cx).and_then(|shelf| shelf.genre.clone()) {
                self.set_genres(GenreView::Drill(genre), cx, fx);
                self.reseat(FocusTarget::ContainerGroup(self.pair.groups_config().detail), fx);
            }
            return Handled::Yes;
        }
        if let Some(item) = self.focused_item(Some(self.key(elem)), cx).filter(|item| !item.rk.is_empty()) {
            let from_deck = self.from_deck(elem, cx);
            let req = if held {
                LibraryReq::ItemMenu { sid: item.sid, rk: item.rk.clone(), from_deck }
            } else if from_deck {
                LibraryReq::Play { sid: item.sid, rk: item.rk.clone(), resume_ns: crate::metadata::resume_ns(item.resume_ms, item.dur_ns / 1_000_000) }
            } else { LibraryReq::Detail { sid: item.sid, rk: item.rk.clone() } };
            fx.push(Fx::App(AppFx::Library(req)));
            return Handled::Yes;
        }
        if elem == RETRY && self.readout == Readout::Failed
            && super::plaintext_question::asks(self.plaintext.verdict())
        {
            // The source's server is on this network and only answers without encryption:
            // *Connect* asks the shared question rather than retrying what cannot succeed.
            let sid = H::directory(cx).source().map(|(sid, _)| *sid);
            if let Some(machine) = self.plaintext.verdict().map(|v| v.machine_id.clone()) {
                self.plaintext_alert.open(&machine, sid, MachineId::Instance(self.instance), fx);
            }
            return Handled::Yes;
        }
        if elem == RETRY {
            if let Some(target) = self.address(cx) {
                self.store(target, LibraryWork::Retry, fx);
            } else {
                let directory = H::directory(cx);
                if let (Some(epoch), Some((sid, _))) = (directory.epoch(), directory.source()) {
                    fx.push(Fx::App(AppFx::Store(StoreId::Browse,
                        StoreCmd::Browse(BrowseCmd::RetrySource { epoch, sid: *sid }))));
                }
            }
            return Handled::Yes;
        }
        Handled::No
    }

    /// Shuffle, as Litefin's (`LibraryPage.js:_handleShuffle`): the page of a random title of the
    /// library on screen, a series in a TV library, picked by the server over the whole library
    /// (the grid holds only a window of it).
    fn shuffle<H: LibraryLike>(&self, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        let Some(id) = H::listing(cx).id() else { return };
        fx.push(Fx::App(AppFx::Library(LibraryReq::Shuffle { sid: id.sid, section: id.section })));
    }

    fn flush<H: LibraryLike>(&mut self, fx: &mut Effects<'_, H>) {
        if let Some((target, query)) = self.pending.flush().map(|(target, action)| (SectionAddress {
            epoch: target.epoch, sid: target.sid, section: target.section,
        }, match action {
            GridAction::Sort { key, desc } => crate::stores::browse::QueryEdit::Sort { key, desc },
            GridAction::Unwatched { desired } => crate::stores::browse::QueryEdit::Unwatched(desired),
            GridAction::Genre { id } => crate::stores::browse::QueryEdit::Genre(id),
            GridAction::Drill { id } => crate::stores::browse::QueryEdit::Drill(id),
            GridAction::LibraryType(kind) => crate::stores::browse::QueryEdit::LibraryType(kind),
        })) {
            self.store(target, LibraryWork::Commit { select: false, choice: false, query: Some(query) }, fx);
        }
    }

    fn command<H: LibraryLike>(&mut self, command: LibraryCmd, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> Handled {
        match command {
            LibraryCmd::Enter(section) => {
                // One genre's grid is a view of the library it was opened in: another library opens
                // on its own listing, and the one left behind is not left filtered to the genre.
                if matches!(self.genres, GenreView::Drill(_)) {
                    if let Some(target) = GridTarget::from_view(H::listing(cx)) {
                        self.pending.request_grid(target, GridAction::Drill { id: None });
                    }
                    self.genres = GenreView::Off;
                }
                self.save_cursor(cx, fx);
                self.flush(fx);
                self.wanted = Some(section);
                self.initial = true;
                self.page_fade.mount();
                self.sync(cx);
            }
            LibraryCmd::FocusGrid { row, col } => {
                if col >= self.layout.cols() { return Handled::No; }
                let Some(index) = row.checked_mul(self.layout.cols()).and_then(|i| i.checked_add(col)) else { return Handled::No };
                let Some(elem) = self.pair.detail.elem_at(index) else { return Handled::No };
                self.initial = false; // An explicit owned command supersedes the pending boot seat.
                self.provisional = None;
                self.reseat(FocusTarget::Elem(self.key(elem)), fx);
            }
            LibraryCmd::FocusShelf { shelf, col } => {
                let Some(&elem) = self.shelves.get(shelf).and_then(|s| s.elems.get(col)) else { return Handled::No };
                self.initial = false; // As FocusGrid: an explicit owned command supersedes the boot seat.
                self.provisional = None;
                self.reseat(FocusTarget::Elem(self.key(elem)), fx);
            }
            LibraryCmd::ItemMenu => return cx.focus.current.map_or(Handled::No, |key| self.activate(key.elem, true, cx, fx)),
            // The TYPE menu's own edit, delivered the way its row delivers it.
            LibraryCmd::SetType(kind) => {
                let Some(target) = GridTarget::from_view(H::listing(cx)) else { return Handled::No };
                let address = SectionAddress { epoch: target.epoch, sid: target.sid, section: target.section };
                self.step(&ScreenEvent::App(AppMsg::LibraryEdit {
                    target: address, edit: crate::stores::browse::QueryEdit::LibraryType(kind),
                }), cx, fx);
                return Handled::Yes;
            }
            LibraryCmd::OpenMenu(kind) => {
                let elem = match kind {
                    crate::screens::registry::LibraryMenuKind::Type => TYPE,
                    crate::screens::registry::LibraryMenuKind::Sort => SORT,
                    crate::screens::registry::LibraryMenuKind::Filter => FILTER,
                    _ => return Handled::No,
                };
                return self.activate(elem, false, cx, fx);
            }
            LibraryCmd::Page(direction) => {
                let Some((row, col)) = self.grid_position(cx.focus.current) else { return Handled::No };
                let rows = self.pair.detail.elems.len().div_ceil(self.layout.cols());
                let next = row.saturating_add_signed(direction.signum() as isize * 2).min(rows.saturating_sub(1));
                if let Some(elem) = self.pair.detail.elem_at((next * self.layout.cols() + col).min(self.pair.detail.elems.len().saturating_sub(1))) {
                    self.reseat(FocusTarget::Elem(self.key(elem)), fx);
                }
            }
            LibraryCmd::Sweep => {
                let current = cx.focus.current.filter(|key| key.entry == self.entry);
                let group = current.and_then(|key| <Self as Focusable<H>>::group_of(self, &key.elem, cx));
                if self.sweep_down && self.grid_position(current).is_some_and(|(row, _)|
                    row + 1 >= self.pair.detail.elems.len().div_ceil(self.layout.cols())) {
                    self.sweep_down = false;
                } else if !self.sweep_down && group == Some(self.first_group()) {
                    self.sweep_down = true;
                }
                fx.push(Fx::Deliver(MachineId::Instance(self.instance), Delivery::Screen(
                    ScreenEvent::Input(nj_machine::machine::InputEvent {
                        at: cx.tick, source: nj_machine::machine::Source::Script,
                        kind: InputKind::Key { key: if self.sweep_down { Key::Down } else { Key::Up },
                            sym: 0, wcode: 0, edge: Edge::Down, at_edge: false },
                    }))));
            }
            LibraryCmd::SwitchStep(step) => {
                let first_of = |kind| { let directory = H::directory(cx);
                    (0..directory.sections().len()).find(|&i| directory.sections()[i].kind == kind)
                        .and_then(|i| section_identity(directory, i)) };
                match step % 14 {
                    0 if first_of(SecKind::Show).is_some() =>
                        return self.command(LibraryCmd::Enter(first_of(SecKind::Show).unwrap()), cx, fx),
                    1 => if let Some(movies) = first_of(SecKind::Movie) {
                        return self.command(LibraryCmd::Enter(movies), cx, fx);
                    },
                    2 => return self.activate(SORT, false, cx, fx),
                    5 | 6 => {
                        if let Some(target) = GridTarget::from_view(H::listing(cx)) {
                            let current = self.pending.grid().and_then(|(pending, action)| {
                                if pending != &target { return None; }
                                match action { GridAction::Unwatched { desired } => Some(*desired), _ => None }
                            }).unwrap_or(H::listing(cx).unwatched());
                            let address = SectionAddress { epoch: target.epoch, sid: target.sid, section: target.section };
                            return self.step(&ScreenEvent::App(AppMsg::LibraryEdit {
                                target: address, edit: crate::stores::browse::QueryEdit::Unwatched(!current),
                            }), cx, fx);
                        }
                    }
                    7 => return self.activate(FILTER, false, cx, fx),
                    12 | 13 => {
                        let letter = if step % 14 == 12 { self.pair.master.elems.last() } else { self.pair.master.elems.first() };
                        if let Some(elem) = letter { self.follow(*elem, fx); }
                    }
                    // Menu steps are delivered to their actual input-owning entry by the bridge.
                    _ => {}
                }
            }
        }
        Handled::Yes
    }
}

impl<H: LibraryLike> Machine<H> for LibraryScreen {
    type Ev = ScreenEvent<H>;
    fn step(&mut self, ev: &Self::Ev, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> Handled {
        use super::plaintext_question::AlertStep;
        match self.plaintext_alert.step(ev, cx) {
            AlertStep::Pass => {}
            AlertStep::Done(handled) => return handled,
            AlertStep::Answer(cmd) => {
                if let Some(cmd) = cmd { fx.push(Fx::App(AppFx::Session(cmd))); }
                self.reseat(FocusTarget::ContainerGroup(STATUS_GROUP), fx);
                return Handled::Yes;
            }
        }
        match ev {
            ScreenEvent::Mount => {
                self.sync(cx);
                // A genre's grid left by a page that is gone (its memory with it, so this page did
                // not open it and cannot show it): the library opens on its listing.
                if H::listing(cx).drilled() && !matches!(self.genres, GenreView::Drill(_)) {
                    if let Some(target) = self.address(cx) {
                        self.store(target, LibraryWork::Commit { select: false, choice: false,
                            query: Some(crate::stores::browse::QueryEdit::Drill(None)) }, fx);
                    }
                }
            }
            ScreenEvent::StoreChanged(..) => { self.sync(cx); }
            ScreenEvent::RestoreMemory(PageMemory::Library(memory)) => { self.restore(memory); self.sync(cx); }
            ScreenEvent::Enter(_) => { self.live = true; self.sync(cx); }
            ScreenEvent::Cover => { self.live = false; }
            ScreenEvent::Uncover => { self.live = true; self.sync(cx); }
            ScreenEvent::WillLeave(_) => { self.save_cursor(cx, fx); self.flush(fx); }
            ScreenEvent::App(AppMsg::Library(command)) => return self.command(command.clone(), cx, fx),
            ScreenEvent::App(AppMsg::LibraryEdit { target, edit }) => {
                use crate::stores::browse::QueryEdit;
                if Some(target.epoch) != self.epoch { return Handled::No; }
                let target = *target;
                let action = match edit {
                    QueryEdit::Sort { key, desc } => GridAction::Sort { key: key.clone(), desc: *desc },
                    QueryEdit::Unwatched(desired) => GridAction::Unwatched { desired: *desired },
                    QueryEdit::Genre(id) => GridAction::Genre { id: id.clone() },
                    QueryEdit::Drill(id) => GridAction::Drill { id: id.clone() },
                    QueryEdit::LibraryType(kind) => GridAction::LibraryType(*kind),
                };
                if matches!(&action, GridAction::Unwatched { desired } if *desired == H::listing(cx).unwatched())
                    && self.address(cx) == Some(target)
                    && self.grid_fade.cancel() {
                    self.pending.cancel();
                    return Handled::Yes;
                }
                self.pending.request_grid(GridTarget { epoch: target.epoch, sid: target.sid, section: target.section, query: self.query.unwrap_or(0) }, action);
                self.grid_fade.reload();
            }
            ScreenEvent::FocusMoved { to, by, .. } => {
                let grid_focus = Some(*to).filter(|key| key.entry == self.entry)
                    .and_then(|key| self.pair.detail.index_of(key.elem));
                self.pair.detail.focus_row(grid_focus.map(|index| index / self.layout.cols()), matches!(by, By::Dir | By::Pointer));
                // A key's move along a shelf brings it back to its focus; a hover does not.
                if *by != By::Pointer {
                    for shelf in &mut self.shelves {
                        if shelf.elems.contains(&to.elem) { shelf.motion.unfree(); }
                    }
                }
                if matches!(by, By::Dir | By::Pointer) {
                    self.initial = false;
                    self.provisional = None;
                    // A deliberate move is the only focus change the poster wall animates: the
                    // tile grows from rest over the frames after this, as a shelf's does. A
                    // restore or a reconcile is left to `GridPart::tick`, which adopts its cell
                    // at full scale so a page returning from a Detail push lands as it was left.
                    if let Some(index) = Some(*to).filter(|key| key.entry == self.entry)
                        .and_then(|key| self.pair.detail.index_of(key.elem)) {
                        self.pair.detail.pop_from_rest(index);
                    }
                }
                let to_group = <Self as Focusable<H>>::group_of(self, &to.elem, cx);
                // Any seat off the head that was not a user move (a restore, a reconcile, a seat by
                // element) is somebody else's choice too, and the head seat no longer describes it.
                if to_group != self.provisional { self.provisional = None; }
                // What the engine now remembers for this group is the page's own seat only if this
                // move IS that seat; any other move into the group was somebody's choice.
                if let Some(group) = to_group {
                    self.placed.retain(|(placed, _)| *placed != group);
                    if self.provisional == Some(group) { self.placed.push((group, to.elem)); }
                }
                // Entering or walking the alphabet bar jumps nothing (it follows on activation
                // only, `Follow::OnCommit`): the remembered grid item survives the visit.
                self.reveal(*to, *by, cx);
            }
            ScreenEvent::Tick(tick) => {
                self.sync(cx);
                if self.watch_readout(cx) {
                    fx.invalidate(Provenance::Landing(fx.from()));
                }
                self.plaintext_alert.update(tick.dt());
                if self.grid_reset_pending {
                    if let Some(elem) = self.pair.detail.elem_at(0) {
                        fx.remember(self.pair.groups_config().detail, elem);
                        self.grid_reset_pending = false;
                        if cx.focus.current.is_some_and(|key| key.entry == self.entry && region_of_elem(key.elem) == Some(KeyRegion::Grid)) {
                            self.reseat(FocusTarget::ContainerGroup(self.pair.groups_config().detail), fx);
                        }
                    } else if self.readout == Readout::Empty {
                        self.grid_reset_pending = false;
                    }
                }
                if let Some(wanted) = &self.wanted {
                    let directory = H::directory(cx);
                    if let (Some(_), Some(epoch)) = (wanted_index(directory, wanted), directory.epoch()) {
                        self.store(SectionAddress { epoch, sid: wanted.sid, section: wanted.key },
                            LibraryWork::Commit { select: true, choice: false, query: None }, fx);
                    } else {
                        self.readout = readout(directory.discovery(), 0, directory.discovery(), -1);
                    }
                }
                // Compact menus leave their host's transaction clock live while owning input.
                let waiting_for_kind = self.wanted.as_ref().is_some_and(|wanted| {
                    let directory = H::directory(cx);
                    wanted_index(directory, wanted).is_some() || directory.discovery() == SecFetch::Loading
                });
                let ready = self.readout != Readout::Loading && !waiting_for_kind;
                // A section's reveal also waits for the document ABOVE its grid. Its shelves are
                // a separate fetch that sets the grid's absolute offset, so revealing on the
                // listing alone drew the grid at the top and then dropped it by the shelves'
                // height when they landed (owner report item 9, the section half). A grid-only
                // swap keeps its own `ready`: its shelves are already the document it fades inside.
                let shelves = shelves_settled(H::section_hubs(cx));
                let page_commit = self.page_fade.tick(tick.dt(), ready && shelves);
                let grid_commit = self.grid_fade.tick(tick.dt(), ready);
                if page_commit || grid_commit { self.flush(fx); }
                if self.live {
                    let dt = tick.dt();
                    let focused = cx.focus.current;
                    self.head_pop.step(focused.filter(|key| key.entry == self.entry)
                        .and_then(|key| self.head_pop_slot(key.elem)), dt);
                    for row in &mut self.shelves {
                        let col = focused.and_then(|key| row.elems.iter().position(|elem| *elem == key.elem));
                        row.motion.update(row.elems.len(), col, shelf_style(row.landscape), dt);
                    }
                    self.scroll.step(self.scroll_target, K_SCROLL, dt);
                    let grid_focus = focused.filter(|key| key.entry == self.entry)
                        .and_then(|key| self.pair.detail.index_of(key.elem));
                    self.pair.detail.tick(grid_focus, dt);
                    self.relayout(focused);
                    // A first seat that applies a bookmarked scroll clamps it to the document, so
                    // behind a section reveal it waits for the same shelves the reveal does:
                    // seated on the shelfless interim, a bookmark deep in the grid landed short of
                    // where the reader left. A head seat needs no wait — `provisional` follows the
                    // head when the shelves land — and a page already on screen seats as it did.
                    let settled = shelves || !self.page_fade.is_swapping() || self.restore_scroll.is_none();
                    if self.initial && settled && self.layout.seat().is_some() && self.seed_cursor(cx, fx) {
                        self.initial = false;
                        let group = match self.layout.seat_for_scroll(self.scroll.pos, self.layout.visible_rows(self.scroll.pos).0) {
                            Some(Block::Grid(_)) => self.pair.groups_config().detail,
                            Some(Block::Shelf(index)) => self.shelves[index].group,
                            _ => self.head_group(),
                        };
                        // Only the page's OWN head seat is provisional. Any restore is the reader's:
                        // a restored scroll (a saved viewport or bookmark, at the head or not) names
                        // the block they left, and a remembered cursor in the seated group is where
                        // `Seat::Remembered` puts them back — neither may follow a landing.
                        let restoring = self.restore_scroll.is_some() || cx.focus.remembered(group)
                            .is_some_and(|elem| !self.placed.contains(&(group, elem)));
                        self.provisional = (group == self.head_group() && !restoring).then_some(group);
                        self.reseat(FocusTarget::ContainerGroup(group), fx);
                    } else if let Some(seated) = self.provisional.filter(|_| self.layout.seat().is_some()) {
                        // A landing moved the head out from under the page's own seat (field report
                        // on a788b27a: the shelves committed above a seat on the grid's heading, and
                        // the first DOWN went on into the grid). Follow it, as a first paint would.
                        let head = self.head_group();
                        let at = focused.filter(|key| key.entry == self.entry)
                            .and_then(|key| <Self as Focusable<H>>::group_of(self, &key.elem, cx));
                        // Focus anywhere but the seat is somebody's move, reported or not yet (the
                        // engine moves before the page hears `FocusMoved`): not the page's to take.
                        if at != Some(seated) { self.provisional = None; }
                        else if head != seated { self.provisional = Some(head); self.follow_head(head, fx); }
                    }
                }
                // The dispatcher ticks the top page beneath a compact menu, but not buried pages.
                // Cover pauses its control motion; it must not stop the query just committed above.
                if let Some(target) = self.address(cx).filter(|_| self.wanted.is_none()) {
                    let (lo, hi) = self.layout.visible_rows(self.scroll.pos);
                    self.store(target, LibraryWork::Want { lo: lo.saturating_sub(1) * self.layout.cols(), hi: (hi + 1) * self.layout.cols() }, fx);
                    self.store(target, LibraryWork::Letters, fx);
                    // A genre's grid applies the section's genre table (`browse::set_genre_by_id`).
                    if self.genres != GenreView::Off { self.store(target, LibraryWork::Genres, fx); }
                    let at_head = self.scroll.pos.abs() < 1.0 && self.scroll.vel.abs() < 1.0 && self.scroll_target.abs() < 0.5;
                    fx.push(Fx::App(AppFx::Library(LibraryReq::PublishShelves {
                        target, at_head, hidden_page: self.page_fade.is_swapping() && self.page_fade.alpha() <= 0.01,
                    })));
                }
                fx.push(Fx::App(AppFx::StoreWork(StoreWork::Browse)));
            }
            ScreenEvent::Activate(elem) => return self.activate(*elem, false, cx, fx),
            ScreenEvent::PressHold(_) => {
                return self.command(LibraryCmd::ItemMenu, cx, fx);
            }
            ScreenEvent::PressCommit(_) => {
                return cx.focus.current.map_or(Handled::No, |key| self.activate(key.elem, false, cx, fx));
            }
            ScreenEvent::Input(input) => {
                // Any key, pointer motion, click, drag or wheel is the reader acting at the seat.
                // It must claim it HERE: OK-down arms a delayed press the dispatcher only commits
                // if focus is still on its key, and this page's next Tick may run before that
                // commit; hovering the already-focused control moves nothing, so no `FocusMoved`.
                if matches!(input.kind, InputKind::Key { edge: Edge::Down, .. } | InputKind::Pointer { .. }
                    | InputKind::Click { .. } | InputKind::Drag { .. } | InputKind::Wheel { .. }) {
                    self.provisional = None;
                }
                // A pointer on a control confirms it the way activating it does.
                if let InputKind::Pointer { hit: Some(hit), .. } | InputKind::Click { hit: Some(hit), .. }
                    | InputKind::Drag { hit: Some(hit), .. } = input.kind {
                    self.placed.retain(|(_, placed)| *placed != hit);
                }
                // OK on a letter is its activation, at once: no press dip on a 38px letter.
                if let (InputKind::Key { key: Key::Ok, edge: Edge::Down, .. }, Some(key)) = (&input.kind, cx.focus.current) {
                    if key.entry == self.entry && region_of_elem(key.elem) == Some(KeyRegion::Rail) {
                        return self.activate(key.elem, false, cx, fx);
                    }
                }
                // A click brings the page to the card it clicked, the way a key's move does.
                if let InputKind::Click { hit: Some(hit), .. } = input.kind {
                    for shelf in &mut self.shelves {
                        if shelf.elems.contains(&hit) { shelf.motion.unfree(); }
                    }
                    if let Some(index) = self.pair.detail.index_of(hit) {
                        self.scroll_target = self.target_layout.row_reveal(index / self.layout.cols());
                        fx.invalidate(Provenance::Input);
                    }
                }
                // The pointer's sideways glide moves the shelf under it, left where it put it
                // until a key moves along it or a click lands on one of its cards.
                if let InputKind::Pan { dx, y, .. } = input.kind {
                    if dx.is_finite() {
                        let under = (0..self.shelves.len()).find(|&index| {
                            let top = self.layout.shelf_y(index, self.scroll.pos);
                            y >= top && y < top + self.layout.shelf_pitch(index)
                        });
                        if let Some(shelf) = under.and_then(|index| self.shelves.get_mut(index)) {
                            let style = shelf_style(shelf.landscape);
                            shelf.motion.glide(dx, shelf.elems.len(), style);
                            fx.invalidate(Provenance::Input);
                        }
                    }
                    return Handled::Yes;
                }
                if let InputKind::Wheel { dy } = input.kind {
                    self.scroll_target = (self.scroll_target - dy * self.layout.grid_pitch())
                        .clamp(0.0, self.target_layout.max_scroll());
                    fx.invalidate(Provenance::Input);
                    return Handled::Yes;
                }
                if let InputKind::Key { sym, wcode, edge: Edge::Down, .. } = input.kind {
                    if let Some(dir) = crate::ui::consts::page_dir(sym, wcode) {
                        return self.command(LibraryCmd::Page(dir), cx, fx);
                    }
                }
                if let InputKind::Key { key: Key::Back, edge: Edge::Down, .. } = input.kind {
                    // BACK inside one genre returns to its row on the Genres tab.
                    if let GenreView::Drill(genre) = self.genres.clone() {
                        self.set_genres(GenreView::Rows, cx, fx);
                        let heading = self.shelves.iter().enumerate()
                            .find(|(row, _)| self.hub(*row, cx).is_some_and(|shelf| shelf.genre.as_deref() == Some(genre.as_str())))
                            .and_then(|(_, row)| row.heading_elem());
                        match heading {
                            Some(elem) => self.reseat(FocusTarget::Elem(self.key(elem)), fx),
                            None => self.reseat(FocusTarget::ContainerGroup(self.tabs), fx),
                        }
                        return Handled::Yes;
                    }
                    let cancelled = self.page_fade.cancel() | self.grid_fade.cancel();
                    if cancelled { self.pending.cancel(); return Handled::Yes; }
                    if cx.focus.current.is_some_and(|key| key.entry == self.entry && region_of_elem(key.elem) == Some(KeyRegion::Rail)) {
                        self.reseat(FocusTarget::ContainerGroup(self.pair.groups_config().detail), fx);
                        return Handled::Yes;
                    }
                    if self.scroll_target > 0.5 {
                        self.scroll_target = 0.0;
                        self.reseat(FocusTarget::ContainerGroup(self.first_group()), fx);
                    } else { fx.push(Fx::App(AppFx::Library(LibraryReq::BackToHome))); }
                    return Handled::Yes;
                }
            }
            _ => {}
        }
        Handled::No
    }
}

impl<H: LibraryLike> Focusable<H> for LibraryScreen {
    fn groups(&self, cx: &Cx<'_, H>, out: &mut Vec<GroupSpec>) {
        if self.plaintext_alert.groups(out) { return; }
        if self.layout.tabs() {
            if let Some((rect, _)) = self.tab_rect(TAB_BASE, cx, At::SpringTarget) {
                out.push(row_group(self.tabs, self.tabs().len(), rect, ElemKind::Control));
            }
        }
        for (index, row) in self.shelves.iter().enumerate() {
            let mut shelf = row_group(row.group, row.elems.len(), Rect::new(LEFT, self.target_layout.shelf_y(index, self.scroll_target) + CARD_DY, layout::RIGHT - LEFT, row_style(row).h), ElemKind::Card);
            if let Some((group, elem)) = row.heading {
                if let Some(rect) = self.heading_rect(index, cx, At::SpringTarget) {
                    out.push(crate::ui::linked_heading::group_spec(group, rect));
                }
                // DOWN from the heading returns to the card this shelf remembered.
                shelf.seat = crate::ui::linked_heading::shelf_seat(&cx.focus, elem, row.group, shelf.seat);
            }
            out.push(shelf);
        }
        if self.layout.grid_head {
            out.push(row_group(self.toolbar, self.toolbar_elems().len(), self.toolbar_chip_rect(self.toolbar_elems()[0], cx, At::SpringTarget), ElemKind::Control));
        }
        if self.readout == Readout::Failed {
            if let Some(rect) = self.status_rect(cx) {
                out.push(GroupSpec { edge: [EdgeRule::Geometric, EdgeRule::Stop, EdgeRule::Stop, EdgeRule::Stop],
                    ..row_group(STATUS_GROUP, 1, rect, ElemKind::Control) });
            }
        }
        if !self.grid_shown() { return; }
        if !self.pair.detail.elems.is_empty() && self.pair.master.eligible(cx) {
            self.pair.groups(cx, out);
        } else {
            self.pair.detail.groups(cx, out);
        }
    }
    fn group_of(&self, elem: &u32, cx: &Cx<'_, H>) -> Option<GroupId> {
        if let Some(answer) = self.plaintext_alert.group_of(*elem) { return answer; }
        if self.layout.tabs() && self.tab_of(*elem).is_some() { return Some(self.tabs); }
        if let Some(row) = self.shelves.iter().find(|row| row.elems.contains(elem)) { return Some(row.group); }
        if let Some((group, _)) = self.shelves.iter().find_map(|row| row.heading.filter(|(_, e)| e == elem)) { return Some(group); }
        if self.layout.grid_head && self.toolbar_elems().contains(elem) { return Some(self.toolbar); }
        if self.readout == Readout::Failed && *elem == RETRY { return Some(STATUS_GROUP); }
        if !self.grid_shown() { return None; }
        self.pair.group_of(elem, cx)
    }
    fn neighbour(&self, key: FocusKey<u32>, dir: Dir, cx: &Cx<'_, H>) -> Step<u32> {
        if let Some(step) = self.plaintext_alert.neighbour(key, dir) { return step; }
        if matches!(region_of_elem(key.elem), Some(KeyRegion::Grid | KeyRegion::Rail)) {
            return self.pair.neighbour(key, dir, cx);
        }
        let elems = self.row_elems(self.group_of(&key.elem, cx));
        let Some(index) = elems.iter().position(|elem| *elem == key.elem) else { return Step::Edge };
        let next = match dir { Dir::Left => index.checked_sub(1), Dir::Right => Some(index + 1), _ => None };
        next.and_then(|i| elems.get(i).copied()).map_or(Step::Edge, |elem| Step::Move(self.key(elem)))
    }
    fn place(&self, elem: &u32, cx: &Cx<'_, H>, at: At) -> Option<Placed> {
        if let Some(placed) = self.plaintext_alert.place(*elem) { return placed; }
        if matches!(region_of_elem(*elem), Some(KeyRegion::Grid | KeyRegion::Rail)) {
            return self.grid_shown().then(|| self.pair.place(elem, cx, at)).flatten();
        }
        let mut rest_rect = None;
        let rect = if let Some((rect, scale)) = self.layout.tabs().then(|| self.tab_rect(*elem, cx, at)).flatten() {
            rest_rect = Some(rect);
            rect.scaled(scale)
        } else if let Some((row, col)) = self.shelves.iter().enumerate().find_map(|(row, shelf)|
            shelf.elems.iter().position(|key| key == elem).map(|col| (row, col))) {
            if at == At::Drawn { rest_rect = Some(self.shelf_rect(row, col)); }
            self.shelf_rect_at(row, col, cx, at)
        } else if let Some(index) = self.shelves.iter().position(|row| row.heading_elem() == Some(*elem)) {
            self.heading_rect(index, cx, at)?
        } else if let Some((rect, scale)) = self.layout.grid_head.then(|| self.control_rect(*elem, cx, at)).flatten() {
            rest_rect = Some(rect);
            rect.scaled(scale)
        }
        else if *elem == RETRY && self.readout == Readout::Failed { self.status_rect(cx)? }
        else { return None };
        Some(Placed { rect, rest_rect: rest_rect.unwrap_or(rect), clip: Rect::new(0.0, 0.0, SCR_W, SCR_H), index: None })
    }
    fn reconcile(&self, want: FocusKey<u32>, cx: &Cx<'_, H>) -> FocusKey<u32> {
        if let Some(key) = self.plaintext_alert.reconcile(want) { return key; }
        // A reloading grid is not a missing grid (one genre's grid requeries under the focus that
        // entered it): focus waits on its cell while the fade holds the grid dark for its answer,
        // and the reset seats the new first title.
        if self.grid_shown() && self.grid_fade.is_swapping()
            && region_of_elem(want.elem) == Some(KeyRegion::Grid) { return want; }
        if matches!(self.keys.region(want.elem).or_else(|| region_of_elem(want.elem)), Some(KeyRegion::Grid | KeyRegion::Rail)) {
            // Ownership is a typed identity query, even when group_of no longer places the key.
            let next = self.pair.reconcile(want, cx);
            if self.pair.place(&next.elem, cx, At::SpringTarget).is_some() { return next; }
        }
        if self.place(&want.elem, cx, At::SpringTarget).is_some() { return want; }
        if let Some((group, index)) = self.keys.last_place(want.elem) {
            let elems = self.row_elems(Some(group));
            if let Some(elem) = elems.get(index.min(elems.len().saturating_sub(1))) { return self.key(*elem); }
        }
        let group = self.first_group();
        self.row_elems(Some(group)).first().copied().map(|elem| self.key(elem))
            .unwrap_or_else(|| self.key(crate::ui::dispatch::STRIP_BASE))
    }
    fn seat(&self, group: GroupId, from: Placed, cx: &Cx<'_, H>) -> FocusKey<u32> {
        if let Some(key) = self.plaintext_alert.seat(group, self.entry) { return key; }
        if group == self.pair.groups_config().detail || group == self.pair.groups_config().master { return self.pair.seat(group, from, cx); }
        let elems = self.row_elems(Some(group));
        let elem = elems.iter().filter_map(|elem| self.place(elem, cx, At::SpringTarget)
            .map(|placed| (*elem, (placed.rect.cx() - from.rect.cx()).abs())))
            .min_by(|a, b| a.1.total_cmp(&b.1)).map(|(elem, _)| elem)
            .unwrap_or(crate::ui::dispatch::STRIP_BASE);
        self.key(elem)
    }
}

impl LibraryScreen {
    fn row_elems(&self, group: Option<GroupId>) -> Vec<u32> {
        match group {
            Some(group) if group == self.tabs => self.tab_elems(),
            Some(group) if group == self.toolbar => self.toolbar_elems().to_vec(),
            Some(STATUS_GROUP) => vec![RETRY],
            Some(group) => self.shelves.iter().find_map(|row| {
                if row.group == group { return Some(row.elems.clone()); }
                row.heading.filter(|(heading, _)| *heading == group).map(|(_, elem)| vec![elem])
            }).unwrap_or_default(),
            None => Vec::new(),
        }
    }
    fn shelf_rect(&self, index: usize, col: usize) -> Rect {
        let row = &self.shelves[index];
        let style = row_style(row);
        crate::ui::card_row::tile_rect(col, style.margin_x, style.w + style.gap, row.motion.scroll_x(),
            self.layout.shelf_y(index, self.scroll.pos) + CARD_DY, (style.w, style.h)).scaled(row.motion.scale(col))
    }
    fn shelf_rect_at<H: LibraryLike>(&self, index: usize, col: usize, cx: &Cx<'_, H>, at: At) -> Rect {
        if at == At::Drawn {
            let rect = self.shelf_rect(index, col);
            return if cx.focus.current.is_some_and(|key| key.entry == self.entry && key.elem == self.shelves[index].elems[col])
                && cx.press.scale > 0.0 { rect.scaled(cx.press.scale) } else { rect };
        }
        let row = &self.shelves[index];
        let style = row_style(row);
        let focused = cx.focus.current.filter(|key| key.entry == self.entry)
            .and_then(|key| row.elems.iter().position(|elem| *elem == key.elem));
        let x = focused.map(|col| crate::ui::card_row::scroll_into_view(row.motion.scroll_x(), col,
            row.elems.len(), style.w, style.gap, SCR_W - 2.0 * style.margin_x)).unwrap_or(row.motion.scroll_x());
        crate::ui::card_row::tile_rect(col, style.margin_x, style.w + style.gap, x,
            self.target_layout.shelf_y(index, self.scroll_target) + CARD_DY, (style.w, style.h))
            .scaled(if focused == Some(col) { style.focus_scale } else { 1.0 })
    }
    /// The linked heading of shelf `index`, over the published hub's own title.
    fn heading_widget<'a, H: LibraryLike>(&self, index: usize, cx: &Cx<'a, H>) -> Option<crate::ui::linked_heading::LinkedHeading<'a>> {
        self.shelves.get(index)?.heading?;
        let shelf = self.hub(index, cx)?;
        // A genre row's heading is its genre's name alone (6 Genres tab); a collection's counts.
        let total = if shelf.genre.is_some() { 0 } else { shelf.total };
        Some(crate::ui::linked_heading::LinkedHeading::heading(&shelf.title, "").total(total)
            .bounded(layout::RIGHT - LEFT))
    }
    /// The linked heading's face, cap top where an unlinked heading's would be.
    fn heading_rect<H: LibraryLike>(&self, index: usize, cx: &Cx<'_, H>, at: At) -> Option<Rect> {
        if let Some(face) = self.genre_face(index, cx, at) { return Some(face); }
        let heading = self.heading_widget(index, cx)?;
        let row = &self.shelves[index];
        let focused = cx.focus.current.is_some_and(|key| key.entry == self.entry && Some(key.elem) == row.heading_elem());
        Some(heading.face_rect(LEFT, self.heading_y(index, at), f32::from(focused), &heading.measure(cx.measure)))
    }
    /// The cap top of shelf `index`'s heading.
    fn heading_y(&self, index: usize, at: At) -> f32 {
        match at {
            At::Drawn => self.layout.shelf_y(index, self.scroll.pos) - crate::ui::consts::TITLE_DY - self.shelves[index].motion.lift(),
            At::SpringTarget => self.target_layout.shelf_y(index, self.scroll_target) - crate::ui::consts::TITLE_DY,
        }
    }
    /// A genre row's heading (Litefin 6 Genres tab): its genre's name, 30/800, in a face padded
    /// 6x16 at radius 12 that shows only under focus, popped 1.08 from its left edge. `None` for
    /// any other shelf.
    // shortcut: the pop is a step, not Litefin's 180 ms spring; add a spring (and its two tests) if it reads abrupt.
    fn genre_face<H: LibraryLike>(&self, index: usize, cx: &Cx<'_, H>, at: At) -> Option<Rect> {
        let shelf = self.hub(index, cx).filter(|shelf| shelf.genre.is_some())?;
        let row = self.shelves.get(index)?;
        let w = cx.measure.width_str(&shelf.title, GENRE_HEADING, true) + 2.0 * GENRE_PAD_X;
        let cy = self.heading_y(index, at) + cx.measure.cap_h(GENRE_HEADING) * 0.5;
        let focused = cx.focus.current.is_some_and(|key| key.entry == self.entry && Some(key.elem) == row.heading_elem());
        let scale = if focused { GENRE_POP } else { 1.0 };
        Some(Rect::new(LEFT - GENRE_PAD_X, cy - GENRE_FACE_H * scale * 0.5, w * scale, GENRE_FACE_H * scale))
    }
}
impl Shelf {
    fn heading_elem(&self) -> Option<u32> { self.heading.map(|(_, elem)| elem) }
    /// The group a door INTO this shelf from above lands in: its linked heading when it has one.
    fn entry_group(&self) -> GroupId { self.heading.map_or(self.group, |(group, _)| group) }
}
/// A genre row's heading: 30px bold in a face padded 6x16 around its 36px line (6 Genres tab).
const GENRE_HEADING: i32 = crate::ui::theme::size::lf::S30;
const GENRE_PAD_X: f32 = 16.0;
const GENRE_FACE_H: f32 = 48.0;
const GENRE_POP: f32 = 1.08;
/// A Library shelf's card row: the shared styles, its first picture inset in the page's 150 box.
const SHELF_HOME: RowStyle = RowStyle { margin_x: layout::CARD_LEFT, ..RowStyle::HOME };
const SHELF_EPISODE: RowStyle = RowStyle { margin_x: layout::CARD_LEFT, ..RowStyle::EPISODE };
fn shelf_style(landscape: bool) -> &'static RowStyle { if landscape { &SHELF_EPISODE } else { &SHELF_HOME } }
fn row_style(row: &Shelf) -> &'static RowStyle { shelf_style(row.landscape) }
fn row_group(id: GroupId, len: usize, extent: Rect, elem: ElemKind) -> GroupSpec {
    GroupSpec { id, kind: GroupKind::Row { wrap: false }, seat: Seat::Remembered,
        reachable: AxisMask::VERTICAL, edge: [EdgeRule::Geometric, EdgeRule::Geometric, EdgeRule::Stop, EdgeRule::Stop], extent, len, elem }
}

impl<H: LibraryLike> Screen<H> for LibraryScreen {
    fn redraw_focused(&self, f: &mut DrawFrame<'_, '_, H>, focus: Option<nj_machine::machine::FocusKey<u32>>) {
        LibraryScreen::redraw_focused::<H>(self, f, focus)
    }
    fn glides(&self) -> bool { true }
    fn name(&self) -> &'static str { "library" }
    fn state(&self) -> &dyn LogicalState { self }
    fn crumb(&self, _: &Cx<'_, H>) -> Option<Cow<'_, str>> { None }
    fn prepare(&mut self, b: &mut Budget, cx: &Cx<'_, H>) { self.pair.prepare(b, cx); }
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, H>) { self.draw_page(f); }
    fn render(&self) -> RenderStrategy { RenderStrategy::Page }
    fn memory(&self) -> PageMemory {
        PageMemory::Library(self.page_memory())
    }
    fn links(&self, out: &mut Vec<Link>) {
        // A continuous document has adjacent blocks even while its offscreen geometry
        // passes the floating strip. Geometric ranking must not skip an intervening block.
        let mut document = Vec::new();
        if self.layout.tabs() { document.push(self.tabs); }
        // A collection shelf's linked heading is its own block, directly above its cards: UP from
        // any card reaches it, DOWN returns to the remembered card (`groups`' seat).
        for row in &self.shelves {
            if let Some((heading, _)) = row.heading { document.push(heading); }
            document.push(row.group);
        }
        if self.layout.grid_head { document.push(self.toolbar); }
        if self.readout == Readout::Failed { document.push(STATUS_GROUP); }
        // The alphabet bar is a block of the document between the controls and the grid, so
        // none of MasterDetail's side doors apply: UP and DOWN are its only ways in and out.
        if self.pair.master.shown() { document.push(self.pair.groups_config().master); }
        if self.grid_shown() && !self.pair.detail.elems.is_empty() { document.push(self.pair.groups_config().detail); }
        for pair in document.windows(2) {
            out.push(Link { from: pair[0], dir: Dir::Down, to: pair[1] });
            out.push(Link { from: pair[1], dir: Dir::Up, to: pair[0] });
        }
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> { Some(self) }
}

impl LogicalState for LibraryScreen {
    fn write(&self, c: &mut Canon) {
        c.u32(self.entry.0).u32(self.instance.0);
        c.option(self.wanted.as_ref(), |c, wanted| { c.u32(wanted.sid.raw() as u32).u64(wanted.key as u64); });
        self.genres.write(c);
        self.head_pop.write_motion(c);
        c.f32(self.scroll.pos).f32(self.scroll.vel).f32(self.scroll_target);
        c.option(self.restore_scroll, |c, value| { c.f32(value); });
        c.bool(self.live).bool(self.initial);
        c.option(self.provisional, |c, group| { c.u32(group.0); });
        c.seq(self.placed.len());
        for (group, elem) in &self.placed { c.u32(group.0).u32(*elem); }
        c.bool(self.sweep_down);
        c.option(self.epoch, |c, value| { c.u32(value); });
        c.option(self.query, |c, value| { c.u32(value); });
        c.bool(self.grid_reset_pending);
        c.option(self.shelf_publication, |c, (id, revision)| {
            c.u32(id.epoch).u32(u32::from(id.sid.raw())).u64(id.section as u64).u64(revision);
        });
        self.page_fade.write(c);
        self.grid_fade.write(c);
        self.pair.state().write(c);
        // `toolbar` is not written: like the pair's ids it is registered in `keys` for the section
        // and epoch, all three of which `page_memory` below already carries.
        c.u32(self.pair.groups_config().master.0).u32(self.pair.groups_config().detail.0);
        self.pending.write(c);
        PageMemory::Library(self.page_memory()).write(c);
        c.seq(self.viewports.len());
        for viewport in &self.viewports { viewport.write(c); }
        c.seq(self.shelves.len());
        for row in &self.shelves {
            let Shelf { id, hub, group, heading, elems, landscape, motion, basis: _ } = row;
            c.str(id).u32(*hub as u32).u32(group.0);
            c.option(*heading, |c, (group, elem)| { c.u32(group.0).u32(elem); });
            c.bool(*landscape).seq(elems.len());
            for elem in elems { c.u32(*elem); }
            motion.write_motion(c);
        }
        c.u32(match self.readout { Readout::Loading => 0, Readout::Empty => 1, Readout::Failed => 2, Readout::Grid => 3 });
        self.layout.write(c);
        self.target_layout.write(c);
        self.pair.detail.write(c);
        self.pair.master.write(c);
    }
    fn probe(&self, out: &mut String) { out.push_str("library"); }
}
