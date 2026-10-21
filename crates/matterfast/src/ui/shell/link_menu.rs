//! The menu a right click on a link in a message opens: follow it, or copy
//! it.
//!
//! The text view says which link a click landed on only when the button comes
//! up, and a `context_menu` around it builds its menu on the way down, before
//! anyone knows there was a link under the pointer. So this menu is opened
//! from the link handler instead, kept by the window, and drawn over the rest
//! of it the way the toolkit draws its own context menus.

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::prelude::*;
use gpui_kit::{
    anchored, deferred, div, px, AnyElement, App, ClipboardItem, DismissEvent, Entity, Focusable,
    Pixels, Point, Subscription, Window,
};

use super::shell_view::Shell;
use super::with_shell::with_shell;
use crate::ui::message::{link_target, LinkTarget};

/// An open link menu, and where it was asked for.
pub(crate) struct LinkMenu {
    position: Point<Pixels>,
    menu: Entity<PopupMenu>,
    _dismiss: Subscription,
}

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
    with_shell(cx, move |shell, window, cx| {
        // Back to the composer, usually, once the menu is gone.
        let back = window.focused(cx).unwrap_or_else(|| shell.focus.clone());
        let target = link_target(&url);
        let menu = PopupMenu::build(window, cx, move |mut menu, _, _| {
            menu = menu.action_context(back);
            for (label, choice) in entries(&target) {
                let url = url.clone();
                menu = menu.item(
                    PopupMenuItem::new(label).on_click(move |_, _, cx| choose(choice, &url, cx)),
                );
            }
            menu
        });
        let dismiss = cx.subscribe_in(&menu, window, |shell, _, _: &DismissEvent, _, cx| {
            shell.link_menu = None;
            cx.notify();
        });
        // Focused, so that Escape and the arrow keys reach it.
        menu.focus_handle(cx).focus(window, cx);
        shell.link_menu = Some(LinkMenu {
            position,
            menu,
            _dismiss: dismiss,
        });
        cx.notify();
    });
}

/// Whether a link menu is open, for a script to check.
pub(crate) fn is_open(cx: &App) -> bool {
    cx.try_global::<super::matterfast::Matterfast>()
        .and_then(|app| app.window.as_ref())
        .is_some_and(|(_, shell)| shell.read(cx).link_menu.is_some())
}

impl Shell {
    /// The open link menu, over everything else in the window.
    pub(super) fn link_menu_layer(&self, window: &Window) -> Option<AnyElement> {
        let open = self.link_menu.as_ref()?;
        let size = window.bounds().size;
        Some(
            deferred(
                anchored().child(
                    div()
                        .w(size.width)
                        .h(size.height)
                        // The list under it must not scroll away from the
                        // link the menu is about.
                        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                        .child(
                            anchored()
                                .position(open.position)
                                .snap_to_window_with_margin(px(8.))
                                .child(open.menu.clone()),
                        ),
                ),
            )
            .with_priority(gpui_kit::base::POPUP_PRIORITY)
            .into_any_element(),
        )
    }
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
