//! **The signed-in Jellyfin user's audio and subtitle preferences**, as Settings → Audio &
//! Subtitles reads and edits them (issue #41).
//!
//! They are the six playback fields of the server's `UserConfiguration`: `AudioLanguagePreference`,
//! `PlayDefaultAudioTrack`, `SubtitleLanguagePreference`, `SubtitleMode`, and
//! `RememberAudioSelections`/`RememberSubtitleSelections` (the server keeps each film's last pick
//! from the progress reports and serves it back as that film's default track). Playback already
//! ranks tracks against the same fields (`Jf::language_prefs` and the item's default streams, read
//! per play), so a save takes effect from the next playback without anything cached here. They live
//! on the server, per user, so every Jellyfin client of that user shares them.
//!
//! Languages are the server's own list (`GET /Localization/Cultures`), keyed by
//! `ThreeLetterISOLanguageName` (`eng`, `fre`), which is what the server stores and what it reports
//! on a stream; this is the list and the key jellyfin-web's settings page uses.
//!
//! **A save is read-modify-write.** `POST /Users/Configuration` REPLACES the whole configuration,
//! so a save reads the current one, changes only the field asked for and sends the rest back as the
//! server gave it. If the six fields no longer match what the page showed (another client changed
//! them), the save is refused as [`PreferenceError::Stale`] and the page reloads, rather than
//! overwriting a change the person never saw.
//!
//! Blocking; runs on a worker (`app::preferences`). Every request is bound to the sign-in that was
//! active when it was captured, and the server's `/Users/Me` must name that same user before
//! anything is read or written.
use crate::http::Method;
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// One settings read or write at a time, so a save never interleaves with a reload.
static PREFERENCE_IO: Mutex<()> = Mutex::new(());

/// `UserConfiguration.SubtitleMode` (`SubtitlePlaybackMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SubtitleMode {
    /// Follow the file's default and forced flags.
    #[default]
    Default,
    /// Subtitles in the preferred language when the audio is in another language.
    Smart,
    /// Subtitles in the preferred language whatever the audio.
    Always,
    /// Only tracks flagged forced.
    OnlyForced,
    /// No subtitles unless chosen during playback.
    None,
}

impl SubtitleMode {
    /// Picker order.
    pub(crate) const ALL: [Self; 5] = [Self::Default, Self::Smart, Self::Always, Self::OnlyForced, Self::None];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Smart => "Smart",
            Self::Always => "Always",
            Self::OnlyForced => "OnlyForced",
            Self::None => "None",
        }
    }

    /// The server's value; anything unknown reads as the server's own default.
    pub(crate) fn parse(s: &str) -> Self {
        Self::ALL.into_iter().find(|m| m.as_str().eq_ignore_ascii_case(s)).unwrap_or_default()
    }
}

/// A language the server offers: its stored code and the name it gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Culture {
    pub code: String,
    pub name: String,
}

/// The six fields this page edits. A language is `None` when unset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AudioPreferences {
    pub audio_language: Option<String>,
    pub play_default_audio_track: bool,
    pub subtitle_language: Option<String>,
    pub subtitle_mode: SubtitleMode,
    /// `RememberAudioSelections`: the server keeps each film's last audio track (from the progress
    /// reports) and serves it back as the film's `DefaultAudioStreamIndex`.
    pub remember_audio_selections: bool,
    /// `RememberSubtitleSelections`: the same for subtitles, Off included.
    pub remember_subtitle_selections: bool,
    /// The server's languages, in its order. Shared by every snapshot of one load.
    pub cultures: Arc<Vec<Culture>>,
}

impl AudioPreferences {
    /// The six edited fields of `config` (a `UserConfiguration` object); `cultures` is carried.
    fn read(config: &Value, cultures: Arc<Vec<Culture>>) -> Self {
        let lang = |key: &str| config.get(key).and_then(Value::as_str).map(str::trim)
            .filter(|s| !s.is_empty()).map(str::to_string);
        Self {
            audio_language: lang("AudioLanguagePreference"),
            play_default_audio_track: config.get("PlayDefaultAudioTrack").and_then(Value::as_bool).unwrap_or(true),
            subtitle_language: lang("SubtitleLanguagePreference"),
            subtitle_mode: SubtitleMode::parse(config.get("SubtitleMode").and_then(Value::as_str).unwrap_or("")),
            // The server's own default for both is on.
            remember_audio_selections: config.get("RememberAudioSelections").and_then(Value::as_bool).unwrap_or(true),
            remember_subtitle_selections: config.get("RememberSubtitleSelections").and_then(Value::as_bool).unwrap_or(true),
            cultures,
        }
    }

    /// Whether the edited fields equal `other`'s (the language list is not a preference).
    fn same_fields(&self, other: &Self) -> bool {
        self.audio_language == other.audio_language
            && self.play_default_audio_track == other.play_default_audio_track
            && self.subtitle_language == other.subtitle_language
            && self.subtitle_mode == other.subtitle_mode
            && self.remember_audio_selections == other.remember_audio_selections
            && self.remember_subtitle_selections == other.remember_subtitle_selections
    }

    /// Write the edited fields into `config`, leaving every other field as it is.
    fn write(&self, config: &mut serde_json::Map<String, Value>) {
        let lang = |v: &Option<String>| v.clone().map_or(Value::Null, Value::String);
        config.insert("AudioLanguagePreference".into(), lang(&self.audio_language));
        config.insert("PlayDefaultAudioTrack".into(), Value::Bool(self.play_default_audio_track));
        config.insert("SubtitleLanguagePreference".into(), lang(&self.subtitle_language));
        config.insert("SubtitleMode".into(), Value::String(self.subtitle_mode.as_str().into()));
        config.insert("RememberAudioSelections".into(), Value::Bool(self.remember_audio_selections));
        config.insert("RememberSubtitleSelections".into(), Value::Bool(self.remember_subtitle_selections));
    }
}

/// The sign-in a load or save belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreferenceRequest {
    server: String,
    user_id: String,
}

/// What a load or save confirmed, for the sign-in it was made under.
#[derive(Clone, Debug)]
pub(crate) struct PreferenceSnapshot {
    pub preferences: AudioPreferences,
    owner: PreferenceRequest,
}

/// One field to change; `None` leaves it. `Some("")` clears a language.
#[derive(Clone, Debug, Default)]
pub(crate) struct PreferenceUpdate {
    pub audio_language: Option<String>,
    pub play_default_audio_track: Option<bool>,
    pub subtitle_language: Option<String>,
    pub subtitle_mode: Option<SubtitleMode>,
    pub remember_audio_selections: Option<bool>,
    pub remember_subtitle_selections: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreferenceError {
    /// The sign-in changed, or the server's values changed since the page read them.
    Stale,
    Refused,
    TimedOut,
    Unavailable,
    InvalidResponse,
    InvalidPreference,
}

impl PreferenceError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Stale => nj_platform::i18n::msg::browse_preferences_stale(),
            Self::Refused => nj_platform::i18n::msg::browse_preferences_refused(),
            Self::TimedOut => nj_platform::i18n::msg::browse_preferences_timed_out(),
            Self::Unavailable => nj_platform::i18n::msg::browse_preferences_unavailable(),
            Self::InvalidResponse => nj_platform::i18n::msg::browse_preferences_invalid_response(),
            Self::InvalidPreference => nj_platform::i18n::msg::browse_preferences_invalid_preference(),
        }
    }
}

/// A status and body from the server, or `None` when nothing answered.
type Answer = Option<(i32, Vec<u8>)>;

impl PreferenceRequest {
    /// The active Jellyfin sign-in, or `None` when nobody is signed in.
    pub(crate) fn capture() -> Option<Self> {
        let live = super::store::current()?;
        if live.user_id.is_empty() || live.server.is_empty() { return None; }
        Some(Self { server: live.server, user_id: super::ids::normalize(&live.user_id) })
    }

    /// Still the active sign-in.
    pub(crate) fn is_current(&self) -> bool {
        Self::capture().as_ref() == Some(self)
    }

    /// A synthetic receipt for composed UI tests; nothing is read or fetched.
    #[cfg(test)]
    pub(crate) fn fixture_for_test(server: &str, user_id: &str, preferences: AudioPreferences)
        -> (Self, PreferenceSnapshot)
    {
        let request = Self { server: server.into(), user_id: super::ids::normalize(user_id) };
        (request.clone(), PreferenceSnapshot { preferences, owner: request })
    }

    pub(crate) fn load(&self) -> Result<PreferenceSnapshot, PreferenceError> {
        let _io = PREFERENCE_IO.lock().unwrap_or_else(|e| e.into_inner());
        self.with_server(|send| self.load_with(send))
    }

    pub(crate) fn save(&self, base: &PreferenceSnapshot, update: PreferenceUpdate)
        -> Result<PreferenceSnapshot, PreferenceError>
    {
        let _io = PREFERENCE_IO.lock().unwrap_or_else(|e| e.into_inner());
        self.with_server(|send| self.save_with(base, update, send))
    }

    /// Run `f` against the live server client, if it is still this request's sign-in.
    fn with_server<T>(&self, f: impl FnOnce(&mut dyn FnMut(Method, &str, Option<&[u8]>) -> Answer)
        -> Result<T, PreferenceError>) -> Result<T, PreferenceError>
    {
        if !self.is_current() { return Err(PreferenceError::Stale); }
        let client = crate::catalog::client_opt().ok_or(PreferenceError::Unavailable)?;
        let jf = client.jf().ok_or(PreferenceError::Unavailable)?;
        if jf.origin().base() != self.server { return Err(PreferenceError::Stale); }
        let mut send = |method: Method, path: &str, body: Option<&[u8]>| {
            jf.reply(path, method, body).map(|r| (r.status, r.body))
        };
        f(&mut send)
    }

    fn load_with(&self, send: &mut dyn FnMut(Method, &str, Option<&[u8]>) -> Answer)
        -> Result<PreferenceSnapshot, PreferenceError>
    {
        let config = self.configuration(send)?;
        let cultures = Arc::new(cultures(send)?);
        Ok(self.snapshot(AudioPreferences::read(&Value::Object(config), cultures)))
    }

    fn save_with(&self, base: &PreferenceSnapshot, update: PreferenceUpdate,
        send: &mut dyn FnMut(Method, &str, Option<&[u8]>) -> Answer)
        -> Result<PreferenceSnapshot, PreferenceError>
    {
        if base.owner != *self { return Err(PreferenceError::Stale); }
        let next = update.apply(&base.preferences)?;
        let mut config = self.configuration(send)?;
        let now = AudioPreferences::read(&Value::Object(config.clone()), base.preferences.cultures.clone());
        if !now.same_fields(&base.preferences) { return Err(PreferenceError::Stale); }
        if next.same_fields(&now) { return Ok(self.snapshot(next)); }
        next.write(&mut config);
        let body = serde_json::to_vec(&Value::Object(config)).map_err(|_| PreferenceError::InvalidPreference)?;
        let path = format!("/Users/Configuration?userId={}", self.user_id);
        accepted(send(Method::Post, &path, Some(&body)))?;
        Ok(self.snapshot(next))
    }

    /// `/Users/Me`'s `Configuration`, after proving the server's answer is this request's user.
    fn configuration(&self, send: &mut dyn FnMut(Method, &str, Option<&[u8]>) -> Answer)
        -> Result<serde_json::Map<String, Value>, PreferenceError>
    {
        let body = accepted(send(Method::Get, "/Users/Me", None))?;
        let me: Value = serde_json::from_slice(&body).map_err(|_| PreferenceError::InvalidResponse)?;
        let id = me.get("Id").and_then(Value::as_str).map(super::ids::normalize);
        if id.as_deref() != Some(self.user_id.as_str()) { return Err(PreferenceError::Stale); }
        match me.get("Configuration") {
            Some(Value::Object(config)) => Ok(config.clone()),
            _ => Err(PreferenceError::InvalidResponse),
        }
    }

    fn snapshot(&self, preferences: AudioPreferences) -> PreferenceSnapshot {
        PreferenceSnapshot { preferences, owner: self.clone() }
    }
}

/// The server's languages, named and de-duplicated by code, in the server's order.
fn cultures(send: &mut dyn FnMut(Method, &str, Option<&[u8]>) -> Answer)
    -> Result<Vec<Culture>, PreferenceError>
{
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct CultureDto {
        #[serde(default)]
        display_name: String,
        #[serde(default, rename = "ThreeLetterISOLanguageName")]
        three_letter: Option<String>,
    }
    let body = accepted(send(Method::Get, "/Localization/Cultures", None))?;
    let list: Vec<CultureDto> = serde_json::from_slice(&body).map_err(|_| PreferenceError::InvalidResponse)?;
    let mut out: Vec<Culture> = Vec::with_capacity(list.len());
    for c in list {
        let Some(code) = c.three_letter.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) else { continue };
        let name = c.display_name.trim();
        if name.is_empty() || out.iter().any(|o| o.code == code) { continue; }
        out.push(Culture { code, name: name.to_string() });
    }
    Ok(out)
}

fn accepted(answer: Answer) -> Result<Vec<u8>, PreferenceError> {
    match answer {
        None => Err(PreferenceError::Unavailable),
        Some((200..=299, body)) => Ok(body),
        Some((401 | 403, _)) => Err(PreferenceError::Refused),
        Some(_) => Err(PreferenceError::Unavailable),
    }
}

impl PreferenceUpdate {
    fn apply(self, previous: &AudioPreferences) -> Result<AudioPreferences, PreferenceError> {
        let mut next = previous.clone();
        let lang = |v: String| -> Result<Option<String>, PreferenceError> {
            let v = v.trim().to_string();
            if v.is_empty() { return Ok(None); }
            // The server's codes are short ASCII letters; anything else is not one it offered.
            if v.len() > 8 || !v.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-') {
                return Err(PreferenceError::InvalidPreference);
            }
            Ok(Some(v))
        };
        if let Some(v) = self.audio_language { next.audio_language = lang(v)?; }
        if let Some(v) = self.subtitle_language { next.subtitle_language = lang(v)?; }
        if let Some(v) = self.play_default_audio_track { next.play_default_audio_track = v; }
        if let Some(v) = self.subtitle_mode { next.subtitle_mode = v; }
        if let Some(v) = self.remember_audio_selections { next.remember_audio_selections = v; }
        if let Some(v) = self.remember_subtitle_selections { next.remember_subtitle_selections = v; }
        Ok(next)
    }
}

/// A live Jellyfin sign-in for a test, restoring the previous one on drop. The caller holds
/// `testlock::serial()`.
#[cfg(test)]
pub(crate) struct LiveSignInForTest(Option<super::store::Stored>);

#[cfg(test)]
impl LiveSignInForTest {
    pub(crate) fn new(server: &str, user_id: &str) -> Self {
        let previous = super::store::current();
        let origin = crate::catalog::Origin::parse(server).expect("a test server URL");
        let mut stored = super::store::Stored::new(&origin, &Default::default());
        stored.token = "synthetic-token".into();
        stored.user_id = user_id.into();
        super::store::set_live(Some(stored));
        Self(previous)
    }
}

#[cfg(test)]
impl Drop for LiveSignInForTest {
    fn drop(&mut self) {
        super::store::set_live(self.0.take());
    }
}

#[cfg(test)]
#[path = "preferences_tests.rs"]
mod tests;
