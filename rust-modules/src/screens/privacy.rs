//! **Privacy & data**, a page of the Settings surface: what JellyRust keeps on this television and
//! the one control over it, Delete all local data, behind a confirming alert. JellyRust has no
//! reporting of its own, so there is nothing else to choose here.

use std::borrow::Cow;

use crate::ui::decision_alert::{Choice as AlertChoice, DecisionAlert};
use crate::ui::form::{Form, FormId, FormSection, FormTable, RowKey, RowKeys, RowKind};
use crate::ui::frame::Budget;
use nj_machine::machine::{
    Canon, Cx, Delivery, Effects, EntryId, FocusKey, Fx, GroupId, Handled, InputEvent, InputKind,
    Key, LogicalState, Machine, MachineId,
};
use crate::ui::route_screen::RouteLayout;
use crate::ui::screen::{
    Activate, At, AxisMask, Dir, DrawFrame, EdgeRule, ElemKind, Enter, FocusSource, FocusTarget,
    Focusable, GroupKind, GroupSpec, HitSource, Hover, Part, Placed, RenderStrategy, Screen,
    ScreenEvent, Seat, Step, Stop,
};
use crate::ui::table::{Row, TableView};
use crate::ui::table_screen::{Header, TableScreen};
use crate::ui::{theme, Rect};

use super::family::{form_activate, form_focus, form_right_target, InnerHost, SettingsPage, ALERT_GROUP, TABLE_GROUP};
use super::registry::{alert_index, word, AppFx, LoopReq, ALERT};

/// The page's one row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RowId {
    Delete,
}

impl FormId for RowId {
    fn key(&self) -> RowKey {
        RowKey(match self {
            RowId::Delete => 0,
        })
    }
}

/// What the row does: ask "Delete all local data?" (the alert).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Act {
    Delete,
}

pub(crate) struct PrivacyPage {
    entry: EntryId,
    form: FormTable<RowId, Act, SettingsPage>,
    alert: DecisionAlert,
    /// The alert's two answers as drawn last (the map is the last DRAWN frame's, §7.6).
    alert_frames: std::cell::Cell<Option<(Rect, Rect)>>,
    layout: RouteLayout,
    state: PrivacyState,
}

struct PrivacyState {
    alert: bool,
    alert_scroll: u32,
}

impl LogicalState for PrivacyState {
    fn write(&self, w: &mut Canon) {
        w.bool(self.alert).u32(self.alert_scroll);
    }
    fn probe(&self, out: &mut String) {
        out.push_str(&format!("privacy alert={} alert_scroll={}", self.alert, self.alert_scroll));
    }
}

fn privacy_form() -> Form<RowId, Act, SettingsPage> {
    Form::new().section(
        FormSection::new(nj_platform::i18n::msg::settings_consent_on_tv()).item(
            RowId::Delete, RowKind::Button, Act::Delete,
            Row::new(nj_platform::i18n::msg::settings_consent_delete()).detail(nj_platform::i18n::msg::settings_consent_delete_detail()).chevron(true),
        ),
    )
}

impl PrivacyPage {
    pub(crate) fn new(entry: EntryId, _cx: &Cx<'_, InnerHost>, _fx: &mut Effects<'_, InnerHost>) -> Self {
        let mut s = Self {
            entry,
            form: FormTable::new(super::registry::BAND),
            alert: DecisionAlert::new(),
            alert_frames: std::cell::Cell::new(None),
            // a Settings page: Litefin's content panel
            layout: RouteLayout::panel(),
            state: PrivacyState { alert: false, alert_scroll: 0 },
        };
        super::family::style_page_table(&mut s.form.table);
        s.form.table.header_ink = theme::TEXT_READING;
        s.form.set(privacy_form(), None);
        s.form.table.list_focused = true;
        s
    }

    fn title(&self) -> &'static str {
        nj_platform::i18n::msg::settings_privacy_title()
    }
    fn body(&self) -> &'static str {
        nj_platform::i18n::msg::settings_consent_settings_copy()
    }
    fn crumb(&self) -> Option<&'static str> {
        Some(nj_platform::i18n::msg::settings_title())
    }

    fn list_frame(&self) -> Rect {
        self.layout.sectioned_table()
    }

    fn view(&self) -> PrivacyView<'_> {
        PrivacyView {
            layout: self.layout,
            table: &self.form.table,
            keys: &self.form,
            frame: self.list_frame(),
            entry: self.entry,
            alert_open: self.alert.is_open(),
            alert_frames: self.alert_frames.get(),
            alert_choice: self.alert.choice(),
        }
    }

    /// The focus key of a row, by identity.
    #[cfg(test)]
    fn key_of(&self, id: RowId) -> u32 {
        self.form.index_of(&id).and_then(|i| self.form.key_at(i)).expect("the row is on this page").0
    }

    /// Commit the row whose focus key is `key`: Delete asks.
    fn row_commit(&mut self, key: u32, fx: &mut Effects<'_, InnerHost>) {
        if self.form.index_of_key(RowKey(key)).is_none() {
            return;
        }
        if form_activate(&self.form, key, fx) == Some(Act::Delete) {
            self.alert.open_with_body(nj_platform::i18n::msg::settings_consent_delete_question_c(), nj_platform::i18n::msg::settings_consent_delete_scope());
            self.state.alert_scroll = 0;
            self.state.alert = true;
            // the alert traps focus: seat the engine on its answers
            fx.push(Fx::Deliver(
                MachineId::Instance(nj_machine::machine::InstanceId(0)),
                Delivery::Screen(ScreenEvent::Enter(Enter::Fresh {
                    focus: FocusTarget::ContainerGroup(ALERT_GROUP),
                })),
            ));
        }
        fx.invalidate(nj_machine::present::Provenance::Input);
    }

    fn alert_answer(&mut self, destructive: bool, fx: &mut Effects<'_, InnerHost>) {
        if destructive {
            fx.push(Fx::App(AppFx::Loop(LoopReq::DeleteAllLocalData)));
        }
        self.alert.dismiss();
        self.state.alert = false;
        fx.push(Fx::Deliver(
            MachineId::Instance(nj_machine::machine::InstanceId(0)),
            Delivery::Screen(ScreenEvent::Enter(Enter::Fresh {
                focus: FocusTarget::ContainerGroup(TABLE_GROUP),
            })),
        ));
        fx.invalidate(nj_machine::present::Provenance::Input);
    }
}

/// The page's focus composition: the table, and while the delete alert is open the alert's two
/// answers ALONE (a modal traps focus, §7.3 step 7).
struct PrivacyView<'a> {
    layout: RouteLayout,
    table: &'a TableView,
    /// The table's row-key map: the page's focus elements are [`RowKey`]s, not row indices.
    keys: &'a dyn RowKeys,
    frame: Rect,
    entry: EntryId,
    alert_open: bool,
    alert_frames: Option<(Rect, Rect)>,
    /// The alert's own current answer, so `reconcile` bounces a stray focus key back to where the
    /// alert already stood rather than to a hardcoded default.
    alert_choice: AlertChoice,
}

impl<'a> PrivacyView<'a> {
    fn screen(&'a self) -> TableScreen<'a> {
        TableScreen::new(Header::new(self.layout, None, "", ""), self.table, TABLE_GROUP, self.entry)
            .keyed(self.keys)
            .with_frame(self.frame)
    }
    fn alert_key(&self, i: usize) -> FocusKey<u32> {
        FocusKey {
            entry: self.entry,
            elem: ALERT + i as u32,
        }
    }
    fn alert_rect(&self, i: usize) -> Rect {
        match (self.alert_frames, i) {
            (Some((c, _)), 0) => c,
            (Some((_, d)), _) => d,
            (None, _) => Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }
}

impl Focusable<InnerHost> for PrivacyView<'_> {
    fn groups(&self, cx: &Cx<'_, InnerHost>, out: &mut Vec<GroupSpec>) {
        if self.alert_open {
            out.push(GroupSpec {
                id: ALERT_GROUP,
                kind: GroupKind::Row { wrap: false },
                seat: Seat::First,
                reachable: AxisMask::BOTH,
                edge: [EdgeRule::Stop; 4],
                extent: self.alert_rect(0).union(self.alert_rect(1)),
                len: 2,
                elem: ElemKind::Control,
            });
            return;
        }
        Focusable::<InnerHost>::groups(&self.screen(), cx, out);
    }
    fn group_of(&self, key: &u32, cx: &Cx<'_, InnerHost>) -> Option<GroupId> {
        if self.alert_open {
            return alert_index(*key).map(|_| ALERT_GROUP);
        }
        Focusable::<InnerHost>::group_of(&self.screen(), key, cx)
    }
    fn neighbour(&self, key: FocusKey<u32>, dir: Dir, cx: &Cx<'_, InnerHost>) -> Step<u32> {
        if self.alert_open {
            return match (alert_index(key.elem), dir) {
                (Some(0), Dir::Right) => Step::Move(self.alert_key(1)),
                (Some(1), Dir::Left) => Step::Move(self.alert_key(0)),
                _ => Step::Edge,
            };
        }
        Focusable::<InnerHost>::neighbour(&self.screen(), key, dir, cx)
    }
    fn place(&self, key: &u32, cx: &Cx<'_, InnerHost>, at: At) -> Option<Placed> {
        if self.alert_open {
            let i = alert_index(*key)?;
            let r = self.alert_rect(i);
            return Some(Placed {
                rect: r,
                rest_rect: r,
                clip: Rect::FULL,
                index: Some(i as u32),
            });
        }
        Focusable::<InnerHost>::place(&self.screen(), key, cx, at)
    }
    fn reconcile(&self, want: FocusKey<u32>, cx: &Cx<'_, InnerHost>) -> FocusKey<u32> {
        if self.alert_open {
            // A pointer over the table can resolve a stray non-alert key while the alert is up;
            // it lands back on the answer the alert already holds, never on a fixed one.
            return self.alert_key(match alert_index(want.elem) {
                Some(i) => i,
                None => match self.alert_choice {
                    AlertChoice::Cancel => 0,
                    AlertChoice::Destructive => 1,
                },
            });
        }
        if alert_index(want.elem).is_some() {
            return FocusKey {
                entry: self.entry,
                elem: self.keys.key_at(self.table.sel.max(0) as usize).map_or(0, |k| k.0),
            };
        }
        Focusable::<InnerHost>::reconcile(&self.screen(), want, cx)
    }
    fn seat(&self, g: GroupId, from: Placed, cx: &Cx<'_, InnerHost>) -> FocusKey<u32> {
        if self.alert_open {
            return self.alert_key(0);
        }
        Focusable::<InnerHost>::seat(&self.screen(), g, from, cx)
    }
}

crate::focusable_via_view!(PrivacyPage, InnerHost, view);

impl Machine<InnerHost> for PrivacyPage {
    type Ev = ScreenEvent<InnerHost>;
    fn step(&mut self, ev: &Self::Ev, cx: &Cx<'_, InnerHost>, fx: &mut Effects<'_, InnerHost>) -> Handled {
        match ev {
            ScreenEvent::Tick(t) => {
                let dt = t.dt();
                self.form.table.update(dt, self.list_frame().h);
                self.alert.update(dt);
                Handled::Yes
            }
            ScreenEvent::FocusMoved { to, .. } => {
                if let Some(i) = alert_index(to.elem) {
                    self.alert.set_choice(if i == 1 { AlertChoice::Destructive } else { AlertChoice::Cancel });
                } else {
                    form_focus(&mut self.form, to.elem);
                }
                Handled::Yes
            }
            ScreenEvent::Activate(e) => {
                // The alert traps the pointer, through its exit fade too (`visible()`, not
                // `is_open()`): a click on the table under the scrim must not reopen it.
                if self.alert.visible() {
                    return Handled::Yes;
                }
                if alert_index(*e).is_none() {
                    self.row_commit(*e, fx);
                }
                Handled::Yes
            }
            ScreenEvent::PressCommit(_) => {
                // Answer only while the alert is still OPEN: a commit queued behind a BACK that
                // already dismissed it carries the closed alert's last selection, and must not
                // erase anything.
                if let Some(i) = cx.focus.current.and_then(|k| alert_index(k.elem)) {
                    if self.alert.is_open() {
                        self.alert_answer(i == 1, fx);
                    }
                }
                Handled::Yes
            }
            ScreenEvent::Input(InputEvent {
                kind: InputKind::Key { key: key @ (Key::Up | Key::Down), edge, .. }, ..
            }) if *edge != nj_machine::machine::Edge::Up && self.alert.is_open() => {
                self.state.alert_scroll = self.alert.scroll_by(
                    cx.measure,
                    if *key == Key::Up { -1 } else { 1 });
                Handled::Yes
            }
            ScreenEvent::Input(InputEvent {
                kind: InputKind::Key { key, edge: nj_machine::machine::Edge::Down, at_edge, .. },
                ..
            }) => {
                if self.alert.is_open() {
                    // the alert answers its own keys: BACK dismisses, OK is the press
                    if *key == Key::Back {
                        self.alert_answer(false, fx);
                        return Handled::Yes;
                    }
                    return Handled::No;
                }
                if self.alert.visible() {
                    // trap the dismissal fade too: a RIGHT at the table's edge must not reopen it
                    return Handled::Yes;
                }
                match key {
                    Key::Right if *at_edge => {
                        if let Some(k) = cx.focus.current {
                            if let Some(key) = form_right_target(&self.form, k.elem) {
                                self.row_commit(key, fx);
                            }
                        }
                        Handled::Yes
                    }
                    // BACK: the surface pops
                    _ => Handled::No,
                }
            }
            ScreenEvent::Enter(_) => Handled::Yes,
            _ => Handled::No,
        }
    }
}

impl Screen<InnerHost> for PrivacyPage {
    fn name(&self) -> &'static str {
        word::PRIVACY
    }
    fn state(&self) -> &dyn LogicalState {
        &self.state
    }
    fn crumb(&self, _cx: &Cx<'_, InnerHost>) -> Option<Cow<'_, str>> {
        PrivacyPage::crumb(self).map(Cow::Borrowed)
    }
    fn prepare(&mut self, _b: &mut Budget, _cx: &Cx<'_, InnerHost>) {}
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, InnerHost>) {
        let p = f.painter;
        let layout = self.layout;
        Header::new(layout, self.crumb(), self.title(), self.body())
            .with_copy_size(theme::size::LABEL).paint(p, f.measure);
        let alert_open = self.alert.is_open();
        {
            let mut ts = TableScreen::new(Header::new(layout, None, "", ""), &self.form.table, TABLE_GROUP, self.entry)
                .keyed(&self.form)
                .with_frame(self.list_frame());
            // the header was painted above with the page's own words
            Part::<InnerHost>::draw(&mut ts.table, f, self.list_frame());
        }
        // the delete alert over everything, its own scrim first; its answers are the stops the
        // engine seats on while it is open
        if self.alert.visible() {
            self.alert.draw_scrim();
            self.alert.draw(nj_platform::i18n::msg::settings_cancel_c(), nj_platform::i18n::msg::settings_delete_c(), f.measure);
            let frames = self.alert.frames(f.measure);
            self.alert_frames.set(Some(frames));
            // register the hit stops only once the entrance spring has arrived, so a click cannot
            // land on "Delete" before the panel is visibly there (`ui::decision_alert` rule 11)
            if alert_open && self.alert.settled() {
                for (i, r) in [frames.0, frames.1].into_iter().enumerate() {
                    f.stop(
                        crate::ui::Painter::root(),
                        Stop {
                            key: FocusKey {
                                entry: self.entry,
                                elem: ALERT + i as u32,
                            },
                            rect: r,
                            rest_rect: r,
                            clip: Rect::FULL,
                            hover: Hover::Focus,
                            activate: Activate::Press,
                        },
                    );
                }
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
}

#[cfg(test)]
#[path = "privacy_tests.rs"]
mod tests;
