//! The set's power state, watched over `luna://com.webos.service.tvpower/power/getPowerState` for
//! [`crate::devcaps::power`].
//!
//! One subscription (`{"subscribe":true}`) for the life of the process; every reply's `state` is
//! published, and `devcaps::power` keeps the moment of the last switch-off. The states seen on
//! these sets: "Active", "Active Standby", "Suspend", "Screen Off", "Screen Saver". Whether the
//! hub lets the jailed app subscribe is unmeasured, so a refusal is final and quiet: one log line,
//! the worker stops, and the player keeps its ordinary rule.
//!
//! One worker owns the registration (no LS2 handle crosses a thread boundary), as for
//! [`super::volume`].

#[cfg(all(not(feature = "hostsim"), not(any(test, feature = "test-support"))))]
const GET_POWER_STATE: &str = "luna://com.webos.service.tvpower/power/getPowerState";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(any(feature = "hostsim", feature = "test-support"), allow(dead_code))]
enum Failure {
    Json,
    Refused,
    State,
    /// Answered once but declined the subscription: no switch-off will ever be reported.
    NotSubscribed,
}

/// The `state` of one reply. A reply with no `state` (one that only reports `processing`) is
/// `State`, which the worker skips. `returnValue:false` is a refusal, and `subscribed:false` (the
/// service answered once and will send no switch-off) ends the watch the same way.
#[cfg_attr(any(feature = "hostsim", feature = "test-support"), allow(dead_code))]
fn parse(reply: &str) -> Result<String, Failure> {
    let value: serde_json::Value = serde_json::from_str(reply).map_err(|_| Failure::Json)?;
    let root = value.as_object().ok_or(Failure::Json)?;
    if root.get("returnValue").and_then(serde_json::Value::as_bool) == Some(false) {
        return Err(Failure::Refused);
    }
    if root.get("subscribed").and_then(serde_json::Value::as_bool) == Some(false) {
        return Err(Failure::NotSubscribed);
    }
    root.get("state")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or(Failure::State)
}

#[cfg(any(feature = "hostsim", test, feature = "test-support"))]
pub fn watch() {}

#[cfg(all(not(feature = "hostsim"), not(any(test, feature = "test-support"))))]
pub fn watch() {
    use std::sync::Once;
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        if nj_base::task::spawn("webos power", serve).is_none() {
            nj_base::eventlog::log("power: no worker for the power-state watch");
            crate::devcaps::power::refused();
        }
    });
}

#[cfg(all(not(feature = "hostsim"), not(any(test, feature = "test-support"))))]
fn serve() {
    use super::ls2::{self, Fail};
    let registration = match ls2::register() {
        Ok(registration) => registration,
        Err(e) => {
            nj_base::eventlog::log(&format!("power: {GET_POWER_STATE} not asked: {e}"));
            crate::devcaps::power::refused();
            return;
        }
    };
    let mut last = String::new();
    let outcome = registration.subscribe(GET_POWER_STATE, r#"{"subscribe":true}"#, |reply| match parse(reply) {
        Ok(state) => {
            if state != last {
                nj_base::eventlog::log(&format!("power: state {state}"));
                last = state.clone();
            }
            crate::devcaps::power::publish(&state);
            true
        }
        Err(Failure::Refused) => {
            nj_base::eventlog::log(&format!("power: {GET_POWER_STATE} refused: {reply}"));
            crate::devcaps::power::refused();
            false
        }
        Err(Failure::NotSubscribed) => {
            nj_base::eventlog::log(&format!("power: {GET_POWER_STATE} answered once, not subscribed: {reply}"));
            crate::devcaps::power::refused();
            false
        }
        Err(Failure::Json | Failure::State) => true,
    });
    if let Err(Fail::Setup { stage, detail, .. }) = outcome {
        nj_base::eventlog::log(&format!("power: {GET_POWER_STATE} not subscribed: stage={stage} {detail}"));
        crate::devcaps::power::refused();
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, Failure};

    #[test]
    fn each_reply_reads_as_its_state() {
        assert_eq!(parse(r#"{"returnValue":true,"state":"Active","subscribed":true}"#), Ok("Active".into()));
        assert_eq!(parse(r#"{"state":"Active Standby","processing":"Request Active Standby"}"#), Ok("Active Standby".into()));
        assert_eq!(parse(r#"{"returnValue":true,"state":"Screen Saver"}"#), Ok("Screen Saver".into()));
    }

    #[test]
    fn a_refusal_ends_the_watch_and_anything_else_is_skipped() {
        for (reply, failure) in [
            (r#"{"returnValue":false,"errorText":"Denied method call"}"#, Failure::Refused),
            ("not json", Failure::Json),
            ("[]", Failure::Json),
            (r#"{"returnValue":true,"processing":"Request Suspend"}"#, Failure::State),
            (r#"{"returnValue":true,"state":""}"#, Failure::State),
            (r#"{"returnValue":true,"state":"Active","subscribed":false}"#, Failure::NotSubscribed),
        ] {
            assert_eq!(parse(reply), Err(failure), "{reply}");
        }
    }
}
