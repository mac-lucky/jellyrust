# Playback settings

Settings has **Video & playback** and **Audio & subtitles** pages. Quality and Direct Play
are saved on this television; language preferences are saved to the signed-in Jellyfin user on
the server and are shared with that user's other Jellyfin clients.

## Quality and Direct Play

Default quality uses the same ladder and saved preference as the player's More menu. Settings
changes apply to the next playback; the player menu retains its existing current-playback behavior.

Direct Play defaults to **Auto**, which uses device capabilities and normal routing.
**Disabled** excludes original-file playback while allowing a remux or transcode.
**Force Direct Play (advanced)** attempts original playback through supported engine paths,
overrides the saved quality and disables automatic remux/transcode fallback. It does not add
codec implementations of its own: TrueHD and the other formats the app converts to AC-3 play under
Force exactly as under Auto. It cannot guarantee that the TV accepts a stream.

Entering Force requires an explicit acknowledgement, initially focused on Cancel. The warning
names missing sound, incorrect pictures, freezes, crashes and the possibility that the app
cannot recover gracefully. It instructs the user to enable Force only if they know how to
restart the app and return to Auto. An enabled-state warning remains visible in Settings;
the quality controls explain that the saved quality is overridden. Returning to Auto or
Disabled needs no confirmation.

Audio passthrough remains the television's Digital Sound Output setting.

## Jellyfin user preferences

**Audio & subtitles** edits the six playback fields of the signed-in user's Jellyfin
`UserConfiguration` (`jf/preferences.rs`, issue #41):

| Row | Field | Options |
|---|---|---|
| Preferred audio language | `AudioLanguagePreference` | No preference, or one of the server's languages |
| Subtitles | `SubtitleMode` | As the file marks them (`Default`), When audio isn't in the subtitle language (`Smart`), Always, Only forced subtitles (`OnlyForced`), Off (`None`) |
| Subtitle language | `SubtitleLanguagePreference` | No preference, or one of the server's languages |
| Default audio track | `PlayDefaultAudioTrack` | Play the file's default track, or Prefer my audio language |
| Remember audio choice | `RememberAudioSelections` | On (the server's default), Off |
| Remember subtitle choice | `RememberSubtitleSelections` | On (the server's default), Off |

With a Remember field on, the server keeps the track each progress report names for that film and
serves it back as the source's `DefaultAudioStreamIndex`/`DefaultSubtitleStreamIndex` (subtitles
Off as -1), which playback starts on (`docs/jellyfin-playback.md`, "Remembered tracks").

The values live on the server, per user, so every Jellyfin client of that user shares them, and
nothing of them is stored on the television. Playback reads them from `/Users/Me` at each start
(`Jf::language_prefs`), so a save applies from the next playback.

The page's first row, **Dubbing and voice-over**, is the exception: Jellyfin has no such field, so
it is saved on this television (`Session::voice_preference`, captured into `ResolveEnv` at each
play), its row says so, and the page lists it from the moment it opens, whether or not the account
preferences load. It ranks audio tracks by what their title names (`route::voice_kind`: "lektor
ai"/"ai lektor" is an AI voice-over, "lektor"/"voice-over" a voice-over, "dubbing" or the word
"dub" a dub):

| Option | Effect on the automatic pick |
|---|---|
| No preference (default) | none: the ladder as before |
| Dubbing first | within the chosen language: dubbing, an unnamed track, a voice-over, an AI voice-over |
| Dubbing, never voice-over | voice-over and AI voice-over tracks are never picked; dubbing first in its language |
| No AI voice-over | AI voice-over tracks are never picked |

An excluded track is skipped by every rung of the audio ladder (the server's pick, the languages,
the file's default, the any-playable fallback) on both direct play and conversion, and a
conversion names an allowed track rather than leaving the choice to the server. When every track
would be excluded, none is; under Force Direct Play, which cannot convert, neither is one when the
exclusion leaves nothing it can force-play. A track picked by hand (the player's menu, the Detail
page's chooser) plays as picked for that playback; a pick the server remembers comes back as its
pick next time, and the preference skips it again if it excludes it. The Smart subtitle mode
judges by the audio that plays: the direct-play pick, or the track a conversion carries. The
Detail page's facts row walks the same ladder under the same preference.

An external audio track (Jellyfin's `IsExternal`: a sidecar file beside the video, such as
`ai-lektor`'s `<video>.pl.lektor.mka`) is never direct-played, because the file direct play reads
does not hold it: an automatic pick that lands on one is a conversion (the server remuxes it in),
unless a better-ranked track in its language direct-plays; Force Direct Play passes over it; a
hand pick of one in the player asks the server for it instead of switching natively; and a retry or
quality change while one plays converts it in again rather than swapping it for an embedded track.

A load reads `GET /Users/Me` (its `Id` must be the signed-in user, or the answer is refused as
stale) and `GET /Localization/Cultures`. The languages are the server's list, keyed by
`ThreeLetterISOLanguageName` (`eng`, `fre`): the codes the server stores and reports on a stream,
and the list jellyfin-web's settings page offers. A stored code the server no longer lists stays
visible and selected.

`POST /Users/Configuration?userId=` replaces the WHOLE configuration, so a save reads
`/Users/Me` again, changes only the field picked, and sends every other field back as the server
gave it. If the six fields no longer match what the page showed (another client changed them),
the save is refused as stale and the page reloads instead of overwriting a change nobody saw. A
pick of the value already checked sends nothing. Confirmed values stay authoritative if a save
fails; the page offers Retry. A request captured under one sign-in cannot be saved under another.

Jellyfin has no forced-subtitle preference beside the mode, so the Plex-era *Forced subtitles*
row is gone; `OnlyForced` is a Subtitles mode.

## Verification boundary

Host tests drive the load and save against a fake server (`jf/preferences_tests.rs`). A real
user's preferences must not be changed as part of verification: use a throwaway user on a test
server.

Host tests cannot establish DTS sound, synchronization, resource allocation or DTS-HD core
acceptance. Those require the synthetic DTS fixture and server-backed playback on the television.
A muted device run proves pipeline behavior, not audible output.

Auto keeps the measured DTS channel ceiling: DTS-HD sources reporting more channels than that
ceiling (including 7.1 on the six-channel dev set), or no usable channel count, still convert on
the server. Stripping an HD extension alone does not prove the remaining core fits the decoder;
this change does not advertise all DTS-HD streams as directly playable.

### Implementation verification — 2026-09-27

- `make check` passed: default and hostsim Rust suites, feature checks, native helpers and the
  Python harness. After the final initial-load focus correction, all ten composed Settings
  tests passed, including load/retry focus, save focus, and Force confirmation/cancellation.
- Shipping-feature check (`--no-default-features`, `CARGO_INCREMENTAL=0`), ARM cross-build,
  ELF assertions, and the supported firmware loader matrix passed. No new FFI was introduced.
- All three committed replay anchors were freshly recorded after the state-shape change and
  passed both Targets and Resolve: six runs, every difference counter zero. The alphabet adds
  only source-backed Direct Play enum values and fixed Settings copy.
- Simulator: full account language picker, initial focus, regional-code save to the mock,
  warning with Cancel selected, and persistent Force/quality-override presentation inspected.
- LG webOS 4.10.2 debug install: Force warning and persistent enabled-state copy captured and
  inspected. A server-backed H.264/AC3 case passed while Force was persisted, using the managed
  Guest test identity. Direct Play was then returned to Auto.
- Native H.264/DTS 5.1 synthetic playback passed all pipeline assertions, including declaration,
  video binding, clock progress, displayed frames and in-place seeking.
- DTS-HD MA 5.1 core playback passed the same assertions using a temporary interoperability
  fixture. The public source sample's truncated tail was removed before looping its valid first
  3.4 seconds; every one of the resulting 5,532 audio packets contained a complete core. The
  malformed packet guard remains intact. The sample media is not distributed in this repository.

The television remained muted with its panel off. These results do not claim audible output
verification, and no real Plex account language preferences were written. The throwaway-account
PMS propagation measurement was never made, and is moot since #41: the page no longer writes plex.tv.
