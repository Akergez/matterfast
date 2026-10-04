//! The limits and timings the session is tuned by.

use std::time::Duration;

/// How long the server is left out of a burst of typing. Short enough that
/// the extra names arrive while the eye is still on the list, long enough
/// that a whole word costs one request.
pub(crate) const MENTION_DEBOUNCE: Duration = Duration::from_millis(180);

/// The websocket prefix of <https://github.com/Toxblh/mattermost-reactions-notify-plugin>,
/// which notifies you about reactions to your own posts.
pub(crate) const REACTION_NOTIFY_PREFIX: &str = "custom_ru.toxblh.reactions-notify_";

/// How many rows the quick switcher offers per source. Long enough to find
/// what you meant, short enough to stay a keyboard shortcut.
pub(crate) const QUICK_SWITCH_ROWS: usize = 10;

/// How many completion candidates to offer. More than this and the popover is
/// a list to read rather than a shortcut.
pub(crate) const COMPLETIONS: usize = 8;

/// How many rows of a mention list are kept for groups when both people and
/// groups match: people are the common case, but a group that is never
/// offered cannot be found at all.
pub(crate) const GROUP_COMPLETIONS: usize = 3;

/// How many messages to pull when a channel is first opened.
pub(crate) const INITIAL_POSTS: u32 = 60;

/// How many mentions and threads to keep in the inbox.
pub(crate) const INBOX_PAGE: u32 = 25;

/// How many hits a page of a search is.
pub(crate) const SEARCH_PAGE: u32 = 60;

/// How long a landed picture waits for the rest of its flock before the
/// window is redrawn. Long enough to catch a screenful of faces arriving
/// together, short enough that nobody watches an avatar appear.
pub(crate) const AVATAR_REDRAW_WAIT: Duration = Duration::from_millis(120);

/// How long an attached file is kept on disk once fetched. Upload ids are
/// immutable; the global size limit is the normal eviction mechanism, while
/// this prevents abandoned entries living forever.
pub(crate) const FILE_CACHE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Whether opening a conversation or a thread puts the cursor in its
/// composer. Not on a phone, where that brings the keyboard up over what was
/// opened to be read.
pub(crate) const FOCUS_ON_ARRIVAL: bool = !cfg!(target_os = "android");
