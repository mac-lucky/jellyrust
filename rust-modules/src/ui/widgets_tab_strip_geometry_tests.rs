//! Strip pill geometry (season tabs, Filmography filter), the document rail, and the Litefin
//! rail's geometry, motion and clock.

use super::*;
#[allow(unused_imports)]
use super::test_support::*;

#[test]
fn measured_strip_reuses_shared_padding_gap_and_source_indices() {
    let measure = crate::ui::fixture::FixtureMeasure;
    let size = theme::size::BODY;
    let gap = 37.0;
    let lays = strip_layout_measured(
        ["A".into(), "bad\0label".into(), "Longer".into()].into_iter(),
        96.0, size, gap, &measure,
    );
    assert_eq!(lays.len(), 2);
    assert_eq!((lays[0].i, lays[1].i), (0, 2));
    assert_eq!(lays[0].x, 96.0);
    for lay in &lays {
        assert_eq!(lay.w, nj_machine::machine::Measure::width(&measure, &lay.label, size, true));
    }
    let first = strip_pill_rect(&lays[0], 10.0, 60.0);
    let second = strip_pill_rect(&lays[1], 10.0, 60.0);
    assert_eq!(second.x, first.x + first.w + gap);
}

#[test]
fn continuous_document_rail_tracks_both_ends_and_disappears_when_content_fits() {
    assert_eq!(
        continuous_rail_geom(0.0, 900.0, 300.0),
        Some((0.0, 1.0 / 3.0))
    );
    let (top, h) = continuous_rail_geom(600.0, 900.0, 300.0).unwrap();
    assert!((top + h - 1.0).abs() < 0.0001);
    assert_eq!(continuous_rail_geom(0.0, 300.0, 300.0), None);
}

/// **The rail is where Litefin puts it** (`docs/litefin-look.md` 3, 4.2), number for number: the
/// logo centred at (50, 60), a 72-tall item column centred between the header (y 120) and the
/// footer (968), the avatar item pinned 20 above the bottom, the label pill at x=92 and the
/// popover hanging 6 above its item.
#[test]
fn the_rail_sits_where_litefin_puts_it() {
    assert_eq!((RAIL_LOGO.cx(), RAIL_LOGO.cy(), RAIL_LOGO.w), (50.0, 60.0, 40.0));
    assert_eq!((RAIL_AVATAR.y, RAIL_AVATAR.h, RAIL_AVATAR.cx()), (988.0, 72.0, 50.0));
    // Litefin's six items start at y=328 (3); this rail's four at 400, same centre.
    assert_eq!(rail_item_rect(0, 6).y, 328.0);
    assert_eq!(rail_item_rect(0, 4).y, 400.0);
    assert_eq!(rail_item_rect(3, 4).y + 72.0 - 400.0, 4.0 * 72.0);
    for i in 0..4 {
        let r = rail_item_rect(i, 4);
        assert_eq!((r.x, r.w, r.h, r.cx()), (0.0, 100.0, 72.0, 50.0));
        assert_eq!(rail_slot_rect(i, 4).y, r.y);
    }
    assert_eq!(rail_slot_rect(4, 4).y, RAIL_AVATAR.y, "the slot past the items is the avatar");
    let item = rail_item_rect(1, 4);
    let label = rail_label_rect(item, 60.0);
    assert_eq!((label.x, label.w, label.cy()), (92.0, 60.0 + 36.0, item.cy()));
    assert!((label.h - 41.2).abs() < 1e-4, "8 + a 25.2 line + 8");
    assert_eq!(rail_label_rect(item, 900.0).w, 300.0, "the pill caps at 300");
    assert_eq!(RAIL_LABEL_TEXT_MAX + 36.0, 300.0, "an elided label fills the capped pill");
    let pop = rail_popover_rect(item, 3);
    assert_eq!((pop.x, pop.y, pop.w, pop.h), (92.0, item.y - 6.0, 270.0, 16.0 + 3.0 * 48.0));
    let row = rail_row_rect(pop, 2);
    assert_eq!((row.x, row.y, row.w, row.h), (100.0, pop.y + 8.0 + 96.0, 254.0, 48.0));
    assert_eq!(RAIL_ROW_TEXT_MAX, 254.0 - 54.0 - 14.0);
    // a long list stays on screen: 20 from the bottom, never above 10 from the top
    let low = rail_popover_rect(rail_item_rect(3, 4), 12);
    assert_eq!(low.y + low.h, crate::ui::consts::SCR_H - 20.0);
    assert_eq!(rail_popover_rect(rail_item_rect(0, 4), 40).y, 10.0);
}

#[test]
fn the_clock_reads_24_hours_zero_padded() {
    assert_eq!(clock_text(0), "00:00");
    assert_eq!(clock_text(9 * 60 + 5), "09:05");
    assert_eq!(clock_text(23 * 60 + 59), "23:59");
    assert_eq!(clock_text(24 * 60 + 1), "00:01", "a day wraps");
}

/// **The rail animates while focus moves and asks for nothing at rest** (the two tests
/// `ui/CLAUDE.md` owes anything that animates): the squircle pop, the label pill and the popover's
/// opening all ride its springs, so they report through `nj_machine::idle` exactly while one is
/// travelling.
#[test]
fn the_rail_reports_motion_while_focus_travels() {
    let _serial = serial_for_motion();
    let mut rail = RailRender::new();
    let at = |slot| RailFocus { slot: Some(slot), row: None };
    let ((), moving) = nj_machine::idle::scoped_motion(|| rail.update(5, 0, at(1), DT));
    assert!(moving, "focus arriving on an item pops it");
    for _ in 0..120 { rail.update(5, 0, at(1), DT); }
    assert!((rail.slot_focus(1) - 1.0).abs() < 1e-3);
    let ((), moving) = nj_machine::idle::scoped_motion(|| rail.update(5, 0, at(2), DT));
    assert!(moving, "focus moving down the rail animates both items");
    let ((), moving) = nj_machine::idle::scoped_motion(|| rail.update(5, 3, RailFocus { slot: None, row: Some(0) }, DT));
    assert!(moving, "the popover opens");
    assert!(rail.open_amount() > 0.0 && rail.open_amount() < 1.0);
    // the pop overshoots on the way in, as Litefin's 250 ms spring curve does
    let mut rail = RailRender::new();
    let mut peak = 0.0f32;
    for _ in 0..60 { rail.update(5, 0, at(0), DT); peak = peak.max(rail.slot_focus(0)); }
    assert!(peak > 1.04 && peak < 1.2, "the focus pop overshoots by about a tenth, peaked at {peak}");
}

#[test]
fn a_settled_rail_asks_for_nothing() {
    let _serial = serial_for_motion();
    let mut rail = RailRender::new();
    let focus = RailFocus { slot: None, row: Some(1) };
    for _ in 0..240 { rail.update(5, 3, focus, DT); }
    let ((), moving) = nj_machine::idle::scoped_motion(|| rail.update(5, 3, focus, DT));
    assert!(!moving, "a settled rail with its popover open is silent");
    let mut idle = RailRender::new();
    for _ in 0..3 { idle.update(5, 0, RailFocus::default(), DT); }
    let ((), moving) = nj_machine::idle::scoped_motion(|| idle.update(5, 0, RailFocus::default(), DT));
    assert!(!moving, "a rail nobody is on never moves");
}
