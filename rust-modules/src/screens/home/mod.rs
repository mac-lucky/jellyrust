//! Owned Home page for restructure phase 8, drawn as Litefin's Home (`docs/litefin-look.md` 4):
//! the immersive hero (a masked, slowly zooming backdrop with the slide's logo, meta line and
//! synopsis bottom-anchored on the left, dots bottom-right) and the classic rows overlapping its
//! lower 450 px. Focusing a row lifts the page so that row rests 40 below the top.
//!
//! The instance owns carousel, snap and render motion. The input engine owns the only focus and
//! every group cursor; this module reads them through `Cx::focus` and never mirrors them. Catalog
//! reads come exclusively from the rig-retained `HubsView`, and stable provider/item identities
//! are interned into `HomeMemory` so reorder and eviction preserve meaning rather than position.
//! Shared top chrome is container-owned: Home declares the strip doors and handles semantic strip
//! activation, but neither draws the bar nor stores a bar cursor.
//! The container's first-`Cover`/last-`Uncover` lifecycle pauses hero auto-rotation beneath every
//! modal style and restarts its countdown when the page becomes active again.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::sync::Arc;

use crate::catalog_fetch::{HeroRef, HubIdentity, HubRef, HubsView, PmsMovie, HUB_MY_MEDIA, HUB_NEXT_UP};
use crate::stores::hubs::HubsCmd;
use crate::stores::{StoreCmd, StoreId, StoreWork};
use crate::ui::card_row::{self, CardRow, RowStyle};
use crate::ui::consts::*;
use crate::ui::frame::Budget;
use crate::ui::hero_logo::{HeroLogo, LogoRung};
use crate::ui::label::{Label, VAlign};
use crate::ui::linked_heading::{self, LinkedHeading};
use crate::ui::landing_hero::{
    self, LOGO_GAP, LOGO_H, LOGO_W, META_GAP, META_LH, SYN_LH, SYN_LINES, SYN_W, TEXT_X,
};
use nj_machine::machine::{
    Canon, Cx, Delivery, Edge, Effects, EntryId, FocusKey, Fx, GroupId, Handled, InputEvent,
    InputKind, InstanceId, Key, LogicalState, Machine, MachineId, Measure, Tick,
};
use nj_machine::present::{PresentEvent, Provenance};
use crate::ui::screen::{
    Activate, At, AxisMask, By, Dir, DrawFrame, EdgeRule, ElemKind, Enter, FocusTarget, Focusable,
    GroupKind, GroupSpec, Hover, Link, Placed, RenderStrategy, Screen, ScreenEvent, Seat, Step,
    Stop,
};
use crate::ui::text_view::TextView;
use crate::ui::theme;
use crate::ui::widgets::{AmbientWash, Art, HeroArt, PageDots, StatusKind, StatusOverlay};
use crate::ui::{on_axis, Env, Painter, Rect, Spring, View};

use super::clock_readout::ClockWatch;
use super::plaintext_question::{self, AlertStep, Near, OfferWatch, PlaintextAlert};
use super::registry::{
    Compaction, RowSeats, RowDiff, tile_facts, AppFx, AppMsg, ContentArg, ContentReq, HomeCmd, HomeGroupKey, HomeHubIdentity, HomeItemIdentity, HomeItemKey, HomeLike,
    HomeMemory, HomeReq, LoopReq, PageMemory,
};

const HERO_GROUP: GroupId = GroupId(0);
const FIRST_HUB_GROUP: u32 = 0x100;
/// The hero's one element: OK opens the slide's detail page and LEFT/RIGHT page the slides. Under
/// a status read-out it is the read-out's action.
const HERO_ELEM: u32 = 0;
const FIRST_ITEM_ELEM: u32 = 0x1000;
/// A collection shelf's linked heading (`ui::linked_heading`) is keyed off its shelf's group: the
/// heading's group AND its one element are `HEADING_BASE | shelf group`. Both derive from the
/// interned hub group, so they survive reorder and page memory without a registry of their own;
/// item elements and hub groups stay below this bit.
const HEADING_BASE: u32 = 0x8000_0000;
/// "Connect without encryption?" over the failure read-out (`screens::plaintext_question`): its
/// group sits between the hero's and the first hub's, its two answers below the first item key.
const PLAINTEXT_GROUP: GroupId = GroupId(0xF0);
const PLAINTEXT_CANCEL_ELEM: u32 = 0x0F00;
const PLAINTEXT_CONNECT_ELEM: u32 = 0x0F01;

const MAX_ITEMS: usize = crate::catalog_fetch::MAX_SHELF_ITEMS;
const HERO_FLIP_CD: f32 = 0.35;
const HERO_AUTO_S: f32 = 8.0;
/// The slides' crossfade: critically damped, 99% of the way in Litefin's one second.
const K_FADE: f32 = 45.0;
/// A crossfade this near its end is over: half an 8-bit step of alpha.
const HERO_FADE_REST: f32 = 1.0 / 512.0;
/// How far a slide's text block rises as it fades in (and sinks as it fades out).
const HERO_TEXT_RISE: f32 = 20.0;
/// The focused hero's accent band fades in or out in Litefin's 300 ms.
const K_BAND: f32 = 480.0;
/// How near its target the snap dive has to be, in snap units (0 = hero, 1 = grid), before this
/// page calls the dive over — **together with [`SNAP_REST_VEL`], never alone.** `K_SNAP` is
/// critically damped, so the dive spends its last dozen frames inside any position threshold worth
/// picking while the picture is still visibly moving. Read by the present gate.
const SNAP_REST_POS: f32 = 0.002;
/// The velocity half of [`SNAP_REST_POS`], in snap units per second — the term that sees the
/// critically damped tail the position test is blind to.
const SNAP_REST_VEL: f32 = 0.01;
const HERO_PREFETCH: usize = 1;
/// Past this page offset the hero's art is off screen: the vertical mask's last stop.
const HERO_ART_END: f32 = landing_hero::MASK_Y[landing_hero::MASK_Y.len() - 1].0 * landing_hero::HEIGHT;
/// The logo box: a clearLogo is contained in 400x130 (floor and ceiling both the box's height),
/// the title text falls back at 51 px bold.
const HERO_LOGO: LogoRung = LogoRung::Custom {
    area: LOGO_W * LOGO_H,
    h_min: LOGO_H,
    h_max: LOGO_H,
    fallback_sz: theme::size::lf::S51,
};
/// The meta line: 16 between its items; the rating badge's 2x8 padding and radius 4.
const META_ITEM_GAP: f32 = 16.0;
const BADGE_PAD: (f32, f32) = (8.0, 2.0);
const BADGE_RAD: f32 = 4.0;
/// The community score's star to its number, as on the detail page's meta row.
const META_STAR_GAP: f32 = 6.0;
/// The hero's focus rect ([`HomeScreen::hero_rect`]): its logo box over a full synopsis.
const HERO_SEAT: Rect = Rect::new(TEXT_X, landing_hero::logo_top(SYN_LINES), LOGO_W, LOGO_H);
/// The pager: its right edge 80 from the screen's, its foot 430 above the hero's.
const DOTS_RIGHT: f32 = SCR_W - 80.0;
const DOTS_FOOT: f32 = landing_hero::HEIGHT - 430.0;
const SOURCE_PAD: f32 = theme::space::XS;

// ---- Litefin's classic rows (`docs/litefin-look.md` 4.2) ----
/// Where the rows begin: they overlap the hero's lower 450.
const ROWS_OVERLAP_TOP: f32 = landing_hero::HEIGHT - 450.0;
/// The first row's top with the hero up: 24 of padding into the rows.
const ROWS_TOP: f32 = ROWS_OVERLAP_TOP + 24.0;
/// Where the focused row's top rests: 40 below the top of the screen.
const ROW_FOCUS_TOP: f32 = 40.0;
/// How far the page rises when the first row takes focus.
const PAGE_LIFT: f32 = ROWS_TOP - ROW_FOCUS_TOP;
/// The row title: the rail plus the row's 60 of side padding, in a 48 px line box (32 at 1.5).
const ROW_TITLE_X: f32 = CONTENT_LEFT + 60.0;
const ROW_TITLE_LH: f32 = 48.0;
/// The pictures: 14 under the title, 24 of room above the card boxes for the focus pop, and the
/// box's own border and ring ([`CARD_INSET`]); under their labels the box's bottom border, 38 and
/// then 28 to the next row. A picture's x is its style's `margin_x` ([`card_x`]).
const ROW_CARDS_DY: f32 = ROW_TITLE_LH + 14.0 + 24.0 + CARD_INSET;
const ROW_FOOT: f32 = (CARD_INSET - theme::FOCUS_RING_W) + 38.0 + 28.0;
/// The rows' bottom padding: how far above the screen's foot the last row may rest.
const ROWS_PAD_BOTTOM: f32 = 60.0;

pub(crate) const SHAPE: &str = "HomeScreen{snap_target:f32,snap:{pos:f32,vel:f32},hero_flip_cd:f32,hero_auto:f32,covered:bool,visible_activation:Option<u32>,cta_available:bool,strip_chosen:bool,next_group:u32,next_elem:u32,groups:[HomeGroupKey{identity:HomeHubIdentity,group:u32}],items:[HomeItemKey{identity:HomeItemIdentity,elem:u32,last_row:u32,last_col:u32}],carousel:Option<(sid:u32,rk:str)>,outgoing:Option<(sid:u32,rk:str)>,hero_slide:{pos:f32,vel:f32},hero_band:{pos:f32,vel:f32},hero_zoom:f32,projected_generation:Option<u32>,grid:{scroll_y:{pos:f32,vel:f32},scroll_target:f32,rows:[{identity:HomeHubIdentity,group:u32,elems:[u32],motion:CardRow{scale:[Spring;24],overflow:Spring,scroll_x:Spring,lift:Spring,band:Spring,focus:i32,base_y:f32}}]},restored_scroll:[(group:u32,scroll:f32)],restore_reveal:bool}";

#[derive(Clone)]
struct HubProjection {
    identity: HomeHubIdentity,
    group: GroupId,
    elems: Vec<u32>,
    /// Where the row's linked heading leads: the collection a promoted `custom.collection.*` hub
    /// lists, classified once per publication. Every other row is unlinked and keeps its plain
    /// heading.
    link: Option<crate::catalog::collections::CollectionRef>,
    /// Litefin's landscape row (My Media, Continue Watching, Next Up) rather than a poster row.
    landscape: bool,
    // Keys depend on ordered server/item identities, not changing title/resume metadata.
    // Empty rating keys carry the publication generation and are always rebuilt.
    basis: Vec<(crate::catalog::ServerId, String)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Located {
    Hero(usize),
    Item(usize, usize),
    /// A collection shelf's linked heading, by row.
    Heading(usize),
}

impl Located {
    /// On the shelves (a card or a linked heading) rather than the hero.
    fn on_grid(self) -> bool {
        matches!(self, Self::Item(..) | Self::Heading(_))
    }
}

fn heading_group(shelf: GroupId) -> GroupId {
    GroupId(HEADING_BASE | shelf.0)
}

fn heading_elem(shelf: GroupId) -> u32 {
    HEADING_BASE | shelf.0
}

/// What a row index past the grid reads: a shelf at rest, so a stale index never panics.
static RESTING: CardRow = CardRow::new();

struct Grid {
    /// One motion row per published row: `shelves.len() == rows.len()` once `sync_catalog` returns.
    shelves: Vec<CardRow>,
    scroll_y: Spring,
    scroll_target: f32,
}

impl Grid {
    fn new() -> Self {
        Self {
            shelves: Vec::new(),
            scroll_y: Spring::at(0.0),
            scroll_target: 0.0,
        }
    }

    fn shelf(&self, row: usize) -> &CardRow {
        self.shelves.get(row).unwrap_or(&RESTING)
    }

    fn scroll_x(&self, row: usize) -> f32 {
        self.shelf(row).scroll_x()
    }
}

/// The hero's backdrop layers: the selected slide's art and, through a crossfade, the outgoing
/// slide's, each revealed as its texture lands. Paint state: the layers are a function of the
/// heroes the page names ([`Backdrop::bind`]).
struct Backdrop {
    art: Spring,
    outgoing_art: Spring,
    tex: (u32, f32, f32),
    outgoing_tex: (u32, f32, f32),
    keyed: Option<(crate::catalog::ServerId, String)>,
}

impl Backdrop {
    fn new() -> Self {
        Self {
            art: Spring::at(0.0),
            outgoing_art: Spring::at(0.0),
            tex: (0, 0.0, 0.0),
            outgoing_tex: (0, 0.0, 0.0),
            keyed: None,
        }
    }

    /// **Bind both art layers to the heroes the page names NOW**: resolve the selected and the
    /// outgoing backdrop, and re-key the reveal springs when the selection changed. Idempotent —
    /// a second call with the same heroes changes nothing — so it runs twice a frame: from
    /// [`Self::update`] on the `Tick`, and again from `draw_page` immediately before the draw.
    ///
    /// The second call is the correctness half. A MANUAL flip is the hero's LEFT/RIGHT, which the
    /// dispatcher delivers to the page BEHIND the frame's `Tick`. Layers bound only on the tick
    /// were therefore the PREVIOUS pair on the press frame, drawn at the new pair's crossfade
    /// (owner report 2026-09-30, from the sliding hero this replaced). Binding at the draw makes
    /// the layers a function of the state the draw reads, whatever event changed it. `visible`:
    /// the art reaches the screen at the page's offset; off screen nothing is resolved.
    fn bind(
        &mut self,
        hero: Option<HeroRef<'_>>,
        outgoing: Option<HeroRef<'_>>,
        selected: Option<&(crate::catalog::ServerId, String)>,
        visible: bool,
    ) {
        let resolve = |h: Option<HeroRef<'_>>| {
            h.map(|h| crate::ui::widgets::resolve_tex_wh_on(h.item.sid.raw(), &h.item.art, 1280, 720, 0))
                .unwrap_or((0, 0.0, 0.0))
        };
        if visible {
            self.tex = resolve(hero);
            self.outgoing_tex = resolve(outgoing);
        } else {
            self.tex = (0, 0.0, 0.0);
            self.outgoing_tex = (0, 0.0, 0.0);
        }
        let changed = match (self.keyed.as_ref(), selected) {
            (Some((a, x)), Some((b, y))) => a != b || x != y,
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.outgoing_art.jump(self.art.pos);
            self.art.jump(f32::from(self.tex.0 != 0));
            self.keyed = selected.cloned();
        }
    }

    fn update(
        &mut self,
        hero: Option<HeroRef<'_>>,
        outgoing: Option<HeroRef<'_>>,
        selected: Option<&(crate::catalog::ServerId, String)>,
        visible: bool,
        dt: f32,
    ) {
        self.bind(hero, outgoing, selected, visible);
        // decorative: the art fading in as its texture lands moves no layout
        nj_machine::idle::decorative(|| {
            if self.tex.0 != 0 {
                self.art.step(1.0, AmbientWash::K, dt);
            }
            if self.outgoing_tex.0 != 0 {
                self.outgoing_art.step(1.0, AmbientWash::K, dt);
            }
        });
    }

    /// The ground: the outgoing layer under the incoming one while `crossfade` (the incoming
    /// share) runs, each at its own Ken Burns scale, masked into the page.
    fn draw(&self, p: Painter, crossfade: Option<f32>, zoom: (f32, f32)) {
        let layer = |tex: (u32, f32, f32), zoom: f32, a: f32| HeroArt { tex: tex.0, size: (tex.1, tex.2), zoom, a };
        let t = crossfade.unwrap_or(1.0);
        let layers = [
            layer(self.outgoing_tex, zoom.1, reveal(self.outgoing_tex.0, &self.outgoing_art) * (1.0 - t)),
            layer(self.tex, zoom.0, reveal(self.tex.0, &self.art) * t),
        ];
        let layers = if crossfade.is_some() { &layers[..] } else { &layers[1..] };
        crate::ui::widgets::hero_backdrop(p, layers);
    }
}

/// A card named by borrowed parts, so a lookup in the key table clones nothing.
#[derive(Clone, Copy)]
enum ItemRef<'a> {
    Item { sid: crate::catalog::ServerId, rk: &'a str },
    Slot { generation: u32, ordinal: u32 },
}

/// One hub's entries in the key table: positions in `HomeScreen::items`.
#[derive(Default)]
struct HubKeys {
    by_rk: HashMap<Arc<str>, Vec<(crate::catalog::ServerId, usize)>>,
    slots: HashMap<(u32, u32), usize>,
}

impl HubKeys {
    #[cfg(test)]
    fn len(&self) -> usize {
        self.by_rk.values().map(Vec::len).sum::<usize>() + self.slots.len()
    }
}

pub(crate) struct HomeScreen {
    entry: EntryId,
    instance: InstanceId,

    groups: Vec<HomeGroupKey>,
    /// Shared with `memory()`: an unchanged republication leaves it shared, and only a real
    /// write (a new key, a moved card, a prune) copies it. Mutate through `Arc::make_mut`.
    items: Arc<Vec<HomeItemKey>>,
    next_group: u32,
    next_elem: u32,
    rows: Vec<HubProjection>,
    /// `elem` -> `(row, col)` for every card in `rows`, so [`HomeScreen::locate`] is a lookup
    /// rather than a scan of every row. Derived in `sync_catalog`; not canon.
    elem_at: HashMap<u32, (u32, u32)>,
    /// `items` / `groups` looked up by identity, so keying a card is a hash probe rather than a
    /// scan of the whole table. Positions into the two `Vec`s; kept in step by `push_item` /
    /// `push_group`, the only places either grows. A prune compacts the tables and follows the
    /// surviving index positions in place through `registry::Compaction`. Derived, not canon.
    item_index: HashMap<HomeHubIdentity, HubKeys>,
    group_index: HashMap<HomeHubIdentity, usize>,
    projected_generation: Option<u32>,
    projection_stale: bool,
    volatile_keys: usize,
    restored_scroll: Vec<(u32, f32)>,
    restore_reveal: bool,

    /// Selected hero ITEM identity. It is data, not focus.
    carousel: Option<(crate::catalog::ServerId, String)>,
    outgoing: Option<(crate::catalog::ServerId, String)>,
    hero_flip_cd: f32,
    /// The crossfade from `outgoing` to `carousel`, 0 to 1: the incoming slide's share.
    hero_slide: Spring,
    /// Seconds the slide on screen has been zooming (Ken Burns, `landing_hero::zoom_at`), held at
    /// `landing_hero::ZOOM_S`. Logical state: the timer decides whether the page still moves.
    hero_zoom: f32,
    /// The outgoing slide's zoom, held where it stopped while it fades. Paint only.
    outgoing_zoom: f32,
    /// The focused hero's accent band, 0 to 1.
    hero_band: Spring,
    /// Seconds until the carousel auto-advances, hashed as logical state (`SHAPE`'s
    /// `hero_auto:f32`). Deliberately still a raw per-frame decrement (phase 12 D4 did NOT move
    /// this onto `motion::Ramp`): it is hashed across three committed replay fixtures, and a
    /// `Ramp`'s absolute-`Tick.ms` math computes the same real quantity through a different float
    /// operation sequence that measurably diverges the hash. `tick`'s own comment explains why
    /// the timer deliberately reports no `Motion` while it counts.
    hero_auto: f32,
    /// `HomeCmd::PinHero` holds the billboard: the countdown above stops for the life of the page.
    /// A screenshot pin only, never set by the product, and not logical state (no replay sets it).
    hero_pinned: bool,
    /// The container says this page is covered. For modals, `Navigation` emits `Cover` for the
    /// first presentation and `Uncover` only when the last surface is dismissed, independent of
    /// whether that surface's host-update policy is Live or Frozen.
    covered: bool,

    snap_target: f32,
    /// **The page is where the pointer put it, not where focus would rest it.** Set by a wheel (the
    /// pointer's edge glide, `app::input::EdgeScroll`) and by a hover, which moves focus but never
    /// the page; cleared by a key's move and by a click, which bring the page back to the focused
    /// shelf. While set, the tick does not pull `grid.scroll_target` to the focused row.
    free: bool,
    /// **A click that only brings a card into view.** A click on a shelf card the screen shows only
    /// part of scrolls the page to it and does not open it: its press is dropped when it commits.
    /// Cleared by any key.
    reveal_press: Option<u32>,
    /// Stable element still visible while the engine has already crossed the hero/grid door.
    /// This is a press landing policy, not current focus, and retires at the visual midpoint.
    visible_activation: Option<u32>,
    cta_available: bool,
    strip_chosen: bool,
    snap: Spring,
    status_ms: f32,
    /// The plaintext-only server the failure read-out speaks about, when discovery offers the
    /// question for one (`plex::grant::offers`) — not logical state: it is the grant table's.
    plaintext: OfferWatch,
    /// Why key mode cannot help (`net::keypin::blocked`), held between ticks so a frame's draw and
    /// its hit rects read ONE value — the reason line moves the action row.
    clock: ClockWatch,
    /// The question, asked from the read-out's *Connect*.
    plaintext_alert: PlaintextAlert,
    backdrop: Backdrop,
    grid: Grid,
}

impl HomeScreen {
    pub(crate) fn new(entry: EntryId, id: InstanceId) -> Self {
        Self {
            entry,
            instance: id,
            groups: Vec::new(),
            items: Arc::default(),
            next_group: FIRST_HUB_GROUP,
            next_elem: FIRST_ITEM_ELEM,
            rows: Vec::new(),
            elem_at: HashMap::new(),
            item_index: HashMap::new(),
            group_index: HashMap::new(),
            projected_generation: None,
            projection_stale: false,
            volatile_keys: 0,
            restored_scroll: Vec::new(),
            restore_reveal: false,
            carousel: None,
            outgoing: None,
            hero_flip_cd: 0.0,
            hero_slide: Spring::at(1.0),
            hero_zoom: 0.0,
            outgoing_zoom: 0.0,
            hero_band: Spring::at(0.0),
            hero_auto: HERO_AUTO_S,
            hero_pinned: false,
            covered: false,
            snap_target: 0.0,
            free: false,
            reveal_press: None,
            visible_activation: None,
            cta_available: false,
            strip_chosen: false,
            snap: Spring::at(0.0),
            status_ms: 0.0,
            plaintext: OfferWatch::default(),
            clock: ClockWatch::default(),
            plaintext_alert: PlaintextAlert::new(PLAINTEXT_GROUP, PLAINTEXT_CANCEL_ELEM, PLAINTEXT_CONNECT_ELEM),
            backdrop: Backdrop::new(),
            grid: Grid::new(),
        }
    }

    pub(crate) fn restore(&mut self, memory: &HomeMemory) {
        let mut taken_groups: HashSet<u32> = self.groups.iter().map(|k| k.group).collect();
        for saved in &memory.groups {
            if self.find_group(&saved.identity).is_none() {
                assert!(taken_groups.insert(saved.group), "restored Home group collision");
                self.push_group(saved.clone());
            }
        }
        let mut taken_elems: HashSet<u32> = self.items.iter().map(|k| k.elem).collect();
        for saved in memory.items.iter() {
            if let Some(index) = self.find_item(&saved.identity) {
                let existing = &self.items[index];
                if (existing.last_row, existing.last_col) != (saved.last_row, saved.last_col) {
                    let existing = &mut Arc::make_mut(&mut self.items)[index];
                    existing.last_row = saved.last_row;
                    existing.last_col = saved.last_col;
                }
            } else {
                assert!(taken_elems.insert(saved.elem), "restored Home element collision");
                self.push_item(saved.clone());
            }
        }
        self.next_group = self.next_group.max(memory.next_group).max(FIRST_HUB_GROUP);
        self.next_elem = self.next_elem.max(memory.next_elem).max(FIRST_ITEM_ELEM);
        self.carousel = memory.carousel.clone();
        self.strip_chosen = memory.strip_chosen;
        self.grid.scroll_y.jump(memory.scroll_y);
        self.grid.scroll_target = memory.scroll_y;
        self.restored_scroll = memory.row_scroll.clone();
        self.restore_reveal = true;
        self.projection_stale = true;
        self.projected_generation = None;
    }

    fn hub_identity(view: HubsView<'_>, row: usize, hub: HubRef<'_>) -> HomeHubIdentity {
        match hub.identity {
            Some(HubIdentity::ContinueWatching) => HomeHubIdentity::ContinueWatching,
            Some(HubIdentity::Identifier { sid, id, key }) => HomeHubIdentity::Identifier {
                sid,
                id: id.into(),
                key: key.into(),
            },
            Some(HubIdentity::Key { sid, key }) => HomeHubIdentity::Key {
                sid,
                key: key.into(),
            },
            None => HomeHubIdentity::Ephemeral {
                generation: view.generation,
                ordinal: row as u32,
            },
        }
    }

    fn find_group(&self, identity: &HomeHubIdentity) -> Option<usize> {
        self.group_index.get(identity).copied()
    }

    fn push_group(&mut self, key: HomeGroupKey) {
        Self::index_group(&mut self.group_index, &key, self.groups.len());
        self.groups.push(key);
    }

    /// First wins, as the scan this replaced did.
    fn index_group(index: &mut HashMap<HomeHubIdentity, usize>, key: &HomeGroupKey, at: usize) {
        index.entry(key.identity.clone()).or_insert(at);
    }

    fn group_for(&mut self, identity: &HomeHubIdentity) -> GroupId {
        if let Some(index) = self.find_group(identity) {
            return GroupId(self.groups[index].group);
        }
        let group = self.next_group.max(FIRST_HUB_GROUP);
        self.next_group = group
            .checked_add(1)
            .expect("Home group-key space exhausted");
        assert!(group < HEADING_BASE, "Home groups overlap the linked headings");
        self.push_group(HomeGroupKey {
            identity: identity.clone(),
            group,
        });
        GroupId(group)
    }

    /// Where `identity` sits in `items`. A probe by borrowed parts: no `String` is cloned.
    fn find_item(&self, identity: &HomeItemIdentity) -> Option<usize> {
        match identity {
            HomeItemIdentity::Item { hub, sid, rk } => self.item_slot(hub, ItemRef::Item { sid: *sid, rk }),
            HomeItemIdentity::Slot { hub, generation, ordinal } => {
                self.item_slot(hub, ItemRef::Slot { generation: *generation, ordinal: *ordinal })
            }
        }
    }

    fn item_slot(&self, hub: &HomeHubIdentity, item: ItemRef<'_>) -> Option<usize> {
        let keys = self.item_index.get(hub)?;
        match item {
            ItemRef::Item { sid, rk } => keys.by_rk.get(rk)?.iter().find(|(s, _)| *s == sid).map(|&(_, i)| i),
            ItemRef::Slot { generation, ordinal } => keys.slots.get(&(generation, ordinal)).copied(),
        }
    }

    fn push_item(&mut self, key: HomeItemKey) {
        self.volatile_keys += usize::from(Self::volatile(&key));
        Self::index_item(&mut self.item_index, &key, self.items.len());
        Arc::make_mut(&mut self.items).push(key);
    }

    /// First wins, as the scan this replaced did.
    fn index_item(index: &mut HashMap<HomeHubIdentity, HubKeys>, key: &HomeItemKey, at: usize) {
        match &key.identity {
            HomeItemIdentity::Item { hub, sid, rk } => {
                let sids = index.entry(hub.clone()).or_default().by_rk.entry(rk.clone()).or_default();
                if !sids.iter().any(|(s, _)| s == sid) {
                    sids.push((*sid, at));
                }
            }
            HomeItemIdentity::Slot { hub, generation, ordinal } => {
                index.entry(hub.clone()).or_default().slots.entry((*generation, *ordinal)).or_insert(at);
            }
        }
    }

    /// The position of `item`'s key, minting one if the card has none.
    fn item_registration(&mut self, hub: &HomeHubIdentity, item: ItemRef<'_>) -> usize {
        if let Some(index) = self.item_slot(hub, item) {
            return index;
        }
        let elem = self.next_elem.max(FIRST_ITEM_ELEM);
        self.next_elem = elem
            .checked_add(1)
            .expect("Home element-key space exhausted");
        assert!(elem < HEADING_BASE, "Home elements overlap the linked headings");
        let identity = match item {
            ItemRef::Item { sid, rk } => HomeItemIdentity::Item { hub: hub.clone(), sid, rk: rk.into() },
            ItemRef::Slot { generation, ordinal } => HomeItemIdentity::Slot { hub: hub.clone(), generation, ordinal },
        };
        self.push_item(HomeItemKey { identity, elem, last_row: 0, last_col: 0 });
        self.items.len() - 1
    }

    /// Record where `index`'s card sits and return its elem. The table is written only when the
    /// slot moved, so an unchanged republication leaves it shared with `memory()`.
    fn place_item(&mut self, index: usize, row: u32, col: u32) -> u32 {
        let key = &self.items[index];
        if (key.last_row, key.last_col) != (row, col) {
            let key = &mut Arc::make_mut(&mut self.items)[index];
            key.last_row = row;
            key.last_col = col;
        }
        self.items[index].elem
    }

    #[cfg(test)]
    fn elem_for(&mut self, identity: HomeItemIdentity) -> u32 {
        let (hub, item) = match &identity {
            HomeItemIdentity::Item { hub, sid, rk } => (hub, ItemRef::Item { sid: *sid, rk }),
            HomeItemIdentity::Slot { hub, generation, ordinal } => {
                (hub, ItemRef::Slot { generation: *generation, ordinal: *ordinal })
            }
        };
        let index = self.item_registration(hub, item);
        self.items[index].elem
    }

    /// Reuse exact key projections at the same position; changed rows alone patch `elem_at`.
    /// Payloads still come from the current view, so fresh metadata never needs key registration.
    fn sync_catalog<H: HomeLike>(&mut self, cx: &Cx<'_, H>) {
        let view = H::hubs(cx);
        if self.projected_generation == Some(view.generation) { return; }
        let mut seats = RowSeats::new(std::mem::take(&mut self.rows), view.hub_count());
        let mut rows = Vec::with_capacity(view.hub_count());
        let mut identities = HashSet::new();
        let duplicate = (0..view.hub_count()).filter_map(|row| view.hub(row)?.identity)
            .any(|identity| !identities.insert(identity));
        for row in 0..view.hub_count() {
            let Some(hub) = view.hub(row) else { continue };
            let same = hub.identity.is_some() && seats.at(row)
                .is_some_and(|old: &HubProjection| old.identity.borrowed() == hub.identity);
            let link = match hub.identity {
                Some(HubIdentity::Identifier { sid, id, key }) =>
                    crate::catalog::collections::promoted_collection_link(
                        sid, id, key, hub.title, hub.items.first().map_or(0, |item| item.sec)),
                _ => None,
            };
            let cards = &hub.items[..hub.items.len().min(MAX_ITEMS)];
            if same && !duplicate && !self.projection_stale && seats.at(row).is_some_and(|old|
                old.link == link && old.basis.len() == cards.len()
                    && old.basis.iter().zip(cards).all(|((sid, rk), item)|
                        !item.rk.is_empty() && *sid == item.sid && *rk == item.rk)) {
                rows.push(seats.keep(row));
                continue;
            }
            let identity = Self::hub_identity(view, row, hub);
            let from = seats.locate(row, same, &identity, |old| old.identity.clone());
            let group = self.group_for(&identity);
            let elems = cards.iter().enumerate().map(|(col, item)| {
                let item = if item.rk.is_empty() {
                    ItemRef::Slot { generation: view.generation, ordinal: col as u32 }
                } else { ItemRef::Item { sid: item.sid, rk: &item.rk } };
                let at = self.item_registration(&identity, item);
                self.place_item(at, row as u32, col as u32)
            }).collect();
            seats.rebuilt(from);
            let landscape = landscape(&identity);
            rows.push(HubProjection { identity, group, elems, link, landscape,
                basis: cards.iter().map(|item| (item.sid, item.rk.clone())).collect() });
        }
        let RowDiff { same_rows, matched, rebuilt, unclaimed } = seats.finish();
        if !same_rows {
            let old = std::mem::take(&mut self.grid.shelves);
            self.grid.shelves = matched.iter().map(|from|
                from.and_then(|at| old.get(at)).copied().unwrap_or_else(CardRow::new)).collect();
        }
        for (row, shelf) in rows.iter().zip(&mut self.grid.shelves) {
            if let Some(&(_, scroll)) = self.restored_scroll.iter().find(|(group, _)| *group == row.group.0) {
                shelf.restore_scroll(scroll, row.elems.len(), style(row.landscape));
            }
        }
        // Two provider rows may share identities and elements. A patch of one would otherwise
        // remove the first-wins entry owned by its unchanged sibling; rebuild that map as before.
        let patch = same_rows && !duplicate;
        let was = if patch {
            let mut was = HashMap::new();
            for old in unclaimed {
                for elem in old.elems {
                    if let Some(at) = self.elem_at.remove(&elem) { was.insert(elem, at); }
                }
            }
            for &row in &rebuilt {
                for (col, &elem) in rows[row].elems.iter().enumerate() {
                    self.elem_at.entry(elem).or_insert((row as u32, col as u32));
                }
            }
            was
        } else {
            let was = std::mem::take(&mut self.elem_at);
            for (row, hub) in rows.iter().enumerate() {
                for (col, &elem) in hub.elems.iter().enumerate() {
                    self.elem_at.entry(elem).or_insert((row as u32, col as u32));
                }
            }
            was
        };
        self.rows = rows;
        self.restored_scroll.retain(|(group, _)| {
            view.state != crate::catalog_fetch::HubState::Ready
                && !self.rows.iter().any(|row| row.group.0 == *group)
        });
        self.projected_generation = Some(view.generation);
        self.projection_stale = false;
        self.prune_keys(&was, view.generation, cx.focus.current.map(|key| key.elem));
        self.reconcile_carousel(view);
    }

    fn volatile(key: &HomeItemKey) -> bool {
        match &key.identity {
            HomeItemIdentity::Slot { .. } => true,
            HomeItemIdentity::Item { hub, .. } => matches!(hub, HomeHubIdentity::Ephemeral { .. }),
        }
    }

    /// Bound the key tables. A key is only needed while its card is shown, was just shown (so a
    /// focus on a card that rotated out can recover to its row), or is the focus. Volatile keys
    /// (a `Slot` item, an item of an `Ephemeral` hub) carry their generation and can never match
    /// again once their publication is gone, so those go at once; stable keys stay until the table
    /// outgrows twice the shown cards plus 256, then the oldest go (elems are minted in order).
    /// An `Ephemeral` group of an older generation is never projected again. A remembered cursor
    /// older than what is retained recovers to the first row (`reconcile`). A Home whose hubs are
    /// stable and whose cards carry rating keys, under the cap, drops nothing.
    fn prune_keys(&mut self, was: &HashMap<u32, (u32, u32)>, generation: u32, focus: Option<u32>) {
        let now = &self.elem_at;
        if self.volatile_keys == 0 && self.items.len() <= 2 * now.len() + 256
            && self.groups.iter().all(|key| !matches!(key.identity, HomeHubIdentity::Ephemeral { .. })) {
            return;
        }
        let live = |elem: u32| now.contains_key(&elem) || was.contains_key(&elem) || Some(elem) == focus;
        let mut drop: HashSet<u32> = self.items.iter()
            .filter(|key| {
                let volatile = match &key.identity {
                    HomeItemIdentity::Slot { .. } => true,
                    HomeItemIdentity::Item { hub, .. } => matches!(hub, HomeHubIdentity::Ephemeral { .. }),
                };
                volatile && !live(key.elem)
            })
            .map(|key| key.elem)
            .collect();
        let cap = 2 * now.len() + 256;
        let kept = self.items.len() - drop.len();
        if kept > cap {
            let mut oldest: Vec<u32> = self.items.iter()
                .map(|key| key.elem)
                .filter(|&elem| !live(elem) && !drop.contains(&elem))
                .collect();
            oldest.sort_unstable();
            drop.extend(oldest.into_iter().take(kept - cap));
        }
        let gone_groups = self.groups.iter().enumerate().filter_map(|(at, key)|
            matches!(key.identity, HomeHubIdentity::Ephemeral { generation: g, .. } if g != generation).then_some(at)).collect();
        if let Some(moved) = Compaction::remove(&mut self.groups, gone_groups) {
            self.group_index.retain(|_, at| moved.follow(at));
        }
        if !drop.is_empty() {
            let gone: Vec<_> = self.items.iter().enumerate().filter_map(|(at, key)| drop.contains(&key.elem).then_some(at)).collect();
            self.volatile_keys -= self.items.iter().filter(|key| drop.contains(&key.elem) && Self::volatile(key)).count();
            if let Some(moved) = Compaction::remove(Arc::make_mut(&mut self.items), gone) {
                self.item_index.retain(|_, hub| {
                    hub.by_rk.retain(|_, entries| { entries.retain_mut(|(_, at)| moved.follow(at)); !entries.is_empty() });
                    hub.slots.retain(|_, at| moved.follow(at));
                    !hub.by_rk.is_empty() || !hub.slots.is_empty()
                });
            }
        }
    }

    fn identity_of(item: &PmsMovie) -> Option<(crate::catalog::ServerId, String)> {
        (!item.rk.is_empty()).then(|| (item.sid, item.rk.clone()))
    }

    fn same_item(item: &PmsMovie, identity: &(crate::catalog::ServerId, String)) -> bool {
        item.sid == identity.0 && item.rk == identity.1
    }

    fn hero_by_identity<'a>(
        view: HubsView<'a>,
        identity: &(crate::catalog::ServerId, String),
    ) -> Option<HeroRef<'a>> {
        (0..view.hero_count())
            .filter_map(|i| view.hero(i))
            .find(|h| Self::same_item(h.item, identity))
    }

    /// The pool's first slide, or without a pool the first row of titles: My Media's tiles are
    /// libraries, not something a billboard can open.
    fn first_hero(view: HubsView<'_>) -> Option<HeroRef<'_>> {
        view.hero(0).or_else(|| {
            let hub = (0..view.hub_count()).filter_map(|i| view.hub(i)).find(|h| {
                !matches!(h.identity, Some(HubIdentity::Identifier { id, .. }) if id == HUB_MY_MEDIA)
            })?;
            Some(HeroRef {
                item: hub.items.first()?,
                source: hub.source,
            })
        })
    }

    fn reconcile_carousel(&mut self, view: HubsView<'_>) {
        if self
            .carousel
            .as_ref()
            .and_then(|id| Self::hero_by_identity(view, id))
            .is_some()
        {
            return;
        }
        self.outgoing = None;
        self.hero_slide.jump(1.0);
        self.hero_zoom = 0.0;
        self.carousel = Self::first_hero(view).and_then(|h| Self::identity_of(h.item));
    }

    fn selected_hero<'a>(&self, view: HubsView<'a>) -> Option<HeroRef<'a>> {
        self.carousel
            .as_ref()
            .and_then(|id| Self::hero_by_identity(view, id))
            .or_else(|| Self::first_hero(view))
    }

    fn outgoing_hero<'a>(&self, view: HubsView<'a>) -> Option<HeroRef<'a>> {
        self.outgoing
            .as_ref()
            .and_then(|id| Self::hero_by_identity(view, id))
    }

    fn carousel_index(&self, view: HubsView<'_>) -> usize {
        self.carousel
            .as_ref()
            .and_then(|id| {
                (0..view.hero_count())
                    .find(|&i| view.hero(i).is_some_and(|h| Self::same_item(h.item, id)))
            })
            .unwrap_or(0)
    }

    fn flip(&mut self, view: HubsView<'_>, dir: i32) -> bool {
        let n = view.hero_count();
        if n <= 1 || self.hero_flip_cd > 0.0 {
            return false;
        }
        let cur = self.carousel_index(view).min(n - 1);
        let next = (cur as i32 + dir).rem_euclid(n as i32) as usize;
        let Some(current) = view.hero(cur).and_then(|h| Self::identity_of(h.item)) else {
            return false;
        };
        let Some(incoming) = view.hero(next).and_then(|h| Self::identity_of(h.item)) else {
            return false;
        };
        self.outgoing = Some(current);
        self.carousel = Some(incoming);
        self.hero_slide.jump(0.0);
        self.outgoing_zoom = self.hero_zoom;
        self.hero_zoom = 0.0;
        self.hero_flip_cd = HERO_FLIP_CD;
        self.hero_auto = HERO_AUTO_S;
        true
    }

    /// The incoming slide's share of a running crossfade; `None` once one slide stands alone.
    fn crossfade(&self) -> Option<f32> {
        self.outgoing.as_ref()?;
        Some(self.hero_slide.pos.clamp(0.0, 1.0))
    }

    /// The Ken Burns scales of the incoming and the outgoing slide.
    fn zooms(&self) -> (f32, f32) {
        (landing_hero::zoom_at(self.hero_zoom), landing_hero::zoom_at(self.outgoing_zoom))
    }

    fn locate(&self, elem: u32) -> Option<Located> {
        if elem == HERO_ELEM {
            return Some(Located::Hero(0));
        }
        if elem & HEADING_BASE != 0 {
            return self.rows.iter().position(|row| {
                heading_elem(row.group) == elem && row.link.is_some()
            }).map(Located::Heading);
        }
        self.elem_at
            .get(&elem)
            .map(|&(row, col)| Located::Item(row as usize, col as usize))
    }

    fn focused_loc(&self, focus: Option<FocusKey<u32>>) -> Option<Located> {
        focus
            .filter(|k| k.entry == self.entry)
            .and_then(|k| self.locate(k.elem))
    }

    fn focused_grid(&self, focus: Option<FocusKey<u32>>) -> Option<(usize, usize)> {
        match self.focused_loc(focus)? {
            Located::Item(row, col) => Some((row, col)),
            Located::Hero(_) | Located::Heading(_) => None,
        }
    }

    /// The row a focused linked heading heads.
    fn focused_heading(&self, focus: Option<FocusKey<u32>>) -> Option<usize> {
        match self.focused_loc(focus)? {
            Located::Heading(row) => Some(row),
            Located::Hero(_) | Located::Item(..) => None,
        }
    }

    /// Where a row's linked heading leads, if it has one.
    fn linked(&self, row: usize) -> Option<&crate::catalog::collections::CollectionRef> {
        self.rows.get(row)?.link.as_ref()
    }

    /// The linked heading of `row`: title case, the hub's own title, its collection's member count
    /// and its source annotation.
    fn heading_widget<'a>(&self, view: HubsView<'a>, row: usize) -> Option<LinkedHeading<'a>> {
        self.linked(row)?;
        let hub = self.hub(view, row)?;
        Some(LinkedHeading::heading(hub.title, hub.source).total(hub.total))
    }

    /// The heading's face: its cap top rides the row exactly as an unlinked heading does.
    fn heading_rect(&self, view: HubsView<'_>, row: usize, measure: &dyn Measure, at: At,
        focus: Option<FocusKey<u32>>) -> Option<Rect> {
        let heading = self.heading_widget(view, row)?;
        let m = heading.measure(measure);
        let focused = self.focused_heading(focus) == Some(row);
        let line = match at {
            At::Drawn => self.grid.shelf(row).base_y,
            At::SpringTarget => ROW_FOCUS_TOP + self.flow(row) - self.grid.scroll_target,
        };
        Some(heading.face_rect(ROW_TITLE_X, title_cap_top(line, measure), f32::from(focused), &m))
    }

    fn visible_focus(&self, focus: Option<FocusKey<u32>>) -> Option<FocusKey<u32>> {
        focus.map(|key| FocusKey {
            entry: key.entry,
            elem: self.activation_elem(key.elem),
        })
    }

    fn hub<'a>(&self, view: HubsView<'a>, row: usize) -> Option<HubRef<'a>> {
        (row < self.rows.len()).then(|| view.hub(row)).flatten()
    }

    fn item_at<'a>(&self, view: HubsView<'a>, row: usize, col: usize) -> Option<&'a PmsMovie> {
        self.hub(view, row)?.items.get(col)
    }

    fn env(&self, dt: f32) -> Env {
        let mut env = Env::inert();
        env.dt = dt;
        env.sp = self.snap.pos;
        env
    }

    /// The style row `row` draws its cards with.
    fn style(&self, row: usize) -> &'static RowStyle {
        style(self.rows.get(row).is_some_and(|r| r.landscape))
    }

    /// Whether row `row` is My Media, whose tiles are libraries.
    fn my_media(&self, row: usize) -> bool {
        self.rows.get(row).is_some_and(|r| is_my_media(&r.identity))
    }

    /// Row `row`'s height in the flow: its title, its cards, their labels (a library tile carries
    /// only its name) and the gap to the next row.
    fn pitch(&self, row: usize) -> f32 {
        ROW_CARDS_DY + self.style(row).h + card_row::TileLabel::height(!self.my_media(row)) + ROW_FOOT
    }

    /// Row `row`'s top in the flow, from the first row's.
    fn flow(&self, row: usize) -> f32 {
        (0..row).map(|r| self.pitch(r)).sum()
    }

    /// How far the page has risen this frame: nothing with the hero up, [`PAGE_LIFT`] plus the
    /// rows' scroll on the rows.
    fn page_y(&self) -> f32 {
        self.snap.pos * (PAGE_LIFT + self.grid.scroll_y.pos)
    }

    /// The furthest the rows scroll: until the last one's foot is the rows' padding above the
    /// screen's.
    fn grid_max_scroll(&self) -> f32 {
        (ROW_FOCUS_TOP + self.flow(self.rows.len()) + ROWS_PAD_BOTTOM - SCR_H).max(0.0)
    }

    /// The scroll that rests row `row` [`ROW_FOCUS_TOP`] below the top, as far as the rows go.
    fn row_scroll(&self, row: usize) -> f32 {
        self.flow(row).min(self.grid_max_scroll())
    }

    fn layout_grid(&mut self) {
        let lift = self.page_y();
        let mut flow = 0.0;
        for row in 0..self.grid.shelves.len() {
            let pitch = self.pitch(row);
            self.grid.shelves[row].base_y = ROWS_TOP + flow - lift;
            flow += pitch;
        }
    }

    fn update_grid<H: HomeLike>(&mut self, view: HubsView<'_>, cx: &Cx<'_, H>, dt: f32) {
        let focused = self.focused_grid(cx.focus.current);
        let grid_live = self.snap.pos > 0.5;
        for row in 0..self.grid.shelves.len() {
            let count = self
                .hub(view, row)
                .map_or(0, |h| h.items.len().min(MAX_ITEMS));
            let col = focused
                .filter(|&(r, _)| grid_live && r == row)
                .map(|(_, c)| c);
            let sty = self.style(row);
            let Some(shelf) = self.grid.shelves.get_mut(row) else { continue };
            // A row at exact rest is a fixed point of `update`: skipping it changes no bit.
            if col.is_none() && shelf.at_exact_rest() {
                continue;
            }
            shelf.update(count, col, sty, dt);
        }
        let revealed = focused.map(|(row, _)| row).or_else(|| self.focused_heading(cx.focus.current));
        if let Some(row) = revealed.filter(|r| *r < self.rows.len() && !self.free) {
            self.grid.scroll_target = self.row_scroll(row);
        }
        self.grid
            .scroll_y
            .step(self.grid.scroll_target, K_SCROLL, dt);
        self.layout_grid();
    }

    fn tick<H: HomeLike>(&mut self, t: Tick, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        let dt = t.dt();
        self.sync_catalog(cx);
        let view = H::hubs(cx);
        let cta_available = hero_group_len(view) > 0;
        if cta_available
            && !self.cta_available
            && !self.strip_chosen
            && cx
                .focus
                .current
                .is_none_or(|key| key.elem >= crate::ui::dispatch::STRIP_BASE)
        {
            self.reseat(FocusTarget::ContainerGroup(HERO_GROUP), cx, fx);
        }
        self.cta_available = cta_available;
        if self.rows.is_empty() {
            self.snap_target = 0.0;
            self.snap.jump(0.0);
        }
        let current = crate::catalog::client_for(crate::catalog::current_server()).map(|c| c.machine_id());
        // `|` not `||`: both watches must re-read.
        if self.plaintext.refresh(current, Near::First) | self.clock.refresh(current) {
            fx.invalidate(Provenance::Landing(fx.from()));
        }
        if self.plaintext_alert.is_open()
            && !(status_read(view).is_some_and(|(_, kind, _)| kind == StatusKind::Failed)
                && self.plaintext_alert.subject() == self.plaintext.verdict().map(|v| v.machine_id.as_str())
                && plaintext_question::asks(self.plaintext.verdict()))
        {
            // the read-out it was asked from is gone, or no longer asks about that server
            self.plaintext_alert.withdraw();
        }
        self.plaintext_alert.update(dt);
        self.status_ms = (self.status_ms + dt * 1000.0)
            % (crate::ui::widgets::Spinner::PERIOD_MS as f32 * 1000.0);
        self.hero_flip_cd = (self.hero_flip_cd - dt).max(0.0);
        if self.outgoing.is_some() {
            self.hero_slide.step(1.0, K_FADE, dt);
            if (1.0 - self.hero_slide.pos).abs() < HERO_FADE_REST {
                self.hero_slide.jump(1.0);
                self.outgoing = None;
            }
        }
        if self.covered {
            // Compact surfaces keep ticking their host page, unlike Sheet/Alert/Opaque surfaces.
            // Cover/Uncover is the shared modal lifecycle across BOTH policies, so retain the
            // countdown here and restart it exactly once when the last surface is dismissed.
        } else if self.hero_pinned {
            // Held by `HomeCmd::PinHero`: no countdown, no flip.
        } else if self.snap.pos < 0.05 && view.hero_count() > 1 {
            // Spelled as an assignment, not `-= dt`: bit-for-bit identical arithmetic to the
            // pre-D4 accumulator, deliberately UNCHANGED — `hero_auto` is HASHED `LogicalState`
            // (`SHAPE`'s `hero_auto:f32`) across three committed replay fixtures, and a
            // `motion::Ramp`'s absolute-`Tick.ms` math computes the same real quantity through a
            // different float operation sequence that measurably diverges the hash (verified:
            // `tests/focusfp.sh --replay` on flow 1 disagreed from frame 7 on, once the arithmetic
            // changed).
            //
            // It reports NO `Motion`, deliberately. Nothing draws `hero_auto`: it is a TIMER, not
            // an animator (the `Timer` class of `docs/retui-invalidation-design.md`, not its
            // `Ramp`), and the dispatcher delivers `Tick` to the page every loop iteration whether or not the
            // frame presents (`nj_machine::idle`, "What this module does NOT do"), so the countdown runs
            // whatever the gate does, and the flip it ends in reports itself — `outgoing` is set,
            // and `moving` below reports the crossfade. What keeps a hero left alone presenting is
            // the Ken Burns zoom below, never this timer: the flip restarts the zoom every 8 s, short
            // of its 12, so that zoom never runs out while the hero is on screen. That is Litefin's
            // look and costs a full-rate present (~24 ms of GPU a frame on the TV). Whatever puts the
            // hero away (a cover, the pin, the rows, a single slide) ends the flips; the zoom then
            // holds or runs out its 12 s, and Home goes idle.
            // `a_hero_left_alone_zooms_every_frame_and_flips_on_time` and
            // `the_hero_is_the_only_thing_that_keeps_home_presenting` hold the two halves.
            self.hero_auto = self.hero_auto - dt;
            if self.hero_auto <= 0.0 {
                self.hero_flip_cd = 0.0;
                self.flip(view, 1);
            }
        } else {
            self.hero_auto = HERO_AUTO_S;
        }
        self.snap
            .step(pinned_snap(self.snap_target, self.rows.len()), K_SNAP, dt);
        // ONE answer for "is the dive still running", read by the present gate below. See
        // `SNAP_REST_POS`/`SNAP_REST_VEL` for why both terms are needed.
        let snap_moving = (self.snap.pos - self.snap_target).abs() > SNAP_REST_POS
            || self.snap.vel.abs() > SNAP_REST_VEL;
        let engine_on_grid = self.focused_loc(cx.focus.current).is_some_and(Located::on_grid);
        let picture_is_grid = self.snap.pos >= 0.5;
        if engine_on_grid == picture_is_grid {
            self.visible_activation = None;
        }
        let visible_focus = self.visible_focus(cx.focus.current);
        let hero_focused = matches!(self.focused_loc(visible_focus), Some(Located::Hero(_)))
            && status_read(view).is_none();
        // decorative: the band tints the hero in place and moves no layout, so it must not hold
        // the page-freeze snapshot (`idle::decorative`)
        nj_machine::idle::decorative(|| {
            let band = f32::from(hero_focused);
            self.hero_band.step(band, K_BAND, dt);
            self.hero_band.land(band, dt);
        });
        let visible_cx: Cx<'_, H> = Cx {
            views: cx.views,
            tick: cx.tick,
            measure: cx.measure,
            press: cx.press,
            focus: nj_machine::machine::FocusRead {
                current: visible_focus,
            ..Default::default() },
            owner: cx.owner,
        };
        self.update_grid(view, &visible_cx, dt);

        let selected = self.selected_hero(view);
        let outgoing = self.outgoing_hero(view);
        let art_on_screen = self.page_y() < HERO_ART_END;
        self.backdrop.update(selected, outgoing, self.carousel.as_ref(), art_on_screen, dt);
        self.prefetch(view);
        // Ken Burns: the slide on screen grows for its first `ZOOM_S` seconds, while the page is
        // uncovered and its art in view; never under the screenshot pin, which wants a still frame.
        // With several slides every flip restarts it before it is whole, so a hero left on screen
        // presents every frame (Litefin's look) and this is the term that keeps it doing so.
        let zooming = !self.covered
            && !self.hero_pinned
            && art_on_screen
            && self.hero_zoom < landing_hero::ZOOM_S
            && selected.is_some_and(|h| !h.item.art.is_empty());
        if zooming {
            self.hero_zoom = (self.hero_zoom + dt).min(landing_hero::ZOOM_S);
        }

        let moving = self.outgoing.is_some()
            || snap_moving
            || zooming
            || matches!(status_read(view), Some((_, StatusKind::Working, _)));
        if moving {
            fx.note(PresentEvent::Motion);
        }
        // Route-owned work is queued only after this read-only step has completed.
        fx.push(Fx::App(AppFx::StoreWork(StoreWork::Hubs)));
        fx.push(Fx::App(AppFx::StoreWork(StoreWork::BrowseDiscovery)));
    }

    /// **The billboard's neighbours are HELD, not hoped for.** Both backdrops a flip can land on
    /// (the carousel's ±[`HERO_PREFETCH`] slots, wrapping) are asked for through the DRAW path every
    /// frame the billboard is up — from the moment an index is shown, busy pipeline or not, and
    /// through the crossfade, so a flip re-arms its new neighbours on the flip itself. One mechanism
    /// for the manual flip and the auto-advance alike: both call [`Self::flip`], and both land on
    /// whatever this made resident.
    ///
    /// Why a draw probe and not a `warm`: a warm is speculation the source may drop — it runs only
    /// into an idle pipeline, takes LRU age 0 (the FIRST victim of the next miss, so the two
    /// neighbours and their logos evicted one another in a full store), and a warm never revives a
    /// slot the render cache released. That was the manual-flip blink (owner report 2026-09-30):
    /// a press within a second or so of the last flip found its backdrop not even requested, the
    /// outgoing art slid off over the flat ground and the new one faded in afterwards, while the
    /// 8 s countdown always gave the warm time to land. A probe is demand — LRU-touched,
    /// evict-protected, re-armed after a release — and still goes through the source's queue and
    /// the frame's upload [`Budget`] like every other picture. Its cost is the two neighbours'
    /// residency (≈7.4 MB of the 44 MB ceiling) while Home's billboard is up.
    ///
    /// The neighbours' clearLogos stay speculative: a late logo is a small pop, not a blank panel.
    fn prefetch(&self, view: HubsView<'_>) {
        use crate::ui::tex::Warm;
        if !neighbours_armed(self.snap.pos) {
            return;
        }
        let mut order = [0i32; 2 * HERO_PREFETCH];
        let count = prefetch_order(
            self.carousel_index(view) as i32,
            view.hero_count() as i32,
            &mut order,
        );
        let neighbours = || order[..count].iter().filter_map(|&i| view.hero(i as usize));
        for hero in neighbours() {
            crate::ui::widgets::resolve_tex_wh_on(hero.item.sid.raw(), &hero.item.art, 1280, 720, 0);
        }
        if !(prefetch_armed(self.snap.pos, self.outgoing.is_some())
            && crate::ui::tex::source_idle())
        {
            return;
        }
        for hero in neighbours() {
            if crate::ui::tex::logo_warm(hero.item.sid.raw(), hero_logo_rk(hero.item))
                == Warm::Claimed
            {
                return;
            }
        }
    }

    fn reseat<H: HomeLike>(&mut self, focus: FocusTarget<u32>, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        // Enter is queued behind this frame's Tick. Consume the CTA availability observed by
        // this seating request now, so that Tick cannot queue an implicit initial Hero seat
        // over the accepted target while the engine still holds its previous focus.
        self.cta_available = hero_group_len(H::hubs(cx)) > 0;
        fx.push(Fx::Deliver(
            MachineId::Instance(self.instance),
            Delivery::Screen(ScreenEvent::Enter(Enter::Fresh { focus })),
        ));
    }

    fn emit_item_menu<H: HomeLike>(&self, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> Handled {
        if self.snap.pos < 0.5 {
            return Handled::No;
        }
        let Some((row, col)) = self.focused_grid(cx.focus.current) else {
            return Handled::No;
        };
        let Some(item) = self.item_at(H::hubs(cx), row, col) else {
            return Handled::No;
        };
        // a library tile is no item to act on
        if item.rk.is_empty() || self.my_media(row) {
            return Handled::No;
        }
        fx.push(Fx::App(AppFx::Home(HomeReq::ItemMenu {
            sid: item.sid,
            rk: item.rk.clone(),
        })));
        fx.invalidate(Provenance::Input);
        Handled::Yes
    }

    fn command<H: HomeLike>(
        &mut self,
        command: HomeCmd,
        cx: &Cx<'_, H>,
        fx: &mut Effects<'_, H>,
    ) -> Handled {
        match command {
            HomeCmd::FocusGrid { row, col } => {
                let Some(key) = self.dev_focus_key(row, col, cx) else {
                    return Handled::No;
                };
                self.reseat(FocusTarget::Elem(key), cx, fx);
            }
            HomeCmd::Hero => {
                self.strip_chosen = false;
                self.reseat(FocusTarget::ContainerGroup(HERO_GROUP), cx, fx);
            }
            HomeCmd::Flip(direction) => {
                if direction == 0 || !self.flip(H::hubs(cx), direction.signum()) {
                    return Handled::No;
                }
                fx.invalidate(Provenance::Input);
            }
            HomeCmd::SelectHero(index) | HomeCmd::PinHero(index) => {
                let view = H::hubs(cx);
                if view.hero_count() == 0 {
                    return Handled::No;
                }
                let index = (index.max(0) as usize).min(view.hero_count() - 1);
                let Some(hero) = view.hero(index) else {
                    return Handled::No;
                };
                let Some(identity) = Self::identity_of(hero.item) else {
                    return Handled::No;
                };
                self.carousel = Some(identity);
                self.outgoing = None;
                self.hero_slide.jump(1.0);
                self.hero_zoom = 0.0;
                if matches!(command, HomeCmd::PinHero(_)) {
                    self.hero_pinned = true;
                    nj_base::eventlog::log(&format!("home: hero pinned at slot {index}"));
                }
                fx.invalidate(Provenance::Input);
            }
            HomeCmd::ItemMenu => return self.emit_item_menu(cx, fx),
        }
        Handled::Yes
    }

    fn activate<H: HomeLike>(&mut self, elem: u32, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        let view = H::hubs(cx);
        if status_read(view).is_some_and(|(_, kind, _)| kind == StatusKind::Failed)
            && elem == HERO_ELEM
            && plaintext_question::asks(self.plaintext.verdict())
        {
            // The server is on this network and only answers without encryption: *Connect*
            // asks the shared question rather than retrying what cannot succeed.
            if let Some(v) = self.plaintext.verdict() {
                let machine = v.machine_id.clone();
                let sid = crate::catalog::id_of_machine(&machine);
                self.plaintext_alert.open(&machine, sid, MachineId::Instance(self.instance), fx);
                fx.invalidate(Provenance::Input);
            }
            return;
        }
        if status_read(view).is_some() && elem == HERO_ELEM {
            fx.push(Fx::App(AppFx::Store(
                StoreId::Hubs,
                StoreCmd::Hubs(HubsCmd::Retry),
            )));
            return;
        }
        let elem = self.activation_elem(elem);
        let Some(located) = self.locate(elem) else {
            return;
        };
        let (item, req) = match located {
            // OK on the hero opens the slide's detail page, as Litefin's does
            Located::Hero(_) => {
                let Some(item) = self.selected_hero(view).map(|h| h.item) else {
                    return;
                };
                (
                    item,
                    HomeReq::Detail {
                        sid: item.sid,
                        rk: item.rk.clone(),
                    },
                )
            }
            Located::Heading(row) => {
                // OK on a linked heading opens the collection it names. BACK from that page
                // returns here: the heading is an ordinary engine key the return state restores.
                if let Some(link) = self.linked(row) {
                    fx.push(Fx::App(AppFx::Content(ContentReq::Push(ContentArg::Collection(
                        link.clone(),
                    )))));
                }
                return;
            }
            Located::Item(row, col) => {
                let Some(item) = self.item_at(view, row, col) else {
                    return;
                };
                if self.my_media(row) {
                    // a My Media tile opens THAT library, by its section identity
                    let section = crate::screens::registry::LibrarySectionIdentity { sid: item.sid, key: item.sec };
                    (item, HomeReq::Library(section))
                } else if self
                    .rows
                    .get(row)
                    .is_some_and(|h| h.identity == HomeHubIdentity::ContinueWatching)
                {
                    let resume_ns =
                        crate::metadata::resume_ns(item.resume_ms, item.dur_ns / 1_000_000);
                    (
                        item,
                        HomeReq::Play {
                            sid: item.sid,
                            rk: item.rk.clone(),
                            resume_ns,
                        },
                    )
                } else {
                    (
                        item,
                        HomeReq::Detail {
                            sid: item.sid,
                            rk: item.rk.clone(),
                        },
                    )
                }
            }
        };
        if !item.rk.is_empty() {
            fx.push(Fx::App(AppFx::Home(req)));
        }
    }

    fn draw_page<H: HomeLike>(&mut self, f: &mut DrawFrame<'_, '_, H>) {
        nj_gfx::gfx::frame_clear(theme::CLEAR_RGB.0, theme::CLEAR_RGB.1, theme::CLEAR_RGB.2);
        let view = H::hubs(f.cx);
        let visible_focus = self.visible_focus(f.focus.current);
        let env = self.env(0.0);
        let focus = self.focused_loc(visible_focus);
        let grid_focus = self.focused_grid(visible_focus);
        let p = f.painter.alpha(f.page_alpha);
        let page_y = self.page_y();
        // Re-bind the art to the heroes the crossfade is about: an event delivered after this
        // frame's `Tick` (a manual flip) has changed them since `update` ran (`Backdrop::bind`).
        self.backdrop.bind(
            self.selected_hero(view),
            self.outgoing_hero(view),
            self.carousel.as_ref(),
            page_y < HERO_ART_END,
        );
        // the hero is the top of the page and rises with it
        let hero = p.translate(0.0, -page_y);
        if page_y < HERO_ART_END {
            crate::ui::profile::phase("hm.backdrop", || {
                self.backdrop.draw(hero, self.crossfade(), self.zooms())
            });
        }
        if page_y < DOTS_FOOT {
            crate::ui::profile::phase("hm.hero", || self.draw_hero(view, &env, hero, f.measure));
        }
        if page_y < HERO_ART_END {
            crate::ui::widgets::hero_band(hero, self.hero_band.pos);
        }
        crate::ui::profile::phase("hm.grid", || {
            let heading = self.focused_heading(visible_focus);
            self.draw_grid(view, &env, p, f.press.scale, grid_focus, heading, f.measure)
        });
        crate::ui::profile::phase("hm.status", || self.draw_status(view, &env, p, focus));
        crate::ui::testpat::underlay(p);
        crate::ui::testpat::draw(p);
        if !self.plaintext_alert.visible() {
            // the question owns the pointer while it is up; nothing under it is a target
            self.record_stops(f, view);
        }
        self.plaintext_alert.draw(f, self.entry);
    }

    /// The slide's content block and the pager. A crossfade fades the outgoing block out as it
    /// sinks [`HERO_TEXT_RISE`] and the incoming one in as it rises the same.
    fn draw_hero(&self, view: HubsView<'_>, env: &Env, p: Painter, measure: &dyn Measure) {
        if status_read(view).is_some() {
            return;
        }
        let Some(hero) = self.selected_hero(view) else {
            return;
        };
        match (self.crossfade(), self.outgoing_hero(view)) {
            (Some(t), Some(old)) => {
                hero_content(old.item, display_source(old.source), p.alpha(1.0 - t).translate(0.0, HERO_TEXT_RISE * t), measure);
                hero_content(hero.item, display_source(hero.source), p.alpha(t).translate(0.0, HERO_TEXT_RISE * (1.0 - t)), measure);
            }
            _ => hero_content(hero.item, display_source(hero.source), p, measure),
        }
        if view.hero_count() > 1 {
            PageDots::new(view.hero_count())
                .active(self.carousel_index(view))
                .right_at(DOTS_RIGHT, DOTS_FOOT - PageDots::DOT)
                .draw(env, p);
        }
    }

    /// Whether shelf `row` overlaps the screen vertically, from its title's line box to the foot
    /// of its labels.
    fn shelf_on_screen(&self, row: usize) -> bool {
        on_axis(self.grid.shelf(row).base_y, self.pitch(row), SCR_H, 0.0)
    }

    /// Row `row`'s art for a card: a landscape row's still (an episode's own frame, else the
    /// backdrop, else the poster), a poster row's poster.
    fn art<'a>(&self, row: usize, item: &'a PmsMovie) -> Art<'a> {
        if self.rows.get(row).is_some_and(|r| r.landscape) {
            Art::Still(Some(tile_facts::of(item)))
        } else {
            Art::Poster(Some(tile_facts::of(item)))
        }
    }

    fn draw_grid(
        &self,
        view: HubsView<'_>,
        env: &Env,
        p: Painter,
        press_scale: f32,
        focused: Option<(usize, usize)>,
        heading: Option<usize>,
        measure: &dyn Measure,
    ) {
        for row in 0..self.rows.len() {
            let Some(hub) = self.hub(view, row) else {
                continue;
            };
            if !self.shelf_on_screen(row) {
                continue;
            }
            let cap_top = title_cap_top(self.grid.shelf(row).base_y, measure);
            if let Some(linked) = self.heading_widget(view, row) {
                let m = linked.measure(measure);
                let focus_t = f32::from(heading == Some(row) && env.sp > 0.5);
                linked.draw(p, ROW_TITLE_X, cap_top, focus_t, &m, measure);
            } else {
                card_row::draw_heading(p, hub.title, hub.source, ROW_TITLE_X, cap_top, f32::INFINITY, measure);
            }
            let sty = self.style(row);
            for (col, item) in hub.items.iter().take(MAX_ITEMS).enumerate() {
                if focused == Some((row, col)) && env.sp > 0.5 {
                    continue;
                }
                let x = card_x(sty, col, self.grid.scroll_x(row));
                if !on_axis(x, sty.w, SCR_W, GLOW_PAD) {
                    continue;
                }
                let (rect, scale) = self.drawn_card_geometry(row, col, 1.0);
                let label = card_row::TileLabel::default();
                card_row::draw_cell(p, self.art(row, item), rect, scale, sty, item.resume_frac(), &label, false, measure);
            }
        }
        self.draw_focused_cell(view, env, p, press_scale, focused, measure);
    }

    fn draw_focused_cell(
        &self,
        view: HubsView<'_>,
        env: &Env,
        p: Painter,
        press_scale: f32,
        focused: Option<(usize, usize)>,
        measure: &dyn Measure,
    ) {
        let Some((row, col)) = focused.filter(|_| env.sp > 0.5) else {
            return;
        };
        let Some(hub) = self.hub(view, row) else {
            return;
        };
        let Some(item) = hub.items.get(col) else {
            return;
        };
        let (rect, scale) = self.drawn_card_geometry(row, col, press_scale);
        let count = hub.items.len().min(MAX_ITEMS);
        let sty = self.style(row);
        // the card names itself from its row; its label's width is judged where the glide rests it
        let label = card_row::TileLabel::default().settling(self.grid.shelf(row).settle_lag(count, col, sty));
        card_row::draw_cell(p, self.art(row, item), rect, scale, sty, item.resume_frac(), &label, true, measure);
    }

    /// One drawn rectangle for the tile painter and the input map, including the press bounce.
    fn drawn_card_geometry(&self, row: usize, col: usize, press_scale: f32) -> (Rect, f32) {
        let sty = self.style(row);
        let scale = self.grid.shelf(row).scale(col)
            * if press_scale > 0.0 { press_scale } else { 1.0 };
        let rect = Rect::new(
            card_x(sty, col, self.grid.scroll_x(row)),
            self.grid.shelf(row).base_y + ROW_CARDS_DY,
            sty.w,
            sty.h,
        )
        .scaled(scale);
        (rect, scale)
    }

    fn draw_status(&self, view: HubsView<'_>, env: &Env, p: Painter, focus: Option<Located>) {
        let Some(overlay) = status_overlay(view, &self.plaintext, &self.clock) else {
            return;
        };
        overlay
            .phase(self.status_ms as u32)
            .focused(focus == Some(Located::Hero(0)))
            .draw(env, p);
    }

    pub(crate) fn record_stops<H: HomeLike>(&self, f: &mut DrawFrame<'_, '_, H>, view: HubsView<'_>) {
        if !f.records_stops() { return; }
        let focus = f.focus.current;
        if self.snap.pos < 0.5 && hero_group_len(view) > 0 {
            if let Some(placed) = Focusable::<H>::place(self, &HERO_ELEM, f.cx, At::Drawn) {
                // The hero takes the pointer over its whole face above the rows; a read-out only
                // over its action.
                let rect = if status_read(view).is_some() {
                    placed.rect
                } else {
                    Rect::new(0.0, -self.page_y(), SCR_W, ROWS_OVERLAP_TOP)
                };
                f.stop(
                    f.painter,
                    Stop {
                        key: FocusKey { entry: self.entry, elem: HERO_ELEM },
                        rect,
                        rest_rect: rect,
                        clip: placed.clip,
                        hover: Hover::Focus,
                        activate: Activate::Press,
                    },
                );
            }
        }
        if self.snap.pos >= 0.5 {
            let focused_row = self.focused_grid(focus).map(|(row, _)| row);
            for (row_index, row) in self.rows.iter().enumerate() {
                // Keep the whole focused row hoverable while vertical reveal is in flight.
                // Other partially visible rows must not steal focus as they pass the pointer.
                let row_focused = focused_row == Some(row_index);
                // A row off screen has nothing to hit; only the focused one keeps its stops.
                if !row_focused && !self.shelf_on_screen(row_index) {
                    continue;
                }
                for &elem in &row.elems {
                    let Some(placed) = Focusable::<H>::place(self, &elem, f.cx, At::Drawn) else {
                        continue;
                    };
                    let fully_visible = placed.rest_rect.y >= 40.0
                        && placed.rest_rect.y + placed.rest_rect.h <= SCR_H - 20.0;
                    let hover = if fully_visible || row_focused {
                        Hover::Focus
                    } else {
                        Hover::OnlyIfFocused
                    };
                    f.stop(
                        f.painter,
                        Stop {
                            key: FocusKey {
                                entry: self.entry,
                                elem,
                            },
                            rect: placed.rect,
                            rest_rect: placed.rest_rect,
                            clip: placed.clip,
                            hover,
                            activate: Activate::Press,
                        },
                    );
                }
                // After the row's tiles, so a popped tile's glow never wins the heading's hit.
                if let (Some(heading), Some(rect)) = (
                    self.heading_widget(view, row_index),
                    self.heading_rect(view, row_index, f.measure, At::Drawn, focus),
                ) {
                    let visible = rect.y >= 0.0 && rect.y + rect.h <= SCR_H;
                    if visible {
                        heading.stop(f, rect, FocusKey { entry: self.entry, elem: heading_elem(row.group) });
                    }
                }
            }
        }
    }

    pub(crate) fn hero_item<'a, H: HomeLike>(&self, cx: &Cx<'a, H>) -> Option<&'a PmsMovie> {
        self.selected_hero(H::hubs(cx)).map(|h| h.item)
    }

    pub(crate) fn focused_item<'a, H: HomeLike>(
        &self,
        focus: Option<FocusKey<u32>>,
        cx: &Cx<'a, H>,
    ) -> Option<&'a PmsMovie> {
        let view = H::hubs(cx);
        match self.focused_loc(focus)? {
            Located::Hero(_) => self.selected_hero(view).map(|h| h.item),
            Located::Item(row, col) => self.item_at(view, row, col),
            Located::Heading(_) => None,
        }
    }

    pub(crate) fn grid_position<H: HomeLike>(
        &self,
        focus: Option<FocusKey<u32>>,
        _cx: &Cx<'_, H>,
    ) -> Option<(usize, usize)> {
        self.focused_grid(focus)
    }

    /// How many cards shelf `row` holds, once the catalog has projected it.
    #[cfg(feature = "devtriggers")]
    pub(crate) fn shelf_len(&self, row: usize) -> Option<usize> {
        self.rows.get(row).map(|projection| projection.elems.len())
    }

    /// Diagnostic view of the actual animated state: snap, retained shelf offset,
    /// shelf velocity. No second motion owner or stored telemetry state.
    #[cfg(feature = "devtriggers")]
    pub(crate) fn motion_witness(&self, row: usize) -> Option<[f32; 3]> {
        self.rows.get(row)?;
        Some([self.snap.pos, self.grid.shelf(row).scroll_x(), self.grid.shelf(row).scroll_velocity()])
    }

    pub(crate) fn snap_target(&self) -> f32 {
        self.snap_target
    }

    /// Where the hero-to-grid dive actually IS this frame (`snap_target` is where it is going).
    /// The frame-drop line carries both, so a slow frame can be placed on the dive's curve.
    pub(crate) fn snap_pos(&self) -> f32 {
        self.snap.pos
    }

    pub(crate) fn focused_rect<H: HomeLike>(
        &self,
        focus: Option<FocusKey<u32>>,
        cx: &Cx<'_, H>,
        at: At,
    ) -> Option<Rect> {
        let key = focus.filter(|k| k.entry == self.entry)?;
        self.focused_grid(Some(key))?;
        Focusable::<H>::place(self, &key.elem, cx, at).map(|p| p.rect)
    }

    pub(crate) fn redraw_focused<H: HomeLike>(
        &self,
        f: &mut DrawFrame<'_, '_, H>,
        focus: Option<FocusKey<u32>>,
    ) {
        let Some(key) = focus.filter(|k| k.entry == self.entry) else {
            return;
        };
        let Some((row, col)) = self.focused_grid(Some(key)) else {
            return;
        };
        let view = H::hubs(f.cx);
        let env = self.env(0.0);
        let p = f.painter.alpha(f.page_alpha);
        let _clip = f.clip(p, Rect::new(0.0, 0.0, SCR_W, SCR_H));
        self.draw_focused_cell(view, &env, p, f.press.scale, Some((row, col)), f.measure);
    }

    pub(crate) fn dev_focus_key<H: HomeLike>(
        &self,
        row: usize,
        col: usize,
        _cx: &Cx<'_, H>,
    ) -> Option<FocusKey<u32>> {
        Some(FocusKey {
            entry: self.entry,
            elem: *self.rows.get(row)?.elems.get(col)?,
        })
    }
}

impl LogicalState for HomeScreen {
    fn write(&self, c: &mut Canon) {
        // Exhaustive field census. Entry/instance identity is encoded by Navigation; spinner
        // phase and backdrop resources only paint. Every motion value queried by input/reveal
        // is encoded below, including velocities that determine the next Tick's answer.
        let Self { entry: _, instance: _, groups: _, items: _, next_group: _, next_elem: _,
            // `elem_at` is derived from `rows`, which the census already covers via its elems.
            // `item_index` / `group_index` are caches of `items` / `groups`, which are covered.
            rows: _, elem_at: _, item_index: _, group_index: _, projected_generation: _, projection_stale: _, volatile_keys: _, restored_scroll: _, restore_reveal: _, carousel: _,
            outgoing: _, hero_flip_cd: _, hero_slide: _, hero_auto: _, hero_pinned: _, covered: _,
            hero_zoom: _, hero_band: _,
            // Where the outgoing slide's zoom stopped: it only paints while that slide fades.
            outgoing_zoom: _,
            snap_target: _,
            // Whether the tick pulls the scroll to the focused shelf. What it decides is
            // `grid.scroll_target`, encoded below, and it is a hand-off from the pointer's events.
            free: _,
            // A press hand-off from a click to its commit, like `free`.
            reveal_press: _,
            visible_activation: _, cta_available: _, strip_chosen: _, snap: _, status_ms: _,
            backdrop: _, grid: _,
            // The plaintext question is the grant table's (`plex::grant::offers`), which no
            // recording can raise, and its alert only paints over a failed read-out. The clock
            // fact is `net::keypin`'s, likewise unraisable by a recording.
            plaintext: _, plaintext_alert: _, clock: _ } = self;
        c.f32(self.snap_target)
            .f32(self.snap.pos).f32(self.snap.vel)
            .f32(self.hero_flip_cd)
            .f32(self.hero_auto)
            .bool(self.covered)
            .option(self.visible_activation, |c, elem| {
                c.u32(elem);
            })
            .bool(self.cta_available)
            .bool(self.strip_chosen)
            .u32(self.next_group)
            .u32(self.next_elem)
            .seq(self.groups.len());
        for key in &self.groups {
            write_hub_identity(&key.identity, c);
            c.u32(key.group);
        }
        c.seq(self.items.len());
        for key in self.items.iter() {
            write_item_identity(&key.identity, c);
            c.u32(key.elem).u32(key.last_row).u32(key.last_col);
        }
        c.option(self.carousel.as_ref(), |c, (sid, rk)| {
            c.u32(u32::from(sid.raw())).str(rk);
        });
        c.option(self.outgoing.as_ref(), |c, (sid, rk)| {
            c.u32(u32::from(sid.raw())).str(rk);
        });
        c.f32(self.hero_slide.pos).f32(self.hero_slide.vel)
            .f32(self.hero_band.pos).f32(self.hero_band.vel).f32(self.hero_zoom);
        c.option(self.projected_generation, |c, generation| { c.u32(generation); });
        c.f32(self.grid.scroll_y.pos).f32(self.grid.scroll_y.vel)
            .f32(self.grid.scroll_target).seq(self.rows.len());
        for (index, row) in self.rows.iter().enumerate() {
            write_hub_identity(&row.identity, c);
            c.u32(row.group.0).seq(row.elems.len());
            for elem in &row.elems { c.u32(*elem); }
            self.grid.shelf(index).write_motion(c);
        }
        c.seq(self.restored_scroll.len());
        for &(group, scroll) in &self.restored_scroll { c.u32(group).f32(scroll); }
        c.bool(self.restore_reveal);
    }

    fn probe(&self, out: &mut String) {
        out.push_str("home");
    }
}

impl<H: HomeLike> Focusable<H> for HomeScreen {
    fn groups(&self, cx: &Cx<'_, H>, out: &mut Vec<GroupSpec>) {
        if self.plaintext_alert.groups(out) {
            return;
        }
        let view = H::hubs(cx);
        out.push(GroupSpec {
            id: HERO_GROUP,
            kind: GroupKind::Row { wrap: false },
            seat: Seat::First,
            // The hero is reached only through declared strip/first-shelf links. Its fixed
            // billboard extent remains on the page while folded and must never win a geometric
            // search from a lower shelf. RIGHT pages the slides before the engine sees it
            // (`step`); LEFT finds no group and takes the rail's door.
            reachable: AxisMask(0),
            edge: [
                EdgeRule::Geometric,
                EdgeRule::Geometric,
                EdgeRule::Stop,
                EdgeRule::Stop,
            ],
            extent: self.hero_rect(view, cx.measure).unwrap_or(HERO_SEAT),
            len: hero_group_len(view),
            elem: ElemKind::Control,
        });
        for (row, projection) in self.rows.iter().enumerate() {
            let top = ROW_FOCUS_TOP + self.flow(row) - self.grid.scroll_target;
            let sty = self.style(row);
            let linked = self.linked(row).is_some();
            if linked {
                if let Some(rect) =
                    self.heading_rect(view, row, cx.measure, At::SpringTarget, cx.focus.current)
                {
                    out.push(linked_heading::group_spec(heading_group(projection.group), rect));
                }
            }
            out.push(GroupSpec {
                id: projection.group,
                kind: GroupKind::Row { wrap: false },
                // DOWN from the linked heading returns to the card this shelf remembered;
                // every other door keeps the shelf's nearest-column projection.
                seat: if linked {
                    linked_heading::shelf_seat(
                        &cx.focus,
                        heading_elem(projection.group),
                        projection.group,
                        Seat::Nearest,
                    )
                } else {
                    Seat::Nearest
                },
                reachable: AxisMask::BOTH,
                edge: [
                    EdgeRule::Geometric,
                    EdgeRule::Geometric,
                    EdgeRule::Stop,
                    EdgeRule::Stop,
                ],
                extent: Rect::new(
                    sty.margin_x,
                    top + ROW_CARDS_DY,
                    SCR_W - sty.margin_x,
                    sty.h + card_row::TileLabel::height(!self.my_media(row)),
                ),
                len: projection.elems.len(),
                elem: ElemKind::Card,
            });
        }
    }

    fn group_of(&self, key: &u32, cx: &Cx<'_, H>) -> Option<GroupId> {
        if let Some(answer) = self.plaintext_alert.group_of(*key) {
            return answer;
        }
        match self.locate(*key)? {
            Located::Hero(index) => (index < hero_group_len(H::hubs(cx))).then_some(HERO_GROUP),
            Located::Item(row, _) => self.rows.get(row).map(|r| r.group),
            Located::Heading(row) => self.rows.get(row).map(|r| heading_group(r.group)),
        }
    }

    fn neighbour(&self, key: FocusKey<u32>, dir: Dir, _cx: &Cx<'_, H>) -> Step<u32> {
        if let Some(step) = self.plaintext_alert.neighbour(key, dir) {
            return step;
        }
        let next = match self.locate(key.elem) {
            Some(Located::Item(row, col)) => match dir {
                Dir::Left if col > 0 => self
                    .rows
                    .get(row)
                    .and_then(|r| r.elems.get(col - 1))
                    .copied(),
                Dir::Right => self
                    .rows
                    .get(row)
                    .and_then(|r| r.elems.get(col + 1))
                    .copied(),
                _ => None,
            },
            // One element, LEFT/RIGHT inert; UP/DOWN are the group's doors.
            Some(Located::Hero(_) | Located::Heading(_)) | None => None,
        };
        next.map(|elem| {
            Step::Move(FocusKey {
                entry: key.entry,
                elem,
            })
        })
        .unwrap_or(Step::Edge)
    }

    fn place(&self, key: &u32, cx: &Cx<'_, H>, at: At) -> Option<Placed> {
        if let Some(placed) = self.plaintext_alert.place(*key) {
            return placed;
        }
        let view = H::hubs(cx);
        match self.locate(*key)? {
            Located::Hero(index) => {
                let mut base = self.hero_rect(view, cx.measure)?;
                // The hero rises with the page; a read-out's action stands still. Neither pops:
                // the hero's focus is its band.
                if status_read(view).is_none() && matches!(at, At::Drawn) {
                    base.y -= self.page_y();
                }
                Some(Placed { rect: base, rest_rect: base, clip: Rect::FULL, index: Some(index as u32) })
            }
            Located::Heading(row) => {
                let rect = self.heading_rect(view, row, cx.measure, at, cx.focus.current)?;
                Some(Placed {
                    rect,
                    rest_rect: rect,
                    clip: Rect::new(0.0, 0.0, SCR_W, SCR_H),
                    index: None,
                })
            }
            Located::Item(row, col) => {
                let shelf = self.grid.shelves.get(row)?;
                let sty = self.style(row);
                let y = match at {
                    At::Drawn => shelf.base_y,
                    At::SpringTarget => ROW_FOCUS_TOP + self.flow(row) - self.grid.scroll_target,
                };
                let base = Rect::new(card_x(sty, col, shelf.scroll_x()), y + ROW_CARDS_DY, sty.w, sty.h);
                let scale = match at {
                    At::Drawn => shelf.scale(col),
                    At::SpringTarget => {
                        if self.focused_grid(cx.focus.current) == Some((row, col)) {
                            sty.focus_scale
                        } else {
                            1.0
                        }
                    }
                };
                Some(Placed {
                    rect: if matches!(at, At::Drawn) {
                        let focused = self.snap.pos > 0.5
                            && self.focused_grid(self.visible_focus(cx.focus.current)) == Some((row, col));
                        self.drawn_card_geometry(row, col, if focused { cx.press.scale } else { 1.0 }).0
                    } else { base.scaled(scale) },
                    rest_rect: base.scaled(sty.focus_scale),
                    clip: Rect::new(0.0, 0.0, SCR_W, SCR_H),
                    index: Some(col as u32),
                })
            }
        }
    }

    fn reconcile(&self, want: FocusKey<u32>, cx: &Cx<'_, H>) -> FocusKey<u32> {
        if let Some(key) = self.plaintext_alert.reconcile(want) {
            return key;
        }
        if let Some(located) = self.locate(want.elem) {
            let valid = match located {
                Located::Hero(index) => index < hero_group_len(H::hubs(cx)),
                Located::Item(_, _) | Located::Heading(_) => true,
            };
            if valid {
                return want;
            }
        }
        if let Some(saved) = self.items.iter().find(|k| k.elem == want.elem) {
            let hub_identity = match &saved.identity {
                HomeItemIdentity::Item { hub, .. } | HomeItemIdentity::Slot { hub, .. } => hub,
            };
            if let Some(row) = self.rows.iter().find(|r| &r.identity == hub_identity)
                .or_else(|| self.rows.get((saved.last_row as usize).min(self.rows.len().saturating_sub(1)))) {
                let col = (saved.last_col as usize).min(row.elems.len().saturating_sub(1));
                if let Some(&elem) = row.elems.get(col) {
                    return FocusKey {
                        entry: want.entry,
                        elem,
                    };
                }
            }
        }
        self.rows
            .iter()
            .find_map(|r| r.elems.first().copied())
            .map(|elem| FocusKey {
                entry: want.entry,
                elem,
            })
            .unwrap_or(FocusKey {
                entry: want.entry,
                elem: HERO_ELEM,
            })
    }

    fn seat(&self, group: GroupId, from: Placed, _cx: &Cx<'_, H>) -> FocusKey<u32> {
        if let Some(key) = self.plaintext_alert.seat(group, self.entry) {
            return key;
        }
        if group == HERO_GROUP {
            return FocusKey {
                entry: self.entry,
                elem: HERO_ELEM,
            };
        }
        if let Some(row) = self.rows.iter().find(|r| heading_group(r.group) == group) {
            return FocusKey {
                entry: self.entry,
                elem: heading_elem(row.group),
            };
        }
        let Some((row_index, row)) = self.rows.iter().enumerate().find(|(_, r)| r.group == group)
        else {
            return FocusKey {
                entry: self.entry,
                elem: HERO_ELEM,
            };
        };
        let sty = self.style(row_index);
        let col = card_row::column_near_x(
            from.rect.cx(),
            sty.margin_x,
            sty.w + sty.gap,
            sty.w,
            self.grid.shelf(row_index).scroll_x(),
            row.elems.len(),
            from.index.unwrap_or(0) as usize,
        );
        FocusKey {
            entry: self.entry,
            elem: row.elems.get(col).copied().unwrap_or(HERO_ELEM),
        }
    }
}

impl HomeScreen {
    fn activation_elem(&self, engine_elem: u32) -> u32 {
        let engine_grid = self.locate(engine_elem).is_some_and(Located::on_grid);
        let picture_grid = self.snap.pos >= 0.5;
        if engine_grid != picture_grid {
            self.visible_activation.unwrap_or(engine_elem)
        } else {
            engine_elem
        }
    }

    /// The hero's focus rect: its logo box at a full synopsis (one rect for every slide), or under
    /// a status read-out the read-out's action. Seating from it lands on a row's first column.
    fn hero_rect(&self, view: HubsView<'_>, measure: &dyn Measure) -> Option<Rect> {
        if let Some((_, _, action)) = status_read(view) {
            action?;
            return status_overlay(view, &self.plaintext, &self.clock)?.action_frame_measured(measure);
        }
        self.selected_hero(view)?;
        Some(HERO_SEAT)
    }
}

impl<H: HomeLike> Machine<H> for HomeScreen {
    type Ev = ScreenEvent<H>;

    fn step(&mut self, ev: &Self::Ev, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> Handled {
        match self.plaintext_alert.step(ev, cx) {
            AlertStep::Pass => {}
            AlertStep::Done(handled) => return handled,
            AlertStep::Answer(cmd) => {
                if let Some(cmd) = cmd {
                    fx.push(Fx::App(AppFx::Session(cmd)));
                }
                self.reseat(FocusTarget::ContainerGroup(HERO_GROUP), cx, fx);
                fx.invalidate(Provenance::Input);
                return Handled::Yes;
            }
        }
        if let ScreenEvent::Input(InputEvent { kind: InputKind::Key { .. }, .. }) = ev {
            self.reveal_press = None;
        }
        // A click brings the page to what it clicked, the way a key's move does; a click on a
        // card the screen shows only part of does only that (`reveal_press`).
        if let ScreenEvent::Input(InputEvent { kind: InputKind::Click { hit: Some(elem), .. }, .. }) = ev {
            if let Some(loc) = self.locate(*elem) {
                self.reveal_press = matches!(loc, Located::Item(..))
                    .then(|| Focusable::<H>::place(self, elem, cx, At::Drawn))
                    .flatten()
                    .filter(|placed| placed.rect.y < 0.0 || placed.rect.y + placed.rect.h > SCR_H)
                    .map(|_| *elem);
                self.free = false;
                self.snap_target = if loc.on_grid() { 1.0 } else { 0.0 };
                if let Located::Item(row, _) = loc {
                    if let Some(shelf) = self.grid.shelves.get_mut(row) { shelf.unfree(); }
                }
                fx.invalidate(Provenance::Input);
            }
        }
        match ev {
            // The pointer's sideways glide moves the shelf under it, which stays where it put it
            // until a key moves along it or a click lands on one of its cards (`CardRow::glide`).
            ScreenEvent::Input(InputEvent { kind: InputKind::Pan { dx, y, .. }, .. }) => {
                if dx.is_finite() {
                    let under = (0..self.grid.shelves.len()).find(|&row| {
                        let top = self.grid.shelf(row).base_y;
                        self.shelf_on_screen(row) && *y >= top && *y < top + self.pitch(row)
                    });
                    if let (Some(row), Some(n)) = (under, under.and_then(|row| self.rows.get(row)).map(|hub| hub.elems.len())) {
                        let sty = self.style(row);
                        if let Some(shelf) = self.grid.shelves.get_mut(row) {
                            shelf.glide(*dx, n, sty);
                            fx.invalidate(Provenance::Input);
                        }
                    }
                }
                Handled::Yes
            }
            // A wheel (the pointer's edge glide, a fractional one every frame) moves the page
            // itself, focus staying where it is: down from the hero into the shelves, then through
            // them by a row's pitch; up through the shelves, then back to the hero.
            ScreenEvent::Input(InputEvent { kind: InputKind::Wheel { dy }, .. }) => {
                if !dy.is_finite() || self.rows.is_empty() {
                    return Handled::Yes;
                }
                self.free = true;
                let max = self.grid_max_scroll();
                let to = self.grid.scroll_target - dy * self.pitch(0);
                if *dy > 0.0 {
                    if self.snap_target >= 0.5 && self.grid.scroll_target > 0.0 {
                        self.grid.scroll_target = to.max(0.0);
                    } else {
                        self.snap_target = 0.0;
                    }
                } else if self.snap_target < 0.5 {
                    self.snap_target = 1.0;
                } else {
                    self.grid.scroll_target = to.min(max.max(self.grid.scroll_target));
                }
                fx.invalidate(Provenance::Input);
                Handled::Yes
            }
            ScreenEvent::Mount => {
                self.sync_catalog(cx);
                Handled::Yes
            }
            ScreenEvent::RestoreMemory(PageMemory::Home(memory)) => {
                self.restore(memory);
                self.sync_catalog(cx);
                self.cta_available = hero_group_len(H::hubs(cx)) > 0;
                Handled::Yes
            }
            ScreenEvent::Enter(_) => {
                self.sync_catalog(cx);
                fx.invalidate(Provenance::Nav);
                Handled::Yes
            }
            ScreenEvent::Cover => {
                self.covered = true;
                Handled::Yes
            }
            ScreenEvent::Uncover => {
                self.covered = false;
                self.hero_auto = HERO_AUTO_S;
                self.sync_catalog(cx);
                fx.invalidate(Provenance::Nav);
                Handled::Yes
            }
            ScreenEvent::Tick(tick) => {
                self.tick(*tick, cx, fx);
                Handled::Yes
            }
            ScreenEvent::StoreChanged(ord, _) if *ord == StoreId::Hubs.ord() => {
                self.projected_generation = None;
                self.sync_catalog(cx);
                if self.rows.is_empty() {
                    self.snap_target = 0.0;
                }
                fx.invalidate(Provenance::Landing(fx.from()));
                Handled::Yes
            }
            ScreenEvent::FocusMoved { from, to, by } => {
                let from_loc = from.and_then(|key| self.locate(key.elem));
                let to_loc = self.locate(to.elem);
                if let (Some(a), Some(b)) = (from_loc, to_loc) {
                    if a.on_grid() != b.on_grid() {
                        self.visible_activation = from.map(|key| key.elem);
                    }
                }
                // A hover moves focus, never the page: the pointer scrolls by the screen's edges
                // (and a click by what it clicked). Every other move brings the page to the focus.
                if *by == By::Pointer {
                    self.free = true;
                } else {
                    self.free = false;
                    match to_loc {
                        Some(Located::Hero(_)) => self.snap_target = 0.0,
                        Some(Located::Item(_, _) | Located::Heading(_)) => self.snap_target = 1.0,
                        None => {}
                    }
                    if let Some(Located::Item(row, _)) = to_loc {
                        if let Some(shelf) = self.grid.shelves.get_mut(row) { shelf.unfree(); }
                    }
                }
                if matches!(by, By::Restore) && self.restore_reveal {
                    // A restored page opens at its saved region, rather than replaying the
                    // hero-to-grid door with an invisible engine cursor during the transition.
                    self.restore_reveal = false;
                    self.snap.jump(self.snap_target);
                    self.visible_activation = None;
                    if let Some(Located::Item(row, col)) = to_loc {
                        // The saved viewport is a preference, not an assertion about today's
                        // catalog order. Rest the reconciled item where its row centres it before
                        // first draw.
                        if let (Some(shelf), Some(hub)) =
                            (self.grid.shelves.get_mut(row), self.rows.get(row))
                        {
                            let count = hub.elems.len();
                            let sty = style(hub.landscape);
                            let scroll = shelf.scroll_target(col, count, sty);
                            shelf.restore_scroll(scroll, count, sty);
                        }
                        self.grid.scroll_target = self.row_scroll(row);
                        self.grid.scroll_y.jump(self.grid.scroll_target);
                    }
                    self.layout_grid();
                }
                if to.elem >= crate::ui::dispatch::STRIP_BASE && matches!(by, By::Dir | By::Pointer)
                {
                    self.strip_chosen = true;
                } else if to_loc.is_some() {
                    self.strip_chosen = false;
                }
                if matches!(by, By::Dir | By::Pointer) {
                    self.hero_auto = HERO_AUTO_S;
                }
                fx.invalidate(Provenance::Input);
                Handled::Yes
            }
            ScreenEvent::PressCommit(_) => {
                if let Some(elem) = cx
                    .focus
                    .current
                    .filter(|k| k.entry == self.entry)
                    .map(|k| k.elem)
                {
                    // the click that brought a partly shown card into view does not open it
                    if self.reveal_press.take() != Some(elem) {
                        self.activate(elem, cx, fx);
                    }
                }
                Handled::Yes
            }
            ScreenEvent::Activate(elem) => {
                self.activate(*elem, cx, fx);
                Handled::Yes
            }
            ScreenEvent::PressHold(_) => self.emit_item_menu(cx, fx),
            ScreenEvent::App(AppMsg::Home(command)) => {
                self.sync_catalog(cx);
                self.command(*command, cx, fx)
            }
            // RIGHT on the hero pages its slides before the engine sees it; LEFT is the engine's,
            // whose door enters the rail from the hero as from any page's left edge.
            ScreenEvent::Input(InputEvent {
                kind:
                    InputKind::Key {
                        key: Key::Right,
                        edge: Edge::Down | Edge::Repeat,
                        at_edge: false,
                        ..
                    },
                ..
            }) if matches!(self.focused_loc(cx.focus.current), Some(Located::Hero(_)))
                && status_read(H::hubs(cx)).is_none() =>
            {
                if self.flip(H::hubs(cx), 1) {
                    fx.invalidate(Provenance::Input);
                }
                Handled::Yes
            }
            ScreenEvent::Input(InputEvent {
                kind:
                    InputKind::Key {
                        key: Key::Back,
                        edge: Edge::Down,
                        ..
                    },
                ..
            }) => {
                if self.snap_target >= 0.5
                    || self.snap.pos >= 0.5
                    || self.focused_loc(cx.focus.current).is_some_and(Located::on_grid)
                {
                    self.snap_target = 0.0;
                    self.visible_activation = cx
                        .focus
                        .current
                        .filter(|key| key.entry == self.entry)
                        .map(|_| HERO_ELEM);
                    fx.push(Fx::App(AppFx::Home(HomeReq::FoldToHero)));
                    fx.invalidate(Provenance::Input);
                } else {
                    fx.push(Fx::App(AppFx::Loop(LoopReq::BackAtRoot)));
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

impl<H: HomeLike> Screen<H> for HomeScreen {
    fn redraw_focused(&self, f: &mut DrawFrame<'_, '_, H>, focus: Option<nj_machine::machine::FocusKey<u32>>) {
        HomeScreen::redraw_focused::<H>(self, f, focus)
    }
    fn glides(&self) -> bool { true }
    fn name(&self) -> &'static str {
        super::registry::word::HOME
    }
    fn state(&self) -> &dyn LogicalState {
        self
    }
    fn crumb(&self, _cx: &Cx<'_, H>) -> Option<Cow<'_, str>> {
        None
    }
    fn prepare(&mut self, _budget: &mut Budget, _cx: &Cx<'_, H>) {}
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, H>) {
        self.draw_page(f);
    }
    fn render(&self) -> RenderStrategy {
        RenderStrategy::Page
    }
    fn links(&self, out: &mut Vec<Link>) {
        // A collection shelf's linked heading sits between the shelf above and its own cards:
        // UP from any card reaches it, DOWN returns to the remembered card, and it is the next
        // stop DOWN from the shelf above. The engine takes the FIRST matching link, so these
        // precede the first shelf's UP door to the hero.
        for (row, projection) in self.rows.iter().enumerate() {
            if projection.link.is_none() {
                continue;
            }
            let heading = heading_group(projection.group);
            out.extend(linked_heading::links(heading, projection.group));
            let above = row.checked_sub(1).map_or(HERO_GROUP, |r| self.rows[r].group);
            out.push(Link { from: heading, dir: Dir::Up, to: above });
            out.push(Link { from: above, dir: Dir::Down, to: heading });
        }
        if let Some(first) = self.rows.first() {
            out.push(Link {
                from: HERO_GROUP,
                dir: Dir::Down,
                to: first.group,
            });
            out.push(Link {
                from: first.group,
                dir: Dir::Up,
                to: HERO_GROUP,
            });
        }
    }
    fn memory(&self) -> H::Memory {
        PageMemory::Home(HomeMemory {
            groups: self.groups.clone(),
            items: Arc::clone(&self.items),
            next_group: self.next_group,
            next_elem: self.next_elem,
            carousel: self.carousel.clone(),
            strip_chosen: self.strip_chosen,
            scroll_y: self.grid.scroll_y.pos,
            row_scroll: self.rows.iter().enumerate()
                .map(|(index, row)| (row.group.0, self.grid.shelf(index).scroll_x()))
                .chain(self.restored_scroll.iter().copied()).collect(),
        })
    }
    fn memory_at(&self, _focus: Option<FocusKey<u32>>) -> H::Memory {
        <HomeScreen as Screen<H>>::memory(self)
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

fn write_hub_identity(identity: &HomeHubIdentity, c: &mut Canon) {
    match identity {
        HomeHubIdentity::ContinueWatching => {
            c.u32(0);
        }
        HomeHubIdentity::Identifier { sid, id, key } => {
            c.u32(1).u32(u32::from(sid.raw())).str(id).str(key);
        }
        HomeHubIdentity::Key { sid, key } => {
            c.u32(2).u32(u32::from(sid.raw())).str(key);
        }
        HomeHubIdentity::Ephemeral {
            generation,
            ordinal,
        } => {
            c.u32(3).u32(*generation).u32(*ordinal);
        }
    }
}

fn write_item_identity(identity: &HomeItemIdentity, c: &mut Canon) {
    match identity {
        HomeItemIdentity::Item { hub, sid, rk } => {
            c.u32(0);
            write_hub_identity(hub, c);
            c.u32(u32::from(sid.raw())).str(rk);
        }
        HomeItemIdentity::Slot {
            hub,
            generation,
            ordinal,
        } => {
            c.u32(1);
            write_hub_identity(hub, c);
            c.u32(*generation).u32(*ordinal);
        }
    }
}

fn pinned_snap(target: f32, rows: usize) -> f32 {
    if rows == 0 {
        0.0
    } else {
        target
    }
}

fn status_read(
    view: HubsView<'_>,
) -> Option<(
    &'static std::ffi::CStr,
    StatusKind,
    Option<&'static std::ffi::CStr>,
)> {
    if view.hub_count() > 0 {
        return None;
    }
    Some(match view.state {
        crate::catalog_fetch::HubState::Loading => {
            (nj_platform::i18n::msg::browse_home_loading_c(), StatusKind::Working, None)
        }
        crate::catalog_fetch::HubState::Failed => (
            nj_platform::i18n::msg::browse_home_failed_c(),
            StatusKind::Failed,
            Some(nj_platform::i18n::msg::browse_action_retry_c()),
        ),
        crate::catalog_fetch::HubState::Ready => (
            nj_platform::i18n::msg::browse_home_empty_c(),
            StatusKind::Empty,
            Some(nj_platform::i18n::msg::browse_home_refresh_c()),
        ),
    })
}

/// **The hub read-out, built ONCE for its draw and its hit rect** — the same kind, caption and
/// action on both, so the pill a click lands in is the pill on screen. It fills the page, so a
/// failure stands on the shared page lines (`StatusOverlay::page`), level with the sign-in
/// failure's and a Library section's; loading and the empty answer stay centred.
///
/// A failure while discovery offers "Connect without encryption?" for a server (`plaintext`)
/// says why in the reason slot and — until it is answered — makes *Connect* the primary
/// (`screens::plaintext_question`, shared with the sign-in and a Library source's read-out).
fn status_overlay<'a>(view: HubsView<'_>, plaintext: &'a OfferWatch, clock: &ClockWatch) -> Option<StatusOverlay<'a>> {
    let (caption, kind, action) = status_read(view)?;
    // Home's hub failure carries no typed cause of its own — it is the same untyped "can't reach
    // the server" verdict as the Library's own, so the two share the glyph rather than one
    // inventing a cause the other doesn't have.
    let mut overlay = StatusOverlay::new(Rect::FULL, caption, kind).page(crate::ui::icons::Icon::ServerBadgeMinus);
    let mut action = action;
    if kind == StatusKind::Failed {
        if let (Some(verdict), Some(reason)) = (plaintext.verdict(), plaintext.reason()) {
            overlay = overlay.reason(reason);
            action = Some(plaintext_question::primary(Some(verdict)));
        } else if let Some((reason, glyph)) = clock.reason() {
            // No offer to answer: a wrong clock is the one cause the read-out can name, and it
            // brings its own glyph (the same placement, a different mark).
            overlay = overlay.reason(reason).page(glyph);
        }
    }
    Some(match action {
        Some(label) => overlay.action(label),
        None => overlay,
    })
}


fn hero_group_len(view: HubsView<'_>) -> usize {
    match status_read(view) {
        Some((_, _, action)) => usize::from(action.is_some()),
        None => 1,
    }
}

/// Where a row rests its focused card's centre: the middle of the page beside the rail, x 100..1920
/// (`docs/litefin-look.md` 4.2 Scrolling), clamped at the row's ends by its [`RowStyle`].
const ROW_CENTRE_X: f32 = (CONTENT_LEFT + SCR_W) * 0.5;
const POSTER_ROW: RowStyle = RowStyle { centre_x: Some(ROW_CENTRE_X), ..RowStyle::HOME };
const LANDSCAPE_ROW: RowStyle = RowStyle { centre_x: Some(ROW_CENTRE_X), ..RowStyle::EPISODE };

/// The card style of a landscape row (Litefin's 16:9 box) or a poster row.
fn style(landscape: bool) -> &'static RowStyle {
    if landscape { &LANDSCAPE_ROW } else { &POSTER_ROW }
}

fn is_my_media(identity: &HomeHubIdentity) -> bool {
    matches!(identity, HomeHubIdentity::Identifier { id, .. } if &**id == HUB_MY_MEDIA)
}

/// Litefin's landscape rows: My Media, Continue Watching and Next Up.
fn landscape(identity: &HomeHubIdentity) -> bool {
    match identity {
        HomeHubIdentity::ContinueWatching => true,
        HomeHubIdentity::Identifier { id, .. } => &**id == HUB_MY_MEDIA || &**id == HUB_NEXT_UP,
        HomeHubIdentity::Key { .. } | HomeHubIdentity::Ephemeral { .. } => false,
    }
}

fn card_x(sty: &RowStyle, col: usize, scroll: f32) -> f32 {
    sty.margin_x + col as f32 * (sty.w + sty.gap) - scroll
}
#[cfg(test)]
fn col_at(sty: &RowStyle, x: f32, scroll: f32, count: usize) -> Option<usize> {
    (0..count).find(|&col| {
        let left = card_x(sty, col, scroll);
        x >= left && x <= left + sty.w
    })
}

/// Where text of `sz` puts its cap top to sit in a `lh` line box at `top` the way a browser sets
/// it: Poppins at line-height 1.5 centres its cap band in the box.
fn line_cap_top(top: f32, lh: f32, sz: std::os::raw::c_int, measure: &dyn Measure) -> f32 {
    top + (lh - measure.cap_h(sz)) * 0.5
}

/// A row title's cap top, its line box starting at `line_top`.
fn title_cap_top(line_top: f32, measure: &dyn Measure) -> f32 {
    line_cap_top(line_top, ROW_TITLE_LH, theme::size::lf::S32, measure)
}

fn reveal(tex: u32, spring: &Spring) -> f32 {
    if tex == 0 {
        0.0
    } else {
        spring.pos
    }
}
fn hero_logo_rk(item: &PmsMovie) -> &str {
    if item.kind == 3 && !item.show_rk.is_empty() {
        &item.show_rk
    } else {
        &item.rk
    }
}
fn prefetch_order(cur: i32, n: i32, out: &mut [i32; 2 * HERO_PREFETCH]) -> usize {
    if n <= 1 {
        return 0;
    }
    let cur = cur.rem_euclid(n);
    let mut count = 0;
    for distance in 1..=HERO_PREFETCH as i32 {
        for step in [distance, -distance] {
            let index = (cur + step).rem_euclid(n);
            if index != cur && !out[..count].contains(&index) {
                out[count] = index;
                count += 1;
            }
        }
    }
    count
}
/// Is the billboard up, so its neighbours' backdrops are held ([`HomeScreen::prefetch`])?
fn neighbours_armed(snap: f32) -> bool {
    snap < 0.05
}
/// …and is it also settled, so the neighbours' logos may be warmed?
fn prefetch_armed(snap: f32, sliding: bool) -> bool {
    neighbours_armed(snap) && !sliding
}

fn display_source(real: &str) -> &str {
    static OVERRIDE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    OVERRIDE
        .get_or_init(|| nj_base::devtrig::read("shared"))
        .as_deref()
        .unwrap_or(real)
}

fn shared_by(source: &str) -> String {
    crate::ui::fmt::shared_by(source).unwrap_or_default()
}
fn meta_source_flow(
    base_w: f32,
    source: &str,
    mut run: impl FnMut(&str, f32, f32, i32, i32, [f32; 4]) -> f32,
) -> f32 {
    if source.is_empty() {
        return base_w;
    }
    let mut dx = base_w + SOURCE_PAD;
    dx += run(
        "\u{b7}",
        dx,
        SYN_W - dx,
        theme::size::lf::S21,
        0,
        theme::TEXT_SEPARATOR,
    );
    dx += SOURCE_PAD;
    dx += run(
        &shared_by(source),
        dx,
        SYN_W - dx,
        theme::size::lf::S21,
        0,
        theme::TEXT_TERTIARY,
    );
    dx
}
/// The `· shared by` run after the meta line in its line box at `top`, for a borrowed hero.
fn draw_meta_source(p: Painter, source: &str, x: f32, top: f32, base_w: f32, measure: &dyn Measure) {
    meta_source_flow(base_w, source, |text, dx, budget, size, bold, ink| {
        let elided = nj_gfx::text::elide_by(text, budget, false, |t| {
            measure.width_str(t, size, bold != 0)
        });
        let Ok(text) = CString::new(elided) else {
            return 0.0;
        };
        let mut label = Label::new(text.as_ptr(), size, ink).v(VAlign::Middle);
        if bold == 1 {
            label = label.bold();
        }
        label.draw(p, Rect::new(x + dx, top, budget, META_LH))
    });
}
/// One item of the hero's meta line.
#[derive(Debug, PartialEq)]
enum MetaItem {
    Text(String),
    /// The official rating, on its badge.
    Badge(String),
    /// The community score, one decimal after a gold star.
    Star(String),
}

/// The meta line's items, Litefin's order: the year, the runtime ("1h 52m"), the rating as a
/// badge, the community score after its star. An absent fact is no item.
fn meta_runs(hero: &PmsMovie) -> Vec<MetaItem> {
    let mut runs = Vec::new();
    if hero.year > 0 {
        runs.push(MetaItem::Text(hero.year.to_string()));
    }
    if hero.dur_ns > 0 {
        // to the nearest minute, as Litefin rounds `RunTimeTicks`
        let mins = (hero.dur_ns + 30_000_000_000) / 60_000_000_000;
        let (h, m) = (mins / 60, mins % 60);
        let msg = if h > 0 {
            nj_platform::i18n::msg::home_runtime_hours(h, m)
        } else {
            nj_platform::i18n::msg::home_runtime_minutes(m)
        };
        runs.push(MetaItem::Text(msg));
    }
    if !hero.rating.is_empty() {
        runs.push(MetaItem::Badge(hero.rating.clone()));
    }
    if hero.score > 0.0 {
        runs.push(MetaItem::Star(crate::ui::fmt::decimal(f64::from(hero.score), 1)));
    }
    runs
}

/// Draw the meta line in its line box at `top`; returns the width it took.
fn draw_meta(p: Painter, hero: &PmsMovie, x: f32, top: f32, measure: &dyn Measure) -> f32 {
    let (sz, badge_sz) = (theme::size::lf::S21, theme::size::lf::S18);
    let text = |cs: &CString, at: f32| {
        Label::new(cs.as_ptr(), sz, theme::home_lf::META_INK).draw(p, Rect::new(at, top, f32::INFINITY, META_LH))
    };
    let mut dx = 0.0;
    for (i, run) in meta_runs(hero).iter().enumerate() {
        if i > 0 {
            dx += META_ITEM_GAP;
        }
        let (MetaItem::Text(s) | MetaItem::Badge(s) | MetaItem::Star(s)) = run;
        let Ok(cs) = CString::new(s.as_str()) else { continue };
        match run {
            MetaItem::Text(_) => dx += text(&cs, x + dx),
            MetaItem::Badge(_) => {
                let w = measure.width(&cs, badge_sz, true) + 2.0 * BADGE_PAD.0;
                // the badge's line box is its 1.5 lines plus its padding, centred on the meta line
                let h = badge_sz as f32 * 1.5 + 2.0 * BADGE_PAD.1;
                let r = Rect::new(x + dx, top + (META_LH - h) * 0.5, w, h);
                p.rrect(r, BADGE_RAD, BADGE_RAD, theme::home_lf::BADGE_FILL);
                Label::new(cs.as_ptr(), badge_sz, theme::TEXT_PRIMARY).bold().h(crate::ui::label::HAlign::Center).draw(p, r);
                dx += w;
            }
            MetaItem::Star(_) => {
                // the star is a glyph of the line's own size, centred on its line box
                let d = sz as f32;
                let mark = Rect::new(x + dx, top + (META_LH - d) * 0.5, d, d);
                crate::ui::icons::draw(p, crate::ui::icons::Icon::Star, mark, theme::RATING_STAR);
                dx += d + META_STAR_GAP;
                dx += text(&cs, x + dx);
            }
        }
    }
    dx
}

/// One slide's content block, bottom-anchored at `landing_hero::TEXT_BOTTOM`: the logo box (the
/// clearLogo, else the title), the meta line, the synopsis.
fn hero_content(hero: &PmsMovie, source: &str, p: Painter, measure: &dyn Measure) {
    let episode = hero.kind == 3;
    let title = if episode && !hero.show_title.is_empty() {
        &hero.show_title
    } else {
        &hero.title
    };
    let synopsis = TextView::new(&hero.summary, theme::size::lf::S20, theme::TEXT_SECONDARY)
        .leading(SYN_LH)
        .max_lines(SYN_LINES);
    let lines = if hero.summary.is_empty() {
        0
    } else {
        (synopsis.measure_h(SYN_W) / SYN_LH).round() as usize
    };
    let top = landing_hero::logo_top(lines);
    HeroLogo::new(hero.sid.raw(), hero_logo_rk(hero), title, HERO_LOGO)
        .draw(p, Rect::new(TEXT_X, top, LOGO_W, LOGO_H), measure);
    let meta_top = top + LOGO_H + LOGO_GAP;
    let meta_w = draw_meta(p, hero, TEXT_X, meta_top, measure);
    if !source.is_empty() {
        draw_meta_source(p, source, TEXT_X, meta_top, meta_w, measure);
    }
    if lines > 0 {
        let syn_top = meta_top + META_LH + META_GAP;
        synopsis.draw(p, Rect::new(TEXT_X, line_cap_top(syn_top, SYN_LH, theme::size::lf::S20, measure), SYN_W, 0.0));
    }
}

#[cfg(test)]
mod tests;
