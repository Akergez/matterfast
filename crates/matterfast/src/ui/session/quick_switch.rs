use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::constants::QUICK_SWITCH_ROWS;
use crate::ui::kit::Lucide;
use crate::ui::channel_icon::channel_icon;
use crate::ui::{switcher, Action};

impl Ui {
    /// Asks which channel, using the same switcher as Ctrl+K. Picking a
    /// destination is the same act as picking one to read, and a second list
    /// would be a second thing to keep working.
    pub(crate) fn pick_channel(
        self: &Rc<Self>,
        on_pick: impl Fn(String, &mut App) + 'static,
        cx: &mut App,
    ) {
        let holder: Rc<RefCell<Option<Rc<switcher::Switcher>>>> = Rc::new(RefCell::new(None));
        let search_ui = self.clone();
        let search_holder = holder.clone();
        let opened = switcher::Switcher::present(
            self,
            cx,
            move |term, cx| {
                let rows = search_ui.channel_rows(&term.to_lowercase());
                if let Some(switcher) = search_holder.borrow().as_ref() {
                    switcher.set_results(rows, cx);
                }
            },
            move |target, cx| {
                if let switcher::Target::Channel(channel_id) = target {
                    on_pick(channel_id, cx);
                }
            },
        );
        *holder.borrow_mut() = Some(opened);
    }

    /// Channels whose title contains `lowered`, alphabetically, capped to what
    /// the switcher has room for.
    fn channel_rows(&self, lowered: &str) -> Vec<switcher::Entry> {
        let st = self.state.borrow();
        let mut rows: Vec<switcher::Entry> = st
            .channels
            .values()
            .filter(|c| c.delete_at == 0)
            .filter_map(|channel| {
                let title = st.channel_title(channel);
                (lowered.is_empty() || title.to_lowercase().contains(lowered)).then(|| {
                    switcher::Entry {
                        target: switcher::Target::Channel(channel.id.clone()),
                        title,
                        subtitle: String::new(),
                        icon: channel_icon(channel),
                    }
                })
            })
            .collect();
        rows.sort_by_key(|row| row.title.to_lowercase());
        rows.truncate(QUICK_SWITCH_ROWS);
        rows
    }

    /// Ctrl+K: jump to a channel or a person by typing a few letters.
    ///
    /// Channels are matched locally against the sidebar — instant, and the
    /// list is small. People have to be searched for, because the client only
    /// knows the ones it has seen.
    pub(crate) fn quick_switch(self: &Rc<Self>, cx: &mut App) {
        let holder: Rc<RefCell<Option<Rc<switcher::Switcher>>>> = Rc::new(RefCell::new(None));
        let search_ui = self.clone();
        let pick_ui = self.clone();
        let search_holder = holder.clone();

        let opened = switcher::Switcher::present(
            self,
            cx,
            move |term, cx| {
                let lowered = term.to_lowercase();
                let mut rows = search_ui.channel_rows(&lowered);
                let (client, team_id) = {
                    let st = search_ui.state.borrow();
                    (
                        st.client.clone(),
                        st.current_team.clone().unwrap_or_default(),
                    )
                };

                if let Some(switcher) = search_holder.borrow().as_ref() {
                    switcher.set_results(rows.clone(), cx);
                }
                if lowered.is_empty() {
                    return;
                }

                // People arrive after the channels rather than instead of
                // them: the local answer should never wait on the network.
                let holder = search_holder.clone();
                let state = search_ui.state.clone();
                let asked = term.clone();
                runtime::spawn(
                    async move { client.search_users(&term, &team_id, "", "").await },
                    move |result, cx| {
                        let Ok(users) = result else { return };
                        let display = state.borrow().teammate_name_display().to_string();
                        rows.extend(users.into_iter().take(QUICK_SWITCH_ROWS).map(|user| {
                            switcher::Entry {
                                target: switcher::Target::User(user.id.clone()),
                                title: user.display_name(&display),
                                subtitle: format!("@{}", user.username),
                                icon: Lucide::User,
                            }
                        }));
                        if let Some(switcher) = holder.borrow().as_ref() {
                            // A slow answer for a term the person has already
                            // typed past must not replace the list under them.
                            if switcher.term(cx) == asked {
                                switcher.set_results(rows.clone(), cx);
                            }
                        }
                    },
                );
            },
            move |target, cx| match target {
                switcher::Target::Channel(id) => pick_ui.dispatch(Action::SelectChannel(id), cx),
                switcher::Target::User(id) => pick_ui.dispatch(Action::OpenDirectMessage(id), cx),
            },
        );
        *holder.borrow_mut() = Some(opened);
    }
}
