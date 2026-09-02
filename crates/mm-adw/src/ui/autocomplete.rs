//! Composer autocomplete for `@mentions` and `:emoji`.
//!
//! It knows nothing about Mattermost and never talks to the network: the
//! composer reports the token under the cursor, whoever owns the client answers
//! with candidates. That keeps a keystroke from turning into an HTTP call in
//! here, and leaves the token scanning — the only part with real logic — as a
//! plain function that can be tested without a display.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

/// What the composer is currently asking to complete. The string is what has
/// been typed *after* the sigil, and is empty when only the sigil is there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    Mention(String),
    Emoji(String),
}

/// One row of the popover, with everything it needs to draw itself: the list
/// does no lookups of its own, so a repaint can never turn into a fetch.
#[derive(Clone)]
pub struct Candidate {
    /// What replaces the token when picked, e.g. "@anna".
    pub insert: String,
    /// The line people read: a display name, or the emoji glyph.
    pub primary: String,
    /// Dimmed, to the right: the @handle, or "custom".
    pub secondary: String,
    /// Already-loaded picture, when there is one. Emoji rows have none.
    pub image: Option<gtk::gdk::Texture>,
}

pub struct Autocomplete {
    inner: Rc<Inner>,
}

/// Shared with the signal handlers, which outlive any single borrow of the
/// public wrapper.
struct Inner {
    entry: gtk::TextView,
    popover: gtk::Popover,
    list: gtk::ListBox,
    /// What each row inserts, in row order — the row index is the key.
    inserts: RefCell<Vec<String>>,
    /// The query the visible candidates belong to. `None` means nothing is
    /// being completed, so a late answer must not pop the list back up after
    /// the user has moved on or pressed Escape.
    query: RefCell<Option<Query>>,
}

impl Autocomplete {
    /// Attaches to a text view. `on_query` fires when the token under the
    /// cursor changes; the caller answers with [`Autocomplete::set_candidates`].
    pub fn new(entry: &gtk::TextView, on_query: impl Fn(Option<Query>) + 'static) -> Self {
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Browse)
            // The composer has to keep the keyboard while the list is up:
            // anything focusable in here takes it away and typing stops.
            .can_focus(false)
            .build();

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(240)
            // Wide enough for a name and a handle side by side; without it the
            // popover shrinks to the caret and truncates both to a few letters.
            .width_request(320)
            .child(&list)
            .build();

        // An autohiding popover grabs the input, which is fatal here — the
        // whole point is that you carry on typing while it is open. So it is
        // shown and hidden by hand instead.
        let popover = gtk::Popover::builder()
            .autohide(false)
            .has_arrow(false)
            // The composer sits at the bottom of the window, so the list goes
            // above the caret; GTK flips it back down if there is no room.
            .position(gtk::PositionType::Top)
            .child(&scroller)
            .build();
        popover.set_parent(entry);
        // The composer outlives every popover it opens, but not the window —
        // and a text view finalised with a popover still attached is the same
        // warning the sidebar rows produce.
        entry.connect_destroy({
            let popover = popover.clone();
            move |_| popover.unparent()
        });

        let inner = Rc::new(Inner {
            entry: entry.clone(),
            popover,
            list,
            inserts: RefCell::new(Vec::new()),
            query: RefCell::new(None),
        });

        // One handler covers typing and caret moves alike: the cursor position
        // changes on every insert, delete and jump, which is exactly when the
        // token under it can have become something else.
        entry.buffer().connect_cursor_position_notify({
            let inner = inner.clone();
            move |buffer| {
                let cursor = buffer.iter_at_mark(&buffer.get_insert());
                let before = buffer.text(&buffer.start_iter(), &cursor, false);
                let query = token_at(&before, before.len());
                if *inner.query.borrow() == query {
                    return;
                }
                if query.is_none() {
                    inner.popover.popdown();
                }
                *inner.query.borrow_mut() = query.clone();
                on_query(query);
            }
        });

        inner.list.connect_row_activated({
            let inner = inner.clone();
            move |_, _| inner.accept()
        });

        Autocomplete { inner }
    }

    /// Candidates for the outstanding query. An empty list closes the popover.
    pub fn set_candidates(&self, items: Vec<Candidate>) {
        let inner = &self.inner;
        inner.list.remove_all();
        inner.inserts.borrow_mut().clear();

        if items.is_empty() || inner.query.borrow().is_none() {
            inner.popover.popdown();
            return;
        }

        for item in items {
            let content = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .build();

            // A mention is a person and gets a face — initials until the
            // picture lands, as everywhere else, so the rows stay aligned.
            // An emoji is its own picture and gets none.
            if item.insert.starts_with('@') {
                let avatar = adw::Avatar::builder()
                    .size(24)
                    .show_initials(true)
                    .text(&item.primary)
                    .build();
                if let Some(texture) = &item.image {
                    avatar.set_custom_image(Some(texture));
                }
                content.append(&avatar);
            }

            let name = gtk::Label::builder()
                .label(&item.primary)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            content.append(&name);

            if !item.secondary.is_empty() {
                let hint = gtk::Label::builder()
                    .label(&item.secondary)
                    .xalign(1.0)
                    .hexpand(true)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .build();
                hint.add_css_class("dim-label");
                content.append(&hint);
            }

            let row = gtk::ListBoxRow::builder()
                .child(&content)
                .can_focus(false)
                .build();
            inner.list.append(&row);
            inner.inserts.borrow_mut().push(item.insert);
        }

        // Something is always selected, so Enter and Tab never need a first
        // press just to pick a starting point.
        inner.list.select_row(inner.list.row_at_index(0).as_ref());
        inner.point_at_cursor();
        inner.popover.popup();
    }

    /// True if the popover is open and consumed the key. The composer asks
    /// before acting on it, so Enter picks a candidate instead of sending.
    pub fn handle_key(&self, key: gtk::gdk::Key) -> bool {
        use gtk::gdk::Key;

        if !self.inner.popover.get_visible() {
            return false;
        }
        match key {
            Key::Escape => self.inner.close(),
            Key::Up | Key::KP_Up => self.inner.step(-1),
            Key::Down | Key::KP_Down => self.inner.step(1),
            Key::Tab | Key::ISO_Left_Tab | Key::Return | Key::KP_Enter => self.inner.accept(),
            _ => return false,
        }
        true
    }
}

impl Inner {
    /// Replaces the token under the cursor, sigil and all, with the selected
    /// candidate and a trailing space — the space both ends the completion and
    /// is what you would have typed next anyway.
    fn accept(&self) {
        let Some(row) = self.list.selected_row() else {
            return;
        };
        let Some(insert) = self.inserts.borrow().get(row.index() as usize).cloned() else {
            return;
        };

        let buffer = self.entry.buffer();
        let mut end = buffer.iter_at_mark(&buffer.get_insert());
        let mut start = end;
        // Same scan as `token_at`, in iterator terms: back to whitespace or the
        // start of the line. `starts_line` also guards the start of the buffer,
        // where `backward_char` would otherwise spin.
        while !start.starts_line() {
            let mut probe = start;
            probe.backward_char();
            if probe.char().is_whitespace() {
                break;
            }
            start = probe;
        }

        // `delete` leaves both iters at the deletion point, so `start` is
        // already where the replacement goes.
        buffer.delete(&mut start, &mut end);
        buffer.insert(&mut start, &format!("{insert} "));
        self.close();
    }

    /// Moves the selection, wrapping at both ends.
    fn step(&self, delta: i32) {
        let count = self.inserts.borrow().len() as i32;
        if count == 0 {
            return;
        }
        let next =
            (self.list.selected_row().map_or(0, |row| row.index()) + delta).rem_euclid(count);
        self.list.select_row(self.list.row_at_index(next).as_ref());
    }

    /// Hides the list and forgets the query, so that an answer still in flight
    /// does not reopen it.
    fn close(&self) {
        *self.query.borrow_mut() = None;
        self.popover.popdown();
    }

    /// Anchors the popover on the caret rather than the whole composer: with a
    /// wrapped multi-line draft the two are nowhere near each other.
    fn point_at_cursor(&self) {
        let buffer = self.entry.buffer();
        let cursor = buffer.iter_at_mark(&buffer.get_insert());
        let location = self.entry.iter_location(&cursor);
        let (x, y) = self.entry.buffer_to_window_coords(
            gtk::TextWindowType::Widget,
            location.x(),
            location.y(),
        );
        // Width of one caret: the popover is anchored to the insertion point,
        // not to a run of text.
        let rect = gtk::gdk::Rectangle::new(x, y, 1, location.height());
        self.popover.set_pointing_to(Some(&rect));
    }
}

/// The token under the cursor, if it is one worth completing.
///
/// `cursor` is a byte offset into `text`; everything after it is ignored, so
/// the caller can simply pass the text up to the caret. The sigil only counts
/// at the start of a word, which is what keeps `a@b.com` an email address and
/// `3:4` a ratio rather than two half-typed completions.
fn token_at(text: &str, cursor: usize) -> Option<Query> {
    let before = text.get(..cursor)?;
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());

    let mut token = before[start..].chars();
    match token.next() {
        Some('@') => Some(Query::Mention(token.as_str().to_string())),
        Some(':') => Some(Query::Emoji(token.as_str().to_string())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{token_at, Query};

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
}
