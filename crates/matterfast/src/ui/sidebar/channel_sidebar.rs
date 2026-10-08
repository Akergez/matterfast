use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::{
    px, rems, App, Bounds, ListAlignment, ListState, Pixels, Point, Rems, TouchPhase, Window,
};
use mattermost_api::models::Channel;

use super::tab_swipe::TabSwipe;

/// How far across the list a swipe has to go to turn a tab, as a share of
/// its width: further than a finger drifts while scrolling, and well short
/// of a stretch.
const SWIPE_REACH: f32 = 0.15;

/// How tall a conversation's row is: a face, and two lines beside it. In
/// rems, like everything a row is made of: it is as tall as its text needs,
/// at whatever size the text is set.
pub(super) const ROW_HEIGHT: Rems = rems(3.75);

/// How far past the edges of the list rows are kept drawn, so a flick of the
/// wheel does not outrun them.
const OVERDRAW: f32 = 320.;

/// The list of conversations, pane one.
pub struct ChannelSidebar {
    /// Which rows are on screen: a server can have hundreds of channels, and
    /// only the few in view are built for a frame.
    pub(super) list: ListState,
    /// The conversations in the order drawn, by id. The rest is read from
    /// the state when a row is drawn.
    rows: RefCell<Rc<Vec<String>>>,
    /// The size of text the list was last told its rows' height for: a row
    /// is so many rems tall, and the list counts in pixels.
    rem: Cell<Pixels>,
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
            list: ListState::new(0, ListAlignment::Top, px(OVERDRAW)),
            rows: RefCell::new(Rc::new(Vec::new())),
            rem: Cell::new(px(0.)),
            folder: RefCell::new(None),
            inbox: Cell::new(true),
            body: Cell::new(Bounds::default()),
            tab_swipe: Cell::new(TabSwipe::Idle),
            slide: Cell::new(0.0),
            slid: Cell::new(None),
        }
    }

    /// Starts the rows of a tab just turned to off to one side, to come in
    /// from there.
    pub(super) fn slide_from(&self, side: f32, cx: &mut App) {
        self.slide.set(side);
        self.slid.set(None);
        crate::ui::refresh(cx);
    }

    /// Where the rows are for this frame, a step nearer their place. Asks
    /// for another frame until they are in it.
    pub(super) fn advance(&self, window: &mut Window) -> f32 {
        let mut slide = self.slide.get();
        if slide == 0.0 {
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
        width: f32,
    ) -> (bool, Option<i32>) {
        let started = phase == TouchPhase::Started;
        if started && !self.body.get().contains(&position) {
            self.tab_swipe.set(TabSwipe::Idle);
            return (false, None);
        }
        let (swipe, turn) = self.tab_swipe.get().pan(dx, dy, started, width * SWIPE_REACH);
        self.tab_swipe.set(swipe);
        (swipe.taken(), turn)
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

    /// Opens a folder, or every conversation, from the top: the place the
    /// last list was scrolled to means nothing in this one.
    pub(crate) fn set_folder(&self, folder: Option<String>, cx: &mut App) {
        // Choosing a folder is also leaving the inbox, even for the folder
        // that was open underneath it.
        self.show_inbox(false, cx);
        if *self.folder.borrow() == folder {
            return;
        }
        *self.folder.borrow_mut() = folder;
        *self.rows.borrow_mut() = Rc::new(Vec::new());
        self.list.reset(0);
        crate::ui::refresh(cx);
    }

    /// The rows to draw for these conversations. Nearly every frame they are
    /// the ones already held, and nothing is copied; when they are not — a
    /// message moved a conversation to the top — the list is told, and stays
    /// scrolled to where it was. So it is when the text changed size (`rem`)
    /// and every row with it.
    pub(super) fn rows(&self, chats: &[&Channel], rem: Pixels) -> Rc<Vec<String>> {
        let held = self.rows.borrow().clone();
        let same = held.iter().map(String::as_str).eq(chats.iter().map(|chat| chat.id.as_str()));
        if same && self.rem.replace(rem) == rem {
            return held;
        }
        self.rem.set(rem);
        let rows: Rc<Vec<String>> = Rc::new(chats.iter().map(|chat| chat.id.clone()).collect());
        let top = self.list.logical_scroll_top();
        self.list
            .reset_with_uniform_height(rows.len(), ROW_HEIGHT.to_pixels(rem));
        self.list.scroll_to(top);
        *self.rows.borrow_mut() = rows.clone();
        rows
    }
}
