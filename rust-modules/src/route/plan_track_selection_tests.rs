//! Audio and subtitle track selection tests: the Jellyfin user-preference ladder
//! (`PlayDefaultAudioTrack`, `AudioLanguagePreference`, `SubtitleMode`), direct-play-eligible
//! fallbacks, and subtitle ordinal resolution.

use super::*;
#[allow(unused_imports)]
use super::test_support::*;

fn lang(l: &'static str) -> AudioLangPrefs<'static> {
    AudioLangPrefs { language: Some(l), prefer_default: false, ..Default::default() }
}

fn subs_mode(mode: SubtitleMode, language: Option<&'static str>) -> SubtitleLangPrefs<'static> {
    SubtitleLangPrefs { language, mode }
}

fn forced(mut s: crate::metadata::Stream) -> crate::metadata::Stream {
    s.forced = true;
    s
}

fn flagged_default(mut s: crate::metadata::Stream) -> crate::metadata::Stream {
    s.default = true;
    s
}

#[test]
fn an_empty_track_list_falls_back_to_the_codec_default() {
    assert_eq!(
        pick_dp_audio(&[], "ac3").map(|(i, c, _)| (i, c)),
        Some((-1, "ac3".into()))
    );
    assert!(
        pick_dp_audio(&[], "truehd").is_none(),
        "a non-direct-playable default must convert"
    );
}

/// **With no language preference, the file's default track wins — English has no say.**
/// Issue #202: a French MKV whose default-flagged track is French, beside an English one, opened
/// in English because of a built-in English rung. Both orderings, so the pick is the flag and
/// not the list position.
#[test]
fn the_files_default_track_wins_without_a_language_preference() {
    let tracks = [trk(1, "ac3", "fre", true), trk(2, "ac3", "eng", false)];
    assert_eq!(pick_dp_audio(&tracks, "ac3"), Some((0, "ac3".into(), 1)));
    let tracks = [trk(1, "ac3", "eng", false), trk(2, "eac3", "fre", true)];
    assert_eq!(pick_dp_audio(&tracks, "eac3"), Some((1, "eac3".into(), 2)));
}

/// The conversion half of #202: a video transcode of the same French file must not swap the
/// French default for an English track either.
#[test]
fn a_conversion_without_a_preference_keeps_the_files_default_language() {
    let tracks = [trk(1, "ac3", "fre", true), trk(2, "dca", "eng", false)];
    let dp = pick_dp_audio(&tracks, "ac3").map(|(_, _, id)| id).unwrap_or(0);
    assert_eq!(dp, 1, "direct play takes the French default");
    assert_eq!(encode_audio_id(dp, 0, &tracks, AudioLangPrefs::default()), 1,
        "a cold conversion names the French default, not the English DTS");
}

/// `AudioLanguagePreference` without `PlayDefaultAudioTrack`: the preferred language outranks the
/// file's flag on every path.
#[test]
fn the_users_audio_language_selects_its_track_for_every_cold_play_path() {
    let tracks = [trk(1, "ac3", "eng", true), trk(2, "ac3", "fre", false)];
    let (_, _, dp) = pick_dp_audio_pref(&tracks, "ac3", lang("fra")).unwrap();
    assert_eq!(dp, 2, "direct play honours AudioLanguagePreference");
    assert_eq!(encode_audio_id(dp, 0, &tracks, lang("fra")), 2,
        "a conversion names the preferred-language track");
}

/// `PlayDefaultAudioTrack` (Jellyfin's default) puts the file's flagged track first; the
/// language preference only decides when the file flags nothing.
#[test]
fn play_default_audio_track_outranks_the_language_preference() {
    let prefs = AudioLangPrefs { language: Some("fra"), prefer_default: true, ..Default::default() };
    let tracks = [trk(1, "ac3", "eng", true), trk(2, "ac3", "fre", false)];
    assert_eq!(pick_dp_audio_pref(&tracks, "ac3", prefs), Some((0, "ac3".into(), 1)));
    assert_eq!(encode_audio_id(1, 0, &tracks, prefs), 1);
    let unflagged = [trk(1, "ac3", "eng", false), trk(2, "ac3", "fre", false)];
    assert_eq!(pick_dp_audio_pref(&unflagged, "ac3", prefs), Some((1, "ac3".into(), 2)),
        "no flagged track: the preferred language decides");
}

#[test]
fn an_unmatched_or_blank_language_falls_to_the_file_default() {
    let tracks = [trk(1, "ac3", "eng", true), trk(2, "ac3", "fra", false)];
    for language in ["deu", "", "  "] {
        assert_eq!(pick_dp_audio_pref(&tracks, "ac3", lang(language)), Some((0, "ac3".into(), 1)),
            "{language:?}");
        assert_eq!(encode_audio_id(1, 0, &tracks, lang(language)), 1, "{language:?}");
    }
}

#[test]
fn the_language_preference_matches_every_spelling() {
    for stream in ["fr", "fre", "fra"] {
        let tracks = [trk(1, "ac3", "eng", true), trk(2, "ac3", stream, false)];
        assert_eq!(pick_dp_audio_pref(&tracks, "ac3", lang("fra")), Some((1, "ac3".into(), 2)),
            "{stream}");
        assert_eq!(encode_audio_id(2, 0, &tracks, lang("fra")), 2, "{stream}");
    }
}

/// A preferred track that cannot direct-play does not force a conversion on direct play; the
/// conversion path, which converts anyway, carries the preferred track itself.
#[test]
fn an_undecodable_preferred_track_is_carried_only_by_a_conversion() {
    let tracks = [trk(1, "ac3", "eng", true), trk(2, "dca", "hun", false)];
    let (_, _, dp) = pick_dp_audio_pref(&tracks, "ac3", lang("hun")).unwrap();
    assert_eq!(dp, 1, "direct play keeps the decodable default");
    assert_eq!(encode_audio_id(dp, 0, &tracks, lang("hun")), 2, "a conversion carries the Hungarian DTS");
}

#[test]
fn the_flagged_default_wins_over_an_earlier_track() {
    let tracks = [trk(1, "ac3", "deu", false), trk(2, "ac3", "fra", true)];
    assert_eq!(pick_dp_audio(&tracks, "ac3"), Some((1, "ac3".into(), 2)));
}

#[test]
fn smart_dp_takes_a_playable_sibling_over_a_non_playable_default() {
    // A 4K HEVC item: TrueHD default + an AC3 sibling — direct play beats the server's
    // video-downscaling transcode.
    let tracks = [trk(1, "truehd", "eng", true), trk(2, "ac3", "eng", false)];
    assert_eq!(pick_dp_audio(&tracks, "truehd"), Some((1, "ac3".into(), 2)));
}

#[test]
fn no_direct_playable_track_means_conversion() {
    let tracks = [trk(1, "truehd", "eng", true), trk(2, "dts", "eng", false)];
    assert!(pick_dp_audio(&tracks, "truehd").is_none());
}

/// A conversion keeps a session/retry pick, and with nothing picked names the file's default
/// whatever its codec (the server converts it), not the smart-DP sibling.
#[test]
fn a_conversion_keeps_the_session_pick_then_the_default() {
    let tracks = [trk(1, "truehd", "eng", true), trk(2, "ac3", "fra", false)];
    let dp = pick_dp_audio(&tracks, "truehd").map(|(_, _, id)| id).unwrap_or(0);
    assert_eq!(dp, 2, "smart-DP sibling is the French AC3");
    assert_eq!(encode_audio_id(dp, 0, &tracks, AudioLangPrefs::default()), 1,
        "a cold conversion names the TrueHD default");
    assert_eq!(encode_audio_id(dp, 2, &tracks, AudioLangPrefs::default()), 2,
        "a retry keeps the track already playing");
}

/// The preferred dub already playing as the direct-play pick is kept by a conversion rather than
/// re-encoding a lossless sibling in the same language.
#[test]
fn a_conversion_keeps_a_direct_play_pick_already_in_the_preferred_language() {
    let tracks = [trk(1, "truehd", "eng", true), trk(2, "ac3", "eng", false)];
    assert_eq!(encode_audio_id(2, 0, &tracks, lang("eng")), 2);
}

/// The server's pick (`DefaultAudioStreamIndex`, `selected`) comes first on both paths, ahead of
/// the file's default and the user's language.
#[test]
fn the_servers_audio_pick_comes_first() {
    let tracks = [trk(1, "ac3", "eng", true), server_selected(trk(2, "ac3", "pol", false))];
    let prefs = AudioLangPrefs { language: Some("eng"), prefer_default: true, ..Default::default() };
    assert_eq!(pick_dp_audio_pref(&tracks, "ac3", prefs), Some((1, "ac3".into(), 2)));
    assert_eq!(encode_audio_id(2, 0, &tracks, prefs), 2);
}

/// A server pick the panel cannot decode is carried by a conversion; direct play falls through,
/// to a track in the same language first, so a remembered English choice does not turn into the
/// Polish default.
#[test]
fn an_undecodable_server_pick_falls_through_to_its_own_language() {
    let tracks = [trk(1, "ac3", "pol", true), server_selected(trk(2, "truehd", "eng", false)), trk(3, "ac3", "eng", false)];
    let prefs = AudioLangPrefs { language: Some("pol"), prefer_default: true, ..Default::default() };
    let (_, _, dp) = pick_dp_audio_pref(&tracks, "ac3", prefs).unwrap();
    assert_eq!(dp, 3, "the English AC-3 sibling");
    assert_eq!(encode_audio_id(dp, 0, &tracks, prefs), 2, "a conversion carries the TrueHD pick itself");
}

/// An external text subtitle the client draws itself is a candidate for the automatic pick, ranked
/// ahead of an embedded one as the server ranks it; it comes back with the sidecar's ordinal (-1).
#[test]
fn an_automatic_pick_takes_a_drawable_external_subtitle_first() {
    let external = |id, index, lang| crate::metadata::Stream { key: format!("/Videos/x/x/Subtitles/{index}/0/Stream.srt"), ..sub(id, index, lang, true) };
    let subs = [sub(10, 3, "pol", false), external(11, 4, "pol")];
    assert_eq!(pick_dp_subtitle_pref(&subs, subs_mode(SubtitleMode::Always, Some("pol")), "eng"), Some((11, -1)));
    assert_eq!(pick_dp_subtitle_pref(&subs, SubtitleLangPrefs::default(), "eng"), Some((11, -1)),
        "Default turns an external track on, as the server's own Default mode does");
    let unreachable = [sub(10, 3, "pol", false), sub(11, 4, "pol", true)];
    assert_eq!(pick_dp_subtitle_pref(&unreachable, subs_mode(SubtitleMode::Always, Some("pol")), "eng"), Some((10, 0)),
        "an external track with no file to fetch is never picked");
}

#[test]
fn the_selected_subtitle_resolves_to_the_renderers_embedded_ordinal() {
    // Document order is NOT container order and a sidecar sits in the middle of the list:
    // the renderer counts only embedded streams, sorted on `Stream.index` — the same
    // identifier space the track menu commits (metadata::sub_render_ordinal).
    let subs = [
        sub(10, 7, "fra", true),  // sidecar — not in the container, not counted
        sub(11, 3, "rus", false), // embedded, container-first
        server_selected(sub(12, 4, "eng", false)),
    ];
    assert_eq!(pick_dp_subtitle(&subs), Some((12, 1)));
}

#[test]
fn an_external_selected_subtitle_is_left_to_the_sidecar_path() {
    let subs = [
        server_selected(sub(10, 3, "eng", true)),
        sub(11, 4, "rus", false),
    ];
    assert_eq!(pick_dp_subtitle(&subs), None);
    assert_eq!(pick_dp_subtitle_pref(&subs, subs_mode(SubtitleMode::Always, Some("rus")), "eng"), None,
        "an explicit sidecar selection keeps automatic picks out");
}

#[test]
fn a_selection_with_no_stream_id_is_left_off_rather_than_half_applied() {
    // id and ordinal travel together: the id is what the menu checkmark and the reports key
    // on, so an id-less stream would render subtitles while the menu said Off.
    let subs = [server_selected(sub(0, 3, "eng", false))];
    assert_eq!(pick_dp_subtitle(&subs), None);
}

#[test]
fn subtitle_mode_none_never_turns_a_subtitle_on() {
    let subs = [flagged_default(sub(10, 3, "eng", false)), forced(sub(11, 4, "eng", false))];
    assert_eq!(pick_dp_subtitle_pref(&subs, subs_mode(SubtitleMode::None, Some("eng")), "fra"), None);
}

/// `Default`: what the file flags (default or forced), preferred language ranked first; an
/// unflagged file stays off.
#[test]
fn subtitle_mode_default_follows_the_files_flags() {
    let subs = [sub(10, 3, "eng", false), sub(11, 4, "fra", false)];
    assert_eq!(pick_dp_subtitle_pref(&subs, SubtitleLangPrefs::default(), "eng"), None);
    let subs = [forced(sub(10, 3, "eng", false)), flagged_default(sub(11, 4, "fra", false))];
    assert_eq!(pick_dp_subtitle_pref(&subs, SubtitleLangPrefs::default(), "eng"), Some((10, 0)));
    assert_eq!(pick_dp_subtitle_pref(&subs, subs_mode(SubtitleMode::Default, Some("fra")), "eng"), Some((11, 1)),
        "a flagged track in the preferred language ranks first");
}

/// `Always`: the preferred language's FULL track, not the forced one before it; with none in it,
/// what the file flags.
#[test]
fn subtitle_mode_always_takes_the_full_preferred_track() {
    let subs = [sub(10, 3, "eng", false), forced(sub(11, 4, "hun", false)), sub(12, 5, "hun", false)];
    assert_eq!(pick_dp_subtitle_pref(&subs, subs_mode(SubtitleMode::Always, Some("hun")), "hun"), Some((12, 2)));
    let subs = [sub(10, 3, "eng", false), flagged_default(sub(11, 4, "deu", false))];
    assert_eq!(pick_dp_subtitle_pref(&subs, subs_mode(SubtitleMode::Always, Some("hun")), "eng"), Some((11, 1)));
}

/// `Smart`: the preferred language when the audio is not in it; with the audio already in it,
/// only a forced track.
#[test]
fn subtitle_mode_smart_judges_by_the_audio_language() {
    let subs = [sub(10, 3, "eng", false), forced(sub(11, 4, "hun", false)), sub(12, 5, "hun", false)];
    let smart = subs_mode(SubtitleMode::Smart, Some("hun"));
    assert_eq!(pick_dp_subtitle_pref(&subs, smart, "eng").map(|p| p.0), Some(11),
        "foreign audio: the first preferred-language track");
    assert_eq!(pick_dp_subtitle_pref(&subs, smart, "hun"), Some((11, 1)),
        "audio already in the preferred language: forced only");
    let unforced = [sub(10, 3, "eng", false), sub(12, 5, "hun", false)];
    assert_eq!(pick_dp_subtitle_pref(&unforced, smart, "hun"), None);
}

#[test]
fn subtitle_mode_only_forced_ignores_full_tracks() {
    let subs = [sub(10, 3, "eng", false), forced(sub(11, 4, "eng", false))];
    assert_eq!(pick_dp_subtitle_pref(&subs, subs_mode(SubtitleMode::OnlyForced, None), "eng"), Some((11, 1)));
    assert_eq!(pick_dp_subtitle_pref(&subs[..1], subs_mode(SubtitleMode::OnlyForced, None), "eng"), None);
}

#[test]
fn automatic_subtitles_skip_unrenderable_tracks_without_enabling_a_burn() {
    let mut subs = [sub(1, 0, "fra", false), sub(2, 1, "fra", true), sub(3, 2, "fra", false)];
    subs[0].codec = "unsupported".into();
    let always = subs_mode(SubtitleMode::Always, Some("fra"));
    assert_eq!(pick_dp_subtitle_pref(&subs, always, "eng"), Some((3, 1)));
    assert_eq!(pick_dp_subtitle_pref(&subs[..2], always, "eng"), None);
}

#[test]
fn subtitle_modes_parse_jellyfins_names() {
    for (name, mode) in [
        ("Default", SubtitleMode::Default),
        ("Always", SubtitleMode::Always),
        ("OnlyForced", SubtitleMode::OnlyForced),
        ("None", SubtitleMode::None),
        ("Smart", SubtitleMode::Smart),
        ("", SubtitleMode::Default),
    ] {
        assert_eq!(SubtitleMode::parse(name), mode, "{name:?}");
    }
}

#[test]
fn preferred_language_tags_match_both_three_letter_spellings() {
    assert!(lang_matches("hu-HU", "hun"));
    assert!(lang_matches("de-DE", "ger") && lang_matches("de-DE", "deu"));
    assert!(lang_matches("pt-BR", "por"));
    assert!(lang_matches("en-GB", "eng"));
    assert!(!lang_matches("hu-HU", "eng"));
    assert!(!lang_matches("", "hun"));
    assert!(!lang_matches("hu-HU", ""));
    assert!(lang_matches("fre", "fra") && lang_matches("ger", "deu") && lang_matches("no", "nob"));
    assert!(!lang_matches("fre", "eng") && !lang_matches("", "") && !lang_matches("xx", "yy"));
}

#[test]
fn known_audio_channels_cannot_fall_back_to_an_unknown_codec_default() {
    let caps = nj_platform::devcaps::Caps { audio_channels: [("aac".into(), 6)].into(), ..nj_platform::devcaps::Caps::assumed() };
    let mut track = trk(1, "aac", "eng", false);
    track.channels = 8;
    assert!(caps.audio_supports("aac", 0), "legacy unknown remains compatible");
    assert_eq!(pick_dp_audio_eligible(&[track.clone()], "aac", AudioLangPrefs::default(), |c, n| caps.audio_supports(c, n), true), None);
    track.channels = 6;
    assert_eq!(pick_dp_audio_eligible(&[track], "aac", AudioLangPrefs::default(), |c, n| caps.audio_supports(c, n), true), Some((0, "aac".into(), 1)));
}

/// The catalog spells DTS `dca` (`jf::convert::codec`) and the device table `dts`: both modes
/// read the track through one canonicaliser, so a set with a DTS row direct-plays it and a set
/// without one (the C1 of issue #24) converts it, in Auto and in Force Direct Play alike.
#[test]
fn a_dca_track_follows_the_dts_row_in_both_modes() {
    let mut with_row = nj_platform::devcaps::Caps::assumed();
    with_row.audio = "aac,ac3,eac3,dts".into();
    with_row.audio_channels.insert("dts".into(), 6);
    let without = nj_platform::devcaps::Caps::assumed();
    for mode in [DirectPlayMode::Auto, DirectPlayMode::Forced] {
        assert!(audio_direct_plays_with(&with_row, mode, "dca", 6), "{mode:?}");
        assert!(!audio_direct_plays_with(&without, mode, "dca", 6), "{mode:?}");
    }
    assert!(!audio_direct_plays_with(&with_row, DirectPlayMode::Auto, "dca", 8), "Auto keeps the ceiling");
    assert!(audio_direct_plays_with(&with_row, DirectPlayMode::Forced, "dca", 8), "Forced skips it");
}

#[test]
fn only_an_eac3_atmos_track_is_logged_as_declared_immersive() {
    assert_eq!(atmos_plan_line("eac3"), "audio: dolby atmos — declaring contents.immersive=ATMOS");
    for codec in ["truehd", "aac"] {
        let line = atmos_plan_line(codec);
        assert!(!line.contains("declaring"), "{codec}: {line}");
        assert!(line.contains(codec) && line.contains("not declared"), "{codec}: {line}");
    }
}

// ---- Dubbing and lektor: the voice preference over the same ladder ----

fn titled(title: &str, s: crate::metadata::Stream) -> crate::metadata::Stream {
    crate::metadata::Stream { title: title.into(), ..s }
}

/// A Polish speaker with Jellyfin's default `PlayDefaultAudioTrack`, under `voice`.
fn voice(voice: VoicePreference) -> AudioLangPrefs<'static> {
    AudioLangPrefs { language: Some("pol"), prefer_default: true, voice }
}

/// Both cold-play paths: (the direct-play pick's id, the id a conversion of it carries).
fn picks(tracks: &[crate::metadata::Stream], prefs: AudioLangPrefs<'_>) -> (i64, i64) {
    let dp = pick_dp_audio_pref(tracks, "ac3", prefs).map_or(0, |(_, _, id)| id);
    (dp, encode_audio_id(dp, 0, tracks, prefs))
}

/// `ai-lektor`'s track the way the household's library has it: an external sidecar
/// (`<video>.pl.lektor.mka`, one stereo AAC stream) that Jellyfin lists after every embedded
/// stream, `pol`, titled "Lektor AI", not default.
fn sidecar(id: i64) -> crate::metadata::Stream {
    crate::metadata::Stream { external: true, ..titled("Lektor AI", trk(id, "aac", "pol", false)) }
}

/// The household's shape: the English original (the file's default), a Polish human lektor, and
/// the AI lektor sidecar.
fn household() -> Vec<crate::metadata::Stream> {
    vec![
        titled("English", trk(1, "ac3", "eng", true)),
        titled("Polish - Lektor - AC3", trk(2, "ac3", "pol", false)),
        sidecar(9),
    ]
}

#[test]
fn a_tracks_title_names_its_voice_kind() {
    for (title, kind) in [
        ("Lektor AI", VoiceKind::AiLektor),
        ("ai lektor", VoiceKind::AiLektor),
        ("POLISH - LEKTOR AI - AAC - Stereo", VoiceKind::AiLektor),
        ("Polish - Lektor - AC3", VoiceKind::Lektor),
        ("LEKTOR PL", VoiceKind::Lektor),
        ("Voice-over", VoiceKind::Lektor),
        ("voiceover", VoiceKind::Lektor),
        ("Polish Voice Over", VoiceKind::Lektor),
        ("Dubbing PL", VoiceKind::Dubbing),
        ("PL Dub", VoiceKind::Dubbing),
        ("Polish [DUB] 5.1", VoiceKind::Dubbing),
        ("English", VoiceKind::Unknown),
        ("", VoiceKind::Unknown),
        ("Dubstep commentary", VoiceKind::Unknown),
    ] {
        assert_eq!(voice_kind(&titled(title, trk(1, "ac3", "pol", false))), kind, "{title:?}");
    }
}

/// Any is the ladder as it was: the server's pick, AI lektor or not, then the file's default. The
/// AI lektor is a sidecar the file direct play reads does not hold, so landing on it is a
/// conversion that carries it (the server's remux of an `AudioIsExternal` track), never a
/// direct play of something else under its name.
#[test]
fn any_voice_keeps_the_ladder_unchanged() {
    let mut tracks = household();
    tracks[2].selected = true;
    assert_eq!(picks(&tracks, voice(VoicePreference::Any)), (0, 9));
    assert_eq!(picks(&household(), voice(VoicePreference::Any)), (1, 1));
}

/// "Dubbing first" moves a landing on the AI sidecar to a better-ranked track in its language that
/// direct-plays, so the preference costs no conversion when the file already holds the better track.
#[test]
fn a_better_ranked_embedded_track_spares_the_sidecars_conversion() {
    let mut tracks = household();
    tracks[2].selected = true;
    assert_eq!(picks(&tracks, voice(VoicePreference::DubbingFirst)), (2, 2));
}

/// Force Direct Play has no conversion to carry a sidecar, so its rungs pass over one the way they
/// pass over a codec the panel cannot decode; and when the voice preference leaves nothing it can
/// force-play while an excluded track could play, the exclusion is ignored, as when it excludes
/// every track. Auto converts instead (the English TrueHD).
#[test]
fn force_direct_play_passes_over_a_sidecar_and_drops_an_exclusion_it_cannot_honour() {
    let caps = nj_platform::devcaps::Caps::assumed();
    let mut tracks = household();
    tracks[2].selected = true;
    assert_eq!(pick_dp_audio_mode(&caps, &tracks, "ac3", voice(VoicePreference::Any), DirectPlayMode::Forced).map(|p| p.2), Some(2));
    let lektor_only_playable = [
        titled("English", trk(1, "truehd", "eng", true)),
        server_selected(titled("Lektor", trk(2, "ac3", "pol", false))),
    ];
    let never = voice(VoicePreference::DubbingNoLektor);
    assert_eq!(pick_dp_audio_mode(&caps, &lektor_only_playable, "truehd", never, DirectPlayMode::Forced).map(|p| p.2), Some(2));
    assert_eq!(pick_dp_audio_mode(&caps, &lektor_only_playable, "truehd", never, DirectPlayMode::Auto), None);
}

/// "No AI lektor": a server pick of the AI track (remembered, or the server ranking the Polish
/// preference) is skipped, and its language rung takes the human lektor rather than the same AI
/// track again; with no human lektor the ladder goes on to the file's default. A human lektor the
/// server picked still plays: nothing else changes.
#[test]
fn no_ai_lektor_skips_a_server_pick_of_the_ai_track() {
    let no_ai = voice(VoicePreference::NoAiLektor);
    let mut tracks = household();
    tracks[2].selected = true;
    assert_eq!(picks(&tracks, no_ai), (2, 2));
    let no_human = [tracks[0].clone(), tracks[2].clone()];
    assert_eq!(picks(&no_human, no_ai), (1, 1));
    let mut human = household();
    human[1].selected = true;
    assert_eq!(picks(&human, no_ai), (2, 2));
}

/// "Dubbing, never lektor" with no Polish dub: a server pick of the human lektor is skipped, no
/// Polish track is left, and the English original plays, whichever rung comes first.
#[test]
fn never_lektor_skips_a_server_pick_of_a_human_lektor_for_the_original() {
    let mut tracks = household();
    tracks[1].selected = true;
    assert_eq!(picks(&tracks, voice(VoicePreference::DubbingNoLektor)), (1, 1));
    let language_first = AudioLangPrefs { prefer_default: false, ..voice(VoicePreference::DubbingNoLektor) };
    assert_eq!(picks(&tracks, language_first), (1, 1));
}

/// "Dubbing, never lektor" with a Polish dub: the dub plays, ahead of the lektor the server picked
/// and ahead of an untitled Polish track the server picked.
#[test]
fn never_lektor_plays_the_dub_first_in_its_language() {
    let never = voice(VoicePreference::DubbingNoLektor);
    let mut tracks = household();
    tracks.push(titled("Dubbing PL", trk(4, "ac3", "pol", false)));
    tracks[1].selected = true;
    assert_eq!(picks(&tracks, never), (4, 4));
    let untitled = [
        titled("English", trk(1, "ac3", "eng", true)),
        server_selected(trk(2, "ac3", "pol", false)),
        titled("PL Dub", trk(3, "ac3", "pol", false)),
    ];
    assert_eq!(picks(&untitled, never), (3, 3));
}

/// "Dubbing first" ranks inside the language the ladder chose: dubbing, an unnamed track, a
/// lektor, the AI lektor. Another language is never a step up, so a remembered English pick stays.
#[test]
fn dubbing_first_ranks_within_the_language_only() {
    let first = voice(VoicePreference::DubbingFirst);
    let all = vec![
        titled("English", trk(1, "ac3", "eng", true)),
        server_selected(titled("Lektor AI", trk(2, "ac3", "pol", false))),
        titled("Lektor", trk(3, "ac3", "pol", false)),
        titled("Polski", trk(4, "ac3", "pol", false)),
        titled("Dubbing", trk(5, "ac3", "pol", false)),
    ];
    assert_eq!(picks(&all, first), (5, 5), "dubbing");
    assert_eq!(picks(&all[..4], first), (4, 4), "then the unnamed track");
    assert_eq!(picks(&all[..3], first), (3, 3), "then the human lektor");
    assert_eq!(picks(&all[..2], first), (2, 2), "the AI lektor when it is the only Polish track");
    let mut english = all.clone();
    english[1].selected = false;
    english[0].selected = true;
    assert_eq!(picks(&english, first), (1, 1));
}

/// Direct play re-ranks only among tracks the panel decodes; a conversion carries the better-ranked
/// track whatever its codec.
#[test]
fn a_conversion_re_ranks_without_the_codec_limit() {
    let tracks = [
        titled("English", trk(1, "ac3", "eng", true)),
        server_selected(titled("Lektor", trk(2, "ac3", "pol", false))),
        titled("Dubbing", trk(3, "truehd", "pol", false)),
    ];
    assert_eq!(picks(&tracks, voice(VoicePreference::DubbingFirst)), (2, 3));
}

/// A preference that would exclude every track excludes none: something always plays, picked as
/// Any would pick it.
#[test]
fn a_preference_that_would_exclude_every_track_excludes_none() {
    let lektors = [
        titled("Lektor", trk(1, "ac3", "pol", true)),
        server_selected(titled("Lektor AI", trk(2, "ac3", "pol", false))),
    ];
    assert_eq!(picks(&lektors, voice(VoicePreference::DubbingNoLektor)), (2, 2));
    let only_ai = [titled("Lektor AI", trk(1, "ac3", "pol", true))];
    assert_eq!(picks(&only_ai, voice(VoicePreference::NoAiLektor)), (1, 1));
}

/// The conversion obeys the same rule when nothing direct-plays: it names an allowed track rather
/// than sending none, which would let the server choose its own default, the excluded one.
#[test]
fn a_conversion_never_leaves_an_excluded_track_to_the_server() {
    let tracks = [
        server_selected(titled("Lektor", trk(1, "truehd", "pol", true))),
        titled("English", trk(2, "truehd", "eng", false)),
    ];
    let never = voice(VoicePreference::DubbingNoLektor);
    assert_eq!(pick_dp_audio_pref(&tracks, "truehd", never), None);
    assert_eq!(encode_audio_id(0, 0, &tracks, never), 2);
    assert_eq!(encode_audio_id(0, 0, &tracks, voice(VoicePreference::Any)), 1);
    let unflagged = [titled("Lektor AI", trk(1, "truehd", "pol", false)), titled("Polski", trk(2, "truehd", "pol", false))];
    let bare = |voice| AudioLangPrefs { voice, ..Default::default() };
    assert_eq!(encode_audio_id(0, 0, &unflagged, bare(VoicePreference::NoAiLektor)), 2);
    assert_eq!(encode_audio_id(0, 0, &unflagged, bare(VoicePreference::Any)), 0, "Any still leaves it to the server");
}

/// A track picked by hand (the player's menu, the Detail page's chooser, a retry's carry) is
/// carried whatever the preference says about it.
#[test]
fn a_hand_picked_track_is_carried_whatever_the_preference() {
    assert_eq!(encode_audio_id(1, 9, &household(), voice(VoicePreference::NoAiLektor)), 9);
    assert_eq!(encode_audio_id(1, 2, &household(), voice(VoicePreference::DubbingNoLektor)), 2);
}
