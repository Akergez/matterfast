use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::component::input::TextareaState;
use gpui_kit::{point, px, App, Entity, FollowMode, ListAlignment, ListState, ScrollHandle};

use super::inbox::Inbox;
use super::panel_mode::PanelMode;
use super::search_row::SearchRow;
use super::thread_row::ThreadRow;
use crate::ui::autocomplete::Completions;
use crate::ui::WindowSlot;

pub struct RightPanel {
    pub(super) window: Rc<WindowSlot>,
    pub(super) mode: RefCell<PanelMode>,
    /// The open thread's rows, and the channel it lives in.
    pub(super) thread: RefCell<Vec<ThreadRow>>,
    pub(super) thread_channel: RefCell<String>,
    /// False until the thread, or at least its root, is known.
    pub(super) thread_loaded: Cell<bool>,
    pub(super) list: ListState,
    pub(super) following: Cell<bool>,
    pub(super) inbox: RefCell<Inbox>,
    /// Which rows of the inbox are on screen, and how tall the ones seen so
    /// far were. A reader who follows every channel has hundreds of rows in
    /// it, and building all of them for every frame of a scroll is what made
    /// scrolling it crawl: only the few in view are built.
    pub(super) inbox_list: ListState,
    pub(super) search: RefCell<Vec<SearchRow>>,
    pub(super) searching: Cell<bool>,
    /// Whether the server may have hits past the ones listed.
    pub(super) search_more: Cell<bool>,
    /// Where the list of hits is scrolled to: reaching its end is what asks
    /// for the next page.
    pub(super) search_scroll: ScrollHandle,
    /// How many hits the last frame drew, which is what the scroll position
    /// was measured against.
    pub(super) search_drawn: Cell<usize>,
    /// The reply box, while there is a window to put it in.
    pub(crate) composer: RefCell<Option<Entity<TextareaState>>>,
    /// Set while a draft is being restored, so it is not mistaken for typing.
    pub(super) restoring: Cell<bool>,
    /// What the reply box is offering to complete, as the conversation's
    /// composer has a list of its own.
    pub(crate) completions: RefCell<Completions>,
}

impl RightPanel {
    /// The inbox's retained row order, for scripted UI assertions.
    pub(crate) fn inbox_keys(&self) -> Vec<String> {
        self.inbox.borrow().entries.iter().map(|entry| entry.key()).collect()
    }

    pub fn new(window: Rc<WindowSlot>) -> Self {
        let list = ListState::new(0, ListAlignment::Bottom, px(400.));
        list.set_follow_mode(FollowMode::Tail);
        RightPanel {
            window,
            mode: RefCell::new(PanelMode::Hidden),
            thread: RefCell::new(Vec::new()),
            thread_channel: RefCell::new(String::new()),
            thread_loaded: Cell::new(false),
            list,
            following: Cell::new(false),
            inbox: RefCell::new(Inbox::default()),
            inbox_list: ListState::new(0, ListAlignment::Top, px(320.)),
            search: RefCell::new(Vec::new()),
            searching: Cell::new(false),
            search_more: Cell::new(false),
            search_scroll: ScrollHandle::new(),
            search_drawn: Cell::new(0),
            composer: RefCell::new(None),
            restoring: Cell::new(false),
            completions: RefCell::new(Completions::default()),
        }
    }

    /// The channel the open thread lives in, empty when none is open.
    pub(crate) fn thread_channel(&self) -> String {
        self.thread_channel.borrow().clone()
    }

    pub(crate) fn attach(&self, composer: Entity<TextareaState>) {
        *self.composer.borrow_mut() = Some(composer);
    }

    pub(crate) fn detach(&self) {
        self.composer.borrow_mut().take();
    }

    pub fn mode(&self, _cx: &App) -> PanelMode {
        self.mode.borrow().clone()
    }

    pub fn set_mode(&self, mode: PanelMode, cx: &mut App) {
        if *self.mode.borrow() != mode {
            // A different thread starts at its newest reply, not wherever the
            // last one was left.
            self.thread.borrow_mut().clear();
            self.thread_loaded.set(false);
            self.list.reset(0);
            self.list.set_follow_mode(FollowMode::Tail);
        }
        *self.mode.borrow_mut() = mode;
        crate::ui::refresh(cx);
    }

    /// Reflects whether the open thread is followed.
    pub fn set_following(&self, following: bool, cx: &mut App) {
        if self.following.replace(following) != following {
            crate::ui::refresh(cx);
        }
    }

    /// A new search starts at its newest hit, wherever the last one was left.
    pub(crate) fn search_from_the_top(&self) {
        self.search_scroll.set_offset(point(px(0.), px(0.)));
    }
}
