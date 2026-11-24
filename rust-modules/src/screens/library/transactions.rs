//! The semantic deferred transaction of the grid fade scope: a query edit waits for the fade.

use crate::stores::browse::ListingView;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GridTarget {
    pub epoch: u32,
    pub sid: crate::catalog::ServerId,
    pub section: i64,
    pub query: u32,
}

impl GridTarget {
    pub(super) fn from_view(view: ListingView<'_>) -> Option<Self> {
        let id = view.id()?;
        Some(Self { epoch: id.epoch, sid: id.sid, section: id.section, query: id.query })
    }

    pub(super) fn matches(&self, view: ListingView<'_>) -> bool {
        Self::from_view(view).as_ref() == Some(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GridAction {
    Sort { key: String, desc: bool },
    Unwatched { desired: bool },
    Genre { id: Option<String> },
    /// The Genres tab's drill-in (`QueryEdit::Drill`): into one genre, or out of it.
    Drill { id: Option<String> },
    LibraryType(crate::browse::LibraryType),
}

#[derive(Clone, Debug, Default)]
pub(super) struct PendingTransactions {
    grid: Option<(GridTarget, GridAction)>,
}

impl PendingTransactions {
    pub(super) fn grid(&self) -> Option<&(GridTarget, GridAction)> { self.grid.as_ref() }

    pub(super) fn request_grid(&mut self, target: GridTarget, action: GridAction) {
        self.grid = Some((target, action));
    }

    pub(super) fn cancel(&mut self) { self.grid = None; }

    /// Leaving the page (or the fade landing) commits the pending edit.
    pub(super) fn flush(&mut self) -> Option<(GridTarget, GridAction)> { self.grid.take() }
}

pub(super) const SHAPE: &str = "PendingTransactions{grid:Option<{target:{epoch:u32,sid:u32,section:u64,query:u32},action:Sort{key:str,desc:bool}|Unwatched{desired:bool}|Genre{id:Option<str>}|Drill{id:Option<str>}|LibraryType{code:u32}}>}";

impl nj_machine::machine::LogicalState for PendingTransactions {
    fn write(&self, c: &mut nj_machine::machine::Canon) {
        let Self { grid } = self;
        c.option(grid.as_ref(), |c, (target, action)| {
            let GridTarget { epoch, sid, section, query } = target;
            c.u32(*epoch).u32(u32::from(sid.raw())).u64(*section as u64).u32(*query);
            match action {
                GridAction::Sort { key, desc } => { c.u32(0).str(key).bool(*desc); }
                GridAction::Unwatched { desired } => { c.u32(1).bool(*desired); }
                GridAction::Genre { id } => { c.u32(2).option(id.as_deref(), |c, id| { c.str(id); }); }
                GridAction::LibraryType(kind) => { c.u32(3).u32(kind.code()); }
                GridAction::Drill { id } => { c.u32(4).option(id.as_deref(), |c, id| { c.str(id); }); }
            }
        });
    }
    fn probe(&self, out: &mut String) { out.push_str("library_pending"); }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(query: u32) -> GridTarget {
        GridTarget {
            epoch: 7,
            sid: crate::catalog::ServerId::from_raw(2),
            section: 42,
            query,
        }
    }

    #[test]
    fn the_newest_edit_supersedes_and_flush_takes_it_once() {
        let mut pending = PendingTransactions::default();
        pending.request_grid(grid(9), GridAction::Genre { id: Some("7".into()) });
        pending.request_grid(grid(9), GridAction::Sort { key: "titleSort".into(), desc: true });
        assert_eq!(pending.flush(), Some((grid(9), GridAction::Sort { key: "titleSort".into(), desc: true })));
        assert_eq!(pending.flush(), None);
    }

    #[test]
    fn unwatched_action_records_desired_value_not_a_toggle() {
        assert_ne!(
            GridAction::Unwatched { desired: true },
            GridAction::Unwatched { desired: false }
        );
    }
}
