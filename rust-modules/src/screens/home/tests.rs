use super::*;

use crate::ui::fixture::FixtureMeasure;
use crate::ui::focus::{FocusEngine, Outcome};
use crate::ui::hit::{HitMap, PointerKind};
use nj_machine::machine::{
    Chrome, FocusRead, Host, InputOwner, PressRead, ScreenId, Source, Stamped, Tick,
};
use crate::ui::screen::{ScreenArg, ScreenEvent};

/// How many rows the layout sweeps cover: past the data layer's own cap, which is not Home's.
const SWEEP_ROWS: usize = 40;

#[derive(Clone, PartialEq, Eq)]
struct TestArg;

impl LogicalState for TestArg {
    fn write(&self, c: &mut Canon) {
        c.u32(0);
    }
    fn probe(&self, _: &mut String) {}
}

impl ScreenArg for TestArg {
    fn chrome(&self) -> Chrome {
        Chrome::TabBar
    }
    fn id(&self) -> ScreenId {
        ScreenId(800)
    }
    fn title(&self) -> Option<&str> {
        None
    }
    fn same_instance(&self, other: &Self) -> bool {
        self == other
    }
}

struct TestHost;

impl Host for TestHost {
    type Arg = TestArg;
    type Fx = AppFx;
    type Msg = super::super::registry::AppMsg;
    type Elem = u32;
    type Views<'a> = HubsView<'a>;
    type Init = super::super::family::NoInit;
    type Memory = PageMemory;
}

impl HomeLike for TestHost {
    fn hubs<'a>(cx: &Cx<'a, Self>) -> HubsView<'a> {
        cx.views
    }
}

fn cx<'a>(view: HubsView<'a>, focus: Option<FocusKey<u32>>) -> Cx<'a, TestHost> {
    static MEASURE: FixtureMeasure = FixtureMeasure;
    Cx {
        views: view,
        tick: Tick::default(),
        measure: &MEASURE,
        press: PressRead::default(),
        focus: FocusRead { current: focus , ..Default::default() },
        owner: InputOwner::Entry(focus.map_or(EntryId(7), |k| k.entry)),
    }
}

fn screen(view: HubsView<'_>) -> HomeScreen {
    let mut s = HomeScreen::new(EntryId(7), InstanceId(9));
    s.sync_catalog(&cx(view, None));
    s.layout_grid();
    s
}

fn step(
    s: &mut HomeScreen,
    view: HubsView<'_>,
    focus: Option<FocusKey<u32>>,
    event: &ScreenEvent<TestHost>,
) -> (Handled, Vec<Stamped<TestHost>>, bool) {
    let context = cx(view, focus);
    let mut out = Vec::new();
    let mut present = nj_machine::present::Present::new();
    let handled = {
        let mut fx = Effects::new(
            &mut out,
            nj_machine::machine::MachineId::Instance(InstanceId(9)),
            &mut present,
        );
        Machine::<TestHost>::step(s, event, &context, &mut fx)
    };
    (handled, out, present.peek(0))
}

fn first_card(s: &HomeScreen) -> FocusKey<u32> {
    FocusKey {
        entry: s.entry,
        elem: s.rows[0].elems[0],
    }
}

fn has_home(out: &[Stamped<TestHost>], pred: impl Fn(&HomeReq) -> bool) -> bool {
    out.iter().any(|stamped| match &stamped.fx {
        Fx::App(AppFx::Home(req)) => pred(req),
        _ => false,
    })
}

#[test]
fn set_hero_focus_clamps_onto_the_last_drawable_pill() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let s = screen(snapshot.view());
    let mut groups = Vec::new();
    Focusable::<TestHost>::groups(&s, &cx(snapshot.view(), None), &mut groups);
    assert_eq!(groups[0].id, HERO_GROUP);
    assert_eq!(groups[0].len, 1, "the hero is one stop: its face, not a row of buttons");
    assert!(groups
        .iter()
        .all(|g| g.id != crate::ui::containers::tabs::STRIP));
}

/// RIGHT on the hero pages its slides BEFORE the engine sees it, Litefin's carousel; LEFT is the
/// engine's on every slide, so the dispatcher's door takes it into the rail.
#[test]
fn the_hero_pages_right_and_leaves_left_to_the_rail() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let mut s = screen(view);
    let hero = Some(FocusKey { entry: s.entry, elem: HERO_ELEM });
    let key = |key| ScreenEvent::Input(InputEvent {
        at: Tick::default(),
        source: Source::Sdl,
        kind: InputKind::Key { key, sym: 0, wcode: 0, edge: Edge::Down, at_edge: false },
    });
    assert_eq!(s.carousel_index(view), 0);
    let (handled, _, _) = step(&mut s, view, hero, &key(Key::Left));
    assert_eq!((handled, s.carousel_index(view)), (Handled::No, 0), "the first slide's LEFT is the engine's");
    let (handled, _, _) = step(&mut s, view, hero, &key(Key::Right));
    assert_eq!((handled, s.carousel_index(view)), (Handled::Yes, 1));
    s.hero_flip_cd = 0.0;
    let (handled, _, _) = step(&mut s, view, hero, &key(Key::Left));
    assert_eq!((handled, s.carousel_index(view)), (Handled::No, 1), "past the first, LEFT still pages nothing");
    // …and the engine never pages: the hero's sides are no edge the screen is asked about
    let mut groups = Vec::new();
    Focusable::<TestHost>::groups(&s, &cx(view, hero), &mut groups);
    assert_eq!(groups[0].edge[2..], [EdgeRule::Stop, EdgeRule::Stop]);
    // a card's LEFT/RIGHT stays the engine's
    let card = Some(first_card(&s));
    assert_eq!(step(&mut s, view, card, &key(Key::Right)).0, Handled::No);
}

/// Home declares no door to the rail: the dispatcher enters it on a LEFT the page has no answer
/// for, the same way on every page, so no screen links to it (`ui::dispatch::enter_rail`).
#[test]
fn home_links_name_no_rail_door() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 1, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let s = screen(snapshot.view());
    let mut links = Vec::new();
    <HomeScreen as Screen<TestHost>>::links(&s, &mut links);
    assert!(links.iter().all(|l| l.from != crate::ui::containers::tabs::STRIP
        && l.to != crate::ui::containers::tabs::STRIP));
}

#[test]
fn step_row_stays_inside_the_addressable_rows() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let s = screen(snapshot.view());
    let first = first_card(&s);
    assert!(matches!(
        Focusable::<TestHost>::neighbour(&s, first, Dir::Left, &cx(snapshot.view(), Some(first))),
        Step::Edge
    ));
    let last = FocusKey {
        entry: s.entry,
        elem: s.rows[0].elems[2],
    };
    assert!(matches!(
        Focusable::<TestHost>::neighbour(&s, last, Dir::Right, &cx(snapshot.view(), Some(last))),
        Step::Edge
    ));
}

#[test]
fn the_status_readout_tells_loading_empty_and_failed_apart() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    for (hub_state, kind, action) in [
        (crate::catalog_fetch::HubState::Loading, StatusKind::Working, false),
        (crate::catalog_fetch::HubState::Failed, StatusKind::Failed, true),
        (crate::catalog_fetch::HubState::Ready, StatusKind::Empty, true),
    ] {
        crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, hub_state);
        let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
        let (_, got, got_action) = status_read(snapshot.view()).unwrap();
        assert_eq!(got, kind);
        assert_eq!(got_action.is_some(), action);
    }
    for hub_state in [crate::catalog_fetch::HubState::Ready, crate::catalog_fetch::HubState::Failed] {
        crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, hub_state);
        assert!(status_read(crate::catalog_fetch::hubs_snapshot(&state).view()).is_none(),
            "a failed refresh retains playable content, not a replacement readout");
    }
}

/// **A failed Home stands on the page read-out's anchor** — the verdict hanging from
/// `StatusOverlay::FULL_ANCHOR_TOP` and *Try again* stacked `space::LG` under it (no reason), the
/// same anchor a failed Library section and a failed sign-in use (each screen's own test pins its
/// side), in the one wording every screen uses for an unreachable server. The hit rect is the drawn pill: it is
/// built from the same overlay the draw uses.
#[test]
fn a_failed_home_stands_on_the_page_readout_lines() {
    use crate::ui::widgets::StatusOverlay;
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, crate::catalog_fetch::HubState::Failed);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let (caption, kind, action) = status_read(snapshot.view()).unwrap();
    assert_eq!((caption.to_str().unwrap(), kind), ("Can\u{2019}t reach your Jellyfin server", StatusKind::Failed));
    assert_eq!(action.unwrap().to_str().unwrap(), "Try again");
    let measure = crate::ui::fixture::FixtureMeasure;
    let no_offer = OfferWatch::default();
    let overlay = status_overlay(snapshot.view(), &no_offer, &ClockWatch::default()).unwrap();
    let verdict = overlay.verdict_band_measured(&measure);
    assert_eq!(verdict.y, StatusOverlay::FULL_ANCHOR_TOP);
    let drawn = overlay.action_frame_measured(&measure).unwrap();
    assert_eq!(drawn.y, verdict.y + verdict.h + crate::ui::theme::space::LG);
    let hit = screen(snapshot.view()).hero_rect(snapshot.view(), &measure).unwrap();
    assert_eq!([hit.x, hit.y, hit.w, hit.h], [drawn.x, drawn.y, drawn.w, drawn.h]);
}

/// **A failed Home says why a wrong clock is the likely cause** — the clock glyph over the verdict
/// and a reason line in the reserved slot, only while key mode cannot help (`net::keypin::blocked`).
/// No fact is byte-for-byte today's read-out; only a Failed read-out takes the reason; an offered
/// plaintext server's reason outranks it; and the hit rect is the pill the SAME frame draws, the
/// reason having moved the action row.
#[test]
fn a_failed_home_names_a_wrong_clock_when_key_mode_cannot_help() {
    use nj_net::net::keypin::{self, Blocked};
    use crate::ui::icons::Icon;
    let _guard = nj_base::testlock::serial();
    crate::catalog::grant::reset_for_test();
    let key = keypin::key_of("home-clock.invalid", 32400);
    let _scoped = keypin::Scoped::watch_machine("home-clock-machine", &key);
    let _current = current_server_for_test("home-clock-machine");
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, crate::catalog_fetch::HubState::Failed);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let mut s = screen(view);
    let measure = FixtureMeasure;

    step(&mut s, view, None, &ScreenEvent::Tick(Tick::default()));
    let plain = status_overlay(view, &s.plaintext, &s.clock).unwrap();
    assert_eq!((plain.reason, plain.glyph, plain.action), (None, Some(Icon::ServerBadgeMinus), Some(c"Try again")));
    let plain_row = plain.action_frame_measured(&measure).unwrap();
    let plain_caption = plain.caption.to_owned();

    keypin::strict_failure(&key, 60, Some(10));
    let before_tick = status_overlay(view, &s.plaintext, &s.clock).unwrap();
    assert_eq!(before_tick.reason, None, "the held fact does not change between ticks");
    step(&mut s, view, None, &ScreenEvent::Tick(Tick::default()));
    let overlay = status_overlay(view, &s.plaintext, &s.clock).unwrap();
    assert_eq!(overlay.reason, Some(nj_platform::i18n::msg::browse_clock_no_key_c()));
    assert_eq!((overlay.glyph, overlay.action), (Some(Icon::ClockBadgeAlert), Some(c"Try again")));
    assert_eq!(overlay.caption, plain_caption.as_c_str(), "the verdict is unchanged");
    let drawn = overlay.action_frame_measured(&measure).unwrap();
    assert!(drawn.y > plain_row.y, "the reason moves the action row");
    let hit = s.hero_rect(view, &measure).unwrap();
    assert_eq!([hit.x, hit.y, hit.w, hit.h], [drawn.x, drawn.y, drawn.w, drawn.h], "the hit rect is the drawn pill");

    keypin::key_changed(&key);
    step(&mut s, view, None, &ScreenEvent::Tick(Tick::default()));
    let overlay = status_overlay(view, &s.plaintext, &s.clock).unwrap();
    assert_eq!(s.clock.blocked(), Some(Blocked::KeyChanged));
    assert_eq!(overlay.reason, Some(nj_platform::i18n::msg::browse_clock_key_changed_c()));

    // A read-out that has not failed takes no reason, whatever the fact says.
    for hub_state in [crate::catalog_fetch::HubState::Loading, crate::catalog_fetch::HubState::Ready] {
        crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, hub_state);
        let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
        let overlay = status_overlay(snapshot.view(), &s.plaintext, &s.clock).unwrap();
        assert_eq!((overlay.reason, overlay.glyph), (None, None), "{hub_state:?}");
    }

    // The plaintext offer's reason (and its glyph) win: its own cause is the one the person can act on.
    let verdict = crate::catalog::grant::PlaintextVerdict {
        machine_id: "lan-machine".into(), name: "Home".into(), shared_by: String::new(),
        eligibility: crate::catalog::probe::PlaintextEligibility::Eligible,
        choice: crate::catalog::session::PlaintextChoice::Undecided,
    };
    crate::catalog::grant::offered(crate::catalog::grant::scope(), verdict.clone());
    step(&mut s, view, None, &ScreenEvent::Tick(Tick::default()));
    let overlay = status_overlay(view, &s.plaintext, &s.clock).unwrap();
    let offer = crate::auth::plaintext_copy(Some(&verdict), crate::auth::ReadoutSurface::SignedIn);
    assert_eq!(overlay.reason.and_then(|r| r.to_str().ok()), Some(offer.as_ref()));
    assert_eq!((overlay.glyph, overlay.action), (Some(Icon::ServerBadgeMinus), Some(plaintext_question::connect())));
    crate::catalog::grant::reset_for_test();
}

/// Make `machine` the current server (the one a failed Home speaks about) for a test, and put the
/// registry back when it ends.
fn current_server_for_test(machine: &str) -> impl Drop {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            crate::catalog::reset_servers_for_test();
        }
    }
    crate::catalog::reset_servers_for_test();
    let sid = crate::catalog::register_pinned_with_client_id(machine, &crate::catalog::Origin::http("192.168.1.53", 32400),
        "", None, "client", Default::default());
    assert!(crate::catalog::set_current(sid) || crate::catalog::current_server() == sid, "the test server is current");
    Reset
}

/// **A fact about ANOTHER server does not colour a failed Home** (scenario B): Home asks
/// `net::keypin::blocked_for` about the current server only.
#[test]
fn a_failed_home_ignores_a_clock_fact_about_another_server() {
    use nj_net::net::keypin;
    let _guard = nj_base::testlock::serial();
    crate::catalog::grant::reset_for_test();
    let elsewhere = keypin::key_of("home-elsewhere.invalid", 32400);
    let _scoped = keypin::Scoped::watch_machine("home-elsewhere-machine", &elsewhere);
    let _current = current_server_for_test("home-here-machine");
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, crate::catalog_fetch::HubState::Failed);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let mut s = screen(view);

    keypin::strict_failure(&elsewhere, 60, Some(10));
    keypin::key_changed(&elsewhere);
    step(&mut s, view, None, &ScreenEvent::Tick(Tick::default()));
    assert_eq!(s.clock.blocked(), None);
    let overlay = status_overlay(view, &s.plaintext, &s.clock).unwrap();
    assert_eq!((overlay.reason, overlay.glyph), (None, Some(crate::ui::icons::Icon::ServerBadgeMinus)));
}

// The observer consumes the same final geometry as widgets::card; the screen
// supplies no independent motion signal that could omit one of these terms.
fn observe_card(h: &mut crate::ui::card_motion::History, s: &HomeScreen, ms: u32) -> crate::ui::card_motion::Verdict {
    h.begin();
    h.observe(crate::ui::card_motion::Identity { owner: 1, asset: 1 }, s.drawn_card_geometry(0, 0, 1.0).0, ms)
}

#[test]
fn the_hero_to_grid_dive_is_observed_from_card_placement_and_then_settles() {
    use crate::ui::card_motion::{History, Verdict};
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    let mut h = History::default();
    s.snap.jump(0.0);
    s.snap_target = 1.0;
    s.layout_grid();
    observe_card(&mut h, &s, 0);
    step(&mut s, snapshot.view(), None, &ScreenEvent::Tick(Tick { ms: 16, dt_us: 16_667 }));
    assert!(s.snap.vel.abs() < 10.0, "the spring remains a dimensionless fraction");
    assert_eq!(observe_card(&mut h, &s, 16), Verdict::Moving);
    let mut last = Verdict::Unknown;
    for i in 2..602 {
        step(&mut s, snapshot.view(), None, &ScreenEvent::Tick(Tick { ms: i * 16, dt_us: 16_667 }));
        last = observe_card(&mut h, &s, i * 16);
    }
    assert_eq!(last, Verdict::Settled);
}

#[test]
fn no_shelves_means_no_grid_snap() {
    assert_eq!(pinned_snap(1.0, 0), 0.0);
    assert_eq!(pinned_snap(0.0, 0), 0.0);
    assert_eq!(pinned_snap(1.0, 1), 1.0);
    assert_eq!(pinned_snap(0.0, 1), 0.0);
    assert_eq!(pinned_snap(1.0, SWEEP_ROWS), 1.0);
}

#[test]
fn the_status_screen_takes_ok_but_never_the_top_band() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, crate::catalog_fetch::HubState::Failed);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    let entry = s.entry;
    let (_, retry, _) = step(
        &mut s,
        snapshot.view(),
        Some(FocusKey {
            entry,
            elem: HERO_ELEM,
        }),
        &ScreenEvent::Activate(HERO_ELEM),
    );
    assert!(retry.iter().any(|stamped| matches!(
        &stamped.fx,
        Fx::App(AppFx::Store(StoreId::Hubs, StoreCmd::Hubs(HubsCmd::Retry)))
    )));
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Failed);
    let populated = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(populated.view());
    let (_, out, _) = step(&mut s, populated.view(), None, &ScreenEvent::Activate(HERO_ELEM));
    assert!(has_home(&out, |r| matches!(r, HomeReq::Detail { .. })), "OK on the hero opens its slide");
    assert!(!out.iter().any(|s| matches!(&s.fx,
        Fx::App(AppFx::Store(StoreId::Hubs, StoreCmd::Hubs(HubsCmd::Retry))))));
}

#[test]
fn pointer_hit_column_matches_the_drawn_card_for_both_card_styles() {
    for sty in [&RowStyle::HOME, &RowStyle::EPISODE] {
        for scroll in [0.0, 415.0, 830.0] {
            for col in 0..24 {
                assert_eq!(col_at(sty, card_x(sty, col, scroll) + sty.w * 0.5, scroll, 24), Some(col));
            }
        }
    }
}

/// The hero is the top of the page: its stop rides the page up as the rows take focus, it takes
/// no press scale (its focus is the band), and the pointer finds it over its whole face.
#[test]
fn the_hero_rides_the_page_and_takes_the_pointer_over_its_face() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    let key = FocusKey { entry: s.entry, elem: HERO_ELEM };
    for snap in [0.0, 0.3, 1.0] {
        s.snap.jump(snap);
        for press in [0.93, 1.0] {
            let mut context = cx(snapshot.view(), Some(key));
            context.press.scale = press;
            let drawn = Focusable::<TestHost>::place(&s, &key.elem, &context, At::Drawn).unwrap();
            let target = Focusable::<TestHost>::place(&s, &key.elem, &context, At::SpringTarget).unwrap();
            assert_eq!(drawn.rect, Rect::new(HERO_SEAT.x, HERO_SEAT.y - snap * PAGE_LIFT, HERO_SEAT.w, HERO_SEAT.h));
            assert_eq!(target.rect, HERO_SEAT, "the hero rests where it is with the hero up");
        }
    }
    s.snap.jump(0.3);
    let context = cx(snapshot.view(), Some(key));
    let mut frame = DrawFrame::new(&context, Painter::root());
    s.record_stops(&mut frame, snapshot.view());
    let mut map = HitMap::new();
    map.fill(frame.into_stops());
    map.swap();
    let lift = 0.3 * PAGE_LIFT;
    assert_eq!(map.resolve(Some(s.entry), PointerKind::Click, 1500.0, 200.0 - lift, Some(key)).hit, Some(key),
        "the art is the hero's face too");
    assert_ne!(map.resolve(Some(s.entry), PointerKind::Click, 1500.0, ROWS_OVERLAP_TOP + 10.0 - lift, Some(key)).hit,
        Some(key), "the rows' overlap is the rows'");
}

#[test]
fn status_action_geometry_does_not_inherit_the_previous_hero_pop_or_slide() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, crate::catalog_fetch::HubState::Failed);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    s.outgoing = Some((crate::catalog::ServerId::UNSET, "old".into()));
    s.hero_slide.jump(0.5);
    s.snap.jump(0.4);
    let key = FocusKey { entry: s.entry, elem: HERO_ELEM };
    let mut context = cx(snapshot.view(), Some(key));
    context.press.scale = 0.93;
    let base = s.hero_rect(snapshot.view(), context.measure).unwrap();
    let drawn = Focusable::<TestHost>::place(&s, &key.elem, &context, At::Drawn).unwrap();
    assert_eq!((drawn.rect.x, drawn.rect.y, drawn.rect.w, drawn.rect.h),
        (base.x, base.y, base.w, base.h));
}

#[test]
fn drawn_card_geometry_includes_press_but_its_rest_anchor_does_not() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    s.snap.jump(1.0);
    let sty = s.style(0);
    for _ in 0..80 { s.grid.shelves[0].update(3, Some(0), sty, 0.016); }
    s.layout_grid();
    let key = first_card(&s);
    for press in [0.93, 1.0, 1.025] {
        let mut context = cx(snapshot.view(), Some(key));
        context.press.scale = press;
        let placed = Focusable::<TestHost>::place(&s, &key.elem, &context, At::Drawn).unwrap();
        assert!((placed.rect.w - sty.w * s.grid.shelves[0].scale(0) * press).abs() < 0.01);
        assert!((placed.rest_rect.w - sty.w * sty.focus_scale).abs() < 0.01);
        context.press = PressRead::default();
        let opener = s.focused_rect(Some(key), &context, At::Drawn).unwrap();
        assert!((opener.w - sty.w * s.grid.shelves[0].scale(0)).abs() < 0.01);
    }
}

#[test]
fn the_card_anchor_rect_tracks_the_drawn_card_through_scroll() {
    for sty in [&RowStyle::HOME, &RowStyle::EPISODE] {
        for &scroll in &[0.0f32, 415.0, 830.0] {
            for c in 0..8usize {
                for &focus in &[1.0f32, sty.focus_scale] {
                    let base = Rect::new(card_x(sty, c, scroll), 176.0 + ROW_CARDS_DY, sty.w, sty.h);
                    let r = base.scaled(focus);
                    // magnification is about the card's CENTRE — the anchor must not slide
                    assert!(
                        (r.cx() - base.cx()).abs() < 0.001 && (r.cy() - base.cy()).abs() < 0.001,
                        "card {c} (scroll={scroll}, focus={focus}) moved its centre"
                    );
                    // …and the rect the panel is placed beside is the one the pointer would hit
                    assert_eq!(col_at(sty, r.cx(), scroll, 8), Some(c), "anchor centre for card {c} is not over it");
                    assert!(r.w >= sty.w && r.h >= sty.h, "focus must not shrink the card");
                }
            }
        }
    }
}

#[test]
fn prefetch_order_wraps_and_never_warms_the_page_on_screen() {
    let mut out = [0 as i32; 2 * HERO_PREFETCH];
    assert_eq!(
        prefetch_order(0, 0, &mut out),
        0,
        "an empty pool has no neighbours"
    );
    assert_eq!(
        prefetch_order(0, 1, &mut out),
        0,
        "…and neither does a one-page pool"
    );
    assert_eq!(
        prefetch_order(0, 2, &mut out),
        1,
        "in a two-page pool +1 and -1 are the same page"
    );
    assert_eq!(out[0], 1);

    for n in 2..=8 as i32 {
        for cur in 0..n {
            let k = prefetch_order(cur, n, &mut out);
            assert!(
                k <= 2 * HERO_PREFETCH,
                "wrote {k} entries into a {}-slot buffer",
                2 * HERO_PREFETCH
            );
            assert_eq!(
                out[0],
                (cur + 1).rem_euclid(n),
                "forward first (n={n}, cur={cur})"
            );
            for i in 0..k {
                assert!(
                    (0..n).contains(&out[i]),
                    "page {} is outside a {n}-page pool",
                    out[i]
                );
                assert_ne!(
                    out[i], cur,
                    "warmed the page already on screen (n={n}, cur={cur})"
                );
                assert!(!out[..i].contains(&out[i]), "warmed page {} twice", out[i]);
            }
        }
    }
}

#[test]
fn the_prefetch_is_armed_only_from_a_settled_hero() {
    assert!(prefetch_armed(0.0, false), "billboard up, nothing moving");
    assert!(
        !prefetch_armed(0.0, true),
        "mid-flip, the incoming layer IS the thing being waited on"
    );
    for &sp in &[0.05f32, 0.2, 1.0] {
        assert!(
            !prefetch_armed(sp, false),
            "at sp={sp} the billboard is on its way out"
        );
    }
}

/// The logo box contains a clearLogo of any shape: 400 wide at most, 130 tall at most, bottom
/// aligned in the box (`hero_logo::place`), so nothing spills out of the block.
#[test]
fn the_hero_logo_is_contained_in_its_box() {
    for (w, h) in [(1000.0f32, 100.0f32), (400.0, 400.0), (100.0, 1000.0), (800.0, 260.0)] {
        let (dw, dh) = crate::ui::hero_logo::fit(HERO_LOGO, w, h, LOGO_W);
        assert!(dw <= LOGO_W + 1e-3 && dh <= LOGO_H + 1e-3, "{w}x{h} drew {dw}x{dh}");
        assert!((dw / dh - w / h).abs() < 1e-3, "{w}x{h} kept its aspect");
        assert!((dw - LOGO_W).abs() < 1e-3 || (dh - LOGO_H).abs() < 1e-3, "{w}x{h} fills one side of the box");
    }
    assert_eq!(crate::ui::hero_logo::band_h(HERO_LOGO), LOGO_H, "the band the block reserves is the box");
}

#[test]
fn the_hero_logo_key_is_the_shows_for_an_episode() {
    let ep = PmsMovie {
        kind: 3,
        rk: "42".into(),
        show_rk: "7".into(),
        ..Default::default()
    };
    assert_eq!(
        hero_logo_rk(&ep),
        "7",
        "an episode's hero wears the show's logotype"
    );
    let orphan = PmsMovie {
        kind: 3,
        rk: "42".into(),
        ..Default::default()
    };
    assert_eq!(
        hero_logo_rk(&orphan),
        "42",
        "…falling back to its own when the server sent no parent"
    );
    let movie = PmsMovie {
        kind: 0,
        rk: "42".into(),
        show_rk: "7".into(),
        ..Default::default()
    };
    assert_eq!(
        hero_logo_rk(&movie),
        "42",
        "a movie is never keyed to a stray parent"
    );
}

/// A Continue Watching card wears its progress bar only while the offset is inside the item: never
/// started, stopped at the end and a stale offset past it draw none.
#[test]
fn a_continue_watching_card_wears_its_bar_only_while_in_progress() {
    let ep = |resume_ms: i64| PmsMovie {
        kind: 3,
        dur_ns: 45 * 60 * 1_000_000_000,
        resume_ms,
        show_title: "Laura".into(),
        ..Default::default()
    };
    for m in [ep(0), ep(45 * 60_000), ep(60 * 60_000)] {
        assert!(m.resume_frac().is_none(), "offset {} is not in progress", m.resume_ms);
    }
    assert!(ep(20 * 60_000).resume_frac().is_some(), "20 minutes into 45 IS in progress");
}

/// A Home of `rows` rows of the seeded kind, its grid focused on `focus_row` and settled there. Its
/// springs jump, which reports to the process-global idle flag: callers hold `testlock::serial()`.
fn settled_on(rows: usize, focus_row: usize) -> HomeScreen {
    let mut s = HomeScreen::new(EntryId(7), InstanceId(9));
    for row in 0..rows {
        let identity = HomeHubIdentity::Key { sid: crate::catalog::ServerId::UNSET, key: format!("/hubs/{row}").into() };
        s.rows.push(HubProjection {
            group: GroupId(FIRST_HUB_GROUP + row as u32),
            landscape: row % 3 == 0, // landscape and poster rows interleaved
            identity, elems: vec![], link: None, basis: Vec::new(),
        });
        s.grid.shelves.push(CardRow::new());
    }
    s.snap.jump(1.0);
    s.grid.scroll_target = s.row_scroll(focus_row);
    s.grid.scroll_y.jump(s.grid.scroll_target);
    s.layout_grid();
    s
}

/// Litefin's rows: the first one's title 546 down with the hero up (the rows overlap the hero by
/// 450, padded 24), and a focused row resting 40 below the top of the screen — unless the rows
/// end first, when the last one's foot rests the rows' padding above the screen's.
#[test]
fn a_focused_row_rests_40_below_the_top_unless_the_rows_end() {
    let _guard = nj_base::testlock::serial();
    let s = settled_on(4, 0);
    let mut hero_up = settled_on(4, 0);
    hero_up.snap.jump(0.0);
    hero_up.layout_grid();
    assert_eq!(hero_up.grid.shelves[0].base_y, 546.0);
    for rows in 1..=SWEEP_ROWS {
        for focus_row in 0..rows {
            let s = settled_on(rows, focus_row);
            let top = s.grid.shelves[focus_row].base_y;
            let foot = s.grid.shelves[rows - 1].base_y + s.pitch(rows - 1);
            if top != ROW_FOCUS_TOP {
                assert!(top > ROW_FOCUS_TOP, "rows {rows} focus {focus_row}: rose past 40 to {top}");
                assert!((foot + ROWS_PAD_BOTTOM - SCR_H).abs() < 1e-3, "rows {rows} focus {focus_row}: held at {top} with the foot at {foot}");
            }
            // the focused row's cards and labels are on screen
            let bottom = top + ROW_CARDS_DY + s.style(focus_row).h + card_row::TileLabel::height(true);
            assert!(bottom <= SCR_H, "rows {rows} focus {focus_row}: its labels end at {bottom}");
        }
    }
    let _ = s;
}

/// The title at x=160 in its 48 px line box; the first card box at x=172 and 86 under the title
/// line's top, so its picture at 178 and 92; 28 between one row's card boxes and the next row's
/// title, which makes a poster row exactly the shared shelf pitch.
#[test]
fn the_rows_keep_litefins_measures() {
    let _guard = nj_base::testlock::serial();
    let s = settled_on(3, 1);
    assert_eq!(ROW_TITLE_X, 160.0);
    for sty in [&RowStyle::HOME, &RowStyle::EPISODE] {
        assert_eq!(card_x(sty, 0, 0.0) - CARD_INSET, 172.0);
        assert_eq!(card_x(sty, 1, 0.0) - card_x(sty, 0, 0.0), sty.w + sty.gap);
    }
    assert_eq!(ROW_CARDS_DY - CARD_INSET, 86.0);
    for row in 0..2 {
        let box_foot = s.grid.shelves[row].base_y + ROW_CARDS_DY + s.style(row).h
            + card_row::TileLabel::height(true) + (CARD_INSET - theme::FOCUS_RING_W);
        let gap = s.grid.shelves[row + 1].base_y - (box_foot + 38.0);
        assert!((gap - 28.0).abs() < 1e-3, "row {row}: {gap}");
    }
    assert!(!s.rows[1].landscape);
    assert_eq!(s.pitch(1), ROW_PITCH, "a poster row is the shared shelf pitch");
    for landscape in [false, true] {
        assert_eq!(style(landscape).centre_x, Some(1010.0), "rows centre their focus in x 100..1920");
    }
    let cap = title_cap_top(100.0, &FixtureMeasure);
    let caps = FixtureMeasure.cap_h(theme::size::lf::S32);
    assert!((cap + caps * 0.5 - 124.0).abs() < 1e-3, "the caps sit centred in the 48 px line");
}

struct Run {
    text: String,
    dx: f32,
    sz: i32,
    bold: i32,
    ink: [f32; 4],
}
fn width_of(text: &str, size: i32, bold: i32) -> f32 {
    text.chars().count() as f32 * (size as f32 + 6.0 * bold as f32)
}
fn heading_flow(title: &str, source: &str) -> (f32, Vec<Run>) {
    let mut runs = Vec::new();
    let width = card_row::heading_flow(title, source, |text, dx, size, bold, ink| {
        runs.push(Run {
            text: text.into(),
            dx,
            sz: size,
            bold,
            ink,
        });
        width_of(text, size, bold)
    });
    (width, runs)
}

#[test]
fn a_shelf_with_no_source_draws_exactly_the_title_and_nothing_else() {
    let (w, runs) = heading_flow("Recently Added", "");
    assert_eq!(
        runs.len(),
        1,
        "an empty source must produce no further runs at all"
    );
    assert_eq!(runs[0].text, "Recently Added");
    assert_eq!(runs[0].dx, 0.0, "the title starts at the heading origin");
    assert_eq!(
        w,
        width_of("Recently Added", theme::size::HEADLINE, 1),
        "the title's own advance, exactly"
    );
}

#[test]
fn a_shared_source_extends_the_heading_past_the_title() {
    let (bare, _) = heading_flow("Recently Added in Film Club", "");
    let (annotated, runs) = heading_flow("Recently Added in Film Club", "friend");
    assert!(
        annotated > bare,
        "the annotation must extend the heading ({annotated} vs {bare})"
    );
    assert_eq!(runs.len(), 3, "title, separator, handle");
    assert_eq!(runs[0].dx, 0.0, "the title still starts at the origin");
    assert_eq!(
        annotated,
        bare + 2.0 * SOURCE_PAD
            + width_of("\u{b7}", theme::size::BODY, 0)
            + width_of("friend", theme::size::BODY, 0),
        "the growth is exactly the dot, the handle and one pad either side of the dot"
    );
    assert_eq!(
        runs[1].dx,
        bare + SOURCE_PAD,
        "the dot is one pad past the title"
    );
    assert_eq!(
        runs[2].dx,
        runs[1].dx + width_of("\u{b7}", theme::size::BODY, 0) + SOURCE_PAD,
        "the handle is one pad past the dot"
    );
}

#[test]
fn each_heading_run_carries_its_own_size_weight_and_ink() {
    let (_, runs) = heading_flow("Recently Added in Film Club", "friend");
    assert_eq!(
        (runs[0].sz, runs[0].bold),
        (theme::size::HEADLINE, 1),
        "the title is HEADLINE bold"
    );
    assert_eq!(
        runs[0].ink,
        theme::TEXT_HEADING,
        "…in the shared section-heading ink"
    );
    assert_eq!(runs[1].text, "\u{b7}");
    assert_eq!(
        (runs[1].sz, runs[1].bold),
        (theme::size::BODY, 0),
        "the separator is measured at BODY regular"
    );
    assert_eq!(
        runs[1].ink,
        theme::TEXT_SEPARATOR,
        "…at the separator token's own .45"
    );
    assert_eq!(runs[2].text, "friend");
    assert_eq!(
        (runs[2].sz, runs[2].bold),
        (theme::size::BODY, 0),
        "the handle is BODY regular, not the title's"
    );
    assert_eq!(runs[2].ink, theme::TEXT_TERTIARY);
    assert!(
        (runs[1].sz, runs[1].bold) == (runs[2].sz, runs[2].bold),
        "the dot and the handle are one annotation: same rung, same weight, so they drop onto the title's baseline together"
    );
    assert!(runs[1].sz < runs[0].sz, "the annotation is a rung DOWN from the title — the drop is why it must be baseline-aligned");
}

struct MetaRun {
    text: String,
    dx: f32,
    budget: f32,
    sz: i32,
    bold: i32,
    ink: [f32; 4],
}
fn meta_flow(base: f32, source: &str) -> (f32, Vec<MetaRun>) {
    let mut runs = Vec::new();
    let width = meta_source_flow(base, source, |text, dx, budget, size, bold, ink| {
        runs.push(MetaRun {
            text: text.into(),
            dx,
            budget,
            sz: size,
            bold,
            ink,
        });
        width_of(text, size, bold).min(budget.max(0.0))
    });
    (width, runs)
}

#[test]
fn a_hero_from_our_own_server_draws_no_source_run_at_all() {
    let (w, runs) = meta_flow(420.0, "");
    assert!(
        runs.is_empty(),
        "an empty source must produce no runs, no pad and no dot"
    );
    assert_eq!(w, 420.0, "…and must not move the line's own end by a pixel");
}

#[test]
fn a_borrowed_hero_states_its_owner_as_the_last_run_on_the_line() {
    let base = 420.0;
    let (w, runs) = meta_flow(base, "friend");
    assert_eq!(runs.len(), 2, "the separator and the run, and nothing else");
    assert_eq!(runs[0].text, "\u{b7}");
    assert_eq!(
        runs[0].dx,
        base + SOURCE_PAD,
        "the dot is one pad past the facts"
    );
    assert_eq!(
        runs[1].text, "Shared by friend",
        "the person, not the machine"
    );
    assert_eq!(
        runs[1].dx,
        runs[0].dx + width_of("\u{b7}", theme::size::lf::S21, 0) + SOURCE_PAD,
        "the run is one pad past the dot"
    );
    assert_eq!(
        w,
        base + 2.0 * SOURCE_PAD
            + width_of("\u{b7}", theme::size::lf::S21, 0)
            + width_of("Shared by friend", theme::size::lf::S21, 0),
        "the line grows by exactly the dot, the run and one pad either side of the dot"
    );
}

#[test]
fn the_hero_source_run_keeps_the_lines_rung_and_takes_one_step_of_ink() {
    let (_, runs) = meta_flow(420.0, "friend");
    for r in &runs {
        assert_eq!(
            (r.sz, r.bold),
            (theme::size::lf::S21, 0),
            "'{}' left the meta line's own rung",
            r.text
        );
    }
    assert_eq!(
        runs[0].ink,
        theme::TEXT_SEPARATOR,
        "the middot carries the separator token's own .45"
    );
    assert_eq!(
        runs[1].ink,
        theme::TEXT_TERTIARY,
        "one step under the line's own ink, never level with it"
    );
    assert_ne!(runs[1].ink, theme::home_lf::META_INK);
}

#[test]
fn an_over_long_handle_truncates_rather_than_wrapping() {
    let ridiculous = "a-very-long-plex-account-handle-that-nobody-would-ever-choose";
    for base in [0.0f32, 420.0, 660.0] {
        let (w, runs) = meta_flow(base, ridiculous);
        assert_eq!(
            runs.len(),
            2,
            "base {base}: a long handle is still ONE run — there is no second line to go to"
        );
        for r in &runs {
            assert!(
                r.budget > 0.0,
                "base {base}: '{}' was given no room at all ({})",
                r.text,
                r.budget
            );
            assert!(
                r.dx + r.budget <= SYN_W + 1e-3,
                "base {base}: '{}' may elide past the bound",
                r.text
            );
        }
        assert!(
            w <= SYN_W + 1e-3,
            "base {base}: the line ran to {w}, past its {SYN_W} bound"
        );
    }
    // …and the room left at that worst case is a real annotation's worth rather than a stub —
    // a QUARTER of the whole line, which is the guard that a future widening of the hero column
    // (or a tightening of the bound) cannot quietly starve the run into a bare ellipsis. It is
    // stated as a share of the line and not in pixels of text because the synthetic metric here
    // is roughly twice the shipped font's advance; the share is a claim about the geometry,
    // which is the part the host can actually speak for.
    let (_, worst) = meta_flow(660.0, "friend");
    assert!(
        worst[1].budget >= 0.25 * SYN_W,
        "a meta line whose facts fill the column leaves the run only {} of {SYN_W}",
        worst[1].budget
    );
}

/// The meta line: year, runtime, the rating as a badge, the community score after its star at one
/// decimal; each present fact an item and nothing for an absent one.
#[test]
fn the_meta_line_is_year_runtime_rating_badge_and_star() {
    let _lang = nj_platform::i18n::language_on_this_thread_for_test(nj_platform::i18n::Preference::En);
    let film = PmsMovie { year: 2010, dur_ns: 112 * 60 * 1_000_000_000, rating: "PG-13".into(), score: 7.14,
        ..Default::default() };
    assert_eq!(meta_runs(&film), [
        MetaItem::Text("2010".into()),
        MetaItem::Text(nj_platform::i18n::msg::home_runtime_hours(1, 52)),
        MetaItem::Badge("PG-13".into()),
        MetaItem::Star("7.1".into()),
    ]);
    let short = PmsMovie { dur_ns: 51 * 60 * 1_000_000_000 + 40_000_000_000, ..Default::default() };
    assert_eq!(meta_runs(&short), [MetaItem::Text(nj_platform::i18n::msg::home_runtime_minutes(52))], "to the nearest minute");
    assert_eq!(meta_runs(&PmsMovie { score: 8.0, ..Default::default() }), [MetaItem::Star("8.0".into())], "one decimal");
    assert!(meta_runs(&PmsMovie::default()).is_empty());
}

// Additional phase-8 ownership proofs: real engine movement, stable identity, effects and map.

#[test]
fn engine_links_hero_to_the_first_shelf_and_back() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let s = screen(snapshot.view());
    let owner = InputOwner::Entry(s.entry);
    let hero = FocusKey {
        entry: s.entry,
        elem: HERO_ELEM,
    };
    let context = cx(snapshot.view(), Some(hero));
    let mut engine = FocusEngine::new();
    engine.set(owner, hero, Some(HERO_GROUP), By::Restore);
    let mut links = Vec::new();
    <HomeScreen as Screen<TestHost>>::links(&s, &mut links);
    let Outcome::Moved { to, .. } = engine.move_dir(owner, &s, &links, Dir::Down, &context) else {
        panic!("hero DOWN must move")
    };
    assert_eq!(to.elem, s.rows[0].elems[0]);
}

#[test]
fn down_from_the_first_shelf_chooses_the_next_shelf_not_the_folded_hero() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 2, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    let second_elem = s.elem_for(HomeItemIdentity::Item {
        hub: HomeHubIdentity::Key {
            sid: crate::catalog::ServerId::UNSET,
            key: "/hubs/second".into(),
        },
        sid: crate::catalog::ServerId::UNSET,
        rk: "second".into(),
    });
    s.rows.push(HubProjection {
        identity: HomeHubIdentity::Key {
            sid: crate::catalog::ServerId::UNSET,
            key: "/hubs/second".into(),
        },
        group: GroupId(FIRST_HUB_GROUP + 1),
        elems: vec![second_elem],
        link: None,
        landscape: false,
        basis: Vec::new(),
    });
    s.grid.shelves.push(CardRow::new());
    s.elem_at.insert(second_elem, (1, 0));
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    s.layout_grid();

    let first = first_card(&s);
    let owner = InputOwner::Entry(s.entry);
    let context = cx(snapshot.view(), Some(first));
    let mut engine = FocusEngine::new();
    engine.set(owner, first, Some(s.rows[0].group), By::Restore);
    let mut links = Vec::new();
    <HomeScreen as Screen<TestHost>>::links(&s, &mut links);
    let Outcome::Moved { to, .. } = engine.move_dir(owner, &s, &links, Dir::Down, &context) else {
        panic!("first-shelf DOWN must reach the second shelf");
    };
    assert_eq!(to.elem, second_elem);
}

#[test]
fn down_from_the_last_shelf_never_reenters_the_offscreen_hero() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 2, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    s.layout_grid();
    let last = FocusKey {
        entry: s.entry,
        elem: *s.rows[0].elems.last().unwrap(),
    };
    let owner = InputOwner::Entry(s.entry);
    let context = cx(snapshot.view(), Some(last));
    let mut engine = FocusEngine::new();
    engine.set(owner, last, Some(s.rows[0].group), By::Restore);
    let mut links = Vec::new();
    <HomeScreen as Screen<TestHost>>::links(&s, &mut links);
    assert_eq!(
        engine.move_dir(owner, &s, &links, Dir::Down, &context),
        Outcome::Nothing
    );
}

#[test]
fn repeated_item_keys_are_scoped_by_hub_identity() {
    let mut s = HomeScreen::new(EntryId(7), InstanceId(9));
    let a = HomeItemIdentity::Item {
        hub: HomeHubIdentity::ContinueWatching,
        sid: crate::catalog::ServerId::UNSET,
        rk: "7".into(),
    };
    let b = HomeItemIdentity::Item {
        hub: HomeHubIdentity::Key {
            sid: crate::catalog::ServerId::UNSET,
            key: "/hubs/new".into(),
        },
        sid: crate::catalog::ServerId::UNSET,
        rk: "7".into(),
    };
    let ka = s.elem_for(a.clone());
    assert_eq!(s.elem_for(a), ka);
    assert_ne!(s.elem_for(b), ka);
}

#[test]
fn unknown_provider_identity_is_explicitly_generation_scoped() {
    assert_ne!(
        HomeHubIdentity::Ephemeral {
            generation: 4,
            ordinal: 2
        },
        HomeHubIdentity::Ephemeral {
            generation: 5,
            ordinal: 2
        },
    );
}

#[test]
fn memory_round_trip_preserves_registries_and_carousel_identity() {
    let mut a = HomeScreen::new(EntryId(7), InstanceId(9));
    let hub = HomeHubIdentity::ContinueWatching;
    a.group_for(&hub);
    a.elem_for(HomeItemIdentity::Item {
        hub,
        sid: crate::catalog::ServerId::UNSET,
        rk: "42".into(),
    });
    a.carousel = Some((crate::catalog::ServerId::UNSET, "42".into()));
    a.strip_chosen = true;
    let memory = match <HomeScreen as Screen<TestHost>>::memory(&a) {
        PageMemory::Home(m) => m,
        _ => unreachable!(),
    };
    let mut b = HomeScreen::new(EntryId(7), InstanceId(10));
    b.restore(&memory);
    assert_eq!(b.groups, a.groups);
    assert_eq!(b.items, a.items);
    assert_eq!(b.carousel, a.carousel);
    assert_eq!(b.strip_chosen, a.strip_chosen);
}

/// OK on a My Media tile opens THAT library by its section identity, never a detail page.
#[test]
fn a_my_media_tile_opens_its_own_library() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_named_hubs_for_test(&mut state, &adapter, 2, &[(HUB_MY_MEDIA, "", "My Media")]);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    s.snap.jump(1.0);
    assert!(s.my_media(0));
    let tile = first_card(&s);
    let item = s.item_at(snapshot.view(), 0, 0).expect("a tile").clone();
    let (_, out, _) = step(&mut s, snapshot.view(), Some(tile), &ScreenEvent::Activate(tile.elem));
    let want = crate::screens::registry::LibrarySectionIdentity { sid: item.sid, key: item.sec };
    assert!(has_home(&out, |r| *r == HomeReq::Library(want.clone())), "{:?}", out.len());
    assert!(!has_home(&out, |r| matches!(r, HomeReq::Detail { .. })));
}

/// With no hero pool (nothing on the server has a backdrop), the billboard falls back to the
/// first row of titles, never to My Media's libraries: a library is no title to open a detail page
/// on. With My Media alone there is no billboard at all.
#[test]
fn an_empty_hero_pool_never_puts_a_library_on_the_billboard() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_named_hubs_for_test(&mut state, &adapter, 2,
        &[(HUB_MY_MEDIA, "", "My Media"), ("test.row", "", "Picks")]);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    assert_eq!(view.hero_count(), 0, "no pool");
    let s = screen(view);
    let hero = s.hero_item(&cx(view, None)).expect("the first row of titles fills the billboard");
    assert!(std::ptr::eq(hero, &view.hub(1).unwrap().items[0]), "not a library: {}", hero.rk);

    crate::catalog_fetch::seed_named_hubs_for_test(&mut state, &adapter, 2, &[(HUB_MY_MEDIA, "", "My Media")]);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let mut s = screen(view);
    assert!(s.hero_item(&cx(view, None)).is_none(), "libraries alone are no billboard");
    let hero = FocusKey { entry: s.entry, elem: HERO_ELEM };
    let (_, out, _) = step(&mut s, view, Some(hero), &ScreenEvent::Activate(HERO_ELEM));
    assert!(!has_home(&out, |r| matches!(r, HomeReq::Detail { .. })), "OK opens no library as a title");
}

#[test]
fn activation_across_the_snap_midpoint_is_not_a_canonical_collision() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 2, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut hero_picture = screen(snapshot.view());
    let mut grid_picture = screen(snapshot.view());
    for s in [&mut hero_picture, &mut grid_picture] {
        s.carousel = Some((crate::catalog::ServerId::UNSET, "2".into()));
        s.snap_target = 1.0;
        s.visible_activation = Some(HERO_ELEM);
    }
    hero_picture.snap.jump(0.49);
    grid_picture.snap.jump(0.51);
    let card = first_card(&hero_picture);
    let event = ScreenEvent::PressCommit(nj_machine::machine::PressId(1));
    let (_, a, _) = step(&mut hero_picture, snapshot.view(), Some(card), &event);
    let (_, b, _) = step(&mut grid_picture, snapshot.view(), Some(card), &event);
    assert!(has_home(&a, |r| matches!(r, HomeReq::Detail { rk, .. } if rk == "2")));
    assert!(has_home(&b, |r| matches!(r, HomeReq::Play { rk, .. } if rk == "1")));
    assert_ne!(hero_picture.hash(), grid_picture.hash(),
        "the same input activates different items, so these cannot be the same logical state");
}

#[test]
fn the_home_census_covers_input_motion_and_current_projection() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let baseline = screen(snapshot.view()).hash();
    let changes: &[fn(&mut HomeScreen)] = &[
        |s| s.snap.vel = 1.0,
        |s| s.hero_slide.pos = 0.4,
        |s| s.hero_slide.vel = 1.0,
        |s| s.outgoing = Some((crate::catalog::ServerId::UNSET, "old".into())),
        |s| s.grid.scroll_y.vel = 1.0,
        |s| s.grid.scroll_target = 100.0,
        |s| s.rows[0].elems.swap(0, 1),
        |s| Arc::make_mut(&mut s.items)[0].last_row += 1,
        |s| Arc::make_mut(&mut s.items)[0].last_col += 1,
        |s| s.projected_generation = None,
        |s| s.hero_band.pos = 0.4,
        |s| s.hero_band.vel = 1.0,
        |s| s.hero_zoom = 3.0,
        |s| s.grid.shelves[0].update(3, Some(1), &RowStyle::HOME, 0.016),
    ];
    for (i, change) in changes.iter().enumerate() {
        let mut s = screen(snapshot.view());
        change(&mut s);
        assert_ne!(s.hash(), baseline, "census omitted input-state variation {i}");
    }
    // This extent is part of SHAPE, not merely a runtime sequence length.
    assert_eq!(crate::ui::card_row::MAX_ROW_ITEMS, 24);
}

/// `person` and `search` cap their shelves at the data layer's `pms::MAX_SHELF_ITEMS`; the card row
/// that draws them owns `ui::card_row::MAX_ROW_ITEMS` springs. The data layer cannot name `ui` and
/// `ui` cannot name the data layer, so there are two constants for one number and this is the only
/// place that sees both.
#[test]
fn the_data_shelf_cap_is_the_card_rows_capacity() {
    assert_eq!(crate::catalog_fetch::MAX_SHELF_ITEMS, crate::ui::card_row::MAX_ROW_ITEMS);
}

#[test]
fn paint_only_backdrop_and_spinner_state_do_not_change_the_canonical_hash() {
    let mut a = HomeScreen::new(EntryId(7), InstanceId(9));
    let mut b = HomeScreen::new(EntryId(7), InstanceId(9));
    b.status_ms = 900.0;
    b.backdrop.art.pos = 0.5;
    b.outgoing_zoom = 5.0;
    assert_eq!(a.hash(), b.hash());
    a.carousel = Some((crate::catalog::ServerId::UNSET, "a".into()));
    assert_ne!(a.hash(), b.hash());
}

#[test]
fn an_explicit_hero_reseat_keeps_the_fold_animation_unlike_page_restoration() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 2, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    let from = first_card(&s);
    let to = FocusKey { entry: s.entry, elem: HERO_ELEM };
    step(&mut s, snapshot.view(), Some(to), &ScreenEvent::FocusMoved {
        from: Some(from), to, by: By::Restore,
    });
    assert_eq!(s.snap_target, 0.0);
    assert_eq!(s.snap.pos, 1.0, "fresh reseating still animates the door");
}

#[test]
fn shelf_viewports_follow_identity_across_a_catalog_reorder() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_grid_for_test(&mut state, &adapter, 2, 24);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    let identity = s.rows[0].identity.clone();
    s.grid.shelves[0].restore_scroll(900.0, 24, &RowStyle::HOME);
    crate::catalog_fetch::reverse_test_hubs(&mut state);
    let changed = crate::catalog_fetch::hubs_snapshot(&state);
    s.sync_catalog(&cx(changed.view(), None));
    assert_eq!(s.rows[1].identity, identity);
    assert_eq!(s.grid.shelves[1].scroll_x(), 900.0);
    assert_eq!(s.grid.shelves[0].scroll_x(), 0.0);
}

#[test]
fn the_grid_holds_one_motion_row_per_published_row() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_grid_for_test(&mut state, &adapter, 5, 24);
    let five = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(five.view());
    assert_eq!(s.rows.len(), 5);
    assert_eq!(s.grid.shelves.len(), s.rows.len());
    let kept = s.rows[1].identity.clone();
    s.grid.shelves[1].restore_scroll(900.0, 24, &RowStyle::HOME);
    crate::catalog_fetch::seed_grid_for_test(&mut state, &adapter, 3, 24);
    let three = crate::catalog_fetch::hubs_snapshot(&state);
    s.sync_catalog(&cx(three.view(), None));
    assert_eq!(s.rows.len(), 3);
    assert_eq!(s.grid.shelves.len(), 3, "the grid shrinks with the published rows");
    assert_eq!(s.rows[1].identity, kept);
    assert_eq!(s.grid.shelves[1].scroll_x(), 900.0, "a surviving row keeps its motion");
    assert_eq!(s.grid.shelves[0].scroll_x(), 0.0);
    assert_eq!(s.grid.shelf(9).scroll_x(), 0.0, "a stale row index reads a resting row");
}

#[test]
fn an_empty_loading_publication_does_not_consume_restored_viewports() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_grid_for_test(&mut state, &adapter, 2, 24);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut original = screen(snapshot.view());
    original.grid.shelves[1].restore_scroll(900.0, 24, &RowStyle::HOME);
    let PageMemory::Home(memory) = <HomeScreen as Screen<TestHost>>::memory(&original) else { unreachable!() };
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, crate::catalog_fetch::HubState::Loading);
    let loading = crate::catalog_fetch::hubs_snapshot(&state);
    let mut restored = screen(loading.view());
    restored.restore(&memory);
    restored.sync_catalog(&cx(loading.view(), None));
    // Another eviction while loading must not discard the still-unmatched saved rows.
    let PageMemory::Home(pending) = <HomeScreen as Screen<TestHost>>::memory(&restored) else { unreachable!() };
    let mut restored = screen(loading.view());
    restored.restore(&pending);
    crate::catalog_fetch::seed_grid_for_test(&mut state, &adapter, 2, 24);
    let ready = crate::catalog_fetch::hubs_snapshot(&state);
    restored.sync_catalog(&cx(ready.view(), None));
    assert_eq!(restored.grid.shelves[1].scroll_x(), 900.0);
}

#[test]
fn viewport_memory_is_canonical_because_return_reuses_it() {
    let a = HomeScreen::new(EntryId(7), InstanceId(9));
    let mut b = HomeScreen::new(EntryId(7), InstanceId(9));
    b.grid.scroll_y.pos = 320.0;
    assert_ne!(a.hash(), b.hash());
    assert_ne!(<HomeScreen as Screen<TestHost>>::memory(&a).hash(),
        <HomeScreen as Screen<TestHost>>::memory(&b).hash());
}

#[test]
fn visible_tick_emits_both_store_work_requests_after_the_step() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, crate::catalog_fetch::HubState::Loading);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    let (_, out, _) = step(
        &mut s,
        snapshot.view(),
        None,
        &ScreenEvent::Tick(Tick {
            ms: 16,
            dt_us: 16_000,
        }),
    );
    let work: Vec<_> = out
        .iter()
        .filter_map(|stamped| match stamped.fx {
            Fx::App(AppFx::StoreWork(work)) => Some(work),
            _ => None,
        })
        .collect();
    assert_eq!(work, vec![StoreWork::Hubs, StoreWork::BrowseDiscovery]);
}

#[test]
fn continue_watching_commit_plays_while_an_ordinary_shelf_opens_detail() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 2, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    let card = first_card(&s);
    let (_, play, _) = step(
        &mut s,
        snapshot.view(),
        Some(card),
        &ScreenEvent::PressCommit(nj_machine::machine::PressId(1)),
    );
    assert!(has_home(
        &play,
        |r| matches!(r, HomeReq::Play { rk, .. } if rk == "1")
    ));
    s.rows[0].identity = HomeHubIdentity::Key {
        sid: crate::catalog::ServerId::UNSET,
        key: "/hubs/recent".into(),
    };
    let (_, detail, _) = step(
        &mut s,
        snapshot.view(),
        Some(card),
        &ScreenEvent::PressCommit(nj_machine::machine::PressId(2)),
    );
    assert!(has_home(
        &detail,
        |r| matches!(r, HomeReq::Detail { rk, .. } if rk == "1")
    ));
}

#[test]
fn holding_a_shelf_card_opens_the_item_menu_without_activation() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 1, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    let card = first_card(&s);
    let (_, out, _) = step(
        &mut s,
        snapshot.view(),
        Some(card),
        &ScreenEvent::PressHold(nj_machine::machine::PressId(1)),
    );
    assert!(has_home(
        &out,
        |r| matches!(r, HomeReq::ItemMenu { rk, .. } if rk == "1")
    ));
    assert!(!has_home(&out, |r| matches!(
        r,
        HomeReq::Play { .. } | HomeReq::Detail { .. }
    )));
}

#[test]
fn a_partially_visible_focused_row_allows_hover_to_its_neighbors_only() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 2, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    s.snap.jump(1.0);
    s.grid.shelves[0].update(2, Some(0), &RowStyle::HOME, 1.0);
    s.layout_grid();
    s.grid.shelves[0].base_y = -20.0 - ROW_CARDS_DY;
    let first = first_card(&s);
    let neighbor = s.rows[0].elems[1];
    for (focus, expected) in [(Some(first), Hover::Focus), (None, Hover::OnlyIfFocused)] {
        let context = cx(snapshot.view(), focus);
        let mut frame = DrawFrame::new(&context, Painter::root());
        s.record_stops(&mut frame, snapshot.view());
        let stops = frame.into_stops();
        let stop = stops.iter().find(|stop| stop.key.elem == neighbor).unwrap();
        assert!(stop.rest_rect.y < 40.0);
        assert_eq!(stop.hover, expected);
    }
}

#[test]
fn drawn_stops_feed_the_real_hit_map_with_scoped_card_keys() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 2, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    let card = first_card(&s);
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    s.grid.shelves[0].update(2, Some(0), &RowStyle::HOME, 1.0);
    s.layout_grid();
    let context = cx(snapshot.view(), Some(card));
    let mut frame = DrawFrame::new(&context, Painter::root());
    s.record_stops(&mut frame, snapshot.view());
    let mut map = HitMap::new();
    map.fill(frame.into_stops());
    map.swap();
    let placed = Focusable::<TestHost>::place(&s, &card.elem, &context, At::Drawn).unwrap();
    let resolution = map.resolve(Some(card.entry), PointerKind::Click,
        placed.rect.cx(),
        placed.rect.cy(),
        Some(card),
    );
    assert_eq!(resolution.hit, Some(card));
    assert_eq!(resolution.activate, Some((card, Activate::Press)));
}

#[test]
fn quick_down_then_ok_activates_the_hero_still_visible_before_the_snap_midpoint() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 2, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    s.carousel = Some((crate::catalog::ServerId::UNSET, "2".into()));
    let hero = FocusKey {
        entry: s.entry,
        elem: HERO_ELEM,
    };
    let card = first_card(&s);
    let move_event = ScreenEvent::FocusMoved {
        from: Some(hero),
        to: card,
        by: By::Dir,
    };
    let _ = step(&mut s, snapshot.view(), Some(card), &move_event);
    assert_eq!(s.snap_target, 1.0);
    assert!(s.snap.pos < 0.5);
    let (_, out, _) = step(
        &mut s,
        snapshot.view(),
        Some(card),
        &ScreenEvent::PressCommit(nj_machine::machine::PressId(7)),
    );
    assert!(has_home(
        &out,
        |req| matches!(req, HomeReq::Detail { rk, .. } if rk == "2")
    ));
    assert!(!has_home(
        &out,
        |req| matches!(req, HomeReq::Play { rk, .. } if rk == "1")
    ));
}

#[test]
fn back_from_grid_folds_to_hero_before_root_back() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 1, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    let card = first_card(&s);
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    let back = ScreenEvent::Input(InputEvent {
        at: Tick::default(),
        source: Source::Sdl,
        kind: InputKind::Key {
            key: Key::Back,
            sym: 0,
            wcode: 0,
            edge: Edge::Down,
            at_edge: false,
        },
    });
    let (_, fold, _) = step(&mut s, snapshot.view(), Some(card), &back);
    assert_eq!(s.snap_target, 0.0);
    assert!(has_home(&fold, |req| matches!(req, HomeReq::FoldToHero)));
    assert!(!fold
        .iter()
        .any(|stamped| matches!(&stamped.fx, Fx::App(AppFx::Loop(LoopReq::BackAtRoot)))));

    s.snap.jump(0.0);
    let hero = FocusKey {
        entry: s.entry,
        elem: HERO_ELEM,
    };
    let (_, root, _) = step(&mut s, snapshot.view(), Some(hero), &back);
    assert!(root
        .iter()
        .any(|stamped| matches!(&stamped.fx, Fx::App(AppFx::Loop(LoopReq::BackAtRoot)))));
}

fn enter_target(out: &[Stamped<TestHost>]) -> Option<FocusTarget<u32>> {
    out.iter().find_map(|stamped| match &stamped.fx {
        Fx::Deliver(
            MachineId::Instance(InstanceId(9)),
            Delivery::Screen(ScreenEvent::Enter(Enter::Fresh { focus })),
        ) => Some(*focus),
        _ => None,
    })
}

#[test]
fn addressed_focus_commands_reseat_only_through_enter_fresh() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 2, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    let card = first_card(&s);

    let (_, grid, _) = step(
        &mut s,
        snapshot.view(),
        None,
        &ScreenEvent::App(AppMsg::Home(HomeCmd::FocusGrid { row: 0, col: 0 })),
    );
    assert!(matches!(enter_target(&grid), Some(FocusTarget::Elem(key)) if key == card));

    let (_, hero, _) = step(
        &mut s,
        snapshot.view(),
        Some(card),
        &ScreenEvent::App(AppMsg::Home(HomeCmd::Hero)),
    );
    assert!(matches!(
        enter_target(&hero),
        Some(FocusTarget::ContainerGroup(HERO_GROUP))
    ));
}

#[test]
fn addressed_carousel_commands_mutate_the_owned_identity_not_focus() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());

    let (handled, _, _) = step(
        &mut s,
        snapshot.view(),
        None,
        &ScreenEvent::App(AppMsg::Home(HomeCmd::SelectHero(2))),
    );
    assert_eq!(handled, Handled::Yes);
    assert_eq!(s.carousel.as_ref().map(|(_, rk)| rk.as_str()), Some("3"));

    s.hero_flip_cd = 0.0;
    let before = s.carousel.clone();
    let _ = step(
        &mut s,
        snapshot.view(),
        None,
        &ScreenEvent::App(AppMsg::Home(HomeCmd::Flip(1))),
    );
    assert_ne!(s.carousel, before);
}

#[test]
fn loading_has_no_phantom_hero_and_terminal_status_has_one_action() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    for (hub_state, expected) in [
        (crate::catalog_fetch::HubState::Loading, 0),
        (crate::catalog_fetch::HubState::Failed, 1),
        (crate::catalog_fetch::HubState::Ready, 1),
    ] {
        crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, hub_state);
        let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
        let s = screen(snapshot.view());
        let context = cx(snapshot.view(), None);
        let mut groups = Vec::new();
        Focusable::<TestHost>::groups(&s, &context, &mut groups);
        assert_eq!(groups[0].len, expected);
        let retry = FocusKey {
            entry: s.entry,
            elem: HERO_ELEM,
        };
        assert!(matches!(
            Focusable::<TestHost>::neighbour(&s, retry, Dir::Right, &context),
            Step::Edge
        ));
    }
}

#[test]
fn first_catalog_landing_reseats_the_default_cta_unless_the_strip_was_chosen() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, crate::catalog_fetch::HubState::Loading);
    let empty = crate::catalog_fetch::hubs_snapshot(&state);
    let mut automatic = screen(empty.view());
    let fallback = FocusKey {
        entry: automatic.entry,
        elem: crate::ui::dispatch::STRIP_BASE + 4,
    };

    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 1, crate::catalog_fetch::HubState::Ready);
    let landed = crate::catalog_fetch::hubs_snapshot(&state);
    let (_, out, _) = step(
        &mut automatic,
        landed.view(),
        Some(fallback),
        &ScreenEvent::Tick(Tick {
            ms: 16,
            dt_us: 16_000,
        }),
    );
    assert!(matches!(
        enter_target(&out),
        Some(FocusTarget::ContainerGroup(HERO_GROUP))
    ));

    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, crate::catalog_fetch::HubState::Loading);
    let empty = crate::catalog_fetch::hubs_snapshot(&state);
    let mut chosen = screen(empty.view());
    let moved = ScreenEvent::FocusMoved {
        from: None,
        to: fallback,
        by: By::Pointer,
    };
    let _ = step(&mut chosen, empty.view(), Some(fallback), &moved);
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 1, crate::catalog_fetch::HubState::Ready);
    let landed = crate::catalog_fetch::hubs_snapshot(&state);
    let (_, out, _) = step(
        &mut chosen,
        landed.view(),
        Some(fallback),
        &ScreenEvent::Tick(Tick {
            ms: 16,
            dt_us: 16_000,
        }),
    );
    assert!(enter_target(&out).is_none());
}

#[test]
fn addressed_item_menu_uses_the_current_owned_grid_item() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 1, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    let card = first_card(&s);
    let (_, out, _) = step(
        &mut s,
        snapshot.view(),
        Some(card),
        &ScreenEvent::App(AppMsg::Home(HomeCmd::ItemMenu)),
    );
    assert!(has_home(
        &out,
        |req| matches!(req, HomeReq::ItemMenu { rk, .. } if rk == "1")
    ));
}

#[test]
fn parent_read_only_api_projects_engine_focus_without_setters() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 2, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let s = screen(snapshot.view());
    let card = first_card(&s);
    let context = cx(snapshot.view(), Some(card));

    assert!(SHAPE.contains("visible_activation:Option<u32>"));
    assert_eq!(
        s.hero_item(&context).map(|item| item.rk.as_str()),
        Some("1")
    );
    assert_eq!(
        s.focused_item(Some(card), &context)
            .map(|item| item.rk.as_str()),
        Some("1")
    );
    assert_eq!(s.grid_position(Some(card), &context), Some((0, 0)));
    assert_eq!(s.snap_target(), 0.0);
    assert!(s.focused_rect(Some(card), &context, At::Drawn).is_some());

    // The lifted redraw query is safe with no focused key and never reaches a global cursor.
    let empty_context = cx(snapshot.view(), None);
    let mut frame = DrawFrame::new(&empty_context, Painter::root());
    s.redraw_focused(&mut frame, None);
}

/// Ticks `s` for `frames` frames of 16 ms, focused on `focus`, and counts the frames the present
/// gate would open for it: a present the page asked for, or a spring that moved. `take` at tick 0
/// keeps the keepalive out, so this counts only what the screen itself wants.
fn presents(s: &mut HomeScreen, view: HubsView<'_>, focus: Option<FocusKey<u32>>, frames: u32) -> u32 {
    let context = cx(view, focus);
    let mut present = nj_machine::present::Present::new();
    present.take(0);
    let mut asked = 0;
    for frame in 1..=frames {
        nj_machine::idle::frame_begin(1.0 / 60.0);
        let mut out = Vec::new();
        {
            let mut fx = Effects::new(&mut out, nj_machine::machine::MachineId::Instance(InstanceId(9)), &mut present);
            Machine::<TestHost>::step(s, &ScreenEvent::Tick(Tick { ms: frame * 16, dt_us: 16_000 }), &context, &mut fx);
        }
        asked += u32::from(present.take(0) | nj_machine::idle::present_moving());
    }
    asked
}

/// **A hero left alone presents every frame, and every one of them is the hero's.** Litefin's Ken
/// Burns zooms the slide on screen over 12 s (`docs/litefin-look.md` 2.5) and the 8 s auto-advance
/// restarts it at every flip, so with several slides the zoom never runs out: each frame is the
/// zoom growing or the crossfade, never the countdown, which is a TIMER and reports nothing. The
/// flip still lands on time. [`the_hero_is_the_only_thing_that_keeps_home_presenting`] is the other
/// half: put the hero away and the same page is silent.
#[test]
fn a_hero_left_alone_zooms_every_frame_and_flips_on_time() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    assert!(view.hero_count() > 1, "the auto-advance only arms with more than one hero slot");
    assert!(HERO_AUTO_S < crate::ui::landing_hero::ZOOM_S, "a flip comes before the zoom is whole");
    let mut s = screen(view);
    let context = cx(view, None);
    let mut present = nj_machine::present::Present::new();
    present.take(0);
    let (mut asleep, mut not_the_hero, mut flipped_at) = (0u32, 0u32, None);
    // 10 s of 16 ms ticks: past HERO_AUTO_S, so the countdown expires exactly once.
    for frame in 1..=625u32 {
        let zoom = s.hero_zoom;
        let mut out = Vec::new();
        {
            let mut fx = Effects::new(&mut out, nj_machine::machine::MachineId::Instance(InstanceId(9)), &mut present);
            let tick = ScreenEvent::Tick(Tick { ms: frame * 16, dt_us: 16_000 });
            Machine::<TestHost>::step(&mut s, &tick, &context, &mut fx);
        }
        let asked = present.take(0);
        if s.outgoing.is_some() {
            flipped_at.get_or_insert(frame);
        }
        asleep += u32::from(!asked);
        not_the_hero += u32::from(asked && s.outgoing.is_none() && s.hero_zoom == zoom);
    }
    let flipped_at = flipped_at.expect("the countdown must still end in a flip");
    assert!(
        (flipped_at as f32 * 0.016 - HERO_AUTO_S).abs() < 0.05,
        "flipped after {flipped_at} ticks, not after {HERO_AUTO_S} s"
    );
    assert_eq!(asleep, 0, "the hero on screen presents every frame");
    assert_eq!(not_the_hero, 0, "every present is the hero's zoom or its crossfade");
    let since_flip = (625 - flipped_at + 1) as f32 * 0.016;
    assert!((s.hero_zoom - since_flip).abs() < 0.05, "the flip restarted the zoom: {} s", s.hero_zoom);
}

/// **The hero is the only thing that keeps Home presenting.** The several-slide Home that presents
/// every frame left alone on its hero is silent once the hero is put away: covered by a modal,
/// pinned by the screenshot pipeline, or scrolled off by the rows, for longer than a countdown.
/// On the first row the art still shows behind it, so its zoom runs out its 12 s and then that
/// page rests too: the countdown never holds a frame open. A single slide is the same once its
/// zoom is whole ([`the_ken_burns_zoom_presents_while_it_grows_and_rests_when_whole`]).
#[test]
fn the_hero_is_the_only_thing_that_keeps_home_presenting() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let names: Vec<(String, String)> = (0..6).map(|r| (format!("test.recent.{r}"), format!("Row {r}"))).collect();
    let rows: Vec<(&str, &str, &str)> = names.iter().map(|(id, title)| (id.as_str(), "", title.as_str())).collect();
    crate::catalog_fetch::seed_named_hubs_for_test(&mut state, &adapter, 3, &rows);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    assert!(view.hero_count() > 1, "several slides: the hero would rotate and zoom forever");
    let ten_s = 625;
    assert_eq!(presents(&mut screen(view), view, None, ten_s), ten_s, "left alone, the hero presents");

    let card = |s: &HomeScreen, row: usize| Some(FocusKey { entry: s.entry, elem: s.rows[row].elems[0] });
    let pin = ScreenEvent::App(AppMsg::Home(HomeCmd::PinHero(0)));
    for (label, settle) in [("covered", 60), ("pinned", 60), ("scrolled away", 240), ("on the first row", 800)] {
        let mut s = screen(view);
        let focus = match label {
            "covered" => {
                step(&mut s, view, None, &ScreenEvent::Cover);
                None
            }
            "pinned" => {
                step(&mut s, view, None, &pin);
                None
            }
            _ => {
                s.snap_target = 1.0;
                card(&s, if label == "scrolled away" { 3 } else { 0 })
            }
        };
        let selected = s.carousel.clone();
        presents(&mut s, view, focus, settle);
        assert_eq!(s.page_y() >= HERO_ART_END, label == "scrolled away", "{label}: where the art is");
        assert_eq!(presents(&mut s, view, focus, ten_s), 0, "{label}: Home must be silent at rest");
        assert_eq!(s.carousel, selected, "{label}: no flip");
    }
}

/// **Ken Burns moves, and stops.** The slide on screen grows 1.00 -> 1.08 over 12 s: every one of
/// those frames presents, and once the zoom is whole (one slide, so nothing flips) the page goes
/// quiet. A covered, pinned or scrolled-away hero does not zoom at all.
#[test]
fn the_ken_burns_zoom_presents_while_it_grows_and_rests_when_whole() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 1, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    assert_eq!(view.hero_count(), 1, "one slide: nothing flips");
    let mut s = screen(view);
    // the first six seconds: zooming, presenting every frame, halfway grown
    assert_eq!(presents(&mut s, view, None, 375), 375);
    assert!((s.zooms().0 - 1.04).abs() < 1e-3, "{}", s.zooms().0);
    // …on to twelve, where it holds at the full scale and asks for nothing more
    presents(&mut s, view, None, 400);
    assert_eq!(s.zooms().0, crate::ui::landing_hero::ZOOM_MAX);
    assert_eq!(presents(&mut s, view, None, 625), 0, "a whole zoom is a still page");

    for (label, hold) in [
        ("covered", (|s: &mut HomeScreen| s.covered = true) as fn(&mut HomeScreen)),
        ("pinned", |s: &mut HomeScreen| s.hero_pinned = true),
        ("scrolled away", |s: &mut HomeScreen| { s.snap.jump(1.0); s.snap_target = 1.0; s.grid.scroll_target = 10_000.0; s.grid.scroll_y.jump(10_000.0) }),
    ] {
        let mut s = screen(view);
        hold(&mut s);
        assert_eq!(presents(&mut s, view, None, 60), 0, "{label}: a still page");
        assert_eq!(s.hero_zoom, 0.0, "{label}: the zoom holds");
    }
}

/// Ticks `s` for `frames` frames of 16 ms, focused on `focus`, and counts the frames on which a
/// spring told the idle gate the page moved.
fn moving_frames(s: &mut HomeScreen, view: HubsView<'_>, focus: Option<FocusKey<u32>>, frames: u32) -> u32 {
    let mut moving = 0;
    for frame in 1..=frames {
        nj_machine::idle::frame_begin(1.0 / 60.0);
        step(s, view, focus, &ScreenEvent::Tick(Tick { ms: frame * 16, dt_us: 16_000 }));
        moving += u32::from(nj_machine::idle::page_moving());
    }
    moving
}

/// **The focused hero's band moves, and stops.** Focus on the hero fades the accent band in over
/// ~300 ms, its spring telling the gate the page moves while it does, then the page is still;
/// focus leaving fades it out the same.
#[test]
fn the_focus_band_moves_while_it_fades_and_rests_when_settled() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 1, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let mut s = screen(view);
    s.hero_zoom = crate::ui::landing_hero::ZOOM_S; // the zoom is done: only the band can move
    let hero = Some(FocusKey { entry: s.entry, elem: HERO_ELEM });
    assert_eq!(moving_frames(&mut s, view, hero, 6), 6, "the band fades in");
    moving_frames(&mut s, view, hero, 60);
    assert_eq!(s.hero_band.pos, 1.0, "…and lands exactly");
    assert_eq!(moving_frames(&mut s, view, hero, 60), 0, "a settled band is a still page");
    let card = Some(first_card(&s));
    assert!(moving_frames(&mut s, view, card, 6) > 0, "focus leaving fades it out");
    moving_frames(&mut s, view, card, 120);
    assert_eq!(s.hero_band.pos, 0.0);
}

#[test]
fn the_hero_does_not_advance_while_a_modal_covers_home() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    assert!(view.hero_count() > 1, "the auto-advance only arms with more than one hero slot");
    let mut s = screen(view);
    let selected = s.carousel.clone();

    step(&mut s, view, None, &ScreenEvent::Cover);
    for frame in 1..=625u32 {
        step(
            &mut s,
            view,
            None,
            &ScreenEvent::Tick(Tick { ms: frame * 16, dt_us: 16_000 }),
        );
    }

    assert_eq!(
        s.carousel, selected,
        "a Compact modal still ticks its host, but covering Home must pause its hero"
    );
    assert!(s.outgoing.is_none(), "no hidden hero slide was started under the modal");
}

#[test]
fn the_hero_countdown_restarts_when_the_last_modal_is_dismissed() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let mut s = screen(view);
    s.hero_auto = 0.25;
    let selected = s.carousel.clone();

    step(&mut s, view, None, &ScreenEvent::Cover);
    // Navigation emits Uncover only when the last modal surface is dismissed.
    step(&mut s, view, None, &ScreenEvent::Uncover);
    step(
        &mut s,
        view,
        None,
        &ScreenEvent::Tick(Tick { ms: 16, dt_us: 16_000 }),
    );

    assert_eq!(s.carousel, selected, "dismissal must not trigger an immediate hero jump");
    assert!(s.outgoing.is_none(), "dismissal must not begin a hidden hero slide");
    assert!(
        (s.hero_auto - (HERO_AUTO_S - 0.016)).abs() < f32::EPSILON,
        "the fresh countdown should have one ordinary tick consumed, got {}",
        s.hero_auto
    );
}

/// `HomeCmd::PinHero` — the screenshot pipeline's hero pin (`/tmp/nativejelly-heropin=<n>`) —
/// selects that slot and HOLDS it: the auto-advance never fires, however long the page sits
/// uncovered, so a capture taken at any settled moment shows the same billboard.
#[test]
fn a_pinned_hero_never_auto_advances() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let mut s = screen(view);
    let (handled, _, _) = step(&mut s, view, None, &ScreenEvent::App(AppMsg::Home(HomeCmd::PinHero(1))));
    assert_eq!(handled, Handled::Yes);
    assert_eq!(s.carousel.as_ref().map(|(_, rk)| rk.as_str()), Some("2"));
    // 20 s of 16 ms ticks: more than twice HERO_AUTO_S.
    for frame in 1..=1250u32 {
        step(&mut s, view, None, &ScreenEvent::Tick(Tick { ms: frame * 16, dt_us: 16_000 }));
        assert!(s.outgoing.is_none(), "the pinned hero began a slide at tick {frame}");
    }
    assert_eq!(s.carousel.as_ref().map(|(_, rk)| rk.as_str()), Some("2"));
}

/// **A signed-in person has a consent path from Home** (PLX-NATIVE-10, code review 3). A failed
/// Home while discovery offers "Connect without encryption?" for a server says why in the reason
/// slot and makes *Connect* the primary; *Connect* asks the SHARED question
/// (`screens::plaintext_question`) seated on *Not now* instead of retrying, and only its answer
/// reaches Session. Once answered *Not now*, the primary is *Try again* again and the reason
/// points at Settings.
#[test]
fn a_failed_home_over_an_offered_server_asks_the_shared_question() {
    use crate::catalog::session::PlaintextChoice;
    let _guard = nj_base::testlock::serial();
    crate::catalog::grant::reset_for_test();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 0, crate::catalog_fetch::HubState::Failed);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let mut s = screen(view);
    let entry = s.entry;
    let verdict = crate::catalog::grant::PlaintextVerdict {
        machine_id: "lan-machine".into(), name: "Home".into(), shared_by: String::new(),
        eligibility: crate::catalog::probe::PlaintextEligibility::Eligible, choice: PlaintextChoice::Undecided,
    };
    crate::catalog::grant::offered(crate::catalog::grant::scope(), verdict.clone());
    step(&mut s, view, None, &ScreenEvent::Tick(Tick::default()));
    let measure = FixtureMeasure;
    let overlay = status_overlay(view, &s.plaintext, &s.clock).unwrap();
    assert_eq!(overlay.action, Some(plaintext_question::connect()));
    let reason = crate::auth::plaintext_copy(Some(&verdict), crate::auth::ReadoutSurface::SignedIn);
    assert_eq!(overlay.reason.and_then(|r| r.to_str().ok()), Some(reason.as_ref()));
    let drawn = overlay.action_frame_measured(&measure).unwrap();
    let hit = s.hero_rect(view, &measure).unwrap();
    assert_eq!([hit.x, hit.y, hit.w, hit.h], [drawn.x, drawn.y, drawn.w, drawn.h], "the hit rect is the drawn pill");

    let hero = Some(FocusKey { entry, elem: HERO_ELEM });
    let (_, opened, _) = step(&mut s, view, hero, &ScreenEvent::Activate(HERO_ELEM));
    let retries = |out: &[Stamped<TestHost>]| out.iter().any(|st| matches!(&st.fx,
        Fx::App(AppFx::Store(StoreId::Hubs, StoreCmd::Hubs(HubsCmd::Retry)))));
    assert!(!retries(&opened), "Connect asks; it does not retry");
    assert!(s.plaintext_alert.is_open());
    assert!(opened.iter().any(|st| matches!(&st.fx, Fx::Deliver(_, Delivery::Screen(ScreenEvent::Enter(
        Enter::Fresh { focus: FocusTarget::ContainerGroup(g) }))) if *g == PLAINTEXT_GROUP)));
    let context = cx(view, None);
    let mut groups = Vec::new();
    Focusable::<TestHost>::groups(&s, &context, &mut groups);
    assert_eq!(groups.iter().map(|g| g.id).collect::<Vec<_>>(), [PLAINTEXT_GROUP], "the question traps focus");
    let from = Placed { rect: Rect::FULL, rest_rect: Rect::FULL, clip: Rect::FULL, index: None };
    assert_eq!(Focusable::<TestHost>::seat(&s, PLAINTEXT_GROUP, from, &context).elem, PLAINTEXT_CANCEL_ELEM,
        "seated on Not now");

    let connect = Some(FocusKey { entry, elem: PLAINTEXT_CONNECT_ELEM });
    let (_, answered, _) = step(&mut s, view, connect, &ScreenEvent::PressCommit(nj_machine::machine::PressId(1)));
    let answers: Vec<_> = answered.iter().filter_map(|st| match &st.fx {
        Fx::App(AppFx::Session(crate::auth::SessionCmd::AnswerPlaintext { machine_id, choice, .. }))
            if machine_id == "lan-machine" => Some(*choice),
        _ => None,
    }).collect();
    assert_eq!(answers, [PlaintextChoice::Allowed]);
    assert!(!s.plaintext_alert.is_open());

    crate::catalog::grant::answer("account", "lan-machine", PlaintextChoice::Declined);
    step(&mut s, view, None, &ScreenEvent::Tick(Tick::default()));
    let overlay = status_overlay(view, &s.plaintext, &s.clock).unwrap();
    assert_eq!(overlay.action, Some(plaintext_question::try_again()));
    assert!(overlay.reason.and_then(|r| r.to_str().ok()).is_some_and(|r|
        r.contains("Settings \u{2192} Unencrypted connections")), "{:?}", overlay.reason);
    let (_, retried, _) = step(&mut s, view, hero, &ScreenEvent::Activate(HERO_ELEM));
    assert!(retries(&retried), "an answered question is not put again from a failure");
    assert!(!s.plaintext_alert.is_open());
    crate::catalog::grant::reset_for_test();
}

// ---- linked collection shelves (#205) -------------------------------------------------------

const COLLECTION_ROWS: [(&str, &str, &str); 3] = [
    ("movie.recentlyadded.1", "/hubs/sections/1/recentlyAdded", "Recently Added"),
    ("custom.collection.1.50001.50001", "/library/collections/50001/children", "Toy Story Collection"),
    ("movie.recentlyviewed.1", "/hubs/sections/1/recentlyViewed", "Recently Viewed"),
];

fn collection_home(
    state: &mut crate::catalog_fetch::PmsState,
    adapter: &std::sync::Arc<crate::catalog_fetch::PmsAdapter>,
) -> crate::catalog_fetch::HubsSnapshot {
    crate::catalog_fetch::seed_named_hubs_for_test(state, adapter, 6, &COLLECTION_ROWS);
    crate::catalog_fetch::hubs_snapshot(state)
}

fn on_grid(s: &mut HomeScreen) {
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    s.layout_grid();
}

fn heading_key(s: &HomeScreen, row: usize) -> FocusKey<u32> {
    FocusKey { entry: s.entry, elem: heading_elem(s.rows[row].group) }
}

fn move_from(
    s: &HomeScreen,
    view: HubsView<'_>,
    engine: &mut FocusEngine<u32>,
    dir: Dir,
) -> Outcome<u32> {
    let owner = InputOwner::Entry(s.entry);
    let focus = engine.read(owner);
    let context = Cx { focus, ..cx(view, None) };
    let mut links = Vec::new();
    <HomeScreen as Screen<TestHost>>::links(s, &mut links);
    engine.move_dir(owner, s, &links, dir, &context)
}

#[test]
fn only_a_promoted_collection_shelf_gets_a_linked_heading() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let snapshot = collection_home(&mut state, &adapter);
    let s = screen(snapshot.view());
    assert_eq!(s.rows.len(), 3);
    assert!(s.linked(0).is_none() && s.linked(2).is_none());
    let linked = s.linked(1).expect("the custom.collection row links to its collection");
    assert_eq!((linked.sec, linked.rk.as_str()), (1, "50001"));
    assert_eq!(s.locate(heading_elem(s.rows[0].group)), None,
        "an unlinked shelf has no heading stop to land on");
    assert_eq!(s.locate(heading_key(&s, 1).elem), Some(Located::Heading(1)));
    let mut groups = Vec::new();
    let context = cx(snapshot.view(), None);
    Focusable::<TestHost>::groups(&s, &context, &mut groups);
    let headings: Vec<_> = groups.iter().filter(|g| g.id.0 & HEADING_BASE != 0).collect();
    assert_eq!(headings.len(), 1, "exactly one heading stop: the collection's");
    assert_eq!(headings[0].id, heading_group(s.rows[1].group));
    assert_eq!(headings[0].len, 1);
    // Member cards stay exactly as they were: no collection tile, no SEE ALL tile.
    assert_eq!(s.rows[1].elems.len(), 6);
}

#[test]
fn up_from_any_collection_card_reaches_the_heading_and_down_returns_to_that_card() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let snapshot = collection_home(&mut state, &adapter);
    let mut s = screen(snapshot.view());
    on_grid(&mut s);
    let owner = InputOwner::Entry(s.entry);
    let card = FocusKey { entry: s.entry, elem: s.rows[1].elems[4] };
    let mut engine = FocusEngine::new();
    engine.set(owner, card, Some(s.rows[1].group), By::Restore);

    let Outcome::Moved { to, .. } = move_from(&s, snapshot.view(), &mut engine, Dir::Up) else {
        panic!("UP from a collection card must move");
    };
    assert_eq!(to, heading_key(&s, 1), "UP lands on the linked heading, not the shelf above");
    for dir in [Dir::Left, Dir::Right] {
        assert_eq!(move_from(&s, snapshot.view(), &mut engine, dir), Outcome::Nothing,
            "LEFT/RIGHT are inert on the heading");
    }
    let Outcome::Moved { to, .. } = move_from(&s, snapshot.view(), &mut engine, Dir::Down) else {
        panic!("DOWN from the heading must return to the shelf");
    };
    assert_eq!(to, card, "DOWN restores the remembered card, not the one under the heading");

    // UP from the heading goes on to the shelf above.
    engine.set(owner, heading_key(&s, 1), Some(heading_group(s.rows[1].group)), By::Restore);
    let Outcome::Moved { to, .. } = move_from(&s, snapshot.view(), &mut engine, Dir::Up) else {
        panic!("UP from the heading must reach the shelf above");
    };
    assert!(s.rows[0].elems.contains(&to.elem));
}

#[test]
fn down_from_the_shelf_above_stops_on_the_heading_and_plain_shelves_keep_projection() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let snapshot = collection_home(&mut state, &adapter);
    let mut s = screen(snapshot.view());
    on_grid(&mut s);
    let owner = InputOwner::Entry(s.entry);
    let mut engine = FocusEngine::new();
    engine.set(owner, FocusKey { entry: s.entry, elem: s.rows[0].elems[2] },
        Some(s.rows[0].group), By::Restore);
    let Outcome::Moved { to, .. } = move_from(&s, snapshot.view(), &mut engine, Dir::Down) else {
        panic!("DOWN from the shelf above must move");
    };
    assert_eq!(to, heading_key(&s, 1));

    // The plain shelf below the collection is entered from its cards and has no heading stop.
    engine.set(owner, FocusKey { entry: s.entry, elem: s.rows[2].elems[0] },
        Some(s.rows[2].group), By::Restore);
    let Outcome::Moved { to, .. } = move_from(&s, snapshot.view(), &mut engine, Dir::Up) else {
        panic!("UP from the plain shelf must move");
    };
    assert!(s.rows[1].elems.contains(&to.elem),
        "UP from a plain shelf enters the collection's cards, not its heading");
}

#[test]
fn ok_on_the_linked_heading_opens_the_collection_page() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let snapshot = collection_home(&mut state, &adapter);
    let mut s = screen(snapshot.view());
    on_grid(&mut s);
    let heading = heading_key(&s, 1);
    let (_, out, _) = step(&mut s, snapshot.view(), Some(heading), &ScreenEvent::Activate(heading.elem));
    let pushed: Vec<_> = out.iter().filter_map(|stamped| match &stamped.fx {
        Fx::App(AppFx::Content(ContentReq::Push(arg))) => Some(arg.clone()),
        _ => None,
    }).collect();
    assert_eq!(pushed.len(), 1);
    assert!(matches!(&pushed[0], ContentArg::Collection(id)
        if id.rk == "50001" && id.sec == 1 && id.tag == 0 && id.name == "Toy Story Collection"), "{:?}", pushed[0]);
    assert!(!has_home(&out, |r| matches!(r, HomeReq::Detail { .. } | HomeReq::Play { .. })),
        "the heading opens the collection, never a member");
}

#[test]
fn the_heading_keeps_focus_on_the_grid_and_reveals_its_row() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let snapshot = collection_home(&mut state, &adapter);
    let mut s = screen(snapshot.view());
    on_grid(&mut s);
    let from = FocusKey { entry: s.entry, elem: s.rows[1].elems[0] };
    let heading = heading_key(&s, 1);
    step(&mut s, snapshot.view(), Some(heading),
        &ScreenEvent::FocusMoved { from: Some(from), to: heading, by: By::Dir });
    assert_eq!(s.snap_target, 1.0, "a heading is on the shelves, not the hero");
    assert!(s.visible_activation.is_none());
    // BACK from the heading folds to the hero like any shelf focus.
    let back = ScreenEvent::Input(InputEvent {
        at: Tick::default(),
        source: Source::Sdl,
        kind: InputKind::Key { key: Key::Back, sym: 0, wcode: 0, edge: Edge::Down, at_edge: false },
    });
    let (_, out, _) = step(&mut s, snapshot.view(), Some(heading), &back);
    assert!(has_home(&out, |r| matches!(r, HomeReq::FoldToHero)));
}

#[test]
fn the_linked_heading_is_a_hover_focus_stop_that_wins_over_the_cards() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let snapshot = collection_home(&mut state, &adapter);
    let mut s = screen(snapshot.view());
    on_grid(&mut s);
    let card = FocusKey { entry: s.entry, elem: s.rows[1].elems[0] };
    let context = cx(snapshot.view(), Some(card));
    let mut frame = DrawFrame::new(&context, Painter::root());
    s.record_stops(&mut frame, snapshot.view());
    let stops = frame.into_stops();
    let heading = heading_key(&s, 1);
    let stop = stops.iter().find(|stop| stop.key == heading).expect("the heading registers a stop");
    assert_eq!(stop.hover, Hover::Focus);
    assert!(stops.iter().all(|stop| stop.key.elem != heading_elem(s.rows[0].group)),
        "an unlinked heading is not a pointer target");
    let placed = Focusable::<TestHost>::place(&s, &heading.elem, &context, At::Drawn).unwrap();
    assert_eq!((stop.rect.x, stop.rect.y, stop.rect.w, stop.rect.h),
        (placed.rect.x, placed.rect.y, placed.rect.w, placed.rect.h), "the hit rect is the drawn face");
    let mut map = HitMap::new();
    map.fill(stops);
    map.swap();
    let resolution = map.resolve(Some(heading.entry), PointerKind::Click,
        placed.rect.cx(), placed.rect.cy(), Some(card));
    assert_eq!(resolution.hit, Some(heading));
}

#[test]
fn back_from_the_collection_page_returns_focus_to_the_heading() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let snapshot = collection_home(&mut state, &adapter);
    let original = screen(snapshot.view());
    let heading = heading_key(&original, 1);
    let PageMemory::Home(memory) = <HomeScreen as Screen<TestHost>>::memory(&original) else {
        unreachable!()
    };
    // An evicted page remounts from its memory; the return state names the heading key.
    let mut restored = HomeScreen::new(EntryId(7), InstanceId(9));
    restored.restore(&memory);
    restored.sync_catalog(&cx(snapshot.view(), None));
    let context = cx(snapshot.view(), Some(heading));
    assert_eq!(Focusable::<TestHost>::reconcile(&restored, heading, &context), heading,
        "the heading is a stable key: the restored page lands back on it");
}

/// A shelf whose cards are still below the screen's bottom edge already has its heading on screen:
/// the title's line box is the top of the row, 86 above the cards. Culling the shelf by its card rect alone left that
/// heading undrawn until the first card pixel crossed the edge, so scrolling down the grid made the
/// next shelf's title pop in at full ink instead of sliding up with the page.
#[test]
fn a_shelf_heading_on_screen_is_drawn_while_its_cards_are_still_below_the_edge() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let mut s = screen(view);
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    s.layout_grid();
    let title = view.hub(0).unwrap().title.to_string();
    let drawn = |s: &HomeScreen| {
        nj_gfx::text::capture_text_runs_for_test(|| {
            let env = s.env(0.0);
            s.draw_grid(view, &env, Painter::recording(), 1.0, None, None, &FixtureMeasure);
        })
        .iter()
        .any(|run| run.contains(title.as_str()))
    };
    // Cards 4px under the bottom edge; the heading's line is well inside the screen.
    s.grid.shelves[0].base_y = SCR_H + 4.0 - ROW_CARDS_DY;
    assert!(title_cap_top(s.grid.shelves[0].base_y, &FixtureMeasure) < SCR_H - 20.0);
    assert!(drawn(&s), "the heading above an off-screen card row must still be drawn");
    // …and a shelf whose heading has not reached the edge yet is still culled.
    s.grid.shelves[0].base_y = SCR_H + 4.0;
    assert!(!drawn(&s), "a shelf entirely below the screen draws nothing");
}

thread_local! {
    /// What [`HeroArtSpy`] was asked, and which backdrops it has "decoded". Thread-local, like the
    /// source slot and the render cache it stands in front of.
    static HERO_ART: std::cell::RefCell<HeroArtState> = std::cell::RefCell::new(HeroArtState::default());
}

#[derive(Default)]
struct HeroArtState {
    /// Every DRAW probe (`resolve_tex_wh_on`) — demand, LRU-touched and evict-protected.
    probed: Vec<String>,
    /// Every speculative warm (`warm_tex_on`).
    warmed: Vec<String>,
    /// Paths whose pixels were handed to the render cache; a path's index is its `PosterKey`.
    delivered: Vec<String>,
    /// Shelf posters still in flight — Home's ordinary state for its first seconds on screen.
    busy: bool,
}

/// A poster source that answers READY only for what the test delivered, and records the rest.
struct HeroArtSpy;
impl crate::ui::tex::Source for HeroArtSpy {
    fn probe(&self, _: u16, path: &str, _: i32, _: i32, _: bool) -> Option<nj_machine::machine::PosterKey> {
        HERO_ART.with(|a| {
            let mut a = a.borrow_mut();
            a.probed.push(path.into());
            a.delivered
                .iter()
                .position(|p| p == path)
                .map(|i| nj_machine::machine::PosterKey(i as u32))
        })
    }
    fn warm(&self, _: u16, path: &str, _: i32, _: i32, _: bool) -> crate::ui::tex::Warm {
        HERO_ART.with(|a| a.borrow_mut().warmed.push(path.into()));
        crate::ui::tex::Warm::Claimed
    }
    fn logo(&self, _: u16, _: &str) -> Option<nj_machine::machine::PosterKey> {
        None
    }
    fn logo_warm(&self, _: u16, _: &str) -> crate::ui::tex::Warm {
        crate::ui::tex::Warm::Known
    }
    fn unresident(&self, _: nj_machine::machine::PosterKey, _: bool) {}
    fn idle(&self) -> bool {
        HERO_ART.with(|a| !a.borrow().busy)
    }
}

fn hero_tick(ms: u32) -> ScreenEvent<TestHost> {
    ScreenEvent::Tick(Tick { ms, dt_us: 16_000 })
}

/// Right on the hero: the press that flips the billboard by hand, which the page takes before the
/// engine sees it.
fn hero_right() -> ScreenEvent<TestHost> {
    ScreenEvent::Input(InputEvent {
        at: Tick::default(),
        source: Source::Sdl,
        kind: InputKind::Key { key: Key::Right, sym: 0, wcode: 0, edge: Edge::Down, at_edge: false },
    })
}

/// Decode and upload `path` through the real render cache, as the poster workers and the frame's
/// PREPARE step would: from here on a draw probe of it resolves to a resident texture.
fn deliver_hero_art(path: &str) {
    struct StubUp(u32);
    impl crate::ui::tex::Uploader for StubUp {
        fn upload(&mut self, d: &crate::ui::tex::Decoded) -> crate::ui::tex::Tex {
            self.0 += 1;
            crate::ui::tex::Tex { id: 100 + self.0, w: d.w, h: d.h }
        }
        fn warm(&mut self, _: crate::ui::tex::Tex) {}
        fn free(&mut self, _: crate::ui::tex::Tex) {}
    }
    let key = HERO_ART.with(|a| {
        let mut a = a.borrow_mut();
        if let Some(i) = a.delivered.iter().position(|p| p == path) {
            return i;
        }
        a.delivered.push(path.into());
        a.delivered.len() - 1
    });
    crate::ui::tex::accept(crate::ui::tex::PosterReady {
        key: nj_machine::machine::PosterKey(key as u32),
        result: Ok(crate::ui::tex::Decoded { w: 16, h: 9, rgba: vec![0; 16 * 9 * 4].into_boxed_slice() }),
    });
    let mut budget = crate::ui::frame::Budget::new();
    budget.begin_frame(0);
    let mut present = nj_machine::present::Present::new();
    let mut handle = nj_machine::machine::PresentHandle::of(&mut present);
    let mut up = StubUp(key as u32 * 10);
    crate::ui::tex::prepare(&mut budget, &mut up, &mut handle, || 0);
}

/// **Owner field report (2026-09-30): a MANUAL hero flip blinks; the 8 s auto-advance never does.**
/// A phone capture showed the outgoing backdrop sliding off over the flat ground and the new one
/// fading in only afterwards: the incoming backdrop was not resident when the flip began.
///
/// Both paths call the same `flip`, so the difference was purely WHEN the neighbour's backdrop had
/// been asked for. The ±1 prefetch was a speculative `warm` that ran only on a settled billboard
/// AND a completely idle poster pipeline — never while a shelf was still loading, never during the
/// slide — and a warmed slot is the source's first LRU victim, which a warm never revives. Eight
/// seconds of idling hid all of that from the auto-advance; a press a second after the previous
/// flip found nothing requested.
///
/// The billboard now holds both neighbours' backdrops as DRAW demand (the one request path the
/// source LRU-protects and re-arms) from the moment an index is shown, busy pipeline or not, and
/// re-arms the new neighbours on the flip itself — the same mechanism for both paths.
#[test]
fn a_manual_hero_flip_lands_on_a_preloaded_backdrop_with_no_ground_frame() {
    let _guard = nj_base::testlock::serial();
    crate::ui::tex::install(&HeroArtSpy);
    crate::ui::tex::reset_for_test(64 << 20);
    HERO_ART.with(|a| *a.borrow_mut() = HeroArtState { busy: true, ..Default::default() });

    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 4, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let n = view.hero_count();
    assert!(n >= 3, "the test needs two distinct neighbours, got {n} hero slots");
    let art = |i: usize| view.hero(i % n).unwrap().item.art.clone();
    let tick = hero_tick;

    let mut s = screen(view);
    assert_eq!(s.carousel_index(view), 0);
    let _ = step(&mut s, view, None, &tick(16));
    let probed = HERO_ART.with(|a| a.borrow().probed.clone());
    for (which, i) in [("next", 1), ("previous (wrapped)", n - 1)] {
        assert!(
            probed.contains(&art(i)),
            "showing hero 0 with shelf posters still loading must already request the {which} \
             backdrop {} as demand; draw probes were {probed:?}",
            art(i)
        );
    }

    // The workers finish what was asked for.
    for path in probed.iter().collect::<std::collections::BTreeSet<_>>() {
        deliver_hero_art(path);
    }
    // Two seconds on hero 0: its own backdrop fades fully in, well inside the 8 s countdown.
    for frame in 2..=125u32 {
        let _ = step(&mut s, view, None, &tick(frame * 16));
    }
    assert!(s.outgoing.is_none(), "no flip yet");
    assert!(reveal(s.backdrop.tex.0, &s.backdrop.art) > 0.999, "hero 0's backdrop is up");

    // A MANUAL flip: Right on the hero.
    let play = FocusKey { entry: s.entry, elem: HERO_ELEM };
    let right = hero_right();
    let (handled, _, _) = step(&mut s, view, Some(play), &right);
    assert_eq!(handled, Handled::Yes, "Right on the hero flips the billboard");
    HERO_ART.with(|a| a.borrow_mut().probed.clear());
    let _ = step(&mut s, view, Some(play), &tick(126 * 16));

    assert!(s.outgoing.is_some(), "the crossfade is running");
    assert_eq!(s.carousel_index(view), 1);
    let incoming_a = reveal(s.backdrop.tex.0, &s.backdrop.art);
    let outgoing_a = reveal(s.backdrop.outgoing_tex.0, &s.backdrop.outgoing_art);
    assert!(
        s.backdrop.tex.0 != 0 && incoming_a > 0.999,
        "the incoming backdrop must cross in fully revealed (tex {}, alpha {incoming_a}), not \
         fade in over the page after the crossfade",
        s.backdrop.tex.0
    );
    assert!(
        outgoing_a > 0.999,
        "no frame of the flip may show the bare page through the outgoing layer ({outgoing_a})"
    );

    // …and the new neighbours are asked for on the flip itself, not after the crossfade settles.
    let probed = HERO_ART.with(|a| a.borrow().probed.clone());
    assert!(
        probed.contains(&art(2)),
        "mid-slide, hero 1's next backdrop {} must already be requested; probes were {probed:?}",
        art(2)
    );
    assert!(probed.contains(&art(0)), "…and its previous one, the outgoing hero");
}

/// **Owner field report (2026-09-30, after 4fbeacfd6): the FIRST frame of a manual flip draws the
/// wrong backdrop.** Phone frames at 30 fps: pressing Right on Top Gear while the previous flip's
/// slide was still settling showed Top Gear's logo and text over FAMILY GUY's backdrop — the hero
/// from two flips ago — full-bright for one frame; pressing on a settled billboard showed one
/// frame of the outgoing hero's text over the bare wash.
///
/// The mechanism is frame ORDER, not texture residency. A manual flip is the hero's at-edge
/// Right/Left, which the dispatcher re-delivers to the page only after the focus engine reports
/// the edge — behind the frame's `Tick` in the queue (`ui::dispatch`, `Outcome::Edge`). So the
/// press frame ran `Backdrop::update` BEFORE `flip`: the art layers were resolved for the old
/// (outgoing, selected) pair, and the draw then read the NEW pair's slide offsets. The outgoing
/// slot at x = 0 drew whatever the previous tick had called outgoing — the hero two flips back
/// mid-slide, nothing at all on a settled billboard — and the real outgoing art was bound to the
/// incoming slot, a full screen off to the side. The auto-advance flips inside `tick`, before
/// `update`, which is why it never showed this.
///
/// The test drives the dispatcher's order: tick, THEN the at-edge press, then the draw, and reads
/// which textures the recording painter was handed.
#[test]
fn the_first_frame_of_a_manual_flip_draws_only_the_outgoing_and_incoming_backdrops() {
    let _guard = nj_base::testlock::serial();
    crate::ui::tex::install(&HeroArtSpy);
    crate::ui::tex::reset_for_test(64 << 20);
    HERO_ART.with(|a| *a.borrow_mut() = HeroArtState::default());

    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 4, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let n = view.hero_count();
    assert!(n >= 3, "the test needs three distinct heroes, got {n}");
    let hero = |i: usize| view.hero(i % n).unwrap();
    for i in 0..n {
        deliver_hero_art(&hero(i).item.art);
    }
    let tex_of = |i: usize| {
        let h = hero(i);
        crate::ui::widgets::resolve_tex_wh_on(h.item.sid.raw(), &h.item.art, 1280, 720, 0).0
    };
    let who = |tex: u32| (0..n).find(|&i| tex_of(i) == tex);
    let tick = hero_tick;
    let play = |s: &HomeScreen| FocusKey { entry: s.entry, elem: HERO_ELEM };
    let right = hero_right();
    // What the frame puts on screen: every backdrop-sized textured quad that reaches the panel.
    let backdrops = |s: &mut HomeScreen| {
        let context = cx(view, Some(play(s)));
        crate::ui::draw_census::capture_tex(|| {
            let mut f = DrawFrame::new(&context, Painter::recording());
            nj_gfx::gfx::without_frame_clear(|| Screen::<TestHost>::draw(s, &mut f));
        })
        .into_iter()
        .filter(|(_, r, a)| r.w >= crate::ui::landing_hero::ART.w && *a > 0.01 && r.x < SCR_W && r.x + r.w > 0.0)
        .collect::<Vec<_>>()
    };

    let mut s = screen(view);
    let mut ms = 0u32;
    let mut frames = |s: &mut HomeScreen, count: u32| {
        for _ in 0..count {
            ms += 16;
            let key = play(s);
            let _ = step(s, view, Some(key), &tick(ms));
        }
        ms
    };
    // Two seconds on hero 0: its backdrop is fully up.
    frames(&mut s, 125);
    assert!(s.outgoing.is_none());

    // Press 1 on a settled billboard, then press 2 while slide 1 is still settling (past the
    // flip cooldown, short of the rest threshold) — the owner's rhythm.
    for (press, settle) in [(1usize, 30u32), (2, 0)] {
        frames(&mut s, 1);
        let key = play(&s);
        let (handled, _, _) = step(&mut s, view, Some(key), &right);
        assert_eq!(handled, Handled::Yes, "press {press}: Right on the hero flips");
        assert_eq!(s.carousel_index(view), press % n);
        let drawn = backdrops(&mut s);
        let (outgoing, incoming) = (press - 1, press);
        for &(tex, r, a) in &drawn {
            assert!(
                who(tex) == Some(outgoing % n) || who(tex) == Some(incoming % n),
                "press {press}: the first flip frame drew hero {:?}'s backdrop (tex {tex} at x \
                 {}, alpha {a}); only the outgoing hero {outgoing} and the incoming hero \
                 {incoming} may be on screen",
                who(tex),
                r.x
            );
        }
        assert!(
            drawn.iter().any(|&(tex, _, a)| who(tex) == Some(outgoing % n) && a > 0.99),
            "press {press}: the outgoing hero {outgoing}'s backdrop must still be whole on the \
             flip's first frame, not the bare page; drew {drawn:?}"
        );
        frames(&mut s, settle);
        if settle > 0 {
            assert!(s.outgoing.is_some(), "press 2 must land while crossfade 1 is still running");
            assert_eq!(s.hero_flip_cd, 0.0, "…and past the flip cooldown");
        }
    }
}

/// The page-freeze holds Home's snapshot until the page is quiescent so first poster admission can
/// settle behind it. The backdrop's reveal and the focused hero's band are decorative — neither
/// moves layout or the grid — so they must be visible pixel motion (`page_moving`) yet not page
/// layout motion (`page_layout_moving`), or Home rides the whole 600 ms hold cap.
#[test]
fn decorative_reveal_and_focus_band_do_not_hold_page_quiescence() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_for_test(&mut state, &adapter, 3, crate::catalog_fetch::HubState::Ready);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    s.layout_grid();
    let key = FocusKey { entry: s.entry, elem: HERO_ELEM };
    let mut saw_decor = false;
    let mut layout_moving_frames = 0;
    for i in 1..40u32 {
        nj_machine::idle::frame_begin(1.0 / 60.0);
        step(&mut s, snapshot.view(), Some(key), &ScreenEvent::Tick(Tick { ms: i * 16, dt_us: 16_667 }));
        saw_decor |= nj_machine::idle::page_moving();
        layout_moving_frames += usize::from(nj_machine::idle::page_layout_moving());
    }
    assert!(saw_decor, "the focus band must still report visible page motion");
    assert_eq!(layout_moving_frames, 0, "decorative springs must not count as page layout motion");
}

#[test]
fn locate_agrees_with_a_row_scan() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_grid_for_test(&mut state, &adapter, 5, 7);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let s = screen(snapshot.view());
    let scan = |elem: u32| s.rows.iter().enumerate().find_map(|(row, hub)| {
        hub.elems.iter().position(|&e| e == elem).map(|col| Located::Item(row, col))
    });
    let mut seen = 0;
    for hub in &s.rows {
        for &elem in &hub.elems {
            assert_eq!(s.locate(elem), scan(elem));
            seen += 1;
        }
    }
    assert_eq!(seen, 35);
    // A key the table holds but no row lists (a card that left the catalog), and one it never held.
    for stale in [s.next_elem, s.next_elem + 1] {
        assert_eq!(s.locate(stale), scan(stale));
    }
}

/// Publish a `rows` x `items` grid into `s`, as one more provider publication.
fn republish(s: &mut HomeScreen, state: &mut crate::catalog_fetch::PmsState, adapter: &std::sync::Arc<crate::catalog_fetch::PmsAdapter>, rows: usize, items: usize) {
    crate::catalog_fetch::seed_grid_for_test(state, adapter, rows, items);
    let snapshot = crate::catalog_fetch::hubs_snapshot(state);
    s.sync_catalog(&cx(snapshot.view(), None));
}

#[test]
fn item_keys_are_stable_across_republication() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let mut s = HomeScreen::new(EntryId(7), InstanceId(9));
    republish(&mut s, &mut state, &adapter, 4, 5);
    let first: Vec<Vec<u32>> = s.rows.iter().map(|r| r.elems.clone()).collect();
    let table = s.items.clone();
    // The same catalog again, and a wider one: no card already keyed changes its element, and the
    // new cards take the next elements in publication order.
    republish(&mut s, &mut state, &adapter, 4, 5);
    assert_eq!(s.rows.iter().map(|r| r.elems.clone()).collect::<Vec<_>>(), first);
    assert_eq!(s.items, table);
    let next = s.next_elem;
    republish(&mut s, &mut state, &adapter, 6, 5);
    for (row, elems) in first.iter().enumerate() {
        assert_eq!(&s.rows[row].elems, elems);
    }
    assert_eq!(s.items[..table.len()].iter().map(|k| k.elem).collect::<Vec<_>>(),
        table.iter().map(|k| k.elem).collect::<Vec<_>>());
    let fresh: Vec<u32> = s.rows[4..].iter().flat_map(|r| r.elems.iter().copied()).collect();
    assert_eq!(fresh, (next..next + 10).collect::<Vec<_>>());
}

#[test]
fn restore_merges_saved_keys_without_duplicates() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let mut a = HomeScreen::new(EntryId(7), InstanceId(9));
    republish(&mut a, &mut state, &adapter, 3, 4);
    let memory = match <HomeScreen as Screen<TestHost>>::memory(&a) {
        PageMemory::Home(m) => m,
        _ => unreachable!(),
    };
    // Restoring into an empty screen reproduces the table; restoring again, or into the screen
    // that wrote it, adds nothing.
    let mut b = HomeScreen::new(EntryId(7), InstanceId(10));
    b.restore(&memory);
    assert_eq!(b.items, a.items);
    assert_eq!(b.groups, a.groups);
    b.restore(&memory);
    assert_eq!((b.items.len(), b.groups.len()), (a.items.len(), a.groups.len()));
    let before = a.items.clone();
    a.restore(&memory);
    assert_eq!(a.items, before);
    // A screen that restored first republishes onto the saved elements.
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    b.sync_catalog(&cx(snapshot.view(), None));
    assert_eq!(b.rows.iter().map(|r| r.elems.clone()).collect::<Vec<_>>(),
        a.rows.iter().map(|r| r.elems.clone()).collect::<Vec<_>>());
    assert_eq!(b.items.len(), a.items.len());
}

#[test]
fn key_index_agrees_with_a_scan_of_the_table() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let mut s = HomeScreen::new(EntryId(7), InstanceId(9));
    for (rows, items) in [(3, 4), (5, 4), (2, 6)] {
        republish(&mut s, &mut state, &adapter, rows, items);
    }
    let memory = match <HomeScreen as Screen<TestHost>>::memory(&s) {
        PageMemory::Home(m) => m,
        _ => unreachable!(),
    };
    let mut r = HomeScreen::new(EntryId(7), InstanceId(10));
    r.restore(&memory);
    republish(&mut r, &mut state, &adapter, 4, 4);
    for screen in [&s, &r] {
        for key in screen.items.iter() {
            assert_eq!(screen.find_item(&key.identity), screen.items.iter().position(|k| k.identity == key.identity));
        }
        for key in &screen.groups {
            assert_eq!(screen.find_group(&key.identity), screen.groups.iter().position(|k| k.identity == key.identity));
        }
        assert_eq!(screen.item_index.values().map(HubKeys::len).sum::<usize>(), screen.items.len());
        assert_eq!(screen.group_index.len(), screen.groups.len());
    }
}

#[test]
fn stops_are_recorded_only_for_rows_on_screen_or_focused() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_grid_for_test(&mut state, &adapter, 16, 3);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let mut s = screen(snapshot.view());
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    s.layout_grid();
    let on_screen = |s: &HomeScreen, row: usize| on_axis(s.grid.shelves[row].base_y, s.pitch(row), SCR_H, 0.0);
    let visible: Vec<usize> = (0..s.rows.len()).filter(|&r| on_screen(&s, r)).collect();
    assert!(!visible.is_empty() && visible.len() < s.rows.len(), "the fixture must straddle the screen edge");
    let far = s.rows.len() - 1;
    assert!(!visible.contains(&far));
    let elems = |s: &HomeScreen, rows: &[usize]| {
        let mut e: Vec<u32> = rows.iter().flat_map(|&r| s.rows[r].elems.clone()).collect();
        e.sort_unstable();
        e
    };
    for (focus_row, expected_rows) in [
        (None, visible.clone()),
        (Some(far), { let mut v = visible.clone(); v.push(far); v }),
    ] {
        let focus = focus_row.map(|r| FocusKey { entry: s.entry, elem: s.rows[r].elems[0] });
        let context = cx(snapshot.view(), focus);
        let mut frame = DrawFrame::new(&context, Painter::root());
        s.record_stops(&mut frame, snapshot.view());
        let mut recorded: Vec<u32> = frame.into_stops().iter()
            .map(|stop| stop.key.elem)
            .filter(|e| e & HEADING_BASE == 0)
            .collect();
        recorded.sort_unstable();
        assert_eq!(recorded, elems(&s, &expected_rows));
    }
}

/// Publish `hubs` named hubs of `items` cards into `s`; an empty id (and no key) makes the hub
/// `Ephemeral`.
fn republish_named(
    s: &mut HomeScreen,
    state: &mut crate::catalog_fetch::PmsState,
    adapter: &std::sync::Arc<crate::catalog_fetch::PmsAdapter>,
    hubs: &[&str],
    items: usize,
    focus: Option<FocusKey<u32>>,
) {
    let rows: Vec<(&str, &str, &str)> = hubs.iter().map(|id| (*id, "", "Row")).collect();
    crate::catalog_fetch::seed_named_hubs_for_test(state, adapter, items, &rows);
    let snapshot = crate::catalog_fetch::hubs_snapshot(state);
    s.sync_catalog(&cx(snapshot.view(), focus));
}

fn assert_indexes_agree(s: &HomeScreen) {
    for key in s.items.iter() {
        assert_eq!(s.find_item(&key.identity), s.items.iter().position(|k| k.identity == key.identity));
    }
    for key in &s.groups {
        assert_eq!(s.find_group(&key.identity), s.groups.iter().position(|k| k.identity == key.identity));
    }
    assert_eq!(s.item_index.values().map(HubKeys::len).sum::<usize>(), s.items.len());
    assert_eq!(s.group_index.len(), s.groups.len());
}

#[test]
fn volatile_keys_do_not_accumulate_across_publications() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let mut s = HomeScreen::new(EntryId(7), InstanceId(9));
    for _ in 0..20 {
        republish_named(&mut s, &mut state, &adapter, &["", "", ""], 4, None);
    }
    // Only the shown publication and the one before it are kept.
    assert!(s.items.len() <= 2 * 12, "{} keys after 20 publications", s.items.len());
    assert!(s.groups.len() <= 3, "{} groups after 20 publications", s.groups.len());
    assert_eq!(s.rows.len(), 3);
    assert_indexes_agree(&s);
}

#[test]
fn the_focused_vanished_key_survives_a_prune() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let mut s = HomeScreen::new(EntryId(7), InstanceId(9));
    republish_named(&mut s, &mut state, &adapter, &[""], 4, None);
    let (focused, other) = (s.rows[0].elems[1], s.rows[0].elems[2]);
    let focus = Some(FocusKey { entry: EntryId(7), elem: focused });
    for _ in 0..4 {
        republish_named(&mut s, &mut state, &adapter, &[""], 4, focus);
    }
    assert!(s.items.iter().any(|k| k.elem == focused), "the focused key was pruned");
    assert!(!s.items.iter().any(|k| k.elem == other), "an unfocused vanished key was kept");
    assert_indexes_agree(&s);
}

#[test]
fn focus_recovers_to_the_same_row_after_its_card_rotates_out() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let mut s = HomeScreen::new(EntryId(7), InstanceId(9));
    republish(&mut s, &mut state, &adapter, 4, 5);
    let focused = s.rows[2].elems[3];
    let focus = FocusKey { entry: EntryId(7), elem: focused };
    // The card (rk "4" in every row) leaves the catalog, and a few publications pass.
    for _ in 0..4 {
        crate::catalog_fetch::seed_grid_for_test(&mut state, &adapter, 4, 5);
        crate::catalog_fetch::remove_test_item(&mut state, "4");
        let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
        s.sync_catalog(&cx(snapshot.view(), Some(focus)));
    }
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    assert_eq!(s.locate(focused), None, "the card is gone");
    let got = Focusable::<TestHost>::reconcile(&s, focus, &cx(snapshot.view(), Some(focus)));
    assert!(s.rows[2].elems.contains(&got.elem), "recovered outside the row");
}

#[test]
fn memory_shares_the_key_table() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let mut s = HomeScreen::new(EntryId(7), InstanceId(9));
    republish(&mut s, &mut state, &adapter, 4, 5);
    let memory = match <HomeScreen as Screen<TestHost>>::memory(&s) {
        PageMemory::Home(m) => m,
        _ => unreachable!(),
    };
    assert!(Arc::ptr_eq(&memory.items, &s.items));
    // The same catalog republished moves no card and mints no key: still one table.
    republish(&mut s, &mut state, &adapter, 4, 5);
    assert!(Arc::ptr_eq(&memory.items, &s.items), "an unchanged republication copied the table");
    // A new row writes the table, which must not reach what memory took.
    let before = memory.items.len();
    republish(&mut s, &mut state, &adapter, 5, 5);
    assert!(!Arc::ptr_eq(&memory.items, &s.items));
    assert_eq!(memory.items.len(), before);
    assert_eq!(s.items.len(), before + 5);
}

/// A Home whose hubs carry identifiers and whose cards carry rating keys never loses a key while
/// it stays under the cap: the table is exactly what it was before pruning existed (replay
/// fixtures hash it).
#[test]
fn a_stable_home_under_the_cap_prunes_nothing() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let mut s = HomeScreen::new(EntryId(7), InstanceId(9));
    let mut table: Vec<HomeItemKey> = Vec::new();
    for (rows, items) in [(3, 4), (5, 4), (2, 6), (5, 4), (1, 1), (4, 5), (4, 5)] {
        republish(&mut s, &mut state, &adapter, rows, items);
        // Every earlier key is still there, in the same place, with the same elem; only
        // `last_row`/`last_col` follow the cards.
        assert!(s.items.len() >= table.len());
        for (kept, old) in s.items.iter().zip(&table) {
            assert_eq!((&kept.identity, kept.elem), (&old.identity, old.elem));
        }
        table = s.items.to_vec();
        assert_indexes_agree(&s);
    }
}

#[test]
fn stable_keys_are_bounded_oldest_first() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let mut s = HomeScreen::new(EntryId(7), InstanceId(9));
    let ids: Vec<String> = (0..100).map(|i| format!("hub.{i}")).collect();
    for id in &ids {
        republish_named(&mut s, &mut state, &adapter, &[id.as_str()], 4, None);
    }
    let cap = 2 * 4 + 256;
    assert!(s.items.len() <= cap, "{} keys", s.items.len());
    assert!(s.items.len() > 4 * 2, "the window is not just the last two publications");
    // The newest survive and the table is still in elem order.
    let elems: Vec<u32> = s.items.iter().map(|k| k.elem).collect();
    assert!(elems.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(elems.last().copied(), s.rows[0].elems.last().copied());
    assert_indexes_agree(&s);
}

#[test]
fn incremental_republication_keeps_unchanged_home_row_storage() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let mut page = HomeScreen::new(EntryId(7), InstanceId(9));
    republish(&mut page, &mut state, &adapter, 80, 12);
    let before: Vec<_> = page.rows.iter().map(|r| (r.elems.as_ptr(), r.elems.clone())).collect();
    republish(&mut page, &mut state, &adapter, 80, 12);
    for (row, (ptr, elems)) in before.iter().enumerate() {
        assert_eq!(&page.rows[row].elems, elems);
        assert_eq!(page.rows[row].elems.as_ptr(), *ptr,
            "an unchanged publication must retain row {row}'s projection");
    }
    assert_indexes_agree(&page);
}

#[test]
fn incremental_republication_changes_only_one_home_row_and_keeps_old_memory() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let mut page = HomeScreen::new(EntryId(7), InstanceId(9));
    republish(&mut page, &mut state, &adapter, 80, 12);
    let before: Vec<_> = page.rows.iter().map(|r| r.elems.as_ptr()).collect();
    let PageMemory::Home(saved) = <HomeScreen as Screen<TestHost>>::memory(&page) else { panic!() };
    let old_hash = PageMemory::Home(saved.clone()).hash();
    let focus = FocusKey { entry: page.entry, elem: page.rows[7].elems[5] };
    crate::catalog_fetch::edit_test_hub_card(&mut state, 7, 5, "replacement", "changed");
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    page.sync_catalog(&cx(snapshot.view(), Some(focus)));
    for (row, ptr) in before.iter().enumerate() {
        if row != 7 { assert_eq!(page.rows[row].elems.as_ptr(), *ptr); }
    }
    assert_eq!(page.locate(focus.elem), None);
    let recovered = Focusable::<TestHost>::reconcile(&page, focus, &cx(snapshot.view(), Some(focus)));
    assert!(page.rows[7].elems.contains(&recovered.elem));
    assert_indexes_agree(&page);
    assert_eq!(PageMemory::Home(saved.clone()).hash(), old_hash);
    // A metadata-only update is read from the new view without rebuilding a key projection.
    let ptr = page.rows[7].elems.as_ptr();
    crate::catalog_fetch::edit_test_hub_card(&mut state, 7, 5, "replacement", "new metadata");
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    page.sync_catalog(&cx(snapshot.view(), Some(recovered)));
    assert_eq!(page.rows[7].elems.as_ptr(), ptr);
    assert_eq!(snapshot.view().hub(7).unwrap().items[5].title, "new metadata");
    // A restore must reapply remembered positions even when every published identity matches.
    page.restore(&saved);
    page.sync_catalog(&cx(snapshot.view(), None));
    assert_indexes_agree(&page);
    for (row, hub) in page.rows.iter().enumerate() {
        for (col, &elem) in hub.elems.iter().enumerate() {
            assert_eq!(page.locate(elem), Some(Located::Item(row, col)));
            let key = page.items.iter().find(|key| key.elem == elem).unwrap();
            assert_eq!((key.last_row, key.last_col), (row as u32, col as u32));
        }
    }
}


#[test]
fn incremental_republication_duplicate_home_rows_keep_last_key_places_and_first_draw() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = Arc::new(crate::catalog_fetch::PmsAdapter::default());
    let mut page = HomeScreen::new(EntryId(7), InstanceId(9));
    republish_named(&mut page, &mut state, &adapter, &["same", "same"], 4, None);
    crate::catalog_fetch::edit_test_hub_card(&mut state, 0, 0, "replacement", "changed");
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    page.sync_catalog(&cx(snapshot.view(), None));
    for &elem in &page.rows[1].elems {
        let key = page.items.iter().find(|key| key.elem == elem).unwrap();
        assert_eq!(key.last_row, 1, "registration retains the existing last-row recovery rule");
    }
    let elem = page.rows[0].elems[1];
    assert_eq!(page.locate(elem), Some(Located::Item(0, 1)), "the first drawn row wins");
    assert_indexes_agree(&page);
}

/// Grade the production Jellyfin DTO -> catalog row -> landscape card path: the show over the
/// episode's name, its code on the badge, and the resume edges that decide the bar.
#[test]
fn continue_watching_uses_jellyfin_episode_numbers_and_resume_metadata() {
    let _serial = nj_base::testlock::serial();
    let _lang = nj_platform::i18n::language_on_this_thread_for_test(nj_platform::i18n::Preference::En);
    let episode = |offset, index: Option<i64>, season: Option<i64>| {
        let dto: crate::jf::models::BaseItemDto = serde_json::from_value(serde_json::json!({
            "Id":"00000000002010000000000000000000", "Type":"Episode", "Name":"Pilot",
            "SeriesName":"Laura",
            "SeriesId":"00000000002000000000000000000000",
            "IndexNumber":index, "ParentIndexNumber":season, "RunTimeTicks":27_000_000_000_i64,
            "UserData":{"PlaybackPositionTicks":offset}
        })).unwrap();
        crate::catalog_fetch::parse_item(&crate::jf::convert::item(&dto, 0), crate::catalog::ServerId::default())
    };
    let m = episode(12_000_000_000_i64, Some(5), Some(2));
    assert_eq!((m.ep_index, m.season_index, m.resume_ms), (5, 2, 1_200_000));
    assert!(m.resume_frac().is_some());
    let label = card_row::tile_label(&tile_facts::of(&m), true);
    let text = |c: &Option<std::ffi::CString>| c.as_ref().map(|c| c.to_str().unwrap().to_string());
    assert_eq!((text(&label.title), text(&label.caption)), (Some("Laura".into()), Some("Pilot".into())));
    assert_eq!(crate::ui::widgets::episode_code(&tile_facts::of(&m)).as_deref(), Some("S02E05"));
    assert_eq!(crate::ui::widgets::episode_code(&tile_facts::of(&episode(0, None, Some(2)))), None);
    for offset in [27_000_000_000_i64, 36_000_000_000_i64] {
        assert!(episode(offset, Some(5), Some(2)).resume_frac().is_none(), "no bar once finished");
    }
}

/// **The pointer moves Home by its edges, never by a hover.** A wheel (the edge glide, fractional,
/// every frame) slides the page out of the hero into the shelves and back, a hover moves focus and
/// nothing else, a click brings the page to its card, and a key rests its row.
#[test]
fn the_edge_glide_moves_home_a_hover_does_not_and_a_click_rests_its_card() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_grid_for_test(&mut state, &adapter, 8, 20);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let mut s = screen(view);
    let hero = FocusKey { entry: s.entry, elem: HERO_ELEM };
    let card = |s: &HomeScreen, row: usize| FocusKey { entry: s.entry, elem: s.rows[row].elems[0] };
    let tick = |s: &mut HomeScreen, focus: FocusKey<u32>| {
        for _ in 0..3 { step(s, view, Some(focus), &ScreenEvent::Tick(Tick { ms: 16, dt_us: 16_667 })); }
    };
    let input = |kind| ScreenEvent::Input(InputEvent { at: Tick::default(), source: Source::Script, kind });
    step(&mut s, view, Some(hero), &input(InputKind::Wheel { dy: -0.05 }));
    assert_eq!(s.snap_target, 1.0, "down from the hero goes to the shelves");
    for _ in 0..60 { step(&mut s, view, Some(hero), &input(InputKind::Wheel { dy: -0.05 })); }
    tick(&mut s, hero);
    let glided = s.grid.scroll_target;
    assert!(glided > 100.0, "the shelves glide by the wheel: {glided}");
    let near = card(&s, 6);
    step(&mut s, view, Some(near), &ScreenEvent::FocusMoved { from: Some(hero), to: near, by: By::Pointer });
    tick(&mut s, near);
    assert_eq!(s.grid.scroll_target, glided, "a hover moves focus, not the page");
    step(&mut s, view, Some(near), &input(InputKind::Click { x: 0.0, y: 0.0, hit: Some(near.elem) }));
    tick(&mut s, near);
    assert_ne!(s.grid.scroll_target, glided, "a click brings the page to its card");
    for _ in 0..800 { step(&mut s, view, Some(near), &input(InputKind::Wheel { dy: 0.05 })); }
    assert_eq!((s.grid.scroll_target, s.snap_target), (0.0, 0.0), "up through the shelves to the hero");
    let deep = card(&s, 7);
    step(&mut s, view, Some(deep), &ScreenEvent::FocusMoved { from: Some(near), to: deep, by: By::Dir });
    tick(&mut s, deep);
    assert!(s.grid.scroll_target > 0.0 && s.snap_target == 1.0, "a key rests its row again");
}

/// **The pointer's sideways glide moves the shelf under it, and the shelf stays there**: neither
/// its focus nor a hover pulls it back, and a click on one of its cards brings it back to that card.
#[test]
fn the_sideways_glide_moves_the_shelf_under_the_pointer_and_a_click_brings_it_back() {
    let _guard = nj_base::testlock::serial();
    let mut state = crate::catalog_fetch::PmsState::default();
    let adapter = std::sync::Arc::new(crate::catalog_fetch::PmsAdapter::default());
    crate::catalog_fetch::seed_grid_for_test(&mut state, &adapter, 3, 20);
    let snapshot = crate::catalog_fetch::hubs_snapshot(&state);
    let view = snapshot.view();
    let mut s = screen(view);
    s.snap.jump(1.0);
    s.snap_target = 1.0;
    s.layout_grid();
    let card = |s: &HomeScreen, row: usize, col: usize| FocusKey { entry: s.entry, elem: s.rows[row].elems[col] };
    let first = card(&s, 0, 0);
    let tick = |s: &mut HomeScreen, focus| for i in 1..=60 { step(s, view, Some(focus), &ScreenEvent::Tick(Tick { ms: i * 16, dt_us: 16_667 })); };
    tick(&mut s, first);
    let y = s.grid.shelf(0).base_y + CARD_H / 2.0;
    let pan = ScreenEvent::Input(InputEvent { at: Tick::default(), source: Source::Script,
        kind: InputKind::Pan { dx: 25.0, x: SCR_W - 10.0, y } });
    for _ in 0..20 { step(&mut s, view, Some(first), &pan); }
    assert_eq!(s.grid.shelves[0].scroll_x(), 500.0, "the shelf under the pointer glides");
    assert_eq!(s.grid.shelves[1].scroll_x(), 0.0, "the shelf below stays");
    tick(&mut s, first);
    assert_eq!(s.grid.shelves[0].scroll_x(), 500.0, "its focus does not pull it back");
    let hovered = card(&s, 0, 4);
    step(&mut s, view, Some(hovered), &ScreenEvent::FocusMoved { from: Some(first), to: hovered, by: By::Pointer });
    tick(&mut s, hovered);
    assert_eq!(s.grid.shelves[0].scroll_x(), 500.0, "nor does a hover");
    step(&mut s, view, Some(first), &ScreenEvent::FocusMoved { from: Some(hovered), to: first, by: By::Pointer });
    step(&mut s, view, Some(first), &ScreenEvent::Input(InputEvent { at: Tick::default(), source: Source::Script,
        kind: InputKind::Click { x: 0.0, y, hit: Some(first.elem) } }));
    tick(&mut s, first);
    tick(&mut s, first);
    assert!(s.grid.shelves[0].scroll_x() < 1.0, "a click on its first card brings the shelf back: {}", s.grid.shelves[0].scroll_x());
}
