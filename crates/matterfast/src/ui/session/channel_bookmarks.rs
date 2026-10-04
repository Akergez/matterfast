use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::ChannelBookmark;

use super::ui::Ui;
use crate::runtime;
use crate::ui::dialogs;

impl Ui {
    /// A channel's bookmarks. Servers older than 9.4 have no such route, so a
    /// failure here says the feature is missing rather than that it broke.
    pub(crate) fn channel_bookmarks(self: &Rc<Self>, cx: &mut App) {
        let (client, channel_id) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_channel.clone())
        };
        let Some(channel_id) = channel_id else { return };

        let holder: Rc<RefCell<Option<Rc<dialogs::BookmarkList>>>> = Rc::new(RefCell::new(None));
        let refill = {
            let holder = holder.clone();
            let client = client.clone();
            let channel_id = channel_id.clone();
            let ui = self.clone();
            move || {
                let holder = holder.clone();
                let client = client.clone();
                let channel_id = channel_id.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move { client.list_bookmarks(&channel_id).await },
                    move |result, cx| match result {
                        Ok(bookmarks) => {
                            let rows = bookmarks
                                .into_iter()
                                .map(|b| (b.id, b.display_name, b.link_url))
                                .collect();
                            if let Some(list) = holder.borrow().as_ref() {
                                list.set_bookmarks(rows, cx);
                            }
                        }
                        Err(e) => ui.toast(&format!("Bookmarks are not available here: {e}"), cx),
                    },
                );
            }
        };

        let add_client = client.clone();
        let add_channel = channel_id.clone();
        let add_refill = refill.clone();
        let delete_refill = refill.clone();
        let ui = self.clone();
        let opened = Rc::new(dialogs::BookmarkList::present(
            self,
            cx,
            move |display_name, link_url, _cx| {
                let bookmark = ChannelBookmark {
                    channel_id: add_channel.clone(),
                    display_name: if display_name.is_empty() {
                        link_url.clone()
                    } else {
                        display_name
                    },
                    link_url,
                    r#type: "link".into(),
                    ..Default::default()
                };
                let client = add_client.clone();
                let channel_id = add_channel.clone();
                let refill = add_refill.clone();
                runtime::spawn(
                    async move { client.create_bookmark(&channel_id, &bookmark).await },
                    move |_, _| refill(),
                );
            },
            move |link_url, cx| {
                crate::open_url(&link_url, cx);
            },
            move |bookmark_id, _cx| {
                let client = client.clone();
                let channel_id = channel_id.clone();
                let refill = delete_refill.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move { client.delete_bookmark(&channel_id, &bookmark_id).await },
                    move |result, cx| {
                        if let Err(e) = result {
                            ui.toast(&format!("Could not remove it: {e}"), cx);
                        }
                        refill();
                    },
                );
            },
        ));
        *holder.borrow_mut() = Some(opened);
        refill();
    }
}
