use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::{
    px, rems, App, Bounds, ListAlignment, ListState, Pixels, Point, Rems, TouchPhase, Window,
};
use mattermost_api::models::Channel;

use super::tab_swipe::TabSwipe;

/// How tall a conversation's row is: a face, and two lines beside it. In
/// rems, like everything a row is made of: it is as tall as its text needs,
/// at whatever size the text is set.
pub(super) const ROW_HEIGHT: Rems = rems(3.75);

/// How far past the edges of the list rows are kept drawn, so a flick of the
/// wheel does not outrun them.
const OVERDRAW: f32 = if cfg!(target_os = "android") { 120. } else { 320. };

/// The list of conversations, pane one.
pub struct ChannelSidebar {
    pages: RefCell<HashMap<Option<String>, ChannelPage>>,
    /// The folder the list is narrowed to — a sidebar category's id — or
    /// nothing for every conversation.
    folder: RefCell<Option<String>>,
    /// Whether the inbox is in front instead of a list of conversations: it
    /// is the first tab, before the folders, and what the column shows until
    /// somebody chooses otherwise — what is waiting on the reader is what
    /// they opened the window for.
    inbox: Cell<bool>,
    /// Where the rows under the tabs are in the window: a swipe that starts
    /// there is for the tabs, and one over the tabs themselves scrolls them.
    pub(super) body: Cell<Bounds<Pixels>>,
    tab_swipe: Cell<TabSwipe>,
    tab_order: Cell<(usize, usize)>,
    /// How far the rows are from their place, in widths of the list: a tab
    /// that was swiped to comes in from the side the finger left for, +1
    /// for the right. Zero when nothing is moving.
    slide: Cell<f32>,
    /// When `slide` last moved, which is what its next step is measured
    /// from.
    slid: Cell<Option<std::time::Instant>>,
}

impl ChannelSidebar {
    pub fn new() -> Self {
        ChannelSidebar {
            pages: RefCell::new(HashMap::new()),
            folder: RefCell::new(None),
            inbox: Cell::new(true),
            body: Cell::new(Bounds::default()),
            tab_swipe: Cell::new(TabSwipe::Idle),
            tab_order: Cell::new((0, 2)),
            slide: Cell::new(0.0),
            slid: Cell::new(None),
        }
    }

    /// Where the rows are for this frame, a step nearer their place. Asks
    /// for another frame until they are in it.
    pub(super) fn advance(&self, window: &mut Window) -> f32 {
        let mut slide = self.slide.get();
        if slide == 0.0 || matches!(self.tab_swipe.get(), TabSwipe::Tracking(_)) {
            return slide;
        }
        let now = std::time::Instant::now();
        let elapsed = self
            .slid
            .replace(Some(now))
            .map_or(1.0 / 60.0, |last| (now - last).as_secs_f32())
            .min(0.05);
        // Fast at first and slowing into place, as a page of the window is.
        slide *= (-elapsed / 0.06).exp();
        if slide.abs() < 0.004 {
            slide = 0.0;
            self.slid.set(None);
        }
        self.slide.set(slide);
        window.request_animation_frame();
        slide
    }

    /// One step of a pan across the list when it is the screen in front.
    /// Says whether the step was taken — it is a swipe between tabs and not
    /// a scroll — and which way to turn, if this step is the one that turns.
    /// `width` is how wide the list is.
    pub(crate) fn swiped(
        &self,
        dx: f32,
        dy: f32,
        phase: TouchPhase,
        position: Point<Pixels>,
    ) -> (bool, Option<i32>) {
        let started = phase == TouchPhase::Started;
        if started && !self.body.get().contains(&position) {
            self.tab_swipe.set(TabSwipe::Idle);
            return (false, None);
        }
        let width = f32::from(self.body.get().size.width);
        if width <= 0.0 {
            return (false, None);
        }
        let swipe = self
            .tab_swipe
            .get()
            .pan(dx / width, dy / width, started, self.slide.get());
        let mut turn = None;
        let swipe = if let TabSwipe::Tracking(offset) = swipe {
            let (now, count) = self.tab_order();
            let minimum = if now + 1 < count { -1.0 } else { 0.0 };
            let maximum = if now > 0 { 1.0 } else { 0.0 };
            let offset = offset.clamp(minimum, maximum);
            self.slide.set(offset);
            if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                let (swipe, next) =
                    TabSwipe::Tracking(offset).release(phase == TouchPhase::Cancelled);
                turn = next;
                // Rebase onto the chosen page without changing either page's position.
                self.slide.set(offset + next.unwrap_or(0) as f32);
                self.slid.set(None);
                swipe
            } else {
                TabSwipe::Tracking(offset)
            }
        } else {
            swipe
        };
        self.tab_swipe.set(swipe);
        (swipe.taken(), turn)
    }

    /// The tab order is supplied by the sidebar before dispatching a gesture.
    pub(super) fn set_tab_order(&self, now: usize, count: usize) {
        self.tab_order.set((now, count));
    }

    fn tab_order(&self) -> (usize, usize) {
        self.tab_order.get()
    }

    pub(crate) fn swipe_offset(&self) -> f32 {
        self.slide.get() * f32::from(self.body.get().size.width)
    }

    pub(crate) fn body_top(&self) -> f32 {
        f32::from(self.body.get().origin.y)
    }

    pub(crate) fn showing_inbox(&self) -> bool {
        self.inbox.get()
    }

    /// Puts the inbox in front, or takes it away again and leaves the folder
    /// that was open before it.
    pub(crate) fn show_inbox(&self, show: bool, cx: &mut App) {
        if self.inbox.replace(show) != show {
            crate::ui::refresh(cx);
        }
    }

    /// The list is drawn from the state every frame, so all a refresh has to
    /// do is ask for a frame.
    #[track_caller]
    pub fn refresh(&self, cx: &mut App) {
        crate::ui::refresh(cx);
    }

    pub(crate) fn folder(&self) -> Option<String> {
        self.folder.borrow().clone()
    }

    /// Opens a folder or every conversation, retaining each list's scroll position.
    pub(crate) fn set_folder(&self, folder: Option<String>, cx: &mut App) {
        // Choosing a folder is also leaving the inbox, even for the folder
        // that was open underneath it.
        self.show_inbox(false, cx);
        if *self.folder.borrow() == folder {
            return;
        }
        *self.folder.borrow_mut() = folder;

        crate::ui::refresh(cx);
    }

    /// Each category keeps its own list while two pages are visible during a drag.
    pub(super) fn rows(
        &self,
        folder: Option<String>,
        chats: &[&Channel],
        rem: Pixels,
    ) -> (ListState, Rc<Vec<String>>) {
        let mut pages = self.pages.borrow_mut();
        let page = pages.entry(folder).or_insert_with(|| ChannelPage {
            list: ListState::new(0, ListAlignment::Top, px(OVERDRAW)),
            rows: Rc::new(Vec::new()),
            rem: px(0.0),
        });
        let same = page
            .rows
            .iter()
            .map(String::as_str)
            .eq(chats.iter().map(|chat| chat.id.as_str()));
        if !same || page.rem != rem {
            let top = page.list.logical_scroll_top();
            page.rows = Rc::new(chats.iter().map(|chat| chat.id.clone()).collect());
            page.rem = rem;
            page.list
                .reset_with_uniform_height(page.rows.len(), ROW_HEIGHT.to_pixels(rem));
            page.list.scroll_to(top);
        }
        (page.list.clone(), page.rows.clone())
    }
}

struct ChannelPage {
    list: ListState,
    rows: Rc<Vec<String>>,
    rem: Pixels,
}
