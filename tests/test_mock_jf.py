"""tests/mock_jf.py: the synthetic library keeps its shapes, and `--demo` serves the demo library
the way the app's Jellyfin client reads it. The demo cases need the derived artwork cache
(`make demo-library`) and ffmpeg; they skip, and say so, without them."""
import json
import math
import shutil
import subprocess
import sys
import threading
import unittest
import urllib.error
import urllib.request
from http.server import ThreadingHTTPServer

import mock_jf
import mock_pms

HAVE_CACHE = (mock_pms.demo_cache_dir() / "derived").is_dir() and shutil.which("ffmpeg") and shutil.which("ffprobe")


def decode_corners(h):
    """jf/blurhash.rs `corners`, transcribed: the sRGB (0..255) colour at 15 % / 85 % of each corner."""
    d83 = lambda s: sum(mock_jf.B83.index(c) * 83 ** i for i, c in enumerate(reversed(s)))
    size = d83(h[0])
    nx, ny = size % 9 + 1, size // 9 + 1
    top = (d83(h[1]) + 1) / 166
    dc = d83(h[2:6])
    comps = [[mock_jf._linear(dc >> 16), mock_jf._linear((dc >> 8) & 255), mock_jf._linear(dc & 255)]]
    for i in range(1, nx * ny):
        v = d83(h[4 + i * 2:6 + i * 2])
        comps.append([math.copysign(((c - 9) / 9) ** 2, c - 9) * top for c in (v // 361, v // 19 % 19, v % 19)])

    def sample(u, v):
        return [mock_jf._srgb(sum(comps[i + j * nx][ch] * math.cos(math.pi * u * i) * math.cos(math.pi * v * j)
                                  for j in range(ny) for i in range(nx))) for ch in range(3)]
    return [sample(0.15, 0.15), sample(0.85, 0.15), sample(0.85, 0.85), sample(0.15, 0.85)]


class BlurHash(unittest.TestCase):
    def test_the_four_corners_read_back(self):
        # a backdrop's 2x2 average: corners near one another (the Sintel header's, a flat grey,
        # a dark frame lit at one side)
        for corners in ([(92, 74, 61), (121, 96, 78), (60, 52, 47), (70, 66, 63)], [(120, 120, 120)] * 4,
                        [(20, 22, 30), (70, 60, 50), (40, 38, 36), (25, 25, 28)]):
            h = mock_jf.blurhash(*corners)
            self.assertEqual(len(h), 12, h)
            for want, got in zip(corners, decode_corners(h)):
                self.assertLessEqual(max(abs(a - b) for a, b in zip(want, got)), 12, (h, want, got))


class Synthetic(unittest.TestCase):
    """Without --demo nothing changed: the synthetic rows, and no artwork route."""

    def test_the_synthetic_library_is_unchanged(self):
        with Served(None) as get:
            views = json.loads(get("/UserViews")[2])["Items"]
            self.assertEqual([v["Name"] for v in views], ["s11111111", "s22222222", "s33333333"])
            movies = json.loads(get(f"/Items?ParentId={mock_jf.LIBRARY_ID}")[2])["Items"]
            self.assertEqual([m["Name"] for m in movies], ["s3fa90c12"])
            self.assertEqual(get(f"/Items/{mock_jf.MOVIE_ID}/Images/Primary")[0], 404)


class Quiet(mock_jf.Handler):
    def log_message(self, fmt, *args):
        pass


class Served:
    """mock_jf's handler on a free loopback port, serving `demo` (None: the synthetic library)."""

    def __init__(self, demo):
        self.demo = demo

    def __enter__(self):
        self.saved, mock_jf.DEMO = mock_jf.DEMO, self.demo
        self.srv = ThreadingHTTPServer(("127.0.0.1", 0), Quiet)
        threading.Thread(target=self.srv.serve_forever, daemon=True).start()
        base = f"http://127.0.0.1:{self.srv.server_address[1]}"

        def get(path, headers=None, data=None):
            req = urllib.request.Request(base + path, headers=headers or {}, data=data)
            try:
                with urllib.request.urlopen(req, timeout=30) as r:
                    return r.status, r.headers.get("Content-Type"), r.read()
            except urllib.error.HTTPError as e:
                with e:
                    return e.code, e.headers.get("Content-Type"), e.read()
        return get

    def __exit__(self, *exc):
        self.srv.shutdown()
        self.srv.server_close()
        mock_jf.DEMO = self.saved


@unittest.skipUnless(HAVE_CACHE, "no derived demo cache (make demo-library) or no ffmpeg/ffprobe")
class Demo(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.demo = mock_jf.DemoJf()

    def jget(self, get, path):
        status, ctype, body = get(path)
        self.assertEqual((status, ctype), (200, "application/json"), path)
        return json.loads(body)

    def test_views_listings_and_artwork(self):
        with Served(self.demo) as get:
            views = {v["CollectionType"]: v for v in self.jget(get, "/UserViews")["Items"]}
            self.assertEqual(set(views), {"movies", "tvshows"})
            page = self.jget(get, f"/Items?ParentId={views['movies']['Id']}&IncludeItemTypes=Movie&Recursive=true"
                                  "&SortBy=SortName&SortOrder=Ascending&StartIndex=0&Limit=5")
            self.assertEqual((len(page["Items"]), page["TotalRecordCount"]), (5, 27))
            names = [m["SortName"] for m in page["Items"]]
            self.assertEqual(names, sorted(names, key=str.lower))  # "Cabinet of Dr. Caligari, The"
            first = page["Items"][0]
            status, ctype, body = get(f"/Items/{first['Id']}/Images/Primary?maxWidth=300&maxHeight=450&quality=90"
                                      f"&tag={first['ImageTags']['Primary']}")
            self.assertEqual((status, ctype, body[:2]), (200, "image/jpeg", b"\xff\xd8"))
            spring = self.jget(get, f"/Items/{mock_jf.gid(101)}")
            self.assertEqual(spring["Name"], "Spring")
            self.assertIn("Logo", spring["ImageTags"])
            self.assertIn("Backdrop", spring["ImageBlurHashes"])
            status, ctype, body = get(f"/Items/{spring['Id']}/Images/Logo?maxWidth=600&maxHeight=300&format=Png")
            self.assertEqual((status, ctype, body[:4]), (200, "image/png", b"\x89PNG"))
            self.assertEqual(get(f"/Items/{'f' * 32}")[0], 404)

    def test_fill_covers_the_box_and_max_fits_inside_it(self):
        """The app asks fillWidth x fillHeight, which Jellyfin answers by COVERING the box (never
        upscaled): a portrait asked for in a square comes back as tall as its width needs."""
        def size(body):
            out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "stream=width,height", "-of", "csv=p=0",
                                  "-"], input=body, capture_output=True, check=True).stdout
            return tuple(int(n) for n in out.decode().strip().split(","))
        with Served(self.demo) as get:
            actor = next(p for p in self.jget(get, f"/Items/{mock_jf.gid(105)}")["People"] if p["Type"] == "Actor")
            path = f"/Items/{actor['Id']}/Images/Primary?quality=90"
            src = size(get(path)[2])
            self.assertNotEqual(src[0], src[1], "a portrait, not a square")
            fill = size(get(path + "&fillWidth=120&fillHeight=120")[2])
            self.assertEqual(min(fill), 120, (src, fill))
            fit = size(get(path + "&maxWidth=120&maxHeight=120")[2])
            self.assertEqual(max(fit), 120, (src, fit))

    def test_shelves_series_and_people(self):
        with Served(self.demo) as get:
            resume = self.jget(get, "/UserItems/Resume?Limit=12&MediaTypes=Video")["Items"]
            self.assertEqual(resume[0]["Name"], "Sintel")  # the catalog's hero heads the deck
            self.assertTrue(all(r["UserData"]["PlaybackPositionTicks"] > 0 for r in resume))
            show = mock_jf.gid(201)
            seasons = self.jget(get, f"/Shows/{show}/Seasons")["Items"]
            episodes = self.jget(get, f"/Shows/{show}/Episodes?SeasonId={seasons[0]['Id']}")["Items"]
            self.assertEqual([e["IndexNumber"] for e in episodes], [1, 2, 3])
            self.assertTrue(all(e["SeriesId"] == show and e["ParentBackdropItemId"] == show for e in episodes))
            nxt = self.jget(get, f"/Shows/NextUp?SeriesId={show}")["Items"]
            self.assertEqual([e["Name"] for e in nxt], ["Llamigos"])
            tos = self.jget(get, f"/Items/{mock_jf.gid(105)}")
            actor = next(p for p in tos["People"] if p["Type"] == "Actor")
            self.assertIn("PrimaryImageTag", actor)
            self.assertEqual(get(f"/Items/{actor['Id']}/Images/Primary?maxWidth=200&maxHeight=300")[0], 200)
            films = self.jget(get, f"/Items?PersonIds={actor['Id']}&Recursive=true&IncludeItemTypes=Movie,Series")
            self.assertIn("Tears of Steel", [f["Name"] for f in films["Items"]])

    def test_what_the_litefin_shots_need(self):
        """The person page's biography, the Skip and Up Next segments, a Next Up episode with no
        resume point, and artwork for the My Media and genre tiles (docs/litefin-shots.md)."""
        with Served(self.demo) as get:
            actor = self.jget(get, f"/Items/{mock_jf.gid(70008)}")
            self.assertEqual(actor["Name"], "Derek de Lint")
            self.assertIn("Tears of Steel (2012)", actor["Overview"])
            self.assertTrue(actor["PremiereDate"].startswith("1950-07-16"))
            episode = mock_jf.gid(2011002)
            segs = self.jget(get, f"/MediaSegments/{episode}")["Items"]
            self.assertEqual([s["Type"] for s in segs], ["Intro", "Outro"])
            self.assertTrue(all(s["StartTicks"] < s["EndTicks"] for s in segs))
            self.assertEqual(self.jget(get, f"/MediaSegments/{mock_jf.gid(102)}")["Items"], [])
            nxt = self.jget(get, "/Shows/NextUp?EnableResumable=false&EnableRewatching=false")["Items"]
            self.assertTrue(nxt and all(e["UserData"]["PlaybackPositionTicks"] == 0 for e in nxt))
            for view in self.jget(get, "/UserViews")["Items"]:
                status, ctype, body = get(f"/Items/{view['Id']}/Images/Primary?maxWidth=388&maxHeight=222"
                                          f"&tag={view['ImageTags']['Primary']}")
                self.assertEqual((status, ctype, body[:2]), (200, "image/jpeg", b"\xff\xd8"), view["Name"])
                genres = self.jget(get, f"/Genres?ParentId={view['Id']}")["Items"]
                self.assertTrue(genres, view["Name"])
                tile = next(g for g in genres if "Primary" in g["ImageTags"])
                self.assertEqual(get(f"/Items/{tile['Id']}/Images/Primary?maxWidth=240")[0], 200)
            nosferatu = self.jget(get, f"/Items/{mock_jf.gid(109)}")
            self.assertNotEqual(nosferatu["OriginalTitle"], nosferatu["Name"])
            spring = self.jget(get, f"/Items/{mock_jf.gid(101)}")
            self.assertEqual(spring["OriginalTitle"], spring["Name"])

    def test_sintel_plays_from_its_file(self):
        with Served(self.demo) as get:
            sintel = mock_jf.gid(102)
            status, _, body = get(f"/Items/{sintel}/PlaybackInfo", data=b"{}")
            src = json.loads(body)["MediaSources"][0]
            self.assertEqual(status, 200)
            self.assertTrue(src["SupportsDirectPlay"])
            self.assertIn("Video", [s["Type"] for s in src["MediaStreams"]])
            status, _, body = get(f"/Videos/{sintel}/stream.{src['Container']}?static=true&MediaSourceId={src['Id']}",
                                  headers={"Range": "bytes=0-99"})
            self.assertEqual((status, len(body)), (206, 100))


if __name__ == "__main__":
    if not HAVE_CACHE:
        print("test_mock_jf: the Demo cases are SKIPPED: no derived demo cache "
              f"({mock_pms.demo_cache_dir()}); `make demo-library` builds it", file=sys.stderr)
    unittest.main()
