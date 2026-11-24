//! A Jellyfin `BaseItemDto` → the [`PmsMovie`] row every shelf, grid and strip draws.
//!
//! The Jellyfin twin of [`super::parse_item`], reading the item's own fields: `Name`,
//! `ProductionYear`, `OfficialRating`, `CommunityRating`, `RunTimeTicks`, `Overview`,
//! `PremiereDate`, `UserData`, `SeriesId`/`SeriesName`, `IndexNumber`/`ParentIndexNumber`,
//! `ChildCount`/`RecursiveItemCount`, the image tags and `MediaSources[0]`. The row it builds is the one `parse_item` builds from the
//! same item, field for field (graded below), so nothing a screen draws or a cache keys on moves.
use super::{clean, PmsMovie, KIND_COLLECTION};
use crate::catalog::ServerId;
use crate::jf::models::BaseItemDto;
use crate::jf::{convert, ids, images, ticks};
use std::os::raw::c_int;

/// The item types a shelf or grid lists — `listable`'s Jellyfin spelling. A plain `Video` or
/// `MusicVideo` (Home Videos, Music Videos) lists as the film it opens as (`convert::item_kind`);
/// an extra does not.
pub(crate) fn listable(it: &BaseItemDto) -> bool {
    match it.kind.as_str() {
        "Movie" | "Series" | "Season" | "Episode" => true,
        "Video" | "MusicVideo" => it.extra_type.is_none(),
        _ => false,
    }
}

/// `UserData.LastPlayedDate` as unix seconds, 0 for never — Continue Watching's merge order.
pub(crate) fn last_played(it: &BaseItemDto) -> i64 {
    it.user_data.as_ref().and_then(|u| u.last_played_date.as_deref()).map(convert::iso_to_unix).unwrap_or(0)
}

fn row_kind(jf_type: &str) -> c_int {
    match jf_type {
        "Series" => 1,
        "Season" => 2,
        "Episode" => 3,
        "BoxSet" => KIND_COLLECTION,
        _ => 0,
    }
}

/// One item → one row. `sid` is the server the answer came from (captured by the spawning thread,
/// as for `parse_item`); `sec` the library it was listed under, 0 when the listing named none.
pub(crate) fn row(it: &BaseItemDto, sid: ServerId, sec: i64) -> PmsMovie {
    images::note_logo(it);
    let kind = row_kind(&it.kind);
    let ud = it.user_data.clone().unwrap_or_default();
    let mut m = PmsMovie { sid, sec, kind, ..Default::default() };
    m.aired = clean(it.premiere_date.as_deref().and_then(|d| d.get(..10)).unwrap_or(""));
    let series_key = ids::rating_key(it.series_id.as_deref().unwrap_or(""));
    let series_name = it.series_name.as_deref().unwrap_or("");
    match kind {
        3 => {
            m.show_rk = clean(&series_key);
            m.season_index = it.parent_index_number.unwrap_or(0) as c_int;
            m.show_title = clean(series_name);
            m.ep_index = it.index_number.unwrap_or(0) as c_int;
        }
        2 => {
            m.show_rk = clean(&series_key);
            m.season_index = it.index_number.unwrap_or(0) as c_int;
            m.show_title = clean(series_name);
        }
        _ => {}
    }
    // A collection has no watch or resume state of its own (see `parse_item`). A show or season
    // counts its episodes: `RecursiveItemCount` of them, `UnplayedItemCount` still to watch.
    let leaves = it.recursive_item_count.or(it.child_count).unwrap_or(0);
    let viewed_leaves = match ud.unplayed_item_count {
        Some(unplayed) => (leaves - unplayed).max(0),
        None if ud.played => leaves,
        None => 0,
    };
    m.unwatched = match kind {
        1 | 2 => viewed_leaves == 0 && leaves > 0,
        KIND_COLLECTION => false,
        _ => !ud.played,
    };
    m.watched = match kind {
        1 | 2 => leaves > 0 && viewed_leaves >= leaves,
        KIND_COLLECTION => false,
        _ => ud.played,
    };
    if kind == KIND_COLLECTION {
        m.child_count = it.child_count.unwrap_or(0).max(0);
    }
    if matches!(kind, 1 | 2) {
        m.unplayed = (leaves - viewed_leaves).max(0);
    }
    m.title = clean(&it.name);
    m.year = it.production_year.unwrap_or(0) as c_int;
    m.rating = clean(it.official_rating.as_deref().unwrap_or(""));
    m.score = it.community_rating.unwrap_or(0.0) as f32;
    m.dur_ns = ticks::to_ms(it.run_time_ticks.unwrap_or(0)) * 1_000_000;
    m.resume_ms = if kind == KIND_COLLECTION { 0 } else { ticks::to_ms(ud.playback_position_ticks) };
    // Poster: an episode wears its series' poster (its own image is a 16:9 still, kept in `still`
    // for landscape tiles); a season without one of its own borrows the series'.
    let own = images::primary(it);
    let series_poster = images::series_primary(it);
    m.thumb = match kind {
        3 if !series_poster.is_empty() => series_poster,
        2 if own.is_empty() => series_poster,
        _ => own.clone(),
    };
    m.still = if kind == 3 { own } else { String::new() };
    m.art = images::backdrop(it);
    m.summary = clean(it.overview.as_deref().unwrap_or(""));
    m.rk = ids::rating_key(&it.id);
    if let Some(source) = it.media_sources.first() {
        (m.vcodec, m.acodec) = convert::primary_codecs(source);
        m.part = convert::part_key(&it.id, source);
    }
    if let Some(blur) = images::ultra_blur(it).and_then(|u| u.corners()) {
        m.blur = blur;
        m.has_blur = true;
    }
    m
}

/// A library (`/UserViews` entry) as a My Media tile: its name and its own Primary image, which
/// is landscape, in `still` (the landscape tile's art) and `thumb`. `sec` is the library section OK
/// opens (`HomeReq::Library`), `kind` 1 for a TV library and 0 for every other. `None` for a view
/// the app cannot open as a library (Live TV, music, playlists).
pub(crate) fn library(view: &BaseItemDto, sid: ServerId) -> Option<PmsMovie> {
    let sec = convert::section(view)?;
    let art = images::primary(view);
    Some(PmsMovie {
        sid,
        sec: sec.key.parse().unwrap_or(0),
        kind: c_int::from(sec.kind == "show"),
        title: clean(&view.name),
        rk: ids::rating_key(&view.id),
        still: art.clone(),
        thumb: art,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_library_tile_opens_its_section_and_wears_its_own_image() {
        let tv = dto(r#"{"Id":"aaaaaaaaaaaaa000000000000000000a","Name":"Seriale","Type":"CollectionFolder",
            "CollectionType":"tvshows","ImageTags":{"Primary":"lib"}}"#);
        let m = library(&tv, ServerId::from_raw(2)).expect("a TV library is a tile");
        assert_eq!((m.kind, m.title.as_str()), (1, "Seriale"));
        assert_eq!(m.sec.to_string(), convert::section(&tv).unwrap().key);
        assert!(m.still.ends_with("-lib") && m.thumb == m.still, "{}", m.still);
        let live = dto(r#"{"Id":"bbbbbbbbbbbbb000000000000000000b","Name":"Live TV","Type":"UserView","CollectionType":"livetv"}"#);
        assert!(library(&live, ServerId::from_raw(2)).is_none(), "Live TV is no library tile");
    }

    fn dto(json: &str) -> BaseItemDto {
        serde_json::from_str(json).expect("fixture parses")
    }

    /// The row built from Jellyfin directly is the row the old two-step path built from the same
    /// item — every field, compared through the row's own serialization.
    fn same_as_converted(it: &BaseItemDto, sec: i64) {
        let sid = ServerId::from_raw(2);
        let direct = serde_json::to_value(row(it, sid, sec)).unwrap();
        let via_plex = serde_json::to_value(super::super::parse_item(&convert::item(it, sec), sid)).unwrap();
        assert_eq!(direct, via_plex, "{} ({})", it.name, it.kind);
    }

    #[test]
    fn every_kind_of_row_matches_the_converted_one() {
        let fixtures = [
            r#"{"Id":"36edf81507b5c8eda566778b2a316c29","Name":"Synthetic\nFilm","Type":"Movie",
                "ProductionYear":2014,"OfficialRating":"PG-13","CommunityRating":7.1,"Overview":"line one\r\nline two",
                "RunTimeTicks":81772160000,"PremiereDate":"2014-04-10T00:00:00.0000000Z",
                "ImageTags":{"Primary":"aaaa","Logo":"llll"},"BackdropImageTags":["bbbb"],
                "ImageBlurHashes":{"Backdrop":{"bbbb":"LEHV6nWB2yk8pyo0adR*.7kCMdnj"}},
                "UserData":{"PlaybackPositionTicks":435781990,"PlayCount":0,"Played":false},
                "MediaSources":[{"Id":"36edf81507b5c8eda566778b2a316c29","Container":"mkv,webm",
                  "DefaultAudioStreamIndex":2,"MediaStreams":[
                    {"Type":"Video","Index":0,"Codec":"hevc"},
                    {"Type":"Audio","Index":1,"Codec":"aac"},
                    {"Type":"Audio","Index":2,"Codec":"dts"}]}]}"#,
            r#"{"Id":"46edf81507b5c8eda566778b2a316c29","Name":"Watched","Type":"Movie",
                "ImageTags":{"Primary":"p"},"UserData":{"PlayCount":3,"Played":true}}"#,
            r#"{"Id":"aaaaaaaaaaaaa000000000000000000b","Type":"Episode","Name":"E",
                "SeriesId":"bbbbbbbbbbbbb000000000000000000c","SeriesName":"S","SeasonId":"ccccccccccccc000000000000000000d",
                "SeasonName":"Season 1","IndexNumber":3,"ParentIndexNumber":1,"SeriesPrimaryImageTag":"sp",
                "ParentBackdropItemId":"bbbbbbbbbbbbb000000000000000000c","ParentBackdropImageTags":["sb"],
                "ParentLogoItemId":"bbbbbbbbbbbbb000000000000000000c","ImageTags":{"Primary":"still"},
                "UserData":{"PlaybackPositionTicks":600000000,"Played":false}}"#,
            r#"{"Id":"aaaaaaaaaaaaa000000000000000001b","Type":"Episode","Name":"No show poster",
                "SeriesId":"bbbbbbbbbbbbb000000000000000000c","SeriesName":"S","IndexNumber":4,
                "ParentIndexNumber":1,"ImageTags":{"Primary":"still"}}"#,
            r#"{"Id":"ccccccccccccc000000000000000000d","Type":"Season","Name":"Season 1","IndexNumber":1,
                "SeriesId":"bbbbbbbbbbbbb000000000000000000c","SeriesName":"S","SeriesPrimaryImageTag":"sp",
                "ChildCount":8,"UserData":{"UnplayedItemCount":8,"Played":false}}"#,
            r#"{"Id":"ccccccccccccc000000000000000001d","Type":"Season","Name":"Season 2","IndexNumber":2,
                "SeriesId":"bbbbbbbbbbbbb000000000000000000c","SeriesName":"S","ImageTags":{"Primary":"own"},
                "RecursiveItemCount":6,"UserData":{"UnplayedItemCount":0,"Played":true}}"#,
            r#"{"Id":"ddddddddddddd000000000000000000e","Type":"Series","Name":"Midway","ChildCount":2,
                "RecursiveItemCount":20,"ImageTags":{"Primary":"sp"},"UserData":{"UnplayedItemCount":5,"Played":false}}"#,
            r#"{"Id":"ddddddddddddd000000000000000001e","Type":"Series","Name":"No counts","ImageTags":{"Primary":"sp"},
                "UserData":{"Played":true}}"#,
            r#"{"Id":"eeeeeeeeeeeee000000000000000000f","Type":"BoxSet","Name":"Trilogy","ChildCount":3,
                "RunTimeTicks":12000000000,"ImageTags":{"Primary":"bx"},
                "UserData":{"PlaybackPositionTicks":600000000,"PlayCount":2,"Played":true}}"#,
            r#"{"Id":"eeeeeeeeeeeee000000000000000001f","Type":"BoxSet","Name":"Odd","ChildCount":-1}"#,
            r#"{"Id":"fffffffffffff000000000000000000a","Type":"Trailer","Name":"Clip"}"#,
            r#"{"Id":"fffffffffffff000000000000000001a","Type":"Video","Name":"Holiday","RunTimeTicks":600000000,
                "ImageTags":{"Primary":"v"},"UserData":{"PlaybackPositionTicks":60000000,"Played":false}}"#,
        ];
        for json in fixtures {
            for sec in [0, 77] {
                same_as_converted(&dto(json), sec);
            }
        }
        assert_eq!(row(&dto(fixtures[0]), ServerId::UNSET, 0).score, 7.1, "CommunityRating is the score");
        assert_eq!(row(&dto(fixtures[1]), ServerId::UNSET, 0).score, 0.0, "no rating, no score");
    }

    /// A show or a season carries the episodes still to watch for its card's badge; a film none.
    #[test]
    fn a_show_counts_its_unwatched_episodes() {
        let sid = ServerId::from_raw(2);
        let series = dto(r#"{"Id":"ddddddddddddd000000000000000000e","Type":"Series","Name":"Midway",
            "RecursiveItemCount":20,"UserData":{"UnplayedItemCount":5,"Played":false}}"#);
        assert_eq!(row(&series, sid, 0).unplayed, 5);
        let done = dto(r#"{"Id":"ccccccccccccc000000000000000001d","Type":"Season","Name":"S2",
            "RecursiveItemCount":6,"UserData":{"UnplayedItemCount":0,"Played":true}}"#);
        assert_eq!(row(&done, sid, 0).unplayed, 0);
        let film = dto(r#"{"Id":"46edf81507b5c8eda566778b2a316c29","Name":"F","Type":"Movie","UserData":{"Played":false}}"#);
        assert_eq!(row(&film, sid, 0).unplayed, 0);
    }

    #[test]
    fn an_episode_wears_its_series_poster_and_keeps_its_own_still() {
        let ep = dto(r#"{"Id":"aaaaaaaaaaaaa000000000000000000b","Type":"Episode","Name":"E",
            "SeriesId":"bbbbbbbbbbbbb000000000000000000c","SeriesName":"S","IndexNumber":3,"ParentIndexNumber":1,
            "SeriesPrimaryImageTag":"sp","ImageTags":{"Primary":"still"}}"#);
        let m = row(&ep, ServerId::UNSET, 0);
        assert_eq!((m.kind, m.season_index, m.ep_index, m.show_title.as_str()), (3, 1, 3, "S"));
        assert!(m.thumb.ends_with("/thumb/bbbbbbbbbbbbb000000000000000000c-sp"), "{}", m.thumb);
        assert!(m.still.ends_with("/thumb/aaaaaaaaaaaaa000000000000000000b-still"), "{}", m.still);
        assert_eq!(m.show_rk, ids::intern("bbbbbbbbbbbbb000000000000000000c").to_string());
    }

    /// Titles list, and so do a Home Videos or Music Videos library's plain videos (they open and
    /// play as films); an extra (a video with an `ExtraType`) does not.
    #[test]
    fn titles_and_plain_videos_list() {
        for (t, ok) in [("Movie", true), ("Series", true), ("Season", true), ("Episode", true),
            ("Video", true), ("MusicVideo", true), ("BoxSet", false), ("Person", false), ("Trailer", false)]
        {
            assert_eq!(listable(&BaseItemDto { kind: t.into(), ..Default::default() }), ok, "{t}");
        }
        let extra = BaseItemDto { kind: "Video".into(), extra_type: Some("Featurette".into()), ..Default::default() };
        assert!(!listable(&extra), "an extra is no title");
    }
}
