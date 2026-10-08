//! Following a link to a message.

use std::rc::Rc;

use gpui_kit::App;

use super::hydrate::hydrate_authors;
use super::ui::Ui;
use crate::runtime;
use crate::state::ChannelFeed;
use crate::ui::constants::INITIAL_POSTS;

impl Ui {
    /// Follows a link to another message on this server.
    ///
    /// The link names only the post, so the channel has to be looked up —
    /// from what is loaded when possible, and from the server when not. A
    /// reminder about a message in a channel you have not opened this session
    /// is exactly the case that would otherwise fail.
    pub(crate) fn open_permalink(self: &Rc<Self>, post_id: String, cx: &mut App) {
        let (client, known) = {
            let st = self.state.borrow();
            (st.client.clone(), st.post(&post_id).map(|p| p.channel_id))
        };
        if let Some(channel_id) = known {
            self.jump_to_post(channel_id, post_id, cx);
            return;
        }

        let ui = self.clone();
        runtime::spawn(
            async move { client.post(&post_id).await },
            move |result, cx| match result {
                Ok(post) => {
                    let channel_id = post.channel_id.clone();
                    let post_id = post.id.clone();
                    ui.state.borrow_mut().apply_post(post);
                    ui.jump_to_post(channel_id, post_id, cx);
                }
                Err(e) => ui.toast(&format!("Could not open that message: {e}"), cx),
            },
        );
    }

    /// Opens a channel and puts one message on screen.
    ///
    /// Whether it is loaded decides what happens: if it is, scroll to it; if
    /// it is not, fetch the page around it, because a hit from three months
    /// ago is not reachable by paging back from today.
    pub(crate) fn jump_to_post(self: &Rc<Self>, channel_id: String, post_id: String, cx: &mut App) {
        self.select_channel(channel_id.clone(), cx);

        // After the channel's own load and layout have had their turn.
        let ui = self.clone();
        runtime::soon(move |cx| {
            if ui.chat.scroll_to_post(&post_id, cx) {
                return;
            }
            let (client, crt) = {
                let st = ui.state.borrow();
                (st.client.clone(), st.crt_enabled)
            };
            runtime::spawn(
                async move {
                    // Half a page either side, so the message lands in the
                    // middle with its context rather than at an edge.
                    let before = client
                        .posts_before(&channel_id, &post_id, INITIAL_POSTS / 2, crt)
                        .await?;
                    let after = client
                        .posts_after(&channel_id, &post_id, INITIAL_POSTS / 2, crt)
                        .await?;
                    let target = client.post(&post_id).await?;
                    let (authors, statuses) = hydrate_authors(&client, &before).await;
                    Ok::<_, mattermost_api::Error>((
                        channel_id, before, after, target, authors, statuses,
                    ))
                },
                move |result, cx| {
                    let Ok((channel_id, before, after, target, authors, statuses)) = result else {
                        ui.toast("That message could not be loaded.", cx);
                        return;
                    };
                    let post_id = target.id.clone();
                    {
                        let mut st = ui.state.borrow_mut();
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        // The page around the message is one block, and it
                        // is where it is: joined to what is held if they
                        // meet, in its place if they do not. Poured in on
                        // top of today's page it left everything between
                        // the two missing, with nothing to fetch it.
                        let older = ChannelFeed::from_list(&before);
                        let newer = ChannelFeed::from_list(&after);
                        let (at_oldest, at_latest) = (older.at_oldest, newer.at_latest);
                        let mut block = older.posts;
                        block.push(target);
                        block.extend(newer.posts);
                        st.feeds
                            .entry(channel_id)
                            .or_default()
                            .land(block, at_oldest, at_latest);
                    }
                    ui.refresh_messages(cx);
                    let ui = ui.clone();
                    runtime::soon(move |cx| {
                        ui.chat.scroll_to_post(&post_id, cx);
                    });
                },
            );
        });
    }
}
