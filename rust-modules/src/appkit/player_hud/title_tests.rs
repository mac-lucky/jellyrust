//! The OSD header's strings, as the real draw lays them out, measured with the shipped advances.
use super::*;
use nj_machine::machine::Measure;
use crate::ui::draw_census::{capture_runs, TextRun};

fn runs(main: &str, secondary: &str, clock: Option<&str>, measure: &dyn Measure) -> Vec<TextRun> {
    capture_runs(|| draw_header(Painter::recording(), main, secondary, clock, measure))
}

/// Litefin's header: the name bold at x = the OSD's edge, the secondary run regular after it, and
/// the clock right-aligned on the right edge — on one line.
#[test]
fn the_header_is_name_then_secondary_then_the_clock() {
    let m = &nj_base::fontcov::advances::ShippedMeasure;
    let drawn = runs("Sintel", "(2010)", Some("21:45"), m);
    let text: Vec<&str> = drawn.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(text[..2], ["Sintel", "(2010)"]);
    assert_eq!(drawn[0].at.x, OSD_X);
    assert!(drawn[0].bold && !drawn[1].bold);
    assert_eq!((drawn[0].size, drawn[1].size), (TITLE_SZ, TITLE_SZ));
    assert!(drawn[1].at.x >= OSD_X + m.width_str("Sintel", TITLE_SZ, true) + TITLE_GAP - 0.5);
    assert_eq!(drawn[2].text, "21:45");
    assert!(drawn[2].size == CLOCK_SZ && !drawn[2].bold);
    assert_eq!(drawn[2].at.x, OSD_RIGHT, "right-aligned on the OSD's edge");
}

/// A long title elides before the clock (and never past Litefin's 60 % cap), and the secondary
/// run takes only what is left — in every shipped language's clock and title alike.
#[test]
fn long_titles_elide_before_the_clock() {
    let m = &nj_base::fontcov::advances::ShippedMeasure;
    let long = "An extraordinarily long episode — Über den Wolken — ".repeat(20);
    let drawn = runs(&long, &long, Some("23:59"), m);
    assert!(drawn[0].text.ends_with('…'));
    assert!(m.width_str(&drawn[0].text, TITLE_SZ, true) <= TITLE_MAX_W + 1.0);
    assert!(drawn[1].text.ends_with('…'));
    let clock_left = OSD_RIGHT - m.width_str("23:59", CLOCK_SZ, false);
    assert!(drawn[1].at.x + m.width_str(&drawn[1].text, TITLE_SZ, false) <= clock_left - theme::space::MD + 1.0,
        "the secondary run overlaps the clock");
}

/// The two runs Litefin composes (`_getFormattedTitle`): a film's year in brackets, an episode's
/// `- S01E03 - Name (Year)` after the show, and no stray separators when a part is missing.
#[test]
fn the_title_runs_follow_litefins_format() {
    let n = |is_real_episode, title: &str, ep_title: &str, season, index, year| crate::metadata::NowPlaying {
        is_episode: is_real_episode,
        is_real_episode,
        title: title.into(),
        ep_title: ep_title.into(),
        season,
        index,
        summary: String::new(),
        year,
        dur_ms: 0,
        rating: String::new(),
        thumb: String::new(),
        detail_rk: String::new(),
    };
    let fmt = |np: crate::metadata::NowPlaying| header_runs(Some(&np), "Sintel", "");
    assert_eq!(fmt(n(false, "Sintel", "", 0, 0, 2010)), ("Sintel".into(), "(2010)".into()));
    assert_eq!(header_runs(Some(&n(false, "Sintel", "", 0, 0, 0)), "Sintel", "Trailer"),
        ("Sintel".into(), "Trailer".into()), "no year: the context line");
    assert_eq!(fmt(n(true, "Caminandes", "Llamigos", 1, 3, 2016)),
        ("Caminandes".into(), "- S01E03 - Llamigos (2016)".into()));
    assert_eq!(fmt(n(true, "", "Llamigos", 1, 3, 0)), (String::new(), "S01E03 - Llamigos".into()));
    assert_eq!(header_runs(None, "Sintel", ""), ("Sintel".into(), String::new()), "before the metadata lands");
}
