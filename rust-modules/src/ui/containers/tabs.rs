//! `TabContainer` (restructure spec §6.2): the shared RAIL as a `Column` group the container
//! contributes beside the page's groups, over ONE shared `NavStack<PageDip>` — Home is the single
//! root; a rail destination is `NavOp::SelectTab`, which covers-and-mints over the existing root
//! rather than replacing it (`NavOp::Root` is the other half of that split: a true replace, root
//! included, for a sign-in/sign-out/profile-switch reset rather than a peer press). The page
//! declares `strip_reachable()` (a `Screen` method with a default of yes); the dispatcher enters
//! the rail on a LEFT the page has no answer for, so no page carries a link to it.
//!
//! The application publishes rail members by stable control identity, with drawn and target
//! geometry from the shared renderer. Removing a destination never renumbers another control.
//! A member of [`POPOVER`] is a row of the floating popover one rail item opens (Litefin's
//! Libraries list); [`TabContainer::popover_opener`] says which item opened it.
//!
//! **A cover-and-mint's focus arrival always animates, whichever destination sent it** (issue #17,
//! fixed 2026-09-28). `NavStack::apply`'s `SelectTab` "cover-and-mint" arm used `Self::fresh`
//! (`FocusTarget::FirstInGroup`, `By::Restore`) exactly like a `Push`/`Root` mint, which several
//! screens (`library::LibraryScreen`'s `pop_from_rest`) treat as "restored to where it was" and
//! do not animate. A rail press always arrives FROM the visible chrome, so that read was never
//! right the first time a destination is visited this session, and it produced a hard snap onto
//! the first content tile with no transition. `Self::fresh_tab` (`FocusTarget::
//! FirstInGroupAnimated`, `By::Dir`) is the fix: same seat, reported as a deliberate move. See
//! `machine.rs`'s doc on `FocusTarget::FirstInGroupAnimated` and the regression test
//! `containers::tests::a_fresh_tab_mint_reports_a_deliberate_move_not_a_restore`.

use nj_machine::machine::{EntryId, GroupId, Host};
use super::super::screen::{AxisMask, EdgeRule, ElemKind, GroupKind, GroupSpec, Seat};
use super::super::Rect;
use super::stack::NavStack;
use super::transition::Transition;

/// The rail's group id — a library constant so a test can name it.
pub const STRIP: GroupId = GroupId(0xFFFF_0001);
/// The rail popover's group id.
pub const POPOVER: GroupId = GroupId(0xFFFF_0002);

#[derive(Clone, Copy)]
pub struct StripMember<K> {
    pub elem: K,
    pub drawn: Rect,
    pub target: Rect,
    pub clip: Rect,
    /// [`STRIP`] for the rail's column, [`POPOVER`] for a popover row.
    pub group: GroupId,
}

impl<K> StripMember<K> {
    pub fn new(elem: K, rect: Rect) -> Self {
        Self { elem, drawn: rect, target: rect, clip: Rect::FULL, group: STRIP }
    }
    /// The same control as a row of the popover.
    pub fn in_popover(self) -> Self {
        Self { group: POPOVER, ..self }
    }
}

pub struct TabContainer<H: Host> {
    pub stack: NavStack<H>,
    /// The destinations, in rail order: what each selects.
    pub pills: Vec<H::Arg>,
    /// The rail's column, then the open popover's rows. Visual order is independent of identity.
    /// Geometry is render state, keys are logical state.
    pub strip: Vec<StripMember<H::Elem>>,
    /// Preferred surviving destination if the focused member disappears. Independent of visual
    /// order: an avatar may be last without being the application's recovery destination.
    pub strip_fallback: Option<H::Elem>,
    /// The current page's own item: where focus lands when the rail is entered.
    pub strip_active: Option<H::Elem>,
    /// The rail item whose popover is open. The application opens it; the dispatcher closes it
    /// (LEFT, BACK, or UP/DOWN past an end) and hands focus back to this item.
    pub popover_opener: Option<H::Elem>,
    /// The selected pill: the destination's while a transition is in flight (`ui::nav::view_tab`).
    pub selected: usize,
}

impl<H: Host> TabContainer<H> {
    pub fn new(transition: Box<dyn Transition>) -> Self {
        Self {
            stack: NavStack::new(transition),
            pills: Vec::new(),
            strip: Vec::new(),
            strip_fallback: None,
            strip_active: None,
            popover_opener: None,
            selected: 0,
        }
    }

    fn extent(&self, group: GroupId) -> (Rect, usize) {
        let mut n = 0;
        let extent = self.strip.iter().filter(|m| m.group == group).fold(None::<Rect>, |acc, member| {
            n += 1;
            Some(acc.map_or(member.target, |a| a.union(member.target)))
        });
        (extent.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)), n)
    }

    /// The rail as a focus group — contributed beside the page's when the page allows it.
    ///
    /// Reachable on NO axis: a geometric search from the page never lands here, because the
    /// dispatcher is the one door in (a LEFT the page's own groups have no answer for), and that
    /// door remembers where the reader came from for the RIGHT back. Its own ends are terminal;
    /// RIGHT out of it is geometric when there is no remembered seat.
    pub fn strip_group(&self) -> GroupSpec {
        let (extent, len) = self.extent(STRIP);
        GroupSpec {
            id: STRIP,
            kind: GroupKind::Column,
            seat: Seat::Nearest,
            reachable: AxisMask(0),
            edge: [EdgeRule::Stop, EdgeRule::Stop, EdgeRule::Stop, EdgeRule::Geometric],
            extent,
            len,
            // Bare: OK activates on the down edge. The rail has no press face to dip.
            elem: ElemKind::Bare,
        }
    }

    /// The open popover's rows as a focus group, or `None` while it is closed.
    pub fn popover_group(&self) -> Option<GroupSpec> {
        let (extent, len) = self.extent(POPOVER);
        (len > 0).then_some(GroupSpec {
            id: POPOVER,
            kind: GroupKind::Column,
            seat: Seat::Nearest,
            reachable: AxisMask(0),
            edge: [EdgeRule::Stop; 4],
            extent,
            len,
            elem: ElemKind::Bare,
        })
    }

    /// Close the popover: its rows leave the strip, and the item that opened it is returned so
    /// the caller can hand focus back to it.
    pub fn close_popover(&mut self) -> Option<H::Elem> {
        self.strip.retain(|m| m.group != POPOVER);
        self.popover_opener.take()
    }

    /// The entry the strip belongs to — the top page (the strip is chrome on the page).
    pub fn strip_entry(&self) -> Option<EntryId> {
        self.stack.top().map(|e| e.id)
    }

    /// Select a pill: `SelectTab(pill)` on the shared stack.
    pub fn select(&mut self, i: usize, ret: super::super::screen::ReturnState<H::Elem, H::Memory>) {
        let Some(arg) = self.pills.get(i).cloned() else {
            return;
        };
        self.selected = i;
        self.stack
            .request(nj_machine::machine::NavOp::SelectTab(arg), ret);
    }
}
