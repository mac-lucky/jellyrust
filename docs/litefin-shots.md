# Offline screenshots of the Jellyfin path

Read this before comparing a screen slice against `docs/litefin-ref/`. `tools/litefin-shot.sh`
takes one 1920x1080 simulator screenshot against `tests/mock_jf.py --demo`, a Jellyfin mock that
serves the openly licensed demo library behind `make screenshots`, with its posters, backdrops,
logos, episode stills and the Sintel film. No household server, no sign-in, no watch state
written anywhere, no television.

## Once per machine

```sh
make demo-library   # ~450 MB into ~/.cache/nativejelly-demo ($NJ_DEMO_CACHE); needs ffmpeg and Pillow
```

The cache is shared by every lane. The pinned Wikimedia URL of `nosferatu-1922-backdrop` is dead
(the file became a redirect). The Wayback Machine copy
(`https://web.archive.org/web/2026id_/<pinned url>`) matches the pinned sha256: put it at
`~/.cache/nativejelly-demo/src/nosferatu-1922-backdrop.webp` and rerun. Wikimedia also answers a
burst of downloads with the odd 404; rerunning picks up where it stopped.

## One shot

```sh
tools/litefin-shot.sh <out.png> [trigger[=value] ...]
tools/litefin-shot.sh /tmp/shots/home.png heropin=0
tools/litefin-shot.sh /tmp/shots/library.png library=0          # 1 = TV Shows
tools/litefin-shot.sh /tmp/shots/detail.png detail=105          # Tears of Steel
tools/litefin-shot.sh /tmp/shots/series.png detail=201          # Caminandes
tools/litefin-shot.sh /tmp/shots/person.png detail=105 detailsec=3 detailok   # Vanja Rukavina
tools/litefin-shot.sh /tmp/shots/settings.png settings=root
tools/litefin-shot.sh /tmp/shots/player.png play=102 simvideo clocksink autopause=at=446020
tools/litefin-shot.sh /tmp/shots/menu.png play=102 simvideo clocksink autopause=at=446020 menu=0   # 1 = Subtitles
tools/litefin-shot.sh /tmp/shots/skip.png play=2011002 simvideo clocksink autopause=at=10000
tools/litefin-shot.sh /tmp/shots/upnext.png play=2011002 simvideo clocksink marker=credits autopause=at=156000
tools/litefin-shot.sh /tmp/shots/account.png acct=1500
tools/litefin-shot.sh /tmp/shots/favorites.png favorites=0     # remote on the first film
tools/litefin-shot.sh /tmp/shots/rail.png rail=1               # Favorites' label pill
tools/litefin-shot.sh /tmp/shots/popover.png library=0 rail=libraries
tools/litefin-shot.sh /tmp/shots/rail-settings.png settings=root rail=libraries
tools/litefin-shot.sh /tmp/shots/random.png random
```

Rail triggers:

- `rail=<slot>[,<rest ms>]` waits until 2 s after boot and until the page has been at rest for
  `<rest ms>` (1500 by default), then puts the remote on rail slot `<slot>`. Slots count the items
  shown, in Litefin's order: Home 0, Favorites 1, Search 2, Random 3, Libraries 4 (only while the
  server lists a library), Settings 5, and the account at the item count (6). `rail=libraries`
  opens the Libraries popover as its OK does, on the library on screen (else the first). It gives
  up after 20 s.
- Over Settings: with `settings=<page>` the rail belongs to the Settings page, so the same `rail`
  values shoot it beside Settings (Settings is the current item there, and the popover still
  opens on the library under it).
- `favorites` opens the Favorites page from Home as its rail item does; `favorites=<row>` also
  puts the remote on the first card of row `<row>` once that row has landed (rows in Litefin's
  order, empty lists skipped: on the demo 0 films, 1 series, 2 seasons, 3 episodes, 4 people).
- `random` waits for Home to rest, then picks a film or series as the rail's Random item does;
  its detail page opens when the pick lands. The mock's `SortBy=Random` is a fixed order, so the
  demo opens the same title every run.
- `library` with no value opens the server's preferred Movies library (else the first), which is
  what the fps manifest's `"nativejelly-library": true` writes; `library=<n>` opens the `n`th
  library of the popover. Both wait up to 20 s for the directory to list it.

The cast row sits three DOWNs below the hero (the Show more pill and the metadata table come
first), and the full-width table seats the DOWN on the card under its centre: Vanja Rukavina, the
fourth. Skip and Up Next ride Caminandes S1E2 (2011002), the one demo episode that plays
from a file and has a successor: its intro is 4-20 s, its credits 150-180 s (`SEGMENTS` in
`tests/mock_jf.py`). `marker=intro|credits` seeks 5 s before the segment once playback runs;
`autopause=at=<ms>` then holds the frame inside it, which is what lets the harness call the
screen still.

It builds `make sim-macos` in the worktree it lives in (so every lane uses its own build tree),
starts the mock on a free port, makes a fresh instance root, arms `jf`, `token` (the mock's
placeholder `mock-jf-token`), `pms-origin`, `plextv` (loopback) and `stillclock=4000`, then the
triggers given (each `name=value` becomes `nativejelly-<name>`; the list is in
`docs/agent-reference.md`). It runs `make sim-macos-shot SIM_W=1920 SIM_H=1080` and, once the mock
has answered no new GET for 2 s and the heartbeat shows a still screen (1 fps keepalive at most, or
a paused player), sends `shot` down the remote FIFO, writes the PNG and stops the simulator and the
mock. It prints the mock's 404s (an image an item lacks is a 404 on Jellyfin too, e.g. the logo of
a film without one) and any refused trigger. A cold lane builds for ~15 min first.

Knobs: `SHOT_QUIET_S` (2), `SHOT_AFTER_S` (4, earliest capture), `SHOT_TIMEOUT_S` (180),
`KEEP=1` keeps the instance root (events log, `mock.log`, `sim.out`). `NJ_LOCALE` passes through.

Traps:

- The digit in the top-right corner is the `sim-macos` build's fps counter (the `devtools`
  feature), not UI. `NJ_RENDER_SCALE` would hide it, but a 2x readback on a loaded machine trips the
  main-thread hang watchdog and aborts, so the harness renders at 1x.
- The app's own settled capture (`NJ_SHOT_SETTLE`) is not used: it counts only screen work, so on
  a loaded machine a page still waiting for the mock reads as at rest and comes out empty.
- On a loaded machine the capture's readback can pass the 2 s main-thread hang watchdog, which
  aborts the simulator and leaves an empty frame: add `guard=log` (it downgrades the watchdog to a
  log line) to any shot that comes back empty with `main-thread hang fatal` in the events log.
- Simulator shots are layout and focus evidence; text rasterization and the video plane are TV
  questions.

## ratingKeys

The mock mints each GUID as the key in 13 hex digits plus 19 zeros, and the app interns a GUID to
its first 13 hex digits, so the keys are mock_pms's and `tests/screenshots/scenes.json`'s.

| What | Keys |
|---|---|
| Movies (catalog order) | 101 Spring, 102 Sintel (the real film, resume 49 %, the hero), 103 Sprite Fright, 104 Charge, 105 Tears of Steel (cast with portraits), 106 Cosmos Laundromat, 107 Big Buck Bunny, 108 Elephants Dream, 109 Nosferatu, 110 Metropolis, 112 The General, 113 Sherlock Jr., ..., 127 Seder-Masochism |
| Series | 201 Caminandes (3 episodes, Next Up S1E3), 202 Sherlock Holmes (cast), 203 Hubblecast |
| Season, episode | season = series * 10 + n (2011); episode = season * 1000 + n (2011003 Llamigos) |
| Collections | 50001 Blender Open Movies, 50002 Silent Classics, 50003 Empty Collection, 50004.. name-fitting cases; open one with `collection=50001` |
| People | 70000 + id: 70008 Derek de Lint (first actor of 105), 70032 Buster Keaton; search them with `/Persons` |
| Libraries | 1 Movies, 2 TV Shows |

Clear logos: 101, 102, 103 only. Films without a backdrop (110, 112, 113 and others) fall back to
their poster's BlurHash for the ambient wash.

## What the demo serves

`/UserViews`, `/Items` (ParentId of a view, series, season or BoxSet; Ids, PersonIds, searchTerm,
GenreIds/Genres, NameStartsWith, Filters, SortBy/SortOrder, StartIndex/Limit), `/Items/{id}`,
`/Items/Latest`, `/Items/Filters2`, `/Items/{id}/Similar`, `/UserItems/Resume`, `/Shows/NextUp`,
`/Shows/{id}/Seasons|Episodes`, `/Persons`, `/Genres` (ParentId), `/MediaSegments` (intro and
credits for 2011002 only), PlaybackInfo, and `/Items/{id}/Images/{Primary|Backdrop|Logo|Thumb}[/{index}]`
fitted inside `maxWidth`/`maxHeight` (never upscaled; `format=Png` honoured), person portraits,
BoxSet composites, and library and genre tiles (a library's newest arrival's backdrop, a genre's
first film's). `/Videos/{id}/stream`
serves the Sintel file and the stand-in episodes with Range support. Sign-in, `/Users/Me` (user
"Demo"), configuration, cultures and playback reports are the synthetic mock's. Writes (watched,
resume point, favourite) are acknowledged and forgotten, so every run starts from the catalog's
pinned state. The app requests no user image, so none is served. Without `--demo` the synthetic
library is unchanged.

Stand-ins: the demo library has no portraits, so each actor wears a Blender film poster (computer
drawn, so no real person's likeness carries another's name), dealt round by person id; directors
and writers have none. A person's biography only lists their credits in the demo library, and only
Derek de Lint and Buster Keaton carry a (public-record) birth date. Nosferatu (109) and The Cabinet
of Dr. Caligari carry their original-language titles. Hubblecast's first episode is watched on
this mock alone, so Next Up has an unstarted episode. BlurHashes are 2x2 hashes of each image's
corner averages.

## Licences

Everything served comes from `tests/demo_library/assets.json`, which pins each source's URL,
sha256, licence and attribution: 62 CC BY 4.0, 17 public domain, 8 CC BY 3.0, 3 CC0 1.0 (the
Blender Studio films and artwork, ESA/Hubble, public-domain silent films and posters). The credits
table is `docs/screenshots/CREDITS.md` (`python3 tools/demo_library.py credits` rewrites it).
The stand-in portraits, BoxSet composites and BlurHashes are derived from those same files under their licences. Screenshots
made here show real titles: keep them out of committed fixtures.

## Reference shots

| `docs/litefin-ref/` | Command |
|---|---|
| `home.png`, `readme-hero.jpg`, `sidebar.png` | `heropin=0` |
| `details.png` | `detail=105` (film), `detail=109` (original title, studio, added date), `detail=201` (series) |
| `library.png`, `genres.png` | `library=0` (shelves), `library=0 libgrid=0,2` (the All grid) |
| `player-osd.png` | `play=102 simvideo clocksink autopause=at=446020`, `menu=0` for the audio menu |
