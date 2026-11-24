//! Litefin's library head and Genres tab (`docs/litefin-look.md` 6): the tabs switch the view,
//! the genre rows open one genre's grid, BACK returns to its row, Shuffle asks for a random pick.
use super::*;
use crate::ui::fixture::{FixtureArg, FixtureMeasure};
use nj_machine::machine::{Host, InputOwner, PressRead, Tick};

struct TestHost;

#[derive(Clone, Copy)]
struct Views<'a> {
    listing: crate::stores::browse::ListingView<'a>,
    directory: crate::stores::browse::DirectoryView<'a>,
    hubs: crate::stores::browse::HubsView<'a>,
}
impl Host for TestHost {
    type Arg = FixtureArg;
    type Fx = AppFx;
    type Msg = AppMsg;
    type Elem = u32;
    type Views<'a> = Views<'a>;
    type Init = FixtureArg;
    type Memory = PageMemory;
}
impl LibraryLike for TestHost {
    fn listing<'a>(cx: &Cx<'a, Self>) -> crate::stores::browse::ListingView<'a> { cx.views.listing }
    fn directory<'a>(cx: &Cx<'a, Self>) -> crate::stores::browse::DirectoryView<'a> { cx.views.directory }
    fn section_hubs<'a>(cx: &Cx<'a, Self>) -> crate::stores::browse::HubsView<'a> { cx.views.hubs }
}

const ENTRY: EntryId = EntryId(84);
const OWNER: InputOwner = InputOwner::Entry(ENTRY);
fn key(elem: u32) -> FocusKey<u32> { FocusKey { entry: ENTRY, elem } }

struct Fixture {
    stores: crate::stores::Stores,
    listing: crate::stores::browse::ListingSnapshot,
    directory: crate::stores::browse::DirectorySnapshot,
    hubs: crate::stores::browse::HubsSnapshot,
}

impl Fixture {
    /// A movie section with twelve titles, a Recently Added shelf and two Jellyfin genre rows.
    fn new() -> Self {
        let stores = crate::stores::Stores::default();
        stores.browse.borrow_mut().seed_two_source_table_for_test();
        let mut directory = crate::stores::browse::DirectorySnapshot::default();
        stores.capture_browse(&mut directory);
        stores.browse_run(BrowseCmd::SetCur(0));
        {
            let mut browse = stores.browse.borrow_mut();
            browse.seed_items_for_test(12);
            browse.seed_query_choices_for_test(Vec::new(), vec![
                crate::browse::GenreEntry { id: "7".into(), title: "Action".into() },
                crate::browse::GenreEntry { id: "9".into(), title: "Drama".into() },
            ]);
            browse.seed_named_shelves_for_test(0, &[
                ("movie.recentlyadded.1", "", "Recently Added"),
                ("movie.genre.1.7", "7", "Action"),
                ("movie.genre.1.9", "9", "Drama"),
            ], 4);
        }
        let publication = stores.capture_browse(&mut directory);
        Self { stores, listing: publication.listing, directory, hubs: publication.section_hubs }
    }
    /// Run the browse commands among `out` on the fixture's store, then read its publications again.
    fn apply(&mut self, out: &[nj_machine::machine::Stamped<TestHost>]) {
        for effect in out {
            if let Fx::App(AppFx::Store(StoreId::Browse, StoreCmd::Browse(cmd))) = &effect.fx {
                self.stores.browse_run(cmd.clone());
            }
        }
        self.recapture();
    }
    fn recapture(&mut self) {
        let publication = self.stores.capture_browse(&mut self.directory);
        (self.listing, self.hubs) = (publication.listing, publication.section_hubs);
    }
    fn cx(&self, focus: Option<FocusKey<u32>>) -> Cx<'_, TestHost> {
        Cx { views: Views { listing: self.listing.view(), directory: self.directory.view(), hubs: self.hubs.view() },
            tick: Tick::default(), measure: &FixtureMeasure,
            focus: nj_machine::machine::FocusRead { current: focus, ..Default::default() },
            press: PressRead::default(), owner: OWNER }
    }
    fn page(&self) -> LibraryScreen {
        let mut page = LibraryScreen::new(ENTRY, InstanceId(22), None);
        page.sync(&self.cx(None));
        page
    }
    /// Deliver `ev` with focus on `focus`; the effects it pushed.
    fn step(&self, page: &mut LibraryScreen, focus: Option<FocusKey<u32>>, ev: ScreenEvent<TestHost>) -> Vec<nj_machine::machine::Stamped<TestHost>> {
        let mut out = Vec::new();
        let mut present = nj_machine::present::Present::new();
        page.step(&ev, &self.cx(focus), &mut Effects::new(&mut out, MachineId::Instance(InstanceId(22)), &mut present));
        out
    }
}

fn genres_tab(page: &LibraryScreen) -> u32 {
    TAB_BASE + page.tabs().iter().position(|tab| *tab == toolbar::Tab::Genres).unwrap() as u32
}

fn seat_of(out: &[nj_machine::machine::Stamped<TestHost>]) -> Option<FocusTarget<u32>> {
    out.iter().find_map(|effect| match &effect.fx {
        Fx::Deliver(_, Delivery::Screen(ScreenEvent::Enter(Enter::Fresh { focus }))) => Some(*focus),
        _ => None,
    })
}

#[test]
fn the_genres_tab_shows_the_genre_rows_and_the_type_tabs_the_other_shelves() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.page();
    assert_eq!(page.tabs(), [toolbar::Tab::Type(crate::browse::LibraryType::Primary),
        toolbar::Tab::Type(crate::browse::LibraryType::Collections), toolbar::Tab::Genres]);
    assert_eq!(page.shelves.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(), ["movie.recentlyadded.1"],
        "a listing type's view keeps every shelf but the genre rows");
    assert!(page.layout.rows > 0 && page.layout.grid_head);

    let tab = genres_tab(&page);
    fixture.step(&mut page, Some(key(tab)), ScreenEvent::Activate(tab));
    assert_eq!(page.genres, GenreView::Rows);
    assert_eq!(page.active_tab(&fixture.cx(None)), toolbar::Tab::Genres);
    assert_eq!(page.shelves.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(), ["movie.genre.1.7", "movie.genre.1.9"]);
    assert!(page.shelves.iter().all(|row| row.heading.is_some()), "every genre row's heading opens its genre");
    assert_eq!((page.layout.rows, page.layout.grid_head), (0, false), "the rows replace the grid and its head");
    let mut groups = Vec::new();
    page.groups(&fixture.cx(None), &mut groups);
    assert!(!groups.iter().any(|group| group.id == page.pair.groups_config().detail || group.id == page.toolbar),
        "no grid or controls are reachable behind the rows");
    let face = page.heading_rect(0, &fixture.cx(None), At::Drawn).unwrap();
    assert_eq!(face.x, layout::LEFT - 16.0, "Litefin's 16px header padding");

    let movies = TAB_BASE;
    fixture.step(&mut page, Some(key(movies)), ScreenEvent::Activate(movies));
    assert_eq!(page.genres, GenreView::Off);
    assert_eq!(page.shelves.len(), 1);
    assert!(page.pending.grid().is_none(), "the listed type's own tab edits nothing");
}

#[test]
fn a_genre_row_opens_that_genres_grid_and_back_returns_to_its_row() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.page();
    let tab = genres_tab(&page);
    fixture.step(&mut page, Some(key(tab)), ScreenEvent::Activate(tab));
    let (_, drama) = page.shelves[1].heading.unwrap();
    let out = fixture.step(&mut page, Some(key(drama)), ScreenEvent::Activate(drama));
    assert_eq!(page.genres, GenreView::Drill("9".into()));
    assert!(matches!(page.pending.grid(), Some((_, GridAction::Drill { id: Some(id) })) if id == "9"),
        "the genre's grid is the listing under that genre's filter");
    let grid = page.pair.groups_config().detail;
    assert!(matches!(seat_of(&out), Some(FocusTarget::ContainerGroup(group)) if group == grid),
        "focus goes into the genre's grid");
    assert_eq!(page.title(&fixture.cx(None)), "Drama");
    assert!(!page.layout.tabs() && !page.layout.grid_head, "one genre's head is its title alone");
    assert_eq!(page.layout.card_w() + 2.0 * crate::ui::consts::CARD_INSET, 240.0, "a 240 card box");
    assert!(page.shelves.iter().all(|row| !row.id.contains(".genre.")));

    let back = ScreenEvent::Input(nj_machine::machine::InputEvent {
        at: Tick::default(), source: nj_machine::machine::Source::Script,
        kind: InputKind::Key { key: Key::Back, sym: 0, wcode: 0, edge: Edge::Down, at_edge: false },
    });
    let first = page.key(page.pair.detail.elems[0]);
    let out = fixture.step(&mut page, Some(first), back);
    assert_eq!(page.genres, GenreView::Rows);
    assert!(matches!(page.pending.grid(), Some((_, GridAction::Drill { id: None }))), "leaving clears the filter");
    let (_, drama) = page.shelves[1].heading.unwrap();
    assert!(matches!(seat_of(&out), Some(FocusTarget::Elem(at)) if at == key(drama)), "BACK lands on the genre it left");
}

/// The Genres view is the page's memory, not only its live state: a page evicted on its rows or
/// inside one genre remounts from that memory on the same view, and a type tab's page on none.
#[test]
fn the_genres_view_survives_an_eviction_and_a_remount_from_memory() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let remount = |page: &LibraryScreen| {
        let PageMemory::Library(memory) = <LibraryScreen as Screen<TestHost>>::memory(page) else {
            panic!("the Library keeps a Library memory");
        };
        let mut again = LibraryScreen::new(ENTRY, InstanceId(23), None);
        again.restore(&memory);
        again.sync(&fixture.cx(None));
        again
    };
    let mut page = fixture.page();
    assert_eq!(remount(&page).genres, GenreView::Off);
    let tab = genres_tab(&page);
    fixture.step(&mut page, Some(key(tab)), ScreenEvent::Activate(tab));
    let rows = remount(&page);
    assert_eq!(rows.genres, GenreView::Rows);
    assert_eq!(rows.active_tab(&fixture.cx(None)), toolbar::Tab::Genres);
    assert_eq!(rows.shelves.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(), ["movie.genre.1.7", "movie.genre.1.9"]);
    let (_, drama) = page.shelves[1].heading.unwrap();
    fixture.step(&mut page, Some(key(drama)), ScreenEvent::Activate(drama));
    let drill = remount(&page);
    assert_eq!(drill.genres, GenreView::Drill("9".into()));
    assert_eq!(drill.title(&fixture.cx(None)), "Drama");
    assert!(!drill.layout.tabs(), "one genre's head is its title alone, as before the eviction");
}

/// Drill into Drama from the Genres tab and land the grid's edit on the store, as the fade's commit
/// does: the section's listing is then filtered to the genre.
fn drill_into_drama(fixture: &mut Fixture, page: &mut LibraryScreen) {
    let tab = genres_tab(page);
    fixture.step(page, Some(key(tab)), ScreenEvent::Activate(tab));
    let (_, drama) = page.shelves[1].heading.unwrap();
    fixture.step(page, Some(key(drama)), ScreenEvent::Activate(drama));
    let mut out = Vec::new();
    let mut present = nj_machine::present::Present::new();
    page.flush(&mut Effects::new(&mut out, MachineId::Instance(InstanceId(22)), &mut present));
    fixture.apply(&out);
    assert_eq!(fixture.listing.view().genre().map(|genre| genre.id.as_str()), Some("9"));
}

/// R3.1: a genre's grid is a view of the library it was opened in. Switching library from the rail
/// leaves that view (the next library opens on its own listing, and BACK there leaves the page)
/// and takes the drill's filter off the library it leaves, which is otherwise filtered on return.
#[test]
fn switching_library_inside_a_genre_leaves_the_genre_and_its_filter() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    let mut page = fixture.page();
    drill_into_drama(&mut fixture, &mut page);
    let shows = section_identity(fixture.directory.view(), 1).unwrap();
    let out = fixture.step(&mut page, None, ScreenEvent::App(AppMsg::Library(LibraryCmd::Enter(shows))));
    assert_eq!(page.genres, GenreView::Off, "the other library opens on its listing, not under Drama");
    fixture.apply(&out);
    fixture.stores.browse_run(BrowseCmd::SetCur(0));
    fixture.recapture();
    assert!(fixture.listing.view().genre().is_none(), "the library left behind is not filtered to Drama on return");
}

/// R3.1: the drill's filter belongs to the page that opened it. A page that left for good (its
/// memory with it) leaves the filter behind; the next page, mounted without that memory, starts on
/// the unfiltered listing, while a genre the reader chose in FILTER is theirs and stays.
#[test]
fn a_page_mounted_without_memory_drops_a_drill_it_did_not_open_but_keeps_a_filter_choice() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    let mut page = fixture.page();
    drill_into_drama(&mut fixture, &mut page);
    let mut fresh = LibraryScreen::new(ENTRY, InstanceId(24), None);
    let out = fixture.step(&mut fresh, None, ScreenEvent::Mount);
    fixture.apply(&out);
    assert!(fixture.listing.view().genre().is_none(), "a fresh page opens the library unfiltered");

    let target = fresh.address(&fixture.cx(None)).unwrap();
    fixture.stores.browse_run(BrowseCmd::Addressed { target, work: LibraryWork::Commit {
        select: false, choice: false, query: Some(crate::stores::browse::QueryEdit::Genre(Some("7".into()))) } });
    fixture.recapture();
    let mut again = LibraryScreen::new(ENTRY, InstanceId(25), None);
    let out = fixture.step(&mut again, None, ScreenEvent::Mount);
    fixture.apply(&out);
    assert_eq!(fixture.listing.view().genre().map(|genre| genre.id.as_str()), Some("7"),
        "the reader's own FILTER genre survives a fresh mount");
}

/// No top-bar floor clips the head any more: every tab pill is a pointer target over its whole
/// drawn face, from the page's top line down.
#[test]
fn the_tab_pills_are_pressable_over_their_whole_face() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let page = fixture.page();
    let cx = fixture.cx(None);
    let mut draw = DrawFrame::new(&cx, crate::ui::Painter::root());
    page.record_stops(&mut draw);
    let stops = draw.into_stops();
    for i in 0..page.tabs().len() as u32 {
        let stop = stops.iter().find(|stop| stop.key.elem == TAB_BASE + i).expect("every tab pill registers a stop");
        assert!(stop.rect.y >= layout::CONTENT_TOP && stop.rect.intersect(stop.clip) == stop.rect,
            "tab {i}: {:?} is clipped to {:?}", stop.rect, stop.clip);
    }
}

#[test]
fn a_type_tab_asks_for_its_listing_and_shuffle_asks_the_server_for_a_pick() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.page();
    let collections = TAB_BASE + 1;
    fixture.step(&mut page, Some(key(collections)), ScreenEvent::Activate(collections));
    assert!(matches!(page.pending.grid(),
        Some((_, GridAction::LibraryType(crate::browse::LibraryType::Collections)))));
    assert_eq!(page.active_tab(&fixture.cx(None)),
        toolbar::Tab::Type(crate::browse::LibraryType::Collections), "the queued type is the tab shown");

    // R3.4: Litefin's Shuffle is a random pick over the WHOLE library, which only the server can
    // make: the page asks for one by the library on screen, never from the grid's loaded window.
    let out = fixture.step(&mut page, Some(key(SHUFFLE)), ScreenEvent::Activate(SHUFFLE));
    let id = fixture.listing.view().id().unwrap();
    let asked: Vec<_> = out.iter().filter_map(|effect| match &effect.fx {
        Fx::App(AppFx::Library(request)) => Some(request.clone()), _ => None,
    }).collect();
    assert_eq!(asked, [LibraryReq::Shuffle { sid: id.sid, section: id.section }]);
}

/// `ui/CLAUDE.md`: anything that animates owes both halves. The head pills' pop reports while it
/// springs and goes quiet at rest.
#[test]
fn the_head_pills_pop_reports_while_it_moves_and_is_silent_at_rest() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.page();
    let slot = page.head_pop_slot(SORT).expect("Sort has a pop slot");
    let (_, moving) = nj_machine::idle::scoped_motion(|| page.head_pop.step(Some(slot), 0.016));
    assert!(moving, "a pill growing under focus reports its motion");
    let grown = page.head_pop.scale_with(slot, 0.0);
    assert!(grown > 1.0 && grown < crate::ui::widgets::CTRL_FOCUS_SCALE);
    for _ in 0..400 { page.head_pop.step(Some(slot), 0.016); }
    let (_, moving) = nj_machine::idle::scoped_motion(|| page.head_pop.step(Some(slot), 0.016));
    assert!(!moving, "a settled pop requests no further presents");
    let rect = page.control_rect(SORT, &fixture.cx(Some(key(SORT))), At::Drawn).unwrap();
    assert!((rect.1 - crate::ui::widgets::CTRL_FOCUS_SCALE).abs() < 0.001, "the pill draws at its settled pop");
}

/// App text never elides (`ui/CLAUDE.md`): the tab pills and the controls row, each sized to its
/// own label, fit the content band in every shipped language, for both library kinds and with
/// a five-figure library's count before the controls.
#[test]
fn the_head_rows_fit_the_content_band_in_every_shipped_language() {
    use nj_platform::i18n::{language_on_this_thread_for_test, SHIPPED};
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let measure = nj_base::fontcov::advances::ShippedMeasure;
    for language in SHIPPED {
        let _language = language_on_this_thread_for_test(language);
        for kind in [SecKind::Movie, SecKind::Show] {
            let mut page = LibraryScreen::new(ENTRY, InstanceId(23), None);
            page.sync(&fixture.cx(None));
            page.kind = kind; // the fixture's current section is a movie library; the tabs read the kind
            let tabs: Vec<_> = page.tabs().into_iter().map(|tab| page.tab_pill(tab, &fixture.cx(None)).width(&measure)).collect();
            let controls: Vec<_> = page.toolbar_elems().iter().map(|&elem| page.control_pill(elem, &fixture.cx(None)).width(&measure)).collect();
            let count = std::ffi::CString::new(nj_platform::i18n::msg::browse_litefin_paging(10_001, 10_014, 99_999)).unwrap();
            let count = nj_machine::machine::Measure::width(&measure, &count, LibraryScreen::count_rung(), false);
            for (name, (start, spans)) in [("tabs", toolbar::centred(0.0, &tabs)), ("controls", toolbar::centred(count, &controls))] {
                let end = spans.last().map_or(start, |(x, w)| x + w);
                assert!(start >= layout::LEFT && end <= layout::RIGHT,
                    "{} {kind:?} {name}: {start}..{end} leaves the band {}..{}", language.tag(), layout::LEFT, layout::RIGHT);
            }
        }
    }
}
