//! The menu a right click on a link in a message opens: follow it, or copy
//! it.
//!
//! The text view says which link a click landed on only when the button comes
//! up, and a `context_menu` around it builds its menu on the way down, before
//! anyone knows there was a link under the pointer. So this menu is opened
//! from the link handler instead, kept by the window, and drawn over the rest
//! of it the way the toolkit draws its own context menus.

use gpui_adaptive::{Command, Menu};
use gpui_kit::{App, ClipboardItem, Pixels, Point, Window};

use super::with_shell::with_shell;
use crate::ui::message::{link_target, LinkTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Follow,
    Copy,
}

/// What the menu offers, worded for what the link is: a mention is a person
/// and a permalink is a message here, not pages a browser would get.
fn entries(target: &LinkTarget) -> [(&'static str, Choice); 2] {
    match target {
        LinkTarget::Profile(_) => [("View profile", Choice::Follow), ("Copy username", Choice::Copy)],
        LinkTarget::Permalink(_) => [("Go to message", Choice::Follow), ("Copy link", Choice::Copy)],
        LinkTarget::Web(_) => [("Open link", Choice::Follow), ("Copy link", Choice::Copy)],
    }
}

/// What "Copy" puts on the clipboard. A mention's link is an address made up
/// inside the app; what someone wants to paste is the handle as typed.
fn copied_text(url: &str) -> String {
    match link_target(url) {
        LinkTarget::Profile(handle) => format!("@{handle}"),
        _ => url.to_string(),
    }
}

fn choose(choice: Choice, url: &str, cx: &mut App) {
    match choice {
        Choice::Follow => {
            let Some(ui) = crate::ui::current(cx) else {
                return;
            };
            let url = url.to_string();
            ui.later(cx, move |ui, cx| ui.follow_link(&url, cx));
        }
        Choice::Copy => cx.write_to_clipboard(ClipboardItem::new_string(copied_text(url))),
    }
}

/// Opens the menu for `url` at `position`, in window coordinates. Must not be
/// called from inside the window's own update: it updates the window.
pub(crate) fn open(url: String, position: Point<Pixels>, cx: &mut App) {
    with_shell(cx, move |_, window, cx| {
        let commands = entries(&link_target(&url))
            .into_iter()
            .enumerate()
            .map(|(index, (label, choice))| {
                let url = url.clone();
                Command::new(("link-menu", index), label).on_trigger(move |_, cx| choose(choice, &url, cx))
            })
            .collect();
        Menu::open_at(None, commands, position, window, cx);
    });
}

/// Whether a link menu is open, for a script to check.
pub(crate) fn is_open(window: &Window, cx: &App) -> bool {
    Menu::is_open_at(window, cx)
}

#[cfg(test)]
mod tests {
    use super::{copied_text, entries, Choice};
    use crate::ui::message::LinkTarget;

    #[test]
    fn every_link_menu_offers_to_follow_and_to_copy() {
        for target in [
            LinkTarget::Profile("anton".into()),
            LinkTarget::Permalink("gedfji9g1pbjjehsngn9j17fzr".into()),
            LinkTarget::Web("https://example.com".into()),
        ] {
            let choices: Vec<_> = entries(&target).iter().map(|(_, choice)| *choice).collect();
            assert_eq!(choices, [Choice::Follow, Choice::Copy]);
        }
    }

    #[test]
    fn a_web_link_is_copied_as_it_is() {
        assert_eq!(copied_text("https://example.com/a?b=c"), "https://example.com/a?b=c");
    }

    #[test]
    fn a_mention_is_copied_as_the_handle() {
        let url = format!("{}anton", crate::markdown::MENTION_SCHEME);
        assert_eq!(copied_text(&url), "@anton");
    }
}
