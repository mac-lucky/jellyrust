use super::*;
use serde_json::json;

const USER: &str = "5c0fe2b1a8d84b0e9f3a6c7d8e9f0a1b";

fn request() -> PreferenceRequest {
    PreferenceRequest { server: "http://10.0.0.2:8096".into(), user_id: USER.into() }
}

fn me(config: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({ "Id": USER, "Name": "alex", "Configuration": config })).unwrap()
}

fn config() -> Value {
    json!({
        "AudioLanguagePreference": "eng",
        "PlayDefaultAudioTrack": true,
        "SubtitleLanguagePreference": "",
        "SubtitleMode": "Smart",
        "RememberAudioSelections": true,
        "RememberSubtitleSelections": true,
        "OrderedViews": ["a", "b"],
        "HidePlayedInLatest": false,
        "CastReceiverId": "F007D354",
    })
}

fn cultures_body() -> Vec<u8> {
    serde_json::to_vec(&json!([
        { "DisplayName": "English", "ThreeLetterISOLanguageName": "eng" },
        { "DisplayName": "French", "ThreeLetterISOLanguageName": "fre" },
        { "DisplayName": "Duplicate English", "ThreeLetterISOLanguageName": "eng" },
        { "DisplayName": "No code", "ThreeLetterISOLanguageName": null },
    ])).unwrap()
}

/// A fake server: answers by path, records every request.
struct Server {
    me: Answer,
    cultures: Answer,
    post: Answer,
    calls: Vec<(Method, String, Option<Value>)>,
}

impl Server {
    fn new(config: Value) -> Self {
        Self { me: Some((200, me(config))), cultures: Some((200, cultures_body())), post: Some((204, Vec::new())), calls: Vec::new() }
    }
    fn send(&mut self, method: Method, path: &str, body: Option<&[u8]>) -> Answer {
        self.calls.push((method, path.to_string(), body.map(|b| serde_json::from_slice(b).unwrap())));
        match (method, path) {
            (Method::Get, "/Users/Me") => self.me.clone(),
            (Method::Get, "/Localization/Cultures") => self.cultures.clone(),
            (Method::Post, _) => self.post.clone(),
            _ => None,
        }
    }
}

fn load(server: &mut Server) -> Result<PreferenceSnapshot, PreferenceError> {
    request().load_with(&mut |m, p, b| server.send(m, p, b))
}

fn save(server: &mut Server, base: &PreferenceSnapshot, update: PreferenceUpdate) -> Result<PreferenceSnapshot, PreferenceError> {
    request().save_with(base, update, &mut |m, p, b| server.send(m, p, b))
}

/// Issue #41: a Jellyfin sign-in has no plex.tv credential, so the Audio & Subtitles page could
/// never capture a request and showed an error on every load and every Retry.
#[test]
fn a_jellyfin_sign_in_can_capture_a_preference_request() {
    let _serial = nj_base::testlock::serial();
    let origin = crate::catalog::Origin::parse("http://10.0.0.2:8096").unwrap();
    let mut stored = super::super::store::Stored::new(&origin, &Default::default());
    stored.token = "jf-token".into();
    stored.user_id = USER.into();
    super::super::store::set_live(Some(stored));
    let captured = PreferenceRequest::capture();
    let current = captured.as_ref().is_some_and(PreferenceRequest::is_current);
    super::super::store::set_live(None);
    assert!(captured.is_some(), "a signed-in Jellyfin user must be able to load the page");
    assert!(current);
    assert!(PreferenceRequest::capture().is_none(), "nobody signed in: nothing to load");
}

#[test]
fn a_load_reads_the_four_fields_and_the_servers_languages() {
    let mut server = Server::new(config());
    let snapshot = load(&mut server).unwrap();
    let p = &snapshot.preferences;
    assert_eq!(p.audio_language.as_deref(), Some("eng"));
    assert!(p.play_default_audio_track);
    assert_eq!(p.subtitle_language, None, "an empty language is unset");
    assert_eq!(p.subtitle_mode, SubtitleMode::Smart);
    assert_eq!(*p.cultures, vec![
        Culture { code: "eng".into(), name: "English".into() },
        Culture { code: "fre".into(), name: "French".into() },
    ], "named, coded and de-duplicated, in the server's order");
    assert!(server.calls.iter().all(|(m, ..)| *m == Method::Get), "a load writes nothing");
}

#[test]
fn a_load_refuses_an_answer_about_another_user() {
    let mut server = Server::new(config());
    server.me = Some((200, serde_json::to_vec(&json!({ "Id": "someone-else", "Configuration": config() })).unwrap()));
    assert_eq!(load(&mut server).unwrap_err(), PreferenceError::Stale);
}

#[test]
fn failures_are_told_apart() {
    let mut server = Server::new(config());
    server.me = Some((401, Vec::new()));
    assert_eq!(load(&mut server).unwrap_err(), PreferenceError::Refused);
    server.me = None;
    assert_eq!(load(&mut server).unwrap_err(), PreferenceError::Unavailable);
    server.me = Some((500, Vec::new()));
    assert_eq!(load(&mut server).unwrap_err(), PreferenceError::Unavailable);
    server.me = Some((200, b"not json".to_vec()));
    assert_eq!(load(&mut server).unwrap_err(), PreferenceError::InvalidResponse);
    server.me = Some((200, me(config())));
    server.cultures = Some((200, b"{}".to_vec()));
    assert_eq!(load(&mut server).unwrap_err(), PreferenceError::InvalidResponse);
}

/// `POST /Users/Configuration` replaces the whole configuration: everything the page does not
/// edit must go back exactly as the server gave it.
#[test]
fn a_save_changes_only_the_field_asked_for_and_keeps_the_rest() {
    let mut server = Server::new(config());
    let base = load(&mut server).unwrap();
    server.calls.clear();
    let saved = save(&mut server, &base, PreferenceUpdate { subtitle_mode: Some(SubtitleMode::OnlyForced), ..Default::default() }).unwrap();
    assert_eq!(saved.preferences.subtitle_mode, SubtitleMode::OnlyForced);
    let (method, path, body) = server.calls.last().unwrap().clone();
    assert_eq!((method, path.as_str()), (Method::Post, format!("/Users/Configuration?userId={USER}").as_str()));
    let mut expected = config();
    expected["SubtitleMode"] = json!("OnlyForced");
    expected["SubtitleLanguagePreference"] = Value::Null;
    assert_eq!(body.unwrap(), expected);
}

#[test]
fn a_save_writes_and_clears_languages() {
    let mut server = Server::new(config());
    let base = load(&mut server).unwrap();
    let saved = save(&mut server, &base, PreferenceUpdate {
        audio_language: Some(String::new()), subtitle_language: Some("fre".into()), ..Default::default()
    }).unwrap();
    assert_eq!(saved.preferences.audio_language, None);
    assert_eq!(saved.preferences.subtitle_language.as_deref(), Some("fre"));
    let body = server.calls.last().unwrap().2.clone().unwrap();
    assert_eq!(body["AudioLanguagePreference"], Value::Null);
    assert_eq!(body["SubtitleLanguagePreference"], json!("fre"));
}

#[test]
fn a_save_refuses_to_overwrite_a_change_made_elsewhere() {
    let mut server = Server::new(config());
    let base = load(&mut server).unwrap();
    let mut changed = config();
    changed["AudioLanguagePreference"] = json!("ger");
    server.me = Some((200, me(changed)));
    server.calls.clear();
    let err = save(&mut server, &base, PreferenceUpdate { play_default_audio_track: Some(false), ..Default::default() }).unwrap_err();
    assert_eq!(err, PreferenceError::Stale);
    assert!(server.calls.iter().all(|(m, ..)| *m == Method::Get), "nothing was written");
}

#[test]
fn a_save_that_changes_nothing_sends_nothing() {
    let mut server = Server::new(config());
    let base = load(&mut server).unwrap();
    server.calls.clear();
    save(&mut server, &base, PreferenceUpdate { subtitle_mode: Some(SubtitleMode::Smart), ..Default::default() }).unwrap();
    assert!(server.calls.iter().all(|(m, ..)| *m == Method::Get));
}

#[test]
fn a_refused_write_is_reported() {
    let mut server = Server::new(config());
    let base = load(&mut server).unwrap();
    server.post = Some((403, Vec::new()));
    let err = save(&mut server, &base, PreferenceUpdate { play_default_audio_track: Some(false), ..Default::default() }).unwrap_err();
    assert_eq!(err, PreferenceError::Refused);
}

#[test]
fn a_language_that_is_not_a_code_is_refused_before_any_request() {
    let mut server = Server::new(config());
    let base = load(&mut server).unwrap();
    server.calls.clear();
    let err = save(&mut server, &base, PreferenceUpdate { audio_language: Some("e n\"g".into()), ..Default::default() }).unwrap_err();
    assert_eq!(err, PreferenceError::InvalidPreference);
    assert!(server.calls.is_empty());
}

#[test]
fn a_snapshot_from_another_sign_in_cannot_be_saved() {
    let mut server = Server::new(config());
    let (_, other) = PreferenceRequest::fixture_for_test("http://10.0.0.2:8096", "another-user", AudioPreferences::default());
    let err = save(&mut server, &other, PreferenceUpdate::default()).unwrap_err();
    assert_eq!(err, PreferenceError::Stale);
    assert!(server.calls.is_empty());
}

#[test]
fn subtitle_modes_round_trip_and_unknown_reads_as_default() {
    for mode in SubtitleMode::ALL {
        assert_eq!(SubtitleMode::parse(mode.as_str()), mode);
    }
    assert_eq!(SubtitleMode::parse("smart"), SubtitleMode::Smart);
    assert_eq!(SubtitleMode::parse("Sometimes"), SubtitleMode::Default);
}

/// `RememberAudioSelections` / `RememberSubtitleSelections`: whether the server keeps each film's
/// last track pick and serves it back as the film's default. A server that omits one keeps its own
/// default, on.
#[test]
fn a_load_reads_whether_the_server_remembers_track_choices() {
    let mut remembers = config();
    remembers["RememberSubtitleSelections"] = json!(false);
    let p = load(&mut Server::new(remembers)).unwrap().preferences;
    assert!(p.remember_audio_selections);
    assert!(!p.remember_subtitle_selections);
    let mut silent = config();
    silent.as_object_mut().unwrap().remove("RememberAudioSelections");
    silent.as_object_mut().unwrap().remove("RememberSubtitleSelections");
    let p = load(&mut Server::new(silent)).unwrap().preferences;
    assert!(p.remember_audio_selections && p.remember_subtitle_selections, "absent reads as on");
}

#[test]
fn a_save_turns_remembering_off_and_keeps_the_rest() {
    let audio = PreferenceUpdate { remember_audio_selections: Some(false), ..Default::default() };
    let subtitles = PreferenceUpdate { remember_subtitle_selections: Some(false), ..Default::default() };
    for (update, key) in [(audio, "RememberAudioSelections"), (subtitles, "RememberSubtitleSelections")] {
        let mut server = Server::new(config());
        let base = load(&mut server).unwrap();
        save(&mut server, &base, update).unwrap();
        let mut expected = config();
        expected[key] = json!(false);
        expected["SubtitleLanguagePreference"] = Value::Null;
        assert_eq!(server.calls.last().unwrap().2.clone().unwrap(), expected, "{key}");
    }
}
