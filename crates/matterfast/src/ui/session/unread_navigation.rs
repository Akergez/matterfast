use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::ui::Action;

impl Ui {
    /// Moves to the next or previous channel with something unread, in
    /// sidebar order. Wraps, because the alternative is a shortcut that
    /// silently stops working at the end of the list.
    pub(crate) fn step_unread(self: &Rc<Self>, forwards: bool, cx: &mut App) {
        let next = {
            let st = self.state.borrow();
            // The order on screen: the folder that is open, newest first.
            let ordered: Vec<String> = st
                .chat_list(self.channels.folder().as_deref())
                .into_iter()
                .map(|c| c.id.clone())
                .collect();
            let unread: Vec<String> = ordered
                .iter()
                .filter(|id| st.unread(id).is_unread())
                .cloned()
                .collect();
            if unread.is_empty() {
                None
            } else {
                let here = st
                    .current_channel
                    .as_ref()
                    .and_then(|id| ordered.iter().position(|other| other == id))
                    .unwrap_or(0);
                let mut candidates: Vec<&String> = unread.iter().collect();
                if !forwards {
                    candidates.reverse();
                }
                candidates
                    .iter()
                    .find(|id| {
                        let position = ordered.iter().position(|other| &other == *id).unwrap_or(0);
                        if forwards {
                            position > here
                        } else {
                            position < here
                        }
                    })
                    .or(candidates.first())
                    .map(|id| (*id).clone())
            }
        };
        if let Some(channel_id) = next {
            self.dispatch(Action::SelectChannel(channel_id), cx);
        }
    }
}
