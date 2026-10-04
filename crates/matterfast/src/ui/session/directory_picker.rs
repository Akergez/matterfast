//! The people-or-channels menu a card can ask for.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::dialogs;

impl Ui {
    /// A card's menu over the server's own people or channels. What is
    /// already known here answers at once, so the list is never empty while a
    /// request is out; the server's own search then fills in everyone and
    /// everything this client has not happened to load.
    pub(crate) fn pick_from_directory(
        self: &Rc<Self>,
        title: &str,
        channels: bool,
        on_pick: impl Fn(String, &mut App) + 'static,
        cx: &mut App,
    ) {
        let picker: Rc<RefCell<Option<Rc<dialogs::Picker>>>> = Rc::new(RefCell::new(None));
        let holder = picker.clone();
        let ui = self.clone();

        let opened = dialogs::Picker::present(
            self,
            cx,
            title,
            if channels {
                "Search channels"
            } else {
                "Search people"
            },
            move |term, cx| {
                let known = ui.known_directory(channels, &term);
                let Some(picker) = holder.borrow().clone() else {
                    return;
                };
                picker.set_results(&term, known.clone(), cx);

                let (client, team, naming) = {
                    let st = ui.state.borrow();
                    (
                        st.client.clone(),
                        st.current_team.clone().unwrap_or_default(),
                        st.teammate_name_display().to_string(),
                    )
                };
                let asked = term.clone();
                runtime::spawn(
                    async move {
                        let found: Vec<(String, String, String)> = if channels {
                            if team.is_empty() {
                                return Ok(Vec::new());
                            }
                            let found = if term.is_empty() {
                                client.channels_for_team(&team, 0, 100).await?
                            } else {
                                client.search_channels(&team, &term).await?
                            };
                            found
                                .into_iter()
                                .filter(|c| c.delete_at == 0)
                                .map(|c| (c.id, c.display_name, c.purpose))
                                .collect()
                        } else {
                            // Not scoped to a team or a channel: the menu is
                            // over everyone the server will let us see.
                            let found = client.search_users(&term, "", "", "").await?;
                            found
                                .into_iter()
                                .filter(|u| u.delete_at == 0)
                                .map(|u| {
                                    let handle = format!("@{}", u.username);
                                    (u.id.clone(), u.display_name(&naming), handle)
                                })
                                .collect()
                        };
                        mattermost_api::Result::Ok(found)
                    },
                    move |result, cx| {
                        let Ok(found) = result else { return };
                        let mut entries = known;
                        for entry in found {
                            if !entries.iter().any(|(id, _, _)| *id == entry.0) {
                                entries.push(entry);
                            }
                        }
                        entries.sort_by_cached_key(|(_, name, _)| name.to_lowercase());
                        picker.set_results(&asked, entries, cx);
                    },
                );
            },
            on_pick,
        );
        *picker.borrow_mut() = Some(Rc::new(opened));
    }

    /// The people or channels already held here that match `term`, as
    /// (id, name, subtitle), in name order.
    fn known_directory(&self, channels: bool, term: &str) -> Vec<(String, String, String)> {
        let needle = term.to_lowercase();
        let st = self.state.borrow();
        let mut entries: Vec<(String, String, String)> = if channels {
            st.channels
                .values()
                .filter(|c| c.delete_at == 0)
                .map(|c| (c.id.clone(), st.channel_title(c), c.purpose.clone()))
                .filter(|(_, name, _)| name.to_lowercase().contains(&needle))
                .collect()
        } else {
            st.users
                .values()
                .filter(|u| u.delete_at == 0)
                .map(|u| (u.id.clone(), st.display_name(u), format!("@{}", u.username)))
                .filter(|(_, name, handle)| {
                    name.to_lowercase().contains(&needle) || handle.to_lowercase().contains(&needle)
                })
                .collect()
        };
        entries.sort_by_cached_key(|(_, name, _)| name.to_lowercase());
        entries
    }
}
