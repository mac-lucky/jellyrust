//! Live executor for the Jellyfin sign-in screen (`screens::jf_login`), and the two account moves
//! that are Jellyfin's own: adopting a sign-in and signing out of one.
//!
//! The Session owner speaks plex.tv only, so a Jellyfin sign-in never passes through it. Adopting
//! one registers the seat, keeps the token (`jf::store`) and queues the same landing a Session
//! handoff takes (`bridge::follow_auth_landing`), which installs the server and enters Home.
use crate::jf::store::Stored;
use crate::screens::registry::{JfAuthCmd, JfAuthReply};
use nj_base::eventlog::log;
use std::sync::mpsc::Sender;

fn answer(what: &str, reply: Sender<JfAuthReply>, work: impl FnOnce() -> JfAuthReply + Send + 'static) {
    // A refused spawn drops the sender; the screen reads the disconnect as a failed step.
    nj_base::task::spawn_small(what, move || {
        let _ = reply.send(work());
        nj_machine::idle::invalidate();
    });
}

pub(super) fn execute(bridge: &mut super::bridge::Bridge, command: JfAuthCmd) {
    let client_id = crate::catalog::session::peek().client_id.clone();
    match command {
        // A server that answers only over plain http, in a build that will not send a credential
        // there on its own, is admitted at once when the person already allowed it (the probe has
        // just proved which server answers), and otherwise asked about.
        JfAuthCmd::Probe { candidates, reply } => answer("jf probe", reply, move || {
            let scope = crate::catalog::grant::scope();
            match crate::jf::auth::probe_first(&candidates, &client_id) {
                Ok((origin, info)) if crate::jf::plaintext::needs_consent(&origin) => {
                    let server_id = crate::jf::ids::normalize(&info.id);
                    match crate::jf::plaintext::admit_proved(scope, &origin, &server_id) {
                        Ok(()) => JfAuthReply::Probed(Ok((origin, info))),
                        Err(_) => {
                            let internet = crate::jf::plaintext::on_internet(&origin);
                            log(&format!("jf: {} answers only without encryption — asking", origin.log_form()));
                            JfAuthReply::Consent { origin, info, internet }
                        }
                    }
                }
                other => JfAuthReply::Probed(other),
            }
        }),
        JfAuthCmd::AllowPlaintext { origin, server_id, server_name, reply } => answer("jf allow plaintext", reply, move || {
            crate::jf::store::set_plaintext(&server_id, &server_name, &origin, true);
            let roster = crate::jf::store::roster();
            let _ = crate::jf::store::persist(&roster);
            log(&format!("jf: unencrypted connections allowed for the server at {}", origin.log_form()));
            JfAuthReply::Allowed(crate::jf::plaintext::admit(&origin, &server_id))
        }),
        JfAuthCmd::Password { origin, username, password, reply } => answer("jf sign-in", reply, move || {
            JfAuthReply::SignedIn(crate::jf::auth::sign_in_with_password(&origin, &client_id, &username, &password))
        }),
        JfAuthCmd::QuickConnectStart { origin, reply } => answer("jf quick connect", reply, move || {
            JfAuthReply::QuickConnect(crate::jf::auth::quick_connect_start(&origin, &client_id))
        }),
        JfAuthCmd::QuickConnectPoll { origin, qc, reply } => answer("jf quick connect", reply, move || {
            JfAuthReply::Polled(crate::jf::auth::quick_connect_poll(&origin, &client_id, &qc))
        }),
        JfAuthCmd::Adopt { origin, signed_in } => adopt(bridge, origin, signed_in),
        JfAuthCmd::PublicUsers { origin, reply } => answer("jf public users", reply, move || {
            JfAuthReply::People(crate::jf::auth::public_users(&origin, &client_id))
        }),
        JfAuthCmd::CheckKept { index, reply } => {
            let Some(user) = crate::jf::store::roster().users.get(usize::from(index)).cloned() else {
                let _ = reply.send(JfAuthReply::Checked(index, Err(crate::jf::auth::AuthError::Malformed)));
                return;
            };
            let Some(origin) = user.origin() else {
                let _ = reply.send(JfAuthReply::Checked(index, Err(crate::jf::auth::AuthError::Malformed)));
                return;
            };
            answer("jf check user", reply, move || {
                JfAuthReply::Checked(index, crate::jf::auth::check_token(&origin, &client_id, &user.token, &user.device_user))
            })
        }
    }
}

fn adopt(bridge: &mut super::bridge::Bridge, origin: crate::catalog::Origin, signed_in: crate::jf::auth::SignedIn) {
    crate::jf::seat::register_with(&origin, signed_in.seat());
    let stored = Stored::new(&origin, &signed_in);
    let gone = crate::jf::store::remember(stored.clone());
    // Persist the whole roster: this sign-in is now the active user, and anyone signed in on this
    // TV before stays remembered beside it.
    let roster = crate::jf::store::roster();
    let _ = nj_base::storage_worker::submit_retained(move || crate::jf::store::persist(&roster));
    // A token this television no longer keeps is signed out at the server, or it would stay valid
    // there with nobody able to sign it out. The seat at `origin` is the new sign-in's: kept.
    for old in still_valid(&stored, gone) {
        if let Some(at) = old.origin() {
            log_out(at, old, false);
        }
    }
    log(&format!("jf: signed in at {} — installing the server", origin.log_form()));
    bridge.hand_off_jf(ready_creds(&origin, signed_in.token));
}

/// What the Jellyfin sign-ins kept on this television make of a boot.
#[derive(Debug, PartialEq)]
pub(super) enum Kept {
    /// The active user, whom the server still honours (or could not be asked): boot installs them.
    Active(crate::catalog::Origin, Stored),
    /// Users are kept and none of them is active — the last one signed out: ask who is watching.
    WhoIsWatching,
    /// The server refused the active user's sign-in (revoked, or expired): they are forgotten here
    /// and asked for their password again, as a refused pick on the who's-watching screen is.
    SignInAgain(crate::screens::jf_login::Opening),
}

/// **The kept Jellyfin sign-ins at boot**: read them, admit the active user's unencrypted
/// connection (`jf::plaintext`), ask their server whether it still honours their token, and
/// register their seat. `None` when nobody is kept.
pub(super) fn at_boot(session: &crate::catalog::session::Session) -> Option<Kept> {
    let Some((origin, stored)) = crate::jf::store::load(session).and_then(|s| Some((s.origin()?, s))) else {
        let users = crate::jf::store::roster().users;
        if users.is_empty() {
            return None;
        }
        // A pick re-mints only the servers proved on this network (`pick_user`): prove each kept
        // server reached without encryption now, as the active user's is below.
        for (origin, server_id) in kept_servers(&users) {
            admit_at_boot(&origin, &server_id);
        }
        log(&format!("boot: {} Jellyfin users kept, nobody signed in — asking who's watching", users.len()));
        return Some(Kept::WhoIsWatching);
    };
    log(&format!("boot: stored Jellyfin sign-in at {}", origin.log_form()));
    admit_at_boot(&origin, &stored.server_id);
    // Only a revoked token counts (`check_token`: a 401, not a 403, which a valid one gets outside
    // its access schedule), and only from this sign-in's own server (another one may answer at a
    // reshuffled address): an unreachable server, or one not yet admitted, boots as before and its
    // shelves say it cannot be reached.
    let client_id = &session.client_id;
    let ours = |info: crate::jf::models::PublicSystemInfo| {
        crate::jf::ids::normalize(&info.id) == crate::jf::ids::normalize(&stored.server_id)
    };
    if crate::jf::auth::check_token(&origin, client_id, &stored.token, &stored.device_user)
        == Err(crate::jf::auth::AuthError::BadCredentials)
        && crate::jf::auth::probe(&origin, client_id).is_ok_and(ours)
    {
        log("boot: the server refused the stored Jellyfin sign-in — asking for it again");
        crate::jf::store::forget_user(&stored.server, &stored.user_id);
        let roster = crate::jf::store::roster();
        let _ = nj_base::storage_worker::submit_retained(move || crate::jf::store::persist(&roster));
        return Some(Kept::SignInAgain(sign_in_again(stored)));
    }
    crate::jf::seat::register_with(&origin, stored.seat());
    Some(Kept::Active(origin, stored))
}

/// The sign-in page's *Sign in as name* for a kept user whose sign-in the server refused.
fn sign_in_again(user: Stored) -> crate::screens::jf_login::Opening {
    let name = if user.user_name.is_empty() { user.user_id.clone() } else { user.user_name };
    crate::screens::jf_login::Opening::SignInAgain { server: user.server, server_name: user.server_name, id: user.user_id, name }
}

/// Each kept server once, by address and id: the people signed in on one server share it, and a
/// server that is off costs `admit_at_boot` a timeout every time it is asked.
fn kept_servers(users: &[Stored]) -> Vec<(crate::catalog::Origin, String)> {
    let mut out: Vec<(crate::catalog::Origin, String)> = Vec::new();
    for user in users {
        let Some(origin) = user.origin() else { continue };
        let id = crate::jf::ids::normalize(&user.server_id);
        if !out.iter().any(|(o, s)| *o == origin && *s == id) {
            out.push((origin, id));
        }
    }
    out
}

/// A server reached without encryption gets its token only after it has answered, here and now, as
/// the server the person allowed (`jf::plaintext`). Asked before anything is installed: a token
/// registered at an origin it may not travel to is blanked on the spot.
fn admit_at_boot(origin: &crate::catalog::Origin, server_id: &str) {
    if !crate::jf::plaintext::needs_consent(origin) || crate::jf::plaintext::granted(origin) {
        return;
    }
    match crate::jf::plaintext::admit(origin, &crate::jf::ids::normalize(server_id)) {
        Ok(()) => log("boot: the Jellyfin server answered as itself — its unencrypted connection is allowed"),
        Err(e) => log(&format!("boot: the Jellyfin server's unencrypted connection is not allowed yet ({e:?})")),
    }
}

/// The install a Jellyfin server takes: itself as the primary, no extras, its tier read off the
/// address as the dev boot does (there is no plex.tv connection list to read one from).
pub(super) fn ready_creds(origin: &crate::catalog::Origin, token: String) -> crate::auth::ReadyCreds {
    crate::auth::ReadyCreds {
        install: crate::auth::owner::ReadyInstall::PrimaryAndExtras(Vec::new()),
        origin: origin.clone(),
        address: origin.host().to_owned(),
        token,
        tier: Some(crate::catalog::probe::configured_tier(origin.host())),
        pin: None,
    }
}

/// **Choose the kept user at `index`** on the who's-watching screen. The user already running
/// enters straight away; another one becomes the active user and takes the same handoff a
/// sign-in does (`bridge::follow_auth_landing`), which reinstalls the server under their token
/// and drops every page of the previous user.
pub(super) fn pick_user(pages: &mut crate::ui::dispatch::Dispatcher<super::bridge::AppHost>,
    bridge: &mut super::bridge::Bridge, index: usize) {
    let roster = crate::jf::store::roster();
    let Some(user) = roster.users.get(index).cloned() else {
        log("jf: who's watching — that user is no longer kept");
        return;
    };
    let Some(origin) = user.origin() else { return };
    if roster.current().is_some_and(|live| live.same_user(&user)) {
        log("jf: who's watching — the signed-in user carries on");
        enter_signed_in(pages, bridge);
        return;
    }
    crate::jf::store::activate(&user.server, &user.user_id);
    let roster = crate::jf::store::roster();
    let _ = nj_base::storage_worker::submit_retained(move || crate::jf::store::persist(&roster));
    // The previous user's clients go; the server is installed again under this user's token and
    // their own DeviceId, exactly as after a sign-in. Revoking ends every unencrypted-connection
    // grant with the identity; the server was proved on this network already, so it is minted
    // again before the install asks for it.
    crate::catalog::revoke_all();
    crate::jf::plaintext::remint();
    crate::jf::seat::register_with(&origin, user.seat());
    log(&format!("jf: switching user at {} — installing the server", origin.log_form()));
    bridge.hand_off_jf(ready_creds(&origin, user.token));
}

/// **The kept user at `index` was refused by the server** (their token was revoked, or expired):
/// forget them here, and ask for their password again — the add-a-user screen, open at
/// *Sign in as name* with a line saying why. Whoever is signed in underneath stays.
pub(super) fn reauth_user(pages: &mut crate::ui::dispatch::Dispatcher<super::bridge::AppHost>,
    bridge: &mut super::bridge::Bridge, index: usize) {
    let Some(user) = crate::jf::store::roster().users.get(index).cloned() else { return };
    crate::jf::store::forget_user(&user.server, &user.user_id);
    let roster = crate::jf::store::roster();
    let _ = nj_base::storage_worker::submit_retained(move || crate::jf::store::persist(&roster));
    log("jf: who's watching — the server refused that user's sign-in; asking for it again");
    super::bridge::open_login_as(pages, bridge, sign_in_again(user));
}

/// The signed-in user leaves the who's-watching screen as themselves: on to their Home, or their
/// first-run Favourites when they have never answered it — the landing a sign-in takes, without
/// installing anything again.
fn enter_signed_in(pages: &mut crate::ui::dispatch::Dispatcher<super::bridge::AppHost>,
    bridge: &mut super::bridge::Bridge) {
    use crate::screens::registry::AppArg;
    super::input::maybe_ask_consent(pages);
    bridge.refresh_browse_directory();
    let to = if crate::stores::browse::onboard::asks(bridge.browse_directory()) { AppArg::Onboard } else { AppArg::Home };
    super::bridge::nav_root(pages, to);
}

/// What a Jellyfin sign-out left on this television.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SignedOut {
    /// No Jellyfin user was signed in: the caller signs out of plex.tv instead.
    NotJellyfin,
    /// Other users are still kept: the who's-watching screen offers them.
    OthersRemain,
    /// Nobody is kept: the sign-in screen, with the server under *Recent*.
    Nobody,
}

/// **Sign the active Jellyfin user out**: forget their sign-in here at once, tell the server on a
/// worker, and drop every registered client. Everyone else kept on this television stays, and so
/// do the server's address and name, for the sign-in screen's *Recent* row.
pub(super) fn sign_out() -> SignedOut {
    let Some(live) = crate::jf::store::current() else { return SignedOut::NotJellyfin };
    crate::jf::store::forget_user(&live.server, &live.user_id);
    let roster = crate::jf::store::roster();
    let left = if roster.users.is_empty() { SignedOut::Nobody } else { SignedOut::OthersRemain };
    let _ = nj_base::storage_worker::submit_retained(move || crate::jf::store::persist(&roster));
    crate::catalog::revoke_all();
    // Revoking ends every unencrypted-connection grant with the identity. The server was proved on
    // this network already: mint again, so the sign-out below reaches it, and whoever is still
    // kept picks themselves next over the same connection.
    crate::jf::plaintext::remint();
    revoke(live, false);
    left
}

/// Sign-out for Delete all local data: every kept user and the server go, and every stored copy
/// with them. Each kept user's token is revoked at the server. Nothing is written back — the
/// session file is about to be deleted, and a write queued behind that delete would bring it back.
pub(super) fn sign_out_and_forget_server() {
    let users = crate::jf::store::roster().users;
    // Each sign-out below may need the answer this erases (a server reached without encryption):
    // read it first, and let each worker prove its server once more for that last request.
    let allowed: Vec<bool> = users
        .iter()
        .map(|u| u.origin().is_some_and(|o| {
            crate::jf::store::plaintext_choice(&crate::jf::ids::normalize(&u.server_id), &o) == Some(true)
        }))
        .collect();
    crate::jf::store::forget_everything();
    let _ = nj_base::storage_worker::submit_retained(crate::jf::store::erase);
    crate::catalog::revoke_all();
    for (user, allowed) in users.into_iter().zip(allowed) {
        revoke(user, allowed);
    }
}

/// Which of the sign-ins `new` pushed out the server still honours: all but the same user's
/// earlier token on the same DeviceId basis, which the server revoked itself when it issued
/// `new` (it revokes per DeviceId and user, `jf::url::device_id`).
fn still_valid(new: &Stored, gone: Vec<Stored>) -> Vec<Stored> {
    gone.into_iter()
        .filter(|old| old.token != new.token && !(old.same_user(new) && old.device_user == new.device_user))
        .collect()
}

/// Tell the server to revoke `stored`'s token, on a worker, and forget its seat here.
/// `last_allowed`: the person's (now erased) answer for a server reached without encryption —
/// Delete all local data's last request to it; the grant it needs is withdrawn again after.
fn revoke(stored: Stored, last_allowed: bool) {
    let Some(origin) = stored.origin() else { return };
    crate::jf::seat::forget(&origin);
    log_out(origin, stored, last_allowed);
}

/// [`revoke`]'s request to the server, which leaves the seat at `origin` alone.
fn log_out(origin: crate::catalog::Origin, stored: Stored, last_allowed: bool) {
    let client_id = crate::catalog::session::peek().client_id.clone();
    nj_base::task::spawn_small("jf sign-out", move || {
        let server_id = crate::jf::ids::normalize(&stored.server_id);
        if last_allowed {
            let _ = crate::jf::plaintext::admit_for_sign_out(&origin, &server_id, true);
        }
        let revoked = crate::jf::auth::sign_out_detached(&origin, &client_id, &stored.token, &stored.device_user);
        if last_allowed {
            crate::jf::plaintext::withdraw(&server_id);
        }
        log(if revoked { "jf: signed out — the server revoked this device's token" }
            else { "jf: signed out — the server could not be told (the token stays valid there)" });
    });
}

/// The re-admission of a server reached without encryption: one attempt in flight at most, the
/// next one not before `due` (frame ms).
struct Readmit {
    in_flight: bool,
    due: u32,
    attempt: u32,
}

static READMIT: std::sync::Mutex<Readmit> = std::sync::Mutex::new(Readmit { in_flight: false, due: 0, attempt: 0 });

/// **Keep the active user's unencrypted connection admitted**, once per frame. Its grant dies with
/// the network (the app returning to the foreground) and is never minted when the server could not
/// be proved (it was off at boot), and Settings can turn it back on; in each case the server is
/// proved again here on a worker — never by asking the person again — and the token the registry
/// blanked is put back. Backs off from 5 s to a minute between attempts; costs one lock when there
/// is nothing to do.
pub(super) fn step_plaintext(now: u32) {
    let Some(user) = crate::jf::store::current() else { return };
    let Some(origin) = user.origin() else { return };
    if !crate::jf::plaintext::needs_consent(&origin) || crate::jf::plaintext::granted(&origin) {
        return;
    }
    let server_id = crate::jf::ids::normalize(&user.server_id);
    if crate::jf::store::plaintext_choice(&server_id, &origin) != Some(true) {
        return;
    }
    {
        let mut r = READMIT.lock().unwrap_or_else(|e| e.into_inner());
        if r.in_flight || now.wrapping_sub(r.due) > u32::MAX / 2 {
            return;
        }
        r.in_flight = true;
    }
    nj_base::task::spawn_small("jf readmit", move || {
        let admitted = crate::jf::plaintext::admit(&origin, &server_id).is_ok();
        if admitted {
            // The registry blanked this server's token when its grant ended: put it back.
            crate::catalog::install(&origin, &user.token, None, crate::catalog::ConnectionFacts::default());
            log(&format!("jf: unencrypted connection to {} proved again — reconnected", origin.log_form()));
        }
        let mut r = READMIT.lock().unwrap_or_else(|e| e.into_inner());
        r.in_flight = false;
        r.attempt = if admitted { 0 } else { (r.attempt + 1).min(4) };
        let wait_ms = 5_000u32 << r.attempt.min(3);
        r.due = now.wrapping_add(wait_ms.min(60_000));
        drop(r);
        nj_machine::idle::invalidate();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A Jellyfin server is registered under its own id**, not by address alone: the per-server
    /// memory keyed on `Client::machine_id` — the subtitle timing a viewer tuned for one file
    /// (`route::persist_subtitle_offset`), the poster disk cache — is skipped for an empty id. The
    /// install a sign-in, a user switch and a boot all take (`ready_creds`), then the readmission's
    /// address-only re-install, all keep it.
    #[test]
    fn a_jellyfin_server_is_installed_under_its_own_id() {
        let _g = nj_base::testlock::serial();
        crate::catalog::reset_servers_for_test();
        crate::jf::seat::reset_for_test();
        let origin = crate::catalog::Origin::parse("http://10.0.0.9:8096").unwrap();
        crate::jf::seat::register_with(&origin, crate::jf::seat::Seat {
            server_id: crate::jf::ids::normalize("AB12-CD34"), user_id: "u1".into(), ..Default::default() });
        let ready = ready_creds(&origin, "tok".into());
        crate::auth::install_captured_registry(&ready.origin, &ready.address, &ready.token, ready.tier,
            ready.pin.as_ref(), &[], Some("cid"));
        let sid = crate::catalog::current_server();
        assert_eq!(crate::catalog::client_for(sid).map(|c| c.machine_id()), Some("ab12cd34"));
        crate::catalog::install(&origin, "tok", None, crate::catalog::ConnectionFacts::default());
        assert_eq!(crate::catalog::current_server(), sid, "the readmission retokens the same slot");
        assert_eq!(crate::catalog::client_for(sid).map(|c| c.machine_id()), Some("ab12cd34"));
        crate::jf::seat::reset_for_test();
        crate::catalog::reset_servers_for_test();
    }

    /// A loopback Jellyfin answering every request with `status` — but `/System/Info/Public` as
    /// the server `id` — and recording each request's line and `Authorization` header.
    struct Server {
        origin: crate::catalog::Origin,
        seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        join: Option<std::thread::JoinHandle<()>>,
    }

    impl Server {
        fn new(status: u16) -> Self {
            Self::named(status, "ab12cd34")
        }

        fn named(status: u16, id: &'static str) -> Self {
            use std::io::{BufRead, BufReader, Write};
            use std::sync::atomic::Ordering;
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let origin = crate::catalog::Origin::parse(&format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port())).unwrap();
            let seen = std::sync::Arc::<std::sync::Mutex<Vec<String>>>::default();
            let stop = std::sync::Arc::<std::sync::atomic::AtomicBool>::default();
            let (log, stopped) = (std::sync::Arc::clone(&seen), std::sync::Arc::clone(&stop));
            let join = std::thread::spawn(move || while !stopped.load(Ordering::Relaxed) {
                match nj_base::testnet::accept(&listener) {
                    Ok((mut socket, _)) => {
                        let mut reader = BufReader::new(socket.try_clone().unwrap());
                        let mut request = Vec::new();
                        loop {
                            let mut line = String::new();
                            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" { break; }
                            if request.is_empty() || line.to_ascii_lowercase().starts_with("authorization:") {
                                request.push(line.trim_end().to_owned());
                            }
                        }
                        let (status, body) = if request[0].contains("/System/Info/Public") {
                            (200, format!(r#"{{"Id":"{id}","ProductName":"Jellyfin Server","StartupWizardCompleted":true}}"#))
                        } else {
                            (status, "{}".to_owned())
                        };
                        log.lock().unwrap().push(request.join(" | "));
                        let _ = write!(socket, "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(std::time::Duration::from_millis(2)),
                    Err(e) => panic!("accept: {e}"),
                }
            });
            Self { origin, seen, stop, join: Some(join) }
        }

        /// The requests seen so far, once one of them contains `want` (up to five seconds).
        fn wait_for(&self, want: &str) -> Vec<String> {
            for _ in 0..500 {
                let seen = self.seen.lock().unwrap().clone();
                if seen.iter().any(|r| r.contains(want)) { return seen; }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("no request with {want}: {:?}", self.seen.lock().unwrap());
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = self.join.take().map(std::thread::JoinHandle::join);
        }
    }

    fn kept(origin: &crate::catalog::Origin, user: &str, token: &str, device_user: &str) -> Stored {
        Stored { version: 2, server: origin.base(), token: token.into(), user_id: user.into(),
            user_name: user.to_uppercase(), server_id: "AB12-CD34".into(), server_name: "Living Room".into(),
            device_user: device_user.into(), ..Default::default() }
    }

    /// The session record boot reads, holding `roster` as the last run left it.
    fn session_holding(roster: crate::jf::store::Roster) -> crate::catalog::session::Session {
        crate::jf::store::forget_everything();
        assert!(crate::jf::store::persist(&roster));
        crate::catalog::session::load()
    }

    /// **A revoked or expired sign-in is found at boot**, not left to fail every shelf as an
    /// unreachable server would: the server's refusal of the kept token forgets the user here and
    /// opens the sign-in page at *Sign in as name*, saying why.
    #[test]
    fn a_kept_token_the_server_refuses_asks_for_the_sign_in_again() {
        let _g = nj_base::testlock::serial();
        let _t = crate::catalog::session::TempSession::new("jf-boot-revoked");
        let server = Server::new(401);
        let alex = kept(&server.origin, "alex", "revoked-token", "alex");
        let mut roster = crate::jf::store::Roster::default();
        roster.remember(alex.clone());
        let session = session_holding(roster);
        let landing = at_boot(&session);
        assert!(server.wait_for("/Users/Me").iter().any(|r| r.contains("revoked-token")), "the kept token is the one asked about");
        assert_eq!(landing, Some(Kept::SignInAgain(crate::screens::jf_login::Opening::SignInAgain {
            server: alex.server.clone(), server_name: "Living Room".into(), id: "alex".into(), name: "ALEX".into() })));
        nj_base::storage_worker::drain_for_test();
        assert!(crate::jf::store::roster().users.is_empty(), "the refused sign-in is not kept");
        assert!(crate::jf::store::load(&crate::catalog::session::load()).is_none(), "nor on the next launch");
        crate::jf::store::forget_everything();
        crate::jf::seat::reset_for_test();
    }

    /// A refusal counts only from the server the sign-in belongs to: another one answering at
    /// the address (a reshuffled home network) leaves the user kept, and boots as before.
    #[test]
    fn a_refusal_from_another_server_keeps_the_sign_in() {
        let _g = nj_base::testlock::serial();
        let _t = crate::catalog::session::TempSession::new("jf-boot-stranger");
        let server = Server::named(401, "ffff0000");
        let alex = kept(&server.origin, "alex", "kept-token", "alex");
        let mut roster = crate::jf::store::Roster::default();
        roster.remember(alex.clone());
        assert_eq!(at_boot(&session_holding(roster)), Some(Kept::Active(server.origin.clone(), alex)));
        assert_eq!(crate::jf::store::roster().users.len(), 1);
        crate::jf::store::forget_everything();
        crate::jf::seat::reset_for_test();
    }

    /// **Only a 401 says the kept token is dead.** Jellyfin answers a valid token with 403 when the
    /// user is outside a parental access schedule, or remote access is off for a remote request
    /// (`DefaultAuthorizationHandler`): that user is kept and boots, and the shelves say the server
    /// refused.
    #[test]
    fn a_kept_token_answered_403_keeps_the_user() {
        let _g = nj_base::testlock::serial();
        let _t = crate::catalog::session::TempSession::new("jf-boot-forbidden");
        let server = Server::new(403);
        let alex = kept(&server.origin, "alex", "scheduled-token", "alex");
        let mut roster = crate::jf::store::Roster::default();
        roster.remember(alex.clone());
        assert_eq!(at_boot(&session_holding(roster)), Some(Kept::Active(server.origin.clone(), alex)));
        nj_base::storage_worker::drain_for_test();
        assert_eq!(crate::jf::store::roster().users.len(), 1, "the user is kept");
        crate::jf::store::forget_everything();
        crate::jf::seat::reset_for_test();
    }

    /// Who's watching at boot proves each kept server once, not once per person kept on it: a
    /// server that is off would otherwise cost a timeout for every one of them.
    #[test]
    fn each_kept_server_is_proved_once_at_boot() {
        let home = crate::catalog::Origin::parse("http://10.0.0.9:8096").unwrap();
        let cabin = crate::catalog::Origin::parse("http://10.0.0.7:8096").unwrap();
        let users = [kept(&home, "sam", "ts", "sam"), kept(&home, "alex", "ta", "alex"), kept(&cabin, "kim", "tk", "kim")];
        assert_eq!(kept_servers(&users), [(home, "ab12cd34".to_string()), (cabin, "ab12cd34".to_string())]);
    }

    /// The who's-watching pick (`JfAuthCmd::CheckKept`) asks `check_token`, and its screen asks for
    /// the sign-in again only on `BadCredentials`: a 401 is that, a 403 is not.
    #[test]
    fn a_kept_pick_is_refused_only_by_a_401() {
        let _g = nj_base::testlock::serial();
        for (status, refused) in [(401, true), (403, false)] {
            let server = Server::new(status);
            let verdict = crate::jf::auth::check_token(&server.origin, "cid", "kept-token", "alex");
            assert_eq!(verdict == Err(crate::jf::auth::AuthError::BadCredentials), refused, "{status}: {verdict:?}");
        }
    }

    /// A token the server still honours boots as before.
    #[test]
    fn a_kept_token_the_server_honours_boots_as_that_user() {
        let _g = nj_base::testlock::serial();
        let _t = crate::catalog::session::TempSession::new("jf-boot-honoured");
        let server = Server::new(200);
        let alex = kept(&server.origin, "alex", "good-token", "alex");
        let mut roster = crate::jf::store::Roster::default();
        roster.remember(alex.clone());
        let landing = at_boot(&session_holding(roster));
        assert_eq!(landing, Some(Kept::Active(server.origin.clone(), alex)));
        assert!(crate::jf::seat::is_jf(&server.origin));
        crate::jf::store::forget_everything();
        crate::jf::seat::reset_for_test();
    }

    /// **After *Sign out of X* with others kept, the next launch asks who is watching** — the
    /// record keeps them with nobody active, and that is not "nobody kept".
    #[test]
    fn users_kept_with_nobody_active_boot_to_whos_watching() {
        let _g = nj_base::testlock::serial();
        let _t = crate::catalog::session::TempSession::new("jf-boot-nobody-active");
        let origin = crate::catalog::Origin::parse("http://10.0.0.9:8096").unwrap();
        let mut roster = crate::jf::store::Roster::default();
        roster.remember(kept(&origin, "sam", "ts", "sam"));
        roster.remember(kept(&origin, "alex", "ta", "alex"));
        assert!(roster.forget(&origin.base(), "alex"));
        assert_eq!((roster.users.len(), roster.active), (1, None));
        assert_eq!(at_boot(&session_holding(roster)), Some(Kept::WhoIsWatching));
        assert_eq!(crate::jf::store::roster().users.len(), 1, "sam is still offered");
        crate::jf::store::forget_everything();
        assert_eq!(at_boot(&session_holding(crate::jf::store::Roster::default())), None, "nobody kept");
        crate::jf::seat::reset_for_test();
    }

    /// **A sign-in that pushes out a still-valid token revokes it at the server.** The server
    /// revokes an earlier token only for the same user on the same DeviceId, and a password
    /// sign-in (the typed name) and Quick Connect (`""`) derive different ones; without this the
    /// old token stayed valid in the server's Devices list with nobody here able to sign it out.
    #[test]
    fn a_sign_in_revokes_the_token_it_replaced_under_another_device_id() {
        let _g = nj_base::testlock::serial();
        let _t = crate::catalog::session::TempSession::new("jf-adopt-revokes");
        crate::jf::store::forget_everything();
        let server = Server::new(204);
        crate::jf::store::remember(kept(&server.origin, "alex", "quick-connect-token", ""));
        let mut bridge = super::super::bridge::Bridge::for_test(|| 0);
        adopt(&mut bridge, server.origin.clone(), crate::jf::auth::SignedIn {
            token: "password-token".into(), user_id: "alex".into(), user_name: "ALEX".into(),
            server_id: "AB12-CD34".into(), server_name: "Living Room".into(), server_version: "10.10.0".into(),
            device_user: "alex".into() });
        let seen = server.wait_for("/Sessions/Logout");
        let logouts: Vec<&String> = seen.iter().filter(|r| r.contains("/Sessions/Logout")).collect();
        assert_eq!(logouts.len(), 1, "{seen:?}");
        assert!(logouts[0].contains("quick-connect-token") && !logouts[0].contains("password-token"), "{seen:?}");
        assert!(crate::jf::seat::is_jf(&server.origin), "the new sign-in's seat stays");
        assert_eq!(crate::jf::store::roster().users.len(), 1);
        nj_base::storage_worker::drain_for_test();
        crate::jf::store::forget_everything();
        crate::jf::seat::reset_for_test();
    }

    /// Which pushed-out sign-ins are revoked here: a token the server already revoked itself (the
    /// same user signing in again on the same DeviceId) is left alone.
    #[test]
    fn only_a_token_the_server_still_honours_is_revoked() {
        let origin = crate::catalog::Origin::parse("http://10.0.0.9:8096").unwrap();
        let new = kept(&origin, "alex", "t-new", "alex");
        let gone = vec![kept(&origin, "alex", "t-same-basis", "alex"), kept(&origin, "alex", "t-qc", ""),
            kept(&origin, "sam", "t-oldest", "sam"), kept(&origin, "alex", "t-new", "")];
        let tokens: Vec<String> = still_valid(&new, gone).into_iter().map(|u| u.token).collect();
        assert_eq!(tokens, ["t-qc", "t-oldest"]);
    }
}
