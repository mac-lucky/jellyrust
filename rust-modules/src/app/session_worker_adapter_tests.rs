//! Session workers graded through the live Session adapter: the profile-switch worker's
//! observations as the adapter's landing delivers them. The worker functions are `auth`'s; the adapter, the
//! `Bridge` and the dispatcher that drive them sit above that layer, so the grading lives here
//! (these four were `auth_session_worker_tests.rs`'s).

use crate::app::adapters::session::SessionAdapter;
use crate::auth::owner::{SessionArrival, SessionOp, SessionWorkKey};
use crate::auth::test_support::cached_session;
use crate::auth::{
    discovery_insecure_only_message, observation, profile_switch_worker_with_io,
    profile_switch_worker_with_output, settled_probe_for_test, ProfileSwitchOutcomeProgress,
    ProfileSwitchProgress, ProfileWorkIo, SessionIdentity, SettledProbe, UserTile,
};
use crate::catalog::account::{AccountClient, CallEvidence, Resource, SwitchOutcome, SwitchedUser};
use crate::catalog::probe::{self, Outcome};
use crate::catalog::session::SourceRef;
use nj_machine::machine::RequestId;

#[test]
fn instance_profile_worker_completes_offline_policy_on_its_own_landing() {
    // No serial lock: both input credentials and both output transports are instance-local.
    let mut a = SessionAdapter::fixture();
    let mut b = SessionAdapter::fixture();
    for (adapter, epoch, uuid) in [(&mut a, 0x1_0000_0001, "u-kid"),
        (&mut b, 7, "not-cached")] {
        let stored = cached_session(None);
        let expected = SessionIdentity::of(&stored);
        let tile = UserTile { uuid: uuid.into(), title: "Synthetic profile".into(),
            ..Default::default() };
        adapter.launch(RequestId(1), SessionWorkKey { epoch, op: SessionOp::ProfileSwitch },
            true, |job| { job(); true }, move |output| {
                profile_switch_worker_with_output(epoch, expected, stored, tile, None,
                    false, &crate::catalog::grant::PlaintextAsk::undecided(), &output,
                    |_, _, _| SwitchOutcome::Unreachable);
            }).unwrap();
    }
    let a = a.take_results();
    let b = b.take_results();
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert!(a[0].terminal && b[0].terminal);
    let SessionArrival::Data(a) = &a[0].outcome else { panic!("missing offline result") };
    let SessionArrival::Data(b) = &b[0].outcome else { panic!("missing failure result") };
    assert!(matches!(&**a, observation::Observation::ProfileSwitch(ProfileSwitchProgress {
        epoch: 0x1_0000_0001, outcome: ProfileSwitchOutcomeProgress::Ready { delta, .. }, ..
    }) if delta.user.uuid == "u-kid"));
    assert!(matches!(&**b, observation::Observation::ProfileSwitch(ProfileSwitchProgress {
        epoch: 7, outcome: ProfileSwitchOutcomeProgress::Failed { pin_denied: false, .. }, ..
    })));
}

/// S7 (plan §4): a grant whose only settled probe is `InsecureOnly` must fail with the SHARED
/// discovery copy, not "has no access to this server" — the grant is real, only the transport
/// is unusable in this build, and the ordinary wording sends the user to ask their friend for
/// access they already have.
#[test]
fn a_grant_verified_only_over_plaintext_reports_the_shared_insecure_only_copy() {
    struct InsecureOnlyGrantIo;
    impl ProfileWorkIo for InsecureOnlyGrantIo {
        fn switch(&mut self, _: &AccountClient, _: &str, _: Option<&str>) -> SwitchOutcome {
            SwitchOutcome::Switched(SwitchedUser {
                id: 1, uuid: "u-kid".into(), title: "Kid".into(), auth_token: "kid-token".into(),
            })
        }
        fn resources(&mut self, _: &AccountClient) -> Result<Vec<Resource>, crate::catalog::account::CallEvidence> {
            Ok(vec![Resource {
                name: "srv".into(), client_identifier: "srv".into(), provides: "server".into(),
                owned: true, access_token: "srv-token".into(), ..Default::default()
            }])
        }
        fn probe(&mut self, _: &Resource, _: &[i64]) -> (Option<SourceRef>, SettledProbe) {
            // No winning source (nothing this build may put a credential on), but the probe
            // itself verified the server — over plaintext only.
            (None, settled_probe_for_test("srv", Outcome::InsecureOnly,
                Some(probe::Location::Local), Some("10.0.0.5".into())))
        }
        fn gap(&mut self) {}
    }

    let mut a = SessionAdapter::fixture();
    let stored = cached_session(None);
    let expected = SessionIdentity::of(&stored);
    let tile = UserTile { uuid: "u-kid".into(), title: "Kid".into(), ..Default::default() };
    a.launch(RequestId(1), SessionWorkKey { epoch: 1, op: SessionOp::ProfileSwitch }, true,
        |job| { job(); true }, move |output| {
            profile_switch_worker_with_io(1, expected, stored, tile, None, false,
                &output, &mut InsecureOnlyGrantIo);
        }).unwrap();
    let results = a.take_results();
    assert_eq!(results.len(), 1);
    assert!(results[0].terminal);
    let SessionArrival::Data(data) = &results[0].outcome else { panic!("missing failure result") };
    assert!(
        matches!(&**data, observation::Observation::ProfileSwitch(ProfileSwitchProgress {
            outcome: ProfileSwitchOutcomeProgress::Failed { error, pin_denied: false }, ..
        }) if error == discovery_insecure_only_message()),
        "an InsecureOnly-only grant must report the shared discovery copy, not the generic \
         'has no access' wording"
    );
}

/// #132: a switched profile whose token plex.tv then REFUSES is not a connection fault. The
/// refusal says so; a request that got no answer keeps the connection wording.
#[test]
fn a_refused_profile_resources_request_does_not_blame_the_connection() {
    use nj_net::net::{RequestError, RequestFailure};

    struct FailingResourcesIo(Option<CallEvidence>);
    impl ProfileWorkIo for FailingResourcesIo {
        fn switch(&mut self, _: &AccountClient, _: &str, _: Option<&str>) -> SwitchOutcome {
            SwitchOutcome::Switched(SwitchedUser {
                id: 1, uuid: "u-kid".into(), title: "Kid".into(), auth_token: "kid-token".into(),
            })
        }
        fn resources(&mut self, _: &AccountClient) -> Result<Vec<Resource>, CallEvidence> {
            Err(self.0.take().expect("one resources request"))
        }
        fn probe(&mut self, _: &Resource, _: &[i64]) -> (Option<SourceRef>, SettledProbe) {
            unreachable!("no resources, nothing to probe")
        }
        fn gap(&mut self) {}
    }

    let run = |evidence: CallEvidence| -> String {
        let mut a = SessionAdapter::fixture();
        let stored = cached_session(None);
        let expected = SessionIdentity::of(&stored);
        let tile = UserTile { uuid: "u-kid".into(), title: "Kid".into(), ..Default::default() };
        let mut io = FailingResourcesIo(Some(evidence));
        a.launch(RequestId(1), SessionWorkKey { epoch: 1, op: SessionOp::ProfileSwitch }, true,
            |job| { job(); true }, move |output| {
                profile_switch_worker_with_io(1, expected, stored, tile, None, false,
                    &output, &mut io);
            }).unwrap();
        let results = a.take_results();
        let SessionArrival::Data(data) = &results[0].outcome else { panic!("missing failure result") };
        match &**data {
            observation::Observation::ProfileSwitch(ProfileSwitchProgress {
                outcome: ProfileSwitchOutcomeProgress::Failed { error, pin_denied: false }, ..
            }) => error.clone(),
            _ => panic!("expected a banner failure"),
        }
    };

    let refused = run(Ok(401));
    assert!(!refused.contains("connection"), "a 401 is an answer: {refused}");
    assert!(refused.contains("Kid"), "the refusal names the profile: {refused}");
    let silent = run(Err(RequestFailure { cause: RequestError::TimedOut, status: None,
        body_limit: None, curl_rc: Some(28) }));
    assert!(silent.contains("check the connection"), "no answer keeps the connection copy: {silent}");
}
