use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::component::input::TextareaState;
use gpui_kit::{px, Entity, FollowMode, ListAlignment, ListState};

use crate::ui::autocomplete::Completions;
use crate::ui::chat::feed::FeedItem;
use crate::ui::chat::scroll_trace::scroll_trace_enabled;
use crate::ui::{Action, Ui, WindowSlot};

/// How far past the visible rows the list keeps things laid out, so a flick
/// of the wheel lands on rows that already exist.
const OVERDRAW: gpui_kit::Pixels = px(600.);

/// Rows from the top at which the page before this one is asked for.
const PAGINATE_WITHIN: usize = 4;

/// The conversation pane's state.
pub struct ChatView {
    pub(crate) window: Rc<WindowSlot>,
    /// The scrolling list. It exists without a window — it is only bookkeeping
    /// about rows and where the reader is among them.
    pub(crate) list: ListState,
    pub(crate) items: RefCell<Vec<FeedItem>>,
    /// Which channel the feed currently holds, so a redraw can tell itself
    /// apart from a channel switch.
    pub(crate) showing: RefCell<Option<String>>,
    /// Shown instead of an empty feed while the first page is in flight, so a
    /// slow channel reads as loading rather than as empty.
    pub(crate) loading: Cell<bool>,
    /// Set while the page before this one is fetched.
    pub(crate) loading_older: Cell<bool>,
    /// Edge-trigger for history pagination. A physical approach to the top
    /// produces one request, not one request per scroll tick.
    pub(crate) pagination_armed: Rc<Cell<bool>>,
    pub(crate) typing: RefCell<Vec<String>>,
    /// `None` means connected. Anything else is shown until it is cleared.
    pub(crate) connection: RefCell<Option<String>>,
    pub(crate) member_count: Cell<Option<i64>>,
    /// The agent menu: (target, label) per bot.
    pub(crate) agents: RefCell<Vec<(String, String)>>,
    pub(crate) uploading: Cell<usize>,
    /// The post being edited, when the composer is in edit mode.
    pub(crate) editing: RefCell<Option<String>>,
    /// The priority chosen for the next message: "", "important" or "urgent".
    pub(crate) priority: RefCell<String>,
    pub(crate) completions: RefCell<Completions>,
    pub(crate) calls_available: Cell<bool>,
    pub(crate) calls_reason: RefCell<Option<String>>,
    pub(crate) in_call: Cell<bool>,
    pub(crate) call_ongoing: Cell<bool>,
    pub(crate) call_participants: Cell<Option<usize>>,
    /// The message something just navigated to, marked out for a moment.
    pub(crate) highlight: RefCell<Option<String>>,
    /// The composer, while there is a window to put it in.
    pub(crate) composer: RefCell<Option<Entity<TextareaState>>>,
    /// Set while a draft is being put back, so the change it causes is not
    /// mistaken for the user typing.
    pub(crate) restoring: Cell<bool>,
}

impl ChatView {
    pub fn new(window: Rc<WindowSlot>) -> Self {
        let list = ListState::new(0, ListAlignment::Bottom, OVERDRAW);
        list.set_follow_mode(FollowMode::Tail);
        ChatView {
            window,
            list,
            items: RefCell::new(Vec::new()),
            showing: RefCell::new(None),
            loading: Cell::new(false),
            loading_older: Cell::new(false),
            pagination_armed: Rc::new(Cell::new(true)),
            typing: RefCell::new(Vec::new()),
            connection: RefCell::new(None),
            member_count: Cell::new(None),
            agents: RefCell::new(Vec::new()),
            uploading: Cell::new(0),
            editing: RefCell::new(None),
            priority: RefCell::new(String::new()),
            completions: RefCell::new(Completions::default()),
            calls_available: Cell::new(false),
            calls_reason: RefCell::new(None),
            in_call: Cell::new(false),
            call_ongoing: Cell::new(false),
            call_participants: Cell::new(None),
            highlight: RefCell::new(None),
            composer: RefCell::new(None),
            restoring: Cell::new(false),
        }
    }

    /// Wires the list's scrolling to the session. Done once, when the session
    /// is built: the handler outlives every window.
    pub(crate) fn connect(&self, ui: &Rc<Ui>) {
        let weak = Rc::downgrade(ui);
        let armed = self.pagination_armed.clone();
        self.list.set_scroll_handler(move |event, _, cx| {
            if scroll_trace_enabled() {
                tracing::info!(
                    target: "matterfast::scroll",
                    event = "scrolled",
                    first = event.visible_range.start,
                    last = event.visible_range.end,
                    following = event.is_following_tail,
                    "scroll trace"
                );
            }
            // Away from the top again: the next approach is a new request.
            if event.is_following_tail || event.visible_range.start > PAGINATE_WITHIN * 4 {
                armed.set(true);
            }
            if !event.is_following_tail
                && event.visible_range.start <= PAGINATE_WITHIN
                && armed.replace(false)
            {
                let Some(ui) = weak.upgrade() else { return };
                ui.dispatch(Action::LoadOlder, cx);
            }
        });
    }

    /// Gives the pane its composer. Called when a window is built.
    pub(crate) fn attach(&self, composer: Entity<TextareaState>) {
        *self.composer.borrow_mut() = Some(composer);
    }

    /// The window went away; the composer went with it.
    pub(crate) fn detach(&self) {
        self.composer.borrow_mut().take();
    }
}
