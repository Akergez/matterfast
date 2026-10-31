use std::cell::{Cell, RefCell};

use gpui_adaptive::{NavigatorState, Route};
use gpui_kit::{App, AppContext as _, Entity};

/// A place in the session: the list of conversations, the conversation, and
/// what is opened beside it.
///
/// They are the design system's [`Route`]s, and its navigator lays them
/// out: side by side while there is room (list, conversation, thread on a
/// wide window; conversation and thread on a narrower one), and on a phone
/// as a stack, each page over the one it was opened from, back by the
/// system's gesture, by a swipe to the right or by Escape. The list is the
/// main screen, as in Telegram: a conversation is a page over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    List,
    Chat,
    Thread,
    /// Search results: glanced at and put away, so where there is room they
    /// are laid over the conversation rather than pushing it aside.
    Search,
}

impl Route for Place {
    fn companion(&self) -> Option<Self> {
        match self {
            Place::List => None,
            Place::Chat => Some(Place::List),
            Place::Thread | Place::Search => Some(Place::Chat),
        }
    }

    /// Where there is room the conversation stands beside the list even
    /// before one is chosen: it says to choose one.
    fn wide_detail(&self) -> Option<Self> {
        (*self == Place::List).then_some(Place::Chat)
    }

    fn grows(&self) -> bool {
        *self == Place::Chat
    }

    fn floats(&self) -> bool {
        *self == Place::Search
    }
}

/// Which pages are open: the stack of the design system's navigator, which
/// is what is open. Made with the window's views, as an entity needs a
/// context; nothing is open before.
#[derive(Default)]
pub struct Pages {
    navigator: RefCell<Option<Entity<NavigatorState<Place>>>>,
    /// Whether the window is a phone's: one page at a time.
    pub(crate) collapsed: Cell<bool>,
}

impl Pages {
    pub(crate) fn attach(&self, cx: &mut App) -> Entity<NavigatorState<Place>> {
        self.navigator
            .borrow_mut()
            .get_or_insert_with(|| cx.new(|_| NavigatorState::new(Place::List)))
            .clone()
    }

    pub(crate) fn navigator(&self) -> Option<Entity<NavigatorState<Place>>> {
        self.navigator.borrow().clone()
    }

    fn stack(&self, cx: &App) -> Vec<Place> {
        self.navigator()
            .map_or_else(|| vec![Place::List], |navigator| navigator.read(cx).stack().to_vec())
    }

    fn set(&self, stack: Vec<Place>, cx: &mut App) {
        if let Some(navigator) = self.navigator() {
            navigator.update(cx, |navigator, cx| navigator.set_stack(stack, cx));
        }
        crate::ui::refresh(cx);
    }

    /// Whether a conversation is open: the page in front on a phone, or
    /// under the panel opened from it.
    pub fn showing_chat(&self, cx: &App) -> bool {
        self.stack(cx).contains(&Place::Chat)
    }

    /// The panel open over or beside the conversation, if one is.
    pub fn panel(&self, cx: &App) -> Option<Place> {
        self.stack(cx).last().copied().filter(|pane| matches!(pane, Place::Thread | Place::Search))
    }

    /// The conversation in front, over the list.
    pub fn open_chat(&self, cx: &mut App) {
        let panel = self.panel(cx);
        self.set([Place::List, Place::Chat].into_iter().chain(panel).collect(), cx);
    }

    /// The list in front. On a phone nothing stays over it; where there is
    /// room the list is a column anyway, and a panel open beside the
    /// conversation stays.
    pub fn show_list(&self, cx: &mut App) {
        let panel = self.panel(cx).filter(|_| !self.collapsed.get());
        self.set([Place::List].into_iter().chain(panel).collect(), cx);
    }

    /// `panel` over what is open: over the conversation, or over the list
    /// when it was opened from there (a thread from the inbox).
    pub fn open_panel(&self, panel: Place, cx: &mut App) {
        let mut stack: Vec<Place> = self.stack(cx).into_iter().filter(|pane| !matches!(pane, Place::Thread | Place::Search)).collect();
        stack.push(panel);
        self.set(stack, cx);
    }

    /// Puts the panel away: back to what it was opened over.
    pub fn close_panel(&self, cx: &mut App) {
        let stack: Vec<Place> = self.stack(cx).into_iter().filter(|pane| !matches!(pane, Place::Thread | Place::Search)).collect();
        self.set(stack, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thread_goes_back_to_where_it_was_opened_from() {
        assert_eq!(Place::Thread.companion(), Some(Place::Chat));
        assert_eq!(Place::Chat.companion(), Some(Place::List));
        assert!(Place::Search.floats() && !Place::Thread.floats());
        assert!(Place::Chat.grows());
    }
}
