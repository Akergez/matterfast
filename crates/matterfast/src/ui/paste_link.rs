//! Pasting an address over selected words makes a link of them.
//!
//! It is what Mattermost's own clients do, and what people who have used one
//! expect: select "the notes", paste `https://…`, and the box holds
//! `[the notes](https://…)` rather than the address in place of the words.
//!
//! Deciding whether that applies, and writing the link, needs no window and
//! is kept apart from the box so that it can be tested without one.

use std::rc::Rc;

use gpui_kit::component::input::TextareaState;
use gpui_kit::{App, ClipboardItem, Entity, Focusable, Window};

use super::autocomplete::Composer;
use super::Ui;

/// Whether this is one address and nothing else.
fn is_address(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once("://") else {
        return false;
    };
    matches!(scheme, "http" | "https" | "ftp")
        && !rest.is_empty()
        && !text.contains(char::is_whitespace)
}

/// The Markdown link that `pasted` makes of `selected`, when it makes one:
/// when what was pasted is an address, and what is selected is words on one
/// line and not an address itself — pasting one address over another is
/// replacing it.
pub(crate) fn link_over(selected: &str, pasted: &str) -> Option<String> {
    let address = pasted.trim();
    if !is_address(address)
        || selected.trim().is_empty()
        || selected.contains('\n')
        || is_address(selected.trim())
    {
        return None;
    }
    // A bracket in the words would end the label early.
    let label = selected.replace('[', "\\[").replace(']', "\\]");
    // And a parenthesis in the address would end the address.
    Some(if address.contains(['(', ')']) {
        format!("[{label}](<{address}>)")
    } else {
        format!("[{label}]({address})")
    })
}

/// Ctrl+V in one of the composers. Answers whether the paste was taken: when
/// it makes no link, the text has to go in as it is.
pub(crate) fn pasted(
    ui: &Rc<Ui>,
    which: Composer,
    item: &ClipboardItem,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let composer = match which {
        Composer::Channel => ui.chat.composer.borrow().clone(),
        Composer::Thread => ui.right.composer.borrow().clone(),
    };
    let Some(composer) = composer else {
        return false;
    };
    let Some(pasted) = item.text() else {
        return false;
    };
    let selected = composer.read(cx).selected_text().to_string();
    let Some(link) = link_over(&selected, &pasted) else {
        return false;
    };
    composer.update(cx, |composer, cx| composer.replace(link, window, cx));
    // The text changed without the box saying so: the draft and the
    // completion list have to hear of it all the same.
    ui.later(cx, move |ui, cx| match which {
        Composer::Channel => ui.chat.composer_changed(ui, cx),
        Composer::Thread => ui.right.composer_changed(ui, cx),
    });
    true
}

/// The composer that has the keyboard, if one of them does.
pub(crate) fn focused(ui: &Rc<Ui>, window: &Window, cx: &App) -> Option<Composer> {
    let has_focus = |composer: &Option<Entity<TextareaState>>| {
        composer
            .as_ref()
            .is_some_and(|composer| composer.focus_handle(cx).is_focused(window))
    };
    if has_focus(&ui.chat.composer.borrow()) {
        Some(Composer::Channel)
    } else if has_focus(&ui.right.composer.borrow()) {
        Some(Composer::Thread)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::link_over;

    #[test]
    fn an_address_pasted_over_words_links_them() {
        assert_eq!(
            link_over("the notes", "https://example.com/notes").as_deref(),
            Some("[the notes](https://example.com/notes)")
        );
    }

    #[test]
    fn an_address_copied_with_space_around_it_is_still_an_address() {
        assert_eq!(
            link_over("here", "  https://example.com\n").as_deref(),
            Some("[here](https://example.com)")
        );
    }

    #[test]
    fn nothing_selected_is_an_ordinary_paste() {
        assert_eq!(link_over("", "https://example.com"), None);
        assert_eq!(link_over("  ", "https://example.com"), None);
    }

    #[test]
    fn what_is_not_one_address_is_an_ordinary_paste() {
        assert_eq!(link_over("words", "see https://example.com"), None);
        assert_eq!(link_over("words", "example.com"), None);
        assert_eq!(link_over("words", "just text"), None);
        assert_eq!(link_over("words", "https://"), None);
    }

    #[test]
    fn an_address_over_an_address_replaces_it() {
        assert_eq!(link_over("https://old.example", "https://new.example"), None);
    }

    #[test]
    fn several_lines_are_not_a_label() {
        assert_eq!(link_over("one\ntwo", "https://example.com"), None);
    }

    #[test]
    fn brackets_in_the_words_and_parentheses_in_the_address_do_not_break_the_link() {
        assert_eq!(
            link_over("[draft] plan", "https://example.com/a_(b)").as_deref(),
            Some("[\\[draft\\] plan](<https://example.com/a_(b)>)")
        );
    }
}
