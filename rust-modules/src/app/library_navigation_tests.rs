//! The rail on a real Library page: entry on LEFT, its column, RIGHT and BACK back to the seat it
//! left, the Libraries popover, and activation by identity.

use super::*;
#[allow(unused_imports)]
use super::test_support::*;
use super::test_support::{frame};

#[test]
fn the_rail_enters_on_left_walks_its_column_and_returns_to_the_seat_it_left() {
    use crate::app::chrome::{item_key, Dest, ACCOUNT_KEY};
    use crate::ui::containers::tabs::{POPOVER, STRIP};
    let _guard = nj_base::testlock::serial();
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            crate::catalog::reset_servers_for_test();
        }
    }
    let _cleanup = Cleanup;
    let session = crate::catalog::session::TempSession::new("library-rail-navigation");
    session.watching("u-library-rail-navigation");
    crate::catalog::reset_servers_for_test();
    let own = crate::catalog::register_for_test("rail-own", "127.0.0.1", 9, "synthetic", "fixture");
    let shared = crate::catalog::register_for_test("rail-shared", "127.0.0.1", 10, "synthetic", "fixture");
    crate::catalog::set_current(own);
    let mut d = Dispatcher::<AppHost>::new();
    let mut rig = Bridge::for_test(|| 0);
    rig.stores.browse.borrow_mut().seed_registered_table_for_test([own, shared]);
    rig.refresh_browse_directory();
    rig.browse_run(crate::stores::browse::BrowseCmd::SetCur(0));
    rig.stores.browse.borrow_mut().seed_items_for_test(12);
    let mut t = 0;
    let mut step = |d: &mut Dispatcher<AppHost>, rig: &mut Bridge, key: Option<Key>| {
        let inputs = key.map(|key| script_key(key, tick(t))).unwrap_or_default();
        frame(d, rig, AppArg::Library, tick(t), inputs);
        t += 1;
    };
    for _ in 0..80 { step(&mut d, &mut rig, None); }
    let entry = d.nav.top_page().unwrap().id;
    let at = |elem| Some(FocusKey { entry, elem });
    let column: Vec<_> = d.nav.tabs.strip.iter().filter(|m| m.group == STRIP).map(|m| m.elem).collect();
    assert_eq!(column, vec![item_key(Dest::Home), item_key(Dest::Favorites), item_key(Dest::Search),
        item_key(Dest::Random), item_key(Dest::Libraries), item_key(Dest::Settings), ACCOUNT_KEY],
        "Litefin's order, keyed by identity");
    let base = crate::ui::dispatch::STRIP_BASE;
    assert!(d.focus().is_some_and(|key| key.elem < base), "the Library seats its own content");

    // LEFT until the page has no answer: the rail takes it, on the Library's own item.
    let mut left_from = d.focus();
    for _ in 0..40 {
        if d.focus().is_some_and(|key| key.elem >= base) { break; }
        left_from = d.focus();
        step(&mut d, &mut rig, Some(Key::Left));
    }
    assert_eq!(d.focus(), at(item_key(Dest::Libraries)), "the rail is entered on the current page's item");
    assert!(content_probe(&d, &rig).contains("pill=4"), "the probe names the rail slot");

    // UP/DOWN walk the column and stop at its ends.
    for want in [item_key(Dest::Settings), ACCOUNT_KEY, ACCOUNT_KEY] {
        step(&mut d, &mut rig, Some(Key::Down));
        assert_eq!(d.focus(), at(want));
    }
    for _ in 0..7 { step(&mut d, &mut rig, Some(Key::Up)); }
    assert_eq!(d.focus(), at(item_key(Dest::Home)), "UP stops on the first item");
    step(&mut d, &mut rig, Some(Key::Left));
    assert_eq!(d.focus(), at(item_key(Dest::Home)), "LEFT on the rail goes nowhere");

    // RIGHT returns to exactly the seat the LEFT came from, wherever the remote went on the rail.
    step(&mut d, &mut rig, Some(Key::Right));
    assert_eq!(d.focus(), left_from, "RIGHT returns to the seat it came from");
    // …and so does BACK.
    step(&mut d, &mut rig, Some(Key::Left));
    assert_eq!(d.focus(), at(item_key(Dest::Libraries)));
    step(&mut d, &mut rig, Some(Key::Back));
    assert_eq!(d.focus(), left_from, "BACK on the rail returns to the page, not off it");

    // OK on Libraries opens the popover, focus on a library row.
    step(&mut d, &mut rig, Some(Key::Left));
    step(&mut d, &mut rig, Some(Key::Ok));
    super::chrome_requests(&mut d, &mut rig);
    assert_eq!(d.nav.tabs.popover_opener, Some(item_key(Dest::Libraries)));
    let rows: Vec<_> = d.nav.tabs.strip.iter().filter(|m| m.group == POPOVER).map(|m| m.elem).collect();
    assert!(rows.len() >= 2, "every library of the server is a row: {rows:?}");
    let on_row = d.focus().expect("the popover takes focus");
    assert!(rows.contains(&on_row.elem), "focus opens on a row");
    step(&mut d, &mut rig, None);
    assert_eq!(d.focus(), Some(on_row), "the popover stays while focus is in it");
    // LEFT closes it back onto the item that opened it.
    step(&mut d, &mut rig, Some(Key::Left));
    assert_eq!(d.focus(), at(item_key(Dest::Libraries)));
    step(&mut d, &mut rig, None);
    assert!(d.nav.tabs.strip.iter().all(|m| m.group == STRIP), "a closed popover publishes no rows");
    assert_eq!(d.nav.tabs.popover_opener, None);

    // OK on Search navigates there, by its identity, through the dispatcher's one door.
    step(&mut d, &mut rig, Some(Key::Up));
    assert_eq!(d.focus(), at(item_key(Dest::Random)));
    step(&mut d, &mut rig, Some(Key::Up));
    assert_eq!(d.focus(), at(item_key(Dest::Search)));
    rig.take_library_reqs();
    step(&mut d, &mut rig, Some(Key::Ok));
    assert!(!rig.take_library_reqs().iter().any(|(_, req, _)| !matches!(req, LibraryReq::PublishShelves { .. })),
        "the page never sees a rail press");
    super::chrome_requests(&mut d, &mut rig);
    assert!(d.has_pending_navigation(), "Search is on its way");
    step(&mut d, &mut rig, None);
    assert!(matches!(d.nav.tabs.stack.pending_dest(), Some(AppArg::Search)) || d.top_arg() == Some(&AppArg::Search),
        "the rail's press selected Search");
}

/// `route` on the two-server fixture: four libraries (own Movies, own TV Shows, then the shared
/// server's two), `current` the store's, at rest. The session lives as long as the test holds it.
fn rail_fixture(name: &str, route: AppArg, current: usize)
    -> (Dispatcher<AppHost>, Bridge, u32, crate::catalog::session::TempSession) {
    let session = crate::catalog::session::TempSession::new(name);
    session.watching(&format!("u-{name}"));
    crate::catalog::reset_servers_for_test();
    let own = crate::catalog::register_for_test("rail-own", "127.0.0.1", 9, "synthetic", "fixture");
    let shared = crate::catalog::register_for_test("rail-shared", "127.0.0.1", 10, "synthetic", "fixture");
    crate::catalog::set_current(own);
    let mut d = Dispatcher::<AppHost>::new();
    let mut rig = Bridge::for_test(|| 0);
    rig.stores.browse.borrow_mut().seed_registered_table_for_test([own, shared]);
    rig.refresh_browse_directory();
    rig.browse_run(crate::stores::browse::BrowseCmd::SetCur(current));
    rig.stores.browse.borrow_mut().seed_items_for_test(12);
    for t in 0..80 { frame(&mut d, &mut rig, route.clone(), tick(t), vec![]); }
    (d, rig, 80, session)
}

struct ServersCleanup;
impl Drop for ServersCleanup {
    fn drop(&mut self) { crate::catalog::reset_servers_for_test(); }
}

/// Choosing a library in the popover closes it and hands focus back to the Libraries item, as
/// LEFT out of it does: on the library already on screen nothing else happens, and the focus
/// does not fall through to Home (whose OK would then leave the library).
#[test]
fn a_chosen_library_row_hands_focus_back_to_its_opener() {
    use crate::app::chrome::{item_key, Dest};
    let _guard = nj_base::testlock::serial();
    let _cleanup = ServersCleanup;
    let (mut d, mut rig, mut t, _session) = rail_fixture("library-rail-choose", AppArg::Library, 0);
    let entry = d.nav.top_page().unwrap().id;
    assert!(super::dev_rail(&mut d, &mut rig, "libraries"), "the popover opens");
    let on_screen = rig.chrome.library_key(&rig.current_library_for_test()).unwrap();
    assert_eq!(d.focus(), Some(FocusKey { entry, elem: on_screen }), "it opens on the library on screen");
    frame(&mut d, &mut rig, AppArg::Library, tick(t), script_key(Key::Ok, tick(t)));
    t += 1;
    super::chrome_requests(&mut d, &mut rig);
    for _ in 0..4 { frame(&mut d, &mut rig, AppArg::Library, tick(t), vec![]); t += 1; }
    assert_eq!(d.nav.tabs.popover_opener, None, "the popover closed");
    assert_eq!(d.focus(), Some(FocusKey { entry, elem: item_key(Dest::Libraries) }),
        "focus is back on the item that opened it");
    assert!(d.top_arg() == Some(&AppArg::Library) && !d.has_pending_navigation(), "still the same library");
}

/// From Home, a library row opens the Library; BACK returns to Home on the Libraries item the
/// row hung from, not on the row that closed with the popover.
#[test]
fn back_from_a_library_opened_from_home_returns_to_the_libraries_item() {
    use crate::app::chrome::{item_key, Dest};
    let _guard = nj_base::testlock::serial();
    let _cleanup = ServersCleanup;
    let (mut d, mut rig, mut t, _session) = rail_fixture("library-rail-home", AppArg::Home, 0);
    let mut step = |d: &mut Dispatcher<AppHost>, rig: &mut Bridge, route: AppArg, key: Option<Key>| {
        let inputs = key.map(|key| script_key(key, tick(t))).unwrap_or_default();
        frame(d, rig, route, tick(t), inputs);
        t += 1;
    };
    assert!(d.top_arg() == Some(&AppArg::Home));
    let home = d.nav.top_page().unwrap().id;
    // As the reader goes: LEFT into the rail, down to Libraries, OK, down to the second row, OK.
    for _ in 0..40 {
        if d.focus().is_some_and(|key| key.elem >= crate::ui::dispatch::STRIP_BASE) { break; }
        step(&mut d, &mut rig, AppArg::Home, Some(Key::Left));
    }
    for _ in 0..8 {
        if d.focus() == Some(FocusKey { entry: home, elem: item_key(Dest::Libraries) }) { break; }
        step(&mut d, &mut rig, AppArg::Home, Some(Key::Down));
    }
    assert_eq!(d.focus(), Some(FocusKey { entry: home, elem: item_key(Dest::Libraries) }), "Libraries is on the rail");
    step(&mut d, &mut rig, AppArg::Home, Some(Key::Ok));
    super::chrome_requests(&mut d, &mut rig);
    step(&mut d, &mut rig, AppArg::Home, Some(Key::Down));
    let row = rig.chrome.library_key(&rig.library_at(1).unwrap()).unwrap();
    assert_eq!(d.focus(), Some(FocusKey { entry: home, elem: row }), "on the popover's second row");
    step(&mut d, &mut rig, AppArg::Home, Some(Key::Ok));
    super::chrome_requests(&mut d, &mut rig);
    for _ in 0..60 { step(&mut d, &mut rig, AppArg::Library, None); }
    assert!(d.top_arg() == Some(&AppArg::Library), "the row opened the Library");
    step(&mut d, &mut rig, AppArg::Library, Some(Key::Back));
    for _ in 0..60 { step(&mut d, &mut rig, AppArg::Home, None); }
    assert!(d.top_arg() == Some(&AppArg::Home));
    assert_eq!(d.focus(), Some(FocusKey { entry: home, elem: item_key(Dest::Libraries) }),
        "BACK lands on the Libraries item, not on Home's");
}

/// Over Settings the rail still knows which library is under it: the popover opens on that row,
/// and choosing it leaves Settings for the same grid without re-entering it.
#[test]
fn the_popover_over_settings_opens_on_the_library_under_it() {
    let _guard = nj_base::testlock::serial();
    let _cleanup = ServersCleanup;
    let (mut d, mut rig, mut t, _session) = rail_fixture("library-rail-settings", AppArg::Library, 1);
    let under = rig.current_library_for_test();
    assert_eq!(Some(&under), rig.library_at(1).as_ref(), "the second library is on screen");
    super::open_settings(&mut d);
    for _ in 0..30 { frame(&mut d, &mut rig, AppArg::Library, tick(t), vec![]); t += 1; }
    let settings = crate::ui::dispatch::rail_entry(&d.nav).unwrap();
    assert!(d.nav.is_surface(settings));
    assert!(super::dev_rail(&mut d, &mut rig, "libraries"));
    assert_eq!(d.focus(), Some(FocusKey { entry: settings, elem: rig.chrome.library_key(&under).unwrap() }),
        "the popover opens on the library under Settings");
    frame(&mut d, &mut rig, AppArg::Library, tick(t), script_key(Key::Ok, tick(t)));
    super::chrome_requests(&mut d, &mut rig);
    assert!(rig.library_commands.is_empty(), "the library on screen is not entered again");
}

/// The dev `library` trigger: a number is that library of the popover; an empty file (how the
/// harness writes `"nativejelly-library": true`) is the server's preferred Movies library; a
/// word is refused; and nothing is named before the directory lists it.
#[test]
fn the_library_trigger_takes_an_empty_value_as_the_preferred_movies_library() {
    let _guard = nj_base::testlock::serial();
    let _cleanup = ServersCleanup;
    let rig = Bridge::for_test(|| 0);
    assert_eq!(rig.dev_library(""), Ok(None), "nothing before the directory lists a library");
    let (_d, rig, _, _session) = rail_fixture("library-rail-trigger", AppArg::Library, 1);
    let movies = rig.library_at(0).unwrap();
    assert_eq!(rig.dev_library(""), Ok(Some(movies)), "an empty value is the preferred Movies library");
    assert_eq!(rig.dev_library("2"), Ok(rig.library_at(2)));
    assert_eq!(rig.dev_library("9"), Ok(None));
    assert_eq!(rig.dev_library("movies"), Err(()));
}
