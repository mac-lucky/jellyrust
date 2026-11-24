use super::*;
use crate::ui::fixture::FixtureMeasure;
use crate::ui::focus::{FocusEngine, Outcome};
use nj_machine::machine::{Host, InputOwner, FocusRead, PressRead, Tick};
use crate::ui::screen::ScreenArg;

#[derive(Clone)]
pub(super) struct Arg;
impl LogicalState for Arg {
    fn write(&self, _: &mut Canon) {}
    fn probe(&self, _: &mut String) {}
}
impl ScreenArg for Arg {
    fn chrome(&self) -> nj_machine::machine::Chrome { nj_machine::machine::Chrome::None }
    fn id(&self) -> nj_machine::machine::ScreenId { nj_machine::machine::ScreenId(1) }
    fn title(&self) -> Option<&str> { None }
    fn same_instance(&self, _: &Self) -> bool { true }
}
pub(super) struct HostFixture;
#[derive(Clone, Copy)]
pub(super) struct Views<'a> {
    listing: crate::stores::browse::ListingView<'a>,
    directory: crate::stores::browse::DirectoryView<'a>,
    hubs: crate::stores::browse::HubsView<'a>,
}
impl Host for HostFixture {
    type Arg = Arg;
    type Fx = AppFx;
    type Msg = AppMsg;
    type Elem = u32;
    type Views<'a> = Views<'a>;
    type Init = Arg;
    type Memory = PageMemory;
}
impl LibraryLike for HostFixture {
    fn listing<'a>(cx: &Cx<'a, Self>) -> crate::stores::browse::ListingView<'a> { cx.views.listing }
    fn directory<'a>(cx: &Cx<'a, Self>) -> crate::stores::browse::DirectoryView<'a> { cx.views.directory }
    fn section_hubs<'a>(cx: &Cx<'a, Self>) -> crate::stores::browse::HubsView<'a> { cx.views.hubs }
}
const ENTRY: EntryId = EntryId(81);
const OWNER: InputOwner = InputOwner::Entry(ENTRY);

/// Return memory is captured several times per frame. A 1,200-item catalog must share its
/// unchanged keys across those snapshots, while later reconciliation cannot mutate a saved
/// return position or its canonical state.
#[test]
fn page_memory_shares_1200_keys_and_preserves_older_snapshots() {
    let _guard = nj_base::testlock::serial();
    let sid = crate::catalog::ServerId::from_raw(1);
    let section = LibrarySectionIdentity { sid, key: 7 };
    let mut page = LibraryScreen::new(ENTRY, InstanceId(20), None);
    for index in 0..1200 {
        page.keys.register(LibraryIdentity::Grid {
            section: section.clone(), sid, rk: format!("fixture-{index}"),
        }, GRID_GROUP, index);
    }
    let original = page.page_memory();
    let original_hash = PageMemory::Library(original.clone()).hash();
    assert_eq!(original.keys.len(), 1200);
    for frame in 0..60 {
        page.scroll.jump(frame as f32);
        let next = page.page_memory();
        assert_eq!(original.keys.as_ptr(), next.keys.as_ptr(),
            "scroll frame {frame} must not copy the unchanged catalog into return memory");
        let forwarded = next.clone();
        assert_eq!(next.keys.as_ptr(), forwarded.keys.as_ptr(),
            "forwarding return memory must not copy the catalog either");
    }

    let key = &original.keys[17];
    let elem = key.elem;
    let identity = key.identity.clone();
    assert_eq!(page.keys.register(identity.clone(), GRID_GROUP, 17), elem);
    page.keys.update_last_place(elem, GRID_GROUP, 17);
    assert_eq!(page.page_memory().keys.as_ptr(), original.keys.as_ptr(),
        "unchanged reconciliation must not detach the shared snapshot");

    assert_eq!(page.keys.register(identity, GroupId(90), 23), elem);
    let moved = page.page_memory();
    assert_ne!(moved.keys.as_ptr(), original.keys.as_ptr());
    assert_eq!(moved.keys[17].last_index, 23);
    assert_eq!(original.keys[17].last_index, 17);
    page.keys.update_last_place(elem, GroupId(91), 31);
    let moved_again = page.page_memory();
    assert_eq!(moved_again.keys[17].last_index, 31);
    assert_eq!(moved.keys[17].last_index, 23, "a previous reconciliation stays immutable");

    let added = page.keys.register(LibraryIdentity::Grid {
        section: section.clone(), sid, rk: "fixture-new".into(),
    }, GRID_GROUP, 1200);
    let grown = page.page_memory();
    assert_eq!(grown.keys.len(), 1201);
    assert_eq!(moved_again.keys.len(), 1200, "appending must not grow an older snapshot");
    assert_ne!(added, elem);

    let mut restored = KeyRegistry::restore(&original);
    let restored_memory = restored.remember(original.section.clone(), original.scroll, Vec::new());
    assert_eq!(restored_memory.keys.as_ptr(), original.keys.as_ptr(),
        "remounting reuses the immutable key snapshot");
    assert_eq!(restored.last_place(elem), Some((GRID_GROUP, 17)));
    restored.update_last_place(elem, GroupId(92), 41);
    assert_eq!(restored.last_place(elem), Some((GroupId(92), 41)));
    assert_eq!(original.keys[17].last_index, 17);
    assert_eq!(PageMemory::Library(original.clone()).hash(), original_hash,
        "neither live reconciliation nor remount mutation may change saved canonical memory");
}

#[test]
fn all_grid_caption_band_restores_with_the_saved_viewport() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.screen();
    let key = page.key(page.pair.detail.elem_at(35).unwrap());
    page.relayout(Some(key));
    page.scroll_target = page.target_layout.row_reveal(5);
    page.scroll.jump(page.scroll_target);
    page.relayout(Some(key));
    let before = page.place(&key.elem, &fixture.cx(Some(key)), At::Drawn).unwrap();
    let PageMemory::Library(memory) = <LibraryScreen as Screen<HostFixture>>::memory(&page) else { panic!() };
    let mut restored = LibraryScreen::new(ENTRY, InstanceId(20), None);
    restored.restore(&memory);
    restored.sync(&fixture.cx(Some(key)));
    let after = restored.place(&key.elem, &fixture.cx(Some(key)), At::Drawn).unwrap();
    assert_eq!(restored.layout.row_expansion(5), 1.0);
    assert_eq!(restored.layout.row_expansion(4), 0.0);
    assert_eq!(restored.scroll.pos, page.scroll.pos);
    assert_eq!([after.rect.x, after.rect.y, after.rect.w, after.rect.h],
        [before.rect.x, before.rect.y, before.rect.w, before.rect.h]);
    assert_eq!(restored.layout.doc_h(), page.layout.doc_h());
}

#[test]
fn saved_last_all_row_opens_before_the_bookmark_scroll_is_clamped() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    let saved_scroll = fixture.screen().layout.with_grid_focus(Some(5)).max_scroll();
    fixture.listing = fixture.listing.clone().with_cursor(crate::stores::browse::Cursor {
        at: crate::stores::browse::CursorAt::SlotIndex(35), scroll: saved_scroll,
    });
    let mut page = fixture.screen();
    let mut output = Vec::new();
    let mut present = nj_machine::present::Present::new();
    assert!(page.seed_cursor(&fixture.cx(None),
        &mut Effects::new(&mut output, MachineId::Instance(InstanceId(19)), &mut present)));
    assert_eq!(page.scroll.pos, saved_scroll);
    assert_eq!(page.target_layout.row_expansion(5), 1.0);
    assert_eq!(page.layout.row_expansion(5), 1.0);
    assert_eq!(page.restore_scroll, Some(saved_scroll));
}

#[test]
fn all_grid_moves_open_only_the_destination_band_and_use_settled_reveal() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.screen();
    page.initial = false;
    let mut engine = FocusEngine::new();
    let first = page.key(page.pair.detail.elem_at(2 * COLS).unwrap());
    page.relayout(Some(first));
    engine.set(OWNER, first, Some(page.pair.groups_config().detail), By::Restore);
    direction(&mut page, &mut engine, &fixture, Dir::Down);
    let next = engine.current(OWNER).unwrap();
    assert_eq!(page.grid_position(Some(next)), Some((3, 0)));
    assert_eq!(page.layout.row_expansion(2), 1.0, "outgoing row starts closing from its live size");
    assert_eq!(page.layout.row_expansion(3), 0.0, "incoming row starts compact");
    assert_eq!(page.target_layout.row_expansion(2), 0.0);
    assert_eq!(page.target_layout.row_expansion(3), 1.0);
    assert_eq!(page.scroll_target, page.target_layout.row_reveal(3));
    let target = page.place(&next.elem, &fixture.cx(Some(next)), At::SpringTarget).unwrap();
    assert_eq!(target.rest_rect.cy(), page.target_layout.row_y(3, page.scroll_target)
        + page.target_layout.card_h() * 0.5);
    let (lo, hi) = page.target_layout.visible_rows(page.scroll_target);
    assert!((lo..hi).contains(&3));
}

#[test]
fn grid_paint_window_keeps_cards_poking_in_at_the_top_edge() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.screen();
    let layout = page.layout;
    let index = 2 * COLS;
    let y = 8.0 - layout.card_h();
    let scroll = layout.row_y(2, 0.0) - y;
    page.pair.detail.set_geometry(layout, scroll, layout, scroll);
    let rect = page.pair.detail.rect_at(index, false, 1.0);
    assert!(rect.y < 0.0 && rect.y + rect.h > 0.0, "the card straddles the top edge: {rect:?}");
    let (lo, hi) = page.pair.detail.visible_window();
    assert!((lo..hi).contains(&index), "the real paint iterator must include this visible card");

    // The paint iterator conservatively retains overscan rows, but never the entire catalog.
    // Far outside that window, the same card must be excluded on either side of the screen.
    for y in [-2.0 * SCR_H, 2.0 * SCR_H] {
        let scroll = layout.row_y(2, 0.0) - y;
        page.pair.detail.set_geometry(layout, scroll, layout, scroll);
        let (lo, hi) = page.pair.detail.visible_window();
        assert!(!(lo..hi).contains(&index), "far-offscreen card at {y} must not enter paint iteration");
    }
}

#[test]
fn duplicate_across_pages_keeps_full_projection_recovery_metadata() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    let sid = crate::catalog::ServerId::from_raw(0);
    let movie = |i| crate::catalog_fetch::PmsMovie { sid, rk: format!("duplicate-test-{i}"), ..Default::default() };
    let mut items = (0..120).map(movie).collect::<Vec<_>>();
    items[85] = items[5].clone();
    fixture.listing = crate::browse::view::ListingSnapshot::fixture(sid,
        items.iter().cloned().map(Some).collect(), Vec::new()).with_total(10_000);
    let mut partial = fixture.screen();
    let mut full = fixture.screen();
    let duplicate = partial.pair.detail.elem_at(5).unwrap();
    let group = partial.pair.groups_config().detail;
    let mut observations = Vec::new();
    let grid_hash = |page: &LibraryScreen| {
        let mut canon = Canon::new();
        page.pair.detail.write(&mut canon);
        canon.finish()
    };
    let mut canonical = Vec::new();
    // First change only unrelated data on the lower page; then remove both copies together.
    // A second pass removes only the higher copy, leaving the lower unchanged.
    for remove_high_only in [false, true] {
        fixture.listing = fixture.listing.clone().with_page(0, items[..60].to_vec())
            .with_page(60, items[60..].to_vec());
        partial.sync(&fixture.cx(None));
        full.pair.detail.clear_projection();
        full.sync(&fixture.cx(None));
        fixture.listing = fixture.listing.clone().with_page(0, vec![movie(1000)]);
        partial.pair.detail.reset_publication_ops();
        partial.sync(&fixture.cx(None));
        full.pair.detail.clear_projection();
        full.sync(&fixture.cx(None));
        assert_eq!(partial.pair.detail.publication_ops().0, 60);
        assert!(partial.pair.detail.publication_ops().1 <= 120);
        canonical.push((grid_hash(&partial), grid_hash(&full)));
        observations.push((partial.keys.last_place(duplicate), full.keys.last_place(duplicate)));
        assert_eq!(partial.pair.detail.index_of(duplicate), Some(5));
        if remove_high_only {
            fixture.listing = fixture.listing.clone().with_page(85, vec![movie(85)]);
        } else {
            fixture.listing = fixture.listing.clone().with_page(5, vec![movie(1005)])
                .with_page(85, vec![movie(85)]);
        }
        partial.sync(&fixture.cx(None));
        full.pair.detail.clear_projection();
        full.sync(&fixture.cx(None));
        canonical.push((grid_hash(&partial), grid_hash(&full)));
        observations.push((partial.keys.last_place(duplicate), full.keys.last_place(duplicate)));
        observations.push((partial.pair.detail.fallback_for(duplicate).map(|e| (group, e as usize)),
            full.pair.detail.fallback_for(duplicate).map(|e| (group, e as usize))));
        if remove_high_only {
            fixture.listing = fixture.listing.clone().with_page(5, vec![movie(1005)]);
            partial.sync(&fixture.cx(None));
            full.pair.detail.clear_projection();
            full.sync(&fixture.cx(None));
            canonical.push((grid_hash(&partial), grid_hash(&full)));
            assert_eq!(partial.keys.last_place(duplicate), Some((group, 5)));
            assert_eq!(partial.pair.detail.fallback_for(duplicate), partial.pair.detail.elem_at(5));
        }
    }
    eprintln!("duplicate recovery partial/full: {observations:?}");
    assert!(observations.iter().all(|(partial, full)| partial == full),
        "partial publication must preserve full projection's last occurrence and tombstone fallback: {observations:?}");
    assert!(canonical.iter().all(|(partial, full)| partial == full),
        "ordered projection and known metadata must equal full projection: {canonical:?}");
}

#[test]
fn large_listing_publication_work_is_bounded_by_initial_slots_then_changed_page() {
    let _guard = nj_base::testlock::serial();
    const TOTAL: usize = 10_000;
    const PAGE: usize = 60;
    let mut fixture = Fixture::new();
    let sid = crate::catalog::ServerId::from_raw(0);
    let movies = |start: usize| (start..start + PAGE).map(|i| crate::catalog_fetch::PmsMovie {
        sid, rk: format!("large-{i}"), title: format!("Large {i}"), ..Default::default()
    }).collect::<Vec<_>>();
    fixture.listing = crate::browse::view::ListingSnapshot::fixture(
        sid, movies(0).into_iter().map(Some).collect(), Vec::new()).with_total(TOTAL);

    let mut page = fixture.screen();
    let (initial_slots, initial_known) = page.pair.detail.publication_ops();
    let initial_registry = page.keys.register_probes();
    page.pair.detail.reset_publication_ops();
    page.keys.reset_register_probes();

    fixture.listing = fixture.listing.clone().with_page(PAGE, movies(PAGE));
    page.sync(&fixture.cx(None));
    let (next_slots, next_known) = page.pair.detail.publication_ops();
    let next_registry = page.keys.register_probes();
    eprintln!("publication-ops initial slots={initial_slots} known={initial_known} registry={initial_registry}; next slots={next_slots} known={next_known} registry={next_registry}");

    assert!(initial_slots == TOTAL && initial_known <= TOTAL && initial_registry <= TOTAL + 128
        && next_slots == PAGE && next_known <= PAGE && next_registry <= PAGE + 128,
        "publication work must be linear initially and page-bounded later: initial slots={initial_slots} known={initial_known} registry={initial_registry}; next slots={next_slots} known={next_known} registry={next_registry}");
}

#[test]
fn derived_grid_indexes_survive_reorder_truncation_clear_and_restore() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    let original = fixture.listing.clone();
    let mut page = fixture.screen();
    let stable = page.pair.detail.elem_at(5).unwrap();
    let removed = page.pair.detail.elem_at(30).unwrap();

    let mut reordered = (0..36).map(|i| original.view().item(i).unwrap().clone()).collect::<Vec<_>>();
    reordered.swap(5, 17);
    fixture.listing = fixture.listing.clone().with_page(0, reordered);
    page.sync(&fixture.cx(None));
    assert_eq!(page.pair.detail.elem_at(17), Some(stable));
    assert_eq!(page.pair.detail.index_of(stable), Some(17), "a same-query reorder follows the stable item key");

    fixture.listing = fixture.listing.clone().with_total(10);
    page.sync(&fixture.cx(None));
    assert_eq!(page.pair.detail.index_of(stable), None);
    assert_eq!(page.pair.detail.index_of(removed), None);
    assert_eq!(page.pair.detail.fallback_for(stable), page.pair.detail.elem_at(9));
    assert_eq!(page.pair.detail.fallback_for(removed), page.pair.detail.elem_at(9),
        "a truncated item falls back through its last published slot");

    let memory = page.page_memory();
    fixture.listing = crate::browse::view::ListingSnapshot::absent();
    page.sync(&fixture.cx(None));
    assert!(page.pair.detail.elems.is_empty());
    assert_eq!(page.pair.detail.index_of(stable), None);

    fixture.listing = original.with_total(10);
    let mut restored = LibraryScreen::new(ENTRY, InstanceId(20), None);
    restored.restore(&memory);
    restored.sync(&fixture.cx(None));
    assert_eq!(restored.pair.detail.elem_at(5), Some(stable),
        "restore rebuilds lookup indexes and reuses the stable item key");
    assert_eq!(restored.pair.detail.index_of(stable), Some(5));
    assert_eq!(restored.pair.detail.fallback_for(removed), restored.pair.detail.elem_at(9));
}

#[test]
fn down_from_a_missing_final_row_column_clamps_to_the_last_item() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    let sid = crate::catalog::ServerId::from_raw(0);
    fixture.listing = crate::browse::view::ListingSnapshot::fixture(sid,
        (0..8).map(|i| Some(crate::catalog_fetch::PmsMovie { sid, rk: format!("{i}"), ..Default::default() })).collect(),
        Vec::new());
    let mut page = fixture.screen();
    let mut engine = FocusEngine::new();
    let key = page.key(page.pair.detail.elems[5]);
    engine.set(OWNER, key, Some(page.pair.groups_config().detail), By::Restore);
    direction(&mut page, &mut engine, &fixture, Dir::Down);
    assert_eq!(engine.current(OWNER), Some(page.key(page.pair.detail.elems[7])));
    direction(&mut page, &mut engine, &fixture, Dir::Down);
    assert_eq!(engine.current(OWNER), Some(page.key(page.pair.detail.elems[7])), "the last row remains an edge");
}

/// The alphabet bar is a block of the document (Litefin 6): the controls above it, the grid below
/// it, reached by UP and DOWN only - the grid's right edge is an edge, not a door into it.
#[test]
fn the_alphabet_bar_stands_between_the_controls_and_the_grid() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.screen();
    let master = page.pair.groups_config().master;
    let detail = page.pair.groups_config().detail;
    let mut links = Vec::new();
    <LibraryScreen as Screen<HostFixture>>::links(&page, &mut links);
    let link = |from: GroupId, dir: Dir| links.iter().find(|link| link.from == from && link.dir == dir).map(|link| link.to);
    eprintln!("LINKS {:?} toolbar {:?} tabs {:?} master {master:?} detail {detail:?}", links.iter().map(|l| (l.from, l.dir, l.to)).collect::<Vec<_>>(), page.toolbar_group(), page.tabs);
    assert_eq!(link(page.toolbar_group(), Dir::Down), Some(master));
    assert_eq!(link(master, Dir::Down), Some(detail));
    assert_eq!(link(detail, Dir::Up), Some(master));
    assert_eq!(link(master, Dir::Left), None, "no side door out of the bar");
    let mut groups = Vec::new();
    page.groups(&fixture.cx(None), &mut groups);
    let bar = groups.iter().find(|group| group.id == master).expect("the bar is always a group under a grid");
    assert_eq!(bar.extent.y, page.target_layout.alpha_y(page.scroll_target));
    assert_eq!(bar.len, layout::ALPHABET.len());

    let mut engine = FocusEngine::new();
    let last = page.key(page.pair.detail.elem_at(COLS - 1).unwrap());
    engine.set(OWNER, last, Some(detail), By::Restore);
    direction(&mut page, &mut engine, &fixture, Dir::Right);
    assert_eq!(engine.current(OWNER), Some(last), "the grid's right edge does not enter the bar");
    let cx = fixture.cx(Some(last));
    let letter = page.key(page.pair.master.elems[0]);
    let placed = page.place(&letter.elem, &cx, At::Drawn).unwrap();
    let mut draw = DrawFrame::new(&cx, crate::ui::Painter::root());
    page.record_stops(&mut draw);
    let mut hit = crate::ui::hit::HitMap::new();
    hit.fill(draw.into_stops()); hit.swap();
    assert_eq!(hit.top_at(placed.rect.cx(), placed.rect.cy()).map(|stop| stop.key), Some(letter),
        "the bar paints and registers its letters");
}

#[test]
fn retry_stop_matches_the_shared_measured_status_action_with_and_without_reason() {
    let _guard = nj_base::testlock::serial();
    for owner in ["", "friend"] {
        let mut fixture = Fixture::new();
        fixture.listing = crate::stores::browse::ListingSnapshot::empty_for_test();
        fixture.directory = crate::browse::view::DirectorySnapshot::fixture_source(4, crate::catalog::ServerId::from_raw(7),
            crate::browse::SrcGroup { name: "Cinema server".into(), handle: owner.into(),
                state: crate::browse::SourceState::Unreachable, tier: None }, SecFetch::Failed);
        let page = fixture.screen();
        let cx = fixture.cx(Some(page.key(RETRY)));
        let (caption, reason) = page.status_text(&cx);
        assert_eq!(reason.is_some(), !owner.is_empty());
        let mut overlay = crate::ui::widgets::StatusOverlay::new(page.status_frame(), &caption,
            crate::ui::widgets::StatusKind::Failed).page(crate::ui::icons::Icon::ServerBadgeMinus).action(c"Try again");
        if let Some(reason) = &reason { overlay = overlay.reason(reason); }
        let expected = overlay.action_frame_measured(cx.measure).unwrap();
        for at in [At::Drawn, At::SpringTarget] {
            let actual = page.place(&RETRY, &cx, at).unwrap().rect;
            assert_eq!([actual.x, actual.y, actual.w, actual.h], [expected.x, expected.y, expected.w, expected.h],
                "reason={owner:?}, at={at:?}");
        }
    }
}

/// **A failed source names a wrong clock when key mode cannot help** (`net::keypin::blocked_for`): the
/// clock reason and glyph take the slot, ahead of the "shared by" line and behind an offered
/// plaintext server's reason. No fact is byte-for-byte today's read-out, the reason moves the row,
/// and the *Try again* stop is the pill the same frame draws.
#[test]
fn a_failed_source_names_a_wrong_clock_when_key_mode_cannot_help() {
    use nj_net::net::keypin::{self, Blocked};
    use crate::ui::icons::Icon;
    use crate::ui::widgets::StatusOverlay;
    let _guard = nj_base::testlock::serial();
    crate::catalog::grant::reset_for_test();
    let key = keypin::key_of("library-clock.invalid", 32400);
    let _scoped = keypin::Scoped::watch_machine("library-clock-machine", &key);
    crate::catalog::reset_servers_for_test();
    let failed_sid = crate::catalog::register_pinned_with_client_id("library-clock-machine",
        &crate::catalog::Origin::http("192.168.1.51", 32400), "", None, "client", Default::default());
    // `tick_damaged`: a fact appearing or clearing under a static Failed read-out moves the
    // action row, so the tick must damage the frame.
    let tick = tick_damaged;
    let read = |page: &LibraryScreen, fixture: &Fixture| {
        let cx = fixture.cx(Some(page.key(RETRY)));
        let (caption, reason) = page.status_text(&cx);
        let overlay = page.status_overlay(&cx, &caption, reason.as_deref());
        let (shown, glyph) = (overlay.reason.map(|r| r.to_owned()), overlay.glyph);
        let row = overlay.action_frame_measured(cx.measure).unwrap();
        let stop = page.place(&RETRY, &cx, At::Drawn).unwrap().rect;
        assert_eq!([stop.x, stop.y, stop.w, stop.h], [row.x, row.y, row.w, row.h], "the hit rect is the drawn pill");
        (shown, glyph, row, caption)
    };
    for owner in ["", "friend"] {
        let fixture = Fixture::failed_source(failed_sid, owner);
        let mut page = fixture.screen();
        tick(&mut page, &fixture);
        let (shown, glyph, plain_row, caption) = read(&page, &fixture);
        let shared = nj_platform::i18n::msg::browse_library_shared_unreachable("friend");
        assert_eq!(shown.as_ref().and_then(|r| r.to_str().ok()), (!owner.is_empty()).then_some(shared.as_str()),
            "no fact: today's read-out");
        assert_eq!(glyph, Some(Icon::ServerBadgeMinus));

        keypin::strict_failure(&key, 60, Some(10));
        let (stale, ..) = read(&page, &fixture);
        assert_eq!(stale, shown, "the held fact does not change between ticks");
        assert!(tick(&mut page, &fixture), "a fact appearing under a static read-out damages the frame");
        let (shown, glyph, row, clock_caption) = read(&page, &fixture);
        assert_eq!(shown.as_deref(), Some(nj_platform::i18n::msg::browse_clock_no_key_c()), "owner {owner:?}");
        assert_eq!(glyph, Some(Icon::ClockBadgeAlert));
        assert_eq!(clock_caption, caption, "the verdict is unchanged");
        assert!(row.y >= plain_row.y);
        if owner.is_empty() {
            assert!(row.y > plain_row.y, "the reason moves the row");
        }
        assert!(row.y >= StatusOverlay::FULL_ANCHOR_TOP);

        keypin::key_changed(&key);
        assert!(tick(&mut page, &fixture));
        assert_eq!(page.clock.blocked(), Some(Blocked::KeyChanged));
        let (shown, ..) = read(&page, &fixture);
        assert_eq!(shown.as_deref(), Some(nj_platform::i18n::msg::browse_clock_key_changed_c()));

        keypin::strict_established(&key);
        assert!(tick(&mut page, &fixture), "…and one clearing does too");
        let (shown, glyph, ..) = read(&page, &fixture);
        assert_eq!(shown.as_ref().and_then(|r| r.to_str().ok()), (!owner.is_empty()).then_some(shared.as_str()),
            "the fact cleared: the shared-by line is back");
        assert_eq!(glyph, Some(Icon::ServerBadgeMinus));
    }

    // An offered plaintext server's reason outranks the clock's, glyph included.
    use crate::catalog::session::PlaintextChoice;
    crate::catalog::reset_servers_for_test();
    let sid = crate::catalog::register_pinned_with_client_id("lan-machine", &crate::catalog::Origin::http("192.168.1.50", 32400), "", None, "client", Default::default());
    let fixture = Fixture::failed_source(sid, "friend");
    let mut page = fixture.screen();
    let lan_key = keypin::key_of("192.168.1.50", 32400);
    let _lan = keypin::Scoped::watch_machine("lan-machine", &lan_key);
    keypin::strict_failure(&lan_key, 60, Some(10));
    crate::catalog::grant::offered(crate::catalog::grant::scope(),
        crate::catalog::grant::PlaintextVerdict { machine_id: "lan-machine".into(), name: "nas".into(), shared_by: String::new(),
            eligibility: crate::catalog::probe::PlaintextEligibility::Eligible, choice: PlaintextChoice::Undecided });
    tick(&mut page, &fixture);
    assert!(page.clock.blocked().is_some(), "the clock fact stands");
    let (shown, glyph, ..) = read(&page, &fixture);
    assert!(shown.as_ref().and_then(|r| r.to_str().ok()).is_some_and(|r| r.contains("Select Connect")), "{shown:?}");
    assert_eq!(glyph, Some(Icon::ServerBadgeMinus));
    crate::catalog::grant::reset_for_test();
}

/// **A fact about ANOTHER server does not colour this source's read-out** (scenario B): the
/// Library asks `net::keypin::blocked_for` about the failed source's own machine, so a second bound
/// server's expired certificate leaves it alone, and a fact for the source's own machine shows.
#[test]
fn a_failed_source_ignores_a_clock_fact_about_another_server() {
    use nj_net::net::keypin;
    use crate::ui::icons::Icon;
    let _guard = nj_base::testlock::serial();
    crate::catalog::grant::reset_for_test();
    crate::catalog::reset_servers_for_test();
    let here = keypin::key_of("library-here.invalid", 32400);
    let elsewhere = keypin::key_of("library-elsewhere.invalid", 32400);
    let _here = keypin::Scoped::watch_machine("library-here-machine", &here);
    let _elsewhere = keypin::Scoped::watch_machine("library-elsewhere-machine", &elsewhere);
    let sid = crate::catalog::register_pinned_with_client_id("library-here-machine",
        &crate::catalog::Origin::http("192.168.1.52", 32400), "", None, "client", Default::default());
    let fixture = Fixture::failed_source(sid, "");
    let mut page = fixture.screen();
    let tick = |page: &mut LibraryScreen| tick_damaged(page, &fixture);
    let shown = |page: &LibraryScreen| {
        let cx = fixture.cx(Some(page.key(RETRY)));
        let (caption, reason) = page.status_text(&cx);
        let overlay = page.status_overlay(&cx, &caption, reason.as_deref());
        (overlay.reason.map(|r| r.to_owned()), overlay.glyph)
    };
    tick(&mut page);
    keypin::strict_failure(&elsewhere, 60, Some(10));
    keypin::key_changed(&elsewhere);
    tick(&mut page);
    assert_eq!(page.clock.blocked(), None, "another server's fact is not this source's");
    assert_eq!(shown(&page), (None, Some(Icon::ServerBadgeMinus)), "today's read-out");

    keypin::strict_failure(&here, 60, Some(10));
    tick(&mut page);
    assert_eq!(shown(&page), (Some(nj_platform::i18n::msg::browse_clock_no_key_c().to_owned()), Some(Icon::ClockBadgeAlert)));
}

/// **A failed Library section and a failed Home stand on ONE line** (owner, 2026-09-19: the
/// Library's read-out sat ~y 830 while Home's was near the centre). The Library's verdict hangs
/// from `StatusOverlay::FULL_ANCHOR_TOP` exactly where Home's failed hub read-out puts its own
/// (`screens/home`'s `status_overlay`: the full frame, `.page()`, *Try again*, no reason); its own
/// server's read-out carries no reason, so its *Try again* is Home's too, word for word, while a
/// borrowed source's reason stacks the row lower by the reason slot and the read-out's drop, on the same column.
#[test]
fn a_failed_library_section_and_a_failed_home_share_the_verdict_and_the_row() {
    use crate::ui::widgets::{StatusKind, StatusOverlay};
    let _guard = nj_base::testlock::serial();
    let home = StatusOverlay::new(Rect::FULL, c"Can\u{2019}t reach your Jellyfin server", StatusKind::Failed)
        .page(crate::ui::icons::Icon::ServerBadgeMinus)
        .action(c"Try again");
    for owner in ["", "friend"] {
        let mut fixture = Fixture::new();
        fixture.listing = crate::stores::browse::ListingSnapshot::empty_for_test();
        fixture.directory = crate::browse::view::DirectorySnapshot::fixture_source(4, crate::catalog::ServerId::from_raw(7),
            crate::browse::SrcGroup { name: "Cinema server".into(), handle: owner.into(),
                state: crate::browse::SourceState::Unreachable, tier: None }, SecFetch::Failed);
        let page = fixture.screen();
        let cx = fixture.cx(Some(page.key(RETRY)));
        let (caption, reason) = page.status_text(&cx);
        if owner.is_empty() {
            assert_eq!(caption.to_str().unwrap(), "Can\u{2019}t reach your Jellyfin server");
        }
        let library = page.status_overlay(&cx, &caption, reason.as_deref());
        let (lv, hv) = (library.verdict_band_measured(cx.measure), home.verdict_band_measured(cx.measure));
        assert_eq!(lv.y, hv.y, "owner={owner:?}: the verdicts share a line");
        assert_eq!(lv.y, StatusOverlay::FULL_ANCHOR_TOP);
        let lr = page.place(&RETRY, &cx, At::Drawn).unwrap().rect;
        let hr = home.action_frame_measured(cx.measure).unwrap();
        assert_eq!(lr.cx(), hr.cx(), "owner={owner:?}: one column");
        if owner.is_empty() {
            assert_eq!(lr.y, hr.y, "own server: the rows share a line");
        } else {
            assert!(lr.y > hr.y, "a borrowed source's reason stacks its row under it");
        }
    }
}

#[test]
fn a_fully_discovered_missing_kind_finishes_its_fade_and_has_no_foreign_grid() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    fixture.directory = crate::browse::view::DirectorySnapshot::fixture(1, 0, vec![
        crate::browse::view::SectionView { sid: Some(crate::catalog::ServerId::from_raw(0)), key: 1,
            kind: SecKind::Movie, row: crate::browse::SrcRow { section: 0, title: "Cinema".into(),
                pinned: true, current: true, ..Default::default() } }]);
    // A library the fully discovered table does not list (the rail named one that has gone).
    let gone = crate::screens::registry::LibrarySectionIdentity { sid: crate::catalog::ServerId::from_raw(0), key: 9 };
    let mut page = LibraryScreen::new(ENTRY, InstanceId(19), Some(gone.clone()));
    page.page_fade.mount();
    let mut out = Vec::new();
    let mut present = nj_machine::present::Present::new();
    for i in 0..40 {
        page.step(&ScreenEvent::Tick(Tick { ms: i * 20, dt_us: 20_000 }), &fixture.cx(None),
            &mut Effects::new(&mut out, MachineId::Instance(InstanceId(19)), &mut present));
    }
    assert_eq!(page.readout, Readout::Empty);
    assert_eq!(page.page_fade.alpha(), 1.0, "a terminal absent type must not hold the page at black");
    assert_eq!(page.wanted, Some(gone), "the requested library is not silently replaced by the current one");
    assert_eq!(page.status_text(&fixture.cx(None)).0.to_str().unwrap(), "Nothing here matches");
    assert!(!out.iter().any(|effect| matches!(&effect.fx, Fx::App(AppFx::Store(StoreId::Browse,
        StoreCmd::Browse(BrowseCmd::Addressed { work: LibraryWork::Want { .. }, .. }))))),
        "waiting for Shows cannot page through the retained Movies listing");
    let mut groups = Vec::new();
    page.groups(&fixture.cx(None), &mut groups);
    assert!(!groups.iter().any(|g| g.id == page.pair.groups_config().detail || g.id == page.toolbar_group()));
    assert!(page.focused_item(Some(page.key(page.keys.keys().first().map_or(0, |k| k.elem))), &fixture.cx(None)).is_none());
}

#[test]
fn owned_card_stops_clip_pointer_hits_and_hold_the_engine_item() {
    use crate::ui::hit::PointerKind;
    use crate::ui::input::{InputMachine, PressEvent};
    use nj_machine::machine::{PressArm, PressFrom};
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.screen();
    page.initial = false;
    page.scroll.jump(500.0);
    page.scroll_target = 500.0;
    let first = page.key(page.pair.detail.elems[0]);
    let chosen = page.key(page.pair.detail.elems[1]);
    page.relayout(Some(first));
    let cx = fixture.cx(Some(first));
    let mut draw = DrawFrame::new(&cx, crate::ui::Painter::root());
    page.record_stops(&mut draw);
    let stops = draw.into_stops();
    let stop = *stops.iter().find(|stop| stop.key == chosen).unwrap();
    let mut input = InputMachine::new();
    input.engine.set(OWNER, first, Some(page.pair.groups_config().detail), By::Restore);
    input.hit.fill(stops);
    input.hit.swap();
    assert!(stop.rect.y < 0.0, "the fixture contains a genuinely clipped card");
    assert!(input.hit.resolve(Some(ENTRY), PointerKind::Click, stop.rect.cx(), stop.rect.y + 1.0, Some(first)).miss);
    let y = stop.clip.y + 10.0;
    assert!(stop.rect.contains(stop.rect.cx(), y));
    let resolved = input.hit.resolve(Some(ENTRY), PointerKind::Click, stop.rect.cx(), y, Some(first));
    assert_eq!(resolved.focus, Some(chosen));
    assert_eq!(resolved.activate, Some((chosen, crate::ui::screen::Activate::Press)));
    input.engine.set(OWNER, chosen, page.group_of(&chosen.elem, &cx), By::Pointer);
    input.arm(PressArm { key: chosen, from: PressFrom::Pointer, holdable: true }, MachineId::Instance(InstanceId(19)), 0);
    let held = input.tick(crate::ui::press::LONG_MS + 1, 0.016);
    assert!(matches!(held.as_slice(), [PressEvent::Hold(_, _, key)] if *key == chosen));
    let PressEvent::Hold(id, _, _) = held[0] else { unreachable!() };
    let mut out = Vec::new();
    let mut present = nj_machine::present::Present::new();
    page.step(&ScreenEvent::PressHold(id), &fixture.cx(input.engine.current(OWNER)),
        &mut Effects::new(&mut out, MachineId::Instance(InstanceId(19)), &mut present));
    assert!(out.iter().any(|effect| matches!(&effect.fx,
        Fx::App(AppFx::Library(LibraryReq::ItemMenu { sid, rk, from_deck: false }))
            if *sid == crate::catalog::ServerId::from_raw(0) && rk == "2")));
    assert!(input.hit.resolve(Some(EntryId(99)), PointerKind::Click, stop.rect.cx(), y, Some(chosen)).miss,
        "a menu owner cannot click through to a retained Library stop");
}

#[test]
fn actual_sort_menu_traps_engine_navigation_in_its_own_entry() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let page = fixture.screen();
    let entry = EntryId(99);
    let owner = InputOwner::Entry(entry);
    let arg = crate::screens::registry::LibraryMenuArg { host: InstanceId(19),
        target: page.address(&fixture.cx(None)).unwrap(), kind: crate::screens::registry::LibraryMenuKind::Sort,
        anchor: [0; 4] };
    let mut menu = menu::LibraryMenu::new(entry, arg);
    let mut out = Vec::new();
    let mut present = nj_machine::present::Present::new();
    menu.step(&ScreenEvent::Enter(Enter::Fresh { focus: FocusTarget::ContainerGroup(GroupId(0)) }), &fixture.cx(None),
        &mut Effects::new(&mut out, MachineId::Instance(InstanceId(20)), &mut present));
    let mut engine = FocusEngine::new();
    engine.enter(owner, &menu, FocusTarget::ContainerGroup(GroupId(0)), None, &fixture.cx(None));
    let first = engine.current(owner).expect("actual menu has a sort row");
    for dir in [Dir::Up, Dir::Left, Dir::Right, Dir::Down] {
        engine.move_dir(owner, &menu, &[], dir, &fixture.cx(Some(first)));
        assert_eq!(engine.current(owner).unwrap().entry, entry);
    }
    assert_eq!(engine.current(OWNER), None, "menu navigation never writes Library focus");
}

#[test]
fn a_compact_menu_keeps_the_host_store_pump_and_deferred_commit_live() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.screen();
    page.wanted = None;
    let target = page.address(&fixture.cx(None)).unwrap();
    let mut out = Vec::new();
    let mut present = nj_machine::present::Present::new();
    let mut fx = Effects::new(&mut out, MachineId::Instance(InstanceId(19)), &mut present);
    page.step(&ScreenEvent::Cover, &fixture.cx(None), &mut fx);
    page.step(&ScreenEvent::App(AppMsg::LibraryEdit { target,
        edit: crate::stores::browse::QueryEdit::Unwatched(true) }), &fixture.cx(None), &mut fx);
    page.step(&ScreenEvent::Tick(Tick { ms: 80, dt_us: 80_000 }), &fixture.cx(None), &mut fx);
    drop(fx);
    assert!(out.iter().any(|effect| matches!(&effect.fx,
        Fx::App(AppFx::Store(StoreId::Browse, StoreCmd::Browse(BrowseCmd::Addressed {
            work: LibraryWork::Commit { query: Some(crate::stores::browse::QueryEdit::Unwatched(true)), .. }, .. }))))));
    assert!(out.iter().any(|effect| matches!(&effect.fx, Fx::App(AppFx::StoreWork(StoreWork::Browse)))),
        "the store must fetch the just-committed query while the compact menu remains open");
}

#[test]
fn published_library_projection_and_layout_enter_canonical_state() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let hash = |page: &LibraryScreen| { let mut c = Canon::new(); page.write(&mut c); c.finish() };
    let mut page = fixture.screen();
    let initial = hash(&page);
    page.pair.detail.elems.swap(0, 1);
    assert_ne!(initial, hash(&page), "published item order determines the next navigation step");
    let prior = hash(&page);
    page.target_layout.rows += 1;
    assert_ne!(prior, hash(&page), "target document geometry determines reveal and placement");
    let prior = hash(&page);
    page.readout = Readout::Failed;
    assert_ne!(prior, hash(&page), "failure control availability is logical state");
}

#[test]
fn head_pill_motion_enters_canonical_state() {
    let hash = |page: &LibraryScreen| {
        let mut c = Canon::new();
        page.write(&mut c);
        c.finish()
    };
    let mut page = LibraryScreen::new(ENTRY, InstanceId(19), None);
    let initial = hash(&page);
    page.head_pop.step(Some(0), 0.016);
    assert_ne!(initial, hash(&page), "control focus and spring velocity affect subsequent frames");
}

#[test]
fn pending_semantic_commits_change_the_library_state_hash() {
    fn hash(page: &LibraryScreen) -> u64 {
        let mut canon = Canon::new();
        page.write(&mut canon);
        canon.finish()
    }
    let mut page = LibraryScreen::new(ENTRY, InstanceId(19), None);
    let empty = hash(&page);
    let target = GridTarget { epoch: 1, sid: crate::catalog::ServerId::from_raw(0), section: 1, query: 1 };
    let actions = [GridAction::Unwatched { desired: true }, GridAction::Unwatched { desired: false },
        GridAction::Sort { key: "titleSort".into(), desc: false }, GridAction::Sort { key: "titleSort".into(), desc: true },
        GridAction::Sort { key: "addedAt".into(), desc: true }, GridAction::Genre { id: None },
        GridAction::Genre { id: Some("7".into()) }, GridAction::Drill { id: None },
        GridAction::Drill { id: Some("7".into()) }];
    let mut hashes = vec![empty];
    for action in actions {
        page.pending.request_grid(target.clone(), action);
        let next = hash(&page);
        assert!(!hashes.contains(&next), "distinct semantic actions must have distinct encodings");
        hashes.push(next);
    }
}

struct Fixture {
    stores: Option<crate::stores::Stores>,
    listing: crate::stores::browse::ListingSnapshot,
    directory: crate::stores::browse::DirectorySnapshot,
    hubs: crate::stores::browse::HubsSnapshot,
    measure: FixtureMeasure,
}

#[test]
fn discovery_failure_retry_targets_the_source_without_a_section() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    let sid = crate::catalog::ServerId::from_raw(7);
    fixture.listing = crate::stores::browse::ListingSnapshot::empty_for_test();
    fixture.directory = crate::browse::view::DirectorySnapshot::fixture_source(4, sid,
        crate::browse::SrcGroup { name: "Cinema server".into(), handle: "friend".into(),
            state: crate::browse::SourceState::Unreachable, tier: None }, SecFetch::Failed);
    let mut page = fixture.screen();
    assert!(fixture.listing.view().id().is_none());
    assert_eq!(page.readout, Readout::Failed);
    assert_eq!(page.status_text(&fixture.cx(None)).0.to_str().unwrap(), "Can\u{2019}t reach Cinema server");
    let mut out = Vec::new();
    let mut present = nj_machine::present::Present::new();
    page.activate(RETRY, false, &fixture.cx(Some(page.key(RETRY))),
        &mut Effects::new(&mut out, MachineId::Instance(InstanceId(19)), &mut present));
    assert!(out.iter().any(|effect| matches!(&effect.fx,
        Fx::App(AppFx::Store(StoreId::Browse, StoreCmd::Browse(BrowseCmd::RetrySource { epoch: 4, sid: target }))) if *target == sid)),
        "Retry must issue work even when discovery never produced a section address");
}

/// **A failed source whose server discovery offers "Connect without encryption?" for asks it**
/// (PLX-NATIVE-10, code review 3) — the SAME question Home and the sign-in ask
/// (`screens::plaintext_question`): the reason says why, *Connect* replaces *Try again* and opens
/// the question seated on *Not now*, and its *Connect* sends one answer, re-finding this source's
/// slot. Another server's offer does not change this source's read-out.
#[test]
fn a_failed_source_over_an_offered_server_asks_the_shared_question() {
    use crate::catalog::session::PlaintextChoice;
    use super::super::plaintext_question::connect;
    let _guard = nj_base::testlock::serial();
    crate::catalog::reset_servers_for_test();
    crate::catalog::grant::reset_for_test();
    let sid = crate::catalog::register_pinned_with_client_id("lan-machine", &crate::catalog::Origin::http("192.168.1.50", 32400), "", None, "client", Default::default());
    let mut fixture = Fixture::new();
    fixture.listing = crate::stores::browse::ListingSnapshot::empty_for_test();
    fixture.directory = crate::browse::view::DirectorySnapshot::fixture_source(4, sid,
        crate::browse::SrcGroup { name: "Cinema server".into(), handle: String::new(),
            state: crate::browse::SourceState::Unreachable, tier: None }, SecFetch::Failed);
    let mut page = fixture.screen();
    let offer = |machine: &str| crate::catalog::grant::offered(crate::catalog::grant::scope(),
        crate::catalog::grant::PlaintextVerdict { machine_id: machine.into(), name: "nas".into(), shared_by: String::new(),
            eligibility: crate::catalog::probe::PlaintextEligibility::Eligible, choice: PlaintextChoice::Undecided });
    let tick = |page: &mut LibraryScreen| {
        let mut out = Vec::new();
        let mut present = nj_machine::present::Present::new();
        page.step(&ScreenEvent::Tick(Tick::default()), &fixture.cx(None),
            &mut Effects::new(&mut out, MachineId::Instance(InstanceId(19)), &mut present));
    };
    offer("another-machine");
    tick(&mut page);
    let cx = fixture.cx(Some(page.key(RETRY)));
    let (caption, reason) = page.status_text(&cx);
    assert_eq!(page.status_overlay(&cx, &caption, reason.as_deref()).action, Some(c"Try again"),
        "another server's offer is not this source's");

    offer("lan-machine");
    tick(&mut page);
    let (caption, reason) = page.status_text(&cx);
    assert!(reason.as_ref().and_then(|r| r.to_str().ok()).is_some_and(|r| r.contains("Select Connect")), "{reason:?}");
    assert_eq!(page.status_overlay(&cx, &caption, reason.as_deref()).action, Some(connect()));
    let mut out = Vec::new();
    let mut present = nj_machine::present::Present::new();
    page.activate(RETRY, false, &cx, &mut Effects::new(&mut out, MachineId::Instance(InstanceId(19)), &mut present));
    assert!(!out.iter().any(|e| matches!(&e.fx, Fx::App(AppFx::Store(..)))), "Connect asks; it does not retry");
    assert!(page.plaintext_alert.is_open());
    let mut out = Vec::new();
    let mut present = nj_machine::present::Present::new();
    page.step(&ScreenEvent::PressCommit(nj_machine::machine::PressId(1)), &fixture.cx(Some(page.key(PLAINTEXT_CONNECT))),
        &mut Effects::new(&mut out, MachineId::Instance(InstanceId(19)), &mut present));
    let answers: Vec<_> = out.iter().filter_map(|e| match &e.fx {
        Fx::App(AppFx::Session(crate::auth::SessionCmd::AnswerPlaintext { machine_id, choice, sid: target }))
            if machine_id == "lan-machine" => Some((*choice, *target)),
        _ => None,
    }).collect();
    assert_eq!(answers, [(PlaintextChoice::Allowed, Some(sid))]);
    crate::catalog::grant::reset_for_test();
    crate::catalog::reset_servers_for_test();
}

#[test]
fn failed_and_empty_readouts_offer_only_their_real_owned_controls() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    let original = fixture.listing.clone();
    // An empty answer keeps the heading row (its TYPE chip is how the reader leaves the empty
    // listing), so `head` is not `grid`.
    for (fetch, total, expected, grid, head, retry) in [
        (SecFetch::Failed, 36, Readout::Grid, true, true, false),
        (SecFetch::Ready, 0, Readout::Empty, false, true, false),
        (SecFetch::Failed, -1, Readout::Failed, false, false, true),
        (SecFetch::Loading, -1, Readout::Loading, false, false, false),
    ] {
        fixture.listing = original.clone().with_fetch(fetch, total);
        let page = fixture.screen();
        let cx = fixture.cx(None);
        assert_eq!(page.readout, expected);
        let mut groups = Vec::new();
        page.groups(&cx, &mut groups);
        assert_eq!(groups.iter().any(|g| g.id == page.pair.groups_config().detail), grid);
        for control in [TYPE, SORT, FILTER] {
            assert_eq!(page.place(&control, &cx, At::SpringTarget).is_some(), head);
        }
        assert_eq!(page.place(&RETRY, &cx, At::SpringTarget).is_some(), retry);
        if !grid {
            assert!(!groups.iter().any(|g| g.id == page.pair.groups_config().master), "stale letters are not a live rail");
        }
        if retry {
            let mut engine = FocusEngine::new();
            engine.enter(OWNER, &page, FocusTarget::ContainerGroup(STATUS_GROUP), None, &cx);
            assert_eq!(engine.current(OWNER), Some(page.key(RETRY)));
        }
    }
}

#[test]
fn foreign_section_replacement_upgrades_a_grid_fade_once() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    let mut page = fixture.screen();
    page.grid_fade.reload();
    fixture.listing = fixture.listing.clone().with_section(2, 1);
    page.sync(&fixture.cx(None));
    assert!(page.page_fade.is_swapping(), "foreign replacement remounts the entire document");
    assert_eq!(page.page_fade.alpha(), 0.0);
    assert!(!page.grid_fade.is_swapping());
    page.page_fade.tick(0.04, true);
    let alpha = page.page_fade.alpha();
    page.sync(&fixture.cx(None));
    assert_eq!(page.page_fade.alpha(), alpha, "the same publication must not remount twice");
}

#[test]
fn control_stops_use_the_pills_own_measurement() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let page = fixture.screen();
    let cx = fixture.cx(None);
    let sort = page.place(&SORT, &cx, At::Drawn).unwrap().rect;
    let filter = page.place(&FILTER, &cx, At::Drawn).unwrap().rect;
    assert_eq!(sort.w, page.control_pill(SORT, &cx).width(cx.measure));
    assert_eq!(filter.w, page.control_pill(FILTER, &cx).width(cx.measure));
    assert_eq!(filter.x, sort.x + sort.w + 20.0, "Litefin's controls stand 20 apart");
    for tab in page.tab_elems() {
        let rect = page.place(&tab, &cx, At::Drawn).unwrap().rect;
        assert_eq!(rect.h, layout::TAB_H);
    }
}

#[test]
fn section_grid_memories_do_not_overwrite_one_another() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    let first = fixture.listing.clone();
    let mut page = fixture.screen();
    let mut engine = FocusEngine::new();
    let group_a = page.pair.groups_config().detail;
    let card_a = page.key(page.pair.detail.elem_at(17).unwrap());
    engine.set(OWNER, card_a, Some(group_a), By::Restore);
    fixture.listing = first.clone().with_section(1, 2);
    page.sync(&fixture.cx(engine.current(OWNER)));
    let group_b = page.pair.groups_config().detail;
    assert_ne!(group_a, group_b, "each section needs its own engine memory slot");
    let card_b = page.key(page.pair.detail.elem_at(23).unwrap());
    engine.set(OWNER, card_b, Some(group_b), By::Restore);
    fixture.listing = first;
    page.sync(&fixture.cx(engine.current(OWNER)));
    assert_eq!(page.pair.groups_config().detail, group_a);
    assert!(engine.remembered_for(ENTRY).contains(&(group_a, card_a.elem)));
    assert!(engine.remembered_for(ENTRY).contains(&(group_b, card_b.elem)));
    engine.enter(OWNER, &page, FocusTarget::ContainerGroup(group_a), None, &fixture.cx(engine.current(OWNER)));
    assert_eq!(engine.current(OWNER), Some(card_a), "Remembered seating restores the exact section card");
    // Enter the bar from the controls: projection must consult only this section's grid memory.
    engine.set(OWNER, page.key(FILTER), Some(page.toolbar_group()), By::Dir);
    direction(&mut page, &mut engine, &fixture, Dir::Down);
    assert_eq!(page.pair.master.start_for_elem(engine.current(OWNER).unwrap().elem), Some(0));
}

#[test]
fn section_viewport_bookmarks_survive_switch_and_evicted_body() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    let first = fixture.listing.clone();
    let mut page = fixture.screen();
    let y = page.layout.row_reveal(3);
    page.scroll.jump(y);
    page.scroll_target = y;
    fixture.listing = first.clone().with_section(1, 2);
    page.sync(&fixture.cx(None));
    assert_eq!(page.scroll.pos, 0.0, "a new section starts at its own head");
    let PageMemory::Library(memory) = <LibraryScreen as Screen<HostFixture>>::memory(&page) else { panic!() };
    let mut evicted = LibraryScreen::new(ENTRY, InstanceId(20), None);
    evicted.restore(&memory);
    evicted.sync(&fixture.cx(None));
    fixture.listing = first;
    for page in [&mut page, &mut evicted] {
        page.sync(&fixture.cx(None));
        assert_eq!(page.scroll.pos, y, "viewport belongs to the section, including after body eviction");
        assert_eq!(page.scroll_target, y);
    }
}
impl Fixture {
    fn shelves(titles: &[&str], count: usize) -> Self {
        let mut fixture = Self::new();
        let stores = crate::stores::Stores::default();
        stores.browse.borrow_mut().seed_two_source_table_for_test();
        stores.capture_browse(&mut fixture.directory);
        stores.browse_run(BrowseCmd::SetCur(0));
        {
            let mut browse = stores.browse.borrow_mut();
            browse.seed_items_for_test(120);
            browse.seed_shelves_for_test(0, titles, count);
        }
        let publication = stores.capture_browse(&mut fixture.directory);
        fixture.listing = publication.listing;
        fixture.hubs = publication.section_hubs;
        fixture.stores = Some(stores);
        let listing = fixture.listing.view().id().unwrap();
        let hubs = fixture.hubs.view().id().unwrap();
        assert_eq!((listing.epoch, listing.sid, listing.section), (hubs.epoch, hubs.sid, hubs.section));
        assert_eq!(fixture.hubs.view().shelves().len(), titles.len());
        fixture
    }

    /// 1200 cards, each with a poster (`/poster/{i}`) and a still (`/still/{i}`): a grid that scrolls
    /// far past its window. `episodes` makes it an Episodes library (stills), else Primary (posters).
    fn art_listing(episodes: bool) -> Self {
        let sid = crate::catalog::ServerId::from_raw(0);
        let mut fixture = Self::new();
        fixture.listing = crate::browse::view::ListingSnapshot::fixture(sid,
            (0..1200).map(|i| Some(crate::catalog_fetch::PmsMovie {
                sid, rk: i.to_string(), kind: if episodes { 3 } else { 0 },
                thumb: format!("/poster/{i}"), still: format!("/still/{i}"),
                ..Default::default()
            })).collect(), Vec::new()).with_library_type(if episodes {
                crate::browse::LibraryType::Episodes
            } else { crate::browse::LibraryType::Primary });
        fixture
    }

    pub(super) fn new() -> Self {
        let sid = crate::catalog::ServerId::from_raw(0);
        let listing = crate::browse::view::ListingSnapshot::fixture(sid, (0..36).map(|i|
            Some(crate::catalog_fetch::PmsMovie { sid, rk: format!("{}", i + 1), title: format!("s{i:04x}"), ..Default::default() })).collect(),
            vec![("A".into(), 18), ("Z".into(), 18)]);
        let directory = crate::browse::view::DirectorySnapshot::fixture(1, 0, vec![
            crate::browse::view::SectionView { sid: Some(sid), key: 1, kind: SecKind::Movie,
                row: crate::browse::SrcRow { section: 0, title: "Cinema".into(), pinned: true, current: true, ..Default::default() } }]);
        Self {
            stores: None,
            listing,
            directory,
            hubs: crate::stores::browse::HubsSnapshot::empty_for_test(),
            measure: FixtureMeasure,
        }
    }
    pub(super) fn cx(&self, focus: Option<FocusKey<u32>>) -> Cx<'_, HostFixture> {
        Cx { views: Views { listing: self.listing.view(), directory: self.directory.view(), hubs: self.hubs.view() },
            tick: Tick::default(), measure: &self.measure, focus: FocusRead { current: focus , ..Default::default() },
            press: PressRead::default(), owner: OWNER }
    }
    pub(super) fn screen(&self) -> LibraryScreen {
        let mut page = LibraryScreen::new(ENTRY, InstanceId(19), None);
        page.sync(&self.cx(None));
        page
    }
    /// A source whose discovery failed, with no section address: the Failed read-out. `handle` is
    /// the sharing owner's ("" for the viewer's own server).
    fn failed_source(sid: crate::catalog::ServerId, handle: &str) -> Self {
        let mut fixture = Self::new();
        fixture.listing = crate::stores::browse::ListingSnapshot::empty_for_test();
        fixture.directory = crate::browse::view::DirectorySnapshot::fixture_source(4, sid,
            crate::browse::SrcGroup { name: "Cinema server".into(), handle: handle.into(),
                state: crate::browse::SourceState::Unreachable, tier: None }, SecFetch::Failed);
        fixture
    }
}
/// One Tick through the screen: whether it damaged the frame.
fn tick_damaged(page: &mut LibraryScreen, fixture: &Fixture) -> bool {
    let mut out = Vec::new();
    let mut present = nj_machine::present::Present::new();
    present.take(0);
    page.step(&ScreenEvent::Tick(Tick::default()), &fixture.cx(None),
        &mut Effects::new(&mut out, MachineId::Instance(InstanceId(19)), &mut present));
    present.changed()
}
fn deliver(page: &mut LibraryScreen, engine: &mut FocusEngine<u32>, fixture: &Fixture, event: ScreenEvent<HostFixture>) -> usize {
    let mut output = Vec::new();
    let mut present = nj_machine::present::Present::new();
    page.step(&event, &fixture.cx(engine.current(OWNER)),
        &mut Effects::new(&mut output, MachineId::Instance(InstanceId(19)), &mut present));
    let mut remembers = 0;
    for effect in output {
        if let Fx::Remember { group, elem } = effect.fx {
            remembers += 1;
            engine.remember_projected(ENTRY, group, elem);
        }
    }
    remembers
}
fn direction(page: &mut LibraryScreen, engine: &mut FocusEngine<u32>, fixture: &Fixture, dir: Dir) -> usize {
    let mut links = Vec::new();
    <LibraryScreen as Screen<HostFixture>>::links(page, &mut links);
    let outcome = engine.move_dir(OWNER, page, &links, dir, &fixture.cx(engine.current(OWNER)));
    if let Outcome::Moved { from, to, by } = outcome {
        deliver(page, engine, fixture, ScreenEvent::FocusMoved { from, to, by })
    } else { 0 }
}

#[test]
fn shelf_horizontal_viewport_and_engine_item_survive_body_eviction() {
    let _guard = nj_base::testlock::serial();
    let session = crate::catalog::session::TempSession::new("library-shelf-return");
    session.watching("u-library-shelf-return");
    let fixture = Fixture::shelves(&["movie.inprogress.1"], 12);
    let mut page = fixture.screen();
    page.shelves[0].motion.restore_scroll(700.0, 12, &RowStyle::HOME);
    let key = page.key(page.shelves[0].elems[3]);
    let before = page.place(&key.elem, &fixture.cx(Some(key)), At::Drawn).unwrap().rest_rect;
    let mut engine = FocusEngine::new();
    engine.set(OWNER, key, Some(page.shelves[0].group), By::Restore);
    let memory = page.page_memory();
    let mut restored = LibraryScreen::new(ENTRY, InstanceId(20), None);
    restored.restore(&memory);
    restored.sync(&fixture.cx(Some(key)));
    engine.enter(OWNER, &restored, FocusTarget::ContainerGroup(restored.shelves[0].group), Some(key), &fixture.cx(Some(key)));
    assert_eq!(engine.current(OWNER), Some(key));
    assert_eq!(restored.shelves[0].motion.scroll_x(), 700.0);
    let after = restored.place(&key.elem, &fixture.cx(Some(key)), At::Drawn).unwrap().rest_rect;
    assert_eq!([before.x, before.y, before.w, before.h], [after.x, after.y, after.w, after.h]);
    assert_eq!(restored.focused_item(engine.current(OWNER), &fixture.cx(Some(key))).unwrap().rk, "movie.inprogress.1-3");
}

#[test]
fn shelf_return_follows_the_film_then_its_last_published_slot() {
    let _guard = nj_base::testlock::serial();
    let session = crate::catalog::session::TempSession::new("library-shelf-removal");
    session.watching("u-library-shelf-removal");
    for evict in [false, true] {
        for remove_focused in [false, true] {
            let mut fixture = Fixture::shelves(&["movie.inprogress.1"], 6);
            let mut page = fixture.screen();
            let key = page.key(page.shelves[0].elems[3]);
            let memory = page.page_memory();
            let item = fixture.hubs.view().shelves()[0].items[if remove_focused { 3 } else { 0 }].clone();
            let stores = fixture.stores.as_ref().unwrap();
            assert!(stores.browse_run(BrowseCmd::LeftTheDeck { sid: item.sid, rk: item.rk }));
            let publication = stores.capture_browse(&mut fixture.directory);
            fixture.hubs = publication.section_hubs;
            if evict {
                page = LibraryScreen::new(ENTRY, InstanceId(20), None);
                page.restore(&memory);
            }
            page.sync(&fixture.cx(Some(key)));
            let mut engine = FocusEngine::new();
            engine.set(OWNER, key, Some(page.shelves[0].group), By::Restore);
            engine.enter(OWNER, &page, FocusTarget::ContainerGroup(page.shelves[0].group), Some(key), &fixture.cx(Some(key)));
            let focus = engine.current(OWNER).unwrap();
            let item = page.focused_item(Some(focus), &fixture.cx(Some(focus))).unwrap();
            assert_eq!(item.rk, if remove_focused { "movie.inprogress.1-4" } else { "movie.inprogress.1-3" });
            assert_eq!(page.shelves[0].elems.iter().position(|elem| *elem == focus.elem), Some(if remove_focused { 3 } else { 2 }));
            if !remove_focused { assert_eq!(focus, key); }
        }
    }
}

#[test]
fn the_bar_seats_the_letter_of_the_remembered_title_and_returns_to_the_controls() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.screen();
    let mut engine = FocusEngine::new();
    let exact = page.key(page.pair.detail.elem_at(23).unwrap());
    engine.set(OWNER, exact, Some(page.pair.groups_config().detail), By::Restore);
    engine.set(OWNER, page.key(FILTER), Some(page.toolbar_group()), By::Dir);
    assert_eq!(direction(&mut page, &mut engine, &fixture, Dir::Down), 0, "moving along the bar jumps nothing");
    assert_eq!(page.pair.master.start_for_elem(engine.current(OWNER).unwrap().elem), Some(18));
    direction(&mut page, &mut engine, &fixture, Dir::Up);
    assert_eq!(engine.current(OWNER), Some(page.key(FILTER)));
    assert_eq!(engine.remembered_for(ENTRY).iter().find(|(g, _)| *g == page.pair.groups_config().detail).unwrap().1, exact.elem);
}

#[test]
fn removed_grid_key_keeps_its_typed_master_detail_reconciliation_path() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::new();
    let mut page = fixture.screen();
    let original = page.key(page.pair.detail.elem_at(17).unwrap());
    let sid = crate::catalog::ServerId::from_raw(0);
    fixture.listing = crate::browse::view::ListingSnapshot::fixture(sid,
        (0..35).map(|i| Some(crate::catalog_fetch::PmsMovie { sid, rk: format!("{}", i + 100), ..Default::default() })).collect(),
        vec![("A".into(), 18), ("Z".into(), 17)]);
    page.sync(&fixture.cx(Some(original)));
    assert_eq!(<LibraryScreen as Focusable<HostFixture>>::group_of(&page, &original.elem, &fixture.cx(Some(original))), None);
    let recovered = <LibraryScreen as Focusable<HostFixture>>::reconcile(&page, original, &fixture.cx(Some(original)));
    assert_eq!(page.grid_position(Some(recovered)), Some((17 / COLS, 17 % COLS)));
}

#[test]
fn a_letter_jumps_the_grid_to_its_first_title_on_activation_only() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.screen();
    let mut engine = FocusEngine::new();
    let exact = page.key(page.pair.detail.elem_at(17).unwrap());
    engine.set(OWNER, exact, Some(page.pair.groups_config().detail), By::Restore);
    let first_row = page.key(page.pair.detail.elem_at(5).unwrap());
    engine.set(OWNER, first_row, Some(page.pair.groups_config().detail), By::Dir);
    direction(&mut page, &mut engine, &fixture, Dir::Up);
    let letter = engine.current(OWNER).unwrap().elem;
    assert_eq!(page.pair.master.start_for_elem(letter), Some(0), "the bar seats the letter in view");
    direction(&mut page, &mut engine, &fixture, Dir::Right);
    let b = engine.current(OWNER).unwrap().elem;
    assert_eq!(page.pair.master.start_for_elem(b), Some(18), "B..Y are absent and jump on to Z");
    assert_eq!(page.grid_position(engine.read(OWNER).remembered(page.pair.groups_config().detail)
        .map(|elem| page.key(elem))), Some((0, 5)), "moving along the bar jumps nothing");
    let mut output = Vec::new();
    let mut present = nj_machine::present::Present::new();
    page.step(&ScreenEvent::Activate(b), &fixture.cx(Some(page.key(b))),
        &mut Effects::new(&mut output, MachineId::Instance(InstanceId(19)), &mut present));
    let mut seat = None;
    for effect in output {
        match effect.fx {
            Fx::Remember { group, elem } => engine.remember_projected(ENTRY, group, elem),
            Fx::Deliver(_, Delivery::Screen(ScreenEvent::Enter(Enter::Fresh { focus }))) => seat = Some(focus),
            _ => {}
        }
    }
    engine.enter(OWNER, &page, seat.expect("the letter goes into the grid"), None, &fixture.cx(Some(page.key(b))));
    assert_eq!(page.grid_position(engine.current(OWNER)), Some((18 / COLS, 18 % COLS)));
}

/// Every grid card wears its title and year at rest, not only the focused one (spec 6): the
/// card names itself from its row through `card_row::draw_cell`.
#[test]
fn every_grid_card_wears_its_title_and_year_at_rest() {
    let _guard = nj_base::testlock::serial();
    let sid = crate::catalog::ServerId::from_raw(0);
    let mut fixture = Fixture::new();
    fixture.listing = crate::browse::view::ListingSnapshot::fixture(sid, (0..2 * COLS).map(|i|
        Some(crate::catalog_fetch::PmsMovie { sid, rk: format!("{}", i + 1), title: format!("Film {i}"),
            year: 2000 + i as i32, ..Default::default() })).collect(), Vec::new());
    let mut page = fixture.screen();
    let cx = fixture.cx(None);
    let runs = crate::ui::draw_census::capture_runs(|| {
        let mut frame = DrawFrame::new(&cx, crate::ui::Painter::recording());
        page.pair.detail.draw(&mut frame, Rect::FULL);
    });
    for i in 0..COLS {
        assert!(runs.iter().any(|run| run.text == format!("Film {i}")), "card {i} wears its title: {runs:?}");
        assert!(runs.iter().any(|run| run.text == (2000 + i).to_string()), "card {i} wears its year: {runs:?}");
    }
}

#[test]
fn rapid_shelf_moves_use_settled_geometry_and_walk_each_document_row() {
    let _guard = nj_base::testlock::serial();
    let session = crate::catalog::session::TempSession::new("library-shelf-geometry");
    session.watching("u-library-shelf-geometry");
    let fixture = Fixture::shelves(&["s0", "s1", "s2"], 12);
    let listing_id = fixture.listing.view().id().unwrap();
    let hubs_id = fixture.hubs.view().id().unwrap();
    assert_eq!((listing_id.epoch, listing_id.sid, listing_id.section), (hubs_id.epoch, hubs_id.sid, hubs_id.section));
    assert_eq!(fixture.hubs.view().shelves().len(), 3);
    let mut page = fixture.screen();
    let mut engine = FocusEngine::new();
    let first = page.key(page.shelves[0].elems[3]);
    engine.set(OWNER, first, Some(page.shelves[0].group), By::Restore);
    let resting = page.place(&first.elem, &fixture.cx(Some(first)), At::Drawn).unwrap();
    let mut pressed_cx = fixture.cx(Some(first));
    pressed_cx.press = PressRead { scale: 0.85, is_long: true };
    let pressed = page.place(&first.elem, &pressed_cx, At::Drawn).unwrap();
    assert!(pressed.rect.w < resting.rect.w);
    let rect = |r: Rect| [r.x, r.y, r.w, r.h];
    assert_eq!(rect(pressed.rest_rect), rect(resting.rest_rect), "hold/menu opener stays on the unpressed card rectangle");
    direction(&mut page, &mut engine, &fixture, Dir::Down);
    assert_eq!(engine.current_group(OWNER), Some(page.shelves[1].group), "the grid cannot stand geometrically above the shelves that precede it");
    let key = engine.current(OWNER).unwrap();
    let drawn = page.place(&key.elem, &fixture.cx(Some(key)), At::Drawn).unwrap();
    let settled = page.place(&key.elem, &fixture.cx(Some(key)), At::SpringTarget).unwrap();
    assert_ne!(drawn.rect.y, settled.rect.y, "the second key must resolve against the destination document and scroll");
    direction(&mut page, &mut engine, &fixture, Dir::Down);
    assert_eq!(engine.current_group(OWNER), Some(page.shelves[2].group));
    assert_eq!(page.shelves[2].elems.iter().position(|elem| *elem == engine.current(OWNER).unwrap().elem), Some(3));
}

#[test]
fn shelf_publication_request_distinguishes_page_fade_from_grid_fade_and_head_focus() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.screen();
    page.initial = false;
    let grid = page.key(page.pair.detail.elem_at(0).unwrap());
    let request = |page: &mut LibraryScreen| {
        let mut out = Vec::new();
        let mut present = nj_machine::present::Present::new();
        page.step(&ScreenEvent::Tick(Tick::default()), &fixture.cx(Some(grid)),
            &mut Effects::new(&mut out, MachineId::Instance(InstanceId(19)), &mut present));
        out.into_iter().find_map(|effect| match effect.fx {
            Fx::App(AppFx::Library(LibraryReq::PublishShelves { hidden_page, at_head, .. })) => Some((hidden_page, at_head)), _ => None,
        }).unwrap()
    };
    assert_eq!(request(&mut page), (false, true), "grid focus at a settled document head does not starve the first shelf publication");
    page.scroll.jump(700.0);
    page.scroll_target = 700.0;
    page.grid_fade.reload();
    assert_eq!(request(&mut page), (false, false), "a fading grid leaves the shelves visible");
    page.page_fade.reload();
    assert_eq!(request(&mut page), (false, false), "the outgoing page remains visible until the fade floor");
    page.page_fade.mount();
    assert_eq!(request(&mut page), (true, false), "only the full-page fade permits publication away from the head");
}

/// **The page glyph and the Library's own live head never overlap** (the title and the tab
/// pills stay live above a failed section's read-out). `status_overlay` passes `glyph_ceiling` at
/// the tab pills' bottom (`CONTENT_TOP + TABS_DY + TAB_H`), and `StatusOverlay::glyph_rect` would
/// shrink the glyph box to clear it rather than let the two overlap. At the read-out's anchor it
/// never has to: the glyph draws at its natural, unshrunk box.
#[test]
fn the_page_glyph_and_the_librarys_live_head_never_overlap() {
    use crate::ui::widgets::StatusOverlay;
    let _guard = nj_base::testlock::serial();
    let tabs_bottom = layout::CONTENT_TOP + layout::TABS_DY + layout::TAB_H;
    assert_eq!(tabs_bottom, 187.0, "the head band moved - re-measure the fix against it");
    let mut fixture = Fixture::new();
    fixture.listing = fixture.listing.clone().with_fetch(SecFetch::Failed, -1);
    let page = fixture.screen();
    assert_eq!(page.readout, Readout::Failed);
    let cx = fixture.cx(Some(page.key(RETRY)));
    let (caption, reason) = page.status_text(&cx);
    let overlay = page.status_overlay(&cx, &caption, reason.as_deref());
    let glyph = overlay.glyph_frame().expect("the live head never omits the glyph at this anchor");
    assert!(glyph.y >= tabs_bottom + StatusOverlay::GLYPH_CEILING_MARGIN,
        "the glyph box (top y={}) must start clear of the tab pills' bottom (y={tabs_bottom})", glyph.y);
    assert_eq!(glyph.y, StatusOverlay::FULL_ANCHOR_TOP - StatusOverlay::GLYPH_GAP - StatusOverlay::GLYPH_SIZE,
        "the glyph keeps its natural, unshrunk position");
    assert_eq!(glyph.h, StatusOverlay::GLYPH_SIZE, "the glyph keeps its natural, unshrunk size");
}

mod type_tests { include!("type_tests.rs"); }

mod art_admission_tests { include!("art_admission_tests.rs"); }
mod lookahead_tests { include!("lookahead_tests.rs"); }

mod grid_motion_tests { include!("grid_motion_tests.rs"); }

#[test]
fn incremental_republication_keeps_unchanged_library_row_storage() {
    let _guard = nj_base::testlock::serial();
    let names: Vec<String> = (0..80).map(|i| format!("row-{i}")).collect();
    let titles: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut fixture = Fixture::shelves(&titles, 12);
    let mut page = fixture.screen();
    let before: Vec<_> = page.shelves.iter().map(|r| (r.elems.as_ptr(), r.elems.clone())).collect();
    let stores = fixture.stores.as_ref().unwrap();
    stores.browse.borrow_mut().seed_shelves_for_test(0, &titles, 12);
    let publication = stores.capture_browse(&mut fixture.directory);
    fixture.listing = publication.listing;
    fixture.hubs = publication.section_hubs;
    page.keys.reset_register_probes();
    page.sync(&fixture.cx(None));
    for (row, (ptr, elems)) in before.iter().enumerate() {
        assert_eq!(&page.shelves[row].elems, elems);
        assert_eq!(page.shelves[row].elems.as_ptr(), *ptr,
            "an unchanged publication must retain shelf {row}'s projection");
    }
    assert!(page.keys.register_probes() < 40, "unchanged shelves must not register every card again");
}

#[test]
fn incremental_republication_changes_only_one_library_row() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::shelves(&["a", "b", "c"], 12);
    let mut page = fixture.screen();
    let before: Vec<_> = page.shelves.iter().map(|r| (r.elems.as_ptr(), r.elems.clone())).collect();
    let saved = page.page_memory();
    let old_hash = PageMemory::Library(saved.clone()).hash();
    let stores = fixture.stores.as_ref().unwrap();
    stores.browse.borrow_mut().seed_shelves_for_test(0, &["a", "changed", "c"], 12);
    let publication = stores.capture_browse(&mut fixture.directory);
    fixture.listing = publication.listing; fixture.hubs = publication.section_hubs;
    page.keys.reset_register_probes();
    page.sync(&fixture.cx(None));
    for row in [0, 2] {
        assert_eq!(page.shelves[row].elems.as_ptr(), before[row].0);
        assert_eq!(page.shelves[row].elems, before[row].1);
    }
    assert!(page.keys.register_probes() < 40);
    assert_eq!(PageMemory::Library(saved).hash(), old_hash);
}

#[test]
fn incremental_republication_after_library_restore_rekeys_live_rows() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::shelves(&["old"], 12);
    let mut page = fixture.screen();
    let memory = page.page_memory();
    let stores = fixture.stores.as_ref().unwrap();
    stores.browse.borrow_mut().seed_shelves_for_test(0, &["new"], 12);
    let publication = stores.capture_browse(&mut fixture.directory);
    fixture.listing = publication.listing; fixture.hubs = publication.section_hubs;
    page.sync(&fixture.cx(None));
    page.restore(&memory);
    page.sync(&fixture.cx(None));
    for shelf in &page.shelves {
        for &elem in &shelf.elems {
            assert!(page.keys.key(elem).is_some(), "a reused row must name keys in the restored registry");
        }
    }
}

#[test]
fn incremental_republication_library_basis_covers_identity_and_placeholder_query() {
    let sid = crate::catalog::ServerId::from_raw(2);
    let mut shelf = crate::browse::section_hubs::Shelf {
        id: "same".into(), items: vec![crate::catalog_fetch::PmsMovie {
            sid, rk: "one".into(), ..Default::default()
        }], ..Default::default()
    };
    let basis = ShelfBasis::of(&shelf, 4);
    shelf.items[0].title = "new title".into();
    shelf.items[0].resume_ms = 8000;
    assert!(basis.matches(&shelf, 5), "real item keys do not depend on metadata or query");
    shelf.items[0].sid = crate::catalog::ServerId::from_raw(3);
    assert!(!basis.matches(&shelf, 4));
    shelf.items[0].sid = sid;
    shelf.items[0].rk.clear();
    let slot = ShelfBasis::of(&shelf, 4);
    assert!(slot.matches(&shelf, 4));
    assert!(!slot.matches(&shelf, 5), "placeholder keys include the listing query");
}

#[test]
fn incremental_republication_duplicate_library_rows_keep_group_recovery() {
    let _guard = nj_base::testlock::serial();
    let mut fixture = Fixture::shelves(&["same", "same"], 4);
    let mut page = fixture.screen();
    let stores = fixture.stores.as_ref().unwrap();
    stores.browse.borrow_mut().seed_shelves_for_test(0, &["same", "same"], 4);
    let publication = stores.capture_browse(&mut fixture.directory);
    fixture.listing = publication.listing; fixture.hubs = publication.section_hubs;
    page.keys.reset_register_probes();
    page.sync(&fixture.cx(None));
    assert!(page.keys.register_probes() >= 10, "duplicate rows use complete registration");
    let group = page.shelves[0].group;
    let group_key = page.keys.key(group.0).unwrap();
    assert_eq!(group_key.last_index, 1, "the last registered group place is retained");
}

/// **A hover moves focus, never the grid; a click brings the grid to its card.** The reported
/// bug: a Magic Remote hover seated the row it touched at the top of the screen, like a key does,
/// so every hover moved the page up to a screenful and a hover after an edge glide threw the glide
/// away. A key still seats its row at rest.
#[test]
fn a_hover_moves_no_grid_and_a_click_brings_it_to_its_card() {
    let _guard = nj_base::testlock::serial();
    let fixture = Fixture::new();
    let mut page = fixture.screen();
    page.initial = false;
    let mut engine = FocusEngine::new();
    let elems: Vec<u32> = [2 * COLS, 3 * COLS, 4 * COLS].iter().map(|&i| page.pair.detail.elem_at(i).unwrap()).collect();
    let keys: Vec<_> = elems.iter().map(|&e| page.key(e)).collect();
    let (row2, row3, row4) = (keys[0], keys[1], keys[2]);
    page.relayout(Some(row2));
    let rest2 = page.target_layout.row_reveal(2);
    page.scroll_target = rest2;
    page.scroll.jump(rest2);
    engine.set(OWNER, row2, Some(page.pair.groups_config().detail), By::Restore);
    let point = |page: &mut LibraryScreen, engine: &mut FocusEngine<u32>, to, by| {
        let from = engine.current(OWNER);
        engine.set(OWNER, to, Some(page.pair.groups_config().detail), by);
        deliver(page, engine, &fixture, ScreenEvent::FocusMoved { from, to, by });
    };
    point(&mut page, &mut engine, row3, By::Pointer);
    assert_eq!(page.scroll_target, rest2, "a hover moves focus, not the page");
    deliver(&mut page, &mut engine, &fixture, ScreenEvent::Input(nj_machine::machine::InputEvent {
        at: Tick::default(), source: nj_machine::machine::Source::Script,
        kind: InputKind::Click { x: 0.0, y: 0.0, hit: Some(row3.elem) } }));
    assert_eq!(page.scroll_target, page.target_layout.row_reveal(3), "a click brings the page to its card");
    point(&mut page, &mut engine, row4, By::Dir);
    assert_eq!(page.scroll_target, page.target_layout.row_reveal(4), "a key seats its row at rest");
}
