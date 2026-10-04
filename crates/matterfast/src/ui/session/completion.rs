//! The composer's `:emoji:` and `@mention` completion.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::constants::{COMPLETIONS, MENTION_DEBOUNCE};
use crate::ui::mentions::{local_groups, local_mentions, mention_list};
use crate::ui::autocomplete;

impl Ui {
    /// Answers the composer's completion query.
    ///
    /// Emoji come from the built-in table, which is local and therefore
    /// instant. Mentions have to be asked for, because who is in a channel is
    /// not something the client holds in full.
    pub(crate) fn complete(self: &Rc<Self>, query: Option<autocomplete::Query>, cx: &mut App) {
        use autocomplete::Query;
        let Some(query) = query else {
            // Closing the list has to cancel what is in flight as well, or a
            // late answer reopens it over a composer nobody is completing in.
            self.completion_generation
                .set(self.completion_generation.get() + 1);
            self.mention_query.borrow_mut().take();
            self.set_completions(Vec::new(), cx);
            return;
        };

        match query {
            Query::Emoji(term) => self.complete_emoji(&term, cx),
            Query::Mention(term) => self.complete_mention(term, cx),
        }
    }

    fn complete_emoji(self: &Rc<Self>, term: &str, cx: &mut App) {
        // Both tables are already here — the built-in one and the
        // server's own, fetched at sign-in — so this is instant and
        // asks nobody.
        let found = crate::emoji::search(term, &self.state.borrow().custom_emoji, COMPLETIONS);
        let items = found
            .into_iter()
            .map(|found| match found {
                crate::emoji::Found::Unicode(name, glyph) => autocomplete::Candidate {
                    insert: format!(":{name}:"),
                    primary: format!("{glyph}  :{name}:"),
                    secondary: String::new(),
                    emoji: None,
                    image: None,
                    user_id: None,
                },
                crate::emoji::Found::Custom(name) => autocomplete::Candidate {
                    insert: format!(":{name}:"),
                    primary: format!(":{name}:"),
                    secondary: String::new(),
                    emoji: Some(name),
                    image: None,
                    user_id: None,
                },
            })
            .collect();
        self.set_completions(items, cx);
    }

    fn complete_mention(self: &Rc<Self>, term: String, cx: &mut App) {
        let lowered = term.to_lowercase();
        // Groups come and go, and so do their people; this is the
        // moment the list has to be right.
        self.refresh_groups(cx);
        let (client, team_id, channel_id, local) = {
            let st = self.state.borrow();
            let display = st.teammate_name_display().to_string();

            // Answered from memory first, before anything touches the
            // network. This is what keeps the list under a frame: a
            // round trip is tens of milliseconds at best, and the
            // names most likely to be wanted are already here.
            let local = local_mentions(st.users.values(), &lowered, &display, &|id| {
                self.avatars.texture(id)
            });
            let local = mention_list(local, local_groups(&st.groups, &lowered));

            (
                st.client.clone(),
                st.current_team.clone().unwrap_or_default(),
                st.current_channel.clone().unwrap_or_default(),
                local,
            )
        };
        self.set_completions(local, cx);

        if channel_id.is_empty() {
            return;
        }

        // Every keystroke invalidates whatever is already in flight.
        self.completion_generation
            .set(self.completion_generation.get() + 1);

        // Only the last keystroke of a burst is asked about. Typing a
        // name is half a dozen letters, and the server is answering
        // the word, not each letter of it — without this, six round
        // trips race each other and five of them are thrown away.
        // The local list is already on screen, so the wait costs
        // nothing anyone can see.
        *self.mention_query.borrow_mut() = Some((term, team_id, channel_id, client));
        if self.mention_query_pending.get() {
            return;
        }
        self.mention_query_pending.set(true);
        let ui = self.clone();
        runtime::after(MENTION_DEBOUNCE, move |cx| {
            ui.mention_query_pending.set(false);
            // Whatever the latest keystroke left behind, not the one
            // that started the timer.
            let pending = ui.mention_query.borrow_mut().take();
            if let Some((term, team_id, channel_id, client)) = pending {
                ui.fetch_mentions(term, team_id, channel_id, client, cx);
            }
        });
    }

    /// The server's half of `@mention` completion: everyone in the channel
    /// this client has never heard of, plus the groups. Debounced by its
    /// caller, so this runs once per typed word rather than once per letter.
    fn fetch_mentions(
        self: &Rc<Self>,
        term: String,
        team_id: String,
        channel_id: String,
        client: mattermost_api::Client,
        _cx: &mut App,
    ) {
        let generation = self.completion_generation.get();
        let ui = self.clone();
        let lowered = term.to_lowercase();
        runtime::spawn(
            async move {
                client
                    .autocomplete_users(&term, &team_id, &channel_id)
                    .await
            },
            move |result, cx| {
                if ui.completion_generation.get() != generation {
                    return;
                }
                let Ok(found) = result else { return };
                let st = ui.state.borrow();
                let display = st.teammate_name_display().to_string();
                // The groups are held here, and matched here: the server is
                // only asked about people.
                let groups = local_groups(&st.groups, &lowered);
                drop(st);
                // People in the channel first; the server already
                // separates them, and suggesting someone who is not
                // here would post a mention that notifies nobody.
                let items: Vec<autocomplete::Candidate> = found
                    .users
                    .iter()
                    .chain(found.out_of_channel.iter())
                    .take(COMPLETIONS)
                    .map(|user| autocomplete::Candidate {
                        insert: format!("@{}", user.username),
                        // The name first: it is what somebody is
                        // looking for, and the handle is how the
                        // account is spelled.
                        primary: user.display_name(&display),
                        secondary: format!("@{}", user.username),
                        emoji: None,
                        image: ui.avatars.texture(&user.id),
                        user_id: Some(user.id.clone()),
                    })
                    .collect();
                // Nobody by that name on the server either: what is on screen
                // from memory stays.
                if items.is_empty() {
                    return;
                }
                ui.set_completions(mention_list(items, groups), cx);
            },
        );
    }
}
