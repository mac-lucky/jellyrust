//! Jellyfin sign-in: the server's address first, then a username and password or a Quick Connect
//! code approved from a device that is already signed in.
//!
//! Every network step runs on the app's workers (`AppFx::JfAuth`); its answer comes back on a
//! channel this instance owns and drains on `Tick`, so a screen that is torn down mid-request simply
//! drops the receiver and the late answer goes nowhere. A successful sign-in is handed back as
//! `JfAuthCmd::Adopt`, and the app installs the server and leaves this route.
//!
//! The password never reaches the logical state, the probe or the log: the canon carries field
//! lengths only.
//!
//! **Layout**: Litefin's *modern* sign-in (`docs/litefin-look.md` 8). A brand pane on the left
//! (`widgets::draw_login_brand`: the app's mark, its name and a tagline at its foot) and, right of
//! it, the step's form on the flat black page, vertically centred: the step's title (51px bold) and
//! explanation, its inputs and buttons (72px at radius 14; Quick Connect is the accent button), and
//! on Quick Connect a centred column with the code in the accent and a status line that pulses while
//! the server waits for approval. Focus walks the form top to bottom ([`JfLoginScreen::order`]);
//! Quick Connect's pair is a row.
//!
//! **Without encryption** (approved design "Unencrypted connection consent"): a server that
//! answers only over plain http, in a build that never sends a credential there on its own, is
//! asked about once — *Connect without encryption?*, between the address and the sign-in — with
//! a stronger warning when the address is on the internet. The answer is the server's (its own id)
//! at that exact address (`jf::plaintext`); the server card marks the connection *Not encrypted*
//! from then on.
//!
//! **Add a user** ([`JfLoginScreen::adding_user`], approved design "Multi-user profiles", boards 3
//! and 3b): the same two columns on the server already signed in to, opening at *Who's signing
//! in?* — the people the server lists (`GET /Users/Public`) as avatars, *Other user* to type a
//! username, and Quick Connect. Picking someone with a password asks only for it, beside their
//! avatar and *Not name?*; someone without one signs straight in; someone already on this
//! television is switched to instead (`LoopReq::PickJellyfinUser`). A server that lists nobody
//! goes straight to the username and password.

use std::ffi::{CStr, CString};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use crate::jf::auth::{AuthError, QuickConnect, SignedIn};
use crate::catalog::Origin;
use crate::ui::frame::Budget;
use crate::ui::label::{HAlign, Label, VAlign};
use crate::ui::screen::{
    Activate, At, AxisMask, Dir, DrawFrame, EdgeRule, ElemKind, Enter, FocusSource, FocusTarget,
    Focusable, GroupKind, GroupSpec, HitSource, Hover, Placed, RenderStrategy, Screen, ScreenEvent,
    Seat, Step, Stop,
};
use crate::ui::text_buffer::TextBuffer;
use crate::ui::text_view::TextView;
use crate::ui::widgets::{login_pane, CtlPop, Spinner, CONTROL_GAP, LOGIN_AVATAR, LOGIN_CTL_H, LOGIN_INPUT_H,
    LOGIN_PERSON_GAP, LOGIN_PERSON_H, LOGIN_PERSON_W};
use crate::ui::{theme, Env, Painter, Rect, View};
use nj_machine::machine::{
    Canon, Cx, Delivery, Edge, Effects, EntryId, FocusKey, Fx, GroupId, Handled, InputKind,
    InputOwner, InstanceId, Key, LogicalState, Machine, MachineId, Measure, TextEdit, Tick,
};
use nj_machine::present::{PresentEvent, Provenance};
use nj_platform::i18n::msg;

use super::registry::{word, AppFx, AppLike, JfAuthCmd, JfAuthReply, LoopReq};

pub(crate) const SHAPE: &str = "JfLoginScreen{entry:u32,instance:u32,step:server|credentials|quick_connect|pick|consent,server_len:u32,user_len:u32,pass_len:u32,editing:Option<u32>,busy:Option<connecting|signing_in|starting|adopting>,waiting:bool,error:bool,origin:bool,adding:bool,people:u32,picked:bool,consent:Option<internet:bool>}";

const SERVER: u32 = 1;
const CONNECT: u32 = 2;
const USER: u32 = 3;
const PASS: u32 = 4;
const SIGN_IN: u32 = 5;
const QUICK: u32 = 6;
const CHANGE: u32 = 7;
const USE_PASSWORD: u32 = 8;
/// *Add a user*: someone not in the server's list types their username.
const OTHER: u32 = 10;
/// *Connect without encryption?*: allow it, or go back to the address.
const ALLOW: u32 = 11;
const NOT_NOW: u32 = 12;
/// One line of what allowing means: its mark and its sentence.
const BULLET_H: f32 = 92.0;
const BULLET_MARK: f32 = 30.0;
/// *Add a user*: the person at position `i` of the server's list is `PERSON + i`.
const PERSON: u32 = 100;
/// The most people the list shows (the server's own sign-in screen pages a longer list); anyone
/// else is *Other user*.
const MAX_PEOPLE: usize = 11;
/// Tiles a row of the server's list holds: Litefin's 215-wide tiles, 20 either side, across the
/// pane's width.
const PEOPLE_COLS: usize = 4;
const PEOPLE_PITCH: f32 = LOGIN_PERSON_W + LOGIN_PERSON_GAP;
/// A list longer than two rows of four draws its tiles at this fraction, six to a row, so it stays
/// two rows: every person the list shows and *Other user* fit.
const PEOPLE_COMPACT: f32 = 0.62;
const PEOPLE_COLS_COMPACT: usize = 6;
const _: () = assert!(MAX_PEOPLE + 1 <= 2 * PEOPLE_COLS_COMPACT);
/// `GroupId(0)` is the container's default fresh-mount target, so the first frame seats with no
/// correction of its own.
const GROUP: GroupId = GroupId(0);

/// Quick Connect is polled, not pushed; the web client asks every five seconds, a television that
/// is being looked at can afford twice that.
const POLL_MS: u32 = 2_500;
/// The *Recent* row: the server this television signed in to last, one press from reconnecting.
const RECENT: u32 = 9;
/// The form's measure, and Quick Connect's centred column (8: max 700).
const FORM_W: f32 = 720.0;
const QC_W: f32 = 700.0;
/// The step's title (8 Right pane: 51.2/700, 8 below) and its explanation under it.
const TITLE_SZ: i32 = theme::size::lf::S51;
const TITLE_GAP: f32 = 8.0;
const COPY_SZ: i32 = theme::size::lf::S22;
const COPY_GAP: f32 = theme::space::LG;
/// Quick Connect's instructions (8: 20.8 -> `S21`, tertiary, 48 below).
const QC_COPY_SZ: i32 = theme::size::lf::S21;
const QC_COPY_GAP: f32 = 48.0;
/// A field's label (8: 19.2/600 -> `S19` bold) and its line.
const LABEL_SZ: i32 = theme::size::lf::S19;
const LABEL_H: f32 = 28.0;
const HINT_H: f32 = 30.0;
/// The server card on the sign-in step: a disc, the name and the address, and *Change server*.
const CARD_H: f32 = 72.0;
const CARD_DISC: f32 = 56.0;
const CHANGE_H: f32 = 56.0;
/// The *Recent* row and its disc.
const RECENT_H: f32 = 96.0;
const RECENT_DISC: f32 = 52.0;
/// The status band (spinner line, or the error box): reserved whether or not it is drawn, so a
/// landing answer never moves a control the viewer is standing on.
const STATUS_H: f32 = 116.0;
const STATUS_SZ: i32 = theme::size::lf::S22;
/// Quick Connect's code: 96/700 in the accent with 0.18em between its characters (8); the TV has
/// no size from 80 up, so the HERO rung, and the tracking is laid out a character at a time.
const CODE_SZ: i32 = theme::size::lf::S72;
const CODE_H: f32 = 104.0;
const CODE_TRACKING: f32 = 0.18;
/// Quick Connect's status line (8: 17.6 -> `S18`, tertiary) pulses between these alphas over
/// [`PULSE_MS`] while the server waits for approval.
const QC_STATUS_SZ: i32 = theme::size::lf::S18;
const PULSE_MS: f32 = 2_500.0;
const PULSE_LOW: f32 = 0.5;
const FIELD_SZ: i32 = crate::ui::widgets::LOGIN_CTL_SZ;
const CARET_W: f32 = 3.0;
const POP_MS: u32 = 450;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Server,
    Credentials,
    QuickConnect,
    /// *Add a user*: who's signing in?
    Pick,
    /// *Connect without encryption?*
    Consent,
}

/// What the consent step is asking about.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Consent {
    /// The server's own id, normalized: the answer is about this server.
    server_id: String,
    /// The address is on the internet, not a home network: the stronger warning.
    internet: bool,
}

/// How the sign-in page opens when it is not a first sign-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Opening {
    /// *Add a user* on the active server, at *Who's signing in?*.
    AddUser,
    /// One kept user's sign-in was refused: straight to *Sign in as name* on their server, with a
    /// line saying why.
    SignInAgain { server: String, server_name: String, id: String, name: String },
}

/// Someone the server lists on its sign-in screen, as *Add a user* shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Person {
    id: String,
    name: String,
    has_password: bool,
    /// Already signed in on this television: their position in the roster, to switch to.
    on_tv: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Busy {
    Connecting,
    SigningIn,
    Starting,
    Adopting,
}

/// The server this television signed in to last, offered as the *Recent* row.
#[derive(Clone, Debug, PartialEq)]
struct Recent {
    name: String,
    /// What the address field would hold: `host:port`.
    address: String,
}

/// Every rect the form draws, in screen space.
struct Form {
    title: Rect,
    copy: Rect,
    card: Option<Rect>,
    labels: Vec<(u32, Rect)>,
    fields: Vec<(u32, Rect)>,
    hint: Option<Rect>,
    code: Option<Rect>,
    status: Rect,
    /// *Connect without encryption?*'s three lines.
    bullets: Vec<Rect>,
    or_rule: Option<Rect>,
    recent_caption: Option<Rect>,
    buttons: Vec<(u32, Rect)>,
    /// The server's list's column pitch, which bounds a tile's name (`widgets::login_name_span`).
    pitch: f32,
}

pub(crate) struct JfLoginScreen {
    entry: EntryId,
    instance: InstanceId,
    stage: Stage,
    server: TextBuffer,
    user: TextBuffer,
    pass: TextBuffer,
    editing: Option<u32>,
    busy: Option<Busy>,
    rx: Option<Receiver<JfAuthReply>>,
    error: Option<String>,
    origin: Option<Origin>,
    server_name: String,
    qc: Option<QuickConnect>,
    next_poll_ms: Option<u32>,
    now_ms: u32,
    pop_until_ms: u32,
    spin: nj_machine::motion::Phase,
    spin_ms: f32,
    pop: CtlPop<3>,
    recent: Option<Recent>,
    /// *Add a user* on the server already signed in to, rather than a first sign-in.
    adding: bool,
    /// *Add a user*: the people the server lists, once they have landed.
    people: Option<Vec<Person>>,
    /// *Add a user*: who was picked, so the password step names them.
    picked: Option<Person>,
    /// *Connect without encryption?*: what is being asked.
    consent: Option<Consent>,
}

impl JfLoginScreen {
    /// **Add a user**: open at *Who's signing in?* on the server the active user is signed in to
    /// (or, with nobody active, the one signed in to last) — or, for a user whose sign-in was
    /// refused, straight at *Sign in as name* with the reason in the status band. With no server
    /// known it is an ordinary sign-in.
    pub(crate) fn adding_user(entry: EntryId, instance: InstanceId, opening: Opening) -> Self {
        let mut s = Self::new(entry, instance);
        if let Opening::SignInAgain { server, server_name, id, name } = opening {
            if let Some(origin) = Origin::parse(&server) {
                s.server_name = if server_name.trim().is_empty() { origin.host().to_owned() } else { server_name };
                s.origin = Some(origin);
                s.adding = true;
                s.stage = Stage::Credentials;
                s.user = TextBuffer::new(name.clone(), name.len());
                s.picked = Some(Person { id, name, has_password: true, on_tv: None });
                s.error = Some(msg::jellyfin_login_error_expired().to_owned());
            }
            return s;
        }
        let server = crate::jf::store::current()
            .map(|u| (u.origin(), u.server_name))
            .or_else(|| crate::jf::store::recent_server().map(|r| (r.origin(), r.server_name)));
        if let Some((Some(origin), name)) = server {
            s.server_name = if name.trim().is_empty() { origin.host().to_owned() } else { name };
            s.origin = Some(origin);
            s.adding = true;
            s.stage = Stage::Pick;
        }
        s
    }

    pub(crate) fn new(entry: EntryId, instance: InstanceId) -> Self {
        // The server signed in to last — kept through a sign-out, and still there when the saved
        // server failed to come up — offered as the *Recent* row, one press from reconnecting;
        // the field stays free for another address.
        let recent = crate::jf::store::recent_server().and_then(|stored| {
            let origin = stored.origin()?;
            let host = origin.host();
            let address =
                if host.contains(':') { format!("[{host}]:{}", origin.port()) } else { format!("{host}:{}", origin.port()) };
            let name = if stored.server_name.trim().is_empty() { address.clone() } else { stored.server_name.clone() };
            Some(Recent { name, address })
        });
        let server = String::new();
        let caret = 0;
        Self {
            entry,
            instance,
            stage: Stage::Server,
            server: TextBuffer::new(server, caret),
            user: TextBuffer::new(String::new(), 0),
            pass: TextBuffer::new(String::new(), 0),
            editing: None,
            busy: None,
            rx: None,
            error: None,
            origin: None,
            server_name: String::new(),
            qc: None,
            next_poll_ms: None,
            now_ms: 0,
            pop_until_ms: 0,
            spin: nj_machine::motion::Phase::default(),
            spin_ms: 0.0,
            pop: CtlPop::new(),
            recent,
            adding: false,
            people: None,
            picked: None,
            consent: None,
        }
    }

    fn key(&self, elem: u32) -> FocusKey<u32> {
        FocusKey { entry: self.entry, elem }
    }

    fn fields(&self) -> &'static [u32] {
        match self.stage {
            Stage::Server => &[SERVER],
            // The person picked from the server's list has their name already: only the password.
            Stage::Credentials if self.picked.is_some() => &[PASS],
            Stage::Credentials => &[USER, PASS],
            Stage::QuickConnect | Stage::Pick | Stage::Consent => &[],
        }
    }

    /// The controls that pop on focus, in their [`CtlPop`] slots. A person's avatar is not one.
    fn buttons(&self) -> &'static [u32] {
        match self.stage {
            Stage::Server => &[CONNECT, RECENT],
            Stage::Credentials => &[SIGN_IN, QUICK, CHANGE],
            Stage::QuickConnect => &[USE_PASSWORD, CHANGE],
            Stage::Pick => &[OTHER, QUICK],
            Stage::Consent => &[ALLOW, NOT_NOW],
        }
    }

    /// The people shown, at most [`MAX_PEOPLE`].
    fn shown(&self) -> &[Person] {
        self.people.as_deref().map_or(&[], |p| &p[..p.len().min(MAX_PEOPLE)])
    }

    /// The person behind a `PERSON + i` element.
    fn person(&self, elem: u32) -> Option<&Person> {
        elem.checked_sub(PERSON).and_then(|i| self.shown().get(i as usize))
    }

    /// *Change server*, or — adding a user — the way back to the server's list: *Not name?* beside
    /// the person picked. Adding a user never changes server, so without a pick it is not offered.
    fn offers_change(&self) -> bool {
        !self.adding || (self.stage == Stage::Credentials && self.picked.is_some())
    }

    /// Every focusable element, in the order focus walks it: down the panel, or along Quick
    /// Connect's row.
    fn order(&self) -> Vec<u32> {
        match self.stage {
            Stage::Server => {
                let mut v = vec![SERVER, CONNECT];
                if self.recent.is_some() {
                    v.push(RECENT);
                }
                v
            }
            Stage::Credentials => {
                let mut v = Vec::with_capacity(5);
                if self.offers_change() {
                    v.push(CHANGE);
                }
                v.extend_from_slice(self.fields());
                v.extend([SIGN_IN, QUICK]);
                v
            }
            Stage::QuickConnect if self.offers_change() => vec![USE_PASSWORD, CHANGE],
            Stage::QuickConnect => vec![USE_PASSWORD],
            Stage::Pick => {
                let mut v: Vec<u32> = (0..self.shown().len() as u32).map(|i| PERSON + i).collect();
                v.extend([OTHER, QUICK]);
                v
            }
            Stage::Consent => vec![ALLOW, NOT_NOW],
        }
    }

    /// Focus across the server's list: LEFT/RIGHT along a row, UP/DOWN a column, DOWN from the last
    /// row to Quick Connect and UP from it back to the row's end.
    fn grid_step(&self, elem: u32, dir: Dir) -> Option<u32> {
        let grid = self.grid();
        if elem == QUICK {
            return (dir == Dir::Up).then(|| *grid.last().expect("Other user is always there"));
        }
        let i = grid.iter().position(|&e| e == elem)?;
        let (n, cols) = (grid.len(), self.cols());
        match dir {
            Dir::Left if i % cols > 0 => Some(grid[i - 1]),
            Dir::Right if i % cols < cols - 1 && i + 1 < n => Some(grid[i + 1]),
            Dir::Up => i.checked_sub(cols).map(|j| grid[j]),
            Dir::Down if (i / cols + 1) * cols < n => Some(grid[(i + cols).min(n - 1)]),
            Dir::Down => Some(QUICK),
            _ => None,
        }
    }

    /// The server's list is a grid: its people and *Other user*, [`Self::cols`] to a row, with
    /// Quick Connect under it.
    fn grid(&self) -> Vec<u32> {
        let mut v: Vec<u32> = (0..self.shown().len() as u32).map(|i| PERSON + i).collect();
        v.push(OTHER);
        v
    }

    /// Quick Connect's two buttons are a row; every other step is a column.
    fn is_row(&self) -> bool {
        self.stage == Stage::QuickConnect
    }

    fn elems(&self) -> impl Iterator<Item = u32> {
        self.order().into_iter()
    }

    /// Where a step seats focus: the Recent server when there is one (one press to reconnect), the
    /// username, or the way back to the password.
    fn first(&self) -> u32 {
        match self.stage {
            Stage::Server if self.recent.is_some() => RECENT,
            Stage::Server => SERVER,
            Stage::Credentials if self.picked.is_some() => PASS,
            Stage::Credentials => USER,
            Stage::QuickConnect => USE_PASSWORD,
            Stage::Consent => ALLOW,
            // The first person not already on this TV: they are who is being added.
            Stage::Pick => self
                .shown()
                .iter()
                .position(|p| p.on_tv.is_none())
                .map_or(OTHER, |i| PERSON + i as u32),
        }
    }

    fn buffer(&self, elem: u32) -> &TextBuffer {
        match elem {
            USER => &self.user,
            PASS => &self.pass,
            _ => &self.server,
        }
    }

    fn buffer_mut(&mut self, elem: u32) -> &mut TextBuffer {
        match elem {
            USER => &mut self.user,
            PASS => &mut self.pass,
            _ => &mut self.server,
        }
    }

    fn waiting(&self) -> bool {
        self.qc.is_some() && self.error.is_none()
    }

    fn spinning(&self) -> bool {
        self.busy.is_some() || self.waiting()
    }

    fn reseat<H: AppLike>(&self, elem: u32, fx: &mut Effects<'_, H>) {
        let me = fx.from();
        fx.push(Fx::Deliver(
            me,
            Delivery::Screen(ScreenEvent::Enter(Enter::Fresh { focus: FocusTarget::Elem(self.key(elem)) })),
        ));
    }

    fn keyboard<H: AppLike>(&self, up: bool, fx: &mut Effects<'_, H>) {
        fx.push(Fx::Deliver(MachineId::Instance(self.instance), Delivery::Keyboard { up }));
        fx.invalidate(Provenance::Input);
    }

    fn open<H: AppLike>(&mut self, field: u32, fx: &mut Effects<'_, H>) {
        if self.busy.is_some() {
            return;
        }
        let buf = self.buffer_mut(field);
        *buf = TextBuffer::new(buf.text().to_owned(), usize::MAX);
        let was_up = self.editing.is_some();
        self.editing = Some(field);
        if !was_up {
            self.keyboard(true, fx);
        }
    }

    fn close<H: AppLike>(&mut self, fx: &mut Effects<'_, H>) {
        if self.editing.take().is_some() {
            self.keyboard(false, fx);
        }
    }

    fn edit<H: AppLike>(&mut self, field: u32, edit: &TextEdit, fx: &mut Effects<'_, H>) {
        self.buffer_mut(field).edit(edit);
        if self.busy.is_none() {
            self.error = None;
        }
        fx.invalidate(Provenance::Input);
    }

    fn go<H: AppLike>(&mut self, stage: Stage, fx: &mut Effects<'_, H>) {
        self.close(fx);
        self.stage = stage;
        self.rx = None;
        self.busy = None;
        self.qc = None;
        self.next_poll_ms = None;
        self.error = None;
        if stage == Stage::Server {
            self.origin = None;
            self.consent = None;
        }
        if stage == Stage::Pick {
            self.picked = None;
            self.user = TextBuffer::new(String::new(), 0);
            self.pass = TextBuffer::new(String::new(), 0);
        }
        self.reseat(self.first(), fx);
        fx.invalidate(Provenance::Input);
    }

    fn request<H: AppLike>(
        &mut self,
        busy: Busy,
        fx: &mut Effects<'_, H>,
        command: impl FnOnce(Sender<JfAuthReply>) -> JfAuthCmd,
    ) {
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.busy = Some(busy);
        self.error = None;
        fx.push(Fx::App(AppFx::JfAuth(command(tx))));
        fx.invalidate(Provenance::Input);
    }

    fn connect<H: AppLike>(&mut self, fx: &mut Effects<'_, H>) {
        let candidates = crate::jf::address::candidates(self.server.text());
        if candidates.is_empty() {
            self.error = Some(msg::jellyfin_login_error_address().to_owned());
            fx.invalidate(Provenance::Input);
            return;
        }
        self.request(Busy::Connecting, fx, |reply| JfAuthCmd::Probe { candidates, reply });
    }

    fn sign_in<H: AppLike>(&mut self, fx: &mut Effects<'_, H>) {
        let Some(origin) = self.origin.clone() else { return };
        let username = self.user.text().trim().to_owned();
        if username.is_empty() {
            self.reseat(USER, fx);
            self.open(USER, fx);
            return;
        }
        let password = self.pass.text().to_owned();
        self.request(Busy::SigningIn, fx, |reply| JfAuthCmd::Password { origin, username, password, reply });
    }

    fn start_quick_connect<H: AppLike>(&mut self, fx: &mut Effects<'_, H>) {
        let Some(origin) = self.origin.clone() else { return };
        self.go(Stage::QuickConnect, fx);
        self.request(Busy::Starting, fx, |reply| JfAuthCmd::QuickConnectStart { origin, reply });
    }

    fn poll<H: AppLike>(&mut self, fx: &mut Effects<'_, H>) {
        let (Some(origin), Some(qc)) = (self.origin.clone(), self.qc.clone()) else { return };
        self.next_poll_ms = None;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        fx.push(Fx::App(AppFx::JfAuth(JfAuthCmd::QuickConnectPoll { origin, qc, reply: tx })));
    }

    fn adopt<H: AppLike>(&mut self, signed_in: SignedIn, fx: &mut Effects<'_, H>) {
        let Some(origin) = self.origin.clone() else { return };
        self.pass = TextBuffer::new(String::new(), 0);
        self.qc = None;
        self.next_poll_ms = None;
        self.busy = Some(Busy::Adopting);
        fx.push(Fx::App(AppFx::JfAuth(JfAuthCmd::Adopt { origin, signed_in })));
    }

    fn fail(&mut self, error: AuthError) {
        self.error = Some(message(&error));
        self.busy = None;
        self.qc = None;
        self.next_poll_ms = None;
    }

    fn drain<H: AppLike>(&mut self, fx: &mut Effects<'_, H>) {
        let Some(rx) = &self.rx else { return };
        let reply = match rx.try_recv() {
            Ok(reply) => reply,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                self.fail(AuthError::Unreachable);
                fx.invalidate(Provenance::Input);
                return;
            }
        };
        self.rx = None;
        match reply {
            JfAuthReply::Probed(Ok((origin, info))) => {
                self.server_name = if info.server_name.trim().is_empty() {
                    origin.host().to_owned()
                } else {
                    info.server_name
                };
                self.go(Stage::Credentials, fx);
                self.origin = Some(origin);
            }
            JfAuthReply::SignedIn(Ok(signed_in)) | JfAuthReply::Polled(Ok(Some(signed_in))) => {
                self.adopt(signed_in, fx);
            }
            JfAuthReply::QuickConnect(Ok(qc)) => {
                self.busy = None;
                self.qc = Some(qc);
                self.next_poll_ms = Some(self.now_ms.wrapping_add(POLL_MS));
            }
            JfAuthReply::Polled(Ok(None)) => {
                self.next_poll_ms = Some(self.now_ms.wrapping_add(POLL_MS));
            }
            JfAuthReply::People(Ok(listed)) => {
                self.busy = None;
                let roster = crate::jf::store::roster();
                let origin = self.origin.as_ref().map(|o| o.base()).unwrap_or_default();
                let people: Vec<Person> = listed
                    .into_iter()
                    .map(|u| {
                        let probe = crate::jf::store::Stored { server: origin.clone(), user_id: u.id.clone(), ..Default::default() };
                        let on_tv = roster.users.iter().position(|kept| kept.same_user(&probe)).map(|i| i as u8);
                        Person { id: u.id, name: u.name, has_password: u.has_password, on_tv }
                    })
                    .collect();
                let nobody = people.is_empty();
                self.people = Some(people);
                if nobody {
                    // The administrator hides the list: ask for a username instead.
                    self.go(Stage::Credentials, fx);
                } else {
                    self.reseat(self.first(), fx);
                }
            }
            JfAuthReply::Probed(Err(e))
            | JfAuthReply::SignedIn(Err(e))
            | JfAuthReply::QuickConnect(Err(e))
            | JfAuthReply::Polled(Err(e))
            | JfAuthReply::People(Err(e)) => self.fail(e),
            // Asked by the who's-watching screen, never by this one.
            JfAuthReply::Checked(..) => {}
            JfAuthReply::Consent { origin, info, internet } => {
                self.server_name = if info.server_name.trim().is_empty() { origin.host().to_owned() } else { info.server_name };
                let server_id = crate::jf::ids::normalize(&info.id);
                self.go(Stage::Consent, fx);
                self.origin = Some(origin);
                self.consent = Some(Consent { server_id, internet });
            }
            JfAuthReply::Allowed(Ok(())) => {
                let origin = self.origin.clone();
                self.go(Stage::Credentials, fx);
                self.origin = origin;
            }
            JfAuthReply::Allowed(Err(e)) => self.fail(e),
        }
        fx.invalidate(Provenance::Input);
    }

    fn tick<H: AppLike>(&mut self, t: Tick, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) {
        self.now_ms = t.ms;
        self.drain(fx);
        if self.rx.is_none() && self.next_poll_ms.is_some_and(|due| t.ms.wrapping_sub(due) < u32::MAX / 2) {
            self.poll(fx);
        }
        self.advance_motion(t, &mut fx.present());
        if t.ms.wrapping_sub(self.pop_until_ms) > u32::MAX / 2 {
            fx.present().note(PresentEvent::Motion);
        }
        let focused = cx
            .focus
            .current
            .filter(|k| k.entry == self.entry)
            .and_then(|k| self.buttons().iter().position(|&b| b == k.elem));
        self.pop.step(focused, t.dt());
    }

    fn activate<H: AppLike>(&mut self, elem: u32, fx: &mut Effects<'_, H>) {
        if !self.elems().any(|e| e == elem) {
            return;
        }
        match elem {
            CHANGE if self.adding => {
                self.go(Stage::Pick, fx);
                if self.people.is_none() {
                    self.load_people(fx);
                }
                return;
            }
            CHANGE => return self.go(Stage::Server, fx),
            NOT_NOW => return self.go(Stage::Server, fx),
            USE_PASSWORD => {
                let origin = self.origin.clone();
                self.go(Stage::Credentials, fx);
                self.origin = origin;
                return;
            }
            _ => {}
        }
        if self.busy.is_some() {
            return;
        }
        match elem {
            SERVER | USER | PASS => self.open(elem, fx),
            CONNECT => self.connect(fx),
            RECENT => {
                if let Some(recent) = &self.recent {
                    let address = recent.address.clone();
                    let caret = address.len();
                    self.server = TextBuffer::new(address, caret);
                    self.connect(fx);
                }
            }
            SIGN_IN => self.sign_in(fx),
            QUICK => self.start_quick_connect(fx),
            ALLOW => self.allow(fx),
            OTHER => {
                self.picked = None;
                self.user = TextBuffer::new(String::new(), 0);
                self.go(Stage::Credentials, fx);
            }
            _ => {
                if let Some(person) = self.person(elem).cloned() {
                    self.pick(person, fx);
                }
            }
        }
    }

    /// *Connect without encryption*: the app records the answer, proves the server again and lets
    /// credentials through; the sign-in step follows.
    fn allow<H: AppLike>(&mut self, fx: &mut Effects<'_, H>) {
        let (Some(origin), Some(consent)) = (self.origin.clone(), self.consent.clone()) else { return };
        let server_name = self.server_name.clone();
        self.request(Busy::Connecting, fx, |reply| JfAuthCmd::AllowPlaintext {
            origin, server_id: consent.server_id, server_name, reply,
        });
    }

    /// The connection to this server is plain http: its card says so.
    fn unencrypted(&self) -> bool {
        self.origin.as_ref().is_some_and(|o| !o.is_tls())
    }

    /// *Add a user*: ask the server who it lists.
    fn load_people<H: AppLike>(&mut self, fx: &mut Effects<'_, H>) {
        if let Some(origin) = self.origin.clone() {
            self.request(Busy::Connecting, fx, |reply| JfAuthCmd::PublicUsers { origin, reply });
        }
    }

    /// *Add a user*: someone already on this television is switched to; someone without a
    /// password signs straight in; anyone else is asked for theirs.
    fn pick<H: AppLike>(&mut self, person: Person, fx: &mut Effects<'_, H>) {
        if let Some(index) = person.on_tv {
            fx.push(Fx::App(AppFx::Loop(LoopReq::PickJellyfinUser(index))));
            return;
        }
        let name = person.name.clone();
        let needs_password = person.has_password;
        self.picked = Some(person);
        self.go(Stage::Credentials, fx);
        self.user = TextBuffer::new(name.clone(), name.len());
        if needs_password {
            self.open(PASS, fx);
        } else {
            self.sign_in(fx);
        }
    }

    /// OK on the keyboard moves the form on: the address connects, the username hands over to the
    /// password, and the password signs in.
    fn submit<H: AppLike>(&mut self, field: u32, fx: &mut Effects<'_, H>) {
        match field {
            USER => {
                self.editing = Some(PASS);
                self.pass = TextBuffer::new(self.pass.text().to_owned(), usize::MAX);
                self.reseat(PASS, fx);
                fx.invalidate(Provenance::Input);
            }
            SERVER => {
                self.close(fx);
                self.connect(fx);
            }
            _ => {
                self.close(fx);
                self.sign_in(fx);
            }
        }
    }

    fn back<H: AppLike>(&mut self, fx: &mut Effects<'_, H>) {
        if self.adding {
            return self.back_adding(fx);
        }
        match self.stage {
            Stage::Server if self.busy.is_some() => {
                self.rx = None;
                self.busy = None;
                fx.invalidate(Provenance::Input);
            }
            Stage::Server => fx.push(Fx::App(AppFx::Loop(LoopReq::BackAtRoot))),
            Stage::Credentials => self.go(Stage::Server, fx),
            Stage::QuickConnect => {
                let origin = self.origin.clone();
                self.go(Stage::Credentials, fx);
                self.origin = origin;
            }
            Stage::Pick => fx.push(Fx::App(AppFx::Loop(LoopReq::BackAtRoot))),
            Stage::Consent => self.go(Stage::Server, fx),
        }
    }

    /// BACK while adding a user walks back to the server's list, then out: to the user signed in
    /// underneath, else to the who's-watching screen. A server that lists nobody has no list to
    /// walk back to.
    fn back_adding<H: AppLike>(&mut self, fx: &mut Effects<'_, H>) {
        let listed = self.people.as_ref().is_some_and(|p| !p.is_empty());
        match self.stage {
            Stage::QuickConnect if self.picked.is_some() || !listed => self.go(Stage::Credentials, fx),
            Stage::Credentials | Stage::QuickConnect if listed => self.go(Stage::Pick, fx),
            _ => {
                let roster = crate::jf::store::roster();
                let req = match roster.active {
                    Some(i) => LoopReq::PickJellyfinUser(i as u8),
                    None => LoopReq::AccountSwitchUser,
                };
                self.rx = None;
                self.busy = None;
                fx.push(Fx::App(AppFx::Loop(req)));
            }
        }
    }

    /// The panel's rects for this step. Built top-down from 0, then the whole panel is centred
    /// vertically on the screen and every rect moved with it.
    /// The column the form stands in: centred in the right pane, at most [`FORM_W`] wide; Quick
    /// Connect's is [`QC_W`], and the server's list takes the whole pane.
    fn column(&self) -> Rect {
        let pane = login_pane();
        let w = match self.stage {
            Stage::Pick => pane.w,
            Stage::QuickConnect => pane.w.min(QC_W),
            _ => pane.w.min(FORM_W),
        };
        Rect::new(pane.x + (pane.w - w) * 0.5, pane.y, w, pane.h)
    }

    /// The step's title: the question it asks.
    fn title_text(&self) -> String {
        match (self.stage, &self.picked) {
            (Stage::Consent, _) => msg::settings_plaintext_question().to_owned(),
            (Stage::Pick, _) => msg::jellyfin_login_title_pick_user().to_owned(),
            (Stage::Credentials, Some(person)) => msg::jellyfin_login_title_sign_in_as(&person.name),
            (Stage::Server, _) => msg::jellyfin_login_title_server().to_owned(),
            (Stage::Credentials, None) if !self.server_name.trim().is_empty() => {
                msg::jellyfin_login_title_sign_in(&self.server_name)
            }
            (Stage::Credentials, None) => msg::jellyfin_login_title().to_owned(),
            (Stage::QuickConnect, _) => msg::jellyfin_login_title_quick_connect().to_owned(),
        }
    }

    /// What the step explains under its title; on Quick Connect, its three instructions.
    fn copy_text(&self) -> String {
        match (self.stage, &self.picked) {
            (Stage::Consent, _) if self.consent.as_ref().is_some_and(|c| c.internet) => {
                msg::jellyfin_login_consent_internet(&self.server_name)
            }
            (Stage::Consent, _) => msg::jellyfin_login_consent_home(&self.server_name),
            (Stage::Pick, _) => msg::jellyfin_login_pick_user_intro(&self.server_name),
            (Stage::Credentials, Some(person)) => msg::jellyfin_login_sign_in_as_intro(&person.name),
            (Stage::Server, _) => msg::jellyfin_login_server_intro().to_owned(),
            (Stage::QuickConnect, _) => [
                msg::jellyfin_login_qc_step_open(),
                msg::jellyfin_login_qc_step_profile(),
                msg::jellyfin_login_qc_step_code(),
            ]
            .join("\n"),
            _ => msg::jellyfin_login_credentials_intro().to_owned(),
        }
    }

    fn title_view<'t>(&self, title: &'t str) -> TextView<'t> {
        let align = if self.stage == Stage::QuickConnect { HAlign::Center } else { HAlign::Left };
        TextView::new(title, TITLE_SZ, theme::TEXT_PRIMARY).bold().leading(TITLE_SZ as f32 * 1.15).h(align)
    }

    fn copy_view<'t>(&self, copy: &'t str) -> TextView<'t> {
        if self.stage == Stage::QuickConnect {
            // Litefin's tertiary ink is legal here: this is the bare page
            TextView::new(copy, QC_COPY_SZ, theme::TEXT_FAINT).leading(QC_COPY_SZ as f32 * 1.5).h(HAlign::Center)
        } else {
            TextView::new(copy, COPY_SZ, theme::TEXT_SECONDARY).leading(COPY_SZ as f32 * 1.45)
        }
    }

    fn form(&self, measure: &dyn Measure) -> Form {
        let col = self.column();
        let (x, w) = (col.x, col.w);
        let (title, copy) = (self.title_text(), self.copy_text());
        let title_h = self.title_view(&title).with_measure(measure).measure_h(w);
        let copy_h = self.copy_view(&copy).with_measure(measure).measure_h(w);
        let mut form = Form {
            title: Rect::new(x, 0.0, w, title_h),
            copy: Rect::new(x, title_h + TITLE_GAP, w, copy_h),
            card: None,
            labels: Vec::new(),
            fields: Vec::new(),
            hint: None,
            code: None,
            status: Rect::new(x, 0.0, w, STATUS_H),
            bullets: Vec::new(),
            or_rule: None,
            recent_caption: None,
            buttons: Vec::new(),
            pitch: PEOPLE_PITCH,
        };
        let mut y = title_h + TITLE_GAP + copy_h
            + if self.stage == Stage::QuickConnect { QC_COPY_GAP } else { COPY_GAP };
        let field = |form: &mut Form, y: &mut f32, field: u32| {
            form.labels.push((field, Rect::new(x, *y, w, LABEL_H)));
            *y += LABEL_H + theme::space::XS;
            form.fields.push((field, Rect::new(x, *y, w, LOGIN_INPUT_H)));
            *y += LOGIN_INPUT_H;
        };
        let button = |form: &mut Form, y: &mut f32, b: u32| {
            form.buttons.push((b, Rect::new(x, *y, w, LOGIN_CTL_H)));
            *y += LOGIN_CTL_H;
        };
        match self.stage {
            Stage::Server => {
                field(&mut form, &mut y, SERVER);
                y += theme::space::XS;
                form.hint = Some(Rect::new(x, y, w, HINT_H));
                y += HINT_H;
                form.status.y = y;
                y += STATUS_H;
                button(&mut form, &mut y, CONNECT);
                if self.recent.is_some() {
                    y += theme::space::LG;
                    form.or_rule = Some(Rect::new(x, y, w, 1.0));
                    y += 1.0 + theme::space::MD;
                    form.recent_caption = Some(Rect::new(x, y, w, LABEL_H));
                    y += LABEL_H + theme::space::SM;
                    form.buttons.push((RECENT, Rect::new(x, y, w, RECENT_H)));
                    y += RECENT_H;
                }
            }
            Stage::Credentials => {
                let card = Rect::new(x, y, w, CARD_H);
                form.card = Some(card);
                if self.offers_change() {
                    let cw = measure.width(&self.label_of(CHANGE), FIELD_SZ, true) + 2.0 * theme::space::MD;
                    form.buttons.push((CHANGE, Rect::new(x + w - cw, card.cy() - CHANGE_H * 0.5, cw, CHANGE_H)));
                }
                y += CARD_H + theme::space::MD;
                form.or_rule = Some(Rect::new(x, y, w, 1.0));
                y += 1.0 + theme::space::MD;
                if self.picked.is_none() {
                    field(&mut form, &mut y, USER);
                    y += theme::space::MD;
                }
                field(&mut form, &mut y, PASS);
                y += theme::space::XS;
                form.status.y = y;
                y += STATUS_H;
                button(&mut form, &mut y, SIGN_IN);
                y += theme::space::MD;
                form.recent_caption = Some(Rect::new(x, y, w, LABEL_H));
                y += LABEL_H + theme::space::MD;
                button(&mut form, &mut y, QUICK);
            }
            Stage::QuickConnect => {
                form.code = Some(Rect::new(x, y, w, CODE_H));
                y += CODE_H + theme::space::SM;
                form.status.y = y;
                y += STATUS_H;
                let pair: &[u32] = if self.offers_change() { &[USE_PASSWORD, CHANGE] } else { &[USE_PASSWORD] };
                let ws: Vec<f32> = pair
                    .iter()
                    .map(|&b| measure.width(&self.label_of(b), FIELD_SZ, true) + 2.0 * theme::space::LG)
                    .collect();
                let total = ws.iter().sum::<f32>() + CONTROL_GAP * (pair.len() - 1) as f32;
                let mut bx = x + ((w - total) * 0.5).max(0.0);
                for (&b, bw) in pair.iter().zip(ws) {
                    form.buttons.push((b, Rect::new(bx, y, bw, LOGIN_CTL_H)));
                    bx += bw + CONTROL_GAP;
                }
                y += LOGIN_CTL_H + theme::space::LG;
                form.card = Some(Rect::new(x, y, w, LABEL_H));
                y += LABEL_H;
            }
            Stage::Consent => {
                form.card = Some(Rect::new(x, y, w, CARD_H));
                y += CARD_H + theme::space::MD;
                form.or_rule = Some(Rect::new(x, y, w, 1.0));
                y += 1.0 + theme::space::MD;
                for _ in 0..3 {
                    form.bullets.push(Rect::new(x, y, w, BULLET_H));
                    y += BULLET_H;
                }
                form.status.y = y;
                y += STATUS_H;
                button(&mut form, &mut y, ALLOW);
                y += theme::space::MD;
                button(&mut form, &mut y, NOT_NOW);
            }
            Stage::Pick => {
                let grid = self.grid();
                let cols = self.cols();
                let rows = grid.len().div_ceil(cols);
                // below `tile_scale` only where the measured title and copy leave the rows, the
                // status band, the caption and Quick Connect less than the safe area's height
                let below = STATUS_H + LABEL_H + theme::space::MD + LOGIN_CTL_H + (rows - 1) as f32 * theme::space::MD;
                let room = (crate::ui::consts::SAFE.h - y - below) / rows as f32;
                // (floored: a server name that wraps the copy for many lines still leaves tiles)
                let k = self.tile_scale().min((room - (LOGIN_PERSON_H - LOGIN_AVATAR)) / LOGIN_AVATAR)
                    .max(PEOPLE_COMPACT * 0.5);
                let pitch = PEOPLE_PITCH * k;
                form.pitch = pitch;
                let x0 = x + ((w - cols.min(grid.len()) as f32 * pitch) * 0.5).max(0.0);
                let (d, row_h) = (LOGIN_AVATAR * k, LOGIN_PERSON_H - LOGIN_AVATAR * (1.0 - k));
                for (i, &elem) in grid.iter().enumerate() {
                    let (row, col) = (i / cols, i % cols);
                    let cy = y + row as f32 * (row_h + theme::space::MD);
                    let cx = x0 + col as f32 * pitch + (pitch - d) * 0.5;
                    form.buttons.push((elem, Rect::new(cx, cy, d, d)));
                }
                y += rows as f32 * row_h + (rows - 1) as f32 * theme::space::MD;
                form.status.y = y;
                y += STATUS_H;
                // the "or" and Quick Connect stand at the form's measure, centred under the list
                let (bx, bw) = (x + (w - w.min(FORM_W)) * 0.5, w.min(FORM_W));
                form.recent_caption = Some(Rect::new(bx, y, bw, LABEL_H));
                y += LABEL_H + theme::space::MD;
                form.buttons.push((QUICK, Rect::new(bx, y, bw, LOGIN_CTL_H)));
                y += LOGIN_CTL_H;
            }
        }
        let safe = crate::ui::consts::SAFE;
        let top = safe.y + ((safe.h - y) * 0.5).max(0.0);
        let mv = |r: &mut Rect| r.y += top;
        mv(&mut form.title);
        mv(&mut form.copy);
        form.card.as_mut().map(mv);
        form.bullets.iter_mut().for_each(mv);
        form.hint.as_mut().map(mv);
        form.code.as_mut().map(mv);
        form.or_rule.as_mut().map(mv);
        form.recent_caption.as_mut().map(mv);
        mv(&mut form.status);
        for (_, r) in form.labels.iter_mut().chain(form.fields.iter_mut()).chain(form.buttons.iter_mut()) {
            mv(r);
        }
        form
    }

    /// The server's list draws its tiles at full size in up to two rows of four, compact beyond
    /// that ([`Self::cols`]); `form` shrinks either further only to fit the safe area.
    fn tile_scale(&self) -> f32 {
        if self.compact() { PEOPLE_COMPACT } else { 1.0 }
    }

    fn compact(&self) -> bool {
        self.grid().len() > 2 * PEOPLE_COLS
    }

    /// Tiles a row of the server's list holds: four, or six compact ones.
    fn cols(&self) -> usize {
        if self.compact() { PEOPLE_COLS_COMPACT } else { PEOPLE_COLS }
    }

    /// A button's words: [`button_label`], except that adding a user, the way back from the
    /// password step is *Not name?*.
    fn label_of(&self, button: u32) -> CString {
        match (button, &self.picked) {
            (CHANGE, Some(person)) => cstring(&msg::jellyfin_login_not_user(&person.name)),
            _ => button_label(button).to_owned(),
        }
    }

    fn elem_rect(&self, elem: u32, measure: &dyn Measure) -> Option<Rect> {
        let form = self.form(measure);
        form.fields
            .iter()
            .chain(form.buttons.iter())
            .find(|(e, _)| *e == elem)
            .map(|(_, r)| *r)
    }

    /// Which field an error is about, for its red rim: the address, or the password.
    fn faulted(&self, field: u32) -> bool {
        self.error.is_some()
            && self.busy.is_none()
            && match self.stage {
                Stage::Server => field == SERVER,
                Stage::Credentials => field == PASS,
                Stage::QuickConnect | Stage::Pick | Stage::Consent => false,
            }
    }

    /// An input as Litefin draws it (`widgets::draw_login_input`), its text and, editing, the caret.
    fn draw_field(&self, p: Painter, field: u32, rect: Rect, focused: bool, focus: f32, measure: &dyn Measure) {
        let editing = self.editing == Some(field);
        let mark = match field {
            SERVER => crate::ui::icons::Icon::Server,
            PASS => crate::ui::icons::Icon::Lock,
            _ => crate::ui::icons::Icon::User,
        };
        let r = crate::ui::widgets::draw_login_input(p, rect, mark, focused, focus);
        if !focused && self.faulted(field) {
            p.rring(r, theme::settings_lf::LOGIN_RAD, 2.0, theme::DANGER);
        }
        let x = r.x + crate::ui::widgets::LOGIN_INPUT_TEXT_X;
        let inner = Rect::new(x, r.y, r.x + r.w - crate::ui::widgets::LOGIN_INPUT_PAD_R - x, r.h);
        let buf = self.buffer(field);
        let (shown, head) = if field == PASS {
            let mask = |s: &str| "\u{2022}".repeat(s.chars().count());
            (mask(buf.text()), mask(&buf.text()[..buf.caret()]))
        } else {
            (buf.text().to_owned(), buf.text()[..buf.caret()].to_owned())
        };
        let ink = if editing { theme::FIELD_EDITING_INK } else { theme::TEXT_PRIMARY };
        let mut caret_x = inner.x;
        if shown.is_empty() {
            if field == SERVER && !editing {
                Label::new(msg::jellyfin_login_server_hint_c().as_ptr(), FIELD_SZ, theme::TEXT_TERTIARY)
                    .draw(p, inner);
            }
        } else {
            let full_w = width(&shown, measure);
            let room = inner.w - if editing { CARET_W + 4.0 } else { 0.0 };
            let visible = tail_fit(&shown, room, measure);
            let dropped = full_w - width(visible, measure);
            let text = cstring(visible);
            Label::new(text.as_ptr(), FIELD_SZ, ink).draw(p, inner);
            caret_x = (inner.x + width(&head, measure) - dropped).clamp(inner.x, inner.x + inner.w - CARET_W);
        }
        if editing {
            let cap = measure.cap_h(FIELD_SZ);
            let h = cap * 1.7;
            p.rrect(Rect::new(caret_x + 2.0, r.y + (r.h - h) * 0.5, CARET_W, h), 1.5, 1.5, theme::BRAND_FILL);
        }
    }

    /// The Quick Connect code, centred in `rect` in the accent, a character at a time so its
    /// characters stand [`CODE_TRACKING`] of an em apart (the text engine has no letter spacing).
    fn draw_code(&self, p: Painter, rect: Rect, measure: &dyn Measure) {
        let Some(qc) = &self.qc else {
            return;
        };
        let glyphs: Vec<CString> = qc.code.chars().map(|c| cstring(&c.to_string())).collect();
        let widths: Vec<f32> = glyphs.iter().map(|g| measure.width(g, CODE_SZ, true)).collect();
        let track = CODE_SZ as f32 * CODE_TRACKING;
        let total = widths.iter().sum::<f32>() + track * glyphs.len().saturating_sub(1) as f32;
        let mut x = rect.cx() - total * 0.5;
        for (glyph, w) in glyphs.iter().zip(widths) {
            Label::new(glyph.as_ptr(), CODE_SZ, theme::BRAND_FILL)
                .bold()
                .v(VAlign::Middle)
                .draw(p, Rect::new(x, rect.y, w + 1.0, rect.h));
            x += w + track;
        }
    }

    /// Quick Connect's status line's alpha: [`PULSE_LOW`]..1 and back over [`PULSE_MS`], on the
    /// same clock that drives the busy spinner (it runs while the server is waited on).
    fn pulse(&self) -> f32 {
        let (_, cos) = nj_machine::motion::sin_cos((self.spin_ms / PULSE_MS) * std::f32::consts::TAU);
        PULSE_LOW + (1.0 - PULSE_LOW) * 0.5 * (1.0 + cos)
    }

    /// Advance the clock the spinner and the pulse read, reporting motion while it runs.
    fn advance_motion(&mut self, t: Tick, present: &mut nj_machine::machine::PresentHandle<'_>) {
        if self.spinning() {
            self.spin_ms = self.spin.advance(t, present);
        }
    }

    fn draw_status(&self, p: Painter, rect: Rect, measure: &dyn Measure) {
        if self.stage == Stage::QuickConnect && self.waiting() && self.busy.is_none() {
            // Litefin's status line: quiet, centred, breathing while the server waits
            TextView::new(msg::jellyfin_login_waiting(), QC_STATUS_SZ, theme::TEXT_FAINT)
                .h(HAlign::Center)
                .max_lines(1)
                .draw(p.alpha(self.pulse()), Rect::new(rect.x, rect.y + theme::space::SM, rect.w, rect.h));
        } else if self.spinning() {
            let line = match self.busy {
                Some(Busy::Connecting) => msg::jellyfin_login_connecting_c(),
                Some(Busy::SigningIn | Busy::Adopting) => msg::jellyfin_login_signing_in_c(),
                Some(Busy::Starting) => msg::jellyfin_login_connecting_c(),
                None => msg::jellyfin_login_waiting_c(),
            };
            let gutter = Spinner::inline_gutter();
            let text_w = measure.width(line, STATUS_SZ, false);
            let centred = self.stage == Stage::QuickConnect;
            let x = if centred { rect.x + ((rect.w - gutter - text_w) * 0.5).max(0.0) } else { rect.x };
            let row = Rect::new(x, rect.y, rect.w - (x - rect.x), rect.h);
            Spinner::leading(row.x, row.cy())
                .phase(self.spin_ms as u32)
                .tint(theme::TEXT_SECONDARY)
                .draw(&Env::inert(), p);
            Label::new(line.as_ptr(), STATUS_SZ, theme::TEXT_SECONDARY)
                .v(VAlign::Middle)
                .draw(p, Rect::new(row.x + gutter, row.y, row.w - gutter, row.h));
        } else if let Some(error) = &self.error {
            // Sized to its sentence and hung from the top of the band, so the band's remainder is
            // always air between the box and the control below it — a focused control's pop
            // included.
            let mark = 28.0;
            let pad = theme::space::SM;
            let mx = rect.x + pad;
            let tx = mx + mark + pad;
            let tw = rect.x + rect.w - pad - tx;
            let view = TextView::new(error, STATUS_SZ, theme::TEXT_PRIMARY)
                .with_measure(measure)
                .max_lines(2);
            let th = view.measure_h(tw);
            let boxed = Rect::new(rect.x, rect.y + theme::space::XS, rect.w, th + 2.0 * pad);
            p.rrect(boxed, theme::settings_lf::LOGIN_RAD, theme::settings_lf::LOGIN_RAD,
                theme::with_a(theme::DANGER, theme::DANGER_IDLE_TINT));
            crate::ui::icons::draw(
                p,
                crate::ui::icons::Icon::Alert,
                Rect::new(mx, boxed.cy() - mark * 0.5, mark, mark),
                theme::DANGER,
            );
            view.draw(p, Rect::new(tx, boxed.y + pad, tw, th));
        }
    }

    /// A disc holding a mark: the server card's and the Recent row's.
    fn disc(p: Painter, center_x: f32, center_y: f32, d: f32, fill: [f32; 4]) -> Rect {
        let r = Rect::new(center_x - d * 0.5, center_y - d * 0.5, d, d);
        p.rrect(r, d * 0.5, d * 0.5, fill);
        r
    }

    /// The person picked from the server's list, as the password step heads its form: their avatar,
    /// their name, the server — and *Not name?* beside them.
    fn draw_person_card(&self, p: Painter, person: &Person, rect: Rect, change_x: f32) {
        let disc = Rect::new(rect.x, rect.cy() - CARD_DISC * 0.5, CARD_DISC, CARD_DISC);
        crate::ui::widgets::avatar_disc(p, disc, &person.name, &person.id, false, theme::size::HEADLINE);
        let tx = disc.x + disc.w + theme::space::MD;
        let tw = (change_x - theme::space::MD - tx).max(0.0);
        TextView::new(&person.name, theme::size::lf::S26, theme::TEXT_PRIMARY)
            .bold()
            .max_lines(1)
            .draw(p, Rect::new(tx, rect.y + 2.0, tw, rect.h * 0.5));
        let server = match &self.origin {
            Some(origin) => format!("{} \u{b7} {}:{}", self.server_name, origin.host(), origin.port()),
            None => self.server_name.clone(),
        };
        TextView::new(&server, theme::size::lf::S19, theme::TEXT_TERTIARY)
            .max_lines(1)
            .draw(p, Rect::new(tx, rect.y + rect.h * 0.5 + 4.0, tw, rect.h * 0.5));
    }

    /// One tile of the server's list (`widgets::draw_login_person`): the person's avatar, or
    /// *Other user*'s mark, their name, and — someone already on this television — "On this TV",
    /// the whole tile quietened.
    fn draw_person(&self, p: Painter, elem: u32, rect: Rect, pitch: f32, focused: bool, scale: f32) {
        use crate::ui::widgets::{draw_login_person, LoginPerson};
        let person = self.person(elem);
        let on_tv = person.is_some_and(|p| p.on_tv.is_some());
        let p = if on_tv && !focused { p.alpha(0.55) } else { p };
        let sub = on_tv.then(msg::jellyfin_login_on_this_tv);
        match person {
            Some(person) => draw_login_person(p, rect, pitch, LoginPerson::Person { name: &person.name, id: &person.id },
                &person.name, sub, focused, scale),
            None => draw_login_person(p, rect, pitch, LoginPerson::Mark(crate::ui::icons::Icon::User),
                msg::jellyfin_login_other_user(), None, focused, scale),
        }
    }

    /// The server card: the server's mark, its name and its address, *Change server* beside them.
    fn draw_card(&self, p: Painter, rect: Rect, change_x: f32) {
        let disc = Self::disc(p, rect.x + CARD_DISC * 0.5, rect.cy(), CARD_DISC, theme::settings_lf::LOGIN_FILL);
        let mark = CARD_DISC * 0.5;
        crate::ui::icons::draw(
            p,
            crate::ui::icons::Icon::Server,
            Rect::new(disc.cx() - mark * 0.5, disc.cy() - mark * 0.5, mark, mark),
            theme::TEXT_PRIMARY,
        );
        let tx = disc.x + disc.w + theme::space::MD;
        let tw = (change_x - theme::space::MD - tx).max(0.0);
        TextView::new(&self.server_name, theme::size::lf::S26, theme::TEXT_PRIMARY)
            .bold()
            .max_lines(1)
            .draw(p, Rect::new(tx, rect.y + 2.0, tw, rect.h * 0.5));
        if let Some(origin) = &self.origin {
            let address = format!("{}:{}", origin.host(), origin.port());
            let line = Rect::new(tx, rect.y + rect.h * 0.5 + 4.0, tw, rect.h * 0.5);
            if self.unencrypted() {
                // *Not encrypted*: amber at home, red for an address on the internet.
                let ink = if self.consent.as_ref().is_some_and(|c| c.internet) { theme::DANGER } else { theme::CAUTION };
                let mark = 22.0;
                crate::ui::icons::draw(p, crate::ui::icons::Icon::LockOpen,
                    Rect::new(line.x, line.y + 2.0, mark, mark), ink);
                let text = format!("{} \u{b7} {address}", msg::jellyfin_login_not_encrypted());
                TextView::new(&text, theme::size::lf::S19, ink)
                    .max_lines(1)
                    .draw(p, Rect::new(line.x + mark + theme::space::XS, line.y, line.w - mark - theme::space::XS, line.h));
            } else {
                TextView::new(&address, theme::size::lf::S19, theme::TEXT_TERTIARY).max_lines(1).draw(p, line);
            }
        }
    }

    /// *Connect without encryption?*'s three lines: what the permission covers, how the server is
    /// checked, and where to take it back.
    fn draw_bullets(&self, p: Painter, rects: &[Rect], measure: &dyn Measure) {
        let lines: [(crate::ui::icons::Icon, &str); 3] = [
            (crate::ui::icons::Icon::Server, msg::jellyfin_login_consent_only_server()),
            (crate::ui::icons::Icon::Check, msg::jellyfin_login_consent_checks()),
            (crate::ui::icons::Icon::Info, msg::jellyfin_login_consent_settings()),
        ];
        for (&rect, (icon, text)) in rects.iter().zip(lines) {
            crate::ui::icons::draw(p, icon, Rect::new(rect.x, rect.y + 4.0, BULLET_MARK, BULLET_MARK), theme::TEXT_SECONDARY);
            let tx = rect.x + BULLET_MARK + theme::space::MD;
            TextView::new(text, COPY_SZ, theme::TEXT_READING)
                .with_measure(measure)
                .max_lines(2)
                .draw(p, Rect::new(tx, rect.y, rect.w - (tx - rect.x), rect.h));
        }
    }

    /// Quick Connect's one-line server reminder, centred under its buttons.
    fn draw_server_line(&self, p: Painter, rect: Rect, measure: &dyn Measure) {
        let line = match &self.origin {
            Some(origin) => format!("{} \u{b7} {}:{}", self.server_name, origin.host(), origin.port()),
            None => self.server_name.clone(),
        };
        let mark = 24.0;
        let gap = theme::space::XS;
        let sz = theme::size::lf::S19;
        let tw = measure.width_str(&line, sz, false).min(rect.w - mark - gap);
        let x = rect.x + ((rect.w - mark - gap - tw) * 0.5).max(0.0);
        crate::ui::icons::draw(
            p,
            crate::ui::icons::Icon::Server,
            Rect::new(x, rect.cy() - mark * 0.5, mark, mark),
            theme::TEXT_FAINT,
        );
        TextView::new(&line, sz, theme::TEXT_FAINT)
            .max_lines(1)
            .draw(p, Rect::new(x + mark + gap, rect.cy() - measure.cap_h(sz) * 0.5, tw + 1.0, rect.h));
    }

    /// The *Recent* row: a secondary control holding the server's mark, name and address.
    fn draw_recent(&self, p: Painter, rect: Rect, focused: bool, focus: f32) {
        use crate::ui::widgets::{draw_login_face, login_popped, LoginKind};
        let Some(recent) = &self.recent else { return };
        let r = login_popped(rect, LoginKind::Secondary, focus);
        draw_login_face(p, r, LoginKind::Secondary, focused);
        let ink = LoginKind::Secondary.ink(focused);
        let disc = Self::disc(p, r.x + theme::space::MD + RECENT_DISC * 0.5, r.cy(), RECENT_DISC, theme::CARD_BG_HOVER);
        let mark = RECENT_DISC * 0.5;
        crate::ui::icons::draw(
            p,
            crate::ui::icons::Icon::Server,
            Rect::new(disc.cx() - mark * 0.5, disc.cy() - mark * 0.5, mark, mark),
            ink,
        );
        let tx = disc.x + disc.w + theme::space::MD;
        let tw = (r.x + r.w - theme::space::MD - tx).max(0.0);
        TextView::new(&recent.name, theme::size::lf::S26, ink)
            .bold()
            .max_lines(1)
            .draw(p, Rect::new(tx, r.y + 16.0, tw, r.h * 0.5));
        TextView::new(&recent.address, theme::size::lf::S19, theme::TEXT_TERTIARY)
            .max_lines(1)
            .draw(p, Rect::new(tx, r.y + r.h * 0.5 + 6.0, tw, r.h * 0.5));
    }

    /// A small upper-case heading in the form ("RECENT").
    fn draw_caption(p: Painter, text: &str, rect: Rect) {
        let up = cstring(&text.to_uppercase());
        Label::new(up.as_ptr(), LABEL_SZ, theme::TEXT_TERTIARY).bold().v(VAlign::Middle).draw(p, rect);
    }

    /// The thin rule between two parts of the form; with a word, the word sits in a gap at its
    /// centre ("or").
    fn draw_rule(p: Painter, rect: Rect, word: Option<&CStr>, measure: &dyn Measure) {
        match word {
            None => p.rrect(Rect::new(rect.x, rect.y, rect.w, 1.0), 0.0, 0.0, theme::HAIRLINE),
            Some(word) => {
                let ww = measure.width(word, LABEL_SZ, false);
                let gap = theme::space::MD;
                let side = ((rect.w - ww) * 0.5 - gap).max(0.0);
                let y = rect.cy();
                p.rrect(Rect::new(rect.x, y, side, 1.0), 0.0, 0.0, theme::HAIRLINE);
                p.rrect(Rect::new(rect.x + rect.w - side, y, side, 1.0), 0.0, 0.0, theme::HAIRLINE);
                Label::new(word.as_ptr(), LABEL_SZ, theme::TEXT_TERTIARY)
                    .h(HAlign::Center)
                    .v(VAlign::Middle)
                    .draw(p, rect);
            }
        }
    }

    fn stop<H: AppLike>(&self, f: &mut DrawFrame<'_, '_, H>, p: Painter, elem: u32, rect: Rect) {
        f.stop(
            p,
            Stop {
                key: self.key(elem),
                rect,
                rest_rect: rect,
                clip: Rect::FULL,
                hover: Hover::Focus,
                activate: Activate::Direct,
            },
        );
    }
}

fn button_label(button: u32) -> &'static CStr {
    match button {
        CONNECT => msg::jellyfin_login_connect_c(),
        SIGN_IN => msg::jellyfin_login_sign_in_c(),
        QUICK => msg::jellyfin_login_use_quick_connect_c(),
        USE_PASSWORD => msg::jellyfin_login_use_password_c(),
        ALLOW => msg::jellyfin_login_consent_allow_c(),
        NOT_NOW => msg::settings_plaintext_not_now_c(),
        _ => msg::jellyfin_login_change_server_c(),
    }
}

fn field_label(field: u32) -> &'static CStr {
    match field {
        SERVER => msg::jellyfin_login_server_label_c(),
        USER => msg::jellyfin_login_username_c(),
        PASS => msg::jellyfin_login_password_c(),
        _ => msg::jellyfin_login_code_label_c(),
    }
}

fn message(error: &AuthError) -> String {
    match error {
        AuthError::Unreachable => msg::jellyfin_login_error_unreachable().to_owned(),
        AuthError::NotJellyfin => msg::jellyfin_login_error_not_jellyfin().to_owned(),
        AuthError::NotSetUp => msg::jellyfin_login_error_not_set_up().to_owned(),
        AuthError::BadCredentials => msg::jellyfin_login_error_credentials().to_owned(),
        AuthError::QuickConnectDisabled => msg::jellyfin_login_error_quick_connect_off().to_owned(),
        AuthError::Refused(status) => msg::jellyfin_login_error_refused(*status as i64),
        AuthError::Malformed => msg::jellyfin_login_error_malformed().to_owned(),
        AuthError::NeedsConsent => msg::jellyfin_login_error_needs_consent().to_owned(),
    }
}

fn cstring(s: &str) -> CString {
    CString::new(s).unwrap_or_default()
}

fn width(s: &str, measure: &dyn Measure) -> f32 {
    measure.width(&cstring(s), FIELD_SZ, false)
}

/// The longest tail of `s` that fits `room`: a field too narrow for its text shows the end being
/// typed, not the beginning.
fn tail_fit<'a>(s: &'a str, room: f32, measure: &dyn Measure) -> &'a str {
    if width(s, measure) <= room {
        return s;
    }
    s.char_indices()
        .map(|(i, _)| &s[i..])
        .find(|tail| width(tail, measure) <= room)
        .unwrap_or("")
}

impl<H: AppLike> Focusable<H> for JfLoginScreen {
    fn groups(&self, cx: &Cx<'_, H>, out: &mut Vec<GroupSpec>) {
        let form = self.form(cx.measure);
        let extent = form
            .fields
            .iter()
            .chain(form.buttons.iter())
            .map(|(_, r)| *r)
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::FULL);
        out.push(GroupSpec {
            id: GROUP,
            kind: GroupKind::Free,
            seat: Seat::First,
            reachable: AxisMask::BOTH,
            edge: [EdgeRule::Stop; 4],
            extent,
            len: self.elems().count(),
            elem: ElemKind::Bare,
        });
    }
    fn group_of(&self, key: &u32, _cx: &Cx<'_, H>) -> Option<GroupId> {
        self.elems().any(|e| e == *key).then_some(GROUP)
    }
    /// The panel is a column: UP/DOWN walk [`JfLoginScreen::order`]. Quick Connect's two buttons
    /// are a row: LEFT/RIGHT walk them. Everything else is an edge.
    fn neighbour(&self, key: FocusKey<u32>, dir: Dir, _cx: &Cx<'_, H>) -> Step<u32> {
        if self.stage == Stage::Pick {
            return self.grid_step(key.elem, dir).map_or(Step::Edge, |elem| Step::Move(self.key(elem)));
        }
        let order = self.order();
        let Some(i) = order.iter().position(|&e| e == key.elem) else { return Step::Edge };
        let (back, ahead) = if self.is_row() { (Dir::Left, Dir::Right) } else { (Dir::Up, Dir::Down) };
        let next = if dir == back {
            i.checked_sub(1).map(|j| order[j])
        } else if dir == ahead {
            order.get(i + 1).copied()
        } else {
            None
        };
        next.map_or(Step::Edge, |elem| Step::Move(self.key(elem)))
    }
    fn place(&self, key: &u32, cx: &Cx<'_, H>, _at: At) -> Option<Placed> {
        let index = self.elems().position(|e| e == *key)?;
        let rect = self.elem_rect(*key, cx.measure)?;
        Some(Placed { rect, rest_rect: rect, clip: Rect::FULL, index: Some(index as u32) })
    }
    fn reconcile(&self, want: FocusKey<u32>, cx: &Cx<'_, H>) -> FocusKey<u32> {
        if self.group_of(&want.elem, cx).is_some() {
            want
        } else {
            self.key(self.first())
        }
    }
    fn seat(&self, _g: GroupId, _from: Placed, _cx: &Cx<'_, H>) -> FocusKey<u32> {
        self.key(self.first())
    }
}

impl<H: AppLike> Machine<H> for JfLoginScreen {
    type Ev = ScreenEvent<H>;
    fn step(&mut self, ev: &Self::Ev, cx: &Cx<'_, H>, fx: &mut Effects<'_, H>) -> Handled {
        match ev {
            ScreenEvent::Mount => {
                self.reseat(self.first(), fx);
                if self.stage == Stage::Pick && self.people.is_none() {
                    self.load_people(fx);
                }
            }
            ScreenEvent::Unmount => {
                self.close(fx);
                self.rx = None;
            }
            ScreenEvent::WillLeave(_) | ScreenEvent::Cover | ScreenEvent::Suspend => self.close(fx),
            ScreenEvent::Tick(t) => self.tick(*t, cx, fx),
            ScreenEvent::Activate(elem) => self.activate(*elem, fx),
            ScreenEvent::FocusMoved { to, .. } => {
                if self.editing.is_some_and(|field| field != to.elem) {
                    self.close(fx);
                }
                self.pop_until_ms = self.now_ms.wrapping_add(POP_MS);
                fx.invalidate(Provenance::Input);
            }
            ScreenEvent::Input(input) => match &input.kind {
                InputKind::SystemKeyboard(up) => {
                    if !*up {
                        self.editing = None;
                    } else if self.editing.is_none() {
                        let focused = cx.focus.current.map(|k| k.elem).filter(|e| self.fields().contains(e));
                        self.editing = focused;
                    }
                    fx.invalidate(Provenance::Input);
                }
                InputKind::Text(edit) if self.editing.is_some() || matches!(cx.owner, InputOwner::System(_)) => {
                    let field = self
                        .editing
                        .or_else(|| cx.focus.current.map(|k| k.elem).filter(|e| self.fields().contains(e)));
                    if let Some(field) = field {
                        self.edit(field, edit, fx);
                    }
                }
                InputKind::Key { key, sym, edge, .. } if *edge != Edge::Up => {
                    if let Some(field) = self.editing {
                        let edit = match (*key, *sym) {
                            (Key::Left, _) => Some(TextEdit::Left),
                            (Key::Right, _) => Some(TextEdit::Right),
                            (_, crate::ui::consts::SDLK_BACKSPACE) => Some(TextEdit::Backspace),
                            (_, crate::ui::consts::SDLK_CLEAR) => Some(TextEdit::Clear),
                            _ => None,
                        };
                        if let Some(edit) = edit {
                            self.edit(field, &edit, fx);
                            return Handled::Yes;
                        }
                        match key {
                            Key::Back => {
                                self.close(fx);
                                return Handled::Yes;
                            }
                            Key::Ok => {
                                self.submit(field, fx);
                                return Handled::Yes;
                            }
                            Key::Up | Key::Down => {
                                self.close(fx);
                                return Handled::No;
                            }
                            _ => {}
                        }
                    }
                    if *key == Key::Back {
                        self.back(fx);
                        return Handled::Yes;
                    }
                    return Handled::No;
                }
                _ => return Handled::No,
            },
            _ => return Handled::No,
        }
        Handled::Yes
    }
}

impl<H: AppLike> Screen<H> for JfLoginScreen {
    fn name(&self) -> &'static str {
        word::LOGIN
    }
    fn state(&self) -> &dyn LogicalState {
        self
    }
    fn crumb(&self, _cx: &Cx<'_, H>) -> Option<std::borrow::Cow<'_, str>> {
        None
    }
    fn prepare(&mut self, _b: &mut Budget, _cx: &Cx<'_, H>) {}
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, H>) {
        use crate::ui::widgets::{draw_login_button, pop_progress, LoginKind, LOGIN_AVATAR_POP};
        let p = f.painter;
        // Litefin's sign-in stands on the flat black page (`docs/litefin-look.md` 2.1, 8)
        Painter::root().rect(Rect::FULL, 0.0, theme::SURFACE_APP, theme::SURFACE_APP, 0.0);
        crate::ui::widgets::draw_login_brand(p, f.measure);

        let form = self.form(f.measure);
        let focus = f.focus.current.filter(|k| k.entry == self.entry).map(|k| k.elem);
        let (title, copy) = (self.title_text(), self.copy_text());
        self.title_view(&title).with_measure(f.measure).draw(p, form.title);
        self.copy_view(&copy).with_measure(f.measure).draw(p, form.copy);
        if let Some(rect) = form.card {
            let change_x = form.buttons.iter().find(|(b, _)| *b == CHANGE).map_or(rect.x + rect.w, |(_, r)| r.x);
            match (self.stage, &self.picked) {
                (Stage::QuickConnect | Stage::Pick, _) => self.draw_server_line(p, rect, f.measure),
                (_, Some(person)) => self.draw_person_card(p, person, rect, change_x),
                _ => self.draw_card(p, rect, change_x),
            }
        }
        self.draw_bullets(p, &form.bullets, f.measure);
        for &(field, rect) in &form.labels {
            Label::new(field_label(field).as_ptr(), LABEL_SZ, theme::TEXT_SECONDARY)
                .bold()
                .v(VAlign::Middle)
                .draw(p, rect);
        }
        for &(field, rect) in &form.fields {
            let focused = focus == Some(field);
            self.draw_field(p, field, rect, focused, if focused { 1.0 } else { 0.0 }, f.measure);
        }
        if let Some(rect) = form.hint {
            TextView::new(msg::jellyfin_login_port_hint(), LABEL_SZ, theme::TEXT_FAINT)
                .max_lines(1)
                .draw(p, Rect::new(rect.x, rect.y + 2.0, rect.w, rect.h));
        }
        if let Some(rect) = form.code {
            self.draw_code(p, rect, f.measure);
        }
        self.draw_status(p, form.status, f.measure);
        if let Some(rect) = form.or_rule {
            Self::draw_rule(p, rect, None, f.measure);
        }
        if let Some(rect) = form.recent_caption {
            match self.stage {
                Stage::Credentials | Stage::Pick => Self::draw_rule(p, rect, Some(msg::jellyfin_login_or_c()), f.measure),
                _ => Self::draw_caption(p, msg::jellyfin_login_recent(), rect),
            }
        }
        let buttons = self.buttons();
        for &(button, rect) in &form.buttons {
            let focused = focus == Some(button);
            // the shared pop's progress, or a plain on/off for a control outside its slots
            let progress = buttons
                .iter()
                .position(|&b| b == button)
                .map_or(if focused { 1.0 } else { 0.0 }, |i| pop_progress(&self.pop, i, f.press.scale));
            if button == RECENT {
                self.draw_recent(p, rect, focused, progress);
                continue;
            }
            if button == OTHER || self.person(button).is_some() {
                let scale = 1.0 + (LOGIN_AVATAR_POP - 1.0) * progress;
                self.draw_person(p, button, rect, form.pitch, focused, scale);
                continue;
            }
            let kind = match button {
                QUICK | CONNECT | ALLOW => LoginKind::Primary,
                _ => LoginKind::Secondary,
            };
            let mark = (button == QUICK).then_some(crate::ui::icons::Icon::Phone);
            draw_login_button(p, rect, &self.label_of(button), kind, focused, progress, mark, f.measure);
        }
        if f.records_stops() {
            for &(elem, rect) in form.fields.iter().chain(form.buttons.iter()) {
                self.stop(f, p, elem, rect);
            }
        }
    }
    fn render(&self) -> RenderStrategy {
        RenderStrategy::Page
    }
    fn focus_source(&self) -> FocusSource {
        FocusSource::Engine
    }
    fn hit_source(&self) -> HitSource {
        HitSource::Engine
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

impl LogicalState for JfLoginScreen {
    fn write(&self, c: &mut Canon) {
        c.u32(self.entry.0).u32(self.instance.0).u32(match self.stage {
            Stage::Server => 0,
            Stage::Credentials => 1,
            Stage::QuickConnect => 2,
            Stage::Pick => 3,
            Stage::Consent => 4,
        });
        c.u32(self.server.text().len() as u32)
            .u32(self.user.text().len() as u32)
            .u32(self.pass.text().len() as u32);
        c.option(self.editing, |c, field| {
            c.u32(field);
        });
        c.option(self.busy, |c, busy| {
            c.u32(busy as u32);
        });
        c.bool(self.waiting()).bool(self.error.is_some()).bool(self.origin.is_some());
        c.bool(self.adding).u32(self.shown().len() as u32).bool(self.picked.is_some());
        c.option(self.consent.as_ref().map(|c| c.internet), |c, internet| {
            c.bool(internet);
        });
    }
    fn probe(&self, out: &mut String) {
        out.push_str(&format!(
            "jf-login stage={:?} editing={:?} busy={:?} waiting={} error={}",
            self.stage,
            self.editing,
            self.busy,
            self.waiting(),
            self.error.is_some()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_stage_walks_its_own_elements_and_nothing_else() {
        let mut s = JfLoginScreen::new(EntryId(0), InstanceId(0));
        s.recent = None;
        assert_eq!(s.elems().collect::<Vec<_>>(), [SERVER, CONNECT]);
        assert_eq!(s.first(), SERVER);
        s.recent = Some(Recent { name: "Living Room".into(), address: "192.168.1.20:8096".into() });
        assert_eq!(s.elems().collect::<Vec<_>>(), [SERVER, CONNECT, RECENT]);
        assert_eq!(s.first(), RECENT, "a saved server is one press from reconnecting");
        s.stage = Stage::Credentials;
        assert_eq!(s.elems().collect::<Vec<_>>(), [CHANGE, USER, PASS, SIGN_IN, QUICK]);
        assert_eq!(s.first(), USER);
        s.stage = Stage::QuickConnect;
        assert_eq!(s.elems().collect::<Vec<_>>(), [USE_PASSWORD, CHANGE]);
        assert_eq!(s.first(), USE_PASSWORD);
    }

    /// After a sign-out the sign-in screen still offers the server signed in to last as *Recent*,
    /// and seats focus on it; Delete all local data leaves nothing to offer.
    #[test]
    fn a_signed_out_television_offers_its_last_server_as_recent() {
        use crate::jf::auth::SignedIn;
        use crate::jf::store::{self, Stored};
        let _g = nj_base::testlock::serial();
        let origin = crate::catalog::Origin::parse("http://192.168.1.20:8096").unwrap();
        let signed_in = SignedIn {
            token: "tok".into(),
            user_id: "u1".into(),
            user_name: "Sam".into(),
            server_id: "AB-CD".into(),
            server_name: "Living Room".into(),
            server_version: "10.11.0".into(),
            device_user: "sam".into(),
        };
        store::set_live(Some(Stored::new(&origin, &signed_in)));
        store::set_live(None);
        let s = JfLoginScreen::new(EntryId(0), InstanceId(0));
        assert_eq!(s.recent, Some(Recent { name: "Living Room".into(), address: "192.168.1.20:8096".into() }));
        assert_eq!(s.first(), RECENT);
        store::forget_everything();
        assert_eq!(JfLoginScreen::new(EntryId(0), InstanceId(0)).recent, None);
    }

    #[test]
    fn the_logical_state_carries_no_credential_text() {
        let mut s = JfLoginScreen::new(EntryId(0), InstanceId(0));
        s.pass = TextBuffer::new("hunter2".into(), 7);
        s.user = TextBuffer::new("someone".into(), 7);
        let mut probe = String::new();
        s.probe(&mut probe);
        assert!(!probe.contains("hunter2") && !probe.contains("someone"));
        let a = s.hash();
        s.pass = TextBuffer::new("hunter3".into(), 7);
        assert_eq!(a, s.hash(), "same length, same state: the password itself is never hashed");
    }

    /// The status band is tall enough for the error box at its largest (two lines of
    /// [`STATUS_SZ`], padded) with air left before the control under it.
    #[test]
    fn the_status_band_holds_a_two_line_error_with_air_to_spare() {
        let two_lines = 2.0 * (STATUS_SZ as f32 + theme::space::XS);
        let boxed = theme::space::XS + two_lines + 2.0 * theme::space::SM;
        assert!(boxed + theme::space::SM <= STATUS_H, "{boxed} in {STATUS_H}");
    }

    /// The two marks the panel adds rasterize at the sizes it draws them, full ink inside and none
    /// on the border.
    #[test]
    fn the_server_and_phone_marks_rasterize_clean() {
        use crate::ui::icons::Icon;
        for (id, svg) in [
            (Icon::Server, include_str!("../../../assets/icons/server.svg")),
            (Icon::Phone, include_str!("../../../assets/icons/phone.svg")),
            (Icon::Plus, include_str!("../../../assets/icons/plus.svg")),
            (Icon::Lock, include_str!("../../../assets/icons/lock.svg")),
            (Icon::BrandMark, include_str!("../../../assets/icons/brand-mark.svg")),
        ] {
            for px in [26, 28, 32, 70] {
                let rgba = nj_gfx::svg::rasterize(svg, px, px).unwrap_or_else(|| panic!("{id:?} at {px}px"));
                let alpha = |x: i32, y: i32| rgba[((y * px + x) * 4 + 3) as usize];
                let max = (0..px).flat_map(|y| (0..px).map(move |x| alpha(x, y))).max().unwrap();
                assert_eq!(max, 255, "{id:?} never reaches full ink at {px}px");
                for i in 0..px {
                    for (x, y) in [(i, 0), (i, px - 1), (0, i), (px - 1, i)] {
                        assert_eq!(alpha(x, y), 0, "{id:?} inks the border at {px}px ({x},{y})");
                    }
                }
            }
        }
    }

    /// **The status clock runs only while something is waited on** (the two animation tests
    /// `ui/CLAUDE.md` asks for): a tick reports motion while the screen is busy or Quick Connect
    /// waits for approval, and nothing at rest; Quick Connect's status line reads its pulse off
    /// that clock, full at the start of each 2.5 s breath and at half at its middle.
    #[test]
    fn the_status_clock_reports_while_waiting_and_is_silent_at_rest() {
        let tick = |ms| Tick { ms, dt_us: 16_000 };
        let reported = |s: &mut JfLoginScreen, ms| {
            let mut present = nj_machine::present::Present::new();
            present.take(0);
            let mut out = Vec::new();
            let mut fx = Effects::<TestHost>::new(&mut out, MachineId::Session, &mut present);
            s.advance_motion(tick(ms), &mut fx.present());
            drop(fx);
            present.changed()
        };
        let mut s = JfLoginScreen::new(EntryId(0), InstanceId(0));
        assert!(!reported(&mut s, 100), "at rest the clock is still and reports nothing");
        assert_eq!(s.spin_ms, 0.0);
        s.busy = Some(Busy::Starting);
        assert!(reported(&mut s, 200), "while waited on, every tick reports");
        assert!(reported(&mut s, 1_450));
        assert!((s.spin_ms - 1_250.0).abs() < 0.5, "the clock is the frame clock");
        assert!((s.pulse() - PULSE_LOW).abs() < 1e-3, "half a breath in, the line is at its low");
        s.spin_ms = 0.0;
        assert!((s.pulse() - 1.0).abs() < 1e-3);
        s.spin_ms = PULSE_MS;
        assert!((s.pulse() - 1.0).abs() < 1e-3, "and full again once a breath is over");
        s.busy = None;
        assert!(!reported(&mut s, 3_000), "and silent again once nothing is waited on");
    }

    /// The stages walk as drawn: a column, except Quick Connect's pair, which is a row.
    #[test]
    fn focus_walks_the_panel_as_it_is_drawn() {
        let mut s = JfLoginScreen::new(EntryId(1), InstanceId(0));
        s.recent = None;
        s.stage = Stage::Credentials;
        let order = s.order();
        assert_eq!(order, [CHANGE, USER, PASS, SIGN_IN, QUICK]);
        assert!(!s.is_row());
        s.stage = Stage::QuickConnect;
        assert!(s.is_row());
    }

    /// The smallest host the screen's effects can be stepped on.
    struct TestHost;
    impl nj_machine::machine::Host for TestHost {
        type Arg = super::super::family::SettingsPage;
        type Fx = AppFx;
        type Msg = crate::screens::registry::AppMsg;
        type Elem = u32;
        type Views<'a> = crate::auth::SessionRead<'a>;
        type Init = super::super::family::NoInit;
        type Memory = ();
    }

    /// What one step asked the app for: the loop requests, and the sign-in commands by name and
    /// username (never a password).
    fn effects(s: &mut JfLoginScreen, f: impl FnOnce(&mut JfLoginScreen, &mut Effects<'_, TestHost>)) -> (Vec<LoopReq>, Vec<String>) {
        let mut out = Vec::new();
        let mut present = nj_machine::present::Present::new();
        let mut fx = Effects::new(&mut out, MachineId::Session, &mut present);
        f(s, &mut fx);
        drop(fx);
        let (mut loops, mut auth) = (Vec::new(), Vec::new());
        for e in out {
            match e.fx {
                Fx::App(AppFx::Loop(req)) => loops.push(req),
                Fx::App(AppFx::JfAuth(JfAuthCmd::Password { username, .. })) => auth.push(format!("password:{username}")),
                Fx::App(AppFx::JfAuth(JfAuthCmd::PublicUsers { .. })) => auth.push("people".into()),
                Fx::App(AppFx::JfAuth(JfAuthCmd::QuickConnectStart { .. })) => auth.push("quick-connect".into()),
                Fx::App(AppFx::JfAuth(JfAuthCmd::AllowPlaintext { server_id, .. })) => auth.push(format!("allow:{server_id}")),
                _ => {}
            }
        }
        (loops, auth)
    }

    fn adding(people: Vec<Person>) -> JfLoginScreen {
        let mut s = JfLoginScreen::new(EntryId(1), InstanceId(0));
        s.recent = None;
        s.adding = true;
        s.origin = Origin::parse("http://192.168.1.20:8096");
        s.server_name = "Living Room".into();
        s.stage = Stage::Pick;
        s.people = Some(people);
        s
    }

    fn person(name: &str, has_password: bool, on_tv: Option<u8>) -> Person {
        Person { id: format!("id-{name}"), name: name.into(), has_password, on_tv }
    }

    /// *Add a user* opens on the server's list: its people, *Other user* and Quick Connect, walked
    /// as a grid. Someone already on this TV is switched to; someone with a password is asked for
    /// only that, beside *Not name?*; someone without one signs straight in.
    #[test]
    fn adding_a_user_picks_from_the_servers_list() {
        let people = vec![person("Alex", true, Some(0)), person("Sam", true, None), person("Kids", false, None)];
        let mut s = adding(people);
        assert_eq!(s.order(), [PERSON, PERSON + 1, PERSON + 2, OTHER, QUICK]);
        assert_eq!(s.first(), PERSON + 1, "focus starts on the first person not on this TV");
        assert_eq!(s.grid_step(PERSON, Dir::Right), Some(PERSON + 1));
        assert_eq!(s.grid_step(PERSON + 1, Dir::Down), Some(QUICK), "one row, then Quick Connect");
        assert_eq!(s.grid_step(QUICK, Dir::Up), Some(OTHER));
        assert_eq!(s.grid_step(OTHER, Dir::Right), None);

        let (loops, auth) = effects(&mut s, |s, fx| s.activate(PERSON, fx));
        assert_eq!((loops, auth), (vec![LoopReq::PickJellyfinUser(0)], vec![]), "Alex is on this TV already");
        assert_eq!(s.stage, Stage::Pick);

        let (loops, auth) = effects(&mut s, |s, fx| s.activate(PERSON + 1, fx));
        assert!(loops.is_empty() && auth.is_empty(), "{loops:?} {auth:?}");
        assert_eq!(s.stage, Stage::Credentials);
        assert_eq!(s.order(), [CHANGE, PASS, SIGN_IN, QUICK], "only the password is asked");
        assert_eq!(s.first(), PASS);
        assert_eq!(s.editing, Some(PASS), "the keyboard is up for it");
        assert_eq!(s.label_of(CHANGE).to_str().unwrap(), "Not Sam?");
        assert_eq!(s.user.text(), "Sam");

        let (loops, _) = effects(&mut s, |s, fx| s.back(fx));
        assert!(loops.is_empty());
        assert_eq!((s.stage, s.picked.is_none(), s.user.text()), (Stage::Pick, true, ""), "BACK is the list again");

        let (_, auth) = effects(&mut s, |s, fx| s.activate(PERSON + 2, fx));
        assert_eq!(auth, ["password:Kids"], "no password: straight in");
        assert_eq!(s.busy, Some(Busy::SigningIn));
    }

    /// **The server's list fits the safe area however long it is**: every tile with its name and
    /// quiet line, the status band, the caption and Quick Connect, in every shipped language, and
    /// focus walks it as the rows are drawn.
    #[test]
    fn the_pick_list_fits_the_safe_area_at_any_length() {
        use nj_base::fontcov::advances::ShippedMeasure;
        use crate::ui::consts::inside_safe;
        for language in nj_platform::i18n::SHIPPED {
            let _guard = nj_platform::i18n::language_on_this_thread_for_test(language);
            for n in [1, 3, 4, 7, 8, 9, MAX_PEOPLE, MAX_PEOPLE + 4] {
                let s = adding((0..n).map(|i| person(&format!("user{i}"), true, None)).collect());
                let form = s.form(&ShippedMeasure);
                let tiles = form.buttons.iter().filter(|(e, _)| *e != QUICK)
                    .map(|(_, r)| Rect::new(r.x, r.y, r.w, r.h + LOGIN_PERSON_H - LOGIN_AVATAR));
                let quick = form.buttons.iter().find(|(e, _)| *e == QUICK).map(|(_, r)| *r);
                for r in [form.title, form.copy, form.status].into_iter().chain(form.recent_caption).chain(quick).chain(tiles) {
                    assert!(inside_safe(r), "{} with {n} people: {r:?} leaves the safe area", language.tag());
                }
                let grid = s.grid();
                let last = *grid.last().expect("Other user");
                assert_eq!(s.grid_step(last, Dir::Down), Some(QUICK));
                for (i, &e) in grid.iter().enumerate().skip(1) {
                    let (a, b) = (s.elem_rect(grid[i - 1], &ShippedMeasure).unwrap(), s.elem_rect(e, &ShippedMeasure).unwrap());
                    if (a.y - b.y).abs() < 0.5 {
                        assert_eq!(s.grid_step(grid[i - 1], Dir::Right), Some(e), "RIGHT walks the drawn row");
                    } else {
                        assert!(b.y > a.y && s.grid_step(grid[i - 1], Dir::Right).is_none(), "a drawn row ends RIGHT");
                    }
                }
            }
        }
    }

    /// *Other user* types a username on the same server — no *Change server*: adding a user never
    /// changes server — and BACK from the list leaves for the who's-watching screen.
    #[test]
    fn other_user_types_a_name_and_back_leaves_the_flow() {
        let _g = nj_base::testlock::serial();
        crate::jf::store::forget_everything();
        let mut s = adding(vec![person("Sam", true, None)]);
        effects(&mut s, |s, fx| s.activate(OTHER, fx));
        assert_eq!(s.stage, Stage::Credentials);
        assert_eq!(s.order(), [USER, PASS, SIGN_IN, QUICK]);
        effects(&mut s, |s, fx| s.start_quick_connect(fx));
        assert_eq!(s.order(), [USE_PASSWORD], "Quick Connect offers no server change either");
        effects(&mut s, |s, fx| s.back(fx));
        effects(&mut s, |s, fx| s.back(fx));
        assert_eq!(s.stage, Stage::Pick);
        let (loops, _) = effects(&mut s, |s, fx| s.back(fx));
        assert_eq!(loops, [LoopReq::AccountSwitchUser], "nobody signed in underneath");
    }

    /// A server whose administrator hides its user list answers with nobody: the flow asks for a
    /// username instead, and BACK from there leaves rather than showing an empty list.
    #[test]
    fn a_server_that_lists_nobody_asks_for_a_username() {
        let _g = nj_base::testlock::serial();
        crate::jf::store::forget_everything();
        let mut s = adding(Vec::new());
        s.people = None;
        let (tx, rx) = mpsc::channel();
        s.rx = Some(rx);
        tx.send(JfAuthReply::People(Ok(Vec::new()))).unwrap();
        effects(&mut s, |s, fx| s.drain(fx));
        assert_eq!(s.stage, Stage::Credentials);
        assert_eq!(s.order(), [USER, PASS, SIGN_IN, QUICK]);
        let (loops, _) = effects(&mut s, |s, fx| s.back(fx));
        assert_eq!(loops, [LoopReq::AccountSwitchUser]);
    }

    /// A server that answers only over plain http, not yet allowed, is asked about between the
    /// address and the sign-in: *Connect without encryption* records the answer for that server and
    /// goes on to the sign-in once the app has let it through; *Not now* and BACK return to the
    /// address. The server card says *Not encrypted* from then on.
    #[test]
    fn a_plaintext_server_is_asked_about_once_before_the_sign_in() {
        let mut s = JfLoginScreen::new(EntryId(1), InstanceId(0));
        s.recent = None;
        let deliver = |s: &mut JfLoginScreen, reply: JfAuthReply| {
            let (tx, rx) = mpsc::channel();
            s.rx = Some(rx);
            tx.send(reply).unwrap();
            effects(s, |s, fx| s.drain(fx))
        };
        let origin = Origin::parse("http://192.168.1.20:8096").unwrap();
        let info = crate::jf::models::PublicSystemInfo { server_name: "Living Room".into(), id: "AB-CD".into(), ..Default::default() };
        deliver(&mut s, JfAuthReply::Consent { origin: origin.clone(), info: info.clone(), internet: false });
        assert_eq!(s.stage, Stage::Consent);
        assert_eq!(s.order(), [ALLOW, NOT_NOW]);
        assert_eq!(s.first(), ALLOW);
        assert!(s.unencrypted());
        assert_eq!(s.consent, Some(Consent { server_id: "abcd".into(), internet: false }));

        let (_, auth) = effects(&mut s, |s, fx| s.activate(ALLOW, fx));
        assert_eq!(auth, ["allow:abcd"], "the answer is about this server");
        assert_eq!(s.busy, Some(Busy::Connecting));
        deliver(&mut s, JfAuthReply::Allowed(Ok(())));
        assert_eq!(s.stage, Stage::Credentials);
        assert_eq!(s.origin, Some(origin.clone()), "on to the sign-in at the same address");
        assert!(s.unencrypted());

        deliver(&mut s, JfAuthReply::Consent { origin: origin.clone(), info: info.clone(), internet: true });
        effects(&mut s, |s, fx| s.activate(NOT_NOW, fx));
        assert_eq!((s.stage, s.origin.is_none(), s.consent.is_none()), (Stage::Server, true, true));
        deliver(&mut s, JfAuthReply::Consent { origin, info, internet: true });
        effects(&mut s, |s, fx| s.back(fx));
        assert_eq!(s.stage, Stage::Server);
        assert!(!message(&AuthError::NeedsConsent).is_empty());
    }

    #[test]
    fn every_error_has_a_sentence() {
        for e in [
            AuthError::Unreachable,
            AuthError::NotJellyfin,
            AuthError::NotSetUp,
            AuthError::BadCredentials,
            AuthError::QuickConnectDisabled,
            AuthError::Refused(503),
            AuthError::Malformed,
            AuthError::NeedsConsent,
        ] {
            assert!(!message(&e).is_empty());
        }
        assert!(message(&AuthError::Refused(503)).contains("503"));
    }
}
