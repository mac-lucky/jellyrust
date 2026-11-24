//! The Jellyfin operations.
//!
//! `Client::jf()` hands one of these out when the client's origin is a Jellyfin seat
//! ([`super::seat`]). The list stores ask it for Jellyfin results directly — [`JfShelf`]s,
//! [`JfPage`]s, [`JfSearch`] and `BaseItemDto` rows — and build their own screen rows from them.
//! The detail, playback and timeline ops still answer in the catalog record types the player and
//! the route planner read, through `catalog::Client`'s delegation. Requests go through
//! `Client::jf_send` — the one [`crate::http`] door, origin, resolve pin and credential gate —
//! with the `Authorization: MediaBrowser …` header.
use super::models::*;
use super::{convert, ids, seat, url};
use crate::http::Method;
use crate::catalog::collections::CollectionOutcome;
use crate::catalog::{
    Client, Hub, LibrarySection, MediaContainer, Meta, MetaType, Metadata, SectionQuery,
    SortOption, Tag,
};
use serde::de::DeserializeOwned;

/// One Home or Library shelf as Jellyfin answered it.
#[derive(Debug, Default, Clone)]
pub struct JfShelf {
    /// The locale-independent shelf id Home and the Library key titles and focus memory by
    /// (`home.movies.recent`, `tv.ondeck.{section}`, …).
    pub identifier: String,
    pub title: String,
    pub key: String,
    /// The library the shelf lists, 0 for none; and that library's name.
    pub section: i64,
    pub library: String,
    pub items: Vec<BaseItemDto>,
}

/// One page of a library listing.
#[derive(Default)]
pub struct JfPage {
    pub items: Vec<BaseItemDto>,
    pub total: i64,
    pub start: i64,
    /// The sort menu, when the query asked for it.
    pub meta: Option<Meta>,
}

/// A search answer: title hits per [`SEARCH_GROUPS`] type, in that order, and person hits.
#[derive(Debug, Default, Clone)]
pub struct JfSearch {
    pub groups: Vec<(&'static str, Vec<BaseItemDto>)>,
    pub people: Vec<BaseItemDto>,
}

/// `(Jellyfin Type, shelf kind, heading)` for each title group a search returns.
pub const SEARCH_GROUPS: [(&str, &str, &str); 4] = [
    ("Movie", "movie", "Movies"),
    ("Series", "show", "Shows"),
    ("Episode", "episode", "Episodes"),
    ("BoxSet", "collection", "Collections"),
];

/// `Fields=` for list endpoints: what a poster grid and a shelf draw, without the per-item
/// `MediaSources` a detail page needs (those make a 50-row page ~40x larger).
const LIST_FIELDS: &str =
    "PrimaryImageAspectRatio,Overview,Genres,DateCreated,ChildCount,RecursiveItemCount,ProviderIds,Taglines";
/// …and for rows that may be PLAYED straight from the list (episodes, Up Next, Continue
/// Watching): those need their media sources so the route can pick a part without a detail read.
const PLAYABLE_FIELDS: &str =
    "PrimaryImageAspectRatio,Overview,Genres,DateCreated,ChildCount,RecursiveItemCount,ProviderIds,Taglines,MediaSources,Chapters";

/// Percent-encoded query assembly — the JF twin of `plex::QueryBuilder`, over the same encoder.
pub(crate) struct Q {
    path: String,
    parts: Vec<String>,
}

impl Q {
    pub(crate) fn new(path: impl Into<String>) -> Self {
        Self { path: path.into(), parts: Vec::new() }
    }
    pub(crate) fn s(mut self, k: &str, v: &str) -> Self {
        if !v.is_empty() {
            self.parts.push(format!("{k}={}", crate::catalog::urlenc_str(v)));
        }
        self
    }
    pub(crate) fn i(mut self, k: &str, v: i64) -> Self {
        self.parts.push(format!("{k}={v}"));
        self
    }
    pub(crate) fn b(self, k: &str, v: bool) -> Self {
        self.s(k, if v { "true" } else { "false" })
    }
    pub(crate) fn build(self) -> String {
        if self.parts.is_empty() {
            self.path
        } else {
            let sep = if self.path.contains('?') { '&' } else { '?' };
            format!("{}{sep}{}", self.path, self.parts.join("&"))
        }
    }
}

/// The Plex `sort` expression (`titleSort:asc`, `addedAt:desc`, `a:desc,b`) → `(SortBy, SortOrder)`.
pub(crate) fn sort_by(plex_sort: &str) -> (String, &'static str) {
    let mut fields = Vec::new();
    let mut order = "Ascending";
    for (i, part) in plex_sort.split(',').filter(|p| !p.is_empty()).enumerate() {
        let (key, dir) = part.split_once(':').unwrap_or((part, "asc"));
        if i == 0 && dir == "desc" {
            order = "Descending";
        }
        let mapped: &[&str] = match key {
            "titleSort" | "title" => &["SortName"],
            "addedAt" => &["DateCreated"],
            "originallyAvailableAt" | "year" => &["PremiereDate", "ProductionYear"],
            "audienceRating" => &["CommunityRating"],
            "rating" => &["CriticRating"],
            "lastViewedAt" => &["DatePlayed"],
            "viewCount" => &["PlayCount"],
            "duration" => &["Runtime"],
            "random" => &["Random"],
            "showOrder" => &["SeriesSortName", "ParentIndexNumber", "IndexNumber"],
            "seasonOrder" => &["SeriesSortName", "IndexNumber"],
            _ => &[],
        };
        for m in mapped {
            if !fields.contains(m) {
                fields.push(*m);
            }
        }
    }
    if fields.is_empty() {
        fields.push("SortName");
    }
    if !fields.contains(&"SortName") && !fields.contains(&"Random") {
        fields.push("SortName");
    }
    (fields.join(","), order)
}

fn sort_option(key: &str, title: &str, default_direction: &str) -> SortOption {
    SortOption { key: key.into(), desc_key: String::new(), default_direction: default_direction.into(), title: title.into() }
}

/// The sort menu a section advertises for `jf_type` — the server-driven `Meta` PMS sends with
/// `includeMeta=1`, synthesized from what `/Items` can actually order by.
fn sort_menu(jf_type: &str) -> Meta {
    let mut sort = match jf_type {
        "Episode" => vec![SortOption {
            key: "showOrder".into(),
            desc_key: "showOrder:desc".into(),
            default_direction: "asc".into(),
            title: "Show".into(),
        }],
        "Season" => vec![sort_option("seasonOrder", "Show", "asc")],
        _ => Vec::new(),
    };
    sort.extend([
        sort_option("titleSort", "Title", "asc"),
        sort_option("addedAt", "Date Added", "desc"),
        sort_option("originallyAvailableAt", "Release Date", "desc"),
        sort_option("audienceRating", "Rating", "desc"),
        sort_option("lastViewedAt", "Last Played", "desc"),
        sort_option("duration", "Duration", "desc"),
    ]);
    Meta { types: vec![MetaType { active: 1, sort }] }
}

pub struct Jf<'a> {
    c: &'a Client,
    /// The DeviceId basis for a sign-in, before the origin has a seat to hold it.
    device_user: Option<String>,
}

impl<'a> Jf<'a> {
    pub(crate) fn new(c: &'a Client) -> Self {
        Self { c, device_user: None }
    }

    pub(crate) fn for_sign_in(c: &'a Client, device_user: &str) -> Self {
        Self { c, device_user: Some(device_user.to_string()) }
    }

    fn seat(&self) -> seat::Seat {
        seat::get(self.c.origin()).unwrap_or_default()
    }

    pub(crate) fn origin(&self) -> &crate::catalog::Origin {
        self.c.origin()
    }

    pub(crate) fn token(&self) -> String {
        self.c.current_token()
    }

    /// The four device facts plus this user's DeviceId — see [`url::device_id`].
    pub(crate) fn identity(&self) -> url::DeviceIdentity {
        let s = self.seat();
        url::DeviceIdentity {
            client: crate::catalog::identity::PRODUCT.to_string(),
            device: crate::catalog::identity::device_name().to_string(),
            device_id: url::device_id(self.c.client_id(), self.device_user.as_deref().unwrap_or(&s.device_user)),
            version: crate::catalog::identity::VERSION.to_string(),
        }
    }

    pub(crate) fn auth_header(&self) -> String {
        url::authorization(&self.identity(), &self.c.current_token())
    }

    fn send(&self, path: &str, method: Method, body: Option<&[u8]>) -> Option<crate::http::Reply> {
        let auth = self.auth_header();
        let mut headers = vec![crate::http::ACCEPT_JSON, auth.as_str()];
        if body.is_some() {
            headers.push("Content-Type: application/json");
        }
        self.c.jf_send(path, method, &headers, body)
    }

    /// The whole answer, for a caller that tells refusals and failures apart (`preferences`).
    pub(crate) fn reply(&self, path: &str, method: Method, body: Option<&[u8]>) -> Option<crate::http::Reply> {
        self.send(path, method, body)
    }

    pub(crate) fn status(&self, path: &str, method: Method, body: Option<&[u8]>) -> Option<i32> {
        self.send(path, method, body).map(|r| r.status)
    }

    fn ok(&self, path: &str, method: Method, body: Option<&[u8]>) -> bool {
        self.send(path, method, body).is_some_and(|r| r.ok())
    }

    /// Worker-only bounded preview request. Header authentication also works on private libraries.
    pub(crate) fn preview_bytes(&self, path: &str, max: usize) -> crate::catalog::ArtFetch {
        self.c.jf_preview_get(path, &[&self.auth_header()], max)
    }

    pub(crate) fn get<T: DeserializeOwned>(&self, path: &str) -> Option<T> {
        let r = self.send(path, Method::Get, None)?;
        if !r.ok() {
            return None;
        }
        parse(path, &r.body)
    }

    fn get_status<T: DeserializeOwned>(&self, path: &str) -> (Option<i32>, Option<T>) {
        match self.send(path, Method::Get, None) {
            None => (None, None),
            Some(r) if r.ok() => (Some(r.status), parse(path, &r.body)),
            Some(r) => (Some(r.status), None),
        }
    }

    pub(crate) fn post<T: DeserializeOwned>(&self, path: &str, body: &impl serde::Serialize) -> Option<T> {
        let bytes = serde_json::to_vec(body).ok()?;
        let r = self.send(path, Method::Post, Some(&bytes))?;
        if !r.ok() {
            return None;
        }
        parse(path, &r.body)
    }

    pub(crate) fn post_ok(&self, path: &str, body: Option<&impl serde::Serialize>) -> bool {
        match body {
            Some(b) => match serde_json::to_vec(b) {
                Ok(bytes) => self.ok(path, Method::Post, Some(&bytes)),
                Err(_) => false,
            },
            None => self.ok(path, Method::Post, None),
        }
    }

    /// The signed-in user's id, learned once per seat from `GET /Users/Me`.
    pub(crate) fn user_id(&self) -> Option<String> {
        let s = self.seat();
        if !s.user_id.is_empty() {
            return Some(s.user_id);
        }
        let me: UserDto = self.get("/Users/Me")?;
        if me.id.is_empty() {
            return None;
        }
        let id = me.id.clone();
        seat::update(self.c.origin(), |s| {
            s.user_id = me.id;
            if s.user_name.is_empty() {
                s.user_name = me.name;
            }
            if s.server_id.is_empty() {
                s.server_id = me.server_id;
            }
        });
        Some(id)
    }

    fn items(&self, q: Q) -> Option<QueryResult<BaseItemDto>> {
        self.query(q)
    }

    /// [`Self::items`], each row parsed as `T`: a sweep that needs two fields of every title keeps
    /// only those, not a whole `BaseItemDto` per item.
    fn query<T: DeserializeOwned + Default>(&self, q: Q) -> Option<QueryResult<T>> {
        let q = match self.user_id() {
            Some(uid) => q.s("userId", &uid),
            None => q,
        };
        self.get(&q.build())
    }

    fn items_container(&self, q: Q, section_id: i64) -> Option<MediaContainer> {
        let r = self.items(q)?;
        let rows = r.items.iter().map(|i| convert::item(i, section_id)).collect();
        Some(convert::container(rows, r.total_record_count, r.start_index))
    }

    /// The GUID behind a key. A key PERSISTED by an earlier run (the cold-open Home cache, a resume
    /// target, a dev `play` trigger) was never minted in this process, and the mapping is one-way;
    /// on such a miss one ids-only sweep of the user's items re-mints every key. Throttled per
    /// origin, so a key that is genuinely gone costs one sweep a minute and not one per lookup.
    pub(crate) fn guid(&self, rk: &str) -> Option<String> {
        if let Some(g) = ids::guid_of_key(rk) {
            return Some(g);
        }
        if rk.is_empty() || !rk.bytes().all(|b| b.is_ascii_digit()) || !self.sweep_due() {
            return None;
        }
        let q = Q::new("/Items")
            .b("Recursive", true)
            .s("IncludeItemTypes", "Movie,Series,Season,Episode,BoxSet,Video,MusicVideo")
            .b("EnableImages", false)
            .b("EnableUserData", false)
            .b("EnableTotalRecordCount", false)
            .s("Fields", "");
        let r = self.items(q)?;
        for i in &r.items {
            ids::intern(&i.id);
        }
        ids::guid_of_key(rk)
    }

    /// The GUID behind an id that is a key ([`Self::guid`]) or a raw GUID: a person's page holds
    /// either.
    pub(crate) fn guid_or_raw(&self, id: &str) -> Option<String> {
        if id.bytes().all(|b| b.is_ascii_digit()) { self.guid(id) } else { Some(ids::normalize(id)) }
    }

    fn sweep_due(&self) -> bool {
        use std::collections::HashMap;
        use std::sync::{Mutex, OnceLock};
        use std::time::{Duration, Instant};
        static LAST: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
        let Ok(mut m) = LAST.get_or_init(Default::default).lock() else { return false };
        let key = self.origin().base().to_ascii_lowercase();
        let now = Instant::now();
        if m.get(&key).is_some_and(|t| now.duration_since(*t) < Duration::from_secs(60)) {
            return false;
        }
        m.insert(key, now);
        true
    }

    // ---- server ---------------------------------------------------------------------------

    pub(crate) fn public_info(&self) -> Option<PublicSystemInfo> {
        let info: PublicSystemInfo = self.get("/System/Info/Public")?;
        seat::update(self.c.origin(), |s| {
            s.server_id = info.id.clone();
            s.server_name = info.server_name.clone();
            s.server_version = info.version.clone();
        });
        Some(info)
    }

    pub fn friendly_name(&self) -> Option<String> {
        self.public_info().map(|i| i.server_name).filter(|s| !s.is_empty())
    }

    pub fn machine_identity(&self) -> Option<String> {
        self.public_info().map(|i| ids::normalize(&i.id)).filter(|s| !s.is_empty())
    }

    /// The "server root" read `serverinfo` makes: version only (Jellyfin has no subscription).
    pub fn server_root(&self) -> Option<MediaContainer> {
        let i = self.public_info()?;
        Some(MediaContainer { friendly_name: i.server_name, version: i.version,
            machine_identifier: ids::normalize(&i.id), ..Default::default() })
    }

    // ---- library --------------------------------------------------------------------------

    pub fn sections(&self) -> Option<MediaContainer> {
        let uid = self.user_id()?;
        let views: QueryResult<BaseItemDto> = self.get(&Q::new("/UserViews").s("userId", &uid).build())?;
        let directory: Vec<LibrarySection> = views.items.iter().filter_map(convert::section).collect();
        Some(MediaContainer { size: directory.len() as i64, directory, ..Default::default() })
    }

    /// Which `IncludeItemTypes` a section listing names: the explicit `type=` filter, else the
    /// section's primary type (Movie for a movie library, Series for TV).
    fn section_type(&self, section_guid: &str, filters: &[(String, String)]) -> &'static str {
        let filter = filters.iter().find(|(k, _)| k == "type").and_then(|(_, v)| v.parse().ok())
            .and_then(convert::jf_type_of_number);
        if let Some(t) = filter {
            return t;
        }
        convert::listing_item_types(None, self.view(section_guid).as_ref())
    }

    /// The library view `section_guid` names, or `None` when `/UserViews` could not be read or no
    /// longer lists it.
    fn view(&self, section_guid: &str) -> Option<BaseItemDto> {
        let uid = self.user_id()?;
        let views: QueryResult<BaseItemDto> = self.get(&Q::new("/UserViews").s("userId", &uid).build())?;
        views.items.into_iter().find(|v| ids::normalize(&v.id) == section_guid)
    }

    /// One page of a library grid: `/Items` under the library, typed, sorted, filtered.
    pub fn section_page(&self, q: &SectionQuery) -> Option<JfPage> {
        let section_guid = ids::guid_of(q.section_key)?;
        let jf_type = self.section_type(&section_guid, q.filters);
        let (sort, order) = sort_by(if q.sort.is_empty() {
            match jf_type { "Episode" => "showOrder", "Season" => "seasonOrder", _ => "titleSort" }
        } else {
            q.sort
        });
        let b = Q::new("/Items")
            .s("IncludeItemTypes", jf_type)
            .b("Recursive", true)
            .s("SortBy", &sort)
            .s("SortOrder", order)
            .s("Fields", if jf_type == "Episode" { PLAYABLE_FIELDS } else { LIST_FIELDS })
            .s("ImageTypeLimit", "1")
            .s("EnableImageTypes", "Primary,Backdrop,Logo,Thumb")
            .b("EnableTotalRecordCount", true)
            .i("StartIndex", q.start)
            .i("Limit", q.size)
            // A box set too: 12.x lists those with members in this library, 10.x ignores it for a
            // lone BoxSet type, and the letter counts below send it either way.
            .s("ParentId", &section_guid);
        let b = with_listing_filters(b, q.filters);
        let r = self.items(b)?;
        Some(JfPage {
            items: r.items,
            total: r.total_record_count,
            start: r.start_index,
            meta: q.include_meta.then(|| sort_menu(jf_type)),
        })
    }

    pub fn section_directory(&self, section_key: i64, directory: &str, metadata_type: Option<i64>, filters: &[(String, String)]) -> Option<MediaContainer> {
        let section_guid = ids::guid_of(section_key)?;
        let jf_type = metadata_type.and_then(convert::jf_type_of_number)
            .unwrap_or_else(|| self.section_type(&section_guid, &[]));
        let rows: Vec<LibrarySection> = match directory {
            "genre" => {
                let f: QueryFilters = self.get(&Q::new("/Items/Filters2")
                    .s("userId", &self.user_id()?)
                    .s("ParentId", &section_guid)
                    .s("IncludeItemTypes", jf_type)
                    .build())?;
                f.genres.iter().map(|g| LibrarySection {
                    key: ids::intern(&g.id).to_string(),
                    kind: "genre".into(),
                    title: g.name.clone(),
                    size: 0,
                    ..Default::default()
                }).collect()
            }
            "firstCharacter" => {
                // One sorted read of every name, grouped by its first character — the server's own
                // SortName order, which is what the grid scrolls through.
                // Under the listing's own filters: these counts are the offsets a letter jumps to.
                let r = self.items(with_listing_filters(Q::new("/Items")
                    .s("ParentId", &section_guid)
                    .s("IncludeItemTypes", jf_type)
                    .b("Recursive", true)
                    .s("SortBy", "SortName")
                    .s("Fields", "SortName")
                    .b("EnableImages", false)
                    .b("EnableUserData", false), filters))?;
                let mut out: Vec<LibrarySection> = Vec::new();
                for it in &r.items {
                    let name = it.sort_name.as_deref().unwrap_or(&it.name);
                    let c = name.chars().next().map(|c| c.to_ascii_uppercase()).unwrap_or('#');
                    let key = if c.is_ascii_alphabetic() { c.to_string() } else { "#".to_string() };
                    match out.last_mut() {
                        Some(last) if last.key == key => last.size += 1,
                        _ => out.push(LibrarySection {
                            key: key.clone(), kind: String::new(), title: key, size: 1, ..Default::default()
                        }),
                    }
                }
                out
            }
            _ => Vec::new(),
        };
        Some(MediaContainer { size: rows.len() as i64, directory: rows, ..Default::default() })
    }

    /// The full item, with its markers (MediaSegments) and — for a show — the next episode.
    pub fn metadata(&self, rk: &str) -> Option<Metadata> {
        let guid = self.guid(rk)?;
        let uid = self.user_id()?;
        let it: BaseItemDto = self.get(&Q::new(format!("/Items/{guid}")).s("userId", &uid).build())?;
        let mut m = convert::item(&it, 0);
        if matches!(it.kind.as_str(), "Movie" | "Episode" | "Video") {
            if let Some(segs) = self.get::<QueryResult<MediaSegmentDto>>(&format!("/MediaSegments/{guid}")) {
                m.marker = convert::markers(&segs.items, m.duration);
            }
        }
        if it.kind == "Series" {
            if let Some(next) = self.items(Q::new("/Shows/NextUp").s("SeriesId", &guid).i("Limit", 1)
                .s("Fields", PLAYABLE_FIELDS).b("EnableResumable", true))
                .and_then(|r| r.items.into_iter().next())
            {
                m.on_deck = Some(crate::catalog::OnDeckHub { metadata: Some(Box::new(convert::item(&next, 0))) });
            }
        }
        Some(m)
    }

    pub fn extras(&self, rk: &str) -> Option<MediaContainer> {
        let guid = self.guid(rk)?;
        let uid = self.user_id()?;
        let rows: Vec<BaseItemDto> = self.get(&Q::new(format!("/Items/{guid}/SpecialFeatures")).s("userId", &uid).build())?;
        let mut items: Vec<Metadata> = rows.iter().map(|i| convert::item(i, 0)).collect();
        // A local trailer is listed separately from special features on Jellyfin.
        if let Some(trailers) = self.get::<Vec<BaseItemDto>>(&Q::new(format!("/Items/{guid}/LocalTrailers")).s("userId", &uid).build()) {
            for t in &trailers {
                let mut m = convert::item(t, 0);
                m.kind = "clip".into();
                m.subtype = "trailer".into();
                m.extra_type = 1;
                items.insert(0, m);
            }
        }
        let n = items.len() as i64;
        Some(convert::container(items, n, 0))
    }

    pub fn metadata_many(&self, rks: &[&str]) -> Option<MediaContainer> {
        let guids: Vec<String> = rks.iter().filter_map(|rk| self.guid(rk)).collect();
        if guids.is_empty() {
            return Some(MediaContainer::default());
        }
        let r = self.items(Q::new("/Items").s("Ids", &guids.join(",")).s("Fields", "People"))?;
        // request order, as PMS answers
        let mut rows: Vec<Metadata> = Vec::with_capacity(r.items.len());
        for g in &guids {
            if let Some(it) = r.items.iter().find(|i| ids::normalize(&i.id) == *g) {
                rows.push(convert::item(it, 0));
            }
        }
        let n = rows.len() as i64;
        Some(convert::container(rows, n, 0))
    }

    pub fn children(&self, rk: &str) -> Option<MediaContainer> {
        let guid = self.guid(rk)?;
        let uid = self.user_id()?;
        let it: BaseItemDto = self.get(&Q::new(format!("/Items/{guid}")).s("userId", &uid).s("Fields", "ChildCount").build())?;
        let q = match it.kind.as_str() {
            "Series" => Q::new(format!("/Shows/{guid}/Seasons")).s("Fields", LIST_FIELDS),
            "Season" => Q::new(format!("/Shows/{}/Episodes", ids::normalize(it.series_id.as_deref().unwrap_or(""))))
                .s("SeasonId", &guid).s("Fields", PLAYABLE_FIELDS),
            _ => Q::new("/Items").s("ParentId", &guid).s("Fields", LIST_FIELDS).s("SortBy", "SortName"),
        };
        self.items_container(q.s("ImageTypeLimit", "1").s("EnableImageTypes", "Primary,Backdrop,Logo,Thumb"), 0)
    }

    pub fn all_leaves(&self, rk: &str) -> Option<MediaContainer> {
        let guid = self.guid(rk)?;
        self.items_container(Q::new(format!("/Shows/{guid}/Episodes")).s("Fields", PLAYABLE_FIELDS), 0)
    }

    /// "More Like This": `/Items/{id}/Similar`.
    pub fn similar_items(&self, rk: &str) -> Option<Vec<BaseItemDto>> {
        let guid = self.guid(rk)?;
        Some(self.items(Q::new(format!("/Items/{guid}/Similar")).i("Limit", 20).s("Fields", LIST_FIELDS))?.items)
    }

    /// The collections (box sets) holding an item: `/Items/{id}/Collections`, new in 12.x. `None`
    /// when the server did not answer, which is every 10.x server (it has no such route).
    pub fn item_collections(&self, rk: &str) -> Option<Vec<BaseItemDto>> {
        let guid = self.guid(rk)?;
        Some(self.items(Q::new(format!("/Items/{guid}/Collections")))?.items)
    }

    /// A person's filmography: every movie, series and episode they are credited in, newest first.
    /// A guest star is credited on the episode alone (the person page lists its show).
    pub fn person_items(&self, person_id: &str) -> Option<Vec<BaseItemDto>> {
        let guid = self.guid_or_raw(person_id)?;
        Some(self.items(Q::new("/Items").s("PersonIds", &guid).b("Recursive", true)
            .s("IncludeItemTypes", "Movie,Series,Episode").s("SortBy", "PremiereDate,SortName")
            .s("SortOrder", "Descending").s("Fields", LIST_FIELDS))?.items)
    }

    /// `find_by_guid`: does this server hold the item a portable guid names (`imdb://tt…`)? Only
    /// among items of `kind`, the origin's own type (`movie` / `show` / `season` / `episode`): TMDb
    /// numbers films, shows and episodes separately, and TVDB shows and episodes, so the same number
    /// on another type is another title.
    pub fn find_by_guid(&self, guid: &str, kind: &str) -> Option<MediaContainer> {
        let (scheme, id) = guid.split_once("://")?;
        let by_ids = |ids: &str| self.items_container(Q::new("/Items").s("Ids", ids).s("Fields", PLAYABLE_FIELDS), 0);
        if scheme == "jellyfin" {
            return by_ids(id);
        }
        let flag = match scheme { "imdb" => "HasImdbId", "tmdb" => "HasTmdbId", "tvdb" => "HasTvdbId", _ => return None };
        let item_type = match kind { "movie" => "Movie", "show" => "Series", "season" => "Season", "episode" => "Episode",
            _ => return Some(convert::container(Vec::new(), 0, 0)) };
        // `/Items` has no provider-id filter, so this reads the provider ids of every title of this
        // type that has one of this kind (ids only, as `guid`'s sweep), then the matches in full.
        // shortcut: one sweep per lookup, and only a second registered server ever asks; pass the
        // title in (`searchTerm`) if a large second library makes "Also available" slow.
        let r: QueryResult<ProviderIdsRow> = self.query(Q::new("/Items").b("Recursive", true).b(flag, true)
            .s("IncludeItemTypes", item_type).s("Fields", "ProviderIds")
            .b("EnableImages", false).b("EnableUserData", false).b("EnableTotalRecordCount", false))?;
        let key = provider_key(scheme);
        let held: Vec<&str> = r.items.iter().filter(|i| i.provider_ids.get(key).is_some_and(|v| v == id))
            .map(|i| i.id.as_str()).collect();
        if held.is_empty() {
            return Some(convert::container(Vec::new(), 0, 0));
        }
        by_ids(&held.join(","))
    }

    // ---- user data ------------------------------------------------------------------------

    pub fn scrobble(&self, rk: &str) -> bool {
        let (Some(guid), Some(uid)) = (self.guid(rk), self.user_id()) else { return false };
        self.ok(&Q::new(format!("/UserPlayedItems/{guid}")).s("userId", &uid).build(), Method::Post, None)
    }

    pub fn unscrobble(&self, rk: &str) -> bool {
        let (Some(guid), Some(uid)) = (self.guid(rk), self.user_id()) else { return false };
        self.ok(&Q::new(format!("/UserPlayedItems/{guid}")).s("userId", &uid).build(), Method::Delete, None)
    }

    /// `POST` / `DELETE /UserFavoriteItems/{id}` — mark or unmark the viewer's favourite: an item
    /// by its key, a person by the key or raw GUID their page holds.
    pub fn set_favorite(&self, rk: &str, on: bool) -> bool {
        let (Some(guid), Some(uid)) = (self.guid_or_raw(rk), self.user_id()) else { return false };
        let method = if on { Method::Post } else { Method::Delete };
        self.ok(&Q::new(format!("/UserFavoriteItems/{guid}")).s("userId", &uid).build(), method, None)
    }

    /// Jellyfin has no hide flag: an item leaves Resume when its position is cleared, so this
    /// resets the resume point (docs/jellyfin-port.md records the difference from PMS).
    pub fn remove_from_continue_watching(&self, rk: &str) -> bool {
        let (Some(guid), Some(uid)) = (self.guid(rk), self.user_id()) else { return false };
        self.post_ok(
            &Q::new(format!("/UserItems/{guid}/UserData")).s("userId", &uid).build(),
            Some(&serde_json::json!({ "PlaybackPositionTicks": 0 })),
        )
    }

    // ---- shelves ---------------------------------------------------------------------------

    /// Home's library rows, in the user's view order: My Media ([`Self::HOME_MY_MEDIA`], every
    /// library the app can open as its `/UserViews` entry, so Live TV is left out) and one Latest
    /// row per library (`/Items/Latest`, grouped) the user has not hidden from Latest.
    pub fn home_shelves(&self, count: i64) -> Option<Vec<JfShelf>> {
        let uid = self.user_id()?;
        let (views, me) = std::thread::scope(|s| {
            let me = nj_base::task::beside(s, "home-me", || self.get::<UserDto>("/Users/Me"));
            (self.get::<QueryResult<BaseItemDto>>(&Q::new("/UserViews").s("userId", &uid).build()), me.finish())
        });
        let views = views?;
        // The server applies `LatestItemsExcludes` only to an unscoped `/Items/Latest`; these are
        // per library, so the client skips the hidden ones, as jellyfin-web does.
        let hidden: Vec<String> = me.map(|u| u.configuration.latest_items_excludes.iter().map(|id| ids::normalize(id)).collect())
            .unwrap_or_default();
        let media: Vec<BaseItemDto> = views.items.iter().filter(|v| convert::section(v).is_some()).cloned().collect();
        // One `/Items/Latest` per library, asked together; the shelves keep the libraries' order.
        let latest = std::thread::scope(|s| {
            let asks: Vec<_> = views.items.iter().filter_map(|v| {
                let sec = convert::section(v)?;
                let parent = ids::normalize(&v.id);
                if hidden.contains(&parent) {
                    return None;
                }
                let uid = uid.as_str();
                Some((sec, nj_base::task::beside(s, "home-latest", move || -> Option<Vec<BaseItemDto>> {
                    self.get(&Q::new("/Items/Latest").s("userId", uid)
                        .s("ParentId", &parent).i("Limit", count).b("GroupItems", true)
                        .s("Fields", LIST_FIELDS).s("ImageTypeLimit", "1")
                        .s("EnableImageTypes", "Primary,Backdrop,Logo,Thumb").build())
                })))
            }).collect();
            asks.into_iter().map(|(sec, ask)| (sec, ask.finish())).collect::<Vec<_>>()
        });
        let mut out = Vec::new();
        if !media.is_empty() {
            out.push(JfShelf {
                identifier: Self::HOME_MY_MEDIA.into(),
                title: nj_platform::i18n::msg::home_my_media().into(),
                key: String::new(),
                section: 0,
                library: String::new(),
                items: media,
            });
        }
        for (sec, latest) in latest {
            let section: i64 = sec.key.parse().unwrap_or(0);
            // A per-library id: every Latest row is headed with its library's name, however many
            // libraries share a type (`catalog::hub_title`).
            let (family, type_num) = match sec.kind.as_str() {
                "show" => ("tv", 2),
                _ => ("movie", 1),
            };
            let items = latest.unwrap_or_default();
            if !items.is_empty() {
                out.push(JfShelf {
                    identifier: format!("{family}.recentlyadded.{section}"),
                    title: nj_platform::i18n::msg::browse_home_hub_recently_added_in(&sec.title),
                    key: format!("/hubs/home/recentlyAdded?type={type_num}&sectionID={section}"),
                    section,
                    library: sec.title,
                    items,
                });
            }
        }
        Some(out)
    }

    /// What the viewer's whole library looks like right now: its size and its newest item, the two
    /// facts a change moves (`stores::library_poll`). No images, no user data: this runs on a timer.
    pub fn library_signature(&self) -> Option<(i64, String)> {
        let r = self.items(Q::new("/Items").b("Recursive", true).s("IncludeItemTypes", "Movie,Series,Episode")
            .s("SortBy", "DateCreated").s("SortOrder", "Descending").i("Limit", 1)
            .b("EnableTotalRecordCount", true).b("EnableImages", false).b("EnableUserData", false))?;
        Some((r.total_record_count, r.items.first().map(|i| ids::normalize(&i.id)).unwrap_or_default()))
    }

    /// One library's genre rows, Litefin's Genres tab (`docs/litefin-look.md` 6): each genre the
    /// library lists (`/Items/Filters2`, the genres the section's filter offers, so their ids are
    /// the filter's) with its first `count` titles in title order, empty genres dropped. A row is
    /// named `{family}.genre.{section}.{genre id}` and keyed by the genre's interned id, the
    /// contract `browse::section_hubs::parse_jf_shelves` reads it by.
    ///
    /// The genres are read [`GENRE_LANES`] at a time rather than one after another (a library's
    /// reveal waits on these rows), and they land whole: one failed genre fails the read, so the
    /// section's backoff retries instead of committing a table with that row silently missing.
    pub fn library_shelves(&self, section_key: i64, count: i64) -> Option<Vec<JfShelf>> {
        /// Genre rows asked for: `browse::section_hubs`' shelf bound.
        const GENRE_ROWS: usize = 24;
        /// Genre reads in flight at once: the TV's thread ceiling, not the server, bounds it.
        const GENRE_LANES: usize = 6;
        let guid = ids::guid_of(section_key)?;
        let view = self.view(&guid);
        let family = if view.as_ref().is_some_and(|v| v.collection_type.as_deref() == Some("tvshows")) { "tv" } else { "movie" };
        let jf_type = convert::listing_item_types(None, view.as_ref());
        let filters: QueryFilters = self.get(&Q::new("/Items/Filters2").s("userId", &self.user_id()?)
            .s("ParentId", &guid).s("IncludeItemTypes", jf_type).build())?;
        let genres = &filters.genres[..filters.genres.len().min(GENRE_ROWS)];
        let mut out = Vec::new();
        for lane in genres.chunks(GENRE_LANES) {
            let reads = std::thread::scope(|s| {
                let asks: Vec<_> = lane.iter().map(|genre| {
                    let guid = guid.as_str();
                    nj_base::task::beside(s, "library-genre", move || self.items(Q::new("/Items").s("ParentId", guid)
                        .s("IncludeItemTypes", jf_type).b("Recursive", true).s("GenreIds", &genre.id)
                        .s("SortBy", "SortName").s("SortOrder", "Ascending").i("Limit", count).s("Fields", LIST_FIELDS)
                        .s("ImageTypeLimit", "1").s("EnableImageTypes", "Primary,Backdrop,Logo,Thumb")))
                }).collect();
                asks.into_iter().map(|ask| ask.finish()).collect::<Vec<_>>()
            });
            for (genre, r) in lane.iter().zip(reads) {
                let r = r?;
                if r.items.is_empty() { continue; }
                let id = ids::intern(&genre.id).to_string();
                out.push(JfShelf { identifier: format!("{family}.genre.{section_key}.{id}"), title: genre.name.clone(),
                    key: id, section: section_key, library: String::new(), items: r.items });
            }
        }
        Some(out)
    }

    /// One random title of a library, Litefin's library Shuffle (`LibraryPage.js:_handleShuffle`):
    /// one of what the library lists ([`convert::view_item_types`]: a series of a TV library, a
    /// film of a film library, a video of a Home Videos or Music Videos one), a film or an episode
    /// when the library could not be read, never a virtual (missing) item. `None` when the library
    /// has none or the server did not answer.
    pub fn library_random(&self, section_key: i64) -> Option<BaseItemDto> {
        let guid = ids::guid_of(section_key)?;
        let types = self.view(&guid).as_ref().and_then(convert::view_item_types).unwrap_or("Movie,Episode");
        self.items(Q::new("/Items").s("ParentId", &guid).i("Limit", 1).s("SortBy", "Random").b("Recursive", true)
            .s("IncludeItemTypes", types).s("ExcludeLocationTypes", "Virtual").s("Fields", ""))?.items.into_iter().next()
    }

    /// Continue Watching: the in-progress items, newest first. Next Up is its own row
    /// ([`Self::next_up_items`]), as Litefin keeps the two.
    pub fn continue_watching_items(&self, count: i64) -> Option<Vec<BaseItemDto>> {
        Some(self.items(Q::new("/UserItems/Resume").i("Limit", count).s("Fields", PLAYABLE_FIELDS)
            .s("MediaTypes", "Video").s("ImageTypeLimit", "1").s("EnableImageTypes", "Primary,Backdrop,Logo,Thumb"))?.items)
    }

    /// Next Up: the next unwatched episode of each series being watched in the last year
    /// (Litefin's and jellyfin-web's default `NextUpDateCutoff`). One in progress is Continue
    /// Watching's, so it is left out here.
    pub fn next_up_items(&self, count: i64) -> Option<Vec<BaseItemDto>> {
        const NEXT_UP_DAYS: u64 = 365;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let cutoff = iso_utc(now.saturating_sub(NEXT_UP_DAYS * 86_400));
        Some(self.items(Q::new("/Shows/NextUp").i("Limit", count).s("Fields", PLAYABLE_FIELDS)
            .b("EnableResumable", false).b("EnableRewatching", false).s("NextUpDateCutoff", &cutoff)
            .s("ImageTypeLimit", "1").s("EnableImageTypes", "Primary,Backdrop,Logo,Thumb"))?.items)
    }

    /// The hero's pool: `count` random films and series that have a backdrop, Litefin's
    /// `_loadHeroCarousel` query.
    pub fn hero_items(&self, count: i64) -> Option<Vec<BaseItemDto>> {
        Some(self.items(hero_query(count))?.items)
    }

    /// The identifier of [`Self::home_shelves`]' My Media row, whose items are libraries.
    pub const HOME_MY_MEDIA: &'static str = "home.mymedia";

    pub fn promoted(&self) -> Option<MediaContainer> {
        Some(MediaContainer::default())
    }

    // ---- search ---------------------------------------------------------------------------

    /// Titles matching `query`, grouped Movies / Shows / Episodes / Collections, and the people
    /// whose name matches it. Each group is its own `/Items` read of `limit` rows, asked together
    /// (as Litefin asks per type), so one type's many matches cannot fill the others' window. A
    /// group that fails fails the search; the people are optional.
    pub fn search_results(&self, query: &str, limit: i64) -> Option<JfSearch> {
        let limit = if limit > 0 { limit } else { 10 };
        let (groups, people) = std::thread::scope(|s| {
            let asks: Vec<_> = SEARCH_GROUPS.iter().map(|&(t, _, _)| {
                (t, nj_base::task::beside(s, "search-group", move || self.items(Q::new("/Items").s("searchTerm", query)
                    .b("Recursive", true).s("IncludeItemTypes", t).i("Limit", limit).s("Fields", LIST_FIELDS))))
            }).collect();
            let people: Option<QueryResult<BaseItemDto>> = self.get(&Q::new("/Persons").s("searchTerm", query)
                .i("Limit", limit).s("userId", &self.user_id().unwrap_or_default()).build());
            (asks.into_iter().map(|(t, ask)| Some((t, ask.finish()?.items))).collect::<Option<Vec<_>>>(), people)
        });
        Some(JfSearch { groups: groups?, people: people.map(|p| p.items).unwrap_or_default() })
    }

    /// [`Self::search_results`] in the hub shape the person page's identity resolution reads.
    pub fn search(&self, query: &str, limit: i64, _section_id: i64) -> Option<MediaContainer> {
        let found = self.search_results(query, limit)?;
        let mut hubs: Vec<Hub> = found.groups.iter().zip(SEARCH_GROUPS).map(|((_, rows), (_, kind, title))| {
            convert::hub(kind, title, kind, "", rows.iter().map(|i| convert::item(i, 0)).collect())
        }).collect();
        let tags: Vec<Tag> = found.people.iter().map(person_hit).collect();
        hubs.push(Hub {
            kind: "actor".into(),
            hub_identifier: "actor".into(),
            title: "Cast & Crew".into(),
            size: tags.len() as i64,
            total_size: tags.len() as i64,
            directory: tags,
            ..Default::default()
        });
        Some(MediaContainer { size: hubs.len() as i64, hub: hubs, ..Default::default() })
    }

    // ---- collections ----------------------------------------------------------------------

    /// The box sets of a library: 12.x lists those with members in it, 10.x every box set (it
    /// ignores `ParentId` for a lone BoxSet type). The rows are stamped with the library that asked.
    pub(crate) fn section_collections(&self, section: i64, start: i64, size: i64) -> CollectionOutcome {
        let parent = ids::guid_of(section).unwrap_or_default();
        let q = Q::new("/Items").s("IncludeItemTypes", "BoxSet").s("ParentId", &parent).b("Recursive", true).s("SortBy", "SortName")
            .s("Fields", LIST_FIELDS).b("EnableTotalRecordCount", true).i("StartIndex", start).i("Limit", size);
        self.collection_page(q, section)
    }

    pub(crate) fn collection(&self, rk: &str) -> CollectionOutcome {
        let Some(guid) = self.guid(rk) else { return CollectionOutcome::Missing };
        let Some(uid) = self.user_id() else { return CollectionOutcome::Transport };
        let (st, it) = self.get_status::<BaseItemDto>(&Q::new(format!("/Items/{guid}")).s("userId", &uid).build());
        collection_outcome(st, it.map(|i| convert::container(vec![convert::item(&i, 0)], 1, 0)))
    }

    /// One page of a BoxSet's members, in the order its owner set (`DisplayOrder`): the server
    /// sorts a box set's children itself, and only when the query names no `SortBy`.
    pub(crate) fn collection_members(&self, rk: &str, start: i64, size: i64) -> CollectionOutcome<QueryResult<BaseItemDto>> {
        let Some(guid) = self.guid(rk) else { return CollectionOutcome::Missing };
        let mut q = Q::new("/Items").s("ParentId", &guid).s("Fields", LIST_FIELDS)
            .b("EnableTotalRecordCount", true).i("StartIndex", start);
        if size > 0 {
            q = q.i("Limit", size);
        }
        self.collection_query(q)
    }

    fn collection_query(&self, q: Q) -> CollectionOutcome<QueryResult<BaseItemDto>> {
        let q = match self.user_id() {
            Some(uid) => q.s("userId", &uid),
            None => return CollectionOutcome::Transport,
        };
        let (st, r) = self.get_status::<QueryResult<BaseItemDto>>(&q.build());
        collection_outcome(st, r)
    }

    fn collection_page(&self, q: Q, section: i64) -> CollectionOutcome {
        match self.collection_query(q) {
            CollectionOutcome::Ok(r) => {
                let rows = r.items.iter().map(|i| convert::item(i, section)).collect();
                CollectionOutcome::Ok(convert::container(rows, r.total_record_count, r.start_index))
            }
            CollectionOutcome::Denied => CollectionOutcome::Denied,
            CollectionOutcome::Missing => CollectionOutcome::Missing,
            CollectionOutcome::Transport => CollectionOutcome::Transport,
        }
    }

    // ---- subtitles ------------------------------------------------------------------------

    /// An external subtitle stream (the key [`convert::stream`] built) as UTF-8 text; ASS/SSA keep
    /// their own format, everything else is asked for as SubRip.
    pub fn sidecar_subtitle(&self, key: &str, codec: &str) -> Option<Vec<u8>> {
        if !key.starts_with("/Videos/") || !key.contains("/Subtitles/") {
            return None;
        }
        let path = if codec.eq_ignore_ascii_case("ass") || codec.eq_ignore_ascii_case("ssa") {
            key.to_string()
        } else {
            match key.rsplit_once("/Stream.") {
                Some((head, _)) => format!("{head}/Stream.srt"),
                None => key.to_string(),
            }
        };
        let r = self.send(&path, Method::Get, None)?;
        (r.ok() && !r.body.is_empty() && r.body.len() <= crate::catalog::SIDECAR_MAX_BYTES).then_some(r.body)
    }
}

/// A person search hit as the search shelf's tag row.
fn person_hit(p: &BaseItemDto) -> Tag {
    Tag {
        tag: p.name.clone(),
        id: ids::intern(&p.id),
        tag_key: ids::normalize(&p.id),
        thumb: super::images::person_thumb(p),
        ..Default::default()
    }
}

fn collection_outcome<T>(status: Option<i32>, page: Option<T>) -> CollectionOutcome<T> {
    match (status, page) {
        (Some(401 | 403), _) => CollectionOutcome::Denied,
        (Some(404), _) => CollectionOutcome::Missing,
        (Some(200..=299), Some(p)) => CollectionOutcome::Ok(p),
        _ => CollectionOutcome::Transport,
    }
}

/// A listing's filter terms (`SecState::query_filters`), as Jellyfin says them. The grid's page and
/// its letter counts both pass through here, so they cannot count different things.
fn with_listing_filters(mut b: Q, filters: &[(String, String)]) -> Q {
    for (k, v) in filters {
        b = match k.as_str() {
            "unwatched" | "unwatchedLeaves" if v == "1" => b.s("Filters", "IsUnplayed"),
            "genre" => match v.parse::<i64>().ok().and_then(ids::guid_of) {
                Some(g) => b.s("GenreIds", &g),
                None => b.s("Genres", v),
            },
            "firstCharacter" => b.s("NameStartsWith", v),
            _ => b,
        };
    }
    b
}

/// [`Jf::hero_items`]' request: random films and series that carry a backdrop. `ImageTypes` is
/// the `/Items` filter on the images an item has; `Filters` takes only `ItemFilter` values, and
/// the server's comma-delimited binder silently drops anything else.
fn hero_query(count: i64) -> Q {
    Q::new("/Items").s("SortBy", "Random").b("Recursive", true).i("Limit", count)
        .s("IncludeItemTypes", "Movie,Series").s("ImageTypes", "Backdrop").s("Fields", LIST_FIELDS)
        .s("ImageTypeLimit", "1").s("EnableImageTypes", "Primary,Backdrop,Logo")
}

/// Unix seconds as an ISO 8601 UTC instant (`2025-11-21T08:00:00Z`), Howard Hinnant's
/// civil-from-days: the inverse of [`convert::iso_to_unix`].
fn iso_utc(secs: u64) -> String {
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rem / 3_600, rem % 3_600 / 60, rem % 60)
}

/// One row of [`Jf::find_by_guid`]'s sweep: all it reads of each title, so a library-wide sweep
/// holds two fields per item rather than a whole `BaseItemDto` (the C1 is a 32-bit TV).
#[derive(Default, serde::Deserialize)]
#[serde(default, rename_all = "PascalCase")]
struct ProviderIdsRow {
    id: String,
    provider_ids: std::collections::HashMap<String, String>,
}

fn provider_key(scheme: &str) -> &'static str {
    match scheme {
        "imdb" => "Imdb",
        "tmdb" => "Tmdb",
        _ => "Tvdb",
    }
}

fn parse<T: DeserializeOwned>(path: &str, body: &[u8]) -> Option<T> {
    match serde_json::from_slice::<T>(body) {
        Ok(v) => Some(v),
        Err(e) => {
            nj_base::eventlog::log(&format!(
                "jf: GET {} answered {} bytes that will not parse — {e}",
                path.split('?').next().unwrap_or(path),
                body.len()
            ));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plex_sort_expressions_translate_to_items_sorts() {
        assert_eq!(sort_by("titleSort:asc"), ("SortName".to_string(), "Ascending"));
        assert_eq!(sort_by("addedAt:desc"), ("DateCreated,SortName".to_string(), "Descending"));
        assert_eq!(sort_by("showOrder:desc").0, "SeriesSortName,ParentIndexNumber,IndexNumber,SortName");
        assert_eq!(sort_by("random").0, "Random");
        assert_eq!(sort_by("").0, "SortName");
        assert_eq!(sort_by("nonsense:desc"), ("SortName".to_string(), "Descending"));
    }

    #[test]
    fn the_query_builder_encodes_values_and_skips_empty_ones() {
        assert_eq!(Q::new("/Items").s("searchTerm", "tom & jerry").s("ParentId", "").i("Limit", 5).build(),
            "/Items?searchTerm=tom%20%26%20jerry&Limit=5");
        assert_eq!(Q::new("/x?a=1").b("Recursive", true).build(), "/x?a=1&Recursive=true");
    }

    /// The hero asks the server for items WITH a backdrop (`ImageTypes=Backdrop`), so its five
    /// random slides are five billboards; `Filters` has no backdrop value for the server to honour.
    #[test]
    fn the_hero_pool_asks_for_items_that_have_a_backdrop() {
        let q = hero_query(5).build();
        assert!(q.contains("ImageTypes=Backdrop"), "{q}");
        assert!(!q.contains("Filters="), "{q}");
        assert!(q.contains("SortBy=Random") && q.contains("Limit=5") && q.contains("IncludeItemTypes=Movie%2CSeries"), "{q}");
    }

    /// A loopback Jellyfin with one film library and two genres, the second of whose `/Items`
    /// read fails while `fail_drama` holds. Records each path.
    fn genre_server(collection: &'static str, fail_drama: bool) -> (i32, std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        std::sync::Arc<std::sync::atomic::AtomicBool>, std::thread::JoinHandle<()>) {
        use std::io::{BufRead, BufReader, Write};
        use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port() as i32;
        listener.set_nonblocking(true).unwrap();
        let (stop, paths) = (Arc::new(AtomicBool::new(false)), Arc::new(Mutex::new(Vec::new())));
        let (stopped, seen) = (stop.clone(), paths.clone());
        let join = std::thread::spawn(move || while !stopped.load(Ordering::Relaxed) {
            match nj_base::testnet::accept(&listener) {
                Ok((mut socket, _)) => {
                    socket.set_nonblocking(false).unwrap();
                    let mut reader = BufReader::new(socket.try_clone().unwrap());
                    let mut request = String::new();
                    reader.read_line(&mut request).unwrap();
                    loop { let mut header = String::new(); reader.read_line(&mut header).unwrap();
                        if header == "\r\n" || header.is_empty() { break; } }
                    let path = request.split_whitespace().nth(1).unwrap().to_string();
                    let body = if path.starts_with("/UserViews") {
                        Some(serde_json::json!({"Items": [{"Id": genre_guid(1), "Name": "Films", "CollectionType": collection}]}))
                    } else if path.starts_with("/Items/Filters2") {
                        Some(serde_json::json!({"Genres": [{"Name": "Action", "Id": genre_guid(7)}, {"Name": "Drama", "Id": genre_guid(9)}]}))
                    } else if fail_drama && path.contains(&genre_guid(9)) {
                        None
                    } else {
                        Some(serde_json::json!({"Items": [{"Id": genre_guid(20), "Name": "Synthetic", "Type": "Movie"}], "TotalRecordCount": 1}))
                    };
                    seen.lock().unwrap().push(path);
                    match body {
                        Some(body) => { let body = body.to_string();
                            write!(socket, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap(); }
                        None => write!(socket, "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap(),
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(std::time::Duration::from_millis(2)),
                Err(e) => panic!("accept: {e}"),
            }
        });
        (port, paths, stop, join)
    }
    fn genre_guid(n: u32) -> String { format!("6e7e{n:09x}0000000000000000000") }

    /// R3.3: a library's genre rows are one answer. Every genre answers, in the server's order, or
    /// the read fails as a whole, so the section's backoff retries rather than committing a table
    /// with a genre silently missing.
    #[test]
    fn the_genre_rows_land_whole_or_not_at_all() {
        let _g = nj_base::testlock::serial();
        for fail_drama in [false, true] {
            let (port, paths, stop, join) = genre_server("movies", fail_drama);
            let sid = crate::catalog::register_for_test("genre-rows", "127.0.0.1", port, "fixture", "fixture");
            let client = crate::catalog::client_for(sid).unwrap();
            super::super::seat::register_with(client.origin(), super::super::seat::Seat { user_id: genre_guid(99), ..Default::default() });
            let key = ids::intern(&genre_guid(1));
            let rows = client.jf().unwrap().library_shelves(key, 4);
            super::super::seat::forget(client.origin());
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
            join.join().unwrap();
            if fail_drama {
                assert!(rows.is_none(), "a genre whose read failed fails the rows: {:?}",
                    rows.map(|rows| rows.into_iter().map(|row| row.title).collect::<Vec<_>>()));
            } else {
                let titles: Vec<_> = rows.expect("every genre answered").into_iter().map(|row| row.title).collect();
                assert_eq!(titles, ["Action", "Drama"], "the rows keep the server's genre order");
            }
            assert_eq!(paths.lock().unwrap().iter().filter(|p| p.contains("GenreIds=")).count(), 2);
        }
    }

    /// R3.4: Shuffle is a pick over the whole library, made by the server: one random series of a
    /// TV library, one film of a film library, one video of a Home Videos or Music Videos library,
    /// never a missing (virtual) item.
    #[test]
    fn a_library_shuffle_asks_the_server_for_one_title_of_the_librarys_kind() {
        let _g = nj_base::testlock::serial();
        for (collection, types) in [("tvshows", "IncludeItemTypes=Series&"), ("movies", "IncludeItemTypes=Movie&"),
            ("musicvideos", "IncludeItemTypes=MusicVideo&"), ("homevideos", "IncludeItemTypes=Video&")]
        {
            let (port, paths, stop, join) = genre_server(collection, false);
            let sid = crate::catalog::register_for_test("library-random", "127.0.0.1", port, "fixture", "fixture");
            let client = crate::catalog::client_for(sid).unwrap();
            super::super::seat::register_with(client.origin(), super::super::seat::Seat { user_id: genre_guid(99), ..Default::default() });
            let picked = client.jf().unwrap().library_random(ids::intern(&genre_guid(1)));
            super::super::seat::forget(client.origin());
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
            join.join().unwrap();
            assert_eq!(picked.map(|item| item.id), Some(genre_guid(20)));
            let paths = paths.lock().unwrap();
            let pick = paths.iter().find(|p| p.contains("SortBy=Random")).expect("a random pick");
            assert!(pick.contains(&format!("ParentId={}", genre_guid(1))) && pick.contains(types) && pick.contains("Limit=1")
                && pick.contains("Recursive=true") && pick.contains("ExcludeLocationTypes=Virtual"), "{pick}");
        }
    }

    /// A loopback Jellyfin answering every request with `route(path)`'s status and JSON body, and
    /// recording each path. Its client is signed in as `genre_guid(99)`; dropping it stops the server.
    pub(crate) struct Fake {
        paths: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        join: Option<std::thread::JoinHandle<()>>,
        pub(crate) client: &'static Client,
    }

    impl Fake {
        pub(crate) fn new(name: &str, route: impl Fn(&str) -> (u16, serde_json::Value) + Send + 'static) -> Self {
            use std::io::{BufRead, BufReader, Write};
            use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port() as i32;
            listener.set_nonblocking(true).unwrap();
            let (stop, paths) = (Arc::new(AtomicBool::new(false)), Arc::new(Mutex::new(Vec::new())));
            let (stopped, seen) = (stop.clone(), paths.clone());
            let join = std::thread::spawn(move || while !stopped.load(Ordering::Relaxed) {
                match nj_base::testnet::accept(&listener) {
                    Ok((mut socket, _)) => {
                        socket.set_nonblocking(false).unwrap();
                        let mut reader = BufReader::new(socket.try_clone().unwrap());
                        let mut request = String::new();
                        reader.read_line(&mut request).unwrap();
                        loop { let mut header = String::new(); reader.read_line(&mut header).unwrap();
                            if header == "\r\n" || header.is_empty() { break; } }
                        let path = request.split_whitespace().nth(1).unwrap().to_string();
                        let (status, body) = route(&path);
                        seen.lock().unwrap().push(path);
                        let body = body.to_string();
                        write!(socket, "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(std::time::Duration::from_millis(2)),
                    Err(e) => panic!("accept: {e}"),
                }
            });
            let sid = crate::catalog::register_for_test(name, "127.0.0.1", port, "fixture", "fixture");
            let client = crate::catalog::client_for(sid).unwrap();
            super::super::seat::register_with(client.origin(), super::super::seat::Seat { user_id: genre_guid(99), ..Default::default() });
            Self { paths, stop, join: Some(join), client }
        }

        pub(crate) fn jf(&self) -> Jf<'static> {
            self.client.jf().unwrap()
        }

        pub(crate) fn paths(&self) -> Vec<String> {
            self.paths.lock().unwrap().clone()
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            super::super::seat::forget(self.client.origin());
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            self.join.take().unwrap().join().unwrap();
        }
    }

    pub(crate) fn page(items: Vec<serde_json::Value>) -> (u16, serde_json::Value) {
        let n = items.len();
        (200, serde_json::json!({"Items": items, "TotalRecordCount": n}))
    }

    /// The value of query parameter `key` in `path`, still percent-encoded.
    pub(crate) fn param<'p>(path: &'p str, key: &str) -> Option<&'p str> {
        path.split(['?', '&']).find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
    }

    /// Next Up leaves out a series last watched more than a year ago, as Litefin and jellyfin-web
    /// do (`NextUpDateCutoff`, their 365-day default); the server keeps it forever otherwise.
    #[test]
    fn next_up_leaves_out_series_abandoned_for_a_year() {
        let _g = nj_base::testlock::serial();
        let fake = Fake::new("next-up-cutoff", |_| page(Vec::new()));
        fake.jf().next_up_items(12).expect("an answer");
        let path = fake.paths().into_iter().find(|p| p.starts_with("/Shows/NextUp")).expect("a Next Up read");
        let cutoff = param(&path, "NextUpDateCutoff").unwrap_or_else(|| panic!("no cutoff: {path}")).replace("%3A", ":");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let age = now - convert::iso_to_unix(&cutoff);
        assert!((age - 365 * 86_400).abs() <= 86_400, "{cutoff} is {age} s back");
        for t in [63_072_000, 951_782_400, 1_763_712_000, 4_107_542_399] {
            assert_eq!(convert::iso_to_unix(&iso_utc(t as u64)), t, "{}", iso_utc(t as u64));
        }
        assert_eq!(iso_utc(951_782_400), "2000-02-29T00:00:00Z");
    }

    /// A library the user hid from Latest (jellyfin-web's Home settings) gets no "Recently added"
    /// row. The server applies `LatestItemsExcludes` only to an unscoped `/Items/Latest`, and Home
    /// asks per library, so the client skips it; My Media still lists it.
    #[test]
    fn a_library_hidden_from_latest_gets_no_latest_row() {
        let _g = nj_base::testlock::serial();
        let (films, shows) = (genre_guid(1), genre_guid(2));
        let hidden = shows.clone();
        let fake = Fake::new("latest-excludes", move |path| {
            if path.starts_with("/UserViews") {
                page(vec![
                    serde_json::json!({"Id": genre_guid(1), "Name": "Films", "Type": "CollectionFolder", "CollectionType": "movies"}),
                    serde_json::json!({"Id": genre_guid(2), "Name": "Shows", "Type": "CollectionFolder", "CollectionType": "tvshows"}),
                ])
            } else if path.starts_with("/Users/Me") {
                (200, serde_json::json!({"Id": genre_guid(99), "Configuration": {"LatestItemsExcludes": [hidden]}}))
            } else if path.starts_with("/Items/Latest") {
                (200, serde_json::json!([{"Id": genre_guid(20), "Name": "Synthetic", "Type": "Movie"}]))
            } else {
                (404, serde_json::json!({}))
            }
        });
        let shelves = fake.jf().home_shelves(12).expect("shelves");
        let ids: Vec<_> = shelves.iter().map(|s| s.identifier.clone()).collect();
        assert_eq!(ids, [Jf::HOME_MY_MEDIA.to_string(), format!("movie.recentlyadded.{}", ids::intern(&films))]);
        assert_eq!(shelves[0].items.len(), 2, "My Media keeps the hidden library");
        assert!(!fake.paths().iter().any(|p| p.starts_with("/Items/Latest") && p.contains(&shows)), "{:?}", fake.paths());
    }

    /// Each search group asks for its own rows: one `/Items` per type, `limit` each, so a word
    /// that matches many episodes cannot crowd the films and shows out of one shared window.
    #[test]
    fn every_search_group_asks_for_its_own_rows() {
        let _g = nj_base::testlock::serial();
        let fake = Fake::new("search-groups", |path| {
            if path.starts_with("/Persons") {
                return page(Vec::new());
            }
            let types = param(path, "IncludeItemTypes").unwrap_or("");
            // A combined query answers its first rows by name: all episodes here.
            let kind = if types.contains("%2C") { "Episode" } else { types }.to_string();
            let limit: u32 = param(path, "Limit").and_then(|v| v.parse().ok()).unwrap_or(0);
            page((0..limit).map(|n| serde_json::json!({"Id": genre_guid(100 + n), "Name": "Love", "Type": kind})).collect())
        });
        let found = fake.jf().search_results("love", 12).expect("an answer");
        assert_eq!(found.groups.iter().map(|(t, rows)| (*t, rows.len())).collect::<Vec<_>>(),
            [("Movie", 12), ("Series", 12), ("Episode", 12), ("BoxSet", 12)]);
        let reads: Vec<_> = fake.paths().into_iter().filter(|p| p.starts_with("/Items?")).collect();
        assert_eq!(reads.len(), 4, "{reads:?}");
        assert!(reads.iter().all(|p| param(p, "Limit") == Some("12") && param(p, "searchTerm") == Some("love")), "{reads:?}");
    }

    /// The Collections tab and its A-Z rail read the same set: both scoped to the library. 12.x
    /// lists the box sets with members there; 10.x ignores `ParentId` for a lone BoxSet type.
    #[test]
    fn the_collections_tab_lists_the_librarys_own_box_sets() {
        let _g = nj_base::testlock::serial();
        let fake = Fake::new("collections-tab", |_| page(Vec::new()));
        let library = genre_guid(1);
        let key = ids::intern(&library);
        let filters = [("type".to_string(), "18".to_string())];
        fake.jf().section_page(&SectionQuery { section_key: key, sort: "", filters: &filters, start: 0, size: 50, include_meta: false })
            .expect("a page");
        fake.jf().section_directory(key, "firstCharacter", Some(18), &[]).expect("letters");
        let _ = fake.jf().section_collections(key, 0, 50);
        let reads: Vec<_> = fake.paths().into_iter().filter(|p| param(p, "IncludeItemTypes") == Some("BoxSet")).collect();
        assert_eq!(reads.len(), 3, "{reads:?}");
        assert!(reads.iter().all(|p| param(p, "ParentId") == Some(library.as_str())), "{reads:?}");
    }

    /// A box set's members come in the order its owner set (`DisplayOrder`: release date, sort
    /// name, or the manual order), which the server applies only when the client sends no sort.
    #[test]
    fn a_box_set_keeps_its_own_order() {
        let _g = nj_base::testlock::serial();
        let fake = Fake::new("box-set-order", |_| page(Vec::new()));
        let rk = ids::intern(&genre_guid(30)).to_string();
        assert!(matches!(fake.jf().collection_members(&rk, 0, 24), CollectionOutcome::Ok(_)));
        let path = fake.paths().into_iter().find(|p| p.contains("ParentId=")).expect("a members read");
        assert!(param(&path, "SortBy").is_none(), "{path}");
    }

    /// A person is favourited by the id their page holds, which may be their raw GUID (a key a
    /// person read never minted), as the person's items are read.
    #[test]
    fn a_person_is_favourited_by_their_raw_guid() {
        let _g = nj_base::testlock::serial();
        let fake = Fake::new("person-favourite", |_| (200, serde_json::json!({})));
        let person = genre_guid(41);
        assert!(fake.jf().set_favorite(&person, true));
        assert!(fake.paths().iter().any(|p| p.starts_with(&format!("/UserFavoriteItems/{person}?"))), "{:?}", fake.paths());
    }

    /// A person's page asks for their episodes too: a guest star is credited on the episode alone.
    #[test]
    fn a_persons_items_include_their_episodes() {
        let _g = nj_base::testlock::serial();
        let fake = Fake::new("person-items", |_| page(Vec::new()));
        fake.jf().person_items(&genre_guid(40)).expect("an answer");
        let path = fake.paths().into_iter().find(|p| p.contains("PersonIds=")).expect("a person read");
        assert_eq!(param(&path, "IncludeItemTypes"), Some("Movie%2CSeries%2CEpisode"), "{path}");
    }

    /// "Also available" finds another server's copy by its provider id however far down the
    /// server's list it sits: `/Items` has no provider-id filter (`AnyProviderIdEquals` is not a
    /// parameter), so ten arbitrary IMDb-tagged rows were all the lookup ever saw.
    #[test]
    fn a_copy_is_found_by_provider_id_past_the_first_ten() {
        let _g = nj_base::testlock::serial();
        let fake = Fake::new("find-by-guid", |path| {
            if param(path, "Ids").is_some() {
                return page(vec![serde_json::json!({"Id": genre_guid(111), "Name": "Wanted", "Type": "Movie",
                    "ProviderIds": {"Imdb": "tt0000011"}})]);
            }
            // The IMDb-tagged items by name, honouring `Limit` like the server.
            let limit: usize = param(path, "Limit").and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);
            page((0..12u32).take(limit).map(|n| serde_json::json!({"Id": genre_guid(100 + n), "Name": "Synthetic",
                "Type": "Movie", "ProviderIds": {"Imdb": format!("tt{n:07}")}})).collect())
        });
        let found = fake.jf().find_by_guid("imdb://tt0000011", "movie").expect("an answer");
        assert_eq!(found.metadata.iter().map(|m| m.rating_key.clone()).collect::<Vec<_>>(),
            [ids::intern(&genre_guid(111)).to_string()]);
        assert!(!fake.paths().iter().any(|p| p.contains("AnyProviderIdEquals")), "{:?}", fake.paths());
    }

    /// **A provider id names a title only within its type.** TMDb numbers films, shows and episodes
    /// separately, and TVDB shows and episodes, so an episode marked watched on one server must not
    /// mark the film, or every episode of the show, that another server files under the same number.
    #[test]
    fn a_copy_is_matched_only_among_items_of_the_origins_type() {
        let _g = nj_base::testlock::serial();
        let fake = Fake::new("find-by-guid-type", |path| {
            let rows = [(120, "Movie"), (121, "Series"), (122, "Episode")].into_iter();
            if let Some(ids) = param(path, "Ids") {
                return page(rows.filter(|(n, _)| ids.contains(&genre_guid(*n)))
                    .map(|(n, t)| serde_json::json!({"Id": genre_guid(n), "Name": t, "Type": t})).collect());
            }
            // The sweep, honouring `IncludeItemTypes` like the server: one id, three types.
            let types = param(path, "IncludeItemTypes").unwrap_or("");
            page(rows.filter(|(_, t)| types.split("%2C").any(|w| w == *t))
                .map(|(n, t)| serde_json::json!({"Id": genre_guid(n), "Type": t, "ProviderIds": {"Tmdb": "603"}})).collect())
        });
        for (kind, n) in [("episode", 122), ("movie", 120), ("show", 121)] {
            let found = fake.jf().find_by_guid("tmdb://603", kind).expect("an answer");
            assert_eq!(found.metadata.iter().map(|m| m.rating_key.clone()).collect::<Vec<_>>(),
                [ids::intern(&genre_guid(n)).to_string()], "{kind}");
        }
    }

    #[test]
    fn every_advertised_sort_is_one_the_translator_knows() {
        for t in ["Movie", "Series", "Episode", "Season"] {
            for s in &sort_menu(t).types[0].sort {
                let (fields, _) = sort_by(&format!("{}:asc", s.key));
                assert!(fields != "SortName" || s.key == "titleSort", "{} unmapped", s.key);
            }
        }
    }
}
