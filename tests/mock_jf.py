#!/usr/bin/env python3
"""A small Jellyfin REST mock for JellyRust (password + Quick Connect + browse + PlaybackInfo).

Synthetic item ids are 32-hex strings. Titles stay in the closed alphabet (`s` + 8 hex digits)
so committed fixtures remain scrub-free. Serve until Ctrl-C:

    python3 tests/mock_jf.py --port 8096
    python3 tests/mock_jf.py --port 8096 --demo   # the demo library, artwork and all

`--demo` serves the openly licensed demo library behind `make screenshots` instead (`DemoJf`,
below; needs `make demo-library` once), for screenshots of the Jellyfin path without a real
server (tools/litefin-shot.sh, docs/litefin-shots.md). Nothing recorded against it may be
committed as a fixture: its titles are real.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import pathlib
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

USER_ID = "a" * 32
SERVER_ID = "b" * 32
TOKEN = "mock-jf-token"
MOVIE_ID = "c" * 32
SHOW_ID = "d" * 32
LIBRARY_ID = "e" * 32
MIXED_ID = "f" * 32
HOME_VIDEOS_ID = "9" * 32
VIDEO_ID = "8" * 32


def item(iid: str, itype: str, name: str, **extra) -> dict:
    row = {
        "Id": iid,
        "Name": name,
        "Type": itype,
        "ServerId": SERVER_ID,
        "ImageTags": {"Primary": "tag1"},
        "UserData": {"PlaybackPositionTicks": 0, "PlayCount": 0, "Played": False},
        "RunTimeTicks": 5_400_000_0000,
        "ProviderIds": {},
    }
    row.update(extra)
    return row


CATALOG = [
    item(LIBRARY_ID, "CollectionFolder", "s11111111", CollectionType="movies"),
    item(MOVIE_ID, "Movie", "s3fa90c12", ProductionYear=2011, PremiereDate="2011-03-14"),
    item(SHOW_ID, "Series", "s8bb12c00", ChildCount=1),
    item(VIDEO_ID, "Video", "s5d0e7a21"),
]

# The libraries /UserViews lists, and what each holds. A "Mixed Movies and Shows" library is a
# CollectionFolder with NO CollectionType (issue #35); a Home Videos one holds Video items (#38).
VIEWS = [
    CATALOG[0],
    item(MIXED_ID, "CollectionFolder", "s22222222"),
    item(HOME_VIDEOS_ID, "CollectionFolder", "s33333333", CollectionType="homevideos"),
]
CONTENTS = {
    LIBRARY_ID: [CATALOG[1]],
    MIXED_ID: [CATALOG[1], CATALOG[2], CATALOG[3]],
    HOME_VIDEOS_ID: [CATALOG[3]],
}


# The user's configuration: GET /Users/Me serves it and POST /Users/Configuration replaces it
# whole, as the real server does (Settings > Audio & subtitles, jf/preferences.rs).
CONFIG = {
    "AudioLanguagePreference": "eng",
    "PlayDefaultAudioTrack": True,
    "SubtitleLanguagePreference": "",
    "SubtitleMode": "Default",
    "OrderedViews": [],
    "HidePlayedInLatest": True,
    "EnableNextEpisodeAutoPlay": True,
}

CULTURES = [
    {"Name": "English", "DisplayName": "English", "TwoLetterISOLanguageName": "en",
     "ThreeLetterISOLanguageName": "eng", "ThreeLetterISOLanguageNames": ["eng"]},
    {"Name": "French", "DisplayName": "French", "TwoLetterISOLanguageName": "fr",
     "ThreeLetterISOLanguageName": "fre", "ThreeLetterISOLanguageNames": ["fre", "fra"]},
    {"Name": "German", "DisplayName": "German", "TwoLetterISOLanguageName": "de",
     "ThreeLetterISOLanguageName": "ger", "ThreeLetterISOLanguageNames": ["ger", "deu"]},
    {"Name": "Japanese", "DisplayName": "Japanese", "TwoLetterISOLanguageName": "ja",
     "ThreeLetterISOLanguageName": "jpn", "ThreeLetterISOLanguageNames": ["jpn"]},
]


def json_bytes(obj) -> bytes:
    return json.dumps(obj).encode()


# ---- the demo library (--demo) ------------------------------------------------------------------
# The films `make screenshots` shows: tests/demo_library/catalog.json is the library,
# tests/demo_library/assets.json every source's pinned URL, licence and attribution (public domain,
# CC BY 3.0/4.0, CC0), docs/screenshots/CREDITS.md the credits. tests/mock_pms.py's
# CatalogLibrary builds it from the derived artwork; this only reshapes it the way Jellyfin
# answers, so the two mocks cannot drift apart.

DEMO = None  # the DemoJf `--demo` serves; None serves the synthetic library above
TICKS_PER_MS = 10_000
# Key blocks for what mock_pms keys by tag id, clear of its items (movies 101.., shows 201..,
# seasons 2011.., episodes 2011001.., collections 50001..).
PERSON_RK, GENRE_RK = 70_000, 80_000
JF_TYPE = {"movie": "Movie", "show": "Series", "season": "Season", "episode": "Episode"}
STREAM_TYPE = {1: "Video", 2: "Audio", 3: "Subtitle"}
# Jellyfin image type -> CatalogLibrary.images slot; Thumb (a landscape card) is the backdrop.
IMAGE_SLOT = {"Primary": "thumb", "Backdrop": "art", "Logo": "clearLogo", "Thumb": "art"}
# The original-language titles of the demo's silent films (public record), which the catalog,
# shared with mock_pms, does not carry: a detail page shows the line only when it differs.
ORIGINAL_TITLES = {"Nosferatu": "Nosferatu, eine Symphonie des Grauens",
                   "The Cabinet of Dr. Caligari": "Das Cabinet des Dr. Caligari"}
# Birth dates (public record) for the two people docs/litefin-shots.md names; nobody else gets one.
BORN = {"Derek de Lint": "1950-07-16", "Buster Keaton": "1895-10-04"}
# Intro and credits segments, `(start ms, end ms)`, for the one demo episode that plays from a file
# and has a successor (Caminandes S1E2, 180 s): the Skip button and the Up Next card's trigger.
SEGMENTS = {2011002: {"Intro": (4_000, 20_000), "Outro": (150_000, 180_000)}}
# Routes the demo leaves to the synthetic handlers: sign-in, the user's configuration, cultures,
# playback reports.
SHARED = ("/Users/", "/Localization/", "/QuickConnect/", "/Sessions/")


def gid(rk: int) -> str:
    """The GUID the app interns back to `rk` (jf/ids.rs: its first 13 hex digits as an integer),
    so the demo keeps mock_pms's ratingKeys: `detail=105` names the same film on both mocks."""
    return f"{rk:013x}{'0' * 19}"


def rk_of(guid: str) -> int:
    try:
        return int(guid[:13], 16) if len(guid) == 32 else 0
    except ValueError:
        return 0


def iso(ts: int) -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%S.0000000Z", time.gmtime(ts))


B83 = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~"


def _b83(v: int, n: int) -> str:
    return "".join(B83[v // 83 ** (n - 1 - i) % 83] for i in range(n))


def _linear(c: int) -> float:
    x = c / 255
    return x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4


def _srgb(x: float) -> int:
    x = min(1.0, max(0.0, x))
    return round(255 * (x * 12.92 if x <= 0.0031308 else 1.055 * x ** (1 / 2.4) - 0.055))


def blurhash(tl, tr, br, bl) -> str:
    """A 2x2-component BlurHash (woltapp's format) that jf/blurhash.rs `corners`, sampling at
    15 % / 85 %, reads back as these four sRGB corners: four samples fix four components.
    shortcut: plain rounding to the format's 19 AC levels, so a near-black corner beside bright
    ones reads back up to ~60 levels off; fine for an ambient wash, fit the digits if it matters."""
    k = math.cos(math.pi * 0.15)
    corners = [((1, 1), tl), ((-1, 1), tr), ((-1, -1), br), ((1, -1), bl)]
    lin = [(s, [_linear(c) for c in rgb]) for s, rgb in corners]

    def comp(basis, d):
        return [sum(basis(*s) * c[ch] for s, c in lin) / d for ch in range(3)]

    dc = comp(lambda sx, sy: 1, 4)
    ac = [comp(lambda sx, sy: sx, 4 * k), comp(lambda sx, sy: sy, 4 * k),
          comp(lambda sx, sy: sx * sy, 4 * k * k)]
    qmax = max(0, min(82, math.floor(max(abs(v) for a in ac for v in a) * 166 - 0.5)))
    top = (qmax + 1) / 166

    def q(v):
        return max(0, min(18, math.floor(math.copysign(abs(v / top) ** 0.5, v) * 9 + 9.5)))

    head = _b83(1 + 1 * 9, 1) + _b83(qmax, 1) + _b83((_srgb(dc[0]) << 16) | (_srgb(dc[1]) << 8) | _srgb(dc[2]), 4)
    return head + "".join(_b83(q(a[0]) * 361 + q(a[1]) * 19 + q(a[2]), 2) for a in ac)


class DemoJf:
    """The demo library as a Jellyfin server answers for it: two views, items, shelves, people,
    collections, artwork and the Sintel film itself. Read-only and deterministic: a watched mark
    or a resume point the app sends is acknowledged and forgotten, so every run starts from the
    catalog's pinned state (Continue Watching, watched marks, `now`)."""

    SORT_KEYS = {
        "SortName": lambda d: d.get("SortName", d["Name"]).lower(),
        "DateCreated": lambda d: d.get("DateCreated", ""),
        "PremiereDate": lambda d: d.get("PremiereDate", ""),
        "ProductionYear": lambda d: d.get("ProductionYear", 0),
        "DatePlayed": lambda d: d["UserData"].get("LastPlayedDate", ""),
        "PlayCount": lambda d: d["UserData"].get("PlayCount", 0),
        "Runtime": lambda d: d.get("RunTimeTicks", 0),
        "Random": lambda d: hashlib.md5(d["Id"].encode()).hexdigest(),
        "SeriesSortName": lambda d: d.get("SeriesName", "").lower(),
        "ParentIndexNumber": lambda d: d.get("ParentIndexNumber", 0),
        "IndexNumber": lambda d: d.get("IndexNumber", 0),
    }

    def __init__(self, catalog=None, cache=None):
        import mock_pms  # beside this file; only the demo needs it
        self.lib = lib = mock_pms.CatalogLibrary(
            catalog or pathlib.Path(__file__).resolve().parent / "demo_library" / "catalog.json",
            cache=cache, loudness_analysis=False)
        self.serve_range = mock_pms.Handler._serve_file_ranged
        self.server = mock_pms.Server  # quiet when a player hangs up mid-stream
        self.tag = f"{lib.now:x}"
        self._scaled, self._hashes = {}, {}
        self.views = [{"Id": gid(int(s["key"])), "Name": s["title"], "Type": "CollectionFolder",
                       "ServerId": SERVER_ID, "IsFolder": True, "ImageTags": {"Primary": self.tag},
                       "CollectionType": {"movie": "movies", "show": "tvshows"}[s["type"]]}
                      for s in lib.sections]
        # A library's own artwork: the backdrop of its newest arrival, as a server's library image
        # usually is one of its films; a genre's, its first film's backdrop (catalog order).
        added = sorted(lib.items.values(), key=lambda it: -it.get("addedAt", 0))
        self.view_art = {v["Id"]: next(lib.images[int(it["ratingKey"])]["art"] for it in added
                                       if it["librarySectionID"] == rk_of(v["Id"]) and it["type"] in ("movie", "show")
                                       and "art" in lib.images.get(int(it["ratingKey"]), {}))
                         for v in self.views}
        self.genre_art = {}
        for it in sorted(lib.items.values(), key=lambda it: int(it["ratingKey"])):
            for g in it.get("Genre", []) if "art" in lib.images.get(int(it["ratingKey"]), {}) else []:
                self.genre_art.setdefault(gid(GENRE_RK + g["id"]), lib.images[int(it["ratingKey"])]["art"])
        # Jellyfin's Next Up shelf (EnableResumable=false) needs a series whose next episode is
        # unstarted, and the shared catalog has none: this mock alone has watched Hubblecast's first.
        hubble = next(it for it in lib.items.values() if it["type"] == "show" and it["title"] == "Hubblecast")
        first = lib.leaves(int(hubble["ratingKey"]))[0]
        first["viewCount"] = 1
        for container in (hubble, lib.items[int(first["parentRatingKey"])]):
            container["viewedLeafCount"] = container.get("viewedLeafCount", 0) + 1
        # shortcut: the demo library has no portraits, so each actor wears a Blender film's poster
        # (computer-drawn, so no real person's likeness carries another's name), dealt round by
        # person id so one cast row differs; upgrade when assets.json gains licensed portraits.
        posters = [lib.images[rk]["thumb"] for rk, it in sorted(lib.items.items()) if it["type"] == "movie"
                   and "Blender" in it.get("studio", "") and "thumb" in lib.images.get(rk, {})]
        actors = sorted({p["id"] for it in lib.items.values() for p in it.get("Role", [])})
        self.portraits = {PERSON_RK + pid: posters[pid % len(posters)] for pid in actors}
        self.genre_ids = {g["tag"]: gid(GENRE_RK + g["id"]) for g in lib.genres.values()}
        self.rows = {gid(rk): it for rk, it in lib.items.items()}
        self.boxsets = {gid(c["ratingKey"]): c for c in lib.collections.values()}
        self.dto = {g: self.item(it) for g, it in self.rows.items()}
        self.dto.update({g: self.boxset(c) for g, c in self.boxsets.items()})
        self.persons = {gid(PERSON_RK + pid): self.person(pid, p) for pid, p in lib.people.items()}

    # ---- shapes -------------------------------------------------------------------------------

    def person(self, pid, p):
        """A person as `/Items/{personId}` answers: the portrait stand-in, a biography that only
        states the demo library's own credits (no invented life story), and a birth date for the
        two people BORN names."""
        credited = sorted({(it.get("year", 0), it["title"]) for it in self.lib.items.values()
                           if it["type"] in ("movie", "show")
                           and any(c["id"] == pid for key in ("Role", "Director", "Writer") for c in it.get(key, []))})
        titles = ", ".join(f"{t} ({y})" if y else t for y, t in credited)
        d = {"Id": gid(PERSON_RK + pid), "Name": p["tag"], "Type": "Person", "ServerId": SERVER_ID,
             "ImageTags": {"Primary": self.tag} if PERSON_RK + pid in self.portraits else {},
             "Overview": f"{p['tag']} is credited in {titles} in the demo library." if titles else ""}
        if p["tag"] in BORN:
            d["PremiereDate"] = BORN[p["tag"]] + "T00:00:00.0000000Z"
        return d

    def item(self, it):
        rk, kind = int(it["ratingKey"]), it["type"]
        img = self.lib.images.get(rk, {})
        d = {"Id": gid(rk), "Name": it["title"], "OriginalTitle": ORIGINAL_TITLES.get(it["title"], it["title"]),
             "SortName": it.get("titleSort", it["title"]),
             "Type": JF_TYPE[kind], "ServerId": SERVER_ID, "IsFolder": kind in ("show", "season"),
             "Overview": it.get("summary", ""), "DateCreated": iso(it.get("addedAt", self.lib.now)),
             "Genres": [g["tag"] for g in it.get("Genre", [])], "ProviderIds": {},
             "Studios": [{"Name": it["studio"]}] if it.get("studio") else [],
             "People": [self.credit(p, t) for t, key in (("Actor", "Role"), ("Director", "Director"),
                                                         ("Writer", "Writer")) for p in it.get(key, [])],
             "UserData": self.user_data(it), "ImageTags": {}, "BackdropImageTags": [], "ImageBlurHashes": {}}
        for key, v in (("ProductionYear", it.get("year")), ("RunTimeTicks", it.get("duration", 0) * TICKS_PER_MS),
                       ("IndexNumber", it.get("index")), ("ParentIndexNumber", it.get("parentIndex"))):
            if v:
                d[key] = v
        if it.get("originallyAvailableAt"):
            d["PremiereDate"] = it["originallyAvailableAt"] + "T00:00:00.0000000Z"
        # A season has no artwork of its own here and an episode only its still: both borrow the
        # series' poster, backdrop and logo through the Parent*/Series* fields, as on Jellyfin.
        own = {"movie": ("thumb", "art", "clearLogo"), "show": ("thumb", "art", "clearLogo"),
               "episode": ("thumb",)}.get(kind, ())
        for jf, slot in (("Primary", "thumb"), ("Logo", "clearLogo")):
            if slot in own and slot in img:
                d["ImageTags"][jf] = self.tag
        if "art" in own and "art" in img:
            d["BackdropImageTags"] = [self.tag]
        series = int(it["grandparentRatingKey"] if kind == "episode" else it["parentRatingKey"]) \
            if kind in ("season", "episode") else 0
        art_owner = rk if "art" in own else series
        if "art" in self.lib.images.get(art_owner, {}):
            d["ImageBlurHashes"]["Backdrop"] = {self.tag: self.hash(art_owner, "art")}
        elif "Primary" in d["ImageTags"]:
            d["ImageBlurHashes"]["Primary"] = {self.tag: self.hash(rk, "thumb")}
        if kind == "show":
            d.update(ChildCount=it.get("childCount", 0), RecursiveItemCount=it.get("leafCount", 0), Status="Ended")
        if kind == "season":
            d.update(ChildCount=it.get("leafCount", 0), RecursiveItemCount=it.get("leafCount", 0))
        if series:
            show, simg, sg = self.lib.items[series], self.lib.images.get(series, {}), gid(series)
            d.update(SeriesId=sg, SeriesName=show["title"], ParentId=gid(int(it["parentRatingKey"])))
            if "thumb" in simg:
                d["SeriesPrimaryImageTag"] = self.tag
            if "art" in simg:
                d.update(ParentBackdropItemId=sg, ParentBackdropImageTags=[self.tag])
            if "clearLogo" in simg:
                d.update(ParentLogoItemId=sg, ParentLogoImageTag=self.tag)
        if kind == "episode":
            d.update(SeasonId=gid(int(it["parentRatingKey"])), SeasonName=it.get("parentTitle", ""))
        if kind in ("movie", "episode"):
            d["MediaSources"] = self.sources(it)
            d["Chapters"] = [{"StartPositionTicks": c["startTimeOffset"] * TICKS_PER_MS, "Name": c["tag"]}
                             for c in it.get("Chapter", [])]
        return d

    def user_data(self, it):
        if it["type"] in ("show", "season"):
            leaves, seen = it.get("leafCount", 0), it.get("viewedLeafCount", 0)
            return {"UnplayedItemCount": leaves - seen, "Played": 0 < leaves <= seen,
                    "PlayCount": 0, "PlaybackPositionTicks": 0}
        u = {"PlaybackPositionTicks": it.get("viewOffset", 0) * TICKS_PER_MS,
             "PlayCount": it.get("viewCount", 0), "Played": it.get("viewCount", 0) > 0}
        if it.get("lastViewedAt"):
            u["LastPlayedDate"] = iso(it["lastViewedAt"])
        if it.get("viewOffset") and it.get("duration"):
            u["PlayedPercentage"] = 100 * it["viewOffset"] / it["duration"]
        return u

    def credit(self, p, kind):
        prk = PERSON_RK + p["id"]
        row = {"Name": p["tag"], "Id": gid(prk), "Type": kind}
        if p.get("role"):
            row["Role"] = p["role"]
        if prk in self.portraits:
            row["PrimaryImageTag"] = self.tag
        return row

    def boxset(self, c):
        members = self.lib.collection_rows(c["id"])
        return {"Id": gid(c["ratingKey"]), "Name": c["tag"], "SortName": c["tag"], "Type": "BoxSet",
                "ServerId": SERVER_ID, "IsFolder": True, "ChildCount": len(members),
                "RecursiveItemCount": len(members),
                "DateCreated": iso(max((m.get("addedAt", 0) for m in members), default=self.lib.now)),
                "ImageTags": {"Primary": self.tag} if members else {}, "BackdropImageTags": [],
                "UserData": {"PlaybackPositionTicks": 0, "PlayCount": 0, "Played": False}}

    def sources(self, it):
        """The item's one MediaSource, from mock_pms's Media row: the probed Sintel film and the
        stand-in episodes play; the rest carry a believable stream list and no file."""
        if not it.get("Media"):
            return []
        m = it["Media"][0]
        part = m["Part"][0]
        streams = []
        for s in part.get("Stream", []):
            row = {"Type": STREAM_TYPE[s["streamType"]], "Index": s["index"],
                   "Codec": {"srt": "subrip"}.get(s["codec"], s["codec"]),
                   "Language": s.get("languageCode") or s.get("language"), "DisplayTitle": s.get("displayTitle"),
                   "IsDefault": bool(s.get("default")), "IsTextSubtitleStream": s["streamType"] == 3,
                   "Width": s.get("width"), "Height": s.get("height"), "Channels": s.get("channels"),
                   "BitDepth": s.get("bitDepth"), "RealFrameRate": s.get("frameRate")}
            streams.append({k: v for k, v in row.items() if v is not None})
        return [{"Id": gid(int(it["ratingKey"])), "Name": it["title"], "Path": part.get("file", ""),
                 "Protocol": "File", "Container": part.get("container") or m.get("container"),
                 "Size": part.get("size", 0), "Bitrate": m.get("bitrate", 0) * 1000,
                 "RunTimeTicks": it.get("duration", 0) * TICKS_PER_MS, "SupportsDirectPlay": True,
                 "SupportsDirectStream": True, "SupportsTranscoding": True,
                 "DefaultAudioStreamIndex": next((s["Index"] for s in streams if s["Type"] == "Audio"), None),
                 "MediaStreams": streams}]

    def hash(self, rk, slot):
        """An image's BlurHash from its 2x2 area average: mock_pms's UltraBlurColors of a backdrop
        undarkened (a Jellyfin hash describes the picture itself), else the same read of it here."""
        path = self.lib.images[rk][slot]
        if path not in self._hashes:
            blur = self.lib.items[rk].get("UltraBlurColors") if slot == "art" else None
            blur = blur or self.lib._blur_of(path)  # #rrggbb at 0.55 of the picture's own
            tl, tr, br, bl = ([min(255, round(int(blur[k][i:i + 2], 16) / 0.55)) for i in (1, 3, 5)]
                              for k in ("topLeft", "topRight", "bottomRight", "bottomLeft"))
            self._hashes[path] = blurhash(tl, tr, br, bl)
        return self._hashes[path]

    # ---- queries ------------------------------------------------------------------------------

    def in_view(self, it, q):
        parent = q.get("ParentId")
        return not parent or it.get("librarySectionID") == rk_of(parent)

    def dtos(self, rows):
        return [self.dto[gid(int(it["ratingKey"]))] for it in rows]

    @staticmethod
    def page(rows, q):
        start = int(q.get("StartIndex") or 0)
        limit = int(q.get("Limit") or 0)
        return {"Items": rows[start:start + limit] if limit else rows[start:],
                "TotalRecordCount": len(rows), "StartIndex": start}

    def under(self, parent, recursive):
        if parent in self.boxsets:
            return self.dtos(self.lib.collection_rows(self.boxsets[parent]["id"]))
        if parent in {v["Id"] for v in self.views}:
            rows = [self.dto[g] for g, it in self.rows.items() if it["librarySectionID"] == rk_of(parent)]
            return rows if recursive else [d for d in rows if d["Type"] in ("Movie", "Series")]
        rk = rk_of(parent)
        return self.dtos(self.lib.children(rk) + (self.lib.leaves(rk) if recursive else []))

    def query(self, q):
        """`/Items`: under `ParentId` (a view, a series, a season, a BoxSet), else everything;
        then the filters, the sort and the page the app asks for."""
        rows = list(self.dto.values())
        if q.get("ParentId"):
            rows = self.under(q["ParentId"], q.get("Recursive", "").lower() == "true")
        if q.get("Ids"):
            rows = [self.dto[g] for g in q["Ids"].split(",") if g in self.dto]
        if q.get("IncludeItemTypes"):
            rows = [d for d in rows if d["Type"] in q["IncludeItemTypes"].split(",")]
        if q.get("PersonIds"):
            rows = [d for d in rows if any(p["Id"] in q["PersonIds"].split(",") for p in d.get("People", []))]
        if q.get("searchTerm"):
            rows = [d for d in rows if q["searchTerm"].lower() in d["Name"].lower()]
        genres = {n for n, g in self.genre_ids.items() if g in q.get("GenreIds", "").split(",")}
        genres |= set(filter(None, q.get("Genres", "").split("|")))
        if genres:
            rows = [d for d in rows if genres & set(d.get("Genres", []))]
        if q.get("NameStartsWith"):
            rows = [d for d in rows if d.get("SortName", d["Name"]).lower().startswith(q["NameStartsWith"].lower())]
        played = {"IsUnplayed": False, "IsPlayed": True}
        for f in q.get("Filters", "").split(","):
            if f in played:
                rows = [d for d in rows if d["UserData"].get("Played", False) == played[f]]
            elif f == "IsResumable":
                rows = [d for d in rows if d["UserData"].get("PlaybackPositionTicks", 0) > 0]
            elif f == "IsFavorite":
                rows = self.favourites(rows)
        keys = [self.SORT_KEYS[k] for k in q.get("SortBy", "").split(",") if k in self.SORT_KEYS]
        if keys:
            rows = sorted(rows, key=lambda d: tuple(k(d) for k in keys),
                          reverse=q.get("SortOrder", "").startswith("Desc"))
        return self.page(rows, q)

    # The demo catalog marks nothing a favourite, so the Favorites page (the rail's second item)
    # gets a pinned set: fourteen films, the three series, two seasons, four episodes and the first
    # six people who wear a portrait.
    FAVOURITE_RKS = (*range(101, 111), 112, 113, 120, 121, 201, 202, 203, 2011, 2021,
                     2011001, 2011002, 2011003, 2021001)

    def favourites(self, rows):
        """`Filters=IsFavorite`: the pinned favourites among `rows`, items or people."""
        pinned = {gid(rk) for rk in (*self.FAVOURITE_RKS, *sorted(self.portraits)[:6])}
        return [d for d in rows if d["Id"] in pinned]

    def latest(self, q):
        """`/Items/Latest` with GroupItems: newest films, and series by their newest arrival."""
        rows = sorted((it for it in self.rows.values() if it["type"] in ("movie", "show") and self.in_view(it, q)),
                      key=lambda it: -it.get("addedAt", 0))
        return self.dtos(rows)[:int(q.get("Limit") or 20)]

    def next_up(self, q):
        """`/Shows/NextUp`: per series with a watched episode, the first unwatched one after the
        last watched; a resumable one only while `EnableResumable` is not false."""
        rows = []
        for show in (it for it in self.rows.values() if it["type"] == "show" and self.in_view(it, q)):
            if q.get("SeriesId") not in (None, gid(int(show["ratingKey"]))):
                continue
            eps = self.lib.leaves(int(show["ratingKey"]))
            seen = [n for n, e in enumerate(eps) if e.get("viewCount", 0) > 0]
            nxt = next((e for e in eps[seen[-1] + 1:] if not e.get("viewCount")), None) if seen else None
            if nxt and not (nxt.get("viewOffset") and q.get("EnableResumable", "").lower() == "false"):
                rows.append(nxt)
        return self.page(self.dtos(rows), q)

    def similar(self, guid, q):
        d = self.dto[guid]
        mine = set(d.get("Genres", []))
        pool = [x for x in self.dto.values()
                if x["Type"] == d["Type"] and x["Id"] != guid and mine & set(x.get("Genres", []))]
        pool.sort(key=lambda x: -len(mine & set(x["Genres"])))  # stable: catalog order breaks ties
        return self.page(pool, {"Limit": q.get("Limit")})

    def filters(self, q):
        rows = self.query({k: q[k] for k in ("ParentId", "IncludeItemTypes") if k in q} | {"Recursive": "true"})
        names = sorted({g for d in rows["Items"] for g in d.get("Genres", [])})
        return {"Genres": [{"Name": n, "Id": self.genre_ids[n]} for n in names], "Tags": []}

    def genres(self, q):
        """`/Genres` under `ParentId` (a view): each genre the view's films and series carry, with
        its tile artwork."""
        rows = self.query({k: q[k] for k in ("ParentId",) if k in q} | {"Recursive": "true",
                                                                        "IncludeItemTypes": "Movie,Series"})["Items"]
        names = sorted({g for d in rows for g in d.get("Genres", [])})
        return self.page([{"Id": self.genre_ids[n], "Name": n, "Type": "Genre", "ServerId": SERVER_ID,
                           "ImageTags": {"Primary": self.tag} if self.genre_ids[n] in self.genre_art else {}}
                          for n in names], q)

    def segments(self, guid):
        """`/MediaSegments/{id}` (Jellyfin 10.10): the item's intro and credits, in ticks."""
        segs = SEGMENTS.get(rk_of(guid), {})
        return {"Items": [{"Id": f"{guid[:-2]}{n + 1:02x}", "ItemId": guid, "Type": kind,
                           "StartTicks": a * TICKS_PER_MS, "EndTicks": b * TICKS_PER_MS}
                          for n, (kind, (a, b)) in enumerate(segs.items())], "TotalRecordCount": len(segs)}

    def episodes(self, series, q):
        rows = self.lib.leaves(rk_of(series))
        if q.get("SeasonId"):
            rows = [e for e in rows if gid(int(e["parentRatingKey"])) == q["SeasonId"]]
        return self.page(self.dtos(rows), q)

    def image(self, guid, kind, q):
        """`(content type, bytes)` for `/Items/{guid}/Images/{kind}`, fitted inside maxWidth x
        maxHeight, or covering fillWidth x fillHeight, the way Jellyfin sizes it (never upscaled);
        None for an image the item lacks."""
        fill = "fillWidth" in q or "fillHeight" in q
        kw, kh = ("fillWidth", "fillHeight") if fill else ("maxWidth", "maxHeight")
        if guid in self.boxsets:
            return self.lib.composite(rk_of(guid), q.get(kw, 400), q.get(kh, 600))
        if guid in self.persons:
            path = self.portraits.get(rk_of(guid)) if kind == "Primary" else None
        elif guid in self.view_art or guid in self.genre_art:
            path = (self.view_art.get(guid) or self.genre_art.get(guid)) if kind == "Primary" else None
        else:
            path = self.lib.images.get(rk_of(guid), {}).get(IMAGE_SLOT.get(kind, ""))
        if path is None:
            return None
        w, h = (str(int(q[k])) if q.get(k, "").isdigit() else whole for k, whole in ((kw, "iw"), (kh, "ih")))
        png = path.suffix == ".png" or q.get("format", "").lower() == "png"
        key = (path, w, h, png, fill)
        if key not in self._scaled:
            enc = ["-pix_fmt", "rgba", "-vcodec", "png"] if png else ["-q:v", "3", "-vcodec", "mjpeg"]
            self._scaled[key] = subprocess.check_output([
                "ffmpeg", "-v", "error", "-threads", "1", "-i", str(path), "-vf",
                f"scale='min(iw,{w})':'min(ih,{h})':force_original_aspect_ratio={'increase' if fill else 'decrease'}"
                ":flags=lanczos",
                "-frames:v", "1", "-bitexact", *enc, "-f", "image2pipe", "-"])
        return ("image/png" if png else "image/jpeg"), self._scaled[key]

    def media_file(self, guid):
        """`(path, content type)` of the file an item's stream plays, None for one without."""
        it = self.rows.get(guid) or {}
        part = (it.get("Media") or [{}])[0].get("Part", [{}])[0].get("id")
        path = self.lib.media_files.get(part)
        return (path, self.lib.media_content_type.get(part, "video/mp4")) if path else None

    def get(self, segs, q):
        """`(status, body, content type)` for a GET the demo answers, None for a 404."""
        def j(obj):
            return 200, json_bytes(obj), "application/json"

        if segs == ["System", "Info", "Public"]:
            return j({"Id": SERVER_ID, "ServerName": self.lib.friendly, "Version": "10.11.0",
                      "StartupWizardCompleted": True})
        if segs == ["UserViews"]:
            return j({"Items": self.views, "TotalRecordCount": len(self.views)})
        if segs == ["Items"]:
            return j(self.query(q))
        if segs == ["Items", "Latest"]:
            return j(self.latest(q))
        if segs == ["Items", "Filters2"]:
            return j(self.filters(q))
        if segs == ["UserItems", "Resume"]:
            return j(self.page(self.dtos([it for it in self.lib.continue_watching() if self.in_view(it, q)]), q))
        if segs == ["Shows", "NextUp"]:
            return j(self.next_up(q))
        if segs == ["Persons"]:
            term = q.get("searchTerm", "").lower()
            people = [p for p in self.persons.values() if term in p["Name"].lower()]
            return j(self.page(self.favourites(people) if "IsFavorite" in q.get("Filters", "") else people, q))
        if segs[:1] == ["MediaSegments"]:
            return j(self.segments(segs[1] if len(segs) > 1 else ""))
        if segs == ["Genres"]:
            return j(self.genres(q))
        if segs[:1] == ["Shows"] and len(segs) == 3 and segs[2] == "Seasons":
            return j(self.page(self.dtos(self.lib.children(rk_of(segs[1]))), q))
        if segs[:1] == ["Shows"] and len(segs) == 3 and segs[2] == "Episodes":
            return j(self.episodes(segs[1], q))
        if segs[:1] == ["Items"] and len(segs) == 2:
            found = self.dto.get(segs[1]) or self.persons.get(segs[1])
            return j(found) if found else None
        if segs[:1] == ["Items"] and len(segs) == 3 and segs[2] in ("SpecialFeatures", "LocalTrailers"):
            return j([])
        if segs[:1] == ["Items"] and len(segs) == 3 and segs[2] == "Similar" and segs[1] in self.dto:
            return j(self.similar(segs[1], q))
        if segs[:1] == ["Items"] and len(segs) >= 4 and segs[2] == "Images":
            found = self.image(segs[1], segs[3], q)
            return (200, found[1], found[0]) if found else None
        return None


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        print(f"mock_jf: {fmt % args}", flush=True)

    def _send(self, status: int, body: bytes, ctype="application/json"):
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _demo(self, method: str, path: str) -> bool:
        """Answer `path` from the demo library; False leaves it to the synthetic routes it shares
        (SHARED). A path the demo does not know is a 404, never a synthetic row."""
        if DEMO is None or (path.startswith(SHARED) and path != "/Users/Me"):
            return False
        segs = path.split("/")[1:]
        q = {k: v[-1] for k, v in parse_qs(urlparse(self.path).query).items()}
        if method == "POST":
            if path.endswith("/PlaybackInfo"):
                sources = DEMO.dto.get(segs[1], {}).get("MediaSources", [])
                self._send(200, json_bytes({"MediaSources": sources, "PlaySessionId": "play1"}))
            else:  # a watched mark, a resume point, a favourite: acknowledged, never kept
                self._send(200, b"{}")
            return True
        if path == "/Users/Me":
            self._send(200, json_bytes({"Id": USER_ID, "Name": "Demo", "ServerId": SERVER_ID, "Configuration": CONFIG}))
            return True
        media = DEMO.media_file(segs[1]) if segs[:1] == ["Videos"] and len(segs) == 3 else None
        if media and segs[2].startswith("stream"):
            DEMO.serve_range(self, method, *media)
            return True
        status, body, ctype = DEMO.get(segs, q) or (404, b"{}", "application/json")
        self._send(status, body, ctype)
        return True

    def do_GET(self):
        path = urlparse(self.path).path.rstrip("/") or "/"
        if self._demo("GET", path):
            return
        if path == "/System/Info/Public":
            return self._send(200, json_bytes({
                "Id": SERVER_ID, "ServerName": "s00000001", "Version": "10.11.0",
                "StartupWizardCompleted": True,
            }))
        if path == "/Users/Me":
            return self._send(200, json_bytes({
                "Id": USER_ID, "Name": "suser0001", "ServerId": SERVER_ID,
                "Configuration": CONFIG,
            }))
        if path == "/Localization/Cultures":
            return self._send(200, json_bytes(CULTURES))
        if path == "/UserViews":
            return self._send(200, json_bytes({"Items": VIEWS, "TotalRecordCount": len(VIEWS)}))
        if path == "/Items":
            q = parse_qs(urlparse(self.path).query)
            rows = CONTENTS.get(q.get("ParentId", [LIBRARY_ID])[0], [])
            types = q.get("IncludeItemTypes", [""])[0]
            if types:
                rows = [r for r in rows if r["Type"] in types.split(",")]
            return self._send(200, json_bytes({"Items": rows, "TotalRecordCount": len(rows)}))
        if path.startswith("/Items/") and path.endswith("/SpecialFeatures"):
            return self._send(200, json_bytes([]))
        if path.startswith("/Items/") and "/Images/" not in path:
            iid = path.split("/")[2]
            found = next((x for x in CATALOG if x["Id"] == iid), CATALOG[1])
            return self._send(200, json_bytes(found))
        if path in ("/UserItems/Resume", "/Shows/NextUp", "/Items/Latest"):
            return self._send(200, json_bytes({"Items": [CATALOG[1]], "TotalRecordCount": 1}
                                             if path != "/Items/Latest" else json_bytes([CATALOG[1]])))
        if path == "/Items/Filters2":
            return self._send(200, json_bytes({"Genres": []}))
        if path == "/QuickConnect/Enabled":
            return self._send(200, b"true")
        if path == "/QuickConnect/Connect":
            return self._send(200, json_bytes({"Authenticated": False}))
        if path.startswith("/Videos/") and "/stream" in path:
            self._send(200, b"\x00" * 16, "application/octet-stream")
            return
        self._send(404, b"{}")

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0") or 0)
        body = self.rfile.read(length) if length else b""
        path = urlparse(self.path).path.rstrip("/") or "/"
        if self._demo("POST", path):
            return
        if path == "/Users/Configuration":
            CONFIG.clear()
            CONFIG.update(json.loads(body or b"{}"))
            return self._send(204, b"")
        if path == "/Users/AuthenticateByName":
            return self._send(200, json_bytes({
                "AccessToken": TOKEN,
                "User": {"Id": USER_ID, "Name": "suser0001"},
                "SessionInfo": {"Id": "sess1"},
            }))
        if path == "/QuickConnect/Initiate":
            return self._send(200, json_bytes({"Secret": "sec", "Code": "123456"}))
        if path.endswith("/PlaybackInfo"):
            return self._send(200, json_bytes({
                "MediaSources": [{
                    "Id": MOVIE_ID,
                    "SupportsDirectPlay": True,
                    "SupportsDirectStream": True,
                    "SupportsTranscoding": True,
                    "Path": "file.mkv",
                    "Container": "mkv",
                    "MediaStreams": [],
                }],
                "PlaySessionId": "play1",
            }))
        if path.startswith("/Sessions/Playing"):
            return self._send(204, b"")
        self._send(404, b"{}")

    def do_DELETE(self):
        self._send(204, b"")


def serve(host: str, port: int, demo: bool = False):
    global DEMO
    server = ThreadingHTTPServer
    if demo:
        try:
            DEMO = DemoJf()
        except ValueError as e:  # no derived artwork: `make demo-library`
            sys.exit(f"mock_jf: {e}")
        server = DEMO.server
    httpd = server((host, port), Handler)
    print(f"mock_jf: serving on http://{host}:{httpd.server_address[1]}", flush=True)
    httpd.serve_forever()


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8096, help="0 picks a free one (printed)")
    p.add_argument("--demo", action="store_true",
                   help="serve the demo library with its artwork (make demo-library) instead of the synthetic one")
    args = p.parse_args()
    serve(args.host, args.port, args.demo)


if __name__ == "__main__":
    main()
