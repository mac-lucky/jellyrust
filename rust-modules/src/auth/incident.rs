//! **The closed vocabulary of an onboarding failure**: a failed sign-in, discovery, save, locked
//! read, profile switch or first content load, as typed evidence. The sign-in read-out picks its
//! caption and its mark ([`IncidentContext::readout_glyph`]) from it. Nothing here leaves the
//! television: every value is closed or bucketed, and there is no free-text slot.

use nj_net::net::RequestFailure;
use crate::catalog::session::async_persistence::{CompletionOutcome, Failure};
use nj_platform::storage::wire::KeymanagerStage;

/// Why server discovery ended with nothing to connect to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum DiscoveryClass {
    /// The account has no server at all.
    NoServers,
    /// A server answered and refused the credentials.
    Refused,
    /// No server answered.
    Silent,
    /// Only insecure connections were offered, and none was allowed.
    InsecureOnly,
}

/// Which first content load failed during onboarding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ContentSource {
    Home,
    Libraries,
}

/// A sign-in that failed inside the app itself rather than on any network: its own machinery
/// refused or lost the work. Closed, like every other class here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum InternalClass {
    /// The sign-in's work was not admitted by the adapter.
    AdmissionRefused,
    /// The sign-in worker could not be started (the thread spawn was refused).
    WorkerRefused,
    /// The sign-in worker went away without an answer.
    WorkerDropped,
    /// The finished sign-in could not be committed.
    CommitRefused,
    /// The installation's client identifier could not be read to start the sign-in.
    ClientIdUnavailable,
    /// An internal counter ran out.
    Exhausted,
}

/// Where onboarding failed. Closed, and each variant's [`Self::code`] is a stable value -
/// the on-screen read-out and the event log show it, so a rename changes what a report quotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum IncidentKind {
    /// The sign-in code could not be created.
    PinCreate,
    /// plex.tv stopped answering while the code was on screen. Offered only behind *Details* —
    /// never by the alert, which would cover a code that may be mid-scan — and never sent
    /// automatically: a slow network is not a failure until the person decides it is one.
    LinkStalled,
    /// Every code the flow issued expired unused.
    PinExpired,
    /// plex.tv refused the account token the sign-in had just been given (a 401 or 403 from
    /// `/resources` during discovery).
    Authorization,
    Discovery(DiscoveryClass),
    /// The sign-in could not be saved.
    SaveFailed,
    /// The saved sign-in exists but could not be opened.
    StoredLocked,
    /// Switching to a profile failed. Never raised for a wrong PIN.
    ProfileSwitch,
    ContentLoad(ContentSource),
    /// The sign-in failed inside the app, not on the network — see [`InternalClass`].
    Internal(InternalClass),
}

impl IncidentKind {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::PinCreate => "pin_create",
            Self::LinkStalled => "link_stalled",
            Self::PinExpired => "pin_expired",
            Self::Authorization => "authorization",
            Self::Discovery(DiscoveryClass::NoServers) => "discovery_no_servers",
            Self::Discovery(DiscoveryClass::Refused) => "discovery_refused",
            Self::Discovery(DiscoveryClass::Silent) => "discovery_silent",
            Self::Discovery(DiscoveryClass::InsecureOnly) => "discovery_insecure_only",
            Self::SaveFailed => "save_failed",
            Self::StoredLocked => "stored_locked",
            Self::ProfileSwitch => "profile_switch",
            Self::ContentLoad(ContentSource::Home) => "content_load_home",
            Self::ContentLoad(ContentSource::Libraries) => "content_load_libraries",
            Self::Internal(InternalClass::AdmissionRefused) => "internal_admission_refused",
            Self::Internal(InternalClass::WorkerRefused) => "internal_worker_refused",
            Self::Internal(InternalClass::WorkerDropped) => "internal_worker_dropped",
            Self::Internal(InternalClass::CommitRefused) => "internal_commit_refused",
            Self::Internal(InternalClass::ClientIdUnavailable) => "internal_client_id_unavailable",
            Self::Internal(InternalClass::Exhausted) => "internal_exhausted",
        }
    }

    /// Does a failure of this kind mean plex.tv refused the account token the sign-in holds? Such
    /// a token cannot be retried: *Try again* after one must start a new QR sign-in, not a
    /// discovery pass that presents the refused token again (`auth::retry_kind`).
    pub(crate) fn refuses_the_account_token(self) -> bool {
        matches!(self, Self::Authorization)
    }
}

// What the most recent network call observed, coarsened into the class this report carries
// (`LinkClass`), and the pure `classify` that coarsens it, are defined in `plex::probe`: the probe
// grades its own transport failures in the same vocabulary and `plex` sits beneath this layer.
// Re-exported, so every incident producer and reader keeps naming them here.
pub(crate) use crate::catalog::probe::{classify, LinkClass};

/// How many consecutive calls came back with no usable answer, bucketed — never the raw count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum UnansweredBucket {
    Zero,
    One,
    TwoToFive,
    SixPlus,
}

impl UnansweredBucket {
    pub(crate) fn from_count(n: u32) -> Self {
        match n {
            0 => Self::Zero,
            1 => Self::One,
            2..=5 => Self::TwoToFive,
            _ => Self::SixPlus,
        }
    }
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Zero => "zero",
            Self::One => "one",
            Self::TwoToFive => "two_to_five",
            Self::SixPlus => "six_plus",
        }
    }
}

/// A count of things plex.tv returned, bucketed — never the raw count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum CountBucket {
    Zero,
    One,
    TwoToFive,
    SixPlus,
}

impl CountBucket {
    pub(crate) fn from_count(n: usize) -> Self {
        match n {
            0 => Self::Zero,
            1 => Self::One,
            2..=5 => Self::TwoToFive,
            _ => Self::SixPlus,
        }
    }
}

/// Which flow ran the discovery that failed: the one right after a fresh code was authorized, or
/// the discovery-only retry (*Try again*) that reuses that authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum DiscoveryTrigger {
    Login,
    Rediscover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum DiscoveryTarget { PlexTv, Servers }
impl DiscoveryTarget {
    pub(crate) fn code(self) -> &'static str {
        match self { Self::PlexTv => "plex_tv", Self::Servers => "servers" }
    }
}

/// Evidence shared by every discovery verdict. `target` is absent for NoServers so moving its
/// trigger into this shared record leaves that established JSON byte-for-byte unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct DiscoveryEvidence {
    pub trigger: DiscoveryTrigger,
    pub target: Option<DiscoveryTarget>,
}

impl DiscoveryTrigger {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Rediscover => "rediscover",
        }
    }
}

/// Why `/resources` named no server: how much it did return, and which flow asked. Every
/// resource counted is a non-server one by definition of the verdict, so one bucket says both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NoServersEvidence {
    pub resources: CountBucket,
    pub trigger: DiscoveryTrigger,
}

/// How long the current run of misses has lasted, bucketed — never the raw duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum FailingForBucket {
    None,
    Under10s,
    Under60s,
    Under5m,
    FiveMinPlus,
}

impl FailingForBucket {
    pub(crate) fn from_duration(d: Option<std::time::Duration>) -> Self {
        let Some(d) = d else { return Self::None };
        match d.as_secs() {
            0..=9 => Self::Under10s,
            10..=59 => Self::Under60s,
            60..=299 => Self::Under5m,
            _ => Self::FiveMinPlus,
        }
    }
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Under10s => "under_10s",
            Self::Under60s => "under_60s",
            Self::Under5m => "under_5m",
            Self::FiveMinPlus => "five_min_plus",
        }
    }
}

/// Why saving (or re-reading) the sign-in failed, from the session persistence completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum PersistenceFailure {
    /// The storage worker refused the job.
    Admission,
    /// The write itself failed.
    WriteFailed,
    /// The store refused or failed the record.
    Storage,
    /// The commit may or may not have reached the disk.
    CommitUncertain,
    /// Sealing the credentials with the platform key service failed.
    Protection,
    /// Sealing may or may not have happened.
    ProtectionUncertain,
    /// The worker went away before answering.
    WorkerDropped,
}

impl PersistenceFailure {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Admission => "admission",
            Self::WriteFailed => "write_failed",
            Self::Storage => "storage",
            Self::CommitUncertain => "commit_uncertain",
            Self::Protection => "protection",
            Self::ProtectionUncertain => "protection_uncertain",
            Self::WorkerDropped => "worker_dropped",
        }
    }
}

pub(crate) fn keymanager_stage_code(stage: KeymanagerStage) -> &'static str {
    match stage {
        KeymanagerStage::Validate => "validate",
        KeymanagerStage::Generate => "generate",
        KeymanagerStage::Begin => "begin",
        KeymanagerStage::Finish => "finish",
        KeymanagerStage::Roundtrip => "roundtrip",
    }
}

/// **What became of a "Connect without encryption?" offer** — the closed consent outcome an
/// insecure-only report carries for an eligible server (`plaintext_consent`). Never the server,
/// its address or its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum PlaintextConsentOutcome {
    /// Eligible, and the person had not answered — the question is on screen.
    Offered,
    /// Allowed, yet this discovery still could not connect (the grant was refused as stale, or
    /// the consented origin did not admit the credential).
    Accepted,
    /// *Not now* on the question.
    Declined,
    /// Turned off in Settings after it had been allowed.
    Revoked,
}

impl PlaintextConsentOutcome {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Offered => "offered",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Revoked => "revoked",
        }
    }
}

/// Everything one incident report carries. Every field is a closed enum, a bucket, a clamped
/// count or a bare number with no identity of its own — see the module doc.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct IncidentContext {
    pub kind: IncidentKind,
    pub link: LinkClass,
    /// The exact HTTP status — only with an `Answered*` link class.
    pub http_status: Option<u16>,
    /// The exact `CURLcode` — only with a `Dns`, `Tls`, `Timeout` or `TransportOther` link class.
    pub curl_rc: Option<i32>,
    pub unanswered: UnansweredBucket,
    pub failing_for: FailingForBucket,
    /// Which sign-in code the flow was on, 1..=4 — clamped, since it counts codes issued during
    /// one flow. `None` outside the code flow.
    pub code_generation: Option<u8>,
    pub persistence: Option<PersistenceFailure>,
    #[serde(default)]
    pub helper: Option<nj_platform::storage::wire::failure::HelperFailure>,
    #[serde(default)]
    pub candidate_errnos: [Option<i32>; 8],
    /// The key-service stage a protection failure stopped at.
    pub keymanager_stage: Option<KeymanagerStage>,
    /// The key service's own numeric error code, when it gave one.
    pub service_error_code: Option<i32>,
    /// Why discovery settled as insecure-only — only on
    /// [`DiscoveryClass::InsecureOnly`]. Every field closed; see
    /// [`crate::catalog::probe::InsecureEvidence`].
    #[serde(default)]
    pub insecure: Option<crate::catalog::probe::InsecureEvidence>,
    /// What became of the "Connect without encryption?" offer for the server an insecure-only
    /// verdict speaks about — only when that server was eligible to be asked. A closed code; the
    /// server and its owner are never carried.
    #[serde(default)]
    pub plaintext_consent: Option<PlaintextConsentOutcome>,
    /// What `/resources` returned when it named no server — only on [`DiscoveryClass::NoServers`].
    #[serde(default)]
    pub no_servers: Option<NoServersEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery: Option<DiscoveryEvidence>,
    /// Exact request count for local presentation. Reports continue to emit only the closed
    /// `unanswered` bucket; old persisted contexts deserialize with this absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_attempts: Option<u32>,
    /// Unix-epoch milliseconds when the context was BUILT — not when a report carrying it reaches
    /// the wire, which for a one-off is whenever the person presses Send and for a standing report
    /// can be a later launch's flush. `0` when the wall clock was at or before the epoch, and for
    /// an [`IncidentKind::Internal`] context, which is built where no clock is read.
    pub occurred_at_ms: u64,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// **The mark a failed sign-in read-out wears above its verdict** (spec "1A"): one per cause this
/// schema can name, chosen by [`IncidentContext::readout_glyph`] from the SAME typed evidence the
/// caption comes from, so the mark and the words cannot disagree. Telemetry owns which mark a cause
/// earns; the screen owns what each mark looks like and maps it to an icon
/// (`screens::login`), which is why this enum names no drawing code.
///
/// Twelve marks, one family: a BASE says what is involved (a server, plex.tv, an account, a
/// sign-in's stored key, a profile roster, a wait's clock), a BADGE says what went wrong.
/// `WifiSlash` stands alone — there is no server to blame when the TV itself has no link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ReadoutGlyph {
    /// A clock, alert badge — a wait that ran out.
    ClockBadgeAlert,
    /// A cloud, alert badge — plex.tv answered with an error, or the app's own machinery failed.
    CloudBadgeAlert,
    /// A globe, minus badge — plex.tv did not answer and the failure is not DNS or TLS.
    GlobeBadgeMinus,
    /// A globe, question badge — plex.tv could not be found (DNS).
    GlobeBadgeQuestion,
    /// A key, alert badge — the sign-in could not be saved or read back.
    KeyBadgeAlert,
    /// A lock, alert badge — plex.tv could not be reached securely (TLS).
    LockBadgeAlert,
    /// A people roster, alert badge — the profile switch failed.
    PeopleBadgeAlert,
    /// A person, x badge — plex.tv refused the token.
    PersonBadgeXmark,
    /// A server, minus badge — the servers (or the first content load) did not answer.
    ServerBadgeMinus,
    /// A server, plus badge — the account has no server.
    ServerBadgePlus,
    /// A server, x badge — a server answered and refused.
    ServerBadgeXmark,
    /// A slashed Wi-Fi mark — the TV itself cannot get out.
    WifiSlash,
}

/// **The [`LinkClass`] outcomes a plex.tv-facing incident always reads the same way**, shared by
/// [`IncidentKind::PinCreate`] (`mint_pin`, `auth.rs`) and the `Discovery(Silent)`/`PlexTv` arm of
/// [`IncidentContext::readout_glyph`] — the one place both used to spell out the same three arms
/// separately, which is exactly how `PinCreate` drifted (it never grew them at all, and kept a
/// fixed `WifiSlash` even once `self.link` recorded plex.tv actually answering). `Dns`/`Tls`/an
/// `Answered*` status all say something SPECIFIC happened reaching or on plex.tv, so both callers
/// agree here. `None` for `Timeout`/`TransportOther`/`Unknown` — the two callers diverge only on
/// that "nothing specific happened" fallback, each picking its own glyph, so this returns `None`
/// rather than guessing one. Exhaustive on [`LinkClass`], no wildcard arm.
#[allow(dead_code)]
fn plextv_link_glyph(link: LinkClass) -> Option<ReadoutGlyph> {
    use ReadoutGlyph as Glyph;
    match link {
        LinkClass::Dns => Some(Glyph::GlobeBadgeQuestion),
        LinkClass::Tls => Some(Glyph::LockBadgeAlert),
        LinkClass::Answered2xx
        | LinkClass::Answered4xx
        | LinkClass::Answered5xx
        | LinkClass::AnsweredOther => Some(Glyph::CloudBadgeAlert),
        LinkClass::Timeout | LinkClass::TransportOther | LinkClass::Unknown => None,
    }
}

impl IncidentContext {
    /// A context for `kind`, classifying the last network call and stamping the time. The link
    /// counters, code generation and persistence evidence start empty; the `with_*` builders set
    /// the ones a producer has.
    pub(crate) fn new(kind: IncidentKind, last: Option<Result<u16, RequestFailure>>) -> Self {
        let (link, http_status, curl_rc) = classify(last);
        Self {
            kind,
            link,
            http_status,
            curl_rc,
            unanswered: UnansweredBucket::Zero,
            failing_for: FailingForBucket::None,
            code_generation: None,
            persistence: None, helper: None, candidate_errnos: [None; 8],
            keymanager_stage: None,
            service_error_code: None,
            insecure: None,
            plaintext_consent: None,
            no_servers: None,
            discovery: None,
            discovery_attempts: None,
            occurred_at_ms: now_ms(),
        }
    }

    /// A failure inside the app ([`IncidentKind::Internal`]). Raised by the session owner, which
    /// reads no clock — it is pure and replayable — so it carries no occurrence time (see
    /// [`Self::occurred_at_ms`]).
    pub(crate) fn internal(class: InternalClass) -> Self {
        Self {
            kind: IncidentKind::Internal(class),
            link: LinkClass::Unknown,
            http_status: None,
            curl_rc: None,
            unanswered: UnansweredBucket::Zero,
            failing_for: FailingForBucket::None,
            code_generation: None,
            persistence: None, helper: None, candidate_errnos: [None; 8],
            keymanager_stage: None,
            service_error_code: None,
            insecure: None,
            plaintext_consent: None,
            no_servers: None,
            discovery: None,
            discovery_attempts: None,
            occurred_at_ms: 0,
        }
    }

    /// The sign-in wait's live counters: consecutive unanswered polls, how long they have lasted,
    /// and which code the flow is on.
    pub(crate) fn with_link_state(
        mut self,
        unanswered: u32,
        failing_for: Option<std::time::Duration>,
        code_generation: u32,
    ) -> Self {
        self.unanswered = UnansweredBucket::from_count(unanswered);
        self.failing_for = FailingForBucket::from_duration(failing_for);
        self.code_generation = Some(code_generation.clamp(1, 4) as u8);
        self
    }

    /// The evidence behind an insecure-only discovery verdict.
    pub(crate) fn with_insecure(mut self, evidence: crate::catalog::probe::InsecureEvidence) -> Self {
        self.insecure = Some(evidence);
        self
    }

    /// The consent outcome for an insecure-only verdict's eligible server (see
    /// [`Self::plaintext_consent`]); `None` leaves it off the report.
    pub(crate) fn with_plaintext_consent(mut self, outcome: Option<PlaintextConsentOutcome>) -> Self {
        self.plaintext_consent = outcome;
        self
    }

    /// The evidence behind a no-servers discovery verdict.
    pub(crate) fn with_no_servers(mut self, evidence: NoServersEvidence) -> Self {
        self.no_servers = Some(evidence);
        self
    }

    pub(crate) fn with_discovery(mut self, evidence: DiscoveryEvidence) -> Self {
        self.discovery = Some(evidence);
        self
    }

    /// A completed bounded retry run. Reuses the established bucket fields and deliberately does
    /// not set `code_generation`, which belongs only to QR/PIN issuance.
    pub(crate) fn with_retry_run(mut self, attempts: u32, elapsed: std::time::Duration) -> Self {
        self.unanswered = UnansweredBucket::from_count(attempts);
        self.failing_for = FailingForBucket::from_duration(Some(elapsed));
        self.discovery_attempts = Some(attempts);
        self
    }

    /// The failure class of a session persistence completion. A completion that is not a failure
    /// (durable, or superseded by a later clear) leaves the context unchanged.
    // The save-failure producer (`IncidentKind::SaveFailed`) is the next stage's; its evidence
    // builder lands with the schema so the body and the notice describe one closed set.
    #[allow(dead_code)]
    pub(crate) fn with_persistence(mut self, outcome: &CompletionOutcome) -> Self {
        let (class, protection) = match outcome {
            CompletionOutcome::Durable(_)
            | CompletionOutcome::Superseded
            | CompletionOutcome::Failed(Failure::Superseded) => return self,
            CompletionOutcome::Uncertain { .. } => (PersistenceFailure::CommitUncertain, None),
            CompletionOutcome::ProtectionUncertain(p) => (PersistenceFailure::ProtectionUncertain, Some(p)),
            CompletionOutcome::Failed(Failure::Admission(_)) => (PersistenceFailure::Admission, None),
            CompletionOutcome::Failed(Failure::Persistence(_)) => (PersistenceFailure::WriteFailed, None),
            CompletionOutcome::Failed(Failure::Storage(_) | Failure::Helper(..)) => (PersistenceFailure::Storage, None),
            CompletionOutcome::Failed(Failure::Protection(p)) => (PersistenceFailure::Protection, Some(p)),
            CompletionOutcome::Failed(Failure::WorkerDropped) => (PersistenceFailure::WorkerDropped, None),
        };
        let (helper, errnos) = outcome.helper_evidence();
        if let Some(failure) = helper {
            self.helper = Some(failure);
            self.candidate_errnos = errnos;
        }
        self.persistence = Some(class);
        self.keymanager_stage = protection.map(|p| p.failure.stage);
        self.service_error_code = protection.and_then(|p| p.failure.service_code);
        self
    }

    /// **The one glyph a page-filling `Failed` read-out draws above its verdict, decided from the
    /// SAME evidence as the caption** (`auth::discovery_failure`'s table), so the two can never
    /// disagree — this is not a second, string-matched opinion about what went wrong.
    ///
    /// Exhaustive on [`IncidentKind`] and, where discovery needs it, on [`DiscoveryClass`] and
    /// [`LinkClass`] — no wildcard arm, so a new variant is a compile error here rather than a
    /// silent fallthrough. The owner-approved mapping (`docs` carries none of this; it lives only
    /// here and at the call sites that read it):
    ///
    /// * [`IncidentKind::PinExpired`] / [`IncidentKind::LinkStalled`] — the wait ran out —
    ///   `ClockBadgeAlert`.
    /// * [`IncidentKind::PinCreate`] — `mint_pin` (`auth.rs`) keeps its last plex.tv response in
    ///   `self.link` the same as discovery does, so this follows the SAME [`LinkClass`] table as
    ///   the `Discovery(Silent)`/`PlexTv` arm below (`plextv_link_glyph`) rather than a fixed
    ///   glyph: `Dns` → `GlobeBadgeQuestion`, `Tls` → `LockBadgeAlert`, an `Answered*` class
    ///   (plex.tv answered, just not usably) → `CloudBadgeAlert`. Only its FALLBACK differs —
    ///   `Timeout`/`TransportOther`/`Unknown`, or no link recorded at all (`mint_pin` never even
    ///   got to ask) — `WifiSlash`, the genuine "can't get out" case, rather than the Discovery
    ///   arm's `GlobeBadgeMinus`.
    /// * [`IncidentKind::Authorization`] — plex.tv refused the token — `PersonBadgeXmark`.
    /// * `Discovery(NoServers)` — the account has no server — `ServerBadgePlus`.
    /// * `Discovery(Refused)` — a server answered and refused — `ServerBadgeXmark`.
    /// * `Discovery(Silent)` targeting [`DiscoveryTarget::Servers`] — plex.tv named servers that
    ///   never answered — `ServerBadgeMinus`; targeting [`DiscoveryTarget::PlexTv`] (or absent) —
    ///   plex.tv itself did not answer, so the badge follows `plextv_link_glyph`, falling back to
    ///   `GlobeBadgeMinus` (not `PinCreate`'s `WifiSlash`: discovery already ran, so SOME network
    ///   reached somewhere, unlike `PinCreate`'s failure to even start).
    /// * `Discovery(InsecureOnly)` — not in the owner's table; the closest honest read is "the
    ///   server itself is the obstacle", the same shape as a Silent/Servers verdict, so this
    ///   shares `ServerBadgeMinus` rather than inventing new art.
    /// * [`IncidentKind::SaveFailed`] / [`IncidentKind::StoredLocked`] — the sign-in could not be
    ///   kept or read back — `KeyBadgeAlert`.
    /// * [`IncidentKind::ProfileSwitch`] — `PeopleBadgeAlert`.
    /// * [`IncidentKind::ContentLoad`] — first content load failing reads exactly like Home's or
    ///   the Library's own "can't reach" verdict — `ServerBadgeMinus`.
    /// * [`IncidentKind::Internal`] — not in the owner's table either; the app's own machinery
    ///   failed rather than any named server or link, so this takes `CloudBadgeAlert`, the same
    ///   "something didn't work" mark an answered-but-broken plex.tv call wears, as the least
    ///   specific honest choice among the twelve.
    #[allow(dead_code)]
    pub(crate) fn readout_glyph(&self) -> ReadoutGlyph {
        use ReadoutGlyph as Glyph;
        match self.kind {
            IncidentKind::PinExpired | IncidentKind::LinkStalled => Glyph::ClockBadgeAlert,
            IncidentKind::PinCreate => plextv_link_glyph(self.link).unwrap_or(Glyph::WifiSlash),
            IncidentKind::Authorization => Glyph::PersonBadgeXmark,
            IncidentKind::Discovery(DiscoveryClass::NoServers) => Glyph::ServerBadgePlus,
            IncidentKind::Discovery(DiscoveryClass::Refused) => Glyph::ServerBadgeXmark,
            IncidentKind::Discovery(DiscoveryClass::InsecureOnly) => Glyph::ServerBadgeMinus,
            IncidentKind::Discovery(DiscoveryClass::Silent) => {
                match self.discovery.and_then(|d| d.target) {
                    Some(DiscoveryTarget::Servers) => Glyph::ServerBadgeMinus,
                    Some(DiscoveryTarget::PlexTv) | None =>
                        plextv_link_glyph(self.link).unwrap_or(Glyph::GlobeBadgeMinus),
                }
            }
            IncidentKind::SaveFailed | IncidentKind::StoredLocked => Glyph::KeyBadgeAlert,
            IncidentKind::ProfileSwitch => Glyph::PeopleBadgeAlert,
            IncidentKind::ContentLoad(ContentSource::Home | ContentSource::Libraries) => {
                Glyph::ServerBadgeMinus
            }
            IncidentKind::Internal(
                InternalClass::AdmissionRefused
                | InternalClass::WorkerRefused
                | InternalClass::WorkerDropped
                | InternalClass::CommitRefused
                | InternalClass::ClientIdUnavailable
                | InternalClass::Exhausted,
            ) => Glyph::CloudBadgeAlert,
        }
    }
}

#[cfg(test)]
mod readout_glyph_tests {
    use super::*;
    use super::ReadoutGlyph as Glyph;

    fn ctx(kind: IncidentKind) -> IncidentContext {
        IncidentContext::new(kind, None)
    }

    fn with_link(mut c: IncidentContext, link: LinkClass) -> IncidentContext {
        c.link = link;
        c
    }

    fn with_target(mut c: IncidentContext, target: DiscoveryTarget) -> IncidentContext {
        c.discovery = Some(DiscoveryEvidence { trigger: DiscoveryTrigger::Login, target: Some(target) });
        c
    }

    /// One row per [`IncidentKind`] arm (plus the discovery target/link split), so a new cause
    /// added to the enum without a row here fails loudly rather than silently inheriting whatever
    /// arm happened to be last.
    #[test]
    fn every_incident_kind_maps_to_the_owner_approved_glyph() {
        assert_eq!(ctx(IncidentKind::PinExpired).readout_glyph(), Glyph::ClockBadgeAlert);
        assert_eq!(ctx(IncidentKind::LinkStalled).readout_glyph(), Glyph::ClockBadgeAlert);
        // `PinCreate` follows the SAME plex.tv `LinkClass` table as the Silent/PlexTv discovery
        // arm below — `mint_pin` (`auth.rs`) keeps its last response in `self.link` too, so an
        // HTTP error or a malformed 2xx from plex.tv must not read as "no connection" (`WifiSlash`)
        // — only its fallback (no specific link recorded) does.
        assert_eq!(with_link(ctx(IncidentKind::PinCreate), LinkClass::Dns).readout_glyph(), Glyph::GlobeBadgeQuestion);
        assert_eq!(with_link(ctx(IncidentKind::PinCreate), LinkClass::Tls).readout_glyph(), Glyph::LockBadgeAlert);
        for answered in [
            LinkClass::Answered2xx,
            LinkClass::Answered4xx,
            LinkClass::Answered5xx,
            LinkClass::AnsweredOther,
        ] {
            assert_eq!(with_link(ctx(IncidentKind::PinCreate), answered).readout_glyph(), Glyph::CloudBadgeAlert,
                "PinCreate with link={answered:?} must not read as no-connection");
        }
        for other in [LinkClass::Timeout, LinkClass::TransportOther, LinkClass::Unknown] {
            assert_eq!(with_link(ctx(IncidentKind::PinCreate), other).readout_glyph(), Glyph::WifiSlash);
        }
        // No link recorded at all (the context's own default) is the genuine "never got to ask" case.
        assert_eq!(ctx(IncidentKind::PinCreate).readout_glyph(), Glyph::WifiSlash);
        assert_eq!(ctx(IncidentKind::Authorization).readout_glyph(), Glyph::PersonBadgeXmark);
        assert_eq!(
            ctx(IncidentKind::Discovery(DiscoveryClass::NoServers)).readout_glyph(),
            Glyph::ServerBadgePlus
        );
        assert_eq!(
            ctx(IncidentKind::Discovery(DiscoveryClass::Refused)).readout_glyph(),
            Glyph::ServerBadgeXmark
        );
        assert_eq!(
            ctx(IncidentKind::Discovery(DiscoveryClass::InsecureOnly)).readout_glyph(),
            Glyph::ServerBadgeMinus
        );
        assert_eq!(
            with_target(ctx(IncidentKind::Discovery(DiscoveryClass::Silent)), DiscoveryTarget::Servers)
                .readout_glyph(),
            Glyph::ServerBadgeMinus
        );
        let plextv = with_target(ctx(IncidentKind::Discovery(DiscoveryClass::Silent)), DiscoveryTarget::PlexTv);
        assert_eq!(with_link(plextv, LinkClass::Dns).readout_glyph(), Glyph::GlobeBadgeQuestion);
        assert_eq!(with_link(plextv, LinkClass::Tls).readout_glyph(), Glyph::LockBadgeAlert);
        for answered in [
            LinkClass::Answered2xx,
            LinkClass::Answered4xx,
            LinkClass::Answered5xx,
            LinkClass::AnsweredOther,
        ] {
            assert_eq!(with_link(plextv, answered).readout_glyph(), Glyph::CloudBadgeAlert);
        }
        for other in [LinkClass::Timeout, LinkClass::TransportOther, LinkClass::Unknown] {
            assert_eq!(with_link(plextv, other).readout_glyph(), Glyph::GlobeBadgeMinus);
        }
        // No discovery target recorded at all (defensive: the field is `Option`) reads the same
        // as `PlexTv` — the fallback branch above, exercised here with the default `Unknown` link.
        assert_eq!(
            ctx(IncidentKind::Discovery(DiscoveryClass::Silent)).readout_glyph(),
            Glyph::GlobeBadgeMinus
        );
        assert_eq!(ctx(IncidentKind::SaveFailed).readout_glyph(), Glyph::KeyBadgeAlert);
        assert_eq!(ctx(IncidentKind::StoredLocked).readout_glyph(), Glyph::KeyBadgeAlert);
        assert_eq!(ctx(IncidentKind::ProfileSwitch).readout_glyph(), Glyph::PeopleBadgeAlert);
        assert_eq!(
            ctx(IncidentKind::ContentLoad(ContentSource::Home)).readout_glyph(),
            Glyph::ServerBadgeMinus
        );
        assert_eq!(
            ctx(IncidentKind::ContentLoad(ContentSource::Libraries)).readout_glyph(),
            Glyph::ServerBadgeMinus
        );
        for internal in [
            InternalClass::AdmissionRefused,
            InternalClass::WorkerRefused,
            InternalClass::WorkerDropped,
            InternalClass::CommitRefused,
            InternalClass::ClientIdUnavailable,
            InternalClass::Exhausted,
        ] {
            assert_eq!(ctx(IncidentKind::Internal(internal)).readout_glyph(), Glyph::CloudBadgeAlert);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn bucket_edges() {
        assert_eq!(UnansweredBucket::from_count(0), UnansweredBucket::Zero);
        assert_eq!(UnansweredBucket::from_count(1), UnansweredBucket::One);
        assert_eq!(UnansweredBucket::from_count(2), UnansweredBucket::TwoToFive);
        assert_eq!(UnansweredBucket::from_count(5), UnansweredBucket::TwoToFive);
        assert_eq!(UnansweredBucket::from_count(6), UnansweredBucket::SixPlus);
        let f = |s| FailingForBucket::from_duration(Some(Duration::from_secs(s)));
        assert_eq!(FailingForBucket::from_duration(None), FailingForBucket::None);
        assert_eq!(f(9), FailingForBucket::Under10s);
        assert_eq!(f(10), FailingForBucket::Under60s);
        assert_eq!(f(59), FailingForBucket::Under60s);
        assert_eq!(f(60), FailingForBucket::Under5m);
        assert_eq!(f(299), FailingForBucket::Under5m);
        assert_eq!(f(300), FailingForBucket::FiveMinPlus);
    }

    #[test]
    fn the_context_buckets_clamps_and_stamps() {
        let before = now_ms();
        let ctx = IncidentContext::new(IncidentKind::Authorization, Some(Ok(429)))
            .with_link_state(3, Some(Duration::from_secs(45)), 9);
        assert!(ctx.occurred_at_ms >= before && ctx.occurred_at_ms <= now_ms());
        assert_eq!((ctx.link, ctx.http_status, ctx.curl_rc), (LinkClass::Answered4xx, Some(429), None));
        assert_eq!(ctx.unanswered, UnansweredBucket::TwoToFive);
        assert_eq!(ctx.failing_for, FailingForBucket::Under60s);
        assert_eq!(ctx.code_generation, Some(4));
        let low = IncidentContext::new(IncidentKind::PinCreate, None).with_link_state(0, None, 0);
        assert_eq!(low.code_generation, Some(1));
    }

    fn every_kind() -> Vec<IncidentKind> {
        use ContentSource as C;
        use DiscoveryClass as D;
        vec![
            IncidentKind::PinCreate,
            IncidentKind::LinkStalled,
            IncidentKind::PinExpired,
            IncidentKind::Authorization,
            IncidentKind::Discovery(D::NoServers),
            IncidentKind::Discovery(D::Refused),
            IncidentKind::Discovery(D::Silent),
            IncidentKind::Discovery(D::InsecureOnly),
            IncidentKind::SaveFailed,
            IncidentKind::StoredLocked,
            IncidentKind::ProfileSwitch,
            IncidentKind::ContentLoad(C::Home),
            IncidentKind::ContentLoad(C::Libraries),
            IncidentKind::Internal(InternalClass::AdmissionRefused),
            IncidentKind::Internal(InternalClass::WorkerRefused),
            IncidentKind::Internal(InternalClass::WorkerDropped),
            IncidentKind::Internal(InternalClass::CommitRefused),
            IncidentKind::Internal(InternalClass::ClientIdUnavailable),
            IncidentKind::Internal(InternalClass::Exhausted),
        ]
    }

    /// The read-out and the canon key on these codes: they must be distinct and closed-shaped.
    #[test]
    fn every_kind_has_a_distinct_stable_code() {
        let codes: Vec<_> = every_kind().into_iter().map(IncidentKind::code).collect();
        let mut unique = codes.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), codes.len(), "duplicate code in {codes:?}");
        assert!(codes.iter().all(|c| c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')));
    }

    fn protection_failure(service_code: Option<i32>) -> crate::catalog::session::persistence::ProtectionFailure {
        use nj_platform::storage::wire::{
            AuthPreservation, ErrorCode, KeymanagerFailure, KeymanagerFailureCategory,
            KeymanagerOperation,
        };
        crate::catalog::session::persistence::ProtectionFailure {
            failure: KeymanagerFailure {
                operation: KeymanagerOperation::Seal,
                stage: KeymanagerStage::Finish,
                code: ErrorCode::Unavailable,
                category: KeymanagerFailureCategory::ServiceRejected,
                service_code,
            },
            preservation: AuthPreservation::Unchanged,
            db8_commit_verified: false,
        }
    }

    #[test]
    fn a_persistence_completion_maps_to_its_closed_class() {
        use crate::catalog::session::async_persistence::PersistOutcome;
        let ctx = || IncidentContext::new(IncidentKind::SaveFailed, None);
        let sealed = ctx().with_persistence(&CompletionOutcome::Failed(Failure::Protection(
            protection_failure(Some(-3961)),
        )));
        assert_eq!(sealed.persistence, Some(PersistenceFailure::Protection));
        assert_eq!(sealed.keymanager_stage, Some(KeymanagerStage::Finish));
        assert_eq!(sealed.service_error_code, Some(-3961));

        let uncertain = ctx().with_persistence(&CompletionOutcome::ProtectionUncertain(protection_failure(None)));
        assert_eq!(uncertain.persistence, Some(PersistenceFailure::ProtectionUncertain));
        assert_eq!(uncertain.service_error_code, None);

        let write = ctx().with_persistence(&CompletionOutcome::Failed(Failure::Persistence(
            PersistOutcome::WriteFailed,
        )));
        assert_eq!(write.persistence, Some(PersistenceFailure::WriteFailed));
        assert_eq!(write.keymanager_stage, None);
        assert_eq!(
            ctx().with_persistence(&CompletionOutcome::Uncertain {
                stage: nj_platform::storage::CommitStage::Rename,
                errno: 5,
                helper: None,
            })
            .persistence,
            Some(PersistenceFailure::CommitUncertain)
        );
        assert_eq!(
            ctx().with_persistence(&CompletionOutcome::Failed(Failure::WorkerDropped)).persistence,
            Some(PersistenceFailure::WorkerDropped)
        );
        assert_eq!(
            ctx().with_persistence(&CompletionOutcome::Failed(Failure::Admission(
                nj_base::storage_worker::SubmitError::Full
            )))
            .persistence,
            Some(PersistenceFailure::Admission)
        );
        // Not failures: the context is untouched.
        assert_eq!(ctx().with_persistence(&CompletionOutcome::Superseded).persistence, None);
        assert_eq!(
            ctx().with_persistence(&CompletionOutcome::Failed(Failure::Superseded)).persistence,
            None
        );
    }

    #[test]
    fn persisted_no_servers_contexts_from_e437f264_and_79ef8433_still_round_trip() {
        const E437F264: &str = r#"{"kind":{"Discovery":"NoServers"},"link":"Unknown","http_status":null,"curl_rc":null,"unanswered":"Zero","failing_for":"None","code_generation":null,"persistence":null,"helper":null,"candidate_errnos":[null,null,null,null,null,null,null,null],"keymanager_stage":null,"service_error_code":null,"insecure":null,"plaintext_consent":null,"no_servers":{"resources":"TwoToFive","trigger":"Rediscover"},"occurred_at_ms":1}"#;
        const N79EF8433: &str = r#"{"kind":{"Discovery":"NoServers"},"link":"Unknown","http_status":null,"curl_rc":null,"unanswered":"Zero","failing_for":"None","code_generation":null,"persistence":null,"helper":null,"candidate_errnos":[null,null,null,null,null,null,null,null],"keymanager_stage":null,"service_error_code":null,"insecure":null,"plaintext_consent":null,"no_servers":{"resources":"One","trigger":"Login"},"occurred_at_ms":2}"#;
        for fixture in [E437F264, N79EF8433] {
            let context: IncidentContext = serde_json::from_str(fixture)
                .expect("a context persisted by the parent shape must deserialize");
            assert_eq!(serde_json::to_string(&context).unwrap(), fixture,
                "new optional discovery fields must not alter legacy NoServers JSON");
            assert!(context.discovery.is_none());
            assert!(context.discovery_attempts.is_none());
        }
    }
}
