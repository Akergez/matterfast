//! What clicking a link or a reaction button in a message does.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::ui::{dialogs, message, Action};

impl Ui {
    /// A clicked link: a person, a message on this server, or a web page.
    pub fn follow_link(self: &Rc<Self>, url: &str, cx: &mut App) {
        match message::link_target(url) {
            message::LinkTarget::Profile(handle) => self.show_profile_by_handle(&handle, cx),
            message::LinkTarget::Permalink(post_id) => self.open_permalink(post_id, cx),
            message::LinkTarget::Web(url) => crate::open_url(&url, cx),
        }
    }

    /// Asks which emoji, from the whole table, and reacts with it.
    pub fn pick_reaction(self: &Rc<Self>, post_id: String, cx: &mut App) {
        let ui = self.clone();
        dialogs::pick_emoji(self, cx, move |name, cx| {
            ui.dispatch(Action::ToggleReaction(post_id.clone(), name), cx)
        });
    }
}
