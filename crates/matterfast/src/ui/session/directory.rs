//! The server-wide lists the composer and the renderer need without a round
//! trip: custom emoji and mentionable groups.

use std::collections::BTreeSet;
use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::Group;

use super::ui::Ui;
use crate::runtime;
use crate::timefmt::now_ms;

/// How old the list of groups may be before a mention asks for it again.
const GROUPS_STALE_MS: i64 = 60 * 1000;

impl Ui {
    /// Fetches the names of the server's own emoji — all of them, a page at
    /// a time. They decide which `:words:` in a message are pictures and are
    /// what the pickers offer, and neither can wait for a round trip.
    pub(crate) fn load_custom_emoji(self: &Rc<Self>, _cx: &mut App) {
        const PAGE: u32 = 200;
        // A server with ten thousand of them is somebody's mistake; stop.
        const PAGES: u32 = 50;
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                let mut names = BTreeSet::new();
                for page in 0..PAGES {
                    let batch = client.custom_emoji(page, PAGE).await?;
                    let last = (batch.len() as u32) < PAGE;
                    names.extend(batch.into_iter().map(|emoji| emoji.name));
                    if last {
                        break;
                    }
                }
                Ok::<_, mattermost_api::Error>(names)
            },
            move |result, cx| match result {
                Ok(names) => {
                    let changed = {
                        let mut st = ui.state.borrow_mut();
                        let changed = st.custom_emoji != names;
                        st.custom_emoji = names;
                        changed
                    };
                    // Messages drawn before this landed spelled them out.
                    if changed {
                        ui.refresh_all(cx);
                    }
                }
                Err(e) => tracing::debug!(error = %e, "could not list the custom emoji"),
            },
        );
    }

    /// Fetches the groups that can be mentioned — all of them, a page at a
    /// time, like the emoji and for the same reason: a message has to know
    /// which `@words` are groups without a round trip, and so does the
    /// composer's list.
    ///
    /// Groups change: they are made, renamed and removed, and people join and
    /// leave. The server says so over the websocket, but not to everyone — an
    /// event about a group goes to its members — so this is also asked again
    /// whenever somebody starts typing a mention or opens a group, no more
    /// often than [`GROUPS_STALE_MS`]. Who is *in* a group is never
    /// kept at all: [`group::show`](crate::ui::group::show) asks each time.
    pub(crate) fn load_groups(self: &Rc<Self>, _cx: &mut App) {
        const PAGE: u32 = 200;
        const PAGES: u32 = 10;
        let client = {
            let mut st = self.state.borrow_mut();
            st.groups_fetched_at = now_ms();
            st.client.clone()
        };
        let ui = self.clone();
        runtime::spawn(
            async move {
                let mut groups = Vec::new();
                for page in 0..PAGES {
                    let batch = client.mentionable_groups(page, PAGE).await?;
                    let last = (batch.len() as u32) < PAGE;
                    // One without a name cannot be written after an `@`.
                    groups.extend(batch.into_iter().filter(|group| !group.name.is_empty()));
                    if last {
                        break;
                    }
                }
                Ok::<_, mattermost_api::Error>(groups)
            },
            move |result, cx| match result {
                Ok(groups) => {
                    let renamed = {
                        let mut st = ui.state.borrow_mut();
                        let names = |groups: &[Group]| {
                            groups
                                .iter()
                                .map(|group| group.name.clone())
                                .collect::<BTreeSet<_>>()
                        };
                        let renamed = names(&st.groups) != names(&groups);
                        st.groups = groups;
                        renamed
                    };
                    // Messages drawn before this landed took them for words.
                    if renamed {
                        ui.refresh_all(cx);
                    }
                }
                // A server without groups answers 501, which is the same as
                // having none.
                Err(e) => tracing::debug!(error = %e, "could not list the groups"),
            },
        );
    }

    /// [`Self::load_groups`], unless it was done a moment ago.
    pub(crate) fn refresh_groups(self: &Rc<Self>, cx: &mut App) {
        let age = now_ms() - self.state.borrow().groups_fetched_at;
        if age > GROUPS_STALE_MS {
            self.load_groups(cx);
        }
    }
}
