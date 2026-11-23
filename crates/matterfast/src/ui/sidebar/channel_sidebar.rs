use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_adaptive::prim::{
    HorizontalScroll, OffsetSpring, PanSample, VerticalListState, ViewportBounds,
};
use gpui_kit::{px, rems, App, Pixels, Rems, TouchPhase, Window};
use mattermost_api::models::Channel;

use super::tab_swipe::TabSwipe;

/// How tall a conversation's row is: a face, and two lines beside it. In
/// rems, like everything a row is made of: it is as tall as its text needs,
/// at whatever size the text is set.
pub(super) const ROW_HEIGHT: Rems = rems(3.75);

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
    pub(crate) body: ViewportBounds,
    tab_swipe: Cell<TabSwipe>,
    tab_order: Cell<(usize, usize)>,
    /// How far the rows are from their place, in widths of the list: a tab
    /// that was swiped to comes in from the side the finger left for, +1
    /// for the right. Zero when nothing is moving.
    slide: Cell<f32>,
    settle: Cell<Option<OffsetSpring>>,
    last_pan: Cell<Option<(Instant, f32)>>,
    /// The strip of folder tabs, which scrolls along when they do not fit.
    pub(crate) tabs_scroll: HorizontalScroll,
}

impl ChannelSidebar {
    pub fn new() -> Self {
        ChannelSidebar {
            pages: RefCell::new(HashMap::new()),
            folder: RefCell::new(None),
            inbox: Cell::new(true),
            body: ViewportBounds::default(),
            tab_swipe: Cell::new(TabSwipe::Idle),
            tab_order: Cell::new((0, 2)),
            slide: Cell::new(0.0),
            settle: Cell::new(None),
            last_pan: Cell::new(None),
            tabs_scroll: HorizontalScroll::default(),
        }
    }

    /// Where the rows are for this frame, a step nearer their place. Asks
    /// for another frame until they are in it.
    pub(super) fn advance(&self, window: &mut Window) -> f32 {
        if let Some(spring) = self.settle.get() {
            let offset = spring.advance(window);
            self.slide.set(offset);
            if offset == 0.0 {
                self.settle.set(None);
            }
        }
        self.slide.get()
    }

    /// One step of a pan across the list when it is the screen in front.
    /// Says whether the step was taken — it is a swipe between tabs and not
    /// a scroll — and which way to turn, if this step is the one that turns.
    /// `width` is how wide the list is.
    pub(crate) fn swiped(&self, sample: PanSample) -> (bool, Option<i32>) {
        let PanSample { dx, dy, phase } = sample;
        let started = phase == TouchPhase::Started;
        let time = Instant::now();
        if started {
            self.last_pan.set(None);
        }
        let width = f32::from(self.body.bounds().size.width);
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
            let displayed = if offset < minimum {
                minimum + (offset - minimum) * 0.2
            } else if offset > maximum {
                maximum + (offset - maximum) * 0.2
            } else {
                offset
            };
            self.settle.set(None);
            self.slide.set(displayed);
            if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                let velocity = self
                    .last_pan
                    .take()
                    .filter(|(last, _)| time.duration_since(*last) < Duration::from_millis(90))
                    .map_or(0.0, |(_, velocity)| velocity);
                let (swipe, next) = TabSwipe::Tracking(offset.clamp(minimum, maximum))
                    .release_with_velocity(phase == TouchPhase::Cancelled, velocity);
                turn = next
                    .filter(|step| (*step > 0 && minimum < 0.0) || (*step < 0 && maximum > 0.0));
                // Rebase onto the chosen page without changing either page's position.
                let rebased = displayed + turn.unwrap_or(0) as f32;
                self.slide.set(rebased);
                self.settle.set(Some(OffsetSpring::new(rebased, velocity)));
                swipe
            } else {
                if let Some((last, _)) = self.last_pan.get() {
                    let elapsed = time.duration_since(last).as_secs_f32().max(0.008);
                    self.last_pan
                        .set(Some((time, (dx / width / elapsed).clamp(-5.0, 5.0))));
                } else {
                    self.last_pan.set(Some((time, 0.0)));
                }
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
        let (previous, _) = self.tab_order.replace((now, count));
        if previous != now {
            self.tabs_scroll.reveal(now);
        }
    }

    fn tab_order(&self) -> (usize, usize) {
        self.tab_order.get()
    }

    pub(crate) fn swipe_offset(&self) -> f32 {
        self.slide.get() * f32::from(self.body.bounds().size.width)
    }

    pub(crate) fn body_top(&self) -> f32 {
        f32::from(self.body.bounds().origin.y)
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
    ) -> (VerticalListState, Rc<Vec<String>>) {
        let mut pages = self.pages.borrow_mut();
        let page = pages.entry(folder).or_insert_with(|| ChannelPage {
            list: VerticalListState::default(),
            rows: Rc::new(Vec::new()),
            rem: px(0.0),
        });
        let same = page
            .rows
            .iter()
            .map(String::as_str)
            .eq(chats.iter().map(|chat| chat.id.as_str()));
        if !same || page.rem != rem {
            page.rows = Rc::new(chats.iter().map(|chat| chat.id.clone()).collect());
            page.rem = rem;
            page.list.reset(page.rows.len(), Some(ROW_HEIGHT), rem);
        }
        (page.list.clone(), page.rows.clone())
    }
}

struct ChannelPage {
    list: VerticalListState,
    rows: Rc<Vec<String>>,
    rem: Pixels,
}
