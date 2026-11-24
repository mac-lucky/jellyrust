//! Hero pool ordering and rotation across borrowed and owned pages.

use super::*;
#[allow(unused_imports)]
use super::test_support::*;

/// The rule: one page moves to the front, nothing is dropped, and nothing else is reordered.
#[test]
fn a_borrowed_page_never_opens_the_door_while_we_have_one_of_our_own() {
    let mut p = pool_of(&["friend", "friend", "", "", "friend"]);
    own_items_first(&mut p);
    assert_eq!(sources_of(&p), ["", "friend", "friend", "", "friend"]);
    assert_eq!(
        p.iter().map(|s| s.idx).collect::<Vec<_>>(),
        [2, 0, 1, 3, 4],
        "the pages themselves survive"
    );
}

#[test]
fn a_pool_that_already_opens_on_one_of_ours_is_left_exactly_alone() {
    for start in [
        vec!["", "friend", "", "friend"],
        vec!["", "", ""],
        vec!["", "friend"],
    ] {
        let mut p = pool_of(&start);
        own_items_first(&mut p);
        assert_eq!(
            sources_of(&p),
            start,
            "an ordering that already holds must not be re-derived"
        );
    }
}

/// Filtering instead of ordering would leave this account with NO hero at all.
#[test]
fn a_borrowed_only_account_still_gets_a_hero_and_it_holds_the_first_rotation() {
    let mut p = pool_of(&["friend", "friend2"]);
    own_items_first(&mut p);
    assert_eq!(
        sources_of(&p),
        ["friend", "friend2"],
        "nothing of ours to promote, so nothing moves"
    );
}

#[test]
fn the_ordering_holds_at_the_empty_and_single_page_ends() {
    let mut empty: Vec<HeroSlot> = Vec::new();
    own_items_first(&mut empty);
    assert!(empty.is_empty());
    for one in [vec![""], vec!["friend"]] {
        let mut p = pool_of(&one);
        own_items_first(&mut p);
        assert_eq!(sources_of(&p), one);
    }
}

/// The hero pool, on a MERGED deck. Two things it can only get right by knowing which source
/// each ROW came from: our own item opens the rotation, and two servers' identical ratingKeys
/// are two different films. The deck's own `source` is empty by design, so a slot that read its
/// handle from its shelf would attribute every borrowed film to nobody — and `own_items_first`
/// would think one of ours had already opened the door.
#[test]
fn the_hero_pool_opens_on_our_own_item_and_never_dedups_across_servers() {
    let _g = nj_base::testlock::serial();
    let mut o = Owner::default();
    reset(&mut o.state, &o.adapter);
    seed(&mut o.state, vec![
        src(
            0,
            "",
            HubState::Ready,
            Some(built(0, &[(100, "1")], vec![])),
        ),
        src(
            1,
            "friend",
            HubState::Ready,
            Some(built(1, &[(900, "1")], vec![])),
        ),
    ]);
    assert_eq!(
        rks(&o.state, 0),
        ["1", "1"],
        "the deck orders them by recency: theirs first"
    );
    assert_eq!(hero_pool_len(&o.state), 2, "one ratingKey, two servers, two films");
    assert_eq!(hero_pool_source(&o.state, 0), "", "the door opens on our own library");
    assert_eq!(
        hero_pool_source(&o.state, 1),
        "friend",
        "…and the borrowed film rotates in behind it, attributed"
    );
    assert!(
        std::ptr::eq(hero_pool_item(&o.state, 0).unwrap(), movie(&o.state, 1).unwrap()),
        "catalog row 1 is ours (row 0 is theirs, watched later)"
    );
    reset(&mut o.state, &o.adapter);
}

/// **Litefin's Home** (`docs/litefin-look.md` 4.2): My Media first, then Continue Watching, then the
/// source's own rows in its order (Next Up, then one Latest row per library); a source's hero shelf
/// is no row at all but the pool itself, its cards in the catalog for the hero to name.
#[test]
fn my_media_leads_the_deck_and_a_sources_hero_shelf_is_the_pool() {
    let _g = nj_base::testlock::serial();
    let mut o = Owner::default();
    reset(&mut o.state, &o.adapter);
    let mut no_art = shelf(0, "", HUB_HERO, &["h3"]);
    no_art.items[0].art.clear();
    let mut hero = shelf(0, "", HUB_HERO, &["h1", "h2"]);
    hero.items.extend(no_art.items);
    seed(&mut o.state, vec![src(0, "", HubState::Ready, Some(built(0, &[(5, "cw")], vec![
        shelf(0, "Next Up", HUB_NEXT_UP, &["next"]),
        shelf(0, "My Media", HUB_MY_MEDIA, &["films", "series"]),
        shelf(0, "Films: recently added", "movie.recentlyadded.1", &["new"]),
        hero,
    ])))]);
    let titles: Vec<&str> = (0..hub_count(&o.state)).map(|h| hub_title(&o.state, h)).collect();
    assert_eq!(titles, ["My Media", nj_platform::i18n::msg::browse_home_continue_watching(), "Next Up", "Films: recently added"]);
    assert!(hub_is_continue(&o.state, 1), "the deck is still the deck behind My Media");
    assert_eq!(rks(&o.state, 0), ["films", "series"]);
    assert_eq!(hero_pool_len(&o.state), 2, "the hero shelf, less its card without a backdrop");
    assert_eq!(hero_pool_item(&o.state, 0).unwrap().rk, "h1");
    assert_eq!(hero_pool_item(&o.state, 1).unwrap().rk, "h2");

    // without a hero shelf the pool is drawn from the deck and the recent rows, as before
    seed(&mut o.state, vec![src(0, "", HubState::Ready, Some(built(0, &[(5, "cw")], vec![
        shelf(0, "Films: recently added", "movie.recentlyadded.1", &["new"]),
    ])))]);
    assert_eq!(hero_pool_len(&o.state), 2);
    assert_eq!(hero_pool_item(&o.state, 0).unwrap().rk, "cw");
    reset(&mut o.state, &o.adapter);
}

/// A Jellyfin source projects in Litefin's order: Next Up first among its shelves (the merge puts
/// the deck and My Media ahead of it), the libraries as My Media tiles, Latest rows headed with
/// their library's name, and a hero shelf of the films and series that have a backdrop.
#[test]
fn a_jellyfin_source_projects_next_up_my_media_latest_and_the_hero() {
    let item = |json: &str| -> crate::jf::models::BaseItemDto { serde_json::from_str(json).unwrap() };
    let film = |id: &str, backdrop: bool| item(&format!(
        r#"{{"Id":"{id}","Name":"F{id}","Type":"Movie","ImageTags":{{"Primary":"p"}}{}}}"#,
        if backdrop { r#","BackdropImageTags":["b"]"# } else { "" }));
    let view = item(r#"{"Id":"aaaaaaaaaaaaa000000000000000000a","Name":"Filmy","Type":"CollectionFolder","CollectionType":"movies","ImageTags":{"Primary":"lib"}}"#);
    let episode = item(r#"{"Id":"ccccccccccccc000000000000000000c","Name":"E3","Type":"Episode","SeriesId":"ddddddddddddd000000000000000000d","SeriesName":"S","ImageTags":{"Primary":"st"},"SeriesPrimaryImageTag":"sp"}"#);
    let shelves = vec![
        crate::jf::JfShelf { identifier: HUB_MY_MEDIA.into(), title: "My Media".into(), items: vec![view], ..Default::default() },
        crate::jf::JfShelf { identifier: "movie.recentlyadded.7".into(), title: "x".into(), section: 7, library: "Filmy".into(),
            items: vec![film("bbbbbbbbbbbbb000000000000000000b", false)], ..Default::default() },
    ];
    let b = project_jf(&shelves, &[], &[episode], &[film("eeeeeeeeeeeee000000000000000000e", true), film("fffffffffffff000000000000000000f", false)], ServerId::from_raw(1));
    let ids: Vec<&str> = b.shelves.iter().map(|s| s.hub_id.as_str()).collect();
    assert_eq!(ids, [HUB_NEXT_UP, HUB_MY_MEDIA, "movie.recentlyadded.7", HUB_HERO]);
    assert_eq!(b.shelves[0].title, nj_platform::i18n::msg::home_next_up());
    assert_eq!(b.shelves[1].items[0].title, "Filmy");
    assert_eq!(b.shelves[2].title, nj_platform::i18n::msg::browse_home_hub_recently_added_in("Filmy"));
    assert_eq!(b.shelves[3].items.len(), 1, "a hero needs a backdrop");
}
