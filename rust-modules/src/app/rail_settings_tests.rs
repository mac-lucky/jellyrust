//! The rail beside the Settings surface, and the rail's Random pick landing, on a real tree.

use super::*;
#[allow(unused_imports)]
use super::test_support::*;
use super::test_support::frame;

/// Litefin shows its sidebar beside Settings: the rail is the Settings surface's, with Settings
/// as the current item; LEFT from the category list enters it there, RIGHT returns to the row it
/// left, and a destination leaves Settings for its page.
#[test]
fn the_rail_stands_beside_settings_and_leaves_it_for_a_destination() {
    use crate::app::chrome::{item_key, Dest};
    let _g = nj_base::testlock::serial();
    let mut d = Dispatcher::<AppHost>::new();
    let mut rig = Bridge::for_test(|| 0);
    let mut t = 0;
    let mut step = |d: &mut Dispatcher<AppHost>, rig: &mut Bridge, key: Option<Key>| {
        let inputs = key.map(|key| script_key(key, tick(t))).unwrap_or_default();
        frame(d, rig, AppArg::Home, tick(t), inputs);
        t += 1;
    };
    for _ in 0..4 { step(&mut d, &mut rig, None); }
    open_settings(&mut d);
    for _ in 0..30 { step(&mut d, &mut rig, None); }
    let settings = crate::ui::dispatch::rail_entry(&d.nav).expect("the rail belongs to someone");
    assert!(d.nav.is_surface(settings), "the rail is the Settings surface's while it is up");
    assert_eq!(d.nav.tabs.strip_active, Some(item_key(Dest::Settings)), "Settings is the current item");
    let row = d.focus().expect("the category list holds focus");
    assert_eq!(row.entry, settings);

    step(&mut d, &mut rig, Some(Key::Left));
    assert_eq!(d.focus(), Some(FocusKey { entry: settings, elem: item_key(Dest::Settings) }),
        "LEFT from the category list enters the rail on Settings, and does not dismiss it");
    assert!(crate::ui::dispatch::rail_entry(&d.nav) == Some(settings));
    step(&mut d, &mut rig, Some(Key::Right));
    assert_eq!(d.focus(), Some(row), "RIGHT returns to the row it left");

    // Search, two items up from Settings on a rail without libraries, leaves Settings for it.
    step(&mut d, &mut rig, Some(Key::Left));
    for _ in 0..2 { step(&mut d, &mut rig, Some(Key::Up)); }
    assert_eq!(d.focus(), Some(FocusKey { entry: settings, elem: item_key(Dest::Search) }));
    step(&mut d, &mut rig, Some(Key::Ok));
    super::chrome_requests(&mut d, &mut rig);
    assert!(d.has_pending_navigation(), "Search is on its way");
    for i in 0..40 { frame(&mut d, &mut rig, AppArg::Search, tick(t + i), vec![]); }
    assert!(d.top_arg() == Some(&AppArg::Search), "the rail's press opened Search");
    assert!(d.nav.modals.input_owner().is_none(), "and Settings is gone");
    assert_eq!(d.nav.tabs.strip_active, Some(item_key(Dest::Search)));
}

/// The account item beside Settings opens the account menu over the page, Settings gone.
#[test]
fn the_account_item_beside_settings_opens_its_menu_over_the_page() {
    use crate::app::chrome::ACCOUNT_KEY;
    let _g = nj_base::testlock::serial();
    let mut d = Dispatcher::<AppHost>::new();
    let mut rig = Bridge::for_test(|| 0);
    let mut t = 0;
    let mut step = |d: &mut Dispatcher<AppHost>, rig: &mut Bridge, key: Option<Key>| {
        let inputs = key.map(|key| script_key(key, tick(t))).unwrap_or_default();
        frame(d, rig, AppArg::Home, tick(t), inputs);
        t += 1;
    };
    for _ in 0..4 { step(&mut d, &mut rig, None); }
    open_settings(&mut d);
    for _ in 0..30 { step(&mut d, &mut rig, None); }
    let settings = crate::ui::dispatch::rail_entry(&d.nav).unwrap();
    step(&mut d, &mut rig, Some(Key::Left));
    step(&mut d, &mut rig, Some(Key::Down));
    assert_eq!(d.focus(), Some(FocusKey { entry: settings, elem: ACCOUNT_KEY }));
    step(&mut d, &mut rig, Some(Key::Ok));
    super::chrome_requests(&mut d, &mut rig);
    for _ in 0..40 { step(&mut d, &mut rig, None); }
    assert!(account_menu_up(&d), "the account menu is up");
    assert!(!surface_up(&d, is_settings), "over the page, with Settings dismissed");
}

/// The Random pick waits for its answer, then opens that title on the page it was pressed on;
/// an answer for a page that has gone is dropped.
#[test]
fn a_random_pick_opens_its_title_on_the_page_it_was_pressed_on() {
    let _g = nj_base::testlock::serial();
    let mut d = Dispatcher::<AppHost>::new();
    let mut rig = Bridge::for_test(|| 0);
    for i in 0..4 { frame(&mut d, &mut rig, AppArg::Home, tick(i), vec![]); }
    let home = d.nav.top_page().expect("Home").id;

    let (reply, rx) = std::sync::mpsc::channel();
    rig.random = Some((home, rx));
    super::chrome_requests(&mut d, &mut rig);
    assert!(rig.random.is_some() && !d.has_pending_navigation(), "nothing opens before the answer");
    reply.send(Some((crate::catalog::ServerId::UNSET, "105".into()))).unwrap();
    super::chrome_requests(&mut d, &mut rig);
    assert!(rig.random.is_none() && d.has_pending_navigation());
    frame(&mut d, &mut rig, detail_arg("105"), tick(5), vec![]);
    let opened = |arg: Option<&AppArg>| matches!(arg, Some(AppArg::Content(ContentArg::Detail { rk, .. })) if rk == "105");
    assert!(opened(d.nav.tabs.stack.pending_dest()) || opened(d.top_arg()), "the title's detail page is on its way");

    let mut d = Dispatcher::<AppHost>::new();
    for i in 0..4 { frame(&mut d, &mut rig, AppArg::Home, tick(10 + i), vec![]); }
    let (reply, rx) = std::sync::mpsc::channel();
    rig.random = Some((EntryId(9999), rx));
    reply.send(Some((crate::catalog::ServerId::UNSET, "105".into()))).unwrap();
    super::chrome_requests(&mut d, &mut rig);
    assert!(rig.random.is_none() && !d.has_pending_navigation(), "a pick for a page that has gone opens nothing");
    let (reply, rx) = std::sync::mpsc::channel::<Option<(crate::catalog::ServerId, String)>>();
    rig.random = Some((d.nav.top_page().unwrap().id, rx));
    drop(reply);
    super::chrome_requests(&mut d, &mut rig);
    assert!(rig.random.is_none() && !d.has_pending_navigation(), "a pick that never answers ends");
}

/// The sign-in pages wear no rail but show the clock, as Litefin's global clock is on every page
/// but the player's.
#[test]
fn the_clock_runs_on_the_sign_in_pages_without_the_rail() {
    let _g = nj_base::testlock::serial();
    let mut d = Dispatcher::<AppHost>::new();
    let mut rig = Bridge::for_test(|| 0);
    for i in 0..4 { frame(&mut d, &mut rig, AppArg::Profiles, tick(i), vec![]); }
    assert!(d.nav.tabs.strip.is_empty(), "no rail on Who's watching");
    assert_eq!(rig.chrome.clock().to_str().unwrap(), "12:00", "but the clock is read");
}
