//! Application-owned data for the shared rail (`docs/litefin-look.md` 3, Litefin's modern-collapsed
//! sidebar). Capture before stepping a frame; paint consumes borrowed entries and never opens the
//! session file or polls the Browse vocabulary.
//!
//! Every rail control is keyed `STRIP_BASE + stable id`: a nav item by its fixed id, the account
//! avatar by its own, and a library row of the Libraries popover by an id interned for its section
//! the first time the directory publishes it and never handed to another, so a library leaving the
//! server renumbers nothing.

use crate::screens::registry::LibrarySectionIdentity;
use crate::stores::browse::{DirectoryView, SecKind};
use crate::ui::containers::tabs::StripMember;
use crate::ui::dispatch::STRIP_BASE;
use crate::ui::icons::Icon;
use crate::ui::widgets::{self, AvatarRead, ChromeRead, RailEntry, RailFocus, RailRead};
use nj_machine::machine::{FocusKey, Measure};

/// Where a rail nav item goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Dest {
    Home,
    Favorites,
    Search,
    Random,
    Libraries,
    Settings,
}

/// What a published rail key is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Control {
    Item(Dest),
    Account,
    Library(LibrarySectionIdentity),
}

struct Item {
    dest: Dest,
    id: u32,
    icon: Icon,
    icon_on: Icon,
    label: fn() -> &'static str,
}

/// The nav items, top to bottom: Litefin's `MODERN_STATIC_ITEMS` as the household sees them
/// (`docs/litefin-look.md` 3). A new destination is one entry here, with a fresh id, and its arm in
/// `bridge::rail_activate`.
const ITEMS: [Item; 6] = [
    Item { dest: Dest::Home, id: 0, icon: Icon::Home, icon_on: Icon::HomeFill, label: nj_platform::i18n::msg::sidebar_home },
    Item { dest: Dest::Favorites, id: 5, icon: Icon::Favorites, icon_on: Icon::FavoritesFill, label: nj_platform::i18n::msg::sidebar_favorites },
    Item { dest: Dest::Search, id: 3, icon: Icon::SearchLine, icon_on: Icon::SearchFill, label: nj_platform::i18n::msg::sidebar_search },
    Item { dest: Dest::Random, id: 6, icon: Icon::Random, icon_on: Icon::RandomFill, label: nj_platform::i18n::msg::sidebar_random },
    Item { dest: Dest::Libraries, id: 1, icon: Icon::Libraries, icon_on: Icon::LibrariesFill, label: nj_platform::i18n::msg::sidebar_libraries },
    Item { dest: Dest::Settings, id: 2, icon: Icon::Settings, icon_on: Icon::SettingsFill, label: nj_platform::i18n::msg::sidebar_settings },
];
const ACCOUNT_ID: u32 = 4;
/// Library rows take ids from here up, interned per section.
const LIBRARY_ID_BASE: u32 = 0x100;

/// The rail key of a nav item.
pub(crate) const fn item_key(dest: Dest) -> u32 {
    let mut i = 0;
    while i < ITEMS.len() {
        if ITEMS[i].dest as u8 == dest as u8 {
            return STRIP_BASE + ITEMS[i].id;
        }
        i += 1;
    }
    STRIP_BASE
}

/// The avatar's rail key.
pub(crate) const ACCOUNT_KEY: u32 = STRIP_BASE + ACCOUNT_ID;

/// A rail label measured and elided to `max` at the rung the rail draws it on.
fn rail_text(label: &str, max: f32, bold: bool, measure: &dyn Measure) -> (std::ffi::CString, f32) {
    let size = widgets::RAIL_TEXT;
    let text = std::ffi::CString::new(nj_gfx::text::elide_by(label, max, false, |t| measure.width_str(t, size, bold)))
        .unwrap_or_default();
    let w = measure.width(&text, size, bold);
    (text, w)
}

/// One library row of the popover.
struct Library {
    identity: LibrarySectionIdentity,
    key: u32,
    title: String,
    kind: SecKind,
}

#[derive(Default)]
pub(crate) struct ChromeSnapshot {
    /// The visible nav items' destinations and keys, parallel to `items`.
    dests: Vec<(Dest, u32)>,
    items: Vec<RailEntry>,
    libraries: Vec<Library>,
    rows: Vec<RailEntry>,
    /// Every section a library row was ever minted for, with its id. Never shrinks.
    interned: Vec<(LibrarySectionIdentity, u32)>,
    profile_generation: Option<u32>,
    session_watch: crate::catalog::session::VisibleSessionWatch,
    thumb: String,
    name: std::ffi::CString,
    name_w: f32,
    clock: std::ffi::CString,
    clock_minute: Option<i64>,
}

impl ChromeSnapshot {
    pub(crate) fn refresh(&mut self, measure: &dyn Measure, directory: DirectoryView<'_>) {
        self.refresh_with_profile(measure, directory, None);
    }

    pub(crate) fn refresh_with_profile(&mut self, measure: &dyn Measure, directory: DirectoryView<'_>,
        captured: Option<(&crate::catalog::session::CurrentProfile, &crate::catalog::session::Session)>) {
        self.refresh_libraries(measure, directory);
        let generation = captured.map_or_else(crate::catalog::session::current_gen, |(profile, _)| profile.generation);
        let session_changed = captured.is_none() && self.session_watch.changed();
        if self.profile_generation != Some(generation) || session_changed {
            let current = captured.map_or_else(crate::catalog::session::current, |(profile, _)| profile.user.clone());
            let account = if let Some((_, saved)) = captured { saved.account(current.as_ref()) }
                else { crate::catalog::session::peek().account(current.as_ref()) };
            self.thumb = current.map(|user| user.thumb).unwrap_or_default();
            let label = crate::screens::account_menu::chip_label(&account);
            (self.name, self.name_w) = rail_text(&label, widgets::RAIL_LABEL_TEXT_MAX, true, measure);
            self.profile_generation = Some(generation);
        }
    }

    /// Rebuild the library rows (and the items, which show Libraries only while there is one)
    /// when the directory's sections change.
    fn refresh_libraries(&mut self, measure: &dyn Measure, directory: DirectoryView<'_>) {
        let sections: Vec<_> = directory.sections().iter()
            .filter_map(|s| Some((LibrarySectionIdentity { sid: s.sid?, key: s.key }, s.kind, s.row.title.as_str())))
            .collect();
        let same = sections.len() == self.libraries.len() && !self.items.is_empty()
            && sections.iter().zip(&self.libraries).all(|((id, kind, title), lib)|
                *id == lib.identity && *kind == lib.kind && *title == lib.title);
        if same { return; }
        self.libraries = sections.into_iter().map(|(identity, kind, title)| {
            let key = self.intern(&identity);
            Library { identity, key, title: title.to_owned(), kind }
        }).collect();
        self.rows = self.libraries.iter().map(|lib| {
            let (label, label_w) = rail_text(&lib.title, widgets::RAIL_ROW_TEXT_MAX, false, measure);
            let (icon, icon_on) = match lib.kind {
                SecKind::Movie => (Icon::LibraryMovies, Icon::LibraryMoviesFill),
                SecKind::Show => (Icon::LibraryTv, Icon::LibraryTvFill),
            };
            RailEntry { icon, icon_on, label, label_w }
        }).collect();
        let visible = ITEMS.iter().filter(|item| item.dest != Dest::Libraries || !self.libraries.is_empty());
        (self.dests, self.items) = visible.map(|item| {
            let (label, label_w) = rail_text((item.label)(), widgets::RAIL_LABEL_TEXT_MAX, true, measure);
            ((item.dest, STRIP_BASE + item.id), RailEntry { icon: item.icon, icon_on: item.icon_on, label, label_w })
        }).unzip();
    }

    fn intern(&mut self, identity: &LibrarySectionIdentity) -> u32 {
        if let Some((_, id)) = self.interned.iter().find(|(known, _)| known == identity) {
            return STRIP_BASE + *id;
        }
        let id = LIBRARY_ID_BASE + self.interned.len() as u32;
        self.interned.push((identity.clone(), id));
        STRIP_BASE + id
    }

    /// What a rail key is, while it is published.
    pub(crate) fn control(&self, elem: u32) -> Option<Control> {
        if elem == ACCOUNT_KEY { return Some(Control::Account); }
        self.dests.iter().find(|(_, key)| *key == elem).map(|(dest, _)| Control::Item(*dest))
            .or_else(|| self.libraries.iter().find(|lib| lib.key == elem).map(|lib| Control::Library(lib.identity.clone())))
    }

    /// The key of the popover row for `section`.
    pub(crate) fn library_key(&self, section: &LibrarySectionIdentity) -> Option<u32> {
        self.libraries.iter().find(|lib| lib.identity == *section).map(|lib| lib.key)
    }

    /// The `n`th library in the server's view order (the dev `library=<n>` trigger).
    pub(crate) fn library_at(&self, n: usize) -> Option<LibrarySectionIdentity> {
        self.libraries.get(n).map(|lib| lib.identity.clone())
    }

    /// The column slot of a destination, if it is on the rail.
    pub(crate) fn slot_of(&self, dest: Dest) -> Option<usize> {
        self.dests.iter().position(|(d, _)| *d == dest)
    }

    /// Where the remote is on the rail, from the engine's focus.
    pub(crate) fn focus(&self, focus: Option<FocusKey<u32>>) -> RailFocus {
        let Some(elem) = focus.map(|focus| focus.elem) else { return RailFocus::default() };
        RailFocus {
            slot: if elem == ACCOUNT_KEY { Some(self.items.len()) } else { self.dests.iter().position(|(_, key)| *key == elem) },
            row: self.libraries.iter().position(|lib| lib.key == elem),
        }
    }

    /// Slots in the column, the avatar included.
    pub(crate) fn slots(&self) -> usize {
        self.items.len() + 1
    }

    pub(crate) fn rows(&self) -> usize {
        self.rows.len()
    }

    /// Publish the rail's members: the column, then (while `open`) the popover's rows.
    pub(crate) fn members(&self, open: bool, out: &mut Vec<StripMember<u32>>) {
        out.clear();
        let n = self.items.len();
        out.extend(self.dests.iter().enumerate().map(|(i, (_, key))| StripMember::new(*key, widgets::rail_item_rect(i, n))));
        out.push(StripMember::new(ACCOUNT_KEY, widgets::RAIL_AVATAR));
        let Some(opener) = self.slot_of(Dest::Libraries).filter(|_| open) else { return };
        let popover = widgets::rail_popover_rect(widgets::rail_item_rect(opener, n), self.libraries.len());
        out.extend(self.libraries.iter().enumerate()
            .map(|(j, lib)| StripMember::new(lib.key, widgets::rail_row_rect(popover, j)).in_popover()));
    }

    fn avatar(&self) -> AvatarRead<'_> {
        // The avatar is drawn against the BROWSED server, which is read where the chrome is
        // published: the library does not ask which server is current.
        AvatarRead { src: crate::catalog::current_server().raw(), thumb: &self.thumb,
            name: &self.name, name_w: self.name_w }
    }

    /// The rail as paint reads it. `active` is the current page's slot; `open` whether the
    /// popover is up; `library` the section on screen.
    pub(crate) fn rail(&self, active: Option<usize>, open: bool, library: Option<&LibrarySectionIdentity>) -> RailRead<'_> {
        RailRead {
            items: &self.items,
            active,
            avatar: self.avatar(),
            rows: if open { &self.rows } else { &[] },
            row_active: library.and_then(|section| self.libraries.iter().position(|lib| lib.identity == *section)),
            opener: self.slot_of(Dest::Libraries).unwrap_or(0),
        }
    }

    pub(crate) fn read(&self, avatar_focus: f32) -> ChromeRead<'_> {
        ChromeRead { avatar: self.avatar(), avatar_focus }
    }

    /// Advance the clock to `unix_s`, reading the local time only when the minute turns. Answers
    /// whether the text changed, so the caller can ask for the one present it needs.
    pub(crate) fn tick_clock(&mut self, unix_s: i64, local_minute: impl FnOnce(i64) -> Option<u32>) -> bool {
        let minute = unix_s.div_euclid(60);
        if self.clock_minute == Some(minute) { return false; }
        self.clock_minute = Some(minute);
        let text = local_minute(unix_s).map(widgets::clock_text).unwrap_or_default();
        let text = std::ffi::CString::new(text).unwrap_or_default();
        let changed = text != self.clock;
        self.clock = text;
        changed
    }

    pub(crate) fn clock(&self) -> &std::ffi::CStr {
        &self.clock
    }

    #[cfg(test)]
    pub(crate) fn seed_for_test(&mut self, name: &str, measure: &dyn Measure) {
        self.profile_generation = Some(1);
        self.thumb.clear();
        (self.name, self.name_w) = rail_text(name, widgets::RAIL_LABEL_TEXT_MAX, true, measure);
    }
}

/// Unix seconds, from the C library.
pub(crate) fn unix_now() -> i64 {
    // SAFETY: `time` with a null pointer only returns the value.
    unsafe { libc::time(std::ptr::null_mut()) as i64 }
}

/// The local minute of the day at `unix_s` (the television's own time zone).
pub(crate) fn local_minute(unix_s: i64) -> Option<u32> {
    let t = unix_s as libc::time_t;
    // SAFETY: `localtime_r` writes only through the two pointers passed, both locals here.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = !unsafe { libc::localtime_r(&t, &mut tm) }.is_null();
    ok.then(|| (tm.tm_hour * 60 + tm.tm_min) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stores::browse::{DirectorySnapshot, SectionView, SrcRow};
    use nj_machine::machine::EntryId;

    fn directory(sections: &[(u16, i64, SecKind, &str)]) -> DirectorySnapshot {
        DirectorySnapshot::fixture(7, 0, sections.iter().enumerate().map(|(i, &(sid, key, kind, title))| SectionView {
            sid: Some(crate::catalog::ServerId::from_raw(sid)),
            key,
            kind,
            row: SrcRow { section: i, title: title.into(), pinned: true, ..Default::default() },
        }).collect())
    }

    fn snapshot(directory: &DirectorySnapshot) -> ChromeSnapshot {
        let mut snapshot = ChromeSnapshot {
            profile_generation: Some(crate::catalog::session::current_gen()), ..Default::default()
        };
        snapshot.refresh(&crate::ui::fixture::FixtureMeasure, directory.view());
        snapshot
    }

    fn section(sid: u16, key: i64) -> LibrarySectionIdentity {
        LibrarySectionIdentity { sid: crate::catalog::ServerId::from_raw(sid), key }
    }

    #[test]
    fn session_refresh_rebuilds_the_avatar_without_a_profile_switch() {
        let _serial = nj_base::testlock::serial();
        let _session = crate::catalog::session::TempSession::new("chrome-session-refresh");
        let mut snapshot = ChromeSnapshot::default();
        let directory = directory(&[]);
        crate::catalog::session::install_transient_for_test(true);
        snapshot.refresh(&crate::ui::fixture::FixtureMeasure, directory.view());
        assert_eq!(snapshot.name.to_str().unwrap(), "Sign in");
        crate::catalog::session::save(&crate::catalog::session::Session {
            client_id: "synthetic-client".into(), account_token: "synthetic-token".into(),
            home_users: vec![crate::catalog::session::HomeUserRef {
                title: "Synthetic owner".into(), admin: true, ..Default::default()
            }], ..Default::default()
        });
        snapshot.refresh(&crate::ui::fixture::FixtureMeasure, directory.view());
        assert_eq!(snapshot.name.to_str().unwrap(), "Synthetic owner");
    }

    /// The column in Litefin's order, keyed by identity: Home, Favorites, Search, Random,
    /// Libraries, Settings, then the avatar, each mapping back to what it does; six items start
    /// the block at y=328 (`docs/litefin-look.md` 3).
    #[test]
    fn the_rail_publishes_its_items_by_identity_in_litefins_order() {
        let directory = directory(&[(0, 1, SecKind::Movie, "Filmy"), (0, 2, SecKind::Show, "Seriale")]);
        let snapshot = snapshot(&directory);
        let mut members = Vec::new();
        snapshot.members(false, &mut members);
        let keys: Vec<_> = members.iter().map(|m| m.elem).collect();
        let order = [Dest::Home, Dest::Favorites, Dest::Search, Dest::Random, Dest::Libraries, Dest::Settings];
        assert_eq!(keys, order.iter().map(|d| item_key(*d)).chain([ACCOUNT_KEY]).collect::<Vec<_>>());
        for (key, want) in order.iter().map(|d| (item_key(*d), Control::Item(*d))).chain([(ACCOUNT_KEY, Control::Account)]) {
            assert_eq!(snapshot.control(key), Some(want));
        }
        assert_eq!(members[0].target.y, 328.0, "six items centred between y=120 and y=968");
        assert!(members.iter().all(|m| m.group == crate::ui::containers::tabs::STRIP));
        assert_eq!(members.last().unwrap().target, widgets::RAIL_AVATAR);
        assert_eq!(snapshot.focus(Some(FocusKey { entry: EntryId(1), elem: ACCOUNT_KEY })).slot, Some(6));
        assert_eq!(snapshot.focus(Some(FocusKey { entry: EntryId(1), elem: 3 })), RailFocus::default());
    }

    /// The popover lists every library in view order, and a library's key is its own: a library
    /// that leaves takes its key with it and every other row keeps the key it had.
    #[test]
    fn library_rows_keep_their_keys_when_a_library_leaves() {
        let mut snapshot = snapshot(&directory(&[(0, 1, SecKind::Movie, "Filmy"), (0, 2, SecKind::Show, "Seriale"),
            (1, 7, SecKind::Movie, "Dokumenty")]));
        let before: Vec<_> = (0..3).map(|n| snapshot.library_key(&snapshot.library_at(n).unwrap()).unwrap()).collect();
        let mut members = Vec::new();
        snapshot.members(true, &mut members);
        let rows: Vec<_> = members.iter().filter(|m| m.group == crate::ui::containers::tabs::POPOVER).map(|m| m.elem).collect();
        assert_eq!(rows, before, "the popover lists the libraries in view order");
        assert_eq!(snapshot.control(before[1]), Some(Control::Library(section(0, 2))));
        let shrunk = directory(&[(0, 1, SecKind::Movie, "Filmy"), (1, 7, SecKind::Movie, "Dokumenty")]);
        snapshot.refresh(&crate::ui::fixture::FixtureMeasure, shrunk.view());
        assert_eq!(snapshot.library_key(&section(0, 1)), Some(before[0]));
        assert_eq!(snapshot.library_key(&section(1, 7)), Some(before[2]), "no renumbering");
        assert_eq!(snapshot.control(before[1]), None, "a removed library's key names nothing");
        assert_eq!(snapshot.library_at(1), Some(section(1, 7)));
        let back = directory(&[(0, 1, SecKind::Movie, "Filmy"), (0, 2, SecKind::Show, "Seriale"), (1, 7, SecKind::Movie, "Dokumenty")]);
        snapshot.refresh(&crate::ui::fixture::FixtureMeasure, back.view());
        assert_eq!(snapshot.library_key(&section(0, 2)), Some(before[1]), "a returning library gets its own key back");
    }

    /// Libraries leaves the rail while the server has none to list, and the block re-centres.
    #[test]
    fn libraries_is_on_the_rail_only_while_there_is_a_library() {
        let snapshot = snapshot(&directory(&[]));
        assert_eq!(snapshot.slot_of(Dest::Libraries), None);
        assert_eq!(snapshot.slots(), 6);
        let mut members = Vec::new();
        snapshot.members(true, &mut members);
        assert!(members.iter().all(|m| m.group == crate::ui::containers::tabs::STRIP), "no popover without a library");
        assert_eq!(members[0].target, widgets::rail_item_rect(0, 5));
    }

    #[test]
    fn the_clock_reads_the_local_time_once_a_minute() {
        let mut snapshot = ChromeSnapshot::default();
        let mut reads = 0;
        assert!(snapshot.tick_clock(600, |_| { reads += 1; Some(9 * 60 + 5) }));
        assert_eq!(snapshot.clock().to_str().unwrap(), "09:05");
        assert!(!snapshot.tick_clock(659, |_| { reads += 1; Some(9 * 60 + 5) }), "the same minute is silent");
        assert!(snapshot.tick_clock(660, |_| { reads += 1; Some(9 * 60 + 6) }));
        assert_eq!(snapshot.clock().to_str().unwrap(), "09:06");
        assert_eq!(reads, 2, "the local time is read only when the minute turns");
        assert!(snapshot.tick_clock(86_400, |_| Some(0)));
        assert_eq!(snapshot.clock().to_str().unwrap(), "00:00", "24 h, zero padded");
    }

    #[test]
    fn the_wall_clock_answers_a_minute_of_the_day() {
        assert!(unix_now() > 1_700_000_000, "the C library's clock is the real one");
        assert!(local_minute(unix_now()).is_some_and(|minute| minute < 24 * 60));
    }

    #[test]
    fn chrome_production_reads_only_its_retained_directory() {
        let src = include_str!("chrome.rs");
        let production = src.split("#[cfg(test)]").next().unwrap();
        assert!(!production.contains("crate::browse::"),
            "Chrome must read only its retained Browse directory");
    }

    #[test]
    fn shared_widgets_read_no_live_application_vocabulary() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/widgets.rs"),
        ).expect("read widgets.rs");
        let live = src.lines().filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>().join("\n");
        for forbidden in ["crate::browse::", "crate::catalog::session::", "crate::screens::"] {
            assert!(!live.contains(forbidden),
                "shared widgets must consume captured app projections, found {forbidden}");
        }
    }
}
