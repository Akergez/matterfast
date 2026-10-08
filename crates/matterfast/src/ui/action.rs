use super::autocomplete;
use super::call_dock::HostAction;
use super::message::PostAction;
use super::sidebar::RowAction;

/// What a pane asks the session to do.
pub enum Action {
    SelectTeam(String),
    SelectChannel(String),
    /// Makes a conversation the current one without bringing it to the
    /// front: the one the window starts with, which nobody chose. On a phone
    /// the list stays the screen.
    LoadChannel(String),
    /// The bottom of a feed that stops short of the newest message was
    /// reached: the page after it.
    LoadNewer,
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
    /// Ask for the page of followed threads after the ones the inbox holds.
    OlderThreads,
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
    Row(String, RowAction),
    /// Raise or lower your hand in the call.
    ToggleHand,
    /// Do something to another participant, as the call's host.
    HostControl(String, HostAction),
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
