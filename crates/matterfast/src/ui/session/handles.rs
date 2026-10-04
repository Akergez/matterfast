//! Resolving `@handle` mentions into people, and opening their cards.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::{group, profile};

impl Ui {
    /// Fetches people known only by a handle, once each, so that a mention of
    /// somebody this client has not met becomes a mention like any other.
    /// Whoever asked is not told: the messages are simply drawn again.
    pub(crate) fn learn_handles(self: &Rc<Self>, handles: Vec<String>) {
        let handles: Vec<String> = {
            let mut asked = self.asked_handles.borrow_mut();
            handles
                .into_iter()
                .filter(|handle| asked.insert(handle.clone()))
                .collect()
        };
        if handles.is_empty() {
            return;
        }
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move { client.users_by_usernames(&handles).await },
            move |result, cx| {
                let Ok(users) = result else { return };
                if users.is_empty() {
                    return;
                }
                {
                    let mut st = ui.state.borrow_mut();
                    for user in users {
                        st.users.insert(user.id.clone(), user);
                    }
                }
                // The text of a message is prepared when the list is built,
                // so the list has to be built again to say their names.
                ui.refresh_messages(cx);
                cx.refresh_windows();
            },
        );
    }

    /// Asks the server about every handle a drawn message mentioned and
    /// nobody here answered to. Cheap when there are none, which is nearly
    /// always, so it can be called on every frame.
    pub(crate) fn learn_unknown_mentions(self: &Rc<Self>) {
        let unknown = {
            let st = self.state.borrow();
            if st.unknown_handles.borrow().is_empty() {
                return;
            }
            std::mem::take(&mut *st.unknown_handles.borrow_mut())
        };
        self.learn_handles(unknown.into_iter().collect());
    }

    /// Opens the card for a mention, which names a handle rather than an id.
    ///
    /// The special ones — here, channel, all — address everybody and have no
    /// account behind them, so there is nothing to open.
    pub(crate) fn show_profile_by_handle(self: &Rc<Self>, handle: &str, cx: &mut App) {
        if matches!(handle, "here" | "channel" | "all") {
            return;
        }
        let known = self
            .state
            .borrow()
            .users
            .values()
            .find(|u| u.username == handle)
            .map(|u| u.id.clone());
        if let Some(user_id) = known {
            self.show_profile(&user_id, cx);
            return;
        }

        // A group: who is in it. The list of groups is asked for again as
        // well, since this one may have been renamed or removed meanwhile.
        let group = self.state.borrow().group(handle).cloned();
        if let Some(group) = group {
            self.refresh_groups(cx);
            group::show(self, group, cx);
            return;
        }

        // Somebody mentioned in a message we are reading but who has never
        // posted here — worth one lookup rather than a dead link.
        let client = self.state.borrow().client.clone();
        let handles = vec![handle.to_string()];
        let ui = self.clone();
        runtime::spawn(
            async move { client.users_by_usernames(&handles).await },
            move |result, cx| {
                let Ok(users) = result else { return };
                let Some(user) = users.into_iter().next() else {
                    return;
                };
                let id = user.id.clone();
                ui.state.borrow_mut().users.insert(id.clone(), user);
                ui.show_profile(&id, cx);
            },
        );
    }

    pub(crate) fn show_profile(self: &Rc<Self>, user_id: &str, cx: &mut App) {
        if profile::show(self, user_id, cx) {
            return;
        }
        // Not held yet — fetch them and try again rather than telling
        // someone to wait for something they cannot make happen.
        let client = self.state.borrow().client.clone();
        let id = user_id.to_string();
        let ui = self.clone();
        runtime::spawn(
            async move { client.user(&id).await },
            move |result, cx| match result {
                Ok(user) => {
                    let id = user.id.clone();
                    ui.state.borrow_mut().users.insert(id.clone(), user);
                    ui.show_profile(&id, cx);
                }
                Err(_) => ui.toast("That person could not be looked up.", cx),
            },
        );
    }
}
