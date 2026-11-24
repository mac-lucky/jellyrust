//! The two lists behind Litefin's rail items of the same names: the user's favourites, one list
//! per item type (`pages/FavoritesPage.js:_loadFavorites`), and one random title
//! (`api/ApiClient.js:getRandomItem`, `components/Sidebar.js:_onRandomClick`). The queries are
//! Litefin's own, minus the music, book and Live TV lists, which this app has no page for.
use super::api::{Jf, Q};
use super::models::{BaseItemDto, QueryResult};

/// The favourites lists, in the order Litefin stacks their rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Favorite {
    Movies,
    Series,
    Seasons,
    Episodes,
    People,
}

impl Favorite {
    pub(crate) const ALL: [Favorite; 5] = [Self::Movies, Self::Series, Self::Seasons, Self::Episodes, Self::People];

    /// `(path, IncludeItemTypes, SortBy, SortOrder, Fields)` as `FavoritesPage.js` asks, plus the
    /// episode counts a show's or season's watched check and unwatched badge are read from
    /// (`catalog_fetch::jf_row`), which the server sends only when asked.
    fn query(self) -> (&'static str, &'static str, &'static str, &'static str, &'static str) {
        match self {
            Self::Movies => ("/Items", "Movie", "SortName", "Ascending", "DateCreated,ProductionYear,MediaSourceCount"),
            Self::Series => ("/Items", "Series", "SortName", "Ascending",
                "ProductionYear,UnplayedItemCount,UserData,MediaSourceCount,RecursiveItemCount,ChildCount"),
            Self::Seasons => ("/Items", "Season", "SortName", "Ascending",
                "ParentTitle,ProductionYear,UnplayedItemCount,UserData,RecursiveItemCount,ChildCount"),
            Self::Episodes => ("/Items", "Episode", "DateCreated", "Descending", "ParentTitle,Overview,RunTimeTicks,IndexNumber,ParentIndexNumber"),
            Self::People => ("/Persons", "", "SortName", "Ascending", ""),
        }
    }
}

/// Litefin's per-list cap.
const LIMIT: i64 = 50;

impl Jf<'_> {
    /// Every favourites list, in [`Favorite::ALL`]'s order; `None` when any request failed, as
    /// Litefin's `Promise.all` fails the whole page.
    pub(crate) fn favorites(&self) -> Option<Vec<(Favorite, Vec<BaseItemDto>)>> {
        let user = self.user_id()?;
        Favorite::ALL.into_iter().map(|kind| {
            let (path, types, sort, order, fields) = kind.query();
            let q = Q::new(path).s("userId", &user).s("Filters", "IsFavorite").s("IncludeItemTypes", types)
                .b("Recursive", true).s("SortBy", sort).s("SortOrder", order).i("Limit", LIMIT)
                .s("Fields", fields).i("ImageTypeLimit", 1).s("EnableImageTypes", "Primary,Backdrop,Thumb");
            self.get::<QueryResult<BaseItemDto>>(&q.build()).map(|page| (kind, page.items))
        }).collect()
    }

    /// One random film or series of the user's libraries; `None` when the server has none or did
    /// not answer.
    pub(crate) fn random_title(&self) -> Option<BaseItemDto> {
        let user = self.user_id()?;
        let q = Q::new("/Items").s("userId", &user).s("IncludeItemTypes", "Movie,Series").s("SortBy", "Random")
            .i("Limit", 1).b("Recursive", true).s("Fields", "");
        self.get::<QueryResult<BaseItemDto>>(&q.build())?.items.into_iter().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};

    fn guid(n: u32) -> String { format!("f00d{n:09x}0000000000000000000") }

    /// A loopback server answering the way `tests/mock_jf.py --demo` does for these queries: the
    /// favourites by type (`DemoJf.favourites`), and a fixed "random" title. Records each path.
    struct Server { port: i32, paths: Arc<Mutex<Vec<String>>>, stop: Arc<AtomicBool>, join: Option<std::thread::JoinHandle<()>> }

    impl Server {
        fn new(empty: bool) -> Self {
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
                        let kind = ["Movie", "Series", "Season", "Episode"].into_iter()
                            .position(|t| path.contains(&format!("IncludeItemTypes={t}&")));
                        let items: Vec<_> = if empty { Vec::new() } else if path.contains("SortBy=Random") {
                            vec![serde_json::json!({"Id": guid(7), "Name": "Synthetic random", "Type": "Movie"})]
                        } else if path.starts_with("/Persons") {
                            vec![serde_json::json!({"Id": guid(90), "Name": "Synthetic person", "Type": "Person"})]
                        } else {
                            let n = kind.map_or(0, |k| k as u32 + 1);
                            (0..n).map(|i| serde_json::json!({"Id": guid(n * 10 + i), "Name": "Synthetic", "Type": "Movie"})).collect()
                        };
                        seen.lock().unwrap().push(path);
                        let body = serde_json::json!({"Items": items, "TotalRecordCount": items.len()}).to_string();
                        write!(socket, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(std::time::Duration::from_millis(2)),
                    Err(e) => panic!("accept: {e}"),
                }
            });
            Self { port, paths, stop, join: Some(join) }
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            self.join.take().unwrap().join().unwrap();
        }
    }

    fn client(server: &Server) -> &'static crate::catalog::Client {
        let sid = crate::catalog::register_for_test("favorites-fixture", "127.0.0.1", server.port, "fixture", "fixture");
        let client = crate::catalog::client_for(sid).unwrap();
        super::super::seat::register_with(client.origin(), super::super::seat::Seat { user_id: guid(99), ..Default::default() });
        client
    }

    /// Litefin's five favourites queries, in its row order, each scoped to the user's favourites
    /// and capped at 50, with episodes newest first.
    #[test]
    fn favourites_are_asked_per_type_in_litefins_order() {
        let _g = nj_base::testlock::serial();
        let server = Server::new(false);
        let client = client(&server);
        let lists = client.jf().unwrap().favorites().expect("every list answered");
        assert_eq!(lists.iter().map(|(k, items)| (*k, items.len())).collect::<Vec<_>>(),
            vec![(Favorite::Movies, 1), (Favorite::Series, 2), (Favorite::Seasons, 3), (Favorite::Episodes, 4), (Favorite::People, 1)]);
        let paths = server.paths.lock().unwrap().clone();
        assert_eq!(paths.len(), 5);
        assert!(paths.iter().all(|p| p.contains("Filters=IsFavorite") && p.contains("Limit=50")
            && p.contains(&format!("userId={}", guid(99)))), "{paths:?}");
        // A show's or season's watched check and unwatched count come from its episode count, which
        // the server sends only when asked (`jf_row::row`).
        for p in &paths[1..3] {
            assert!(p.contains("RecursiveItemCount") && p.contains("ChildCount"), "leaf counts asked: {p}");
        }
        assert!(paths[3].contains("SortBy=DateCreated&SortOrder=Descending"), "episodes newest first: {}", paths[3]);
        assert!(paths[4].starts_with("/Persons?"), "people come from /Persons: {}", paths[4]);
        super::super::seat::forget(client.origin());
    }

    /// Random asks for ONE film or series in random order and answers it, or nothing when the
    /// server has none.
    #[test]
    fn random_asks_for_one_film_or_series() {
        let _g = nj_base::testlock::serial();
        let server = Server::new(false);
        let client = client(&server);
        let picked = client.jf().unwrap().random_title().expect("a title");
        assert_eq!(picked.id, guid(7));
        let path = server.paths.lock().unwrap()[0].clone();
        assert!(path.contains("IncludeItemTypes=Movie%2CSeries") && path.contains("SortBy=Random")
            && path.contains("Limit=1") && path.contains("Recursive=true"), "{path}");
        super::super::seat::forget(client.origin());
        let empty = Server::new(true);
        let client = self::client(&empty);
        assert!(client.jf().unwrap().random_title().is_none(), "an empty server has nothing to open");
        super::super::seat::forget(client.origin());
    }
}
