//! The composer's `:emoji:` and `@mention` completion.
//!
//! There are two composers — the conversation's and a thread's reply box —
//! and they complete the same way: each holds its own list, and what differs
//! between them is said once, by [`autocomplete::Composer`].

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::input::TextareaState;
use gpui_kit::{App, Entity};

use super::ui::Ui;
use crate::runtime;
use crate::ui::autocomplete::{self, Completions, Composer};
use crate::ui::constants::{COMPLETIONS, MENTION_DEBOUNCE};
use crate::ui::mentions::{local_groups, local_mentions, mention_list};
use crate::ui::{Action, WindowSlot};

/// What a box's text now asks to have completed, told to the session if it
/// is not what was being asked a keystroke ago.
pub(crate) fn completion_asked(
    ui: &Rc<Ui>,
    which: Composer,
    composer: &Entity<TextareaState>,
    cx: &mut App,
) {
    let (text, cursor) = {
        let composer = composer.read(cx);
        (composer.value().to_string(), composer.cursor())
    };
    let query = autocomplete::token_at(&text, cursor.min(text.len()));
    let completions = ui.completions(which);
    if completions.borrow().query == query {
        return;
    }
    {
        let mut completions = completions.borrow_mut();
        if query.is_none() {
            completions.close();
        }
        completions.query = query.clone();
    }
    // The answer goes to this box, and the other one is no longer being
    // typed in: a list left open over it would belong to nothing.
    let other = match which {
        Composer::Channel => Composer::Thread,
        Composer::Thread => Composer::Channel,
    };
    ui.completions(other).borrow_mut().close();
    ui.completing_in.set(which);
    ui.dispatch(Action::Complete(query), cx);
}

/// Replaces the token under the cursor of `composer` with the candidate its
/// list has selected. `false` when the list is not open, so the key that
/// asked means what it usually does.
pub(crate) fn accept_in(
    completions: &RefCell<Completions>,
    composer: &RefCell<Option<Entity<TextareaState>>>,
    window: &WindowSlot,
    cx: &mut App,
) -> bool {
    let chosen = {
        let completions = completions.borrow();
        completions.is_open().then(|| completions.chosen()).flatten()
    };
    let Some(insert) = chosen else {
        return false;
    };
    let Some(composer) = composer.borrow().clone() else {
        return false;
    };
    let (text, cursor) = {
        let composer = composer.read(cx);
        (composer.value().to_string(), composer.cursor())
    };
    let (text, caret) = autocomplete::accept(&text, cursor, &insert);
    completions.borrow_mut().close();
    window.update(cx, move |window, cx| {
        composer.update(cx, |composer, cx| {
            composer.set_value(text.clone(), window, cx);
            composer.set_selected_range(caret..caret, cx);
        });
    });
    true
}

impl Ui {
    /// The list of the box named.
    pub(crate) fn completions(&self, which: Composer) -> &RefCell<Completions> {
        match which {
            Composer::Channel => &self.chat.completions,
            Composer::Thread => &self.right.completions,
        }
    }

    /// Picks the selected candidate in the box named; see [`accept_in`].
    pub(crate) fn accept_completion(self: &Rc<Self>, which: Composer, cx: &mut App) -> bool {
        match which {
            Composer::Channel => self.chat.accept_completion(self, cx),
            Composer::Thread => self.right.accept_completion(self, cx),
        }
    }

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

            // Who can be named is asked of the channel being written in, and
            // a thread opened from the inbox is not in the one on screen.
            let channel = match self.completing_in.get() {
                Composer::Thread => Some(self.right.thread_channel()),
                Composer::Channel => None,
            }
            .filter(|channel| !channel.is_empty())
            .or_else(|| st.current_channel.clone())
            .unwrap_or_default();

            (
                st.client.clone(),
                st.current_team.clone().unwrap_or_default(),
                channel,
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
