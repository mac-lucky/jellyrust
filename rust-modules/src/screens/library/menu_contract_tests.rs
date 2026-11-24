fn menu_arg(kind: LibraryMenuKind, anchor: [u32; 4]) -> LibraryMenuArg {
    LibraryMenuArg {
        host: nj_machine::machine::InstanceId(8),
        target: SectionAddress {
            epoch: 11,
            sid: ServerId::from_raw(1),
            section: 7,
        },
        kind,
        anchor,
    }
}

fn anchor_bits(rect: Rect) -> [u32; 3] {
    [rect.x.to_bits(), rect.y.to_bits(), rect.w.to_bits()]
}

fn menu_cx<'a>(
    listing: &'a crate::stores::browse::ListingSnapshot,
    directory: &'a crate::stores::browse::DirectorySnapshot,
    hubs: &'a crate::stores::browse::HubsSnapshot,
    measure: &'a FixtureMeasure,
    tick: Tick,
) -> Cx<'a, HostFixture> {
    Cx {
        views: Views {
            listing: listing.view(),
            directory: directory.view(),
            hubs: hubs.view(),
        },
        tick,
        measure,
        focus: FocusRead { current: None, ..Default::default() },
        press: PressRead::default(),
        owner: InputOwner::Entry(EntryId(7)),
    }
}

#[test]
fn menu_anchor_is_frozen_and_a_new_open_uses_the_new_anchor() {
    let _guard = nj_base::testlock::serial();
    let session = crate::catalog::session::TempSession::new("menu-anchor-contract");
    session.watching("u-menu-anchor-contract");
    let stores = crate::stores::Stores::default();
    stores.browse.borrow_mut().seed_two_source_table_for_test();
    stores.browse_run(crate::stores::browse::BrowseCmd::SetCur(0));
    let measure = FixtureMeasure;
    let mut first = crate::stores::browse::DirectorySnapshot::default();
    let publication = stores.capture_browse(&mut first);
    let listing = publication.listing;
    let hubs = publication.section_hubs;
    let old_anchor = [100.0f32.to_bits(), 400.0f32.to_bits(), 220.0f32.to_bits(), 60.0f32.to_bits()];
    let new_anchor = [760.0f32.to_bits(), 260.0f32.to_bits(), 240.0f32.to_bits(), 60.0f32.to_bits()];
    let mut old = LibraryMenu::new(EntryId(7), menu_arg(LibraryMenuKind::Sort, old_anchor));
    let mut output = Vec::new();
    let mut present = nj_machine::present::Present::new();
    let cx_first = menu_cx(&listing, &first, &hubs, &measure, Tick { ms: 1, dt_us: 16_000 });
    old.step(
        &ScreenEvent::Tick(Tick { ms: 1, dt_us: 16_000 }),
        &cx_first,
        &mut Effects::new(&mut output, MachineId::Instance(nj_machine::machine::InstanceId(7)), &mut present),
    );

    let mut old_groups = Vec::new();
    old.groups(&cx_first, &mut old_groups);
    let old_extent = old_groups[0].extent;

    {
        let mut browse = stores.browse.borrow_mut();
        browse.append_section_for_test(1, 3, "New Films", SecKind::Movie);
        browse.set_pinned_for_test(4, true);
    }
    let mut changed = crate::stores::browse::DirectorySnapshot::default();
    stores.capture_browse(&mut changed);
    let cx_changed = menu_cx(&listing, &changed, &hubs, &measure, Tick { ms: 2, dt_us: 16_000 });
    old.step(
        &ScreenEvent::Tick(Tick { ms: 2, dt_us: 16_000 }),
        &cx_changed,
        &mut Effects::new(&mut output, MachineId::Instance(nj_machine::machine::InstanceId(7)), &mut present),
    );
    let mut old_after_host_move = Vec::new();
    old.groups(&cx_changed, &mut old_after_host_move);
    assert_eq!(anchor_bits(old_after_host_move[0].extent), anchor_bits(old_extent), "an open menu stays on its frozen host anchor");

    let mut fresh = LibraryMenu::new(EntryId(8), menu_arg(LibraryMenuKind::Sort, new_anchor));
    fresh.step(
        &ScreenEvent::Tick(Tick { ms: 3, dt_us: 16_000 }),
        &cx_changed,
        &mut Effects::new(&mut output, MachineId::Instance(nj_machine::machine::InstanceId(8)), &mut present),
    );
    let mut fresh_groups = Vec::new();
    fresh.groups(&cx_changed, &mut fresh_groups);
    assert_ne!(anchor_bits(fresh_groups[0].extent), anchor_bits(old_extent), "a new open releases the old anchor");
    assert_eq!(fresh_groups[0].extent.x, 760.0);
}

#[test]
fn menu_side_actions_keep_sort_and_filter_row_identity() {
    let _guard = nj_base::testlock::serial();
    let mut sort = LibraryMenu::new(EntryId(7), menu_arg(LibraryMenuKind::Sort, [0; 4]));
    let sorts = vec![
        SortEntry { key: "titleSort".into(), desc_key: String::new(), title: "Title".into(), default_desc: false },
        SortEntry { key: "addedAt".into(), desc_key: String::new(), title: "Added".into(), default_desc: true },
    ];
    sort.apply_draft(sort_draft(&sorts, 0, false));
    let sort_key = sort.form.key_at(1).unwrap().0;
    let mut filter = LibraryMenu::new(EntryId(7), menu_arg(LibraryMenuKind::Filter, [0; 4]));
    filter.apply_draft(filter_draft(false, None, true));
    let filter_key = filter.form.key_at(0).unwrap().0;

    with_cx(|cx| {
        assert!(matches!(sort.form.index_of_key(RowKey(sort_key)).and_then(|i| sort.form.binding_at(i)).map(|b| &b.action), Some(Action::Edit(QueryEdit::Sort { key, desc: true })) if key == "addedAt"));
        assert!(matches!(filter.form.index_of_key(RowKey(filter_key)).and_then(|i| filter.form.binding_at(i)).map(|b| &b.action), Some(Action::Edit(QueryEdit::Unwatched(true)))));
        assert!(<LibraryMenu as Focusable<HostFixture>>::place(&sort, &sort_key, cx, At::Drawn).is_some());
        assert!(<LibraryMenu as Focusable<HostFixture>>::place(&filter, &filter_key, cx, At::Drawn).is_some());

        let mut output = Vec::new();
        let mut present = nj_machine::present::Present::new();
        sort.step(
            &ScreenEvent::Activate(sort_key),
            cx,
            &mut Effects::new(&mut output, MachineId::Instance(nj_machine::machine::InstanceId(7)), &mut present),
        );
        assert!(output.iter().any(|effect| matches!(
            &effect.fx,
            Fx::Deliver(_, Delivery::Screen(ScreenEvent::App(AppMsg::LibraryEdit {
                edit: QueryEdit::Sort { key, desc: true }, ..
            }))) if key == "addedAt"
        )));
        assert!(output.iter().any(|effect| matches!(
            &effect.fx,
            Fx::Nav(NavOp::Dismiss(EntryId(7)))
        )));

        output.clear();
        filter.step(
            &ScreenEvent::Activate(filter_key),
            cx,
            &mut Effects::new(&mut output, MachineId::Instance(nj_machine::machine::InstanceId(7)), &mut present),
        );
        assert!(output.iter().any(|effect| matches!(
            &effect.fx,
            Fx::Deliver(_, Delivery::Screen(ScreenEvent::App(AppMsg::LibraryEdit {
                edit: QueryEdit::Unwatched(true), ..
            })))
        )));
        assert!(!output
            .iter()
            .any(|effect| matches!(effect.fx, Fx::Nav(NavOp::Dismiss(_)))));
    });
}
