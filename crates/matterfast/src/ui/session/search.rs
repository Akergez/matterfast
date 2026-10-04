//! Searching messages, files and people, and the pinned list.

use std::rc::Rc;

use gpui_kit::App;

use super::hydrate::hydrate_authors;
use super::ui::Ui;
use crate::runtime;
use crate::state::ChannelFeed;
use crate::ui::constants::SEARCH_PAGE;
use crate::ui::rhs::PanelMode;
use crate::ui::search;

impl Ui {
    /// Pinned messages, in the right panel. They are a property of the channel
    /// rather than of any list we already hold, so they are fetched.
    pub(crate) fn show_pinned(self: &Rc<Self>, _cx: &mut App) {
        let (client, channel_id) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_channel.clone())
        };
        let Some(channel_id) = channel_id else { return };

        let ui = self.clone();
        runtime::spawn(
            async move {
                let posts = client.pinned_posts(&channel_id).await?;
                let (authors, statuses) = hydrate_authors(&client, &posts).await;
                Ok::<_, mattermost_api::Error>((posts, authors, statuses))
            },
            move |result, cx| match result {
                Ok((posts, authors, statuses)) => {
                    {
                        let mut st = ui.state.borrow_mut();
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        st.search_results = ChannelFeed::from_list(&posts).posts;
                        st.search_results.reverse();
                        st.searching = false;
                    }
                    ui.right
                        .set_mode(PanelMode::Search("Pinned messages".into()), cx);
                    ui.refresh_panel_mode(cx);
                    ui.overlay.set_show_sidebar(true, cx);
                    ui.refresh_messages(cx);
                }
                Err(e) => ui.toast(&format!("Could not load the pinned messages: {e}"), cx),
            },
        );
    }

    /// Runs a search and shows the hits in the right panel.
    ///
    /// Search is one of the routes the server refuses while it is busy, so a
    /// failure here is worth saying out loud rather than showing as "no
    /// results" — those mean very different things to whoever is looking.
    pub(crate) fn search(self: &Rc<Self>, terms: String, cx: &mut App) {
        // "file:" scopes the same box to attachments. A second search field
        // would be a second thing to find; Mattermost's own syntax already
        // works this way for `in:` and `from:`.
        if let Some(rest) = terms.strip_prefix("file:") {
            self.search_files(rest.trim().to_string(), cx);
            return;
        }
        let (client, team) = {
            let mut st = self.state.borrow_mut();
            st.searching = true;
            st.search_results.clear();
            st.search_terms = terms.clone();
            st.search_pages = 0;
            st.search_more = false;
            (st.client.clone(), st.current_team.clone())
        };
        let Some(team_id) = team else { return };

        self.right.set_mode(PanelMode::Search(terms.clone()), cx);
        self.right.search_from_the_top();
        self.refresh_panel_mode(cx);
        self.overlay.set_show_sidebar(true, cx);
        self.refresh_messages(cx);
        self.search_page(client, team_id, terms, 0);
    }

    /// The page after the ones already showing, asked for when the list is
    /// scrolled to its end.
    pub(crate) fn search_more(self: &Rc<Self>, cx: &mut App) {
        let (client, team, terms, page) = {
            let mut st = self.state.borrow_mut();
            if st.searching || !st.search_more {
                return;
            }
            st.searching = true;
            (
                st.client.clone(),
                st.current_team.clone(),
                st.search_terms.clone(),
                st.search_pages,
            )
        };
        let Some(team_id) = team else { return };
        self.refresh_messages(cx);
        self.search_page(client, team_id, terms, page);
    }

    /// Fetches one page of hits and adds it under the pages before it.
    fn search_page(
        self: &Rc<Self>,
        client: mattermost_api::Client,
        team_id: String,
        terms: String,
        page: u32,
    ) {
        // The dates in a search are days of the person asking.
        let offset = chrono::Local::now().offset().local_minus_utc();
        let asked = terms.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                let search = mattermost_api::rest::PostSearch {
                    time_zone_offset: offset,
                    page,
                    per_page: SEARCH_PAGE,
                    ..mattermost_api::rest::PostSearch::new(&terms)
                };
                let hits = client.search_posts(&team_id, &search).await?;
                let (authors, statuses) = hydrate_authors(&client, &hits.posts).await;
                Ok::<_, mattermost_api::Error>((hits.posts, authors, statuses))
            },
            move |result, cx| {
                {
                    let mut st = ui.state.borrow_mut();
                    // An answer to a search that has since been replaced by
                    // another has nowhere to go.
                    if st.search_terms != asked || st.search_pages != page {
                        return;
                    }
                    st.searching = false;
                    match result {
                        Ok((posts, authors, statuses)) => {
                            for user in authors {
                                st.users.insert(user.id.clone(), user);
                            }
                            st.apply_statuses(statuses);
                            // Newest first reads better for a search than the
                            // oldest-first order a channel wants, and an
                            // older page goes under the newer ones.
                            let mut found = ChannelFeed::from_list(&posts).posts;
                            found.reverse();
                            st.search_more = found.len() >= SEARCH_PAGE as usize;
                            st.search_pages = page + 1;
                            found.retain(|post| {
                                !st.search_results.iter().any(|known| known.id == post.id)
                            });
                            st.search_results.extend(found);
                        }
                        Err(e) => {
                            st.search_more = false;
                            drop(st);
                            tracing::warn!(error = %e, terms = %asked, page, "search failed");
                            ui.toast(&format!("Search failed: {e}"), cx);
                            ui.refresh_messages(cx);
                            return;
                        }
                    }
                }
                ui.refresh_messages(cx);
            },
        );
    }

    /// Looks the name typed after `from:` up on the server. The people
    /// already known here are listed at once; this adds whoever the client
    /// has not met, when the answer comes — which, on a server of any size,
    /// is most people. It asks the way the composer's `@` list does
    /// (`users/autocomplete`, the team's members), an empty name included:
    /// that is the route the official clients use for this list too.
    pub(crate) fn search_people(self: &Rc<Self>) {
        let Some(typed) = self.search_box.asking_for_person() else {
            return;
        };
        let (client, team_id) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_team.clone().unwrap_or_default())
        };
        let ui = self.clone();
        runtime::spawn(
            {
                let typed = typed.clone();
                async move { client.autocomplete_users(&typed, &team_id, "").await }
            },
            move |result, cx| {
                let found = match result {
                    Ok(found) => found,
                    Err(error) => {
                        tracing::warn!(%error, "could not look up people for the search box");
                        return;
                    }
                };
                // Only for the name still being typed: an answer to "an"
                // must not land in the list for "anna".
                if ui.search_box.asking_for_person().as_deref() != Some(typed.as_str()) {
                    return;
                }
                let rows = {
                    let st = ui.state.borrow();
                    let display = st.teammate_name_display();
                    found
                        .users
                        .iter()
                        .chain(found.out_of_channel.iter())
                        .filter(|user| user.delete_at == 0)
                        .map(|user| search::person(&user.username, &user.display_name(display)))
                        .collect()
                };
                ui.search_box.add_people(rows, cx);
            },
        );
    }

    /// Attachments matching a search, as a list of names to open.
    fn search_files(self: &Rc<Self>, terms: String, cx: &mut App) {
        let (client, team) = {
            let mut st = self.state.borrow_mut();
            st.searching = true;
            st.search_results.clear();
            // One page is all a file search has, and a page of the search
            // before it must not land among its hits.
            st.search_terms = format!("file:{terms}");
            st.search_pages = 0;
            st.search_more = false;
            (st.client.clone(), st.current_team.clone())
        };
        let Some(team_id) = team else { return };

        self.right
            .set_mode(PanelMode::Search(format!("files: {terms}")), cx);
        self.refresh_panel_mode(cx);
        self.overlay.set_show_sidebar(true, cx);
        self.refresh_messages(cx);

        let ui = self.clone();
        runtime::spawn(
            async move { client.search_files(&team_id, &terms).await },
            move |result, cx| {
                match result {
                    Ok(files) => {
                        // A file hit names the post it is attached to, so the
                        // posts are what gets listed — the same rows as any
                        // other search, and clicking one goes to the message.
                        let ids: Vec<String> = files.ordered().map(|f| f.post_id.clone()).collect();
                        ui.load_posts_by_id(ids, cx);
                    }
                    Err(e) => {
                        ui.state.borrow_mut().searching = false;
                        ui.toast(&format!("File search failed: {e}"), cx);
                        ui.refresh_messages(cx);
                    }
                }
            },
        );
    }

    /// Fetches posts by id and shows them as the current search results.
    fn load_posts_by_id(self: &Rc<Self>, ids: Vec<String>, cx: &mut App) {
        if ids.is_empty() {
            self.state.borrow_mut().searching = false;
            self.refresh_messages(cx);
            return;
        }
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                let posts = client.posts_by_ids(&ids).await?;
                let (authors, statuses) = hydrate_authors(&client, &posts).await;
                Ok::<_, mattermost_api::Error>((posts, authors, statuses))
            },
            move |result, cx| {
                {
                    let mut st = ui.state.borrow_mut();
                    st.searching = false;
                    if let Ok((posts, authors, statuses)) = result {
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        st.search_results = ChannelFeed::from_list(&posts).posts;
                        st.search_results.reverse();
                    }
                }
                ui.refresh_messages(cx);
            },
        );
    }
}
