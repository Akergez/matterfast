//! The session: everything the application does, and the single place where
//! state changes are applied.
//!
//! The panes do not mutate state directly. They dispatch an [`Action`], which
//! is queued and applied once whatever was running has finished. That keeps
//! the borrow of `RefCell<AppState>` short and obviously non-overlapping, which
//! is otherwise the standard way an `Rc<RefCell<…>>` application panics at
//! runtime.
//!
//! [`Ui`] is the session. It outlives its window: closing the window while the
//! application keeps running in the background drops what was on screen and
//! nothing else, so the socket, the call and the notifications carry on, and a
//! new window attaches to the same session. For that reason nothing in here
//! holds a window — it asks for one, through [`WindowSlot`], when it needs to
//! do something only a window can.

mod account;
mod autocomplete;
mod call_dock;
mod chat;
mod dialogs;
mod interactive;
mod kit;
mod lightbox;
pub mod login;
mod media;
mod message;
mod notify;
mod profile;
mod rhs;
mod script;
mod search;
mod settings;
mod shell;
mod sidebar;
pub mod sso;
mod storage;
mod switcher;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::notification::Notification;
use gpui_kit::component::WindowExt;
use gpui_kit::{AnyWindowHandle, App, ClickEvent, ClipboardItem, RenderImage, Window};
use mattermost_api::bootstrap::Bootstrap;
use mattermost_api::models::*;
use mattermost_api::ws::{Event, WebSocket, WsUpdate};
use mattermost_calls::{CallSession, CallUpdate, JoinOptions};

use crate::audio::AudioIo;
use crate::avatars::Avatars;
use crate::notifications::{Notice, Notifier};
use crate::runtime;
use crate::state::{ActiveCall, AppState, ChannelFeed, SharedState};
use crate::timefmt::{format_day, format_time, now_ms, unique};
use crate::video;
use call_dock::CallDock;
use chat::ChatView;
use kit::Lucide;
use message::PostAction;
use rhs::{PanelMode, RightPanel};
use sidebar::{ChannelSidebar, RowAction};

pub use shell::{current, handle_request, init};

/// How long the server is left out of a burst of typing. Short enough that
/// the extra names arrive while the eye is still on the list, long enough
/// that a whole word costs one request.
const MENTION_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(180);

/// The websocket prefix of <https://github.com/Toxblh/mattermost-reactions-notify-plugin>,
/// which notifies you about reactions to your own posts.
const REACTION_NOTIFY_PREFIX: &str = "custom_ru.toxblh.reactions-notify_";

/// Mentions answerable from memory: anyone whose handle or name *contains*
/// what has been typed.
///
/// Substring, not prefix — people search by surname ("fomche" for Semyon
/// Fomchenkov) far more often than by the start of a handle. Ranked so the
/// prefix matches still come first, because when the prefix is what was meant
/// it is nearly always the one wanted.
///
/// The scan is linear over the directory: username, nickname, first and last
/// name are each lowercased and searched independently, because the server's
/// teammate-name-display setting only picks what is *shown* — a server set to
/// show bare usernames still has to be searchable by surname.
/// `picture` is asked for a face only for the handful of people that survive
/// the filter — it is a side effect (a missing avatar starts a download), so
/// it must not run for the whole directory, and tests pass one that does
/// nothing.
fn local_mentions<'a>(
    users: impl Iterator<Item = &'a User>,
    lowered: &str,
    display: &str,
    picture: &dyn Fn(&str) -> Option<Arc<RenderImage>>,
) -> Vec<autocomplete::Candidate> {
    /// How the match was made, and therefore how it sorts.
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    enum Rank {
        HandlePrefix,
        NamePrefix,
        Contains,
    }

    let mut found: Vec<(Rank, String, &User)> = Vec::new();
    for user in users {
        let handle = user.username.to_lowercase();
        let nickname = user.nickname.to_lowercase();
        let first = user.first_name.to_lowercase();
        let last = user.last_name.to_lowercase();
        // What the row shows respects the display setting; what it is found
        // by does not.
        let shown = user.display_name(display);

        let name_prefix = nickname.starts_with(lowered)
            || first.starts_with(lowered)
            || last.starts_with(lowered);
        let name_contains =
            nickname.contains(lowered) || first.contains(lowered) || last.contains(lowered);

        let rank = if handle.starts_with(lowered) {
            Rank::HandlePrefix
        } else if name_prefix {
            Rank::NamePrefix
        } else if handle.contains(lowered) || name_contains {
            Rank::Contains
        } else {
            continue;
        };
        found.push((rank, shown, user));
    }

    // Sorted rather than truncated early: a substring match found first must
    // not push out a prefix match found later.
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    found
        .into_iter()
        .take(COMPLETIONS)
        .map(|(_, name, user)| autocomplete::Candidate {
            insert: format!("@{}", user.username),
            primary: name,
            secondary: format!("@{}", user.username),
            emoji: None,
            image: picture(&user.id),
            user_id: Some(user.id.clone()),
        })
        .collect()
}

/// The `@names` a message mentions, by the same rule the renderer uses.
fn mentioned_names(message: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = message;
    while let Some(at) = rest.find('@') {
        // Mid-word, so it is an email address rather than a mention.
        let preceded = rest[..at].chars().next_back();
        rest = &rest[at + 1..];
        if preceded.is_some_and(|c| c.is_alphanumeric()) {
            continue;
        }
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '-' | '_')))
            .unwrap_or(rest.len());
        if end > 0 {
            names.push(rest[..end].trim_end_matches('.').to_string());
        }
        rest = &rest[end..];
    }
    names
}

#[cfg(test)]
mod mention_tests {
    use super::{local_mentions, mentioned_names, COMPLETIONS};
    use mattermost_api::models::User;

    /// A directory the size of a large company, to check that answering from
    /// memory stays instant. The claim is "under a frame"; this asserts an
    /// order of magnitude below that, so it fails long before anyone notices.
    #[test]
    fn ten_thousand_users_complete_instantly() {
        let users: Vec<User> = (0..10_000)
            .map(|i| User {
                id: format!("u{i}"),
                username: format!("person{i}"),
                first_name: "Person".into(),
                last_name: i.to_string(),
                ..Default::default()
            })
            .collect();

        let started = std::time::Instant::now();
        let found = local_mentions(users.iter(), "person9", "full_name", &|_| None);
        let elapsed = started.elapsed();

        assert_eq!(found.len(), COMPLETIONS);
        assert!(
            elapsed < std::time::Duration::from_millis(150),
            "took {elapsed:?} for 10k users"
        );

        // The worst case is a term nobody matches: every user is examined and
        // the early exit never fires.
        let started = std::time::Instant::now();
        let none = local_mentions(users.iter(), "nobodyatall", "full_name", &|_| None);
        let elapsed = started.elapsed();
        assert!(none.is_empty());
        // Both bounds are generous on purpose: this is an unoptimised build
        // on a machine that may be compiling something else at the same time,
        // and the old 10ms/30ms failed for that reason rather than for a slow
        // scan. What they guard against is an accidental quadratic, which
        // would be seconds rather than milliseconds.
        assert!(
            elapsed < std::time::Duration::from_millis(150),
            "worst case took {elapsed:?} for 10k users"
        );
    }

    #[test]
    fn matches_inside_a_name_not_just_the_start() {
        let users = [
            User {
                id: "u1".into(),
                username: "fomchenkovsv".into(),
                first_name: "Semyon".into(),
                last_name: "Fomchenkov".into(),
                ..Default::default()
            },
            User {
                id: "u2".into(),
                username: "fedorov".into(),
                first_name: "Fedor".into(),
                ..Default::default()
            },
        ];

        // Searching by surname, which is what people actually do.
        let found = local_mentions(users.iter(), "fomche", "full_name", &|_| None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].insert, "@fomchenkovsv");
        assert_eq!(found[0].primary, "Semyon Fomchenkov");

        // A prefix match outranks a substring one even when it is found later.
        let found = local_mentions(users.iter(), "fe", "full_name", &|_| None);
        assert_eq!(found[0].insert, "@fedorov");
    }

    #[test]
    fn finds_by_surname_even_when_the_server_shows_usernames() {
        let users = [User {
            id: "u1".into(),
            username: "ivan42".into(),
            first_name: "Семён".into(),
            last_name: "Фомченко".into(),
            ..Default::default()
        }];

        // The handle has nothing in common with the surname, and the display
        // setting is "username" — so a full_name search would have found
        // nothing, and only searching first/last name directly finds this.
        let found = local_mentions(users.iter(), "фомче", "username", &|_| None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].insert, "@ivan42");
        // The row still shows the configured display (username), not the
        // field it was actually found by.
        assert_eq!(found[0].primary, "ivan42");
    }

    #[test]
    fn finds_mentions_and_ignores_addresses() {
        assert_eq!(mentioned_names("hi @anna and @bob"), ["anna", "bob"]);
        // An email is not a mention, and neither is a trailing full stop.
        assert_eq!(mentioned_names("mail me at a@b.com"), Vec::<String>::new());
        assert_eq!(mentioned_names("ask @anna."), ["anna"]);
        assert_eq!(mentioned_names("nothing here"), Vec::<String>::new());
        assert_eq!(mentioned_names("@"), Vec::<String>::new());
    }
}

/// The entries in the two header menus.
#[derive(Debug, Clone, Copy)]
pub enum MenuAction {
    NewChannel,
    BrowseChannels,
    AccountNotifications,
    EditProfile,
    CustomStatus,
    QuickSwitch,
    SignOut,
    ScheduledPosts,
    ChannelMembers,
    ChannelBookmarks,
    BrowseTeams,
    NewCategory,
    LeaveTeam,
    FocusSearch,
    OpenInbox,
    NextUnread,
    PreviousUnread,
    EditChannel,
    ArchiveChannel,
    ChannelNotifications,
    LeaveChannel,
    PinnedPosts,
    Storage,
    Settings,
}

/// How many rows the quick switcher offers per source. Long enough to find
/// what you meant, short enough to stay a keyboard shortcut.
const QUICK_SWITCH_ROWS: usize = 10;

/// How many completion candidates to offer. More than this and the popover is
/// a list to read rather than a shortcut.
const COMPLETIONS: usize = 8;

/// How many messages to pull when a channel is first opened.
const INITIAL_POSTS: u32 = 60;
/// How many mentions and threads to keep in the inbox.
const INBOX_PAGE: u32 = 25;

pub enum Action {
    SelectTeam(String),
    SelectChannel(String),
    Send(String),
    /// Join the current channel's call, or leave the one we are in.
    ToggleCall,
    /// Turn our own microphone on or off.
    ToggleMute,
    /// Start or stop the server-side recording.
    ToggleRecording,
    /// Start or stop sharing the screen.
    ToggleScreen,
    /// Start or stop the camera.
    ToggleCamera,
    /// Open the thread rooted at this post id in the right panel.
    OpenThread(String),
    /// Reply into the currently open thread.
    ReplyInThread(String),
    /// Show the mentions and threads inbox.
    OpenInbox,
    CloseRightPanel,
    /// Toggle our own reaction: post id, emoji name.
    ToggleReaction(String, String),
    /// A button on a card was pressed, or `selected` was picked from one of
    /// its menus. The cookie is the action's own, set on ephemeral posts.
    CardAction {
        post_id: String,
        action_id: String,
        selected: String,
        cookie: String,
    },
    /// Jump to a channel, and into a thread when the second field is set.
    OpenPost(String, String),
    /// Open a channel and put a specific message on screen.
    JumpToPost(String, String),
    /// Open (or create) the direct-message channel with a user.
    OpenDirectMessage(String),
    /// Jump to the channel whose call we are in.
    OpenCallChannel,
    /// The composer's contents changed; the flag says whether it is non-empty.
    ComposerChanged(bool),
    /// Something from a message's own menu.
    Post(String, PostAction),
    /// Search this team's messages.
    Search(String),
    /// The next page of the hits already showing.
    SearchMore,
    /// Ask the server who the name typed after `from:` could be.
    SearchPeople,
    /// Open the file chooser to attach something.
    PickAttachment,
    /// Ask the LLM agent to summarise what is unread here.
    SummariseUnreads,
    /// Set your own presence.
    SetStatus(String),
    /// Something from a channel row's own menu.
    Row(String, sidebar::RowAction),
    /// Raise or lower your hand in the call.
    ToggleHand,
    /// Do something to another participant, as the call's host.
    HostControl(String, call_dock::HostAction),
    /// React in the call: emoji name and the glyph to show.
    CallReaction(String, String),
    /// Drop an uploaded file before it is sent.
    DropAttachment(String),
    /// Files arrived by drag and drop.
    AttachFiles(Vec<std::path::PathBuf>),
    /// The composer wants candidates for the token under the cursor.
    Complete(Option<autocomplete::Query>),
    /// Send what is in the composer at a chosen time instead of now.
    ScheduleMessage,
    /// The thread panel's reply box changed.
    ThreadDraftChanged,
    /// The reader reached the top of the feed and wants what came before.
    LoadOlder,
    /// Follow or unfollow the thread the panel is showing.
    FollowThread(bool),
}

/// How long a landed picture waits for the rest of its flock before the
/// window is redrawn. Long enough to catch a screenful of faces arriving
/// together, short enough that nobody watches an avatar appear.
const AVATAR_REDRAW_WAIT: std::time::Duration = std::time::Duration::from_millis(120);

/// How long an attached file is kept on disk once fetched. Upload ids are
/// immutable; the global size limit is the normal eviction mechanism, while
/// this prevents abandoned entries living forever.
const FILE_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 60 * 60);

/// The icon for a channel in a flat list, where there is no "#" column.
fn channel_icon(channel: &Channel) -> Lucide {
    match channel.r#type {
        ChannelType::Open => Lucide::Hash,
        ChannelType::Private => Lucide::Lock,
        ChannelType::Direct => Lucide::User,
        _ => Lucide::Users,
    }
}

/// The window a session is shown in, when it is shown in one.
///
/// The session outlives its window, so everything that needs a window — a
/// toast, the composer's text, the title — goes through here and quietly does
/// nothing while there is none.
#[derive(Default)]
pub struct WindowSlot(Cell<Option<AnyWindowHandle>>);

impl WindowSlot {
    pub fn set(&self, window: Option<AnyWindowHandle>) {
        self.0.set(window);
    }

    /// Runs `f` with the window, if there is one.
    ///
    /// Session code only ever runs between window updates — from a finished
    /// request, a timer, or a queued action — never inside one, which is what
    /// makes asking for the window here sound. A caller that breaks that rule
    /// finds the window already lent out; that is logged rather than hidden,
    /// because the symptom would otherwise be a click that does nothing.
    pub fn update<R>(
        &self,
        cx: &mut App,
        f: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> Option<R> {
        let handle = self.0.get()?;
        match handle.update(cx, |_, window, cx| f(window, cx)) {
            Ok(result) => Some(result),
            Err(error) => {
                if cx.windows().contains(&handle) {
                    tracing::error!(%error, "the window was asked for while it was busy");
                } else {
                    // Closed since we last looked.
                    self.0.set(None);
                }
                None
            }
        }
    }
}

/// Whether the right-hand panel is showing, and how.
#[derive(Default)]
pub struct Overlay {
    shown: Cell<bool>,
    /// Laid over the conversation rather than beside it.
    collapsed: Cell<bool>,
}

impl Overlay {
    pub fn set_show_sidebar(&self, shown: bool, cx: &mut App) {
        self.shown.set(shown);
        cx.refresh_windows();
    }

    pub fn set_collapsed(&self, collapsed: bool, cx: &mut App) {
        if self.collapsed.replace(collapsed) != collapsed {
            cx.refresh_windows();
        }
    }

    pub fn shown(&self) -> bool {
        self.shown.get()
    }
}

/// The channel list and the conversation: side by side when there is room,
/// two pages of a stack when there is not.
pub struct Split {
    /// When collapsed, which page is in front. A chat app should land on the
    /// conversation, not the sidebar.
    show_content: Cell<bool>,
    collapsed: Cell<bool>,
}

impl Default for Split {
    fn default() -> Self {
        Split {
            show_content: Cell::new(true),
            collapsed: Cell::new(false),
        }
    }
}

impl Split {
    pub fn set_show_content(&self, show: bool, cx: &mut App) {
        if self.show_content.replace(show) != show {
            cx.refresh_windows();
        }
    }

    pub fn show_content(&self) -> bool {
        self.show_content.get()
    }
}

/// Which of the two dividers between the panes: the one after the channel
/// list, or the one before the thread panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Divider {
    Sidebar,
    Panel,
}

impl Divider {
    fn key(self) -> &'static str {
        match self {
            Divider::Sidebar => "sidebar_width",
            Divider::Panel => "panel_width",
        }
    }
}

/// How wide the person dragged the side columns, in pixels; nothing for a
/// column left at its share of the window. It is about this machine's screen,
/// so it is kept with the other settings of the machine, and read once: the
/// settings are a file, and a width is asked for on every frame.
pub struct Widths {
    sidebar: Cell<Option<f32>>,
    panel: Cell<Option<f32>>,
    /// A drag moved a divider and the file does not know yet. Written when
    /// the drag is over rather than on every pixel of it.
    unsaved: Cell<bool>,
}

impl Default for Widths {
    fn default() -> Self {
        let stored = |divider: Divider| {
            crate::background::setting(divider.key())
                .and_then(|value| value.as_f64())
                .map(|width| width as f32)
                .filter(|width| width.is_finite() && *width > 0.0)
        };
        Widths {
            sidebar: Cell::new(stored(Divider::Sidebar)),
            panel: Cell::new(stored(Divider::Panel)),
            unsaved: Cell::new(false),
        }
    }
}

impl Widths {
    fn cell(&self, divider: Divider) -> &Cell<Option<f32>> {
        match divider {
            Divider::Sidebar => &self.sidebar,
            Divider::Panel => &self.panel,
        }
    }

    pub fn get(&self, divider: Divider) -> Option<f32> {
        self.cell(divider).get()
    }

    /// `None` puts the column back to its share of the window.
    pub fn set(&self, divider: Divider, width: Option<f32>, cx: &mut App) {
        if self.cell(divider).replace(width) != width {
            self.unsaved.set(true);
            cx.refresh_windows();
        }
    }

    /// Writes what a drag changed, once the drag has let go.
    pub fn save_when_settled(&self, cx: &App) {
        if cx.has_active_drag() || !self.unsaved.replace(false) {
            return;
        }
        for divider in [Divider::Sidebar, Divider::Panel] {
            let value = match self.get(divider) {
                Some(width) => serde_json::json!(width.round()),
                None => serde_json::Value::Null,
            };
            crate::background::set_setting(divider.key(), value);
        }
    }
}

/// How many hits a page of a search is.
const SEARCH_PAGE: u32 = 60;

/// The session.
pub struct Ui {
    window: Rc<WindowSlot>,
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
    video_views: RefCell<Vec<(String, video::RemoteView)>>,
    /// Handles already looked up on the server, found or not, so that a
    /// mention of nobody is asked about once rather than on every frame.
    asked_handles: RefCell<std::collections::HashSet<String>>,
    pub avatars: Avatars,
    notifier: Notifier,
    /// Set while the window is too narrow for a static thread column.
    narrow: Cell<bool>,
    /// The window's scale factor, as last drawn. Decides which size of an
    /// attached image is worth fetching.
    scale: Cell<f32>,
    sidebar_reload_pending: Cell<bool>,
    typing_sweep_pending: Cell<bool>,
    draft_save_pending: Cell<bool>,
    thread_draft_pending: Cell<bool>,
    loading_older: Cell<bool>,
    snapshot_pending: Cell<bool>,
    pruned_at: Cell<Option<std::time::Instant>>,
    /// The local message store, once it has opened.
    store: RefCell<Option<crate::store::Store>>,
    typing_sent_recently: Cell<bool>,
    /// Bumped on every completion query, so a slow answer for a term the
    /// person has already typed past is discarded rather than replacing the
    /// list under them.
    completion_generation: Cell<u64>,
    /// Set while a redraw for freshly-landed pictures is already booked.
    avatar_redraw_pending: Cell<bool>,
    /// Set while a debounced `@mention` network lookup is scheduled; further
    /// keystrokes just overwrite `mention_query` instead of scheduling again.
    mention_query_pending: Cell<bool>,
    /// The term/team/channel/client the pending lookup above will use — always
    /// the latest keystroke's, not the one that started the debounce.
    mention_query: RefCell<Option<(String, String, String, mattermost_api::Client)>>,
    /// The candidates currently shown in the completion list, kept around
    /// so a picture landing later can redraw them without asking again.
    last_completions: RefCell<Vec<autocomplete::Candidate>>,
    /// The inline players for attached video and audio, by file id.
    players: RefCell<HashMap<String, Rc<media::Player>>>,
    /// Videos a still has already been attempted for, so a file that gives
    /// none is not decoded again on every redraw.
    stills_tried: RefCell<HashSet<String>>,
    /// The image being looked at full size, if one is.
    lightbox: RefCell<Option<lightbox::Open>>,
}

impl Ui {
    /// Builds a session around a state. Nothing is fetched yet; see
    /// [`bootstrap`].
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
            asked_handles: RefCell::new(std::collections::HashSet::new()),
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

    /// Runs `f` once whatever is running now has finished. This is how a
    /// click, which happens in the middle of the window drawing itself,
    /// reaches session code that may want that window.
    pub fn later(self: &Rc<Self>, cx: &mut App, f: impl FnOnce(&Rc<Ui>, &mut App) + 'static) {
        let ui = self.clone();
        cx.defer(move |cx| f(&ui, cx));
    }

    /// A click handler that runs `f` with the session, later.
    pub fn click(
        self: &Rc<Self>,
        f: impl Fn(&Rc<Ui>, &mut App) + 'static,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let ui = self.clone();
        let f = Rc::new(f);
        move |_, _, cx| {
            let ui = ui.clone();
            let f = f.clone();
            cx.defer(move |cx| f(&ui, cx));
        }
    }

    /// Runs `f` with the window, if there is one.
    pub fn with_window<R>(
        &self,
        cx: &mut App,
        f: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> Option<R> {
        self.window.update(cx, f)
    }

    /// The window's scale factor, as last drawn.
    pub fn scale_factor(&self) -> f32 {
        self.scale.get()
    }

    /// Whether the window is in front of the person right now.
    fn window_is_active(&self, cx: &mut App) -> bool {
        self.window
            .update(cx, |window, _| window.is_window_active())
            .unwrap_or(false)
    }

    /// Whether anything is uploaded and waiting to go out with a message.
    pub fn has_pending_files(&self) -> bool {
        !self.state.borrow().pending_files.is_empty()
    }

    /// A clicked link: a person, a message on this server, or a web page.
    pub fn follow_link(self: &Rc<Self>, url: &str, cx: &mut App) {
        match message::link_target(url) {
            message::LinkTarget::Profile(handle) => self.show_profile_by_handle(&handle, cx),
            message::LinkTarget::Permalink(post_id) => self.open_permalink(post_id, cx),
            message::LinkTarget::Web(url) => cx.open_url(&url),
        }
    }

    /// Raises a desktop notification for a message, tagged by its channel.
    fn notify_message(&self, channel_id: &str, title: &str, body: &str) {
        self.notifier.show(Notice {
            tag: notify::message_tag(channel_id),
            title: title.to_string(),
            body: body.to_string(),
            urgent: false,
            actions: Vec::new(),
        });
    }

    /// Asks which emoji, from the whole table, and reacts with it.
    pub fn pick_reaction(self: &Rc<Self>, post_id: String, cx: &mut App) {
        let ui = self.clone();
        dialogs::pick_emoji(self, cx, move |name, cx| {
            ui.dispatch(Action::ToggleReaction(post_id.clone(), name), cx)
        });
    }

    /// Fetches an attached file, from the disk cache when it is there. The
    /// file is behind the session token, so it is fetched here rather than
    /// handed to anything else as a URL.
    fn fetch_file(
        &self,
        file_id: String,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, mattermost_api::Error>> + Send + 'static
    {
        let client = self.state.borrow().client.clone();
        let resources = self.avatars.resources();
        async move {
            let key = format!("original:{file_id}");
            resources
                .get_or_fetch(key, FILE_CACHE_TTL, || async move {
                    client.download_file(&file_id).await
                })
                .await
        }
    }

    /// Opens the full-size image over the window. A lightbox rather than a
    /// second window: a picture is something you glance at and dismiss, not
    /// something to manage in the window list.
    pub fn open_image(self: &Rc<Self>, file: &FileInfo, cx: &mut App) {
        let _ = cx;
        let fetch = self.fetch_file(file.id.clone());
        let file = file.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                let bytes = fetch.await.map_err(|e| e.to_string())?;
                let decoded = bytes.clone();
                // Decoding a photograph is tens of milliseconds, which is a
                // few frames nobody should have to watch stall.
                let picture = tokio::task::spawn_blocking(move || {
                    image::load_from_memory(&decoded)
                        .map(|picture| crate::avatars::render_image(picture.into_rgba8()))
                        .map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())??;
                Ok::<_, String>((picture, bytes))
            },
            move |result, cx| match result {
                Ok((picture, bytes)) => lightbox::show(&ui, &file, Arc::new(picture), bytes, cx),
                Err(e) => ui.toast(&format!("Could not open that image: {e}"), cx),
            },
        );
    }

    /// Downloads an attachment to wherever the person says.
    pub fn save_attachment(self: &Rc<Self>, file: &FileInfo, cx: &mut App) {
        let fetch = self.fetch_file(file.id.clone());
        let directory = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        let chosen = cx.prompt_for_new_path(&directory, Some(&file.name));
        let ui = self.clone();
        cx.spawn(async move |cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            cx.update(|_| {
                runtime::spawn(
                    async move {
                        let bytes = fetch.await.map_err(|e| e.to_string())?;
                        tokio::fs::write(&path, bytes)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |result, cx| match result {
                        Ok(()) => ui.toast("Saved.", cx),
                        Err(e) => {
                            tracing::warn!(error = %e, "could not save the attachment");
                            ui.toast(&format!("Could not save it: {e}"), cx);
                        }
                    },
                );
            });
        })
        .detach();
    }

    /// Something was pressed on one of our notifications.
    pub(super) fn notification_pressed(
        self: &Rc<Self>,
        tag: &str,
        action: Option<&str>,
        cx: &mut App,
    ) {
        match (notify::pressed(tag), action) {
            (Some(notify::Pressed::Call(channel_id)), Some(notify::DISMISS_CALL)) => {
                self.stop_ringing(cx);
                // In a DM there is one person waiting, and declining tells
                // them; anywhere else there is nobody to tell, so it is only
                // silenced for us and our other devices.
                let (client, direct) = {
                    let st = self.state.borrow();
                    let direct = st
                        .channel(channel_id)
                        .is_some_and(|c| matches!(c.r#type, ChannelType::Direct));
                    (st.client.clone(), direct)
                };
                let channel_id = channel_id.to_string();
                runtime::spawn(
                    async move {
                        if direct {
                            mattermost_calls::decline(&client, &channel_id).await
                        } else {
                            mattermost_calls::dismiss_notification(&client, &channel_id).await
                        }
                    },
                    |_, _| {},
                );
            }
            (Some(notify::Pressed::Call(channel_id)), Some(notify::JOIN_CALL)) => {
                self.stop_ringing(cx);
                shell::present(cx);
                self.dispatch(Action::SelectChannel(channel_id.to_string()), cx);
                self.dispatch(Action::ToggleCall, cx);
            }
            // The notification itself: go and look.
            (Some(notify::Pressed::Call(channel_id) | notify::Pressed::Message(channel_id)), _) => {
                shell::present(cx);
                self.dispatch(Action::SelectChannel(channel_id.to_string()), cx);
            }
            (None, _) => {}
        }
    }

    /// A window was built around this session.
    pub(super) fn attach(self: &Rc<Self>, window: AnyWindowHandle, cx: &mut App) {
        self.window.set(Some(window));
        self.refresh_title(cx);
        self.restore_draft(cx);
        self.restore_thread_draft(cx);
    }

    /// The window is going away; the session is not.
    pub(super) fn detach(&self, cx: &mut App) {
        // The debounce means the last few seconds would otherwise be lost,
        // and closing the window is exactly when the next launch's picture
        // is decided.
        self.save_snapshot(cx);
        self.window.set(None);
        self.chat.detach();
        self.right.detach();
        self.search_box.detach();
    }

    /// Says something that happened, where it will be seen and then go away.
    pub fn toast(&self, message: &str, cx: &mut App) {
        let message = message.to_string();
        self.window.update(cx, move |window, cx| {
            window.push_notification(Notification::new().message(message.clone()), cx);
        });
    }

    /// Queues an action. It runs once whatever is running now has finished —
    /// never inside it — so the code that dispatches it does not have to care
    /// what state it is in the middle of changing.
    pub fn dispatch(self: &Rc<Self>, action: Action, cx: &mut App) {
        let ui = self.clone();
        cx.defer(move |cx| ui.handle(action, cx));
    }

    /// Answers an outstanding completion query, and remembers the answer so a
    /// picture landing later can redraw the popover without re-querying it.
    fn set_completions(&self, items: Vec<autocomplete::Candidate>, cx: &mut App) {
        *self.last_completions.borrow_mut() = items.clone();
        self.chat.set_completions(items, cx);
    }

    /// A picture that finished downloading after the popover opened belongs
    /// to one of the rows already drawn — redraw it with whatever pictures
    /// are available now. No query, no network: same candidates, same order.
    fn refresh_completion_avatars(&self, cx: &mut App) {
        let mut items = self.last_completions.borrow().clone();
        if items.is_empty() {
            return;
        }
        let mut changed = false;
        for item in &mut items {
            if item.image.is_some() {
                continue;
            }
            if let Some(id) = &item.user_id {
                if let Some(texture) = self.avatars.texture(id) {
                    item.image = Some(texture);
                    changed = true;
                }
            }
        }
        if changed {
            self.set_completions(items, cx);
        }
    }

    /// Redraws only the message surfaces — used when something cosmetic lands,
    /// such as an avatar finishing its download.
    fn refresh_messages(self: &Rc<Self>, cx: &mut App) {
        self.chat.refresh(&self.state, cx);
        self.right.refresh(&self.state, cx);
    }

    /// Redraws only the thread panel — used when a thread's own data changes,
    /// which has nothing to do with the channel feed behind it. The feed's
    /// "N replies" footer is kept live separately, by the ordinary post-apply
    /// path that already runs on every incoming reply.
    fn refresh_thread_panel(self: &Rc<Self>, cx: &mut App) {
        self.right.refresh(&self.state, cx);
    }

    fn refresh_all(self: &Rc<Self>, cx: &mut App) {
        self.channels.refresh(cx);
        self.refresh_messages(cx);
        self.refresh_call_ui(cx);
        self.refresh_title(cx);
        self.hydrate_dm_teammates(cx);
    }

    /// Applies a streamed LLM answer.
    ///
    /// `next` carries the whole message so far rather than the new part, so
    /// this replaces the text instead of appending — appending would double
    /// every character.
    fn apply_stream_update(self: &Rc<Self>, data: &mattermost_api::ws::Data, cx: &mut App) {
        use crate::agents::StreamUpdate;
        match crate::agents::parse_stream(data) {
            StreamUpdate::Text { post_id, message } => {
                let mut st = self.state.borrow_mut();
                let Some(mut post) = st.post(&post_id) else {
                    // The post itself arrives over the ordinary `posted`
                    // event; a stream frame that beats it has nothing to
                    // write into yet, and the next frame will.
                    return;
                };
                post.message = message;
                st.apply_post(post);
                drop(st);
                // The row keeps its place in the feed; only what it says
                // changes, so the reader is not moved by an answer growing.
                self.refresh_messages(cx);
            }
            // The final text already arrived as a Text frame, and the post is
            // updated server-side too; nothing left to do.
            StreamUpdate::Done { .. } | StreamUpdate::Ignored => {}
        }
    }

    /// Announces an incoming call, with the two things you might want to do
    /// about it. Mattermost has no ring signal of its own — a call starting is
    /// the whole event — so this is the client's doing.
    fn ring(self: &Rc<Self>, channel_id: &str, _cx: &mut App) {
        let title = {
            let st = self.state.borrow();
            st.channel(channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_else(|| "Someone".to_string())
        };
        // Tagged by channel, so a second call in the same DM replaces the
        // first rather than stacking two doorbells.
        self.notifier.show(Notice {
            tag: notify::call_tag(channel_id),
            title: format!("{title} is calling"),
            body: "Incoming call".to_string(),
            urgent: true,
            actions: vec![
                (notify::JOIN_CALL.to_string(), "Join".to_string()),
                (notify::DISMISS_CALL.to_string(), "Dismiss".to_string()),
            ],
        });
        self.state.borrow_mut().ringing.push(channel_id.to_string());
    }

    /// Takes every incoming-call notification back down.
    pub(super) fn stop_ringing(&self, _cx: &mut App) {
        let ringing = std::mem::take(&mut self.state.borrow_mut().ringing);
        for channel_id in ringing {
            self.notifier.withdraw(&notify::call_tag(&channel_id));
        }
    }

    /// Applies a host control. These are HTTP routes rather than websocket
    /// messages — the one part of the calls protocol that is.
    fn host_control(self: &Rc<Self>, session_id: String, what: call_dock::HostAction, _cx: &mut App) {
        let Some(session) = self.state.borrow().call.as_ref().map(|c| c.session.clone()) else {
            return;
        };
        let ui = self.clone();
        runtime::spawn(
            async move {
                match what {
                    call_dock::HostAction::MuteOthers => session.host_mute_others().await,
                    call_dock::HostAction::EndCall => session.host_end_call().await,
                    call_dock::HostAction::Mute => session.host_mute(&session_id).await,
                    call_dock::HostAction::StopSharing => {
                        session.host_screen_off(&session_id).await
                    }
                    call_dock::HostAction::LowerHand => session.host_lower_hand(&session_id).await,
                    call_dock::HostAction::MakeHost => session.host_make(&session_id).await,
                    call_dock::HostAction::Remove => session.host_remove(&session_id).await,
                }
            },
            move |result, cx| {
                if let Err(e) = result {
                    ui.toast(&format!("Could not do that: {e}"), cx);
                }
            },
        );
    }

    /// Raises or lowers your own hand. The SFU echoes it back as
    /// `user_raise_hand`, which is what actually updates the roster.
    fn toggle_hand(self: &Rc<Self>, _cx: &mut App) {
        let (session, raise) = {
            let st = self.state.borrow();
            let Some(call) = st.call.as_ref() else { return };
            (
                call.session.clone(),
                !call.hands.iter().any(|id| id == &st.me.id),
            )
        };
        let ui = self.clone();
        runtime::spawn(
            async move { session.raise_hand(raise).await },
            move |result, cx| {
                if let Err(e) = result {
                    ui.toast(&format!("Could not do that: {e}"), cx);
                }
            },
        );
    }

    /// Looks up people named in messages that we do not hold.
    ///
    /// A mention is only highlighted once the name resolves, and a channel you
    /// have just opened is full of names you may never have seen — without
    /// this, every one of them reads as plain text until they happen to post.
    fn resolve_mentions(self: &Rc<Self>, _cx: &mut App) {
        let (client, wanted) = {
            let st = self.state.borrow();
            let known: HashSet<&str> = st.users.values().map(|u| u.username.as_str()).collect();
            let mut wanted: HashSet<String> = HashSet::new();
            let feed = st.current_channel.as_ref().and_then(|id| st.feeds.get(id));
            for post in feed.into_iter().flat_map(|feed| feed.posts.iter()) {
                for name in mentioned_names(&post.message) {
                    if !known.contains(name.as_str()) {
                        wanted.insert(name);
                    }
                }
            }
            (st.client.clone(), wanted)
        };
        if wanted.is_empty() {
            return;
        }

        let names: Vec<String> = wanted.into_iter().take(50).collect();
        let ui = self.clone();
        runtime::spawn(
            async move { client.users_by_usernames(&names).await },
            move |result, cx| {
                let Ok(users) = result else { return };
                if users.is_empty() {
                    return;
                }
                {
                    let mut st = ui.state.borrow_mut();
                    for user in users {
                        st.users.insert(user.id.clone(), user);
                    }
                }
                ui.refresh_messages(cx);
            },
        );
    }

    /// Quietly fetches the channels most likely to be opened next.
    ///
    /// Switching to a channel that has never been read waits on the network;
    /// the ones with something unread are exactly the ones about to be
    /// clicked, so their first page is fetched before it is asked for. Three
    /// of them, because this is a guess and a wrong guess should be cheap.
    fn preload_unread(self: &Rc<Self>, _cx: &mut App) {
        const PRELOAD: usize = 3;

        let (client, crt, wanted) = {
            let st = self.state.borrow();
            let wanted: Vec<String> = st
                .sidebar_groups()
                .into_iter()
                .flat_map(|(_, channels)| channels)
                .map(|c| c.id)
                .filter(|id| st.unread(id).is_unread() && !st.feeds.contains_key(id))
                .take(PRELOAD)
                .collect();
            (st.client.clone(), st.crt_enabled, wanted)
        };

        for channel_id in wanted {
            let client = client.clone();
            let ui = self.clone();
            runtime::spawn(
                async move {
                    let posts = client
                        .posts_for_channel(&channel_id, 0, INITIAL_POSTS, crt)
                        .await?;
                    let (authors, statuses) = hydrate_authors(&client, &posts).await;
                    Ok::<_, mattermost_api::Error>((channel_id, posts, authors, statuses))
                },
                move |result, cx| {
                    let Ok((channel_id, posts, authors, statuses)) = result else {
                        return;
                    };
                    {
                        let mut st = ui.state.borrow_mut();
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        // Only if it is still absent: the person may have
                        // opened it while this was in flight, and that copy is
                        // the one being read.
                        st.feeds
                            .entry(channel_id)
                            .or_insert_with(|| ChannelFeed::from_list(&posts));
                    }
                    ui.store_posts(posts.chronological().into_iter().cloned().collect(), cx);
                },
            );
        }
    }

    /// Files posts in the local store. Fire and forget: a failure costs a
    /// slower next launch and nothing on this one.
    fn store_posts(&self, posts: Vec<Post>, _cx: &mut App) {
        if posts.is_empty() {
            return;
        }
        let Some(store) = self.store.borrow().clone() else {
            return;
        };
        // Once every ten minutes of running is plenty for a cache that grows a
        // screenful at a time.
        const PRUNE_EVERY: std::time::Duration = std::time::Duration::from_secs(600);
        let prune = self
            .pruned_at
            .get()
            .is_none_or(|at| at.elapsed() >= PRUNE_EVERY);
        if prune {
            self.pruned_at.set(Some(std::time::Instant::now()));
        }
        runtime::spawn(
            async move {
                if let Err(e) = store.save_posts(posts).await {
                    tracing::warn!(error = %e, "could not store those messages");
                }
            },
            |_, _| {},
        );
    }

    /// Writes the snapshot the next launch will open with.
    ///
    /// Debounced hard: this serialises a chunk of state, and the only thing
    /// that matters is that it ran reasonably recently before the app closed.
    fn schedule_snapshot(self: &Rc<Self>, _cx: &mut App) {
        if self.snapshot_pending.replace(true) {
            return;
        }
        let ui = self.clone();
        runtime::after(std::time::Duration::from_secs(5), move |cx| {
            ui.snapshot_pending.set(false);
            ui.save_snapshot(cx);
        });
    }

    fn capture_scroll_anchor(&self, cx: &mut App) {
        let Some((channel_id, post_id)) = self.chat.current_anchor(cx) else {
            return;
        };
        let mut st = self.state.borrow_mut();
        match post_id {
            Some(post_id) => {
                st.scroll_anchors.insert(channel_id, post_id);
            }
            None => {
                st.scroll_anchors.remove(&channel_id);
            }
        }
    }

    fn save_snapshot(&self, cx: &mut App) {
        self.capture_scroll_anchor(cx);
        let st = self.state.borrow();
        crate::cache::save(&crate::cache::Snapshot {
            server: st.client.site_url().to_string(),
            current_team: st.current_team.clone(),
            current_channel: st.current_channel.clone(),
            scroll_anchors: st.scroll_anchors.clone(),
        });
        drop(st);

        // The content goes to the store, which keeps every channel rather than
        // the handful a single file could hold.
        let Some(store) = self.store.borrow().clone() else {
            return;
        };
        // Once every ten minutes of running is plenty for a cache that grows a
        // screenful at a time.
        const PRUNE_EVERY: std::time::Duration = std::time::Duration::from_secs(600);
        let prune = self
            .pruned_at
            .get()
            .is_none_or(|at| at.elapsed() >= PRUNE_EVERY);
        if prune {
            self.pruned_at.set(Some(std::time::Instant::now()));
        }
        let (channels, members, users, posts) = {
            let st = self.state.borrow();
            let posts: Vec<Post> = st
                .feeds
                .values()
                .flat_map(|feed| feed.posts.iter().cloned())
                .collect();
            (
                st.channels.values().cloned().collect::<Vec<_>>(),
                st.memberships.values().cloned().collect::<Vec<_>>(),
                st.users.values().cloned().collect::<Vec<_>>(),
                posts,
            )
        };
        runtime::spawn(
            async move {
                // Failures here cost a slower next launch and nothing else, so
                // they are logged rather than surfaced.
                if let Err(e) = store.save_channels(channels, members).await {
                    tracing::warn!(error = %e, "could not store the channel list");
                }
                if let Err(e) = store.save_users(users).await {
                    tracing::warn!(error = %e, "could not store the users");
                }
                if let Err(e) = store.save_posts(posts).await {
                    tracing::warn!(error = %e, "could not store the messages");
                }
                // Occasionally, not on every write: switching channels
                // writes a snapshot each time, and scanning every post to
                // delete nothing is pure work.
                if prune {
                    if let Err(e) = store.prune(crate::store::DEFAULT_KEEP_PER_CHANNEL).await {
                        tracing::warn!(error = %e, "could not trim the store");
                    }
                }
            },
            |_, _| {},
        );
    }

    /// Sets your own presence, showing it immediately: the server echoes it
    /// back as a status_change, but the click should not wait for a round trip
    /// to look like it landed.
    fn set_status(self: &Rc<Self>, status: String, cx: &mut App) {
        let (client, me) = {
            let mut st = self.state.borrow_mut();
            let me = st.me.id.clone();
            let presence = mattermost_api::models::Presence::from(status.as_str());
            st.statuses.insert(me.clone(), presence);
            (st.client.clone(), me)
        };
        self.channels.refresh(cx);
        self.refresh_messages(cx);

        let ui = self.clone();
        runtime::spawn(
            async move { client.set_status(&me, &status).await },
            move |result, cx| {
                if let Err(e) = result {
                    ui.toast(&format!("Could not change your status: {e}"), cx);
                }
            },
        );
    }

    /// Asks the Agents plugin what bots exist. A server without the plugin
    /// 404s, which is indistinguishable from having no bots.
    /// Fetches the names of the server's own emoji — all of them, a page at
    /// a time. They decide which `:words:` in a message are pictures and are
    /// what the pickers offer, and neither can wait for a round trip.
    fn load_custom_emoji(self: &Rc<Self>, _cx: &mut App) {
        const PAGE: u32 = 200;
        // A server with ten thousand of them is somebody's mistake; stop.
        const PAGES: u32 = 50;
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                let mut names = std::collections::BTreeSet::new();
                for page in 0..PAGES {
                    let batch = client.custom_emoji(page, PAGE).await?;
                    let last = (batch.len() as u32) < PAGE;
                    names.extend(batch.into_iter().map(|emoji| emoji.name));
                    if last {
                        break;
                    }
                }
                Ok::<_, mattermost_api::Error>(names)
            },
            move |result, cx| match result {
                Ok(names) => {
                    let changed = {
                        let mut st = ui.state.borrow_mut();
                        let changed = st.custom_emoji != names;
                        st.custom_emoji = names;
                        changed
                    };
                    // Messages drawn before this landed spelled them out.
                    if changed {
                        ui.refresh_all(cx);
                    }
                }
                Err(e) => tracing::debug!(error = %e, "could not list the custom emoji"),
            },
        );
    }

    fn load_bots(self: &Rc<Self>, _cx: &mut App) {
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move { crate::agents::bots(&client).await },
            move |result, cx| match result {
                Ok(bots) => {
                    ui.state.borrow_mut().bots = bots.bots;
                    ui.refresh_agent_actions(cx);
                }
                Err(e) => tracing::debug!(error = %e, "no agents plugin on this server"),
            },
        );
    }

    /// Shows or hides the agent entries, which only make sense when there is
    /// a bot to answer them.
    fn refresh_agent_actions(self: &Rc<Self>, cx: &mut App) {
        let bots: Vec<(String, String)> = self
            .state
            .borrow()
            .bots
            .iter()
            .map(|bot| {
                let name = if bot.display_name.is_empty() {
                    format!("@{}", bot.username)
                } else {
                    bot.display_name.clone()
                };
                // The DM channel may not exist yet; the bot's user id is
                // enough to make one.
                let target = if bot.dm_channel_id.is_empty() {
                    format!("user:{}", bot.id)
                } else {
                    format!("channel:{}", bot.dm_channel_id)
                };
                (target, format!("Chat with {name}"))
            })
            .collect();
        self.chat.set_agents(&bots, cx);
    }

    /// "Catch me up": asks the default bot to summarise what you have not read
    /// in this channel. The answer is written into a DM post, so this only
    /// starts it — the text arrives over the socket.
    fn summarise_unreads(self: &Rc<Self>, cx: &mut App) {
        let (client, channel) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_channel.clone())
        };
        let Some(channel_id) = channel else { return };

        let ui = self.clone();
        self.toast("Asking the agent…", cx);
        runtime::spawn(
            async move { crate::agents::summarise_unreads(&client, &channel_id).await },
            move |result, cx| match result {
                // The summary is a DM from the bot, so go and read it there.
                Ok(target) => ui.dispatch(Action::OpenPost(target.channel_id, target.post_id), cx),
                Err(e) => ui.toast(&format!("The agent could not answer: {e}"), cx),
            },
        );
    }

    /// Handles the reactions-notify plugin, which tells you when somebody
    /// reacts to something you wrote — something core Mattermost does not.
    ///
    /// The plugin creates no posts: the websocket and its own feed endpoint
    /// are the whole client surface.
    fn apply_reaction_notice(self: &Rc<Self>, kind: &str, data: &mattermost_api::ws::Data, cx: &mut App) {
        let string = |key: &str| {
            data.get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        match kind {
            "reaction_item" => {
                // The plugin has already applied the push-content policy and
                // decided whether a toast is appropriate — it knows things we
                // do not, like whether you are active in that channel.
                let suppressed = data
                    .get("suppress_desktop")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                self.state.borrow_mut().reaction_unread += 1;
                self.refresh_messages(cx);
                if suppressed {
                    return;
                }
                let channel_id = string("channel_id");
                let title = match string("channel_name").as_str() {
                    "" => format!(
                        ":{}: from {}",
                        string("emoji_name"),
                        string("reactor_username")
                    ),
                    channel => format!("{channel} — :{}:", string("emoji_name")),
                };
                let body = match string("text").as_str() {
                    "" => string("snippet"),
                    text => text.to_string(),
                };
                self.notify_message(&channel_id, &title, &body);
            }
            // Authoritative count, so it replaces ours rather than adjusting it.
            "unread" => {
                let count = data
                    .get("count")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                self.state.borrow_mut().reaction_unread = count;
                self.refresh_messages(cx);
            }
            "item_removed" => {
                let mut st = self.state.borrow_mut();
                st.reaction_unread = (st.reaction_unread - 1).max(0);
                drop(st);
                self.refresh_messages(cx);
            }
            other => tracing::debug!(event = other, "unhandled reactions-notify event"),
        }
    }

    /// Applies an edit. An emptied message means delete, which is what the
    /// other clients do and what the server expects.
    fn submit_edit(self: &Rc<Self>, post_id: String, text: String, cx: &mut App) {
        self.chat.end_edit(cx);
        if text.trim().is_empty() {
            self.confirm_delete(post_id, cx);
            return;
        }
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move { client.update_post(&post_id, &text).await },
            move |result, cx| {
                // Success arrives as post_edited over the socket, so only the
                // failure needs saying.
                if let Err(e) = result {
                    ui.toast(&format!("Could not save the edit: {e}"), cx);
                }
            },
        );
    }

    fn menu_action(self: &Rc<Self>, action: MenuAction, cx: &mut App) {
        match action {
            MenuAction::NewChannel => self.new_channel(cx),
            MenuAction::BrowseChannels => self.browse_channels(cx),
            MenuAction::AccountNotifications => self.account_notifications(cx),
            MenuAction::EditProfile => self.edit_profile(cx),
            MenuAction::QuickSwitch => self.quick_switch(cx),
            MenuAction::SignOut => self.sign_out(cx),
            MenuAction::ScheduledPosts => self.scheduled_posts(cx),
            MenuAction::ChannelMembers => self.channel_members(cx),
            MenuAction::ChannelBookmarks => self.channel_bookmarks(cx),
            MenuAction::BrowseTeams => self.browse_teams(cx),
            MenuAction::NewCategory => self.new_category(cx),
            MenuAction::FocusSearch => self.search_box.focus(cx),
            MenuAction::OpenInbox => self.open_inbox(cx),
            MenuAction::NextUnread => self.step_unread(true, cx),
            MenuAction::PreviousUnread => self.step_unread(false, cx),
            MenuAction::LeaveTeam => self.leave_team(cx),
            MenuAction::EditChannel => self.edit_channel(cx),
            MenuAction::ArchiveChannel => self.archive_channel(cx),
            MenuAction::CustomStatus => self.custom_status(cx),
            MenuAction::ChannelNotifications => self.channel_notifications(cx),
            MenuAction::LeaveChannel => self.leave_channel(cx),
            MenuAction::PinnedPosts => self.show_pinned(cx),
            MenuAction::Storage => storage::show(self, self.avatars.resources(), cx),
            MenuAction::Settings => settings::show(self, cx),
        }
    }

    fn new_channel(self: &Rc<Self>, cx: &mut App) {
        let ui = self.clone();
        dialogs::create_channel(self, cx, move |display_name, url, purpose, private, _cx| {
            let (client, team) = {
                let st = ui.state.borrow();
                (st.client.clone(), st.current_team.clone())
            };
            let Some(team_id) = team else { return };
            let kind = if private {
                ChannelType::Private
            } else {
                ChannelType::Open
            };
            let ui = ui.clone();
            let purpose = purpose.clone();
            runtime::spawn(
                async move {
                    let channel = client
                        .create_channel(&team_id, &url, &display_name, kind)
                        .await?;
                    // Purpose is a separate patch; a create that succeeds and
                    // a purpose that does not is still a usable channel.
                    if !purpose.is_empty() {
                        let _ = client.update_channel_header(&channel.id, &purpose).await;
                    }
                    Ok::<_, mattermost_api::Error>(channel)
                },
                move |result, cx| match result {
                    Ok(channel) => {
                        ui.schedule_sidebar_reload(cx);
                        ui.dispatch(Action::SelectChannel(channel.id), cx);
                    }
                    Err(e) => ui.toast(&format!("Could not create it: {e}"), cx),
                },
            );
        });
    }

    fn browse_channels(self: &Rc<Self>, cx: &mut App) {
        let browser = Rc::new(RefCell::new(None::<Rc<dialogs::ChannelBrowser>>));
        let ui = self.clone();
        let search_ui = self.clone();
        let holder = browser.clone();

        let opened = Rc::new(dialogs::ChannelBrowser::present(self, cx, move |term, _cx| {
                let (client, team) = {
                    let st = search_ui.state.borrow();
                    (st.client.clone(), st.current_team.clone())
                };
                let Some(team_id) = team else { return };
                let holder = holder.clone();
                let state = search_ui.state.clone();
                let asked = term.clone();
                runtime::spawn(
                    async move {
                        // An empty box means "show me what is there", which is
                        // the browse list rather than a search for nothing.
                        if term.trim().is_empty() {
                            client.channels_for_team(&team_id, 0, 100).await
                        } else {
                            client.search_channels(&team_id, &term).await
                        }
                    },
                    move |result, cx| {
                        let Ok(channels) = result else { return };
                        let joined = &state.borrow().channels;
                        let rows = channels
                            .into_iter()
                            .filter(|c| c.delete_at == 0)
                            .map(|c| {
                                let member = joined.contains_key(&c.id);
                                (c.id, c.display_name, c.purpose, member)
                            })
                            .collect();
                        if let Some(browser) = holder.borrow().as_ref() {
                            browser.set_results(&asked, rows, cx);
                        }
                    },
                );
            },
            move |channel_id, _cx| {
                let (client, me) = {
                    let st = ui.state.borrow();
                    (st.client.clone(), st.me.id.clone())
                };
                let ui = ui.clone();
                runtime::spawn(
                    async move { client.join_channel(&channel_id, &me).await },
                    move |result, cx| match result {
                        Ok(member) => {
                            ui.schedule_sidebar_reload(cx);
                            ui.dispatch(Action::SelectChannel(member.channel_id), cx);
                        }
                        Err(e) => ui.toast(&format!("Could not join: {e}"), cx),
                    },
                );
            },
        ));
        // The browser asks for its first page as it opens, and that request
        // needs the handle to fill — which only exists once present returns.
        // Storing it here is what closes that loop.
        *browser.borrow_mut() = Some(opened);
    }

    /// Shows what a message used to say, newest first, each with the time it
    /// was replaced.
    fn show_history(self: &Rc<Self>, versions: Vec<Post>, cx: &mut App) {
        // The current text is the first entry the server returns, so anything
        // after it is something you could go back to.
        let rows = versions
            .into_iter()
            .enumerate()
            .map(|(index, version)| {
                let mut row = dialogs::Row::new(
                    version.id.clone(),
                    format!(
                        "{} {}",
                        format_day(version.create_at),
                        format_time(version.create_at)
                    ),
                )
                .body(version.source_text());
                if index > 0 {
                    let ui = self.clone();
                    let post_id = version.original_id.clone();
                    let version_id = version.id.clone();
                    row = row.button("Restore this version", "Restoring…", move |_cx| {
                        let client = ui.state.borrow().client.clone();
                        let post_id = post_id.clone();
                        let version_id = version_id.clone();
                        let ui = ui.clone();
                        runtime::spawn(
                            async move { client.restore_post_version(&post_id, &version_id).await },
                            move |result, cx| match result {
                                Ok(_) => ui.toast("Restored.", cx),
                                Err(e) => ui.toast(&format!("Could not restore it: {e}"), cx),
                            },
                        );
                    });
                }
                row
            })
            .collect();
        dialogs::show_rows(self, cx, "Edit history", rows, None);
    }

    /// Sends what is in the composer at a chosen time.
    ///
    /// The server keeps it and posts it for you, so this works with the app
    /// closed — which is the only reason to use it over waiting.
    fn schedule_message(self: &Rc<Self>, cx: &mut App) {
        let text = self.chat.composer_text(cx);
        if text.trim().is_empty() {
            self.toast("Write the message first.", cx);
            return;
        }
        let (client, channel_id) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_channel.clone())
        };
        let Some(channel_id) = channel_id else { return };

        let ui = self.clone();
        dialogs::schedule_message(self, cx, move |when_ms, _cx| {
            let scheduled = mattermost_api::models::ScheduledPost {
                post: Post {
                    channel_id: channel_id.clone(),
                    message: text.clone(),
                    ..Default::default()
                },
                scheduled_at: when_ms,
                error_code: String::new(),
            };
            let client = client.clone();
            let ui = ui.clone();
            runtime::spawn(
                async move { client.create_scheduled_post(&scheduled).await },
                move |result, cx| match result {
                    Ok(_) => {
                        // The composer is empty now, so the draft goes too.
                        ui.chat.set_composer_text("", cx);
                        ui.save_draft(cx);
                        ui.toast("Scheduled.", cx);
                    }
                    Err(e) => ui.toast(&format!("Could not schedule it: {e}"), cx),
                },
            );
        });
    }

    /// Muting a channel, or filing it under a different category.
    fn row_action(self: &Rc<Self>, channel_id: String, what: RowAction, cx: &mut App) {
        let (client, me, team_id) = {
            let st = self.state.borrow();
            (
                st.client.clone(),
                st.me.id.clone(),
                st.current_team.clone().unwrap_or_default(),
            )
        };

        match what {
            RowAction::MarkRead => {
                let crt = self.state.borrow().crt_enabled;
                let ui = self.clone();
                runtime::spawn(
                    async move { client.view_channel(&channel_id, "", crt).await },
                    move |result, cx| match result {
                        // The server broadcasts multiple_channels_viewed, which
                        // is what actually clears the badge.
                        Ok(_) => ui.schedule_sidebar_reload(cx),
                        Err(e) => ui.toast(&format!("Could not mark it read: {e}"), cx),
                    },
                );
            }
            RowAction::MarkUnread => {
                let crt = self.state.borrow().crt_enabled;
                let ui = self.clone();
                let target = channel_id.clone();
                runtime::spawn(
                    async move { client.mark_channel_unread(&me, &channel_id, crt).await },
                    move |result, cx| match result {
                        Ok(()) => {
                            // Reading it again on the way out would undo this.
                            if ui.state.borrow().current_channel.as_deref() == Some(target.as_str())
                            {
                                ui.state.borrow_mut().current_channel = None;
                                ui.refresh_messages(cx);
                            }
                            ui.schedule_sidebar_reload(cx);
                        }
                        Err(e) => ui.toast(&format!("Could not mark it unread: {e}"), cx),
                    },
                );
            }
            RowAction::SetMuted(muted) => {
                // Muted is stored as mark_unread: "mention" — the same field
                // the notification dialog writes, so it goes the same way.
                let mut props = mattermost_api::models::StringMap::new();
                props.insert(
                    "mark_unread".into(),
                    if muted { "mention" } else { "all" }.into(),
                );
                let ui = self.clone();
                runtime::spawn(
                    async move {
                        client
                            .set_channel_notify_props(&channel_id, &me, &props)
                            .await
                    },
                    move |result, cx| match result {
                        Ok(()) => ui.schedule_sidebar_reload(cx),
                        Err(e) => ui.toast(&format!("Could not change that: {e}"), cx),
                    },
                );
            }
            RowAction::RenameCategory => {
                let current = self
                    .state
                    .borrow()
                    .categories
                    .categories
                    .iter()
                    .find(|c| c.id == channel_id)
                    .map(|c| c.display_name.clone())
                    .unwrap_or_default();
                let ui = self.clone();
                dialogs::name_category(self, cx, "Rename category", &current, move |name, _cx| {
                    let Some(mut category) = ui
                        .state
                        .borrow()
                        .categories
                        .categories
                        .iter()
                        .find(|c| c.id == channel_id)
                        .cloned()
                    else {
                        return;
                    };
                    category.display_name = name;
                    let client = ui.state.borrow().client.clone();
                    let me = ui.state.borrow().me.id.clone();
                    let team_id = ui.state.borrow().current_team.clone().unwrap_or_default();
                    let ui = ui.clone();
                    runtime::spawn(
                        async move {
                            client
                                .update_categories(&me, &team_id, std::slice::from_ref(&category))
                                .await
                        },
                        move |result, cx| match result {
                            Ok(_) => ui.schedule_sidebar_reload(cx),
                            Err(e) => ui.toast(&format!("Could not rename it: {e}"), cx),
                        },
                    );
                });
            }
            RowAction::DeleteCategory => {
                // The channels in it are not deleted — they fall back to the
                // default category — so this needs no confirmation.
                let ui = self.clone();
                runtime::spawn(
                    async move { client.delete_category(&me, &team_id, &channel_id).await },
                    move |result, cx| match result {
                        Ok(()) => ui.schedule_sidebar_reload(cx),
                        Err(e) => ui.toast(&format!("Could not delete it: {e}"), cx),
                    },
                );
            }
            RowAction::MoveTo(category_id) => {
                // The categories route replaces membership wholesale, so both
                // the old and the new category have to be sent together.
                let mut categories = self.state.borrow().categories.categories.clone();
                for category in categories.iter_mut() {
                    category.channel_ids.retain(|id| id != &channel_id);
                    if category.id == category_id {
                        category.channel_ids.insert(0, channel_id.clone());
                    }
                }
                let ui = self.clone();
                runtime::spawn(
                    async move { client.update_categories(&me, &team_id, &categories).await },
                    move |result, cx| match result {
                        Ok(_) => ui.schedule_sidebar_reload(cx),
                        Err(e) => ui.toast(&format!("Could not move it: {e}"), cx),
                    },
                );
            }
        }
    }

    fn new_category(self: &Rc<Self>, cx: &mut App) {
        let ui = self.clone();
        dialogs::name_category(self, cx, "New category", "", move |name, _cx| {
            let (client, me, team_id) = {
                let st = ui.state.borrow();
                (
                    st.client.clone(),
                    st.me.id.clone(),
                    st.current_team.clone().unwrap_or_default(),
                )
            };
            let category = mattermost_api::models::SidebarCategory {
                display_name: name,
                team_id: team_id.clone(),
                user_id: me.clone(),
                r#type: mattermost_api::models::CategoryType::Custom,
                ..Default::default()
            };
            let ui = ui.clone();
            runtime::spawn(
                async move { client.create_category(&me, &team_id, &category).await },
                move |result, cx| match result {
                    Ok(_) => ui.schedule_sidebar_reload(cx),
                    Err(e) => ui.toast(&format!("Could not create it: {e}"), cx),
                },
            );
        });
    }

    fn leave_team(self: &Rc<Self>, cx: &mut App) {
        let (client, me, team_id, name) = {
            let st = self.state.borrow();
            let Some(team_id) = st.current_team.clone() else {
                return;
            };
            let name = st
                .teams
                .iter()
                .find(|t| t.id == team_id)
                .map(|t| t.display_name.clone())
                .unwrap_or_default();
            (st.client.clone(), st.me.id.clone(), team_id, name)
        };

        let ui = self.clone();
        dialogs::confirm_leave(self, cx, &name, move |_cx| {
            let client = client.clone();
            let me = me.clone();
            let team_id = team_id.clone();
            let ui = ui.clone();
            runtime::spawn(
                async move { client.leave_team(&team_id, &me).await },
                move |result, cx| match result {
                    Ok(()) => {
                        // Land somewhere real rather than on a team we just
                        // left.
                        ui.reload_teams(cx);
                        let next = ui.state.borrow().teams.first().map(|t| t.id.clone());
                        if let Some(team_id) = next {
                            ui.dispatch(Action::SelectTeam(team_id), cx);
                        }
                    }
                    Err(e) => ui.toast(&format!("Could not leave: {e}"), cx),
                },
            );
        });
    }

    /// The channel's name and topic.
    fn edit_channel(self: &Rc<Self>, cx: &mut App) {
        let (client, channel_id, current) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let Some(channel) = st.channel(&channel_id) else {
                return;
            };
            (
                st.client.clone(),
                channel_id,
                (channel.display_name.clone(), channel.header.clone()),
            )
        };

        let ui = self.clone();
        dialogs::edit_channel(self, cx, current, move |name, header, _cx| {
            let client = client.clone();
            let channel_id = channel_id.clone();
            let ui = ui.clone();
            runtime::spawn(
                async move {
                    // Two routes, because the server patches these separately.
                    if !name.is_empty() {
                        client.rename_channel(&channel_id, &name).await?;
                    }
                    client.update_channel_header(&channel_id, &header).await
                },
                move |result, cx| match result {
                    // channel_updated comes back over the socket and repaints.
                    Ok(_) => {}
                    Err(e) => ui.toast(&format!("Could not save that: {e}"), cx),
                },
            );
        });
    }

    fn archive_channel(self: &Rc<Self>, cx: &mut App) {
        let (client, channel_id, name) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let name = st
                .channel(&channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default();
            (st.client.clone(), channel_id, name)
        };

        let ui = self.clone();
        dialogs::confirm_archive(self, cx, &name, move |_cx| {
            let client = client.clone();
            let channel_id = channel_id.clone();
            let ui = ui.clone();
            runtime::spawn(
                async move { client.archive_channel(&channel_id).await },
                move |result, cx| match result {
                    Ok(()) => {
                        ui.state.borrow_mut().current_channel = None;
                        ui.schedule_sidebar_reload(cx);
                        ui.refresh_messages(cx);
                    }
                    Err(e) => ui.toast(&format!("Could not archive it: {e}"), cx),
                },
            );
        });
    }

    /// Who is in this channel, and a way to add or remove people.
    fn channel_members(self: &Rc<Self>, cx: &mut App) {
        let (client, channel_id, name, team_id) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let name = st
                .channel(&channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default();
            (
                st.client.clone(),
                channel_id,
                name,
                st.current_team.clone().unwrap_or_default(),
            )
        };

        let holder: Rc<RefCell<Option<Rc<dialogs::MemberList>>>> = Rc::new(RefCell::new(None));
        let search_holder = holder.clone();
        let search_client = client.clone();
        let search_channel = channel_id.clone();
        let search_ui = self.clone();
        let add_ui = self.clone();
        let add_client = client.clone();
        let add_channel = channel_id.clone();
        let remove_ui = self.clone();
        let remove_channel = channel_id.clone();

        let opened = Rc::new(dialogs::MemberList::present(self, cx, &name,
            move |term, _cx| {
                if term.trim().is_empty() {
                    return;
                }
                let client = search_client.clone();
                let team_id = team_id.clone();
                let channel_id = search_channel.clone();
                let holder = search_holder.clone();
                let state = search_ui.state.clone();
                let asked = term.clone();
                runtime::spawn(
                    // Not-in-channel only: offering someone already here is an
                    // add that does nothing.
                    async move { client.search_users(&term, &team_id, "", &channel_id).await },
                    move |result, cx| {
                        let Ok(users) = result else { return };
                        let display = state.borrow().teammate_name_display().to_string();
                        let rows = users
                            .into_iter()
                            .map(|u| {
                                (
                                    u.id.clone(),
                                    u.display_name(&display),
                                    format!("@{}", u.username),
                                )
                            })
                            .collect();
                        if let Some(list) = holder.borrow().as_ref() {
                            list.set_candidates(&asked, rows, cx);
                        }
                    },
                );
            },
            move |user_id, _cx| {
                let client = add_client.clone();
                let channel_id = add_channel.clone();
                let ui = add_ui.clone();
                runtime::spawn(
                    async move { client.join_channel(&channel_id, &user_id).await },
                    move |result, cx| match result {
                        Ok(_) => ui.toast("Added.", cx),
                        Err(e) => ui.toast(&format!("Could not add them: {e}"), cx),
                    },
                );
            },
            move |user_id, cx| {
                // Removing someone is not undoable and is visible to them, so
                // it asks first.
                let ui = remove_ui.clone();
                let name = ui.user_name(&user_id, cx);
                let client = client.clone();
                let channel_id = remove_channel.clone();
                let confirm_ui = ui.clone();
                dialogs::confirm_remove_member(&confirm_ui, cx, &name, move |_cx| {
                    let client = client.clone();
                    let channel_id = channel_id.clone();
                    let user_id = user_id.clone();
                    let ui = ui.clone();
                    runtime::spawn(
                        async move { client.leave_channel(&channel_id, &user_id).await },
                        move |result, cx| match result {
                            Ok(()) => ui.toast("Removed.", cx),
                            Err(e) => ui.toast(&format!("Could not remove them: {e}"), cx),
                        },
                    );
                });
            },
        ));
        *holder.borrow_mut() = Some(opened.clone());

        // Fill the current members in.
        let ui = self.clone();
        let fill_client = self.state.borrow().client.clone();
        runtime::spawn(
            async move {
                // Two requests rather than three: the users route can filter
                // by channel directly, and the memberships are only needed to
                // find out who the channel admins are.
                let (members, users) = tokio::try_join!(
                    fill_client.channel_members(&channel_id, 0, 200),
                    fill_client.users_in_channel(&channel_id, 0, 200),
                )?;
                Ok::<_, mattermost_api::Error>((members, users))
            },
            move |result, cx| {
                let Ok((members, users)) = result else { return };
                let display = ui.state.borrow().teammate_name_display().to_string();
                let rows = users
                    .into_iter()
                    .map(|user| {
                        let admin = members
                            .iter()
                            .find(|m| m.user_id == user.id)
                            .is_some_and(|m| m.roles.contains("channel_admin"));
                        let name = user.display_name(&display);
                        (user.id, name, format!("@{}", user.username), admin)
                    })
                    .collect();
                opened.set_members(rows, cx);
            },
        );
    }

    /// A channel's bookmarks. Servers older than 9.4 have no such route, so a
    /// failure here says the feature is missing rather than that it broke.
    fn channel_bookmarks(self: &Rc<Self>, cx: &mut App) {
        let (client, channel_id) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_channel.clone())
        };
        let Some(channel_id) = channel_id else { return };

        let holder: Rc<RefCell<Option<Rc<dialogs::BookmarkList>>>> = Rc::new(RefCell::new(None));
        let refill = {
            let holder = holder.clone();
            let client = client.clone();
            let channel_id = channel_id.clone();
            let ui = self.clone();
            move || {
                let holder = holder.clone();
                let client = client.clone();
                let channel_id = channel_id.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move { client.list_bookmarks(&channel_id).await },
                    move |result, cx| match result {
                        Ok(bookmarks) => {
                            let rows = bookmarks
                                .into_iter()
                                .map(|b| (b.id, b.display_name, b.link_url))
                                .collect();
                            if let Some(list) = holder.borrow().as_ref() {
                                list.set_bookmarks(rows, cx);
                            }
                        }
                        Err(e) => ui.toast(&format!("Bookmarks are not available here: {e}"), cx),
                    },
                );
            }
        };

        let add_client = client.clone();
        let add_channel = channel_id.clone();
        let add_refill = refill.clone();
        let delete_refill = refill.clone();
        let ui = self.clone();
        let opened = Rc::new(dialogs::BookmarkList::present(self, cx, move |display_name, link_url, _cx| {
                let bookmark = mattermost_api::models::ChannelBookmark {
                    channel_id: add_channel.clone(),
                    display_name: if display_name.is_empty() {
                        link_url.clone()
                    } else {
                        display_name
                    },
                    link_url,
                    r#type: "link".into(),
                    ..Default::default()
                };
                let client = add_client.clone();
                let channel_id = add_channel.clone();
                let refill = add_refill.clone();
                runtime::spawn(
                    async move { client.create_bookmark(&channel_id, &bookmark).await },
                    move |_, _| refill(),
                );
            },
            move |link_url, cx| {
                cx.open_url(&link_url);
            },
            move |bookmark_id, _cx| {
                let client = client.clone();
                let channel_id = channel_id.clone();
                let refill = delete_refill.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move { client.delete_bookmark(&channel_id, &bookmark_id).await },
                    move |result, cx| {
                        if let Err(e) = result {
                            ui.toast(&format!("Could not remove it: {e}"), cx);
                        }
                        refill();
                    },
                );
            },
        ));
        *holder.borrow_mut() = Some(opened);
        refill();
    }

    /// Teams on this server you are not in yet.
    fn browse_teams(self: &Rc<Self>, cx: &mut App) {
        let (client, me) = {
            let st = self.state.borrow();
            (st.client.clone(), st.me.id.clone())
        };

        let join_client = client.clone();
        let ui = self.clone();
        let browser = Rc::new(dialogs::TeamBrowser::present(self, cx, move |team_id, _cx| {
                let client = join_client.clone();
                let me = me.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move { client.join_team(&team_id, &me).await },
                    move |result, cx| match result {
                        Ok(member) => {
                            ui.reload_teams(cx);
                            ui.dispatch(Action::SelectTeam(member.team_id), cx);
                        }
                        Err(e) => ui.toast(&format!("Could not join: {e}"), cx),
                    },
                );
            },
        ));

        let mine = self.state.borrow().teams.clone();
        runtime::spawn(
            async move { client.all_teams(0, 100).await },
            move |result, cx| {
                let Ok(teams) = result else { return };
                let rows = teams
                    .into_iter()
                    .filter(|t| t.delete_at == 0)
                    .map(|team| {
                        let member = mine.iter().any(|m| m.id == team.id);
                        (team.id, team.display_name, team.description, member)
                    })
                    .collect();
                browser.set_teams(rows, cx);
            },
        );
    }

    /// Messages waiting to be sent later, with a way to call them off.
    ///
    /// The response is a bucket map keyed by team, plus a separate one for
    /// direct messages, so this walks the values rather than assuming a shape.
    fn scheduled_posts(self: &Rc<Self>, _cx: &mut App) {
        let (client, team) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_team.clone())
        };
        let Some(team_id) = team else { return };

        let ui = self.clone();
        runtime::spawn(
            async move { client.scheduled_posts_for_team(&team_id).await },
            move |result, cx| match result {
                Ok(value) => ui.show_scheduled(value, cx),
                Err(e) => ui.toast(&format!("Could not load them: {e}"), cx),
            },
        );
    }

    fn show_scheduled(
        self: &Rc<Self>,
        scheduled: mattermost_api::models::TeamScheduledPosts,
        cx: &mut App,
    ) {
        // Every bucket, team and direct alike: they are all messages this
        // person has waiting, and separating them here would be a
        // distinction without a difference.
        let mut posts: Vec<mattermost_api::models::ScheduledPost> =
            scheduled.0.into_values().flatten().collect();
        posts.sort_by_key(|p| p.scheduled_at);

        let rows = posts
            .into_iter()
            .map(|scheduled| {
                // Rescheduling rather than cancel-and-retype: the message is
                // already written, and the usual reason to touch one of these
                // is that the time was wrong.
                let reschedule_ui = self.clone();
                let reschedule = scheduled.clone();
                let cancel_ui = self.clone();
                let id = scheduled.post.id.clone();
                dialogs::Row::new(
                    scheduled.post.id.clone(),
                    format!(
                        "{} {}",
                        format_day(scheduled.scheduled_at),
                        format_time(scheduled.scheduled_at)
                    ),
                )
                .body(scheduled.post.message.clone())
                .icon_button(Lucide::AlarmClock, "Change the time", move |cx| {
                    let ui = reschedule_ui.clone();
                    let scheduled = reschedule.clone();
                    dialogs::schedule_message(&reschedule_ui, cx, move |when_ms, _cx| {
                        let client = ui.state.borrow().client.clone();
                        let mut updated = scheduled.clone();
                        updated.scheduled_at = when_ms;
                        let id = updated.post.id.clone();
                        let ui = ui.clone();
                        runtime::spawn(
                            async move { client.update_scheduled_post(&id, &updated).await },
                            move |result, cx| match result {
                                Ok(_) => ui.toast("Rescheduled.", cx),
                                Err(e) => ui.toast(&format!("Could not reschedule it: {e}"), cx),
                            },
                        );
                    });
                })
                .icon_button(Lucide::Trash, "Cancel", move |_cx| {
                    let client = cancel_ui.state.borrow().client.clone();
                    let id = id.clone();
                    let ui = cancel_ui.clone();
                    runtime::spawn(
                        async move { client.delete_scheduled_post(&id).await },
                        move |result, cx| match result {
                            Ok(()) => ui.toast("Cancelled.", cx),
                            Err(e) => ui.toast(&format!("Could not cancel it: {e}"), cx),
                        },
                    );
                })
            })
            .collect();
        dialogs::show_rows(
            self,
            cx,
            "Scheduled messages",
            rows,
            Some((
                Lucide::AlarmClock,
                "Nothing scheduled",
                "Messages you send later wait here.",
            )),
        );
    }

    /// Signs out of this server, forgetting its token and its cached
    /// messages, and leaves any other server signed in.
    fn sign_out(self: &Rc<Self>, cx: &mut App) {
        let ui = self.clone();
        dialogs::confirm(
            self,
            cx,
            "Sign out?",
            "This device will forget the session. Anything unsent is lost.",
            "Sign Out",
            true,
            move |cx| {
                let client = ui.state.borrow().client.clone();
                let server = client.site_url().to_string();
                crate::cache::clear();
                let stored = server.clone();
                runtime::spawn(
                    async move {
                        if let Err(e) = crate::store::Store::clear(&stored).await {
                            tracing::warn!(error = %e, "could not delete the local messages");
                        }
                    },
                    |_, _| {},
                );
                runtime::spawn(
                    async move {
                        // Revoke it server-side too, so a copy that leaked with
                        // the file is useless rather than merely forgotten.
                        let _ = client.logout().await;
                        crate::session::forget_async(&server).await;
                    },
                    |_, _| {},
                );
                shell::signed_out(cx);
            },
        );
    }

    /// Moves to the next or previous channel with something unread, in
    /// sidebar order. Wraps, because the alternative is a shortcut that
    /// silently stops working at the end of the list.
    fn step_unread(self: &Rc<Self>, forwards: bool, cx: &mut App) {
        let next = {
            let st = self.state.borrow();
            let ordered: Vec<String> = st
                .sidebar_groups()
                .into_iter()
                .flat_map(|(_, channels)| channels)
                .map(|c| c.id)
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

    /// Asks which channel, using the same switcher as Ctrl+K. Picking a
    /// destination is the same act as picking one to read, and a second list
    /// would be a second thing to keep working.
    fn pick_channel(self: &Rc<Self>, on_pick: impl Fn(String, &mut App) + 'static, cx: &mut App) {
        let holder: Rc<RefCell<Option<Rc<switcher::Switcher>>>> = Rc::new(RefCell::new(None));
        let search_ui = self.clone();
        let search_holder = holder.clone();
        let opened = switcher::Switcher::present(
            self,
            cx,
            move |term, cx| {
                let lowered = term.to_lowercase();
                let rows = {
                    let st = search_ui.state.borrow();
                    let mut rows: Vec<switcher::Entry> = st
                        .channels
                        .values()
                        .filter(|c| c.delete_at == 0)
                        .filter_map(|channel| {
                            let title = st.channel_title(channel);
                            (lowered.is_empty() || title.to_lowercase().contains(&lowered)).then(
                                || switcher::Entry {
                                    target: switcher::Target::Channel(channel.id.clone()),
                                    title,
                                    subtitle: String::new(),
                                    icon: channel_icon(channel),
                                },
                            )
                        })
                        .collect();
                    rows.sort_by_key(|row| row.title.to_lowercase());
                    rows.truncate(QUICK_SWITCH_ROWS);
                    rows
                };
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

    /// Ctrl+K: jump to a channel or a person by typing a few letters.
    ///
    /// Channels are matched locally against the sidebar — instant, and the
    /// list is small. People have to be searched for, because the client only
    /// knows the ones it has seen.
    fn quick_switch(self: &Rc<Self>, cx: &mut App) {
        let holder: Rc<RefCell<Option<Rc<switcher::Switcher>>>> = Rc::new(RefCell::new(None));
        let search_ui = self.clone();
        let pick_ui = self.clone();
        let search_holder = holder.clone();

        let opened = switcher::Switcher::present(
            self,
            cx,
            move |term, cx| {
                let lowered = term.to_lowercase();
                let (client, team_id, mut rows) = {
                    let st = search_ui.state.borrow();
                    let mut rows: Vec<switcher::Entry> = st
                        .channels
                        .values()
                        .filter(|c| c.delete_at == 0)
                        .filter_map(|channel| {
                            let title = st.channel_title(channel);
                            (lowered.is_empty() || title.to_lowercase().contains(&lowered)).then(
                                || switcher::Entry {
                                    target: switcher::Target::Channel(channel.id.clone()),
                                    title,
                                    subtitle: String::new(),
                                    icon: channel_icon(channel),
                                },
                            )
                        })
                        .collect();
                    rows.sort_by_key(|row| row.title.to_lowercase());
                    rows.truncate(QUICK_SWITCH_ROWS);
                    (
                        st.client.clone(),
                        st.current_team.clone().unwrap_or_default(),
                        rows,
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

    /// Your own name, nickname, position and picture.
    fn edit_profile(self: &Rc<Self>, cx: &mut App) {
        let (client, me, current) = {
            let st = self.state.borrow();
            let me = &st.me;
            (
                st.client.clone(),
                me.id.clone(),
                (
                    me.first_name.clone(),
                    me.last_name.clone(),
                    me.nickname.clone(),
                    me.position.clone(),
                ),
            )
        };
        let save_ui = self.clone();
        let avatar_ui = self.clone();
        let save_client = client.clone();
        let save_me = me.clone();
        account::edit_profile(self, cx, current,
            move |first, last, nickname, position, _cx| {
                let patch = serde_json::json!({
                    "first_name": first,
                    "last_name": last,
                    "nickname": nickname,
                    "position": position,
                });
                let client = save_client.clone();
                let me = save_me.clone();
                let ui = save_ui.clone();
                runtime::spawn(
                    async move { client.patch_user(&me, &patch).await },
                    move |result, cx| match result {
                        Ok(user) => {
                            ui.state
                                .borrow_mut()
                                .users
                                .insert(user.id.clone(), user.clone());
                            ui.state.borrow_mut().me = user;
                            ui.refresh_all(cx);
                        }
                        Err(e) => ui.toast(&format!("Could not save that: {e}"), cx),
                    },
                );
            },
            move |path, _cx| {
                let client = client.clone();
                let me = me.clone();
                let ui = avatar_ui.clone();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "avatar.png".into());
                runtime::spawn(
                    async move {
                        let bytes = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
                        client
                            .set_profile_image(&me, &name, bytes)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |result, cx| match result {
                        Ok(()) => {
                            // The cached texture is now wrong everywhere it is
                            // drawn, so drop it and let it refetch.
                            let me = ui.state.borrow().me.id.clone();
                            ui.avatars.forget(&me, cx);
                            ui.refresh_all(cx);
                        }
                        Err(e) => ui.toast(&format!("Could not upload that: {e}"), cx),
                    },
                );
            },
        );
    }

    /// The emoji-and-a-line status that shows next to your name.
    fn custom_status(self: &Rc<Self>, cx: &mut App) {
        let (client, current) = {
            let st = self.state.borrow();
            let status = st.me.custom_status();
            (
                st.client.clone(),
                status
                    .map(|s| (s.emoji.clone(), s.text.clone()))
                    .unwrap_or_default(),
            )
        };

        // The last few, which the server keeps as a preference so every client
        // offers the same list.
        let recents: Vec<(String, String)> = self
            .state
            .borrow()
            .preferences
            .iter()
            .find(|p| p.category == "custom_status" && p.name == "recentCustomStatuses")
            .and_then(|p| serde_json::from_str::<Vec<serde_json::Value>>(&p.value).ok())
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| {
                let emoji = entry.get("emoji")?.as_str()?.to_string();
                let text = entry.get("text")?.as_str()?.to_string();
                (!emoji.is_empty() || !text.is_empty()).then_some((emoji, text))
            })
            .take(5)
            .collect();

        let set_client = client.clone();
        let set_ui = self.clone();
        let clear_ui = self.clone();
        account::custom_status(self, cx, current,
            recents,
            move |emoji, text, expires_at, _cx| {
                let status = mattermost_api::models::CustomStatus {
                    emoji,
                    text,
                    duration: if expires_at > 0 {
                        "date_and_time".into()
                    } else {
                        String::new()
                    },
                    // RFC3339, unlike every other time in this API — and it
                    // is decoded into a Go time.Time, which insists on the
                    // colon in the zone offset. `format_iso8601` writes
                    // "+0300", which fails to parse and comes back as
                    // "invalid or missing custom_status", so the format is
                    // spelled out.
                    expires_at: (expires_at > 0)
                        .then(|| crate::timefmt::rfc3339_local(expires_at))
                        .flatten(),
                };
                let client = set_client.clone();
                let ui = set_ui.clone();
                runtime::spawn(
                    async move { client.set_custom_status(&status).await },
                    move |result, cx| match result {
                        Ok(()) => ui.reload_me(cx),
                        Err(e) => ui.toast(&format!("Could not set that: {e}"), cx),
                    },
                );
            },
            move |_cx| {
                let client = client.clone();
                let ui = clear_ui.clone();
                runtime::spawn(
                    async move { client.clear_custom_status().await },
                    move |result, cx| match result {
                        Ok(()) => ui.reload_me(cx),
                        Err(e) => ui.toast(&format!("Could not clear that: {e}"), cx),
                    },
                );
            },
        );
    }

    /// Refetches our own user after changing something the server owns the
    /// canonical version of.
    fn reload_me(self: &Rc<Self>, _cx: &mut App) {
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(async move { client.me().await }, move |result, cx| {
            if let Ok(user) = result {
                let mut st = ui.state.borrow_mut();
                st.users.insert(user.id.clone(), user.clone());
                st.me = user;
                drop(st);
                ui.refresh_all(cx);
            }
        });
    }

    /// Per-channel notification overrides. The dialog is shown with what the
    /// membership currently says, and only what changed is written.
    fn channel_notifications(self: &Rc<Self>, cx: &mut App) {
        let (client, me, channel_id, name, current) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let name = st
                .channel(&channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default();
            let props = st
                .memberships
                .get(&channel_id)
                .map(|m| m.notify_props.clone())
                .unwrap_or_default();
            let desktop = props
                .get("desktop")
                .cloned()
                .unwrap_or_else(|| "default".to_string());
            // "mark_unread: mention" is how Mattermost stores a muted channel,
            // so the switch is the inverse of it.
            let all_activity = props.get("mark_unread").map(String::as_str) != Some("mention");
            let ignore_mentions =
                props.get("ignore_channel_mentions").map(String::as_str) == Some("on");
            (
                st.client.clone(),
                st.me.id.clone(),
                channel_id,
                name,
                (desktop, all_activity, ignore_mentions),
            )
        };

        let ui = self.clone();
        dialogs::channel_notifications(self, cx, &name,
            current,
            move |desktop, all_activity, ignore_mentions, _cx| {
                let mut props = mattermost_api::models::StringMap::new();
                props.insert("desktop".into(), desktop.clone());
                props.insert(
                    "mark_unread".into(),
                    if all_activity { "all" } else { "mention" }.into(),
                );
                props.insert(
                    "ignore_channel_mentions".into(),
                    if ignore_mentions { "on" } else { "off" }.into(),
                );

                let client = client.clone();
                let me = me.clone();
                let channel_id = channel_id.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move {
                        client
                            .set_channel_notify_props(&channel_id, &me, &props)
                            .await
                    },
                    move |result, cx| match result {
                        // The server broadcasts channel_member_updated, which
                        // is what refreshes the muted styling in the sidebar.
                        Ok(()) => ui.schedule_sidebar_reload(cx),
                        Err(e) => ui.toast(&format!("Could not save that: {e}"), cx),
                    },
                );
            },
        );
    }

    /// Account-wide notification settings. These live on the user object's
    /// notify_props, not in preferences — a distinction that trips up most
    /// third-party clients.
    fn account_notifications(self: &Rc<Self>, cx: &mut App) {
        let (client, me, current) = {
            let st = self.state.borrow();
            let props = &st.me.notify_props;
            let desktop = props
                .get("desktop")
                .cloned()
                .unwrap_or_else(|| "mention".to_string());
            let sound = props.get("desktop_sound").map(String::as_str) != Some("false");
            let keys = props.get("mention_keys").cloned().unwrap_or_default();
            let first_name = props.get("first_name").map(String::as_str) == Some("true");
            (
                st.client.clone(),
                st.me.id.clone(),
                (desktop, sound, keys, first_name),
            )
        };

        let ui = self.clone();
        dialogs::account_notifications(self, cx, current,
            move |desktop, sound, keys, first_name, _cx| {
                let patch = serde_json::json!({
                    "notify_props": {
                        "desktop": desktop,
                        "desktop_sound": sound.to_string(),
                        "mention_keys": keys,
                        "first_name": first_name.to_string(),
                    }
                });
                let client = client.clone();
                let me = me.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move { client.patch_user(&me, &patch).await },
                    move |result, cx| match result {
                        Ok(user) => {
                            // These decide every future toast, so the local
                            // copy has to be the server's answer, not ours.
                            ui.state.borrow_mut().me = user;
                        }
                        Err(e) => ui.toast(&format!("Could not save that: {e}"), cx),
                    },
                );
            },
        );
    }

    fn leave_channel(self: &Rc<Self>, cx: &mut App) {
        let (client, me, channel_id, name) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let name = st
                .channel(&channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default();
            (st.client.clone(), st.me.id.clone(), channel_id, name)
        };

        let ui = self.clone();
        dialogs::confirm_leave(self, cx, &name, move |_cx| {
            let client = client.clone();
            let me = me.clone();
            let channel_id = channel_id.clone();
            let ui = ui.clone();
            runtime::spawn(
                async move { client.leave_channel(&channel_id, &me).await },
                move |result, cx| match result {
                    Ok(()) => {
                        ui.state.borrow_mut().current_channel = None;
                        ui.schedule_sidebar_reload(cx);
                        ui.refresh_messages(cx);
                    }
                    Err(e) => ui.toast(&format!("Could not leave: {e}"), cx),
                },
            );
        });
    }

    /// Pinned messages, in the right panel. They are a property of the channel
    /// rather than of any list we already hold, so they are fetched.
    fn show_pinned(self: &Rc<Self>, _cx: &mut App) {
        let (client, channel_id) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_channel.clone())
        };
        let Some(channel_id) = channel_id else { return };

        let ui = self.clone();
        runtime::spawn(
            async move {
                let posts = client.pinned_posts(&channel_id).await?;
                let (authors, statuses) = hydrate_authors(&client, &posts).await;
                Ok::<_, mattermost_api::Error>((posts, authors, statuses))
            },
            move |result, cx| match result {
                Ok((posts, authors, statuses)) => {
                    {
                        let mut st = ui.state.borrow_mut();
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        st.search_results = ChannelFeed::from_list(&posts).posts;
                        st.search_results.reverse();
                        st.searching = false;
                    }
                    ui.right
                        .set_mode(PanelMode::Search("Pinned messages".into()), cx);
                    ui.refresh_panel_mode(cx);
                    ui.overlay.set_show_sidebar(true, cx);
                    ui.refresh_messages(cx);
                }
                Err(e) => ui.toast(&format!("Could not load the pinned messages: {e}"), cx),
            },
        );
    }

    /// Answers the composer's completion query.
    ///
    /// Emoji come from the built-in table, which is local and therefore
    /// instant. Mentions have to be asked for, because who is in a channel is
    /// not something the client holds in full.
    fn complete(self: &Rc<Self>, query: Option<autocomplete::Query>, cx: &mut App) {
        use autocomplete::Query;
        let Some(query) = query else {
            // Closing the list has to cancel what is in flight as well, or a
            // late answer reopens it over a composer nobody is completing in.
            self.completion_generation
                .set(self.completion_generation.get() + 1);
            self.mention_query.borrow_mut().take();
            self.set_completions(Vec::new(), cx);
            return;
        };

        match query {
            Query::Emoji(term) => {
                // Both tables are already here — the built-in one and the
                // server's own, fetched at sign-in — so this is instant and
                // asks nobody.
                let found = crate::emoji::search(
                    &term,
                    &self.state.borrow().custom_emoji,
                    COMPLETIONS,
                );
                let items = found
                    .into_iter()
                    .map(|found| match found {
                        crate::emoji::Found::Unicode(name, glyph) => autocomplete::Candidate {
                            insert: format!(":{name}:"),
                            primary: format!("{glyph}  :{name}:"),
                            secondary: String::new(),
                            emoji: None,
                            image: None,
                            user_id: None,
                        },
                        crate::emoji::Found::Custom(name) => autocomplete::Candidate {
                            insert: format!(":{name}:"),
                            primary: format!(":{name}:"),
                            secondary: String::new(),
                            emoji: Some(name),
                            image: None,
                            user_id: None,
                        },
                    })
                    .collect();
                self.set_completions(items, cx);
            }
            Query::Mention(term) => {
                let lowered = term.to_lowercase();
                let (client, team_id, channel_id, local) = {
                    let st = self.state.borrow();
                    let display = st.teammate_name_display().to_string();

                    // Answered from memory first, before anything touches the
                    // network. This is what keeps the list under a frame: a
                    // round trip is tens of milliseconds at best, and the
                    // names most likely to be wanted are already here.
                    let local = local_mentions(st.users.values(), &lowered, &display, &|id| {
                        self.avatars.texture(id)
                    });

                    (
                        st.client.clone(),
                        st.current_team.clone().unwrap_or_default(),
                        st.current_channel.clone().unwrap_or_default(),
                        local,
                    )
                };
                self.set_completions(local.clone(), cx);

                if channel_id.is_empty() {
                    return;
                }

                // Every keystroke invalidates whatever is already in flight.
                self.completion_generation
                    .set(self.completion_generation.get() + 1);

                // Only the last keystroke of a burst is asked about. Typing a
                // name is half a dozen letters, and the server is answering
                // the word, not each letter of it — without this, six round
                // trips race each other and five of them are thrown away.
                // The local list is already on screen, so the wait costs
                // nothing anyone can see.
                *self.mention_query.borrow_mut() = Some((term, team_id, channel_id, client));
                if self.mention_query_pending.get() {
                    return;
                }
                self.mention_query_pending.set(true);
                let ui = self.clone();
                runtime::after(MENTION_DEBOUNCE, move |cx| {
                    ui.mention_query_pending.set(false);
                    // Whatever the latest keystroke left behind, not the one
                    // that started the timer.
                    let pending = ui.mention_query.borrow_mut().take();
                    if let Some((term, team_id, channel_id, client)) = pending {
                        ui.fetch_mentions(term, team_id, channel_id, client, cx);
                    }
                });
            }
        }
    }

    /// The server's half of `@mention` completion: everyone in the channel
    /// this client has never heard of, plus the groups. Debounced by its
    /// caller, so this runs once per typed word rather than once per letter.
    fn fetch_mentions(
        self: &Rc<Self>,
        term: String,
        team_id: String,
        channel_id: String,
        client: mattermost_api::Client,
        _cx: &mut App,
    ) {
        let generation = self.completion_generation.get();
        let ui = self.clone();
        let groups_client = client.clone();
        let group_term = term.clone();
        let group_ui = self.clone();
        runtime::spawn(
            async move {
                client
                    .autocomplete_users(&term, &team_id, &channel_id)
                    .await
            },
            move |result, cx| {
                if ui.completion_generation.get() != generation {
                    return;
                }
                let Ok(found) = result else { return };
                let display = ui.state.borrow().teammate_name_display().to_string();
                // People in the channel first; the server already
                // separates them, and suggesting someone who is not
                // here would post a mention that notifies nobody.
                let items: Vec<autocomplete::Candidate> = found
                    .users
                    .iter()
                    .chain(found.out_of_channel.iter())
                    .take(COMPLETIONS)
                    .map(|user| autocomplete::Candidate {
                        insert: format!("@{}", user.username),
                        // The name first: it is what somebody is
                        // looking for, and the handle is how the
                        // account is spelled.
                        primary: user.display_name(&display),
                        secondary: format!("@{}", user.username),
                        emoji: None,
                        image: ui.avatars.texture(&user.id),
                        user_id: Some(user.id.clone()),
                    })
                    .collect();
                if items.is_empty() {
                    return;
                }
                ui.set_completions(items.clone(), cx);

                runtime::spawn(
                    async move { groups_client.mentionable_groups(&group_term).await },
                    move |result, cx| {
                        if group_ui.completion_generation.get() != generation {
                            return;
                        }
                        let Ok(groups) = result else { return };
                        if groups.is_empty() {
                            return;
                        }
                        let mut items = items;
                        items.extend(groups.into_iter().map(|group| autocomplete::Candidate {
                            insert: format!("@{}", group.name),
                            primary: format!("@{}", group.name),
                            secondary: match group.member_count {
                                Some(n) => {
                                    format!("{} · {n} people", group.display_name)
                                }
                                None => group.display_name,
                            },
                            emoji: None,
                            image: None,
                            user_id: None,
                        }));
                        group_ui.set_completions(items, cx);
                    },
                );
            },
        );
    }

    /// Asks for files and uploads them straight away.
    ///
    /// Uploading on pick rather than on send is what the other clients do, and
    /// it is the reason sending feels instant: by the time a message goes out
    /// its attachments are already on the server.
    fn pick_attachment(self: &Rc<Self>, cx: &mut App) {
        let Some(channel_id) = self.state.borrow().current_channel.clone() else {
            return;
        };
        let picked = cx.prompt_for_paths(gpui_kit::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        let ui = self.clone();
        cx.spawn(async move |cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            if paths.is_empty() {
                return;
            }
            cx.update(|cx| {
                ui.chat.set_uploading(paths.len(), cx);
                for path in paths {
                    ui.upload(&channel_id, path, cx);
                }
            });
        })
        .detach();
    }

    /// Above this a file goes up in chunks through an upload session, so a
    /// dropped connection resumes instead of starting the whole thing again.
    /// Below it, one multipart request is fewer round trips.
    const CHUNKED_ABOVE: u64 = 8 * 1024 * 1024;
    const CHUNK: usize = 4 * 1024 * 1024;

    fn upload(self: &Rc<Self>, channel_id: &str, path: std::path::PathBuf, _cx: &mut App) {
        let client = self.state.borrow().client.clone();
        let channel_id = channel_id.to_string();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());

        let ui = self.clone();
        let label = name.clone();
        runtime::spawn(
            async move {
                // Reading in the worker: a large file would otherwise block
                // the frame this was started from.
                let bytes = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
                if bytes.len() as u64 <= Self::CHUNKED_ABOVE {
                    return client
                        .upload_file(&channel_id, &name, bytes, None)
                        .await
                        .map_err(|e| e.to_string());
                }

                // Chunked: create a session, then send from wherever the
                // server says it got to. There is no Content-Range here — the
                // body simply starts at file_offset.
                let session = client
                    .create_upload_session(&channel_id, &name, bytes.len() as i64)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut offset = session.file_offset as usize;
                loop {
                    let end = (offset + Self::CHUNK).min(bytes.len());
                    let info = client
                        .upload_data(&session.id, bytes[offset..end].to_vec())
                        .await
                        .map_err(|e| e.to_string())?;
                    if let Some(info) = info {
                        return Ok(mattermost_api::models::FileUploadResponse {
                            file_infos: vec![info],
                            ..Default::default()
                        });
                    }
                    if end >= bytes.len() {
                        return Err("the server never finished the upload".to_string());
                    }
                    // Trust the server's idea of where it got to rather than
                    // our own arithmetic: a partial write is its to report.
                    offset = client
                        .upload_session(&session.id)
                        .await
                        .map_err(|e| e.to_string())?
                        .file_offset as usize;
                }
            },
            move |result, cx| {
                match result {
                    Ok(response) => {
                        let mut st = ui.state.borrow_mut();
                        for info in response.file_infos {
                            st.pending_files.push((info.id, info.name.clone()));
                        }
                    }
                    Err(e) => ui.toast(&format!("Could not attach {label}: {e}"), cx),
                }
                ui.chat.upload_finished(cx);
                ui.refresh_attachments(cx);
            },
        );
    }

    fn refresh_attachments(self: &Rc<Self>, cx: &mut App) {
        // The chips are drawn from the state's own list of waiting files, so
        // all there is to do is draw again.
        cx.refresh_windows();
    }

    /// Runs a search and shows the hits in the right panel.
    ///
    /// Search is one of the routes the server refuses while it is busy, so a
    /// failure here is worth saying out loud rather than showing as "no
    /// results" — those mean very different things to whoever is looking.
    fn search(self: &Rc<Self>, terms: String, cx: &mut App) {
        // "file:" scopes the same box to attachments. A second search field
        // would be a second thing to find; Mattermost's own syntax already
        // works this way for `in:` and `from:`.
        if let Some(rest) = terms.strip_prefix("file:") {
            self.search_files(rest.trim().to_string(), cx);
            return;
        }
        let (client, team) = {
            let mut st = self.state.borrow_mut();
            st.searching = true;
            st.search_results.clear();
            st.search_terms = terms.clone();
            st.search_pages = 0;
            st.search_more = false;
            (st.client.clone(), st.current_team.clone())
        };
        let Some(team_id) = team else { return };

        self.right.set_mode(PanelMode::Search(terms.clone()), cx);
        self.right.search_from_the_top();
        self.refresh_panel_mode(cx);
        self.overlay.set_show_sidebar(true, cx);
        self.refresh_messages(cx);
        self.search_page(client, team_id, terms, 0);
    }

    /// The page after the ones already showing, asked for when the list is
    /// scrolled to its end.
    fn search_more(self: &Rc<Self>, cx: &mut App) {
        let (client, team, terms, page) = {
            let mut st = self.state.borrow_mut();
            if st.searching || !st.search_more {
                return;
            }
            st.searching = true;
            (
                st.client.clone(),
                st.current_team.clone(),
                st.search_terms.clone(),
                st.search_pages,
            )
        };
        let Some(team_id) = team else { return };
        self.refresh_messages(cx);
        self.search_page(client, team_id, terms, page);
    }

    /// Fetches one page of hits and adds it under the pages before it.
    fn search_page(
        self: &Rc<Self>,
        client: mattermost_api::Client,
        team_id: String,
        terms: String,
        page: u32,
    ) {
        // The dates in a search are days of the person asking.
        let offset = chrono::Local::now().offset().local_minus_utc();
        let asked = terms.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                let search = mattermost_api::rest::PostSearch {
                    time_zone_offset: offset,
                    page,
                    per_page: SEARCH_PAGE,
                    ..mattermost_api::rest::PostSearch::new(&terms)
                };
                let hits = client.search_posts(&team_id, &search).await?;
                let (authors, statuses) = hydrate_authors(&client, &hits.posts).await;
                Ok::<_, mattermost_api::Error>((hits.posts, authors, statuses))
            },
            move |result, cx| {
                {
                    let mut st = ui.state.borrow_mut();
                    // An answer to a search that has since been replaced by
                    // another has nowhere to go.
                    if st.search_terms != asked || st.search_pages != page {
                        return;
                    }
                    st.searching = false;
                    match result {
                        Ok((posts, authors, statuses)) => {
                            for user in authors {
                                st.users.insert(user.id.clone(), user);
                            }
                            st.apply_statuses(statuses);
                            // Newest first reads better for a search than the
                            // oldest-first order a channel wants, and an
                            // older page goes under the newer ones.
                            let mut found = ChannelFeed::from_list(&posts).posts;
                            found.reverse();
                            st.search_more = found.len() >= SEARCH_PAGE as usize;
                            st.search_pages = page + 1;
                            found.retain(|post| {
                                !st.search_results.iter().any(|known| known.id == post.id)
                            });
                            st.search_results.extend(found);
                        }
                        Err(e) => {
                            st.search_more = false;
                            drop(st);
                            tracing::warn!(error = %e, terms = %asked, page, "search failed");
                            ui.toast(&format!("Search failed: {e}"), cx);
                            ui.refresh_messages(cx);
                            return;
                        }
                    }
                }
                ui.refresh_messages(cx);
            },
        );
    }

    /// Looks the name typed after `from:` up on the server. The people
    /// already known here are listed at once; this adds whoever the client
    /// has not met, when the answer comes — which, on a server of any size,
    /// is most people. It asks the way the composer's `@` list does
    /// (`users/autocomplete`, the team's members), an empty name included:
    /// that is the route the official clients use for this list too.
    fn search_people(self: &Rc<Self>) {
        let Some(typed) = self.search_box.asking_for_person() else {
            return;
        };
        let (client, team_id) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_team.clone().unwrap_or_default())
        };
        let ui = self.clone();
        runtime::spawn(
            {
                let typed = typed.clone();
                async move { client.autocomplete_users(&typed, &team_id, "").await }
            },
            move |result, cx| {
                let found = match result {
                    Ok(found) => found,
                    Err(error) => {
                        tracing::warn!(%error, "could not look up people for the search box");
                        return;
                    }
                };
                // Only for the name still being typed: an answer to "an"
                // must not land in the list for "anna".
                if ui.search_box.asking_for_person().as_deref() != Some(typed.as_str()) {
                    return;
                }
                let rows = {
                    let st = ui.state.borrow();
                    let display = st.teammate_name_display();
                    found
                        .users
                        .iter()
                        .chain(found.out_of_channel.iter())
                        .filter(|user| user.delete_at == 0)
                        .map(|user| search::person(&user.username, &user.display_name(display)))
                        .collect()
                };
                ui.search_box.add_people(rows, cx);
            },
        );
    }

    /// Attachments matching a search, as a list of names to open.
    fn search_files(self: &Rc<Self>, terms: String, cx: &mut App) {
        let (client, team) = {
            let mut st = self.state.borrow_mut();
            st.searching = true;
            st.search_results.clear();
            // One page is all a file search has, and a page of the search
            // before it must not land among its hits.
            st.search_terms = format!("file:{terms}");
            st.search_pages = 0;
            st.search_more = false;
            (st.client.clone(), st.current_team.clone())
        };
        let Some(team_id) = team else { return };

        self.right
            .set_mode(PanelMode::Search(format!("files: {terms}")), cx);
        self.refresh_panel_mode(cx);
        self.overlay.set_show_sidebar(true, cx);
        self.refresh_messages(cx);

        let ui = self.clone();
        runtime::spawn(
            async move { client.search_files(&team_id, &terms).await },
            move |result, cx| {
                match result {
                    Ok(files) => {
                        // A file hit names the post it is attached to, so the
                        // posts are what gets listed — the same rows as any
                        // other search, and clicking one goes to the message.
                        let ids: Vec<String> = files.ordered().map(|f| f.post_id.clone()).collect();
                        ui.load_posts_by_id(ids, cx);
                    }
                    Err(e) => {
                        ui.state.borrow_mut().searching = false;
                        ui.toast(&format!("File search failed: {e}"), cx);
                        ui.refresh_messages(cx);
                    }
                }
            },
        );
    }

    /// Fetches posts by id and shows them as the current search results.
    fn load_posts_by_id(self: &Rc<Self>, ids: Vec<String>, cx: &mut App) {
        if ids.is_empty() {
            self.state.borrow_mut().searching = false;
            self.refresh_messages(cx);
            return;
        }
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                let posts = client.posts_by_ids(&ids).await?;
                let (authors, statuses) = hydrate_authors(&client, &posts).await;
                Ok::<_, mattermost_api::Error>((posts, authors, statuses))
            },
            move |result, cx| {
                {
                    let mut st = ui.state.borrow_mut();
                    st.searching = false;
                    if let Ok((posts, authors, statuses)) = result {
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        st.search_results = ChannelFeed::from_list(&posts).posts;
                        st.search_results.reverse();
                    }
                }
                ui.refresh_messages(cx);
            },
        );
    }

    /// Everything a message's own menu can ask for.
    fn post_action(self: &Rc<Self>, post_id: String, what: PostAction, cx: &mut App) {
        let (client, me, post) = {
            let st = self.state.borrow();
            (st.client.clone(), st.me.id.clone(), st.post(&post_id))
        };
        let Some(post) = post else {
            self.toast("That message is no longer here.", cx);
            return;
        };

        match what {
            PostAction::CopyText => {
                cx.write_to_clipboard(ClipboardItem::new_string(post.message.clone()));
                self.toast("Message copied.", cx);
            }
            PostAction::CopyLink => {
                let team = self
                    .state
                    .borrow()
                    .current_team_name()
                    .unwrap_or_else(|| "_redirect".to_string());
                let link = format!("{}/{team}/pl/{post_id}", client.site_url());
                cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                self.toast("Link copied.", cx);
            }
            PostAction::Summarise => {
                if self.state.borrow().bots.is_empty() {
                    self.toast("This server has no agent to ask.", cx);
                    return;
                }
                let ui = self.clone();
                self.toast("Asking the agent…", cx);
                runtime::spawn(
                    async move { crate::agents::summarise_thread(&client, &post_id).await },
                    move |result, cx| match result {
                        Ok(target) => {
                            ui.dispatch(Action::OpenPost(target.channel_id, target.post_id), cx)
                        }
                        Err(e) => ui.toast(&format!("The agent could not answer: {e}"), cx),
                    },
                );
            }
            PostAction::Remind => {
                let ui = self.clone();
                let me = me.clone();
                dialogs::post_reminder(self, cx, move |when_ms, _cx| {
                    let client = client.clone();
                    let me = me.clone();
                    let post_id = post_id.clone();
                    let ui = ui.clone();
                    runtime::spawn(
                        // The reminder route takes seconds, unlike everything
                        // else in this API.
                        async move {
                            client
                                .set_post_reminder(&me, &post_id, when_ms / 1000)
                                .await
                        },
                        move |result, cx| match result {
                            Ok(()) => ui.toast("You will be reminded.", cx),
                            Err(e) => ui.toast(&format!("Could not set that: {e}"), cx),
                        },
                    );
                });
            }
            PostAction::History => {
                let ui = self.clone();
                runtime::spawn(
                    async move { client.post_edit_history(&post_id).await },
                    move |result, cx| match result {
                        Ok(versions) if versions.is_empty() => {
                            ui.toast("No earlier versions are kept for this message.", cx)
                        }
                        Ok(versions) => ui.show_history(versions, cx),
                        Err(e) => ui.toast(&format!("Could not load the history: {e}"), cx),
                    },
                );
            }
            PostAction::Acknowledge | PostAction::Unacknowledge => {
                let ack = what == PostAction::Acknowledge;
                let ui = self.clone();
                runtime::spawn(
                    async move {
                        if ack {
                            client.acknowledge_post(&me, &post_id).await.map(|_| ())
                        } else {
                            client.unacknowledge_post(&me, &post_id).await
                        }
                    },
                    move |result, cx| {
                        // The server broadcasts the change, which is what
                        // redraws the row; only a failure needs saying.
                        if let Err(e) = result {
                            ui.toast(&format!("Could not do that: {e}"), cx);
                        }
                    },
                );
            }
            PostAction::Forward => {
                // Forwarding is a new message carrying a permalink: the server
                // resolves that back into the original, which is how the other
                // clients do it and why the quoted post stays live rather than
                // becoming a stale copy.
                let team = self
                    .state
                    .borrow()
                    .current_team_name()
                    .unwrap_or_else(|| "_redirect".to_string());
                let link = format!("{}/{team}/pl/{post_id}", client.site_url());
                let ui = self.clone();
                self.pick_channel(move |channel_id, _cx| {
                    let client = client.clone();
                    let link = link.clone();
                    let ui = ui.clone();
                    runtime::spawn(
                        async move { client.send_message(&channel_id, &link, None).await },
                        move |result, cx| match result {
                            Ok(post) => {
                                ui.dispatch(Action::OpenPost(post.channel_id, String::new()), cx)
                            }
                            Err(e) => ui.toast(&format!("Could not forward it: {e}"), cx),
                        },
                    );
                }, cx);
            }
            PostAction::MoveThread => {
                let ui = self.clone();
                self.pick_channel(move |channel_id, _cx| {
                    let client = client.clone();
                    let post_id = post_id.clone();
                    let ui = ui.clone();
                    runtime::spawn(
                        async move { client.move_thread(&post_id, &channel_id).await },
                        move |result, cx| match result {
                            Ok(()) => ui.toast("Thread moved.", cx),
                            Err(e) => ui.toast(&format!("Could not move it: {e}"), cx),
                        },
                    );
                }, cx);
            }
            PostAction::Edit => self.chat.begin_edit(&post_id, post.source_text(), cx),
            PostAction::Delete => self.confirm_delete(post_id, cx),
            PostAction::Pin | PostAction::Unpin => {
                let pin = what == PostAction::Pin;
                let ui = self.clone();
                runtime::spawn(
                    async move { client.pin_post(&post_id, pin).await },
                    move |result, cx| match result {
                        // The server echoes the change as post_edited, so
                        // there is nothing to apply here.
                        Ok(()) => ui.toast(if pin { "Pinned." } else { "Unpinned." }, cx),
                        Err(e) => ui.toast(&format!("Could not change the pin: {e}"), cx),
                    },
                );
            }
            PostAction::Save | PostAction::Unsave => {
                let save = what == PostAction::Save;
                self.set_saved(post_id, save, cx);
            }
            PostAction::MarkUnread => {
                let (crt, channel) = {
                    let st = self.state.borrow();
                    (st.crt_enabled, post.channel_id.clone())
                };
                let ui = self.clone();
                runtime::spawn(
                    async move { client.set_post_unread(&me, &post_id, crt).await },
                    move |result, cx| match result {
                        Ok(()) => {
                            // Nothing is being read here any more, so stop
                            // marking it read on the way out.
                            if ui.state.borrow().current_channel.as_deref()
                                == Some(channel.as_str())
                            {
                                ui.state.borrow_mut().current_channel = None;
                                ui.chat.set_composer_text("", cx);
                            }
                            ui.schedule_sidebar_reload(cx);
                        }
                        Err(e) => ui.toast(&format!("Could not mark it unread: {e}"), cx),
                    },
                );
            }
        }
    }

    /// Deleting is destructive and has no undo, so it asks first.
    fn confirm_delete(self: &Rc<Self>, post_id: String, cx: &mut App) {
        let ui = self.clone();
        dialogs::confirm(
            self,
            cx,
            "Delete this message?",
            "It will be removed for everyone. This cannot be undone.",
            "Delete",
            true,
            move |_cx| {
                let client = ui.state.borrow().client.clone();
                let post_id = post_id.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move { client.delete_post(&post_id).await },
                    move |result, cx| {
                        if let Err(e) = result {
                            ui.toast(&format!("Could not delete it: {e}"), cx);
                        }
                    },
                );
            },
        );
    }

    /// Saving a post is a preference, not a post field, so it is written and
    /// mirrored locally rather than waiting for an echo that never comes.
    fn set_saved(self: &Rc<Self>, post_id: String, save: bool, cx: &mut App) {
        let (client, me) = {
            let mut st = self.state.borrow_mut();
            if save {
                st.saved_posts.insert(post_id.clone());
            } else {
                st.saved_posts.remove(&post_id);
            }
            (st.client.clone(), st.me.id.clone())
        };
        self.refresh_messages(cx);

        let ui = self.clone();
        let id = post_id.clone();
        runtime::spawn(
            async move {
                let pref = mattermost_api::models::Preference {
                    user_id: me.clone(),
                    category: "flagged_post".into(),
                    name: id.clone(),
                    value: "true".into(),
                };
                if save {
                    client.save_preferences(&me, &[pref]).await
                } else {
                    client.delete_preferences(&me, &[pref]).await
                }
            },
            move |result, cx| {
                if let Err(e) = result {
                    // Put the local view back where the server still has it.
                    let mut st = ui.state.borrow_mut();
                    if save {
                        st.saved_posts.remove(&post_id);
                    } else {
                        st.saved_posts.insert(post_id.clone());
                    }
                    drop(st);
                    ui.refresh_messages(cx);
                    ui.toast(&format!("Could not change that: {e}"), cx);
                }
            },
        );
    }

    /// Follows or unfollows the open thread. Following is what keeps a thread
    /// in the inbox after you stop being mentioned in it.
    fn follow_thread(self: &Rc<Self>, following: bool, cx: &mut App) {
        let PanelMode::Thread(root_id) = self.right.mode(cx) else {
            return;
        };
        let (client, team_id) = {
            let st = self.state.borrow();
            (
                st.client.clone(),
                st.current_team.clone().unwrap_or_default(),
            )
        };
        let ui = self.clone();
        runtime::spawn(
            async move { client.follow_thread(&team_id, &root_id, following).await },
            move |result, cx| match result {
                Ok(()) => ui.load_inbox(cx),
                Err(e) => ui.toast(&format!("Could not change that: {e}"), cx),
            },
        );
    }

    /// Shows an integration's form and posts the answers back.
    ///
    /// The server does not interpret the submission — it forwards it to the
    /// integration's own URL, which is why that URL has to be echoed back
    /// exactly as it arrived.
    fn open_dialog(self: &Rc<Self>, request: mattermost_api::models::dialog::OpenDialogRequest, cx: &mut App) {
        let (client, me, channel_id, team_id) = {
            let st = self.state.borrow();
            (
                st.client.clone(),
                st.me.id.clone(),
                st.current_channel.clone().unwrap_or_default(),
                st.current_team.clone().unwrap_or_default(),
            )
        };

        let dialog = request.dialog.clone();
        let submit_ctx = (
            client.clone(),
            request.url.clone(),
            dialog.callback_id.clone(),
            dialog.state.clone(),
            me.clone(),
            channel_id.clone(),
            team_id.clone(),
        );
        let cancel_ctx = submit_ctx.clone();
        let notify_on_cancel = dialog.notify_on_cancel;
        let ui = self.clone();
        let cancel_ui = self.clone();

        interactive::present(self, cx, &dialog,
            move |submission, _cx| {
                let (client, url, callback_id, state, user_id, channel_id, team_id) =
                    submit_ctx.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move {
                        let request = mattermost_api::models::dialog::SubmitDialogRequest {
                            url,
                            callback_id,
                            state,
                            user_id,
                            channel_id,
                            team_id,
                            submission,
                            cancelled: false,
                        };
                        client.submit_dialog(&request).await
                    },
                    move |result, cx| match result {
                        // Errors come back inside a 200 as well, since the
                        // integration is what validated the form.
                        Ok(response) => {
                            if let Some(error) = response
                                .get("error")
                                .and_then(|v| v.as_str())
                                .filter(|e| !e.is_empty())
                            {
                                ui.toast(error, cx);
                            }
                        }
                        Err(e) => ui.toast(&format!("That form was not accepted: {e}"), cx),
                    },
                );
            },
            move |_cx| {
                // Only when the dialog asked to be told; most do not care.
                if !notify_on_cancel {
                    return;
                }
                let (client, url, callback_id, state, user_id, channel_id, team_id) =
                    cancel_ctx.clone();
                let _ = &cancel_ui;
                runtime::spawn(
                    async move {
                        let request = mattermost_api::models::dialog::SubmitDialogRequest {
                            url,
                            callback_id,
                            state,
                            user_id,
                            channel_id,
                            team_id,
                            submission: Default::default(),
                            cancelled: true,
                        };
                        client.submit_dialog(&request).await
                    },
                    |_, _| {},
                );
            },
        );
    }

    /// Presses something on a card. What it does is the integration's
    /// business and comes back over the websocket — the post edited in place,
    /// an ephemeral message, a dialog — so only a failure is ours to report.
    fn card_action(
        self: &Rc<Self>,
        post_id: String,
        action_id: String,
        selected: String,
        cookie: String,
        cx: &mut App,
    ) {
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        let _ = cx;
        runtime::spawn(
            async move {
                client
                    .do_post_action(&post_id, &action_id, &selected, &cookie)
                    .await
            },
            move |result, cx| {
                if let Err(e) = result {
                    ui.toast(&format!("That did not go through: {e}"), cx);
                }
            },
        );
    }

    /// A card's menu over the server's own people or channels. What is
    /// already known here answers at once, so the list is never empty while a
    /// request is out; the server's own search then fills in everyone and
    /// everything this client has not happened to load.
    pub(super) fn pick_from_directory(
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
            if channels { "Search channels" } else { "Search people" },
            move |term, cx| {
                let known = ui.known_directory(channels, &term);
                let Some(picker) = holder.borrow().clone() else { return };
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

    /// Runs a slash command. Its output arrives as a post or an ephemeral
    /// message, so there is usually nothing to show from the response itself.
    fn run_command(self: &Rc<Self>, channel_id: String, command: String, cx: &mut App) {
        let client = self.state.borrow().client.clone();
        self.chat.set_composer_text("", cx);
        let ui = self.clone();
        runtime::spawn(
            async move { client.execute_command(&channel_id, &command).await },
            move |result, cx| match result {
                Ok(response) => {
                    // Some commands answer with somewhere to go rather than
                    // something to say.
                    if let Some(location) = response
                        .get("goto_location")
                        .and_then(|v| v.as_str())
                        .filter(|l| !l.is_empty())
                    {
                        cx.open_url(location);
                    }
                }
                Err(e) => ui.toast(&format!("That command failed: {e}"), cx),
            },
        );
    }

    /// Follows a link to another message on this server.
    ///
    /// The link names only the post, so the channel has to be looked up —
    /// from what is loaded when possible, and from the server when not. A
    /// reminder about a message in a channel you have not opened this session
    /// is exactly the case that would otherwise fail.
    fn open_permalink(self: &Rc<Self>, post_id: String, cx: &mut App) {
        let (client, known) = {
            let st = self.state.borrow();
            (st.client.clone(), st.post(&post_id).map(|p| p.channel_id))
        };
        if let Some(channel_id) = known {
            self.jump_to_post(channel_id, post_id, cx);
            return;
        }

        let ui = self.clone();
        runtime::spawn(
            async move { client.post(&post_id).await },
            move |result, cx| match result {
                Ok(post) => {
                    let channel_id = post.channel_id.clone();
                    let post_id = post.id.clone();
                    ui.state.borrow_mut().apply_post(post);
                    ui.jump_to_post(channel_id, post_id, cx);
                }
                Err(e) => ui.toast(&format!("Could not open that message: {e}"), cx),
            },
        );
    }

    /// Opens a channel and puts one message on screen.
    ///
    /// Whether it is loaded decides what happens: if it is, scroll to it; if
    /// it is not, fetch the page around it, because a hit from three months
    /// ago is not reachable by paging back from today.
    fn jump_to_post(self: &Rc<Self>, channel_id: String, post_id: String, cx: &mut App) {
        self.select_channel(channel_id.clone(), cx);

        // After the channel's own load and layout have had their turn.
        let ui = self.clone();
        runtime::soon(move |cx| {
            if ui.chat.scroll_to_post(&post_id, cx) {
                return;
            }
            let (client, crt) = {
                let st = ui.state.borrow();
                (st.client.clone(), st.crt_enabled)
            };
            runtime::spawn(
                async move {
                    // Half a page either side, so the message lands in the
                    // middle with its context rather than at an edge.
                    let before = client
                        .posts_before(&channel_id, &post_id, INITIAL_POSTS / 2, crt)
                        .await?;
                    let after = client
                        .posts_after(&channel_id, &post_id, INITIAL_POSTS / 2, crt)
                        .await?;
                    let target = client.post(&post_id).await?;
                    let (authors, statuses) = hydrate_authors(&client, &before).await;
                    Ok::<_, mattermost_api::Error>((
                        channel_id, before, after, target, authors, statuses,
                    ))
                },
                move |result, cx| {
                    let Ok((channel_id, before, after, target, authors, statuses)) = result else {
                        ui.toast("That message could not be loaded.", cx);
                        return;
                    };
                    let post_id = target.id.clone();
                    {
                        let mut st = ui.state.borrow_mut();
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        let feed = st.feeds.entry(channel_id).or_default();
                        for post in ChannelFeed::from_list(&before).posts {
                            feed.upsert(post);
                        }
                        for post in ChannelFeed::from_list(&after).posts {
                            feed.upsert(post);
                        }
                        feed.upsert(target);
                        // The feed no longer runs to the newest post, so the
                        // view must not claim it does.
                        feed.at_latest = false;
                    }
                    ui.refresh_messages(cx);
                    let ui = ui.clone();
                    runtime::soon(move |cx| {
                        ui.chat.scroll_to_post(&post_id, cx);
                    });
                },
            );
        });
    }

    /// Fetches everything posted in a channel *after* the newest post we
    /// hold. Used when the socket has been away long enough that the feed has
    /// a hole in it — scrolling up finds older messages, and nothing else
    /// would find the ones in the middle.
    /// Fetches what came after the newest message we hold, a page at a time
    /// until the server says there is nothing later.
    fn load_newer(self: &Rc<Self>, channel_id: String, _cx: &mut App) {
        let (client, crt, newest) = {
            let st = self.state.borrow();
            let Some(feed) = st.feeds.get(&channel_id) else {
                return;
            };
            let Some(newest) = feed.posts.last().map(|p| p.id.clone()) else {
                return;
            };
            (st.client.clone(), st.crt_enabled, newest)
        };

        let ui = self.clone();
        runtime::spawn(
            async move {
                let posts = client
                    .posts_after(&channel_id, &newest, INITIAL_POSTS, crt)
                    .await?;
                let (authors, statuses) = hydrate_authors(&client, &posts).await;
                Ok::<_, mattermost_api::Error>((channel_id, posts, authors, statuses))
            },
            move |result, cx| {
                let Ok((channel_id, posts, authors, statuses)) = result else {
                    return;
                };
                let caught_up = {
                    let mut st = ui.state.borrow_mut();
                    for user in authors {
                        st.users.insert(user.id.clone(), user);
                    }
                    st.apply_statuses(statuses);
                    let newer = ChannelFeed::from_list(&posts);
                    // An empty page is the end whatever else it claims;
                    // asking again would ask the same question forever.
                    let caught_up = newer.at_latest || newer.posts.is_empty();
                    if let Some(feed) = st.feeds.get_mut(&channel_id) {
                        for post in newer.posts {
                            feed.upsert(post);
                        }
                        feed.at_latest = caught_up;
                    }
                    caught_up
                };
                ui.refresh_messages(cx);
                ui.store_posts(posts.chronological().into_iter().cloned().collect(), cx);
                if !caught_up {
                    ui.load_newer(channel_id, cx);
                }
            },
        );
    }

    /// Fetches the page of messages before the oldest one we hold.
    ///
    /// Only one at a time, and never past the beginning: reaching the top of a
    /// short channel would otherwise ask for the same empty page on every
    /// scroll event.
    fn load_older(self: &Rc<Self>, cx: &mut App) {
        if self.loading_older.get() {
            if chat::scroll_trace_enabled() {
                tracing::info!(
                    target: "matterfast::scroll",
                    event = "pagination-suppressed",
                    reason = "already-loading",
                    "scroll trace"
                );
            }
            return;
        }
        let (client, crt, channel_id, oldest) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let Some(feed) = st.feeds.get(&channel_id) else {
                return;
            };
            if feed.at_oldest {
                if chat::scroll_trace_enabled() {
                    tracing::info!(
                        target: "matterfast::scroll",
                        event = "pagination-suppressed",
                        reason = "at-oldest",
                        channel_id,
                        "scroll trace"
                    );
                }
                return;
            }
            let Some(oldest) = feed.posts.first().map(|p| p.id.clone()) else {
                return;
            };
            (st.client.clone(), st.crt_enabled, channel_id, oldest)
        };

        self.loading_older.set(true);
        if chat::scroll_trace_enabled() {
            tracing::info!(
                target: "matterfast::scroll",
                event = "pagination-request",
                channel_id,
                oldest,
                "scroll trace"
            );
        }
        self.chat.set_loading_older(true, cx);
        let ui = self.clone();
        runtime::spawn(
            async move {
                let posts = client
                    .posts_before(&channel_id, &oldest, INITIAL_POSTS, crt)
                    .await?;
                let (authors, statuses) = hydrate_authors(&client, &posts).await;
                Ok::<_, mattermost_api::Error>((channel_id, posts, authors, statuses))
            },
            move |result, cx| {
                ui.chat.set_loading_older(false, cx);
                let Ok((channel_id, posts, authors, statuses)) = result else {
                    if chat::scroll_trace_enabled() {
                        tracing::warn!(
                            target: "matterfast::scroll",
                            event = "pagination-error",
                            "scroll trace"
                        );
                    }
                    ui.loading_older.set(false);
                    ui.chat.retry_older_on_next_edge_change(cx);
                    return;
                };
                if chat::scroll_trace_enabled() {
                    tracing::info!(
                        target: "matterfast::scroll",
                        event = "pagination-response",
                        channel_id,
                        posts = posts.posts.len(),
                        "scroll trace"
                    );
                }
                let kept;
                // Whether the channel this page belongs to is still the one
                // on screen; a page for a feed nobody is looking at only goes
                // into the state.
                let showing;
                {
                    let mut st = ui.state.borrow_mut();
                    for user in authors {
                        st.users.insert(user.id.clone(), user);
                    }
                    st.apply_statuses(statuses);
                    let older = ChannelFeed::from_list(&posts);
                    // An empty page means there is nothing before this, and
                    // the feed should stop asking.
                    let exhausted = older.posts.is_empty();
                    kept = older.posts.clone();
                    let now_at_oldest = exhausted || older.at_oldest;
                    showing = st.current_channel.as_deref() == Some(channel_id.as_str());
                    if let Some(feed) = st.feeds.get_mut(&channel_id) {
                        for post in older.posts {
                            feed.upsert(post);
                        }
                        feed.at_oldest = now_at_oldest;
                    }
                }
                // The new rows are a splice at the top of the list: nothing
                // the reader is looking at is rebuilt, and the list keeps its
                // own anchor across it. This used to redraw everything ever
                // paged into the channel on every page turn.
                if showing {
                    ui.chat.refresh(&ui.state, cx);
                }
                ui.loading_older.set(false);
                ui.store_posts(kept, cx);
            },
        );
    }

    /// A thread's reply box has its own draft, keyed by the thread root —
    /// which is how the server stores them too, so they sync with the other
    /// clients rather than only surviving locally.
    fn schedule_thread_draft_save(self: &Rc<Self>, _cx: &mut App) {
        if self.thread_draft_pending.replace(true) {
            return;
        }
        let ui = self.clone();
        runtime::after(std::time::Duration::from_millis(900), move |cx| {
            ui.thread_draft_pending.set(false);
            ui.save_thread_draft(cx);
        });
    }

    fn save_thread_draft(self: &Rc<Self>, cx: &mut App) {
        let PanelMode::Thread(root_id) = self.right.mode(cx) else {
            return;
        };
        let text = self.right.composer_text(cx);
        let (client, channel_id, synced, changed) = {
            let mut st = self.state.borrow_mut();
            let Some(channel_id) = st.find_post(&root_id).map(|p| p.channel_id.clone()) else {
                return;
            };
            let trimmed = text.trim().to_string();
            let changed = if trimmed.is_empty() {
                st.thread_drafts.remove(&root_id).is_some()
            } else {
                st.thread_drafts.insert(root_id.clone(), trimmed.clone()) != Some(trimmed)
            };
            (st.client.clone(), channel_id, st.drafts_synced, changed)
        };
        if !changed || !synced {
            return;
        }

        let body = text.trim().to_string();
        let draft = mattermost_api::models::Draft::new(&channel_id, &root_id, &body);
        runtime::spawn(
            async move {
                if body.is_empty() {
                    client.delete_draft(&channel_id, &root_id).await
                } else {
                    client.upsert_draft(&draft).await.map(|_| ())
                }
            },
            move |result, _cx| {
                if let Err(e) = result {
                    tracing::warn!(error = %e, "could not save the thread draft");
                }
            },
        );
    }

    fn restore_thread_draft(&self, cx: &mut App) {
        let text = match self.right.mode(cx) {
            PanelMode::Thread(root_id) => self
                .state
                .borrow()
                .thread_drafts
                .get(&root_id)
                .cloned()
                .unwrap_or_default(),
            _ => return,
        };
        self.right.set_composer_text(&text, cx);
    }

    /// Saves the composer as a draft shortly after typing stops.
    ///
    /// Debounced rather than per-keystroke: a draft is worth one request when
    /// someone pauses, not one per character.
    fn schedule_draft_save(self: &Rc<Self>, _cx: &mut App) {
        if self.draft_save_pending.replace(true) {
            return;
        }
        let ui = self.clone();
        runtime::after(std::time::Duration::from_millis(900), move |cx| {
            ui.draft_save_pending.set(false);
            ui.save_draft(cx);
        });
    }

    /// Stores the current composer text for the channel it belongs to, locally
    /// and — when the server keeps drafts — there too.
    fn save_draft(self: &Rc<Self>, cx: &mut App) {
        let text = self.chat.composer_text(cx);
        let (client, channel_id, synced, changed) = {
            let mut st = self.state.borrow_mut();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let trimmed = text.trim().to_string();
            let changed = if trimmed.is_empty() {
                st.drafts.remove(&channel_id).is_some()
            } else {
                st.drafts.insert(channel_id.clone(), trimmed.clone()) != Some(trimmed)
            };
            (st.client.clone(), channel_id, st.drafts_synced, changed)
        };
        if !changed {
            return;
        }
        // The pencil in the sidebar has to follow.
        self.channels.refresh(cx);
        if !synced {
            return;
        }

        let ui = self.clone();
        let body = text.trim().to_string();
        let draft = mattermost_api::models::Draft::new(&channel_id, "", &body);
        runtime::spawn(
            async move {
                // An empty upsert deletes server-side, but the explicit route
                // says what is meant and does not depend on that behaviour
                // staying true.
                if body.is_empty() {
                    client.delete_draft(&channel_id, "").await
                } else {
                    client.upsert_draft(&draft).await.map(|_| ())
                }
            },
            move |result, _cx| {
                if let Err(e) = result {
                    // 501 means the admin turned synced drafts off. Stop
                    // asking; the local copy still works.
                    if matches!(&e, mattermost_api::Error::Api(a) if a.status_code == 501) {
                        tracing::info!("synced drafts are disabled on this server");
                        ui.state.borrow_mut().drafts_synced = false;
                    } else {
                        tracing::warn!(error = %e, "could not save the draft");
                    }
                }
            },
        );
    }

    /// Pulls this team's drafts and puts the current channel's back.
    fn load_drafts(self: &Rc<Self>, _cx: &mut App) {
        let (client, team) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_team.clone())
        };
        let Some(team_id) = team else { return };

        let ui = self.clone();
        runtime::spawn(
            async move { client.my_drafts(&team_id).await },
            move |result, cx| match result {
                Ok(drafts) => {
                    {
                        let mut st = ui.state.borrow_mut();
                        for draft in drafts {
                            if draft.message.is_empty() {
                                continue;
                            }
                            if draft.root_id.is_empty() {
                                st.drafts.insert(draft.channel_id, draft.message);
                            } else {
                                st.thread_drafts.insert(draft.root_id, draft.message);
                            }
                        }
                    }
                    ui.restore_draft(cx);
                    ui.channels.refresh(cx);
                }
                Err(e) => {
                    if matches!(&e, mattermost_api::Error::Api(a) if a.status_code == 501) {
                        ui.state.borrow_mut().drafts_synced = false;
                    }
                }
            },
        );
    }

    /// Puts the current channel's draft into the composer.
    fn restore_draft(&self, cx: &mut App) {
        let text = {
            let st = self.state.borrow();
            st.current_channel
                .as_ref()
                .and_then(|id| st.drafts.get(id))
                .cloned()
                .unwrap_or_default()
        };
        self.chat.set_composer_text(&text, cx);
    }

    /// Repaints the "someone is typing" line, and schedules the repaint that
    /// will clear it. The server never says anyone stopped, so the line has to
    /// expire on its own clock.
    fn refresh_typing(self: &Rc<Self>, cx: &mut App) {
        let names = {
            let mut st = self.state.borrow_mut();
            let Some(channel_id) = st.current_channel.clone() else {
                drop(st);
                self.chat.set_typing(&[], cx);
                return;
            };
            let ids = st.typing_in(&channel_id);
            let display = st.teammate_name_display().to_string();
            ids.iter()
                .filter_map(|id| st.users.get(id).map(|u| u.display_name(&display)))
                .collect::<Vec<_>>()
        };
        let anyone = !names.is_empty();
        self.chat.set_typing(&names, cx);

        // One pending sweep at a time, or every keystroke would add a timer.
        if anyone && !self.typing_sweep_pending.replace(true) {
            let ui = self.clone();
            runtime::after(AppState::TYPING_TTL, move |cx| {
                ui.typing_sweep_pending.set(false);
                ui.refresh_typing(cx);
            });
        }
    }

    /// Tells the server we are typing, at most once every few seconds. The
    /// server repeats to other clients on its own schedule, so sending on every
    /// keystroke would be pure noise.
    fn notify_typing(self: &Rc<Self>, _cx: &mut App) {
        if self.typing_sent_recently.replace(true) {
            return;
        }
        {
            let st = self.state.borrow();
            if let (Some(ws), Some(channel)) = (st.ws.as_ref(), st.current_channel.as_ref()) {
                let _ = ws.typing(channel, "");
            }
        }
        let ui = self.clone();
        runtime::after(std::time::Duration::from_secs(3), move |_cx| {
            ui.typing_sent_recently.set(false);
        });
    }

    /// Refetches the channel list, its memberships and the categories for the
    /// current team.
    ///
    /// Debounced: joining a team, or an admin reorganising channels, produces a
    /// burst of these events, and one reload after the burst is as correct as
    /// nine during it.
    fn schedule_sidebar_reload(self: &Rc<Self>, _cx: &mut App) {
        if self.sidebar_reload_pending.replace(true) {
            return;
        }
        let ui = self.clone();
        runtime::after(std::time::Duration::from_millis(400), move |cx| {
            ui.sidebar_reload_pending.set(false);
            ui.reload_sidebar(cx);
        });
    }

    fn reload_sidebar(self: &Rc<Self>, _cx: &mut App) {
        let (client, team) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_team.clone())
        };
        let Some(team_id) = team else { return };

        let ui = self.clone();
        runtime::spawn(
            async move {
                tokio::try_join!(
                    client.my_channels(&team_id, false, 0),
                    client.my_channel_members(&team_id),
                    client.sidebar_categories(&team_id),
                )
            },
            move |result, cx| {
                let Ok((channels, members, categories)) = result else {
                    // A failed refresh is not worth interrupting anyone over:
                    // the next event, or the next resync, tries again.
                    return;
                };
                {
                    let mut st = ui.state.borrow_mut();
                    st.channels.clear();
                    st.memberships.clear();
                    for c in channels {
                        st.channels.insert(c.id.clone(), c);
                    }
                    for m in members {
                        st.memberships.insert(m.channel_id.clone(), m);
                    }
                    st.categories = categories;
                }
                ui.channels.refresh(cx);
                ui.refresh_title(cx);
                ui.hydrate_dm_teammates(cx);
            },
        );
    }

    /// Unread and mention counts for every team, so the switcher can show
    /// where something is waiting rather than only what is in front of you.
    fn load_team_unreads(self: &Rc<Self>, _cx: &mut App) {
        let (client, crt) = {
            let st = self.state.borrow();
            (st.client.clone(), st.crt_enabled)
        };
        let ui = self.clone();
        runtime::spawn(
            async move { client.my_team_unreads(crt).await },
            move |result, cx| {
                let Ok(unreads) = result else { return };
                {
                    let mut st = ui.state.borrow_mut();
                    st.team_unreads = unreads
                        .into_iter()
                        .map(|u| (u.team_id, (u.msg_count, u.mention_count)))
                        .collect();
                }
                ui.channels.refresh(cx);
            },
        );
    }

    /// Being added to or removed from a team changes the switcher, not the
    /// channel list.
    fn reload_teams(self: &Rc<Self>, _cx: &mut App) {
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(async move { client.my_teams().await }, move |result, cx| {
            if let Ok(teams) = result {
                ui.state.borrow_mut().teams = teams;
                ui.channels.refresh(cx);
            }
        });
    }

    /// A DM channel carries no display name — just `"<idA>__<idB>"` — so until
    /// the other person is in `users` the sidebar row is blank. Nothing in the
    /// startup sequence fetches them, so do it here: after any load that
    /// replaces the channel list.
    fn hydrate_dm_teammates(self: &Rc<Self>, _cx: &mut App) {
        let (client, ids) = {
            let st = self.state.borrow();
            let ids: Vec<String> = st
                .channels
                .values()
                .filter(|c| c.r#type == ChannelType::Direct)
                .filter_map(|c| c.dm_teammate_id(&st.me.id))
                // Missing either half is a reason to ask: a teammate known
                // from a message may still have no presence.
                .filter(|id| !st.users.contains_key(*id) || !st.statuses.contains_key(*id))
                .map(str::to_string)
                .collect();
            (st.client.clone(), ids)
        };
        if ids.is_empty() {
            return;
        }

        let ui = self.clone();
        runtime::spawn(
            async move {
                (
                    client.users_by_ids(&ids).await.unwrap_or_default(),
                    client.statuses_by_ids(&ids).await.unwrap_or_default(),
                )
            },
            move |(users, statuses), cx| {
                {
                    let mut st = ui.state.borrow_mut();
                    for user in users {
                        st.users.insert(user.id.clone(), user);
                    }
                    st.apply_statuses(statuses);
                }
                ui.channels.refresh(cx);
                ui.refresh_title(cx);
            },
        );
    }

    /// Mirrors what the desktop app puts in its tray badge: a single mention
    /// count for the whole account.
    fn refresh_title(&self, cx: &mut App) {
        let mentions = self.state.borrow().total_mentions();
        let title = if mentions > 0 {
            format!("({mentions}) Matterfast")
        } else {
            "Matterfast".to_string()
        };
        self.window
            .update(cx, move |window, _| window.set_window_title(&title));
    }

    fn refresh_call_ui(&self, cx: &mut App) {
        let st = self.state.borrow();
        let available = st.calls.is_some() && st.current_channel.is_some();
        let reason = if st.calls.is_none() {
            Some("Calls are not enabled on this server")
        } else {
            None
        };
        // The call we are in only shows as "ours" on its own channel; switching
        // away leaves it running, exactly as the web client does — which is why
        // the dock, not the header, carries the controls.
        let in_call = st
            .call
            .as_ref()
            .is_some_and(|c| Some(&c.channel_id) == st.current_channel.as_ref());
        let in_progress = st
            .current_channel
            .as_ref()
            .and_then(|id| st.active_calls.get(id))
            .map(Vec::len);
        drop(st);

        self.chat.set_calls_available(available, reason, cx);
        self.chat.set_call_in_progress(in_progress, cx);
        self.chat.set_in_call(in_call, in_progress.is_some(), cx);
        self.dock.refresh(&self.state, cx);
    }

    /// A thread is a place you read alongside the conversation, so it earns a
    /// static column when there is room. The inbox is a stack you glance at and
    /// dismiss, so it always overlays — pushing the conversation aside for it
    /// would be a heavier gesture than the content deserves.
    fn refresh_panel_mode(&self, cx: &mut App) {
        let overlays = self.narrow.get()
            || matches!(self.right.mode(cx), PanelMode::Inbox | PanelMode::Search(_));
        self.overlay.set_collapsed(overlays, cx);
    }

    /// Opens the card for a mention, which names a handle rather than an id.
    ///
    /// The special ones — here, channel, all — address everybody and have no
    /// account behind them, so there is nothing to open.
    /// Fetches people known only by a handle, once each, so that a mention of
    /// somebody this client has not met becomes a mention like any other.
    /// Whoever asked is not told: the messages are simply drawn again.
    pub(super) fn learn_handles(self: &Rc<Self>, handles: Vec<String>) {
        let handles: Vec<String> = {
            let mut asked = self.asked_handles.borrow_mut();
            handles
                .into_iter()
                .filter(|handle| asked.insert(handle.clone()))
                .collect()
        };
        if handles.is_empty() {
            return;
        }
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move { client.users_by_usernames(&handles).await },
            move |result, cx| {
                let Ok(users) = result else { return };
                if users.is_empty() {
                    return;
                }
                {
                    let mut st = ui.state.borrow_mut();
                    for user in users {
                        st.users.insert(user.id.clone(), user);
                    }
                }
                // The text of a message is prepared when the list is built,
                // so the list has to be built again to say their names.
                ui.refresh_messages(cx);
                cx.refresh_windows();
            },
        );
    }

    /// Asks the server about every handle a drawn message mentioned and
    /// nobody here answered to. Cheap when there are none, which is nearly
    /// always, so it can be called on every frame.
    pub(super) fn learn_unknown_mentions(self: &Rc<Self>) {
        let unknown = {
            let st = self.state.borrow();
            if st.unknown_handles.borrow().is_empty() {
                return;
            }
            std::mem::take(&mut *st.unknown_handles.borrow_mut())
        };
        self.learn_handles(unknown.into_iter().collect());
    }

    pub(super) fn show_profile_by_handle(self: &Rc<Self>, handle: &str, cx: &mut App) {
        if matches!(handle, "here" | "channel" | "all") {
            return;
        }
        let known = self
            .state
            .borrow()
            .users
            .values()
            .find(|u| u.username == handle)
            .map(|u| u.id.clone());
        if let Some(user_id) = known {
            self.show_profile(&user_id, cx);
            return;
        }

        // Somebody mentioned in a message we are reading but who has never
        // posted here — worth one lookup rather than a dead link.
        let client = self.state.borrow().client.clone();
        let handles = vec![handle.to_string()];
        let ui = self.clone();
        runtime::spawn(
            async move { client.users_by_usernames(&handles).await },
            move |result, cx| {
                let Ok(users) = result else { return };
                let Some(user) = users.into_iter().next() else {
                    return;
                };
                let id = user.id.clone();
                ui.state.borrow_mut().users.insert(id.clone(), user);
                ui.show_profile(&id, cx);
            },
        );
    }

    pub(super) fn show_profile(self: &Rc<Self>, user_id: &str, cx: &mut App) {
        if profile::show(self, user_id, cx) {
            return;
        }
        // Not held yet — fetch them and try again rather than telling
        // someone to wait for something they cannot make happen.
        let client = self.state.borrow().client.clone();
        let id = user_id.to_string();
        let ui = self.clone();
        runtime::spawn(
            async move { client.user(&id).await },
            move |result, cx| match result {
                Ok(user) => {
                    let id = user.id.clone();
                    ui.state.borrow_mut().users.insert(id.clone(), user);
                    ui.show_profile(&id, cx);
                }
                Err(_) => ui.toast("That person could not be looked up.", cx),
            },
        );
    }

    fn handle(self: &Rc<Self>, action: Action, cx: &mut App) {
        match action {
            Action::SelectTeam(team_id) => self.select_team(team_id, cx),
            Action::SelectChannel(channel_id) => self.select_channel(channel_id, cx),
            Action::Send(text) => {
                // The composer is shared with editing, so what "send" means
                // depends on which mode it is in.
                match self.chat.editing(cx) {
                    Some(post_id) => self.submit_edit(post_id, text, cx),
                    None => self.send_message(text, None, cx),
                }
                // The composer is empty now, so the draft has to go with it —
                // and immediately, not on the debounce.
                self.save_draft(cx);
            }
            Action::ToggleCall => self.toggle_call(cx),
            Action::ToggleMute => self.toggle_mute(cx),
            Action::ToggleRecording => self.toggle_recording(cx),
            Action::ToggleScreen => self.toggle_screen(cx),
            Action::ToggleCamera => self.toggle_camera(cx),
            Action::OpenThread(root_id) => self.open_thread(root_id, cx),
            Action::ReplyInThread(text) => {
                // The reply box is empty after this, and the draft has to go
                // with it — immediately, not on the debounce.
                let flush = self.clone();
                runtime::soon(move |cx| flush.save_thread_draft(cx));
                if let PanelMode::Thread(root) = self.right.mode(cx) {
                    self.send_message(text, Some(root), cx);
                }
            }
            Action::OpenInbox => self.open_inbox(cx),
            Action::CloseRightPanel => {
                self.right.set_mode(PanelMode::Hidden, cx);
                self.overlay.set_show_sidebar(false, cx);
            }
            Action::ToggleReaction(post_id, emoji) => self.toggle_reaction(post_id, emoji, cx),
            Action::CardAction {
                post_id,
                action_id,
                selected,
                cookie,
            } => self.card_action(post_id, action_id, selected, cookie, cx),
            Action::OpenPost(channel_id, root_id) => {
                self.select_channel(channel_id, cx);
                if !root_id.is_empty() {
                    self.open_thread(root_id, cx);
                }
            }
            Action::JumpToPost(channel_id, post_id) => self.jump_to_post(channel_id, post_id, cx),
            Action::OpenDirectMessage(user_id) => self.open_direct_message(user_id, cx),
            Action::Post(post_id, what) => self.post_action(post_id, what, cx),
            Action::Search(terms) => self.search(terms, cx),
            Action::SearchMore => self.search_more(cx),
            Action::SearchPeople => self.search_people(),
            Action::Complete(query) => self.complete(query, cx),
            Action::ScheduleMessage => self.schedule_message(cx),
            Action::ThreadDraftChanged => self.schedule_thread_draft_save(cx),
            Action::LoadOlder => self.load_older(cx),
            Action::FollowThread(following) => self.follow_thread(following, cx),
            Action::PickAttachment => self.pick_attachment(cx),
            Action::AttachFiles(paths) => {
                let Some(channel_id) = self.state.borrow().current_channel.clone() else {
                    return;
                };
                self.chat.set_uploading(paths.len(), cx);
                for path in paths {
                    self.upload(&channel_id, path, cx);
                }
            }
            Action::SummariseUnreads => self.summarise_unreads(cx),
            Action::SetStatus(status) => self.set_status(status, cx),
            Action::Row(channel_id, what) => self.row_action(channel_id, what, cx),
            Action::ToggleHand => self.toggle_hand(cx),
            Action::HostControl(session_id, what) => self.host_control(session_id, what, cx),
            Action::CallReaction(name, glyph) => {
                let Some(session) = self.state.borrow().call.as_ref().map(|c| c.session.clone())
                else {
                    return;
                };
                let ui = self.clone();
                runtime::spawn(
                    async move {
                        session
                            .react(&mattermost_calls::CallReaction {
                                name: name.clone(),
                                literal: glyph,
                                ..Default::default()
                            })
                            .await
                    },
                    move |result, cx| {
                        if let Err(e) = result {
                            ui.toast(&format!("Could not react: {e}"), cx);
                        }
                    },
                );
            }
            Action::DropAttachment(file_id) => {
                self.state
                    .borrow_mut()
                    .pending_files
                    .retain(|(id, _)| id != &file_id);
                self.refresh_attachments(cx);
            }
            Action::ComposerChanged(has_text) => {
                if has_text {
                    self.notify_typing(cx);
                }
                // Emptying the composer is a draft change too — that is how a
                // draft gets deleted.
                self.schedule_draft_save(cx);
            }
            Action::OpenCallChannel => {
                let channel = self
                    .state
                    .borrow()
                    .call
                    .as_ref()
                    .map(|c| c.channel_id.clone());
                if let Some(id) = channel {
                    self.select_channel(id, cx);
                }
            }
        }
    }

    // ------------------------------------------------------------- navigation

    fn select_team(self: &Rc<Self>, team_id: String, cx: &mut App) {
        // On a collapsed window, picking a team should land on that team's
        // channel list rather than straight into a conversation.
        self.split.set_show_content(false, cx);
        {
            let mut st = self.state.borrow_mut();
            if st.current_team.as_deref() == Some(team_id.as_str()) {
                return;
            }
            st.current_team = Some(team_id.clone());
            st.current_channel = None;
        }

        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                tokio::try_join!(
                    client.my_channels(&team_id, false, 0),
                    client.my_channel_members(&team_id),
                    client.sidebar_categories(&team_id),
                )
            },
            move |result, cx| match result {
                Ok((channels, members, categories)) => {
                    let first = {
                        let mut st = ui.state.borrow_mut();
                        st.channels.clear();
                        st.memberships.clear();
                        for c in channels {
                            st.channels.insert(c.id.clone(), c);
                        }
                        for m in members {
                            st.memberships.insert(m.channel_id.clone(), m);
                        }
                        st.categories = categories;
                        st.sidebar_groups()
                            .into_iter()
                            .flat_map(|(_, cs)| cs)
                            .next()
                            .map(|c| c.id)
                    };
                    ui.refresh_all(cx);
                    ui.load_inbox(cx);
                    ui.load_drafts(cx);
                    ui.load_bots(cx);
            ui.load_custom_emoji(cx);
                    if let Some(id) = first {
                        ui.dispatch(Action::SelectChannel(id), cx);
                    }
                }
                Err(e) => ui.toast(&format!("Could not load that team: {e}"), cx),
            },
        );
    }

    fn select_channel(self: &Rc<Self>, channel_id: String, cx: &mut App) {
        self.split.set_show_content(true, cx);
        self.capture_scroll_anchor(cx);
        // Flush the outgoing channel's draft *before* the composer is pointed
        // at a new one, or the text would be filed under the wrong channel.
        self.save_draft(cx);
        {
            let mut st = self.state.borrow_mut();
            if st.current_channel.as_deref() == Some(channel_id.as_str()) {
                return;
            }
            st.current_channel = Some(channel_id.clone());
        }
        self.refresh_messages(cx);
        if let Some(anchor) = self.state.borrow().scroll_anchors.get(&channel_id).cloned() {
            self.chat.restore_anchor(&anchor, cx);
        }
        self.refresh_call_ui(cx);
        self.refresh_typing(cx);
        self.restore_draft(cx);
        self.schedule_snapshot(cx);

        let (client, crt, have_feed) = {
            let st = self.state.borrow();
            (
                st.client.clone(),
                st.crt_enabled,
                st.feeds.contains_key(&channel_id),
            )
        };

        self.chat.set_loading(!have_feed, cx);
        // A channel with unread messages opens *at* them rather than at the
        // newest post — the same fetch the other clients use, so the page
        // arrives centred on where reading stopped instead of needing a scroll
        // back to find it.
        let unread = self.state.borrow().unread(&channel_id).is_unread();

        if !have_feed {
            // Repaint now that the pane knows it is waiting; the fetch below
            // may take a while and the reader should not be looking at "this
            // is the beginning of…" in the meantime.
            self.refresh_messages(cx);
            let id = channel_id.clone();
            let fetch_client = client.clone();
            runtime::spawn(
                async move {
                    let posts = if unread {
                        fetch_client
                            .posts_around_unread(&id, INITIAL_POSTS / 2, INITIAL_POSTS / 2, crt)
                            .await?
                    } else {
                        fetch_client
                            .posts_for_channel(&id, 0, INITIAL_POSTS, crt)
                            .await?
                    };
                    let (authors, statuses) = hydrate_authors(&fetch_client, &posts).await;
                    Ok::<_, mattermost_api::Error>((posts, authors, statuses))
                },
                {
                    let ui = self.clone();
                    let channel_id = channel_id.clone();
                    move |result, cx| match result {
                        Ok((posts, authors, statuses)) => {
                            {
                                let mut st = ui.state.borrow_mut();
                                for user in authors {
                                    st.users.insert(user.id.clone(), user);
                                }
                                st.apply_statuses(statuses);
                                st.feeds
                                    .insert(channel_id.clone(), ChannelFeed::from_list(&posts));
                            }
                            ui.chat.set_loading(false, cx);
                            ui.refresh_messages(cx);
                            if let Some(anchor) =
                                ui.state.borrow().scroll_anchors.get(&channel_id).cloned()
                            {
                                ui.chat.restore_anchor(&anchor, cx);
                            }
                            ui.chat.focus_composer(cx);
                            ui.resolve_mentions(cx);
                            // Straight into the store, so the next launch has
                            // this channel without asking for it again.
                            ui.store_posts(posts.chronological().into_iter().cloned().collect(), cx);
                        }
                        Err(e) => {
                            ui.chat.set_loading(false, cx);
                            ui.refresh_messages(cx);
                            ui.toast(&format!("Could not load messages: {e}"), cx);
                        }
                    }
                },
            );
        } else {
            self.chat.focus_composer(cx);
            // A feed that came out of the cache is last time's picture: it
            // is worth showing at once and wrong until it has been topped up.
            let behind = self
                .state
                .borrow()
                .feeds
                .get(&channel_id)
                .is_some_and(|feed| !feed.at_latest);
            if behind {
                self.load_newer(channel_id.clone(), cx);
            }
        }

        // How many people are here. Cheap, cached by the server, and the
        // question "who can see this" comes up constantly in a channel you
        // have just walked into.
        {
            let stats_client = client.clone();
            let id = channel_id.clone();
            let ui = self.clone();
            runtime::spawn(
                async move { stats_client.channel_stats(&id).await },
                move |result, cx| {
                    if let Ok(stats) = result {
                        ui.chat.set_member_count(Some(stats.member_count), cx);
                    }
                },
            );
        }

        // Tell the server we are looking at this channel so read state syncs to
        // our other sessions.
        let id = channel_id.clone();
        let view_client = client.clone();
        runtime::spawn(
            async move { view_client.view_channel(&id, "", crt).await },
            |_, _| {},
        );

        // Locally zero the unread counters so the sidebar reacts immediately.
        {
            let mut st = self.state.borrow_mut();
            let total = st
                .channels
                .get(&channel_id)
                .map(|c| (c.total_msg_count, c.total_msg_count_root));
            if let (Some((total, total_root)), Some(member)) =
                (total, st.memberships.get_mut(&channel_id))
            {
                member.msg_count = total;
                member.msg_count_root = total_root;
                member.mention_count = 0;
                member.mention_count_root = 0;
                member.urgent_mention_count = 0;
            }
        }
        self.channels.refresh(cx);
        self.refresh_title(cx);
    }

    fn open_direct_message(self: &Rc<Self>, user_id: String, _cx: &mut App) {
        let (client, me) = {
            let st = self.state.borrow();
            (st.client.clone(), st.me.id.clone())
        };
        let ui = self.clone();
        runtime::spawn(
            async move { client.create_direct_channel(&me, &user_id).await },
            move |result, cx| match result {
                Ok(channel) => {
                    let id = channel.id.clone();
                    ui.state.borrow_mut().channels.insert(id.clone(), channel);
                    ui.channels.refresh(cx);
                    ui.dispatch(Action::SelectChannel(id), cx);
                }
                Err(e) => ui.toast(&format!("Could not open that conversation: {e}"), cx),
            },
        );
    }

    // ---------------------------------------------------------------- threads

    fn open_thread(self: &Rc<Self>, root_id: String, cx: &mut App) {
        // Flush the previous thread's reply before the box is reused.
        self.save_thread_draft(cx);
        self.right.set_mode(PanelMode::Thread(root_id.clone()), cx);
        self.refresh_panel_mode(cx);
        self.overlay.set_show_sidebar(true, cx);
        self.restore_thread_draft(cx);
        let following = {
            let st = self.state.borrow();
            st.thread_inbox
                .iter()
                .any(|t| t.id == root_id && t.is_following)
        };
        self.right.set_following(following, cx);

        // Opening a thread is reading it, so its unread count should go —
        // the inbox badge only ever grew before.
        {
            let (client, team_id, unread) = {
                let st = self.state.borrow();
                let unread = st
                    .thread_inbox
                    .iter()
                    .any(|t| t.id == root_id && t.unread_replies > 0);
                (
                    st.client.clone(),
                    st.current_team.clone().unwrap_or_default(),
                    unread,
                )
            };
            if unread && !team_id.is_empty() {
                let id = root_id.clone();
                let ui = self.clone();
                let now = now_ms();
                runtime::spawn(
                    async move { client.mark_thread_read(&team_id, &id, now).await },
                    move |result, cx| {
                        if result.is_ok() {
                            ui.load_inbox(cx);
                        }
                    },
                );
            }
        }

        // Seed the panel from the root we already hold, so it draws the
        // message immediately and fills in the replies when they arrive.
        // Waiting for the round trip is what made opening a thread feel slow
        // and, on a slow link, look like nothing had happened.
        {
            let mut st = self.state.borrow_mut();
            if !st.threads.contains_key(&root_id) {
                if let Some(root) = st.post(&root_id) {
                    st.threads
                        .insert(root_id.clone(), ChannelFeed::from_posts(vec![root]));
                }
            }
        }
        self.refresh_thread_panel(cx);

        // Always refetch. A thread we opened earlier may have grown, and the
        // root's reply count is not enough to tell which replies we hold.
        let (client, crt) = {
            let st = self.state.borrow();
            (st.client.clone(), st.crt_enabled)
        };
        let id = root_id.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                let list = client.post_thread(&id, crt).await?;
                let (authors, statuses) = hydrate_authors(&client, &list).await;
                Ok::<_, mattermost_api::Error>((list, authors, statuses))
            },
            move |result, cx| match result {
                Ok((list, authors, statuses)) => {
                    {
                        let mut st = ui.state.borrow_mut();
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        st.threads
                            .insert(root_id.clone(), ChannelFeed::from_list(&list));
                    }
                    // Both panes here: a fetched thread can change the reply
                    // footer in the feed behind it. Off the click's critical
                    // path, so the cost does not show.
                    ui.refresh_messages(cx);
                    // A slow fetch can lose the race to a click elsewhere —
                    // only steal focus into the reply box if the panel is
                    // still showing the thread this answer is for.
                    if ui.right.mode(cx) == PanelMode::Thread(root_id) {
                        ui.right.focus_composer(cx);
                    }
                }
                Err(e) => {
                    ui.toast(&format!("Could not load the thread: {e}"), cx);
                    // Stop the panel claiming it is still loading. Whatever we
                    // hold of the root is better than a spinner that never
                    // resolves.
                    let root = ui.state.borrow().find_post(&root_id).cloned();
                    if let Some(root) = root {
                        ui.state.borrow_mut().threads.insert(
                            root_id,
                            ChannelFeed {
                                posts: vec![root],
                                ..Default::default()
                            },
                        );
                    }
                    ui.refresh_messages(cx);
                }
            },
        );
    }

    fn open_inbox(self: &Rc<Self>, cx: &mut App) {
        self.right.set_mode(PanelMode::Inbox, cx);
        self.refresh_panel_mode(cx);
        self.overlay.set_show_sidebar(true, cx);

        // Land on whichever tab has something to show: unread threads with no
        // mentions would otherwise open onto an empty list.
        let st = self.state.borrow();
        let threads_first = st.mentions.is_empty() && st.unread_threads() > 0;
        drop(st);
        if threads_first {
            self.right.show_threads_tab(cx);
        }

        self.refresh_messages(cx);
        self.load_inbox(cx);
    }

    /// Fetches recent mentions and the thread inbox for the current team.
    fn load_inbox(self: &Rc<Self>, _cx: &mut App) {
        let (client, team_id, username, crt, me) = {
            let st = self.state.borrow();
            let Some(team) = st.current_team.clone() else {
                return;
            };
            (
                st.client.clone(),
                team,
                st.me.username.clone(),
                st.crt_enabled,
                st.me.id.clone(),
            )
        };

        let ui = self.clone();
        runtime::spawn(
            async move {
                // Mattermost's "Recent Mentions" is literally a search for your
                // mention keys; @username is the one every account has.
                let mentions = client
                    .search_posts(&team_id, &mattermost_api::rest::PostSearch::new(&format!("@{username}")))
                    .await
                    .ok();
                let threads = if crt {
                    client.my_threads(&team_id, false, INBOX_PAGE).await.ok()
                } else {
                    None
                };

                // Saved posts are a preference list of ids; the posts
                // themselves have to be fetched, or the saved tab can only
                // show whatever a channel happened to load.
                let saved = client.flagged_posts(&me, INBOX_PAGE).await.ok();

                let mut ids: HashSet<String> = HashSet::new();
                if let Some(s) = &saved {
                    ids.extend(s.posts.values().map(|p| p.user_id.clone()));
                }
                if let Some(m) = &mentions {
                    ids.extend(m.posts.posts.values().map(|p| p.user_id.clone()));
                }
                if let Some(t) = &threads {
                    ids.extend(t.threads.iter().map(|t| t.post.user_id.clone()));
                }
                let ids: Vec<String> = ids.into_iter().filter(|i| !i.is_empty()).collect();
                let authors = if ids.is_empty() {
                    Vec::new()
                } else {
                    client.users_by_ids(&ids).await.unwrap_or_default()
                };
                (mentions, threads, saved, authors)
            },
            move |(mentions, threads, saved, authors), cx| {
                {
                    let mut st = ui.state.borrow_mut();
                    for user in authors {
                        st.users.insert(user.id.clone(), user);
                    }
                    if let Some(results) = mentions {
                        st.mentions = results.posts.ordered().cloned().collect();
                    }
                    if let Some(threads) = threads {
                        st.thread_inbox = threads.threads;
                    }
                    // Filed under their own channels, so the saved tab and the
                    // conversation agree about what a post says.
                    if let Some(saved) = saved {
                        for post in saved.posts.values() {
                            st.apply_post(post.clone());
                        }
                    }
                }
                ui.refresh_messages(cx);
            },
        );
    }

    // ---------------------------------------------------------------- posting

    fn send_message(self: &Rc<Self>, text: String, root_id: Option<String>, cx: &mut App) {
        let (client, channel_id, me, file_ids) = {
            let mut st = self.state.borrow_mut();
            let channel = match &root_id {
                // A reply belongs to the root's channel, which is not
                // necessarily the one on screen.
                Some(root) => st
                    .find_post(root)
                    .map(|p| p.channel_id.clone())
                    .or_else(|| st.current_channel.clone()),
                None => st.current_channel.clone(),
            };
            let Some(id) = channel else { return };
            // Attachments leave the queue with the message they go out on.
            let files: Vec<String> = st.pending_files.drain(..).map(|(id, _)| id).collect();
            (st.client.clone(), id, st.me.id.clone(), files)
        };
        // An empty message with nothing attached is not a message.
        if text.trim().is_empty() && file_ids.is_empty() {
            return;
        }
        // A slash command is an instruction to the server, not a message. It
        // was being posted as literal text, which is how "/away" ended up in
        // channels as a joke about the client.
        if text.starts_with('/') && !text.starts_with("//") && file_ids.is_empty() {
            self.run_command(channel_id, text, cx);
            return;
        }
        let priority = self.chat.priority(cx);
        self.chat.reset_priority(cx);
        self.refresh_attachments(cx);

        // Show it immediately. The websocket echo replaces this copy — matched
        // on `pending_post_id` — so a slow round trip never looks like a
        // dropped message.
        let pending_id = format!("pending{}", unique());
        let optimistic = Post {
            id: pending_id.clone(),
            pending_post_id: pending_id.clone(),
            channel_id: channel_id.clone(),
            user_id: me,
            message: text.clone(),
            root_id: root_id.clone().unwrap_or_default(),
            create_at: now_ms(),
            file_ids: file_ids.clone(),
            ..Default::default()
        };
        self.state.borrow_mut().apply_post(optimistic.clone());
        self.refresh_messages(cx);
        // Both send paths — the channel composer and the thread panel's —
        // come through here, so the rule that your own message brings you to
        // the bottom lives here rather than at either call site.
        self.chat.follow_own_post(&optimistic, &self.state, cx);

        let ui = self.clone();
        let reply_to = root_id.clone();
        let placeholder_id = pending_id.clone();
        runtime::spawn(
            async move {
                let mut post = Post {
                    channel_id,
                    message: text,
                    root_id: reply_to.unwrap_or_default(),
                    pending_post_id: pending_id,
                    file_ids,
                    ..Default::default()
                };
                if !priority.is_empty() {
                    // Priority travels in the post's metadata, which the
                    // server reads on create and echoes back on the result.
                    post.metadata.get_or_insert_with(Default::default).priority =
                        Some(mattermost_api::models::PostPriority {
                            priority: Some(priority),
                            requested_ack: None,
                            persistent_notifications: None,
                        });
                }
                client.create_post(&post).await
            },
            move |result, cx| match result {
                Ok(post) => {
                    {
                        let mut st = ui.state.borrow_mut();
                        // Retire the optimistic copy explicitly rather than
                        // trusting the response to carry `pending_post_id`
                        // back — otherwise a server that drops it leaves the
                        // message on screen twice.
                        st.remove_post(&placeholder_id);
                        st.apply_post(post);
                    }
                    ui.refresh_messages(cx);
                }
                Err(e) => {
                    ui.toast(&format!("Message not sent: {e}"), cx);
                    // Take the optimistic copy back down: leaving it there
                    // would claim the message was sent.
                    ui.state.borrow_mut().remove_post(&placeholder_id);
                    ui.refresh_messages(cx);
                }
            },
        );
    }

    fn toggle_reaction(self: &Rc<Self>, post_id: String, emoji: String, cx: &mut App) {
        let (client, me, currently_mine) = {
            let st = self.state.borrow();
            (
                st.client.clone(),
                st.me.id.clone(),
                st.has_my_reaction(&post_id, &emoji),
            )
        };

        // Optimistic, like the send path: a reaction is the one interaction
        // where a round trip is very visible.
        let reaction = Reaction {
            user_id: me.clone(),
            post_id: post_id.clone(),
            emoji_name: emoji.clone(),
            ..Default::default()
        };
        self.state
            .borrow_mut()
            .apply_reaction(&reaction, !currently_mine);
        self.refresh_messages(cx);

        let ui = self.clone();
        let request_post = post_id.clone();
        let request_emoji = emoji.clone();
        runtime::spawn(
            async move {
                if currently_mine {
                    client
                        .remove_reaction(&me, &request_post, &request_emoji)
                        .await
                } else {
                    client
                        .add_reaction(&me, &request_post, &request_emoji)
                        .await
                        .map(|_| ())
                }
            },
            move |result, cx| {
                if let Err(e) = result {
                    ui.toast(&format!("Reaction failed: {e}"), cx);
                    // Put it back the way it was.
                    ui.state
                        .borrow_mut()
                        .apply_reaction(&reaction, currently_mine);
                    ui.refresh_messages(cx);
                }
            },
        );
    }

    /// The call button: join the current channel's call, or hang up.
    fn toggle_call(self: &Rc<Self>, cx: &mut App) {
        let st = self.state.borrow();
        if let Some(call) = &st.call {
            let session = call.session.clone();
            drop(st);
            // Dropping `ActiveCall` stops the microphone; the SFU is told
            // separately, and either way we are out of the call.
            self.close_videos(cx);
            self.state.borrow_mut().call = None;
            self.refresh_call_ui(cx);
            runtime::spawn(async move { session.leave().await }, |_, _| {});
            return;
        }
        let Some(channel_id) = st.current_channel.clone() else {
            return;
        };
        let (Some(discovery), Some(ws)) = (st.calls.clone(), st.ws.clone()) else {
            drop(st);
            self.toast("Calls are not enabled on this server.", cx);
            return;
        };
        let client = st.client.clone();
        drop(st);

        let ui = self.clone();
        runtime::spawn(
            async move {
                let opts = JoinOptions {
                    channel_id,
                    ..Default::default()
                };
                CallSession::join(&client, ws, opts, &discovery).await
            },
            move |result, cx| match result {
                Ok(session) => ui.call_joined(session, cx),
                Err(e) => ui.toast(&format!("Could not join the call: {e}"), cx),
            },
        );
    }

    /// Starts audio for a freshly joined call and subscribes to its updates.
    fn call_joined(self: &Rc<Self>, session: Arc<CallSession>, cx: &mut App) {
        let audio = match AudioIo::start(session.clone()) {
            Ok(audio) => audio,
            Err(e) => {
                self.toast(&format!("No audio for this call: {e}"), cx);
                runtime::spawn(async move { session.leave().await }, |_, _| {});
                return;
            }
        };
        self.state.borrow_mut().call = Some(ActiveCall {
            channel_id: session.channel_id().to_string(),
            recording: false,
            roster: HashMap::new(),
            speaking: Vec::new(),
            sharing: Vec::new(),
            host_id: String::new(),
            sessions: HashMap::new(),
            muted_users: HashSet::new(),
            hands: Vec::new(),
            screen: None,
            camera: None,
            audio,
            // Mattermost clients join muted, and so does the session itself.
            muted: session.is_muted(),
            session: session.clone(),
        });

        let updates = session.subscribe();
        runtime::spawn_stream(
            move |tx| async move {
                let mut rx = updates;
                loop {
                    match rx.recv().await {
                        Ok(update) => {
                            if tx.send(update).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            },
            {
                let ui = self.clone();
                move |update, cx| ui.apply_call_update(update, cx)
            },
        );

        // The roster arrived before that subscription existed.
        if let Some(state) = session.last_state() {
            self.apply_call_update(CallUpdate::State(state), cx);
        }
        self.refresh_call_ui(cx);
    }

    fn toggle_mute(self: &Rc<Self>, _cx: &mut App) {
        let st = self.state.borrow();
        let Some(call) = &st.call else {
            return;
        };
        let session = call.session.clone();
        let muted = call.muted;
        drop(st);

        let ui = self.clone();
        if muted {
            runtime::spawn(
                async move { session.unmute().await },
                move |result, cx| match result {
                    // The capture loop discards frames until it has this track.
                    Ok(track) => {
                        if let Some(call) = ui.state.borrow().call.as_ref() {
                            call.audio.set_track(track);
                        }
                    }
                    Err(e) => ui.toast(&format!("Could not unmute: {e}"), cx),
                },
            );
        } else {
            runtime::spawn(async move { session.mute().await }, move |result, cx| {
                if let Err(e) = result {
                    ui.toast(&format!("Could not mute: {e}"), cx);
                }
            });
        }
    }

    fn toggle_recording(self: &Rc<Self>, _cx: &mut App) {
        let st = self.state.borrow();
        let Some(call) = &st.call else {
            return;
        };
        let (client, channel_id, on) =
            (st.client.clone(), call.channel_id.clone(), !call.recording);
        drop(st);

        let ui = self.clone();
        runtime::spawn(
            async move { mattermost_calls::set_recording(&client, &channel_id, on).await },
            move |result, cx| {
                if let Err(e) = result {
                    // Only the host may record, and only when the server
                    // allows it at all; both come back as a plain refusal.
                    ui.toast(&format!("Could not change the recording: {e}"), cx);
                }
            },
        );
    }

    fn toggle_screen(self: &Rc<Self>, cx: &mut App) {
        let st = self.state.borrow();
        let Some(call) = &st.call else {
            return;
        };
        let session = call.session.clone();
        if call.screen.is_some() {
            drop(st);
            if let Some(call) = self.state.borrow_mut().call.as_mut() {
                // Dropping the sender stops the encoder and the portal capture.
                call.screen = None;
            }
            self.refresh_call_ui(cx);
            runtime::spawn(async move { session.stop_screen_share().await }, |_, _| {});
            return;
        }
        drop(st);

        let ui = self.clone();
        runtime::spawn(
            async move {
                // The portal picker comes first: there is no point announcing a
                // share the user is about to cancel.
                let (node_id, fd) = video::pick_screen().await?;
                let track = session
                    .start_screen_share()
                    .await
                    .map_err(|e| e.to_string())?;
                Ok::<_, String>((track, node_id, fd))
            },
            move |result, cx| {
                let sender = result.and_then(|(track, node_id, fd)| {
                    video::VideoSender::screen(track, node_id, fd)
                });
                match sender {
                    Ok(sender) => {
                        if let Some(call) = ui.state.borrow_mut().call.as_mut() {
                            call.screen = Some(sender);
                        }
                        ui.refresh_call_ui(cx);
                    }
                    Err(e) => ui.toast(&format!("Could not share the screen: {e}"), cx),
                }
            },
        );
    }

    fn toggle_camera(self: &Rc<Self>, cx: &mut App) {
        let st = self.state.borrow();
        let Some(call) = &st.call else {
            return;
        };
        let session = call.session.clone();
        if call.camera.is_some() {
            drop(st);
            if let Some(call) = self.state.borrow_mut().call.as_mut() {
                call.camera = None;
            }
            self.refresh_call_ui(cx);
            runtime::spawn(async move { session.stop_video().await }, |_, _| {});
            return;
        }
        drop(st);

        let ui = self.clone();
        runtime::spawn(async move { session.start_video().await }, move |result, cx| {
            let sender = result
                .map_err(|e| e.to_string())
                .and_then(video::VideoSender::camera);
            match sender {
                Ok(sender) => {
                    if let Some(call) = ui.state.borrow_mut().call.as_mut() {
                        call.camera = Some(sender);
                    }
                    ui.refresh_call_ui(cx);
                }
                // The server allows camera only on DM channels, and only
                // when EnableVideo is on.
                Err(e) => ui.toast(&format!("Could not start the camera: {e}"), cx),
            }
        });
    }

    /// Removes one remote picture, if it is on screen.
    fn drop_video(&self, session_id: &str, kind: &str, cx: &mut App) {
        let key = format!("{session_id}:{kind}");
        self.video_views.borrow_mut().retain(|(id, _)| id != &key);
        cx.refresh_windows();
    }

    fn close_videos(&self, cx: &mut App) {
        self.video_views.borrow_mut().clear();
        cx.refresh_windows();
    }

    /// Who a media session belongs to, as far as the roster knows.
    /// A person's display name, or something honest when we do not have them.
    fn user_name(&self, user_id: &str, _cx: &mut App) -> String {
        let st = self.state.borrow();
        st.users
            .get(user_id)
            .map(|user| st.display_name(user))
            .unwrap_or_else(|| "Someone".to_string())
    }

    fn speaker_name(&self, session_id: &str, _cx: &mut App) -> String {
        let st = self.state.borrow();
        st.call
            .as_ref()
            .and_then(|call| call.roster.get(session_id))
            .and_then(|user_id| st.users.get(user_id))
            .map(|user| st.display_name(user))
            .unwrap_or_else(|| "Someone".to_string())
    }

    fn apply_call_update(self: &Rc<Self>, update: CallUpdate, cx: &mut App) {
        if self.state.borrow().call.is_none() {
            // We hung up; whatever is still arriving belongs to a call that is
            // no longer ours.
            return;
        }
        match update {
            CallUpdate::RemoteTrack {
                session_id,
                track_type,
                track,
            } => {
                use mattermost_calls::protocol::track_type as tt;
                match track_type.as_str() {
                    // Screen audio is audio like any other; it just happens to
                    // come from a share rather than a microphone.
                    tt::VOICE | tt::SCREEN_AUDIO => {
                        if let Some(call) = self.state.borrow().call.as_ref() {
                            call.audio.play(session_id, track);
                        }
                    }
                    tt::SCREEN | tt::VIDEO => {
                        tracing::info!(%session_id, track = %track_type, "a remote video arrived");
                        let who = self.speaker_name(&session_id, cx);
                        let title = if track_type == tt::SCREEN {
                            format!("{who} is sharing a screen")
                        } else {
                            format!("{who} — camera")
                        };
                        match video::show_remote(&title, track, |cx| cx.refresh_windows()) {
                            Ok(view) => {
                                let key = format!("{session_id}:{track_type}");
                                let mut views = self.video_views.borrow_mut();
                                views.retain(|(id, _)| id != &key);
                                views.push((key, view));
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "could not show a remote video");
                                self.toast(&format!("Could not show the video: {e}"), cx)
                            }
                        }
                    }
                    other => tracing::debug!(track = other, "ignoring a remote track"),
                }
            }
            // Voice activity and screen shares are what the dock reports, and
            // both arrive per session; the roster turns those into people.
            CallUpdate::Participant(mattermost_calls::CallsEvent::UserSpeaking {
                user_id,
                speaking,
                ..
            }) => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.speaking.retain(|id| id != &user_id);
                    if speaking {
                        call.speaking.insert(0, user_id);
                    }
                }
                self.refresh_call_ui(cx);
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::UserScreenShare {
                user_id,
                session_id,
                sharing,
            }) => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.sharing.retain(|id| id != &user_id);
                    if sharing {
                        call.sharing.push(user_id);
                    }
                }
                // A share ending is also the cue to tear its picture down; the
                // track itself may never close.
                if !sharing {
                    self.drop_video(&session_id, mattermost_calls::protocol::track_type::SCREEN, cx);
                }
                self.refresh_call_ui(cx);
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::UserDismissedNotification {
                user_id,
                ..
            }) => {
                // Answered somewhere else: take the doorbell down here too.
                if user_id == self.state.borrow().me.id {
                    let channel = self.state.borrow().current_channel.clone();
                    if let Some(channel) = channel {
                        self.notifier.withdraw(&notify::call_tag(&channel));
                    }
                }
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::Caption {
                user_id,
                text,
                ..
            }) => {
                let who = self.user_name(&user_id, cx);
                self.dock.set_caption(&format!("{who}:"), &text, cx);
            }
            // Host controls are advisory: the server asks, and the client is
            // what actually mutes or stops sharing. Ignoring them meant a host
            // muting someone did nothing at all on their machine.
            CallUpdate::Participant(mattermost_calls::CallsEvent::HostMuteRequest { .. }) => {
                let muted = self.state.borrow().call.as_ref().is_some_and(|c| c.muted);
                if !muted {
                    self.toggle_mute(cx);
                    self.toast("The host muted you.", cx);
                }
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::HostScreenOffRequest {
                ..
            }) => {
                if self
                    .state
                    .borrow()
                    .call
                    .as_ref()
                    .is_some_and(|c| c.screen.is_some())
                {
                    self.toggle_screen(cx);
                    self.toast("The host stopped your screen share.", cx);
                }
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::HostLowerHandRequest {
                ..
            }) => {
                let raised = {
                    let st = self.state.borrow();
                    st.call
                        .as_ref()
                        .is_some_and(|c| c.hands.iter().any(|id| id == &st.me.id))
                };
                if raised {
                    self.toggle_hand(cx);
                    self.toast("The host lowered your hand.", cx);
                }
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::HostRemoved { .. }) => {
                self.toast("The host removed you from the call.", cx);
                self.toggle_call(cx);
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::HostChanged {
                host_id, ..
            }) => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.host_id = host_id;
                }
                self.refresh_call_ui(cx);
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::UserMuted {
                user_id,
                muted,
                ..
            }) => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    if muted {
                        call.muted_users.insert(user_id);
                    } else {
                        call.muted_users.remove(&user_id);
                    }
                }
                self.refresh_call_ui(cx);
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::UserRaisedHand {
                user_id,
                raised_at,
                ..
            }) => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.hands.retain(|id| id != &user_id);
                    // Zero means the hand went back down.
                    if raised_at > 0 {
                        // Appended, not prepended: the queue is who asked
                        // first.
                        call.hands.push(user_id.clone());
                    }
                }
                self.refresh_call_ui(cx);
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::UserReacted {
                user_id,
                reaction,
                ..
            }) => {
                // In the dock rather than as a toast: a reaction is about the
                // call, and it belongs where the call is. It clears itself,
                // because a reaction is a moment and not a state.
                let who = self.user_name(&user_id, cx);
                let glyph = if reaction.literal.is_empty() {
                    crate::emoji::label(&reaction.name)
                } else {
                    reaction.literal.clone()
                };
                self.dock.set_caption(&who, &glyph, cx);
                let ui = self.clone();
                runtime::after(std::time::Duration::from_secs(4), move |cx| {
                    ui.dock.set_caption("", "", cx);
                });
            }
            CallUpdate::MuteChanged { muted } => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.muted = muted;
                }
                self.refresh_call_ui(cx);
            }
            CallUpdate::State(state) => {
                let mut st = self.state.borrow_mut();
                if let Some(call) = st.call.as_mut() {
                    call.recording = is_running(state.recording.as_ref());
                    call.host_id = state.host_id.clone();
                    call.sessions = state
                        .sessions
                        .iter()
                        .map(|s| (s.user_id.clone(), s.session_id.clone()))
                        .collect();
                    call.muted_users = state
                        .sessions
                        .iter()
                        .filter(|s| !s.unmuted)
                        .map(|s| s.user_id.clone())
                        .collect();
                    call.hands = state
                        .sessions
                        .iter()
                        .filter(|s| s.raised_hand > 0)
                        .map(|s| (s.raised_hand, s.user_id.clone()))
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .map(|(_, id)| id)
                        .collect();
                    call.roster = state
                        .sessions
                        .iter()
                        .map(|s| (s.session_id.clone(), s.user_id.clone()))
                        .collect();
                    let channel = call.channel_id.clone();
                    st.active_calls.insert(channel, participants(&state));
                }
                drop(st);
                self.refresh_call_ui(cx);
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::UserVideo {
                session_id,
                on: false,
                ..
            }) => self.drop_video(&session_id, mattermost_calls::protocol::track_type::VIDEO, cx),
            CallUpdate::Participant(mattermost_calls::CallsEvent::UserLeft {
                session_id, ..
            }) => {
                use mattermost_calls::protocol::track_type as tt;
                self.drop_video(&session_id, tt::SCREEN, cx);
                self.drop_video(&session_id, tt::VIDEO, cx);
            }
            CallUpdate::Participant(mattermost_calls::CallsEvent::JobState { state, .. }) => {
                if state.job_type != "recording" {
                    return;
                }
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.recording = is_running(Some(&state));
                }
                self.refresh_call_ui(cx);
            }
            CallUpdate::Connected(false) | CallUpdate::Ended => {
                self.close_videos(cx);
                self.state.borrow_mut().call = None;
                self.refresh_call_ui(cx);
                self.toast("The call ended.", cx);
            }
            CallUpdate::Error(e) => self.toast(&format!("Call error: {e}"), cx),
            _ => {}
        }
    }

    // ------------------------------------------------------------ ws handling

    fn apply_event(self: &Rc<Self>, event: Event, cx: &mut App) {
        // Calls traffic arrives as plugin-namespaced events and never overlaps
        // with the core event set, so it is cheapest to split it off first.
        if let Some(calls) = mattermost_calls::signaling::parse(&event) {
            self.apply_calls_event(calls, cx);
            return;
        }
        if let Event::Other {
            event: name, data, ..
        } = &event
        {
            if let Some(kind) = name.strip_prefix(REACTION_NOTIFY_PREFIX) {
                self.apply_reaction_notice(kind, data, cx);
                return;
            }
            // An LLM answer being written a token at a time.
            if name.strip_prefix(crate::agents::WS_PREFIX) == Some("postupdate") {
                self.apply_stream_update(data, cx);
                return;
            }
        }

        // A whole-feed rebuild, for the events that really do change every
        // row — somebody's avatar, a presence dot, a channel switch.
        let mut redraw_messages = false;
        // The events that change exactly one post, which is nearly all of
        // them.
        let mut touched = false;
        let mut redraw_sidebar = false;
        let mut redraw_typing = false;
        let mut redraw_draft = false;
        let mut reload_sidebar = false;
        let mut reload_teams = false;
        let mut reload_inbox = false;
        let mut reload_emoji = false;
        let mut open_dialog: Option<mattermost_api::models::dialog::OpenDialogRequest> = None;
        let mut notice: Option<String> = None;
        let mut refetch_post: Option<String> = None;
        let mut forget_avatar: Option<String> = None;
        let mut notify_about: Option<mattermost_api::ws::Posted> = None;
        // Reading a message as it lands is not something to be told about, but
        // only while the window is actually in front of the person.
        let focused_channel = self
            .window_is_active(cx)
            .then(|| self.state.borrow().current_channel.clone())
            .flatten();

        {
            let mut st = self.state.borrow_mut();
            let me = st.me.id.clone();
            let crt = st.crt_enabled;

            match event {
                Event::Posted(posted) => {
                    let post = posted.post.clone();
                    let channel_id = post.channel_id.clone();
                    let is_mine = post.user_id == me;
                    let mentions_me = posted.mentions.contains(&me);
                    let viewing = st.current_channel.as_deref() == Some(channel_id.as_str());
                    let is_reply = post.is_reply();
                    let create_at = post.create_at;

                    st.apply_post(post.clone());

                    // The channel's own counter has to move too, or the unread
                    // maths (total − my count) goes negative on the next read.
                    if let Some(channel) = st.channels.get_mut(&channel_id) {
                        channel.total_msg_count += 1;
                        if !is_reply {
                            channel.total_msg_count_root += 1;
                        }
                        channel.last_post_at = create_at;
                    }
                    if let Some(member) = st.memberships.get_mut(&channel_id) {
                        if is_mine || viewing {
                            // Reading it counts as reading it.
                            member.msg_count += 1;
                            if !is_reply {
                                member.msg_count_root += 1;
                            }
                        } else if mentions_me {
                            member.mention_count += 1;
                            if !is_reply || !crt {
                                member.mention_count_root += 1;
                            }
                        }
                    }
                    if mentions_me && !is_mine {
                        st.mentions.insert(0, post);
                        st.mentions.truncate(INBOX_PAGE as usize);
                    }
                    if viewing {
                        // Under collapsed threads a reply never enters the
                        // feed; what changes there is the root's reply
                        // footer, and only if the root is on screen at all.
                        touched = true;
                    }
                    redraw_sidebar = true;
                    // Decided here, raised below: the decision needs the state
                    // borrow, the toast must not hold it.
                    if notify::should_notify(
                        &posted,
                        &st.me,
                        st.memberships.get(&channel_id),
                        focused_channel.as_deref(),
                    ) {
                        notify_about = Some(posted);
                    }
                }
                // Ephemeral posts are shown like any other, but the server
                // will never mention them again — no edit, no delete, and
                // they are gone on the next fetch. That is the intent.
                // A plugin or slash command asking for a form. Shown outside
                // the state borrow, since it needs the window.
                Event::OpenDialog(request) => open_dialog = Some(*request),
                Event::TeamsChanged => reload_teams = true,
                // The directory is refetched lazily: everyone on screen is
                // already held, and a deactivated user's posts do not vanish.
                Event::UsersChanged => {}
                // Somebody added one: it should be a picture, and on offer,
                // without a restart.
                Event::EmojiChanged => reload_emoji = true,
                Event::Notice { message } => notice = Some(message),
                Event::ThreadsChanged => reload_inbox = true,
                // Nothing on screen depends on these continuously; they matter
                // when one of those windows is open, and it refills on open.
                Event::ListsChanged => {}
                Event::AcknowledgementChanged { post_id } => refetch_post = Some(post_id),
                Event::EphemeralMessage(post) => {
                    touched = true;
                    st.apply_post(*post);
                }
                Event::PostEdited(post) => {
                    touched = true;
                    st.apply_post(post);
                }
                Event::PostDeleted(post) => {
                    if let Some(feed) = st.feeds.get_mut(&post.channel_id) {
                        feed.remove(&post.id);
                    }
                    let root = post.thread_root().to_string();
                    if let Some(thread) = st.threads.get_mut(&root) {
                        thread.remove(&post.id);
                    }
                    touched = true;
                }
                Event::ReactionAdded(reaction) => {
                    st.apply_reaction(&reaction, true);
                    touched = true;
                }
                Event::ReactionRemoved(reaction) => {
                    st.apply_reaction(&reaction, false);
                    touched = true;
                }
                Event::UserUpdated(user) => {
                    // A new picture means the cached texture is stale.
                    let changed = st
                        .users
                        .get(&user.id)
                        .map(|old| old.last_picture_update != user.last_picture_update)
                        .unwrap_or(true);
                    if changed {
                        forget_avatar = Some(user.id.clone());
                    }
                    st.users.insert(user.id.clone(), *user);
                    redraw_messages = true;
                }
                Event::StatusChange { user_id, status } => {
                    let presence = Presence::from(status.as_str());
                    // Only redraw when the dot would actually change colour —
                    // the server repeats a user's current status often enough
                    // that rebuilding the feed each time would be noticeable.
                    // Unknown users render as offline already, hence the
                    // default rather than an Option compare.
                    if st.statuses.insert(user_id, presence).unwrap_or_default() != presence {
                        redraw_messages = true;
                    }
                }
                // Another session marked a channel unread; mirror the counters
                // it reported rather than guessing them.
                Event::PostUnread {
                    channel_id,
                    msg_count,
                    mention_count,
                    ..
                } => {
                    let total = st
                        .channels
                        .get(&channel_id)
                        .map(|c| c.total_msg_count)
                        .unwrap_or(msg_count);
                    if let Some(member) = st.memberships.get_mut(&channel_id) {
                        member.msg_count = total - msg_count;
                        member.msg_count_root = member.msg_count;
                        member.mention_count = mention_count;
                        member.mention_count_root = mention_count;
                    }
                    redraw_sidebar = true;
                }
                Event::Typing {
                    channel_id,
                    user_id,
                    ..
                } => {
                    if user_id != me {
                        st.typing_started(channel_id, user_id);
                        redraw_typing = true;
                    }
                }
                Event::ChannelsViewed { channel_times } => {
                    // Another session read something; mirror it.
                    for (channel_id, at) in channel_times {
                        let totals = st
                            .channels
                            .get(&channel_id)
                            .map(|c| (c.total_msg_count, c.total_msg_count_root));
                        if let (Some((total, root)), Some(member)) =
                            (totals, st.memberships.get_mut(&channel_id))
                        {
                            member.last_viewed_at = Some(at);
                            member.msg_count = total;
                            member.msg_count_root = root;
                            member.mention_count = 0;
                            member.mention_count_root = 0;
                        }
                    }
                    redraw_sidebar = true;
                }
                // Everything that can reshape the channel list. Nine events,
                // one answer: ask the server for the list again. A delta per
                // event would be nine chances to drift out of sync with it,
                // and these arrive rarely enough that three requests is
                // cheaper than being wrong.
                // A draft written on another device. Our own writes carry a
                // Connection-Id, so the server never echoes these back to us.
                Event::DraftCreated(draft) => {
                    if draft.root_id.is_empty() {
                        st.drafts.insert(draft.channel_id, draft.message);
                    } else {
                        st.thread_drafts.insert(draft.root_id, draft.message);
                    }
                    redraw_draft = true;
                }
                Event::DraftDeleted(draft) => {
                    if draft.root_id.is_empty() {
                        st.drafts.remove(&draft.channel_id);
                    } else {
                        st.thread_drafts.remove(&draft.root_id);
                    }
                    redraw_draft = true;
                }
                Event::ChannelCreated { .. }
                | Event::ChannelUpdated { .. }
                | Event::ChannelDeleted { .. }
                | Event::ChannelMemberUpdated { .. }
                | Event::DirectAdded { .. }
                | Event::PreferencesChanged(_)
                | Event::SidebarCategoriesInvalidated { .. } => reload_sidebar = true,
                // Someone joining a channel only matters to the list when the
                // someone is us.
                Event::UserAdded { user_id, .. } | Event::UserRemoved { user_id, .. }
                    if user_id == me =>
                {
                    reload_sidebar = true
                }
                Event::AddedToTeam { user_id, .. } | Event::LeaveTeam { user_id, .. }
                    if user_id == me =>
                {
                    reload_teams = true
                }
                _ => {}
            }
        }

        if let Some(posted) = notify_about {
            let title = if posted.channel_display_name.is_empty() {
                posted.sender_name.clone()
            } else {
                format!("{} — {}", posted.sender_name, posted.channel_display_name)
            };
            self.notify_message(
                &posted.post.channel_id,
                title.trim_start_matches(" — "),
                &notify::body(&posted),
            );
        }
        if reload_teams {
            self.reload_teams(cx);
        }
        if reload_emoji {
            self.load_custom_emoji(cx);
        }
        if reload_inbox {
            self.load_inbox(cx);
        }
        if let Some(request) = open_dialog {
            self.open_dialog(request, cx);
        }
        if let Some(message) = notice.filter(|m| !m.is_empty()) {
            self.toast(&message, cx);
        }
        if let Some(post_id) = refetch_post {
            // The event says which post changed but not to what, and
            // acknowledgements live in the post's metadata.
            let client = self.state.borrow().client.clone();
            let ui = self.clone();
            runtime::spawn(async move { client.post(&post_id).await }, move |result, cx| {
                if let Ok(post) = result {
                    ui.state.borrow_mut().apply_post(post);
                    ui.refresh_messages(cx);
                }
            });
        }
        if reload_sidebar {
            self.schedule_sidebar_reload(cx);
        }
        if redraw_typing {
            self.refresh_typing(cx);
        }
        if redraw_draft {
            self.restore_draft(cx);
            self.restore_thread_draft(cx);
            redraw_sidebar = true;
        }

        if let Some(user_id) = forget_avatar {
            self.avatars.forget(&user_id, cx);
        }
        // One post changed, which is nearly every event. The feed is rebuilt
        // from the state and compared with what the list holds, so this costs
        // one row's worth of layout and never moves the reader.
        redraw_messages |= touched;
        if redraw_messages {
            self.refresh_messages(cx);
        }
        if redraw_sidebar {
            self.channels.refresh(cx);
            self.refresh_title(cx);
        }
    }

    /// Keeps the sidebar's "call in progress" marker and the channel banner
    /// honest. Joining is a separate, explicit action.
    fn apply_calls_event(self: &Rc<Self>, event: mattermost_calls::CallsEvent, cx: &mut App) {
        use mattermost_calls::CallsEvent as Ev;
        let mut touched = true;
        let mut ring: Option<String> = None;
        let mut stop_ringing = false;
        {
            let mut st = self.state.borrow_mut();
            match event {
                Ev::CallStarted { channel_id, .. } => {
                    // Worth interrupting someone for only where a call is
                    // addressed to them: a DM, or a group they are in. A busy
                    // public channel starting calls all day is not a doorbell.
                    let direct = st.channel(&channel_id).is_some_and(|c| {
                        matches!(c.r#type, ChannelType::Direct | ChannelType::Group)
                    });
                    if direct && st.call.is_none() {
                        ring = Some(channel_id.clone());
                    }
                    st.active_calls.entry(channel_id).or_default();
                }
                Ev::CallEnded { channel_id } => {
                    st.active_calls.remove(&channel_id);
                    // Nothing left to answer.
                    stop_ringing = true;
                }
                // Answered or dismissed on another device. It has to be
                // handled here rather than in the joined-call path: while
                // being rung there is no call of ours, and that path returns
                // early when there is not.
                Ev::UserDismissedNotification { .. } => stop_ringing = true,
                // The server only sends a full roster to the joiner, so the
                // list has to follow the individual comings and goings too —
                // otherwise the banner keeps claiming a call we have left.
                Ev::UserJoined {
                    channel_id,
                    user_id,
                    ..
                } => {
                    let people = st.active_calls.entry(channel_id).or_default();
                    // One person can be in a call from two devices, which is
                    // two sessions but still one face.
                    if !people.contains(&user_id) {
                        people.push(user_id);
                    }
                }
                Ev::UserLeft {
                    channel_id,
                    user_id,
                    ..
                } => {
                    if let Some(people) = st.active_calls.get_mut(&channel_id) {
                        people.retain(|id| id != &user_id);
                        if people.is_empty() {
                            st.active_calls.remove(&channel_id);
                        }
                    }
                }
                Ev::CallState { channel_id, state } => {
                    let people = participants(&state);
                    if people.is_empty() {
                        st.active_calls.remove(&channel_id);
                    } else {
                        st.active_calls.insert(channel_id, people);
                    }
                }
                _ => touched = false,
            }
        }
        if stop_ringing {
            self.stop_ringing(cx);
        }
        if let Some(channel_id) = ring {
            self.ring(&channel_id, cx);
        }
        if touched {
            self.channels.refresh(cx);
            self.refresh_call_ui(cx);
        }
    }
}

// ---- free functions
/// The distinct people in a call. A roster is keyed by *session*, and one
/// person joining from two devices holds two of them.
fn participants(state: &mattermost_calls::protocol::CallState) -> Vec<String> {
    let mut people: Vec<String> = Vec::with_capacity(state.sessions.len());
    for session in &state.sessions {
        if !people.contains(&session.user_id) {
            people.push(session.user_id.clone());
        }
    }
    people
}

/// Whether a recording/transcription job is actually running right now.
fn is_running(job: Option<&mattermost_calls::protocol::JobState>) -> bool {
    job.is_some_and(|j| j.start_at > 0 && j.end_at == 0 && j.err.is_empty())
}

/// Post authors are not included in a feed response, so hydrate them before
/// rendering or every message reads "unknown". Presence comes along for the
/// ride: the same ids, and the avatars carry a status badge.
async fn hydrate_authors(
    client: &mattermost_api::Client,
    list: &PostList,
) -> (Vec<User>, Vec<Status>) {
    let ids: Vec<String> = list
        .posts
        .values()
        .map(|p| p.user_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .filter(|id| !id.is_empty())
        .collect();
    if ids.is_empty() {
        return (Vec::new(), Vec::new());
    }
    (
        client.users_by_ids(&ids).await.unwrap_or_default(),
        client.statuses_by_ids(&ids).await.unwrap_or_default(),
    )
}

/// Runs the startup sequence and connects the websocket.
///
/// `on_auth_failure` is only `Some` for a restored session: one whose token
/// came out of the keyring untested (see `restore_session`, which no longer
/// spends a round trip on `client.me()` before drawing anything). If the boot
/// call below is the one that finds out the token is no good, this is what
/// sends the reader to the login form instead of leaving them looking at a
/// window that will never finish loading. A fresh login has no such fallback
/// to offer — its account was already proven live — so it passes `None`.
fn bootstrap(ui: Rc<Ui>, on_auth_failure: Option<Box<dyn FnOnce(&mut App)>>, _cx: &mut App) {
    let client = ui.state.borrow().client.clone();

    // Draw last time's picture first. Everything here is replaced the moment
    // the real data lands; it is on screen so that launching the app shows
    // your channels instead of an empty window for the length of a round trip.
    {
        let pointer = crate::cache::load(client.site_url());
        let server = client.site_url().to_string();
        let ui = ui.clone();
        runtime::spawn(
            async move {
                let store = crate::store::Store::open(&server).await.ok()?;
                let (channels, members) = store.channels().await.ok()?;
                let users = store.users().await.ok()?;
                Some((store, channels, members, users))
            },
            move |loaded, cx| {
                let Some((store, channels, members, users)) = loaded else {
                    return;
                };
                *ui.store.borrow_mut() = Some(store.clone());
                {
                    let mut st = ui.state.borrow_mut();
                    for channel in channels {
                        st.channels.entry(channel.id.clone()).or_insert(channel);
                    }
                    for member in members {
                        st.memberships
                            .entry(member.channel_id.clone())
                            .or_insert(member);
                    }
                    for user in users {
                        st.users.entry(user.id.clone()).or_insert(user);
                    }
                    if let Some(pointer) = &pointer {
                        // Only if the live data has not already answered.
                        if st.current_team.is_none() {
                            st.current_team = pointer.current_team.clone();
                        }
                        st.scroll_anchors.clone_from(&pointer.scroll_anchors);
                    }
                }
                ui.refresh_all(cx);

                // And the messages for wherever we were, so the conversation
                // is there too rather than just the list around it.
                if let Some(channel_id) = pointer
                    .as_ref()
                    .and_then(|p| p.current_channel.clone())
                    .filter(|id| ui.state.borrow().current_channel.as_deref() != Some(id.as_str()))
                {
                    let anchor = pointer
                        .as_ref()
                        .and_then(|p| p.scroll_anchors.get(&channel_id).cloned());
                    let ui = ui.clone();
                    runtime::spawn(
                        async move {
                            let posts = if let Some(post_id) = anchor.as_deref() {
                                let around = store.posts_around(&channel_id, post_id, 10, 10).await;
                                match around {
                                    Ok(posts) if !posts.is_empty() => Ok(posts),
                                    Ok(_) => store.posts(&channel_id, INITIAL_POSTS as usize).await,
                                    Err(error) => Err(error),
                                }
                            } else {
                                store.posts(&channel_id, INITIAL_POSTS as usize).await
                            };
                            (channel_id, anchor, posts)
                        },
                        move |(channel_id, anchor, posts), cx| {
                            let Ok(posts) = posts else { return };
                            if posts.is_empty() {
                                return;
                            }
                            {
                                let mut st = ui.state.borrow_mut();
                                st.feeds
                                    .entry(channel_id.clone())
                                    .or_insert_with(|| ChannelFeed::from_posts(posts));
                                if st.current_channel.is_none() {
                                    st.current_channel = Some(channel_id);
                                }
                            }
                            ui.refresh_messages(cx);
                            if let Some(anchor) = anchor {
                                ui.chat.restore_anchor(&anchor, cx);
                            }
                        },
                    );
                }
            },
        );
    }

    runtime::spawn(
        async move {
            let boot = Bootstrap::run(&client, None, None, 0).await?;
            // Calls discovery is optional: the plugin may not be installed.
            let calls = mattermost_calls::discover(&client)
                .await
                .inspect_err(|e| tracing::warn!(error = %e, "calls discovery failed"))
                .ok();
            let active = match &calls {
                Some(_) => mattermost_calls::config::all_channel_states(&client)
                    .await
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            Ok::<_, mattermost_api::Error>((boot, calls, active))
        },
        move |result, cx| {
            let (boot, calls, active) = match result {
                Ok(v) => v,
                Err(e) => {
                    // A refused token is spent; anything else (server down,
                    // no network) leaves it alone so the next launch can
                    // retry — the same split `restore_session` used to make
                    // itself before this call was the first thing to ask.
                    match (on_auth_failure, &e) {
                        (Some(fallback), e) if !matches!(e, mattermost_api::Error::Http(_)) => {
                            tracing::info!("stored session not usable: {e}");
                            runtime::spawn(crate::session::clear_async(), |_, _| {});
                            crate::cache::clear();
                            fallback(cx);
                        }
                        _ => ui.toast(&format!("Could not load your account: {e}"), cx),
                    }
                    return;
                }
            };

            let ws_url;
            let token;
            let initial_channel;
            {
                let mut st = ui.state.borrow_mut();
                st.config = boot.config;
                st.crt_enabled = boot.crt_enabled;
                st.me = boot.me;
                st.teams = boot.teams;
                st.current_team = boot.initial_team.as_ref().map(|t| t.id.clone());
                for c in boot.channels {
                    st.channels.insert(c.id.clone(), c);
                }
                for m in boot.channel_members {
                    st.memberships.insert(m.channel_id.clone(), m);
                }
                st.categories = boot.categories;
                // Saved posts are preferences, and the startup sequence has
                // already fetched those — asking again would be a second
                // request for something we are holding.
                st.preferences = boot.preferences.clone();
                st.saved_posts = boot
                    .preferences
                    .iter()
                    .filter(|p| p.category == "flagged_post" && p.value == "true")
                    .map(|p| p.name.clone())
                    .collect();
                st.users_fetched_at = now_ms();
                st.calls = calls;
                st.active_calls = active
                    .into_iter()
                    .filter_map(|c| {
                        let call = c.call?;
                        Some((c.channel_id, participants(&call)))
                    })
                    .collect();

                ws_url = st.client.websocket_url();
                token = st.client.token().unwrap_or_default();
                initial_channel = boot.initial_channel.map(|c| c.id);
            }

            ui.refresh_all(cx);
            ui.load_inbox(cx);
            ui.load_drafts(cx);
            ui.load_bots(cx);
            ui.load_custom_emoji(cx);
            ui.load_team_unreads(cx);
            ui.preload_unread(cx);

            if let Some(id) = initial_channel {
                // The cache may already have put us in this channel. That was
                // a picture of it; arriving is what fetches, marks it read
                // and tells the server, so make sure arriving still happens.
                {
                    let mut st = ui.state.borrow_mut();
                    if st.current_channel.as_deref() == Some(id.as_str()) {
                        st.current_channel = None;
                    }
                }
                ui.dispatch(Action::SelectChannel(id), cx);
            }

            // --- live updates
            // Inside the runtime guard: `connect` spawns its own task, and
            // calling that from the main thread panics with "no reactor running".
            let ws = runtime::with_runtime(|| WebSocket::connect(ws_url, token));
            ui.state.borrow_mut().ws = Some(ws.clone());

            let subscription = ws.subscribe();
            runtime::spawn_stream(
                move |tx| async move {
                    let mut rx = subscription;
                    loop {
                        match rx.recv().await {
                            Ok(update) => {
                                if tx.send(update).await.is_err() {
                                    break; // the window went away
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(_) => break,
                        }
                    }
                },
                {
                    let ui = ui.clone();
                    move |update: WsUpdate, cx| match update {
                        WsUpdate::Event(event) => ui.apply_event(event, cx),
                        WsUpdate::MissedMessages => {
                            // The server could not replay its buffer, so
                            // anything could have changed while we were away.
                            ui.chat.set_connection_problem(None, cx);
                            resync(&ui, cx);
                        }
                        WsUpdate::Connected { connection_id, .. } => {
                            // Writes carry this from now on, so the server
                            // leaves us out of their echo.
                            ui.state.borrow().client.set_connection_id(connection_id);
                            ui.chat.set_connection_problem(None, cx);
                        }
                        // Losing the socket is not an event that scrolls past:
                        // it stays true until it stops being true, so it is a
                        // banner, and it says whether it is being worked on.
                        WsUpdate::Disconnected { will_retry, reason } => {
                            ui.chat.set_connection_problem(Some(&if will_retry {
                                "Reconnecting…".to_string()
                            } else {
                                format!("Disconnected: {reason}")
                            }), cx);
                        }
                        _ => {}
                    }
                },
            );
        },
    );
}

/// The gap-fill after a failed websocket resume.
fn resync(ui: &Rc<Ui>, _cx: &mut App) {
    let (client, team_id, crt, channel_id, since) = {
        let st = ui.state.borrow();
        let Some(team) = st.current_team.clone() else {
            return;
        };
        let channel = st.current_channel.clone();
        let since = channel
            .as_ref()
            .and_then(|id| st.feeds.get(id))
            .map(|f| f.last_fetched_at)
            .unwrap_or(0);
        (st.client.clone(), team, st.crt_enabled, channel, since)
    };

    let ui = ui.clone();
    runtime::spawn(
        async move {
            let channels = client.my_channels(&team_id, false, 0).await?;
            let members = client.my_channel_members(&team_id).await?;
            let missed = match (&channel_id, since) {
                (Some(id), since) if since > 0 => Some(client.posts_since(id, since, crt).await?),
                _ => None,
            };
            Ok::<_, mattermost_api::Error>((channels, members, channel_id, missed))
        },
        move |result, cx| {
            let Ok((channels, members, channel_id, missed)) = result else {
                return;
            };
            {
                let mut st = ui.state.borrow_mut();
                for c in channels {
                    st.channels.insert(c.id.clone(), c);
                }
                for m in members {
                    st.memberships.insert(m.channel_id.clone(), m);
                }
                if let (Some(id), Some(list)) = (channel_id.clone(), missed) {
                    let posts: Vec<Post> = list.chronological().into_iter().cloned().collect();
                    for post in posts {
                        if post.is_deleted() {
                            if let Some(feed) = st.feeds.get_mut(&id) {
                                feed.remove(&post.id);
                            }
                        } else {
                            // `?since=` reports edits and deletes as well as
                            // new posts, so upsert rather than append.
                            st.apply_post(post);
                        }
                    }
                }
            }
            // Names, pictures and positions change while we are away, and
            // nothing else tells us: status_change carries presence only.
            {
                let (client, ids, since) = {
                    let st = ui.state.borrow();
                    let ids: Vec<String> = st.users.keys().cloned().collect();
                    (st.client.clone(), ids, st.users_fetched_at)
                };
                if !ids.is_empty() && since > 0 {
                    let ui = ui.clone();
                    runtime::spawn(
                        async move { client.users_updated_since(&ids, since).await },
                        move |result, cx| {
                            let Ok(users) = result else { return };
                            if users.is_empty() {
                                return;
                            }
                            {
                                let mut st = ui.state.borrow_mut();
                                for user in users {
                                    // A new picture makes the cached texture
                                    // stale, exactly as user_updated does.
                                    let changed = st.users.get(&user.id).is_none_or(|old| {
                                        old.last_picture_update != user.last_picture_update
                                    });
                                    if changed {
                                        ui.avatars.forget(&user.id, cx);
                                    }
                                    st.users.insert(user.id.clone(), user);
                                }
                                st.users_fetched_at = now_ms();
                            }
                            ui.refresh_all(cx);
                        },
                    );
                }
            }

            ui.refresh_all(cx);
            ui.load_inbox(cx);
            // `?since=` is capped by the server, so a long absence can leave a
            // hole between what it returned and now. Scrolling up finds older
            // messages and nothing finds the ones in the middle, so ask for
            // whatever came after the newest post we hold.
            if let Some(id) = channel_id {
                ui.load_newer(id, cx);
            }
        },
    );
}