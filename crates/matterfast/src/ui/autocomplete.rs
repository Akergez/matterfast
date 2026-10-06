//! Composer autocomplete for `@mentions` and `:emoji`.
//!
//! It knows nothing about Mattermost and never talks to the network: the
//! composer reports the token under the cursor, whoever owns the client answers
//! with candidates. That keeps a keystroke from turning into an HTTP call in
//! here, and leaves the token scanning — the only part with real logic — as
//! plain functions that can be tested without a display.

use std::sync::Arc;

use gpui_kit::RenderImage;

/// What the composer is currently asking to complete. The string is what has
/// been typed *after* the sigil, and is empty when only the sigil is there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    Mention(String),
    Emoji(String),
}

/// Which of the two boxes is being completed in: the conversation's, or the
/// reply box of a thread. Each keeps a list of its own, but only one of them
/// has the cursor, so an answer is only ever owed to one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Composer {
    #[default]
    Channel,
    Thread,
}

/// One row of the list, with everything it needs to draw itself: the list
/// does no lookups of its own, so a repaint can never turn into a fetch.
#[derive(Clone)]
pub struct Candidate {
    /// What replaces the token when picked, e.g. "@anna".
    pub insert: String,
    /// The line people read: a display name, or the emoji glyph.
    pub primary: String,
    /// Dimmed, to the right: the @handle, or "custom".
    pub secondary: String,
    /// The server's own emoji this row offers, by name: drawn as its picture
    /// where a built-in one has its glyph in `primary`.
    pub emoji: Option<String>,
    /// Already-loaded picture, when there is one. Emoji rows have none.
    pub image: Option<Arc<RenderImage>>,
    /// Whose picture this is, so a redraw can look one up again later.
    /// `None` for rows with no avatar to fetch, such as emoji and groups.
    pub user_id: Option<String>,
}

/// What the completion list is showing right now.
#[derive(Default)]
pub struct Completions {
    pub items: Vec<Candidate>,
    /// Something is always selected while the list is up, so Enter and Tab
    /// never need a first press just to pick a starting point.
    pub selected: usize,
    /// The query the visible candidates belong to. `None` means nothing is
    /// being completed, so a late answer must not pop the list back up after
    /// the person has moved on or pressed Escape.
    pub query: Option<Query>,
}

impl Completions {
    /// Whether the list is on screen, and so has first refusal on the keys.
    pub fn is_open(&self) -> bool {
        self.query.is_some() && !self.items.is_empty()
    }

    /// Candidates for the outstanding query. An empty list closes it.
    pub fn set(&mut self, items: Vec<Candidate>) {
        // Nobody is asking any more: a late answer has nothing to open.
        if self.query.is_none() {
            self.items.clear();
            return;
        }
        self.items = items;
        self.selected = 0;
    }

    /// Moves the selection, wrapping at both ends.
    pub fn step(&mut self, delta: i32) {
        self.selected = next_index(self.selected, delta, self.items.len());
    }

    /// What the selected row would insert.
    pub fn chosen(&self) -> Option<String> {
        self.items.get(self.selected).map(|item| item.insert.clone())
    }

    /// Hides the list and forgets the query, so that an answer still in flight
    /// does not reopen it.
    pub fn close(&mut self) {
        self.query = None;
        self.items.clear();
        self.selected = 0;
    }
}

/// The next row in a list of `len`, wrapping at both ends.
pub fn next_index(selected: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (selected as i64 + delta as i64).rem_euclid(len as i64) as usize
}

/// The token under the cursor, if it is one worth completing.
///
/// `cursor` is a byte offset into `text`; everything after it is ignored, so
/// the caller can simply pass the text up to the caret. The sigil only counts
/// at the start of a word, which is what keeps `a@b.com` an email address and
/// `3:4` a ratio rather than two half-typed completions.
pub fn token_at(text: &str, cursor: usize) -> Option<Query> {
    let before = text.get(..cursor)?;
    let start = token_start(before);

    let mut token = before[start..].chars();
    match token.next() {
        Some('@') => Some(Query::Mention(token.as_str().to_string())),
        Some(':') => Some(Query::Emoji(token.as_str().to_string())),
        _ => None,
    }
}

/// Where the word that ends at the end of `before` begins.
fn token_start(before: &str) -> usize {
    before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8())
}

/// Replaces the token under the cursor, sigil and all, with the picked
/// candidate and a trailing space — the space both ends the completion and is
/// what you would have typed next anyway. Answers the new text and where the
/// cursor belongs in it.
pub fn accept(text: &str, cursor: usize, insert: &str) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let Some(before) = text.get(..cursor) else {
        return (text.to_string(), cursor);
    };
    let start = token_start(before);
    let mut out = String::with_capacity(text.len() + insert.len() + 1);
    out.push_str(&text[..start]);
    out.push_str(insert);
    out.push(' ');
    let caret = out.len();
    out.push_str(&text[cursor..]);
    (out, caret)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The usual case: the cursor is at the end of what has been typed.
    fn typed(text: &str) -> Option<Query> {
        token_at(text, text.len())
    }

    #[test]
    fn completes_a_word_that_starts_with_a_sigil() {
        assert_eq!(typed("hey @ann"), Some(Query::Mention("ann".into())));
        assert_eq!(typed("nice :sm"), Some(Query::Emoji("sm".into())));
        // The sigil alone is enough to ask: the answer is the full list.
        assert_eq!(typed("@"), Some(Query::Mention(String::new())));
    }

    #[test]
    fn a_sigil_inside_a_word_is_not_a_token() {
        assert_eq!(typed("mail a@b.com"), None);
        assert_eq!(typed("a ratio of 3:4"), None);
    }

    #[test]
    fn a_newline_starts_a_new_token() {
        assert_eq!(
            typed("first line\n@bob"),
            Some(Query::Mention("bob".into()))
        );
    }

    #[test]
    fn text_after_the_cursor_is_not_part_of_the_token() {
        assert_eq!(
            token_at("@ann and the rest", 4),
            Some(Query::Mention("ann".into()))
        );
    }

    #[test]
    fn accepting_replaces_the_token_and_leaves_the_rest() {
        assert_eq!(accept("hey @an", 7, "@anna"), ("hey @anna ".into(), 10));
        // In the middle of a line: what follows the caret is untouched.
        assert_eq!(
            accept("@an and more", 3, "@anna"),
            ("@anna  and more".into(), 6)
        );
        assert_eq!(accept("ok :ta", 6, ":tada:"), ("ok :tada: ".into(), 10));
        // A cursor past the end is clamped rather than trusted.
        assert_eq!(accept("@a", 99, "@anna"), ("@anna ".into(), 6));
    }

    #[test]
    fn the_selection_wraps_at_both_ends() {
        assert_eq!(next_index(0, -1, 3), 2);
        assert_eq!(next_index(2, 1, 3), 0);
        assert_eq!(next_index(1, 1, 3), 2);
        // Nothing to select from.
        assert_eq!(next_index(0, 1, 0), 0);
    }

    #[test]
    fn a_late_answer_does_not_reopen_a_closed_list() {
        let candidate = Candidate {
            insert: "@anna".into(),
            primary: "Anna".into(),
            secondary: String::new(),
            emoji: None,
            image: None,
            user_id: None,
        };
        let mut list = Completions::default();
        list.set(vec![candidate.clone()]);
        assert!(!list.is_open(), "no query outstanding, so nothing opens");

        list.query = Some(Query::Mention("an".into()));
        list.set(vec![candidate]);
        assert!(list.is_open());
        assert_eq!(list.chosen().as_deref(), Some("@anna"));

        list.close();
        assert!(!list.is_open());
    }
}
