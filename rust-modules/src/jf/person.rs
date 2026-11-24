//! A Jellyfin person's own record. `/Items/{personId}` carries the biography (`Overview`) and the
//! birth and death dates (`PremiereDate`, `EndDate`). The person page shows it in place of
//! plex.tv's profile, which this fork never dials: the household's server is the only peer.

use super::api::{Jf, Q};
use super::models::BaseItemDto;
use crate::catalog::discover::PersonProfile;

impl Jf<'_> {
    /// WORKER THREAD: the person's profile from their own server. `person_id` is the credit's
    /// person key, the interned integer or the GUID itself.
    pub(crate) fn person_profile(&self, person_id: &str) -> Option<PersonProfile> {
        let guid = self.guid_or_raw(person_id)?;
        let uid = self.user_id()?;
        let it: BaseItemDto = self.get(&Q::new(format!("/Items/{guid}")).s("userId", &uid).build())?;
        Some(profile_of(&it))
    }
}

/// The person's record as the profile the person page reads. Dates keep their ISO day, the form
/// the page formats; a person without them has none. A Jellyfin person can always be favourited.
pub(crate) fn profile_of(it: &BaseItemDto) -> PersonProfile {
    let day = |d: &Option<String>| d.as_deref().and_then(|d| d.get(..10)).unwrap_or("").to_string();
    PersonProfile {
        title: it.name.clone(),
        summary: it.overview.clone().unwrap_or_default(),
        born_at: day(&it.premiere_date),
        died_at: day(&it.end_date),
        favorite: Some(it.user_data.as_ref().is_some_and(|u| u.is_favorite)),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_person_item_becomes_the_profile_the_page_reads() {
        let it: BaseItemDto = serde_json::from_str(
            r#"{"Id":"aa","Name":"Derek de Lint","Type":"Person","Overview":"A Dutch actor.",
                "PremiereDate":"1950-07-15T00:00:00.0000000Z"}"#,
        )
        .unwrap();
        let p = profile_of(&it);
        assert_eq!(p.title, "Derek de Lint");
        assert_eq!(p.summary, "A Dutch actor.");
        assert_eq!(p.born_at, "1950-07-15");
        assert_eq!(p.died_at, "");
        let bare = profile_of(&BaseItemDto::default());
        assert!(bare.summary.is_empty() && bare.born_at.is_empty());
        assert_eq!(p.favorite, Some(false), "a Jellyfin person can be favourited");
        let fav: BaseItemDto = serde_json::from_str(r#"{"Id":"aa","Type":"Person","UserData":{"IsFavorite":true}}"#).unwrap();
        assert_eq!(profile_of(&fav).favorite, Some(true));
    }
}
