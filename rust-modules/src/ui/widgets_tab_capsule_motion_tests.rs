//! The tab strip's travelling selection/focus capsules: canonical motion hashing, arrival and ink.

use super::*;
#[allow(unused_imports)]
use super::test_support::*;

// ---- the travelling capsules --------------------------------------------------------------
#[test]
fn tab_strip_canonical_motion_covers_both_capsules_and_each_spring() {
    let hash = |strip: &TabStrip| {
        let mut c = nj_machine::machine::Canon::new();
        strip.write_motion(&mut c);
        c.finish()
    };
    let initial = hash(&TabStrip::new());
    for focused in [false, true] {
        for field in 0..7 {
            let mut strip = TabStrip::new();
            let capsule = if focused { &mut strip.foc } else { &mut strip.sel };
            match field {
                0 => capsule.x.pos = 1.0,
                1 => capsule.x.vel = 1.0,
                2 => capsule.w.pos = 1.0,
                3 => capsule.w.vel = 1.0,
                4 => capsule.a.pos = 1.0,
                5 => capsule.a.vel = 1.0,
                _ => capsule.at = 0,
            }
            assert_ne!(initial, hash(&strip), "capsule={focused} field={field}");
        }
    }
}

/// The one rule both the fill and the ink read, at every edge it has.
#[test]
fn cap_cover_is_the_pills_own_share_of_what_is_over_it() {
    let pill = (100.0, 200.0);
    assert_eq!(
        cap_cover(pill, pill),
        1.0,
        "a capsule sitting exactly on the pill covers it"
    );
    assert_eq!(
        cap_cover(pill, (0.0, 100.0)),
        0.0,
        "abutting on the left is not covering"
    );
    assert_eq!(cap_cover(pill, (300.0, 100.0)), 0.0, "…nor on the right");
    assert_eq!(
        cap_cover(pill, (0.0, 200.0)),
        0.5,
        "half over the left edge is half the pill"
    );
    assert_eq!(
        cap_cover(pill, (200.0, 400.0)),
        0.5,
        "…and half over the right edge likewise"
    );
    assert_eq!(
        cap_cover(pill, (-500.0, 5000.0)),
        1.0,
        "a capsule wider than the pill still covers 1, not more"
    );
    let z = cap_cover((100.0, 0.0), pill);
    assert_eq!(z, 0.0, "a zero-width pill covers nothing");
    assert!(
        z.is_finite(),
        "…and does not divide by zero into a NaN that would poison every ink"
    );
}

/// A fixture in the shape the two tests below need: seeded, in one material, with the OTHER

/// The regression that would otherwise draw a bright capsule sweeping the whole strip on the
/// first Library frame: an UNPLACED capsule lands on its pill, it does not fly to it. Alpha still
/// fades in, because arriving in a row is a fade — the two are deliberately different motions.
#[test]
fn a_capsule_lands_on_its_first_pill_instead_of_flying_in_from_the_origin() {
    let _g = serial_for_motion();
    let w = widths_for(12);
    let p7 = span_of(&w, 7);
    let mut c = Capsule::new();
    c.step(Some((7, p7)), false, DT);
    assert_eq!(
        cap_cover(p7, c.span()),
        1.0,
        "the first frame must already be ON pill 7"
    );
    assert!(
        c.alpha() > 0.0 && c.alpha() < 1.0,
        "…while its alpha is still ramping up ({})",
        c.alpha()
    );
}

/// A capsule mid-travel must always be sitting on SOMETHING. The label ink is derived from
/// coverage, so a frame where neither neighbour is covered is a frame where both labels read as
/// plain while a bright capsule floats in the gutter between them.
#[test]
fn a_travelling_capsule_is_never_sitting_on_nothing() {
    let _g = serial_for_motion();
    let w = widths_for(12);
    let (p0, p1) = (span_of(&w, 0), span_of(&w, 1));
    let mut c = Capsule::new();
    for _ in 0..60 {
        c.step(Some((0, p0)), false, DT);
    }
    assert!(
        cap_cover(p0, c.span()) > 0.99,
        "the fixture must start settled on pill 0"
    );

    c.step(Some((1, p1)), false, DT);
    assert!(
        cap_cover(p0, c.span()) > 0.5,
        "one frame in it must still be mostly on pill 0, not teleported"
    );
    for f in 0..60 {
        c.step(Some((1, p1)), false, DT);
        let on = cap_cover(p0, c.span()).max(cap_cover(p1, c.span()));
        assert!(
            on > 0.0,
            "frame {f}: the capsule is on neither pill (span {:?})",
            c.span()
        );
    }
    assert!(
        cap_cover(p1, c.span()) > 0.99,
        "it must arrive fully on pill 1"
    );
    assert!(cap_cover(p0, c.span()) < 0.01, "…and leave pill 0 behind");
}

/// Focus leaving the row fades the capsule where it stands rather than parking it at the origin,
/// and coming back LANDS — the "capsule streaks across the row when you come back from the grid"
/// failure, which is the whole reason [`CAP_LAND_A`] exists.
#[test]
fn focus_leaving_the_row_fades_in_place_and_coming_back_lands() {
    let _g = serial_for_motion();
    let w = widths_for(12);
    let (p2, p9) = (span_of(&w, 2), span_of(&w, 9));
    let mut c = Capsule::new();
    for _ in 0..60 {
        c.step(Some((2, p2)), false, DT);
    }
    let held = c.span();
    for _ in 0..40 {
        c.step(None, false, DT);
    }
    assert!(
        c.alpha() < CAP_LAND_A,
        "focus off the row must fade the capsule out ({})",
        c.alpha()
    );
    assert_eq!(
        c.span(),
        held,
        "…in place: an invisible capsule must not also drift"
    );

    c.step(Some((9, p9)), false, DT);
    assert_eq!(
        cap_cover(p9, c.span()),
        1.0,
        "returning focus to a FAR pill lands on it, never glides"
    );
}

/// The two-capsule model, locked against a regression to one: on the Library screen the selected
/// tab is the section you are browsing while focus walks the row, so a pill can be wearing either,
/// both, or neither — and the mixes it is inked with are exactly that answer.
#[test]
fn the_ink_a_pill_wears_is_exactly_the_capsule_over_it() {
    let _g = serial_for_motion();
    let w = widths_for(12);
    let settle = |sel: c_int, foc: c_int| {
        let mut s = TabStrip::new();
        for _ in 0..90 {
            s.update(
                sel,
                foc,
                |i| (i < w.len()).then(|| span_of(&w, i)),
                SelMark::Travels,
                DT,
            );
        }
        s
    };
    let s = settle(2, 5);
    let near =
        |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3;
    assert!(
        near(s.mixes(span_of(&w, 2)), (0.0, 1.0)),
        "the selected pill wears only the selection"
    );
    assert!(
        near(s.mixes(span_of(&w, 5)), (1.0, 0.0)),
        "the focused pill wears only the focus"
    );
    assert!(
        near(s.mixes(span_of(&w, 0)), (0.0, 0.0)),
        "an idle pill wears neither"
    );
    let both = settle(3, 3);
    assert!(
        near(both.mixes(span_of(&w, 3)), (1.0, 1.0)),
        "one pill can wear both at once"
    );
    // …and nothing is marked at all when there is nothing to mark (an emptied season strip, or
    // focus off the row on a page that has no selection either).
    let none = settle(-1, -1);
    assert!(
        near(none.mixes(span_of(&w, 0)), (0.0, 0.0)),
        "no target means no ink anywhere"
    );
}

/// Ink CONSERVATION across a travel: the two capsules only ever have one pill's worth of coverage
/// between them, so no frame can ink two labels toward `ACCENT_INK` at once. This is what bounds
/// the one transient the design accepts — a partly covered pill inks its WHOLE label — to half
/// a label on each of two pills for the length of one travel, rather than a whole row dimming.
#[test]
fn a_travel_never_inks_more_than_one_pills_worth_of_label() {
    let _g = serial_for_motion();
    let w = widths_for(12);
    let mut s = TabStrip::new();
    let span = |i: usize| (i < w.len()).then(|| span_of(&w, i));
    for _ in 0..90 {
        s.update(-1, 4, span, SelMark::Travels, DT);
    }
    for f in 0..90 {
        s.update(-1, 5, span, SelMark::Travels, DT);
        let total: f32 = (0..w.len()).map(|i| s.mixes(span_of(&w, i)).0).sum();
        assert!(
            total <= 1.0 + 1e-3,
            "frame {f}: {total} pills' worth of focus ink is lit at once"
        );
        assert!(
            total > 0.0,
            "frame {f}: the focus ink went out entirely mid-travel"
        );
    }
}

/// **The season strip's selection LANDS; every other strip's travels.** Both halves, because the
/// rule is a difference between two callers and a regression would be one call site copied onto
/// the other.
///
/// The owner's report (2026-08-22) was that a grey pill sliding across the season row "does not
/// fit". It is graded on the capsule's POSITION one frame after the selection moves: a travelling
/// capsule is still near where it started, a landing one is already there.
#[test]
fn the_selection_mark_lands_on_a_season_strip_and_travels_on_a_tab_bar() {
    const DT: f32 = 1.0 / 60.0;
    let w = [140.0f32, 160.0, 150.0, 170.0];
    let span_of =
        |i: usize| -> (f32, f32) { (w.iter().take(i).map(|v| v + 20.0).sum::<f32>(), w[i]) };
    let span = |i: usize| (i < w.len()).then(|| span_of(i));
    let settle = |s: &mut TabStrip, sel: c_int, mark: SelMark| {
        for _ in 0..120 {
            s.update(sel, -1, span, mark, DT);
        }
    };
    let (far, near) = (span_of(3).0, span_of(0).0);

    // LANDS — one frame after the selection moves it is already at the new pill
    let mut season = TabStrip::new();
    settle(&mut season, 0, SelMark::Lands);
    assert!(
        (season.sel.span().0 - near).abs() < 0.5,
        "settled on pill 0"
    );
    season.update(3, -1, span, SelMark::Lands, DT);
    assert!(
        (season.sel.span().0 - far).abs() < 0.5,
        "a landing mark is AT the new pill on the very next frame, not on its way: {}",
        season.sel.span().0
    );

    // TRAVELS — one frame later it has barely left, which is the whole point of the other strip
    let mut bar = TabStrip::new();
    settle(&mut bar, 0, SelMark::Travels);
    bar.update(3, -1, span, SelMark::Travels, DT);
    let moved = bar.sel.span().0;
    assert!(
        moved > near && moved < near + (far - near) * 0.5,
        "a travelling mark is between the two after one frame: {moved}"
    );
}

/// Every RESTING state a strip-driven pill can be in must be the look it replaced, to the bit —
/// this is the whole reason the mix lerps between the same ink roles the boolean arms pick from
/// rather than inventing a ramp of its own.
///
/// **The IDLE role is the one deliberate exception, and it is a fact about grounds rather than
/// about pills.** The strip-driven row is the standing tab track: it sits on glass over
/// uncontrolled artwork, and its scrim is solved so this ink clears [`TRACK_INK_CONTRAST`] — a
/// muted `TEXT_TERTIARY` there costs .61 of black over a bright hero, which is a band laid across
/// the picture rather than a material. The boolean arms are the detail page's season tabs, drawn
/// bare on a controlled dark ground where nothing is solved and the muted ink is right. Two
/// grounds, two answers; the roles they SHARE still have to agree to the bit.
#[test]
fn a_settled_mixed_pill_is_the_boolean_look_it_replaced() {
    // "to the bit" means to the PANEL's bit: a lerp that lands on its endpoint still carries a
    // ~3e-8 float residue, and the framebuffer is 8 bits per channel.
    let is = |got: [f32; 4], want: [f32; 4], what: &str| {
        for i in 0..4 {
            assert!(
                (got[i] - want[i]).abs() < 0.5 / 255.0,
                "{what}: channel {i} is {} not {} — over half a display code out",
                got[i],
                want[i]
            );
        }
    };
    is(
        TabPill::mixed_ink(0.0, 0.0),
        theme::TEXT_READING,
        "plain segment on glass",
    );
    assert_ne!(
        theme::TEXT_READING,
        theme::TEXT_TERTIARY,
        "the fixture is meaningless if the two idle inks have converged"
    );
    is(
        TabPill::mixed_ink(0.0, 1.0),
        theme::TEXT_PRIMARY,
        "selected, focus elsewhere",
    );
    is(
        TabPill::mixed_ink(1.0, 0.0),
        crate::ui::ACCENT_INK,
        "focused",
    );
    is(
        TabPill::mixed_ink(1.0, 1.0),
        crate::ui::ACCENT_INK,
        "focus outranks selection",
    );
    // out-of-range mixes clamp rather than extrapolating past the tokens
    is(
        TabPill::mixed_ink(-1.0, 4.0),
        theme::TEXT_PRIMARY,
        "an over-range selection clamps",
    );
    is(
        TabPill::mixed_ink(9.0, -9.0),
        crate::ui::ACCENT_INK,
        "an over-range focus clamps",
    );
}

/// **The plated composite**, which is arithmetic and therefore has no business being graded by
/// eye. A season tab's ground is now TWO layers — the pill's own idle plate and the strip's
/// travelling selection capsule over it — where it used to be one flat token. Same-colour
/// source-over is `a + b − ab`, so the pair must land back on [`theme::TAB_PLATE_SELECTED`]; and
/// the plate must retire exactly as the OPAQUE focus capsule arrives, or a focused season would
/// wear a 0.08 white film its unplated twin in the top row does not.
#[test]
fn a_travelling_plate_lands_back_on_the_plate_it_replaced() {
    let idle = theme::TAB_PLATE_IDLE[3];
    let over = theme::TAB_PLATE_SELECTED_OVER[3];
    let composite = over + idle * (1.0 - over);
    assert!(
        (composite - theme::TAB_PLATE_SELECTED[3]).abs() < 1.0 / 255.0,
        "capsule .{over} over plate .{idle} composites to {composite}, not TAB_PLATE_SELECTED's {}",
        theme::TAB_PLATE_SELECTED[3]
    );
    // order-independence is what lets the pill keep painting its plate AFTER the strip drew the
    // capsule under it — the draw order this component actually uses
    assert!(
        ((idle + over * (1.0 - idle)) - composite).abs() < 1e-6,
        "same-colour source-over must be order-independent, or the draw order becomes load-bearing"
    );
    // all three plate tokens are the same white; only their weight differs
    for i in 0..3 {
        assert_eq!(
            theme::TAB_PLATE_SELECTED_OVER[i],
            theme::TAB_PLATE_IDLE[i],
            "channel {i}"
        );
        assert_eq!(
            theme::TAB_PLATE_SELECTED_OVER[i],
            theme::TAB_PLATE_SELECTED[i],
            "channel {i}"
        );
    }
    // …and the split survives the `Painter` cascade, which scales BOTH layers: two layers under
    // a cascade are not the same function as one, so the drift is bounded here rather than
    // assumed. (Nothing draws a tab strip at α<1 today; this is the guard for the day one does.)
    for &a in &[1.0f32, 0.75, 0.5, 0.25] {
        let one = a * theme::TAB_PLATE_SELECTED[3];
        let two = a * over + a * idle * (1.0 - a * over);
        assert!(
            (one - two).abs() < 1.0 / 255.0,
            "at cascade α={a} the two-layer plate is {two} against the old {one} — over a display code apart"
        );
    }
}
