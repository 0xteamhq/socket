//! Notes written on a deal, a person, an organisation or a lead.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Note {
    pub id: u64,
    /// What the note says, as HTML. A list returns only its beginning; see `truncated`.
    pub content: Option<String>,
    /// True when `content` is only the beginning of the note, as in a list. `get` returns it whole.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    /// The user who wrote it.
    pub user_id: Option<u64>,
    pub deal_id: Option<u64>,
    pub person_id: Option<u64>,
    pub org_id: Option<u64>,
    pub lead_id: Option<String>,
    pub add_time: Option<String>,
    pub update_time: Option<String>,
}

/// How much of a note a list returns, in characters.
const PREVIEW: usize = 500;

/// `note` as a row of a list: with the beginning of a long text only.
pub(crate) fn preview(mut note: Note) -> Note {
    if let Some(content) = &mut note.content {
        // Cut between characters, never inside one.
        if let Some((end, _)) = content.char_indices().nth(PREVIEW) {
            content.truncate(end);
            note.truncated = true;
        }
    }
    note
}

/// Which notes to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListNotes {
    /// Only the notes on this deal.
    pub deal_id: Option<u64>,
    /// Only the notes on this person.
    pub person_id: Option<u64>,
    /// Only the notes on this organisation.
    pub org_id: Option<u64>,
    /// Only the notes on this lead.
    pub lead_id: Option<String>,
    /// Only the notes this user wrote.
    pub user_id: Option<u64>,
    /// Only notes changed at or after this time, in RFC 3339.
    pub updated_since: Option<String>,
    /// A field and a direction, such as `update_time DESC`; several are separated by commas.
    pub sort: Option<String>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most notes to return in a page, from 1 to 500. Pipedrive returns 100 when not given.
    pub limit: Option<u32>,
}

/// A note to write. It has to be on a deal, a person, an organisation or a lead.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateNote {
    /// What the note says, as HTML. Pipedrive removes what is not safe to show.
    pub content: String,
    pub deal_id: Option<u64>,
    pub person_id: Option<u64>,
    pub org_id: Option<u64>,
    pub lead_id: Option<String>,
}

/// What to change on a note. A field that is not set is left as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateNote {
    /// What the note says now, as HTML. It replaces the text that was there.
    pub content: Option<String>,
    pub deal_id: Option<u64>,
    pub person_id: Option<u64>,
    pub org_id: Option<u64>,
    pub lead_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(content: &str) -> Note {
        Note {
            id: 1,
            content: Some(content.to_owned()),
            truncated: false,
            user_id: None,
            deal_id: None,
            person_id: None,
            org_id: None,
            lead_id: None,
            add_time: None,
            update_time: None,
        }
    }

    #[test]
    fn a_long_note_is_cut_at_the_limit_and_says_so() {
        let row = preview(note(&"a".repeat(PREVIEW + 1)));
        assert_eq!(row.content.as_deref().map(str::len), Some(PREVIEW));
        assert!(row.truncated);
    }

    #[test]
    fn a_note_of_exactly_the_limit_or_less_is_whole() {
        for length in [0, 1, PREVIEW] {
            let row = preview(note(&"a".repeat(length)));
            assert_eq!(row.content.as_deref().map(str::len), Some(length));
            assert!(!row.truncated, "{length}");
        }
        let absent = preview(Note {
            content: None,
            ..note("")
        });
        assert_eq!((absent.content, absent.truncated), (None, false));
    }

    #[test]
    fn the_cut_falls_between_characters() {
        // Each of these is three bytes, so a cut by bytes would split one.
        let row = preview(note(&"語".repeat(PREVIEW + 10)));
        assert_eq!(row.content.as_deref().map(|c| c.chars().count()), Some(PREVIEW));
        assert!(row.truncated);
    }
}
