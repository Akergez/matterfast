use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Instant;

use mattermost_api::models::*;
use mattermost_api::{Client, WebSocket};
use mattermost_calls::Discovery;

use super::active_call::ActiveCall;
use super::channel_feed::ChannelFeed;
use super::reply_ledger::ReplyLedger;

/// Everything the UI reads.
pub struct AppState {
    pub client: Client,
    pub ws: Option<WebSocket>,
    pub me: User,
    pub crt_enabled: bool,
    pub config: ClientConfig,

    pub teams: Vec<Team>,
    pub current_team: Option<String>,

    pub channels: HashMap<String, Channel>,
    /// Our own membership per channel — the hot, frequently rewritten half.
    pub memberships: HashMap<String, ChannelMember>,
    pub categories: OrderedSidebarCategories,
    pub current_channel: Option<String>,
    /// Per-channel stable message at roughly 30% of the viewport. Absence
    /// means the reader left that channel at its live bottom.
    pub scroll_anchors: HashMap<String, String>,

    pub users: HashMap<String, User>,
    pub statuses: HashMap<String, Presence>,
    pub feeds: HashMap<String, ChannelFeed>,
    /// Thread contents, keyed by **root post id**. A thread is identified by
    /// its root everywhere in the API, so replies are looked up through
    /// `Post::thread_root`, never through their own id.
    pub threads: HashMap<String, ChannelFeed>,
    /// The collapsed-reply-threads inbox for the current team.
    pub thread_inbox: Vec<UserThread>,
    /// How many threads the reader follows in all, as the server counts
    /// them: `thread_inbox` holds the newest of those, a page at a time.
    pub thread_inbox_total: i64,
    pub replies: ReplyLedger,
    /// Recent posts that name us, newest first.
    pub mentions: Vec<Post>,

    /// Calls discovery, when the plugin is installed and reachable.
    pub calls: Option<Discovery>,
    /// Channels with a call in progress, mapped to who is in it. User ids, in
    /// the order they joined — the sidebar draws the first few faces, and the
    /// length is the participant count.
    pub active_calls: HashMap<String, Vec<String>>,
    /// The call we are in, if any.
    pub call: Option<ActiveCall>,

    /// Unsent text, keyed by channel id. Mirrors what the server has, so the
    /// same half-written message is waiting on every device.
    pub drafts: HashMap<String, String>,
    /// When each person was last seen, from the status route. Only meaningful
    /// for people who are not online now.
    pub last_seen: HashMap<String, Millis>,
    /// The account's preferences, as the server holds them. Kept whole rather
    /// than picked apart: several features read one key each, and the list is
    /// small.
    pub preferences: Vec<Preference>,
    /// When the user map was last refreshed wholesale, so a resync can ask
    /// only for what changed since.
    pub users_fetched_at: Millis,
    /// Channels we have raised an incoming-call notification for, so it can
    /// be taken back down when the call ends or is answered elsewhere. The
    /// dismissal event names a *call*, not a channel, and we hold no mapping
    /// between the two — so this is the mapping.
    pub ringing: Vec<String>,
    /// Unread counts for teams other than the current one, so the switcher
    /// can say where something is waiting. Keyed by team id.
    pub team_unreads: HashMap<String, (i64, i64)>,
    /// The LLM bots this server offers, empty when the Agents plugin is not
    /// installed — which is the same as having nothing to offer, so it needs
    /// no separate flag.
    pub bots: Vec<crate::agents::Bot>,
    /// The names of the server's own emoji, in order. What decides whether
    /// `:word:` in a message is a picture, and what the pickers offer.
    pub custom_emoji: BTreeSet<String>,
    /// The groups that can be mentioned, as last fetched: what decides
    /// whether `@word` in a message is a group, and what the composer offers
    /// beside people. Empty on a server without them.
    pub groups: Vec<Group>,
    /// When `groups` was last asked for. They change, and not every change
    /// is announced to everyone, so the list is only trusted for a while.
    pub groups_fetched_at: Millis,
    /// Handles mentioned in messages that have been drawn but that name
    /// nobody held here. Written while preparing a message, which only has
    /// this state to hand; the session drains it and asks the server.
    pub unknown_handles: RefCell<BTreeSet<String>>,
    /// Files uploaded and waiting to be attached to the next message: the
    /// server's file id and the name to show on the chip.
    pub pending_files: Vec<(String, String)>,
    /// The last search's hits, newest first, and whether one is in flight.
    pub search_results: Vec<Post>,
    pub searching: bool,
    /// What the hits are for, how many pages of them are here, and whether
    /// the last page came back full — which is all the server says about
    /// there being more.
    pub search_terms: String,
    pub search_pages: u32,
    pub search_more: bool,
    /// Posts you saved. Mattermost has no "saved" flag on a post — it is a
    /// preference in the `flagged_post` category, one row per post id.
    pub saved_posts: HashSet<String>,
    /// Unread reaction notices, from the reactions-notify plugin. Zero when
    /// the plugin is not installed, which is indistinguishable from "nothing
    /// new" and needs no special case.
    pub reaction_unread: i64,
    /// Unsent replies, keyed by thread root. Separate from `drafts` because
    /// the server keys them the same way: one per (channel, root) pair.
    pub thread_drafts: HashMap<String, String>,
    /// False once the server has told us drafts are turned off, after which
    /// they are kept in this map and nowhere else.
    pub drafts_synced: bool,

    /// Who is typing where: channel id → user id → when we heard about it.
    /// The server sends no "stopped typing", so entries are aged out instead.
    pub typing: HashMap<String, HashMap<String, Instant>>,
}
