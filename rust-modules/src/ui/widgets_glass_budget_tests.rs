//! The live-glass policy: no glass on the shared chrome (Litefin's black theme has none,
//! `docs/litefin-look.md` 11.2), glass only where the frame mechanism and the dev tools put it, and
//! the dynamic glass refresh cadence.

use super::*;
#[allow(unused_imports)]
use super::test_support::*;

/// The shipped default, asserted in ONE place so moving it is a one-line decision with a
/// measurement behind it rather than a cascade through the cadence tests.
#[test]
fn the_shipped_cadence_is_every_changed_present() {
    use crate::ui::frame::backdrop::{decide,Request,Damage,Z,canvas};
    assert!(decide(Request {z:Z::CHROME,rect:canvas(),valid:true}, &[],
        &[Damage {z:Z::PAGE,rect:canvas()}]).refresh);
    assert!(!decide(Request {z:Z::CHROME,rect:canvas(),valid:true}, &[], &[]).refresh);
}

/// **Every glass policy COMPOSITES**, and the axis that let one not to is gone.
///
/// This test used to assert the source-dim arithmetic — `Glass::DYNAMIC.source_rgb(0.5) == 0.75`
/// and so on — for a preset nothing outside this module ever constructed. The mechanism was
/// measured against the item menu over one checker ground and rejected (`e75b5e49`): dimming the
/// page going INTO the backdrop destroys the modulation the frost is layered over, so the panel
/// arrives flat. What is worth pinning now is that no policy can reintroduce it by accident —
/// a material carries no private scheduling policy.
#[test]
fn live_glass_carries_no_private_source_policy() {
    // Scheduling is owned by the layer walk, never an independently phased widget clock.
    assert_eq!(
        std::mem::size_of::<Glass>(),
        0,
        "a second axis on Glass would show up here first"
    );
}

/// **No popover panel frosts a backdrop blur.** Every panel stands on the latched underlay field
/// through `panel_ground` (directly, or through `Popover::panel`), and the blur chain's users are
/// the dev tile band in `widgets.rs`, the frame mechanism and the dev load dial.
///
/// A source grep, because the failure it guards is a call that COMPILES: `Glass` and
/// `Painter::backdrop_blur` stay reachable for the chrome, so a panel that reached for them again
/// would draw, look plausible on a still page, and put the ~11% of a frame's GPU the migration
/// removed straight back.
#[test]
fn no_popover_panel_uses_the_blur_path_and_every_one_stands_on_the_field() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // The blur path's legitimate users: the dev tile band, the frame plan, the dev dial, the API
    // itself, and a fixture that names it in a string.
    const CHROME: &[&str] = &[
        "ui/widgets.rs",
        "ui/frame/glass.rs",
        "ui/frame/backdrop_tests.rs",
        "ui/glassload.rs",
        "ui/mod.rs",
        "ui/fixture.rs",
        "ui/widgets_glass_budget_tests.rs",
    ];
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).expect("read src dir") {
            let p = e.expect("dir entry").path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(&root.join("ui"), &mut files);
    walk(&root.join("appkit"), &mut files);
    walk(&root.join("screens"), &mut files);
    let mut offenders = Vec::new();
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        if CHROME.contains(&rel.as_str()) {
            continue;
        }
        let src = std::fs::read_to_string(f).expect("read source");
        for (n, line) in src.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if ["Glass::", "GlassState", "backdrop_blur(", ".backdrop("]
                .iter()
                .any(|pat| code.contains(pat))
            {
                offenders.push(format!("{rel}:{}: {code}", n + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the blur path is the frame mechanism's — a panel's ground is `widgets::panel_ground`:\n{}",
        offenders.join("\n")
    );

    // …and every panel that used to frost one now stands on the field.
    for rel in [
        "screens/item_menu.rs",
        "screens/account_menu.rs",
        "screens/alt_sources.rs",
        "screens/about_panel.rs",
        "screens/tracks_panel.rs",
        "screens/library/menu.rs",
        "screens/person_bio.rs",
        "ui/decision_alert.rs",
    ] {
        let src = std::fs::read_to_string(root.join(rel)).expect("read panel source");
        assert!(
            src.contains("panel_ground(") || src.contains("pop.panel("),
            "{rel} must draw its ground through `panel_ground` (or `Popover::panel`)"
        );
    }
}

/// **The shared chrome wears no glass.** Litefin's rail is transparent over the page and its clock
/// is bare text (`docs/litefin-look.md` 3, 4.2, 11.2), so the rail's region of `widgets.rs` asks
/// for no backdrop at all: a source grep, because a `Glass::DYNAMIC_BACKDROP` call there would
/// compile, draw and quietly bring back a per-frame blur the page under it pays for.
#[test]
fn the_rail_and_the_clock_ask_for_no_backdrop() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/widgets.rs"),
    ).expect("read widgets.rs");
    let start = src.find("// ---- The Litefin rail").expect("the rail's region");
    let end = start + src[start..].find("/// Which colour treatment a control").expect("the region's end");
    let rail = &src[start..end];
    assert!(rail.contains("fn draw_rail_item") && rail.contains("fn draw_clock"), "the region is the rail's");
    for pat in ["Glass::", "backdrop_blur(", ".backdrop(", "GlassFace"] {
        assert!(!rail.contains(pat), "the rail draws flat on the page; found {pat}");
    }
}
