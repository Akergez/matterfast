use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::avatars::Avatars;
use crate::notifications::Notifier;
use crate::runtime;
use crate::state::SharedState;
use crate::ui::call_dock::CallDock;
use crate::ui::chat::ChatView;
use crate::ui::constants::AVATAR_REDRAW_WAIT;
use crate::ui::layout::{Overlay, Split, Widths, WindowSlot};
use crate::ui::rhs::RightPanel;
use crate::ui::sidebar::ChannelSidebar;
use crate::ui::{autocomplete, lightbox, media, search};
use crate::video;

/// The session.
pub struct Ui {
    pub(crate) window: Rc<WindowSlot>,
    pub state: SharedState,
    pub channels: ChannelSidebar,
    pub dock: CallDock,
    pub chat: ChatView,
    pub right: RightPanel,
    pub overlay: Overlay,
    pub split: Split,
    pub widths: Widths,
    pub search_box: search::SearchBox,
    /// Remote screens and cameras, keyed by the media session they belong to
    /// and in the order they arrived.
    pub(crate) video_views: RefCell<Vec<(String, video::RemoteView)>>,
    /// Handles already looked up on the server, found or not, so that a
    /// mention of nobody is asked about once rather than on every frame.
    pub(crate) asked_handles: RefCell<HashSet<String>>,
    pub avatars: Avatars,
    pub(crate) notifier: Notifier,
    /// Set while the window is too narrow for a static thread column.
    pub(crate) narrow: Cell<bool>,
    /// The window's scale factor, as last drawn. Decides which size of an
    /// attached image is worth fetching.
    pub(crate) scale: Cell<f32>,
    pub(crate) sidebar_reload_pending: Cell<bool>,
    pub(crate) typing_sweep_pending: Cell<bool>,
    pub(crate) draft_save_pending: Cell<bool>,
    pub(crate) thread_draft_pending: Cell<bool>,
    pub(crate) loading_older: Cell<bool>,
    pub(crate) snapshot_pending: Cell<bool>,
    pub(crate) pruned_at: Cell<Option<std::time::Instant>>,
    /// The local message store, once it has opened.
    pub(crate) store: RefCell<Option<crate::store::Store>>,
    pub(crate) typing_sent_recently: Cell<bool>,
    /// Bumped on every completion query, so a slow answer for a term the
    /// person has already typed past is discarded rather than replacing the
    /// list under them.
    pub(crate) completion_generation: Cell<u64>,
    /// Set while a redraw for freshly-landed pictures is already booked.
    pub(crate) avatar_redraw_pending: Cell<bool>,
    /// Set while a debounced `@mention` network lookup is scheduled; further
    /// keystrokes just overwrite `mention_query` instead of scheduling again.
    pub(crate) mention_query_pending: Cell<bool>,
    /// The term/team/channel/client the pending lookup above will use — always
    /// the latest keystroke's, not the one that started the debounce.
    pub(crate) mention_query: RefCell<Option<(String, String, String, mattermost_api::Client)>>,
    /// The candidates currently shown in the completion list, kept around
    /// so a picture landing later can redraw them without asking again.
    pub(crate) last_completions: RefCell<Vec<autocomplete::Candidate>>,
    /// The inline players for attached video and audio, by file id.
    pub(crate) players: RefCell<HashMap<String, Rc<media::Player>>>,
    /// Videos a still has already been attempted for, so a file that gives
    /// none is not decoded again on every redraw.
    pub(crate) stills_tried: RefCell<HashSet<String>>,
    /// The image being looked at full size, if one is.
    pub(crate) lightbox: RefCell<Option<lightbox::Open>>,
}

impl Ui {
    /// Builds a session around a state. Nothing is fetched yet; see
    /// [`bootstrap`](crate::ui::bootstrap).
    pub fn new(state: SharedState, notifier: Notifier) -> Rc<Self> {
        let window = Rc::new(WindowSlot::default());
        let avatars = Avatars::new(state.borrow().client.clone());
        let ui = Rc::new(Ui {
            channels: ChannelSidebar,
            dock: CallDock::new(),
            chat: ChatView::new(window.clone()),
            right: RightPanel::new(window.clone()),
            overlay: Overlay::default(),
            split: Split::default(),
            widths: Widths::default(),
            search_box: search::SearchBox::new(window.clone()),
            video_views: RefCell::new(Vec::new()),
            asked_handles: RefCell::new(HashSet::new()),
            avatars: avatars.clone(),
            notifier,
            narrow: Cell::new(false),
            scale: Cell::new(1.0),
            sidebar_reload_pending: Cell::new(false),
            typing_sweep_pending: Cell::new(false),
            draft_save_pending: Cell::new(false),
            thread_draft_pending: Cell::new(false),
            loading_older: Cell::new(false),
            snapshot_pending: Cell::new(false),
            pruned_at: Cell::new(None),
            store: RefCell::new(None),
            typing_sent_recently: Cell::new(false),
            completion_generation: Cell::new(0),
            avatar_redraw_pending: Cell::new(false),
            mention_query_pending: Cell::new(false),
            mention_query: RefCell::new(None),
            last_completions: RefCell::new(Vec::new()),
            players: RefCell::new(HashMap::new()),
            stills_tried: RefCell::new(HashSet::new()),
            lightbox: RefCell::new(None),
            window,
            state,
        });
        ui.chat.connect(&ui);

        // A picture landing changes what a row draws, and nothing else does
        // the redraw for it. Debounced, because a screenful of faces arrives
        // as a burst and one frame is as good as twenty.
        avatars.connect_loaded({
            let weak = Rc::downgrade(&ui);
            move |key, cx| {
                let Some(ui) = weak.upgrade() else { return };
                if !key.contains(':') {
                    ui.refresh_completion_avatars(cx);
                }
                if ui.avatar_redraw_pending.replace(true) {
                    return;
                }
                runtime::after(AVATAR_REDRAW_WAIT, move |cx| {
                    ui.avatar_redraw_pending.set(false);
                    cx.refresh_windows();
                });
            }
        });
        ui
    }
}
