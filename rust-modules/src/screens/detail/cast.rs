//! Cast-and-crew shelf geometry and Person navigation: Litefin's `Cast & Crew` row (`docs/litefin-look.md`
//! 5), the classic poster card with the person's name and credit under every card, left aligned
//! (4.2 card labels): an actor's character worded "as {role}", a crew member's job as it is.

use crate::metadata::Detail;
use crate::catalog::ServerId;
use crate::ui::card_row::{self, CardRow, RowStyle};
use crate::ui::tile::{TileFacts, TileKind};
use nj_machine::machine::GroupId;
use crate::ui::widgets::Art;
use crate::ui::{Painter, Rect};

pub(crate) const CAST_ELEM_RANGE_START: u32 = 1152;
pub(crate) const CAST_ELEM_RANGE_END: u32 = 1664;
pub(crate) const CAST_GROUP: GroupId = GroupId(4);
/// Heading cap top to card top — the SHARED shelf pitch, stated as the sum, so the detail shelves
/// move together (see [`super::related::LABEL_H`]).
pub(crate) const LABEL_H: f32 = crate::ui::consts::TITLE_DY + crate::ui::consts::CARD_DY;
/// The Home poster shelf with the detail rows' left edge ([`super::section::POSTERS`]).
const STYLE: RowStyle = super::section::POSTERS;
const SLOT: f32 = STYLE.w + STYLE.gap;

pub(crate) fn elem(index: usize) -> Option<u32> {
    (index < (CAST_ELEM_RANGE_END - CAST_ELEM_RANGE_START) as usize)
        .then_some(CAST_ELEM_RANGE_START + index as u32)
}

pub(crate) fn locate(key: u32) -> Option<usize> {
    (CAST_ELEM_RANGE_START..CAST_ELEM_RANGE_END)
        .contains(&key)
        .then(|| (key - CAST_ELEM_RANGE_START) as usize)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
    None,
    OpenPerson {
        sid: ServerId,
        key: String,
        guid: String,
        name: String,
        thumb: String,
    },
}

pub(crate) fn action(d: &Detail, key: u32) -> Action {
    let Some(c) = locate(key).and_then(|i| d.credit(i)) else {
        return Action::None;
    };
    let person_key = c.person_key();
    if person_key.is_empty() {
        return Action::None;
    }
    Action::OpenPerson {
        sid: d.sid,
        key: person_key,
        guid: c.tag_key.clone(),
        name: c.tag.clone(),
        thumb: c.thumb.clone(),
    }
}

pub(crate) fn rect(row: &CardRow, index: usize, top: f32, at_drawn: bool) -> Rect {
    let base = card_row::tile_rect(index, STYLE.margin_x, SLOT, row.scroll_x(), top + LABEL_H, (STYLE.w, STYLE.h));
    if at_drawn {
        base.scaled(row.scale(index))
    } else {
        base.scaled(STYLE.focus_scale)
    }
}

/// The cast row's under-band is FIXED: every card wears its name and credit (4.2), so the room is
/// the shared label block's whole height on every frame.
pub(crate) fn block_h() -> f32 {
    LABEL_H + STYLE.h + card_row::UNDER_LABEL_H
}

/// Credit `i` as the facts its card is worded from: the name over the actor's character or the
/// crew member's job ([`card_row::tile_label`]).
fn facts(d: &Detail, i: usize) -> Option<TileFacts<'_>> {
    let c = d.credit(i)?;
    let kind = if i < d.cast.len() { TileKind::Person } else { TileKind::Crew };
    Some(TileFacts { kind, title: &c.tag, role: d.credit_role(i).unwrap_or_default(), ..Default::default() })
}

pub(crate) fn draw(
    p: Painter,
    d: &Detail,
    row: &CardRow,
    top: f32,
    focused: Option<usize>,
    measure: &dyn nj_machine::machine::Measure,
) {
    super::section::heading(p, nj_platform::i18n::msg::browse_detail_cast_c(), top - row.lift());
    card_row::strip(
        p,
        row,
        d.credits_len(),
        focused.map(|i| i as i32).unwrap_or(-1),
        top + LABEL_H,
        (STYLE.w, STYLE.h),
        SLOT,
        &STYLE,
        crate::ui::consts::SCR_W,
        |i| Art::Person {
            sid: d.sid.raw(),
            key: d.credit(i).map(|c| c.thumb.as_str()).unwrap_or(""),
            res: (STYLE.w as i32, STYLE.h as i32),
        },
        |_| None,
        |i| facts(d, i).map(|m| card_row::tile_label(&m, false)).unwrap_or_default(),
        |_, _, _, _| {},
        measure,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credit(name: &str, role: &str, id: i64) -> crate::metadata::Cast {
        crate::metadata::Cast { tag: name.into(), role: role.into(), thumb: String::new(), id, tag_key: String::new() }
    }

    fn words(d: &Detail, i: usize) -> (Option<String>, Option<String>) {
        let label = card_row::tile_label(&facts(d, i).expect("a credit"), false);
        let text = |c: Option<std::ffi::CString>| c.map(|c| c.into_string().unwrap());
        (text(label.title), text(label.caption))
    }

    /// Litefin's `CardRenderer`: an actor's card reads "as {role}" under the name, a crew member's
    /// reads the job as it is, and a credit with no role reads the name alone.
    #[test]
    fn a_cast_card_words_its_credit_as_litefin_does() {
        let _serial = nj_base::testlock::serial();
        let _lang = nj_platform::i18n::language_on_this_thread_for_test(nj_platform::i18n::Preference::En);
        let d = Detail {
            cast: vec![credit("Derek de Lint", "Thom", 1), credit("Extra", "", 2)],
            crew: vec![credit("Ian Hubert", "Director", 3)],
            ..Default::default()
        };
        assert_eq!(words(&d, 0), (Some("Derek de Lint".into()), Some("as Thom".into())));
        assert_eq!(words(&d, 1), (Some("Extra".into()), None));
        assert_eq!(words(&d, 2).1.as_deref(), d.credit_role(2), "the job, translated, with no prefix");
        assert!(facts(&d, 3).is_none());
        let _lang = nj_platform::i18n::language_on_this_thread_for_test(nj_platform::i18n::Preference::Pl);
        assert_eq!(words(&d, 0).1.as_deref(), Some("jako Thom"));
    }

    /// The row is the Home poster shelf moved to the detail rows' edge, and its block reserves the
    /// shared label band under every card.
    #[test]
    fn the_cast_row_is_the_home_poster_shelf_with_its_label_band() {
        assert_eq!((STYLE.w, STYLE.h, STYLE.gap, STYLE.focus_scale), (RowStyle::HOME.w, RowStyle::HOME.h, RowStyle::HOME.gap, RowStyle::HOME.focus_scale));
        assert_eq!(block_h() - LABEL_H - STYLE.h, card_row::TileLabel::height(true));
    }

    #[test]
    fn ok_on_a_crew_tile_opens_that_crew_members_page_not_an_actor_at_the_same_index() {
        let detail = Detail {
            sid: ServerId::UNSET,
            cast: vec![credit("Actor", "Role", 1)],
            crew: vec![credit("Writer", "Writer", 2)],
            ..Default::default()
        };
        assert_eq!(
            action(&detail, elem(1).unwrap()),
            Action::OpenPerson {
                sid: ServerId::UNSET,
                key: "2".into(),
                guid: String::new(),
                name: "Writer".into(),
                thumb: String::new(),
            }
        );
    }
}
